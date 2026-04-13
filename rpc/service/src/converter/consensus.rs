use async_trait::async_trait;
use spora_addresses::Address;
use spora_consensus_core::{
    block::Block,
    blockstatus::BlockStatus,
    config::Config,
    header::Header,
    mass::{project_cell_tx_mass_with_calculator, project_verifiable_transaction_mass_with_calculator, MassCalculator},
    tx::{CellInput, CellTx, MutableTransaction, ResolvedCellTransaction, TransactionId, TransactionOutpoint, VerifiableTransaction},
    ChainPath,
};
use spora_consensus_notify::notification::{self as consensus_notify, Notification as ConsensusNotification};
use spora_consensusmanager::{ConsensusManager, ConsensusProxy};
use spora_math::Uint256;
use spora_mining::model::{owner_txs::OwnerTransactions, TransactionIdSet};
use spora_notify::converter::Converter;
use spora_rpc_core::{
    BlockAddedNotification, Notification, RpcAcceptedTransactionIds, RpcBlock, RpcBlockStatus, RpcBlockVerboseData, RpcHash,
    RpcMempoolEntry, RpcMempoolEntryByAddress, RpcResult, RpcTransaction, RpcTransactionInput, RpcTransactionOutput,
    RpcTransactionVerboseData,
};
use std::{collections::HashMap, fmt::Debug, sync::Arc};

/// Conversion of consensus_core to rpc_core structures
pub struct ConsensusConverter {
    consensus_manager: Arc<ConsensusManager>,
    config: Arc<Config>,
}

impl ConsensusConverter {
    pub fn new(consensus_manager: Arc<ConsensusManager>, config: Arc<Config>) -> Self {
        Self { consensus_manager, config }
    }

    fn mass_calculator(&self) -> MassCalculator {
        MassCalculator::new_with_consensus_params(&self.config.params)
    }

    /// Returns the proof-of-work difficulty as a multiple of the minimum difficulty using
    /// the passed bits field from the header of a block.
    pub fn get_difficulty_ratio(&self, bits: u32) -> f64 {
        // The minimum difficulty is the max possible proof-of-work limit bits
        // converted back to a number. Note this is not the same as the proof of
        // work limit directly because the block difficulty is encoded in a block
        // with the compact form which loses precision.
        let target = Uint256::from_compact_target_bits(bits);
        self.config.max_difficulty_target_f64 / target.as_f64()
    }

    fn get_cell_transaction_input(&self, input: &CellInput, witness: Vec<u8>) -> RpcTransactionInput {
        let out_point = TransactionOutpoint::new(input.previous_output.tx_hash, input.previous_output.index);
        RpcTransactionInput::from_cell_ref(&CellInput::new(out_point, input.since), witness)
    }

    fn build_rpc_transaction(
        &self,
        transaction: &CellTx,
        header: Option<&Header>,
        include_verbose_data: bool,
        projected_mass: Option<(u64, u64)>,
    ) -> RpcTransaction {
        let txid: TransactionId = transaction.id().into();
        let fallback_projection = project_cell_tx_mass_with_calculator(&self.mass_calculator(), transaction, None);
        let (selection_mass, effective_compute_mass) =
            projected_mass.unwrap_or((fallback_projection.selection_mass, fallback_projection.effective_compute_mass));
        RpcTransaction {
            version: transaction.version,
            inputs: transaction
                .inputs
                .iter()
                .enumerate()
                .map(|(index, input)| {
                    let witness = transaction.witnesses.get(index).cloned().unwrap_or_default();
                    self.get_cell_transaction_input(input, witness)
                })
                .collect(),
            outputs: transaction
                .outputs
                .iter()
                .enumerate()
                .map(|(index, output)| {
                    let output_data = transaction.outputs_data.get(index).map(Vec::as_slice).unwrap_or(&[]);
                    RpcTransactionOutput::from_cell_output(output, output_data)
                })
                .collect(),
            payload: transaction.payload().map(ToOwned::to_owned).unwrap_or_default(),
            mass: selection_mass,
            verbose_data: include_verbose_data.then(|| RpcTransactionVerboseData {
                transaction_id: txid,
                hash: txid,
                compute_mass: effective_compute_mass,
                block_hash: header.map_or_else(RpcHash::default, |x| x.hash),
                block_time: header.map_or(0, |x| x.timestamp),
            }),
        }
    }

    pub fn get_cell_transaction(
        &self,
        _consensus: &ConsensusProxy,
        transaction: &CellTx,
        header: Option<&Header>,
        include_verbose_data: bool,
    ) -> RpcTransaction {
        self.build_rpc_transaction(transaction, header, include_verbose_data, None)
    }

    pub fn get_verifiable_transaction(
        &self,
        transaction: &(impl VerifiableTransaction + ?Sized),
        header: Option<&Header>,
        include_verbose_data: bool,
    ) -> RpcTransaction {
        let projected_mass = project_verifiable_transaction_mass_with_calculator(&self.mass_calculator(), transaction, None);
        self.build_rpc_transaction(
            transaction.tx(),
            header,
            include_verbose_data,
            Some((projected_mass.selection_mass, projected_mass.effective_compute_mass)),
        )
    }

    pub fn get_resolved_cell_transaction(
        &self,
        transaction: &ResolvedCellTransaction,
        header: Option<&Header>,
        include_verbose_data: bool,
    ) -> RpcTransaction {
        let signable = transaction.clone().into_signable_transaction();
        let verifiable = signable.as_verifiable();
        self.get_verifiable_transaction(&verifiable, header, include_verbose_data)
    }

    fn get_mempool_transaction(&self, transaction: &MutableTransaction) -> RpcTransaction {
        self.build_rpc_transaction(
            transaction.tx.as_ref(),
            None,
            true,
            transaction.selection_mass().zip(transaction.effective_compute_mass()),
        )
    }

    /// Converts a consensus [`Block`] into an [`RpcBlock`], optionally including transaction verbose data.
    ///
    /// Mirrors the previous Go implementation's block verbose-data population behavior.
    pub async fn get_block(
        &self,
        consensus: &ConsensusProxy,
        block: &Block,
        include_transactions: bool,
        include_transaction_verbose_data: bool,
    ) -> RpcResult<RpcBlock> {
        let hash = block.hash();
        let ghostdag_data = consensus.async_get_ghostdag_data(hash).await?;
        let block_status = consensus.async_get_block_status(hash).await.unwrap();
        let children = consensus.async_get_block_children(hash).await.unwrap_or_default();
        let is_chain_block = consensus.async_is_chain_block(hash).await?;
        let verbose_data = Some(RpcBlockVerboseData {
            hash,
            difficulty: self.get_difficulty_ratio(block.header.bits),
            selected_parent_hash: ghostdag_data.selected_parent,
            transaction_ids: block.transactions.iter().map(|x| x.id().into()).collect(), // CellTx::id() -> Hash
            is_header_only: block_status.is_header_only(),
            blue_score: ghostdag_data.blue_score,
            children_hashes: children,
            merge_set_blues_hashes: ghostdag_data.mergeset_blues,
            merge_set_reds_hashes: ghostdag_data.mergeset_reds,
            is_chain_block,
        });

        let transactions = if include_transactions {
            let mut transactions = Vec::with_capacity(block.transactions.len());
            for transaction in block.transactions.iter() {
                let rpc_tx = if transaction.is_coinbase() {
                    self.get_cell_transaction(consensus, transaction, Some(&block.header), include_transaction_verbose_data)
                } else {
                    let resolved = consensus
                        .async_get_resolved_cell_transaction_in_accepting_block(transaction.id().into(), hash)
                        .await
                        .map_err(spora_rpc_core::RpcError::General)?;
                    self.get_resolved_cell_transaction(&resolved, Some(&block.header), include_transaction_verbose_data)
                };
                transactions.push(rpc_tx);
            }
            transactions
        } else {
            vec![]
        };

        Ok(RpcBlock { header: block.header.as_ref().into(), transactions, verbose_data })
    }

    pub fn get_block_status(&self, status: &BlockStatus) -> RpcBlockStatus {
        RpcBlockStatus { status: *status as u32 }
    }

    pub fn get_mempool_entry(&self, consensus: &ConsensusProxy, transaction: &MutableTransaction) -> RpcMempoolEntry {
        let is_orphan = !transaction.is_fully_populated();
        let rpc_transaction = if is_orphan {
            self.get_cell_transaction(consensus, transaction.tx.as_ref(), None, true)
        } else {
            self.get_mempool_transaction(transaction)
        };
        RpcMempoolEntry::new(transaction.calculated_fee.unwrap_or_default(), rpc_transaction, is_orphan)
    }

    pub fn get_mempool_entries_by_address(
        &self,
        consensus: &ConsensusProxy,
        address: Address,
        owner_transactions: &OwnerTransactions,
        transactions: &HashMap<TransactionId, MutableTransaction>,
    ) -> RpcMempoolEntryByAddress {
        let sending = self.get_owner_entries(consensus, &owner_transactions.sending_txs, transactions);
        let receiving = self.get_owner_entries(consensus, &owner_transactions.receiving_txs, transactions);
        RpcMempoolEntryByAddress::new(address, sending, receiving)
    }

    pub fn get_owner_entries(
        &self,
        consensus: &ConsensusProxy,
        transaction_ids: &TransactionIdSet,
        transactions: &HashMap<TransactionId, MutableTransaction>,
    ) -> Vec<RpcMempoolEntry> {
        transaction_ids.iter().map(|x| self.get_mempool_entry(consensus, transactions.get(x).expect("transaction exists"))).collect()
    }

    pub async fn get_virtual_chain_accepted_transaction_ids(
        &self,
        consensus: &ConsensusProxy,
        chain_path: &ChainPath,
        merged_blocks_limit: Option<usize>,
    ) -> RpcResult<Vec<RpcAcceptedTransactionIds>> {
        let acceptance_data = consensus.async_get_blocks_acceptance_data(chain_path.added.clone(), merged_blocks_limit).await.unwrap();
        Ok(chain_path
            .added
            .iter()
            .zip(acceptance_data.iter())
            .map(|(hash, block_data)| RpcAcceptedTransactionIds {
                accepting_block_hash: hash.to_owned(),
                accepted_transaction_ids: block_data
                    .iter()
                    .flat_map(|x| x.accepted_transactions.iter().map(|tx| tx.transaction_id))
                    .collect(),
            })
            .collect())
    }
}

#[async_trait]
impl Converter for ConsensusConverter {
    type Incoming = ConsensusNotification;
    type Outgoing = Notification;

    async fn convert(&self, incoming: ConsensusNotification) -> Notification {
        match incoming {
            consensus_notify::Notification::BlockAdded(msg) => {
                let session = self.consensus_manager.consensus().unguarded_session();
                // If get_block fails, rely on the infallible From implementation which will lack verbose data
                let block = Arc::new(self.get_block(&session, &msg.block, true, true).await.unwrap_or_else(|_| (&msg.block).into()));
                Notification::BlockAdded(BlockAddedNotification { block })
            }
            _ => (&incoming).into(),
        }
    }
}

impl Debug for ConsensusConverter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConsensusConverter").field("consensus_manager", &"").field("config", &self.config).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spora_consensus::consensus::test_consensus::TestConsensus;
    use spora_consensus_core::{
        cell_metadata::CellMetadata,
        mass::ProjectedTransactionMass,
        tx::{OutPoint, Script},
    };

    struct TestConverter {
        converter: ConsensusConverter,
        _consensus: TestConsensus,
    }

    fn build_converter() -> TestConverter {
        let config = Arc::new(Config::new(spora_consensus::params::SIMNET_PARAMS));
        let consensus = TestConsensus::new(&config);
        let consensus_manager = Arc::new(ConsensusManager::from_consensus(consensus.consensus_clone()));
        TestConverter { converter: ConsensusConverter::new(consensus_manager, config), _consensus: consensus }
    }

    fn build_resolved_transaction() -> ResolvedCellTransaction {
        let lock = Script::new([0u8; 32], 0, vec![]);
        let prev_tx_id = [0x88; 32];
        let inputs = vec![
            CellInput::new(OutPoint::new(prev_tx_id, 0), 0),
            CellInput::new(OutPoint::new(prev_tx_id, 1), 0),
        ];
        let outputs = vec![
            spora_consensus_core::tx::CellOutput { lock: lock.clone(), type_: None, capacity: 50 },
            spora_consensus_core::tx::CellOutput { lock: lock.clone(), type_: None, capacity: 250 },
        ];
        let outputs_data = vec![vec![0; 15], vec![0; 15]];
        let tx = CellTx { version: 0, inputs, cell_deps: vec![], header_deps: vec![], outputs, outputs_data, witnesses: vec![] };
        let resolved_inputs = vec![
            CellMetadata {
                out_point: OutPoint::new(prev_tx_id, 0),
                capacity: 100,
                data_bytes: 0,
                lock_hash: lock.hash(),
                type_hash: None,
                data_hash: [0; 32],
                block_daa_score: 0,
                is_cellbase: false,
                block_hash: [0; 32].into(),
                lock_code_hash: Some(lock.code_hash),
                type_code_hash: None,
                lock_script: Some(lock.clone()),
                type_script: None,
                data: Some(vec![]),
            },
            CellMetadata {
                out_point: OutPoint::new(prev_tx_id, 1),
                capacity: 200,
                data_bytes: 0,
                lock_hash: lock.hash(),
                type_hash: None,
                data_hash: [0; 32],
                block_daa_score: 0,
                is_cellbase: false,
                block_hash: [0; 32].into(),
                lock_code_hash: Some(lock.code_hash),
                type_code_hash: None,
                lock_script: Some(lock),
                type_script: None,
                data: Some(vec![]),
            },
        ];
        ResolvedCellTransaction::new(tx, resolved_inputs)
    }

    fn project_resolved(converter: &ConsensusConverter, resolved: &ResolvedCellTransaction) -> ProjectedTransactionMass {
        let signable = resolved.clone().into_signable_transaction();
        let verifiable = signable.as_verifiable();
        project_verifiable_transaction_mass_with_calculator(&converter.mass_calculator(), &verifiable, None)
    }

    #[test]
    fn resolved_transactions_use_verifiable_selection_mass() {
        let test = build_converter();
        let resolved = build_resolved_transaction();

        let projected = project_resolved(&test.converter, &resolved);
        let fallback = project_cell_tx_mass_with_calculator(&test.converter.mass_calculator(), &resolved.tx, None);
        assert!(projected.selection_mass > fallback.selection_mass, "test fixture must exercise storage mass");

        let rpc_tx = test.converter.get_resolved_cell_transaction(&resolved, None, true);

        assert_eq!(rpc_tx.mass, projected.selection_mass);
        assert_eq!(rpc_tx.verbose_data.as_ref().map(|data| data.compute_mass), Some(projected.effective_compute_mass));
    }
}
