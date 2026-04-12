use super::coinbase_mock::CoinbaseManagerMock;
use crate::cell_conversion::{cell_output_to_placeholder_entry, legacy_tx_to_cell_tx};
use spora_consensus_core::{
    api::{
        args::{TransactionValidationArgs, TransactionValidationBatchArgs},
        ConsensusApi,
    },
    block::{BlockTemplate, MutableBlock, TemplateBuildMode, TemplateTransactionSelector, VirtualStateApproxId},
    cell_metadata::CellMetadata,
    coinbase::MinerData,
    constants::BLOCK_VERSION,
    errors::{
        block::RuleError,
        coinbase::CoinbaseResult,
        consensus::{ConsensusError, ConsensusResult},
        tx::{TxResult, TxRuleError},
    },
    header::Header,
    mass::{cell_tx_estimated_serialized_size, ContextualMasses, NonContextualMasses},
    merkle::{calc_hash_merkle_root, calc_hash_merkle_root_cell},
    tx::{
        cell_tx_from_legacy_transaction, legacy_compat_transaction_from_cell_tx, CellEntry, CellTx, MutableTransaction,
        OutPointCompat, ScriptRef, Transaction, TransactionId, TransactionOutpoint,
    },
};
use spora_core::time::unix_now;
use spora_hashes::{Hash, ZERO_HASH};

use parking_lot::RwLock;
use std::{collections::HashMap, sync::Arc};

type CellCollection = HashMap<TransactionOutpoint, CellEntry>;

pub(crate) struct ConsensusMock {
    legacy_transactions: RwLock<HashMap<TransactionId, Transaction>>,
    transactions: RwLock<HashMap<TransactionId, Arc<CellTx>>>,
    cell_transactions: RwLock<HashMap<TransactionId, Arc<CellTx>>>,
    legacy_ids_by_cell_id: RwLock<HashMap<TransactionId, TransactionId>>,
    statuses: RwLock<HashMap<TransactionId, TxResult<()>>>,
    cells: RwLock<CellCollection>,
}

impl ConsensusMock {
    pub(crate) fn new() -> Self {
        Self {
            legacy_transactions: RwLock::new(HashMap::default()),
            transactions: RwLock::new(HashMap::default()),
            cell_transactions: RwLock::new(HashMap::default()),
            legacy_ids_by_cell_id: RwLock::new(HashMap::default()),
            statuses: RwLock::new(HashMap::default()),
            cells: RwLock::new(HashMap::default()),
        }
    }

    pub(crate) fn set_status(&self, transaction_id: TransactionId, status: TxResult<()>) {
        self.statuses.write().insert(transaction_id, status);
    }

    pub(crate) fn add_transaction(&self, transaction: Transaction, block_daa_score: u64) {
        let legacy_id = transaction.id();
        let cell_tx = Arc::new(cell_tx_from_legacy_transaction(&transaction));
        let mut legacy_transactions = self.legacy_transactions.write();
        let mut transactions = self.transactions.write();
        let mut cell_transactions = self.cell_transactions.write();
        let mut legacy_ids_by_cell_id = self.legacy_ids_by_cell_id.write();
        let mut cells = self.cells.write();

        // Remove the spent cells
        cell_tx.inputs.iter().for_each(|x| {
            cells.remove(&x.out_point);
            if let Some(parent_cell_id) = transactions.get(&x.out_point.transaction_id()).map(|tx| TransactionId::from_bytes(tx.id())) {
                cells.remove(&TransactionOutpoint::new(parent_cell_id.as_bytes(), x.out_point.index));
            }
            if let Some(parent_legacy_id) = legacy_ids_by_cell_id.get(&x.out_point.transaction_id()).copied() {
                cells.remove(&TransactionOutpoint::new(parent_legacy_id.as_bytes(), x.out_point.index));
            }
        });
        // Create the new cells
        cell_tx.outputs.iter().zip(cell_tx.outputs_data.iter()).enumerate().for_each(|(i, (output, data))| {
            let entry = cell_output_to_placeholder_entry(output, data, block_daa_score, cell_tx.is_coinbase());
            cells.insert(TransactionOutpoint::new(legacy_id.as_bytes(), i as u32), entry.clone());
            cells.insert(TransactionOutpoint::new(cell_tx.id(), i as u32), entry);
        });
        // Register the transaction
        legacy_transactions.insert(legacy_id, transaction);
        transactions.insert(legacy_id, cell_tx.clone());
        cell_transactions.insert(cell_tx.id().into(), cell_tx.clone());
        legacy_ids_by_cell_id.insert(cell_tx.id().into(), legacy_id);
    }

    pub(crate) fn can_finance_transaction(&self, transaction: &MutableTransaction) -> bool {
        let cells = self.cells.read();
        for outpoint in transaction.missing_outpoints() {
            if !cells.contains_key(&outpoint) {
                return false;
            }
        }
        true
    }
}

impl ConsensusApi for ConsensusMock {
    fn build_block_template(
        &self,
        miner_data: MinerData,
        mut tx_selector: Box<dyn TemplateTransactionSelector>,
        _build_mode: TemplateBuildMode,
    ) -> Result<BlockTemplate, RuleError> {
        let mut txs = tx_selector.select_transactions();
        let coinbase_manager = CoinbaseManagerMock::new();
        let coinbase = coinbase_manager.expected_coinbase_transaction(miner_data.clone());
        txs.insert(0, legacy_tx_to_cell_tx(&coinbase.tx).expect("mock coinbase must be Cell-convertible"));
        let now = unix_now();
        let hash_merkle_root = calc_hash_merkle_root_cell(txs.iter(), false);
        let header = Header::new_finalized(
            BLOCK_VERSION,
            vec![],
            hash_merkle_root,
            ZERO_HASH,
            ZERO_HASH,
            ZERO_HASH, // cell_root
            now,
            123456789u32,
            0,
            0,
            0.into(),
            0,
            ZERO_HASH,
        );
        let mutable_block = MutableBlock::new(header, txs);

        Ok(BlockTemplate::new(mutable_block, miner_data, coinbase.has_red_reward, now, 0, ZERO_HASH, vec![]))
    }

    fn validate_mempool_transaction(&self, mutable_tx: &mut MutableTransaction, _: &TransactionValidationArgs) -> TxResult<()> {
        // If a predefined status was registered to simulate an error, return it right away
        if let Some(status) = self.statuses.read().get(&mutable_tx.id()) {
            if status.is_err() {
                return status.clone();
            }
        }
        let cells = self.cells.read();
        let mut has_missing_outpoints = false;
        for i in 0..mutable_tx.tx.inputs.len() {
            // Keep existing resolved inputs.
            if mutable_tx.entries[i].is_some() || mutable_tx.resolved_cell_metadata[i].is_some() {
                continue;
            }
            // Try add missing entries from the mock cell set.
            if let Some(entry) = cells.get(&mutable_tx.tx.inputs[i].out_point) {
                let mut metadata = CellMetadata::from(entry);
                metadata.out_point = mutable_tx.tx.inputs[i].out_point;
                metadata.lock_script = Some(ScriptRef::new(entry.lock_hash, 0, vec![]));
                metadata.type_script = entry.type_hash.map(|hash| ScriptRef::new(hash, 0, vec![]));
                mutable_tx.resolved_cell_metadata[i] = Some(metadata);
                mutable_tx.entries[i] = Some(entry.clone());
            } else {
                has_missing_outpoints = true;
            }
        }
        if has_missing_outpoints {
            return Err(TxRuleError::MissingTxOutpoints);
        }
        // At this point we know all inputs are resolved, so we can safely calculate the fee.
        let total_in: u64 = mutable_tx
            .tx
            .inputs
            .iter()
            .enumerate()
            .map(|(i, _)| {
                mutable_tx
                    .resolved_cell_metadata(i)
                    .map(|metadata| metadata.capacity)
                    .or_else(|| mutable_tx.entries[i].as_ref().map(|entry| entry.capacity()))
                    .expect("expected resolved input in consensus mock")
            })
            .sum();
        let total_out: u64 = mutable_tx.tx.outputs.iter().map(|x| x.capacity).sum();

        if mutable_tx.calculated_fee.is_none() {
            let calculated_fee = total_in - total_out;
            mutable_tx.calculated_fee = Some(calculated_fee);
        }
        if mutable_tx.calculated_non_contextual_masses.is_none() {
            mutable_tx.calculated_non_contextual_masses = Some(self.calculate_transaction_non_contextual_masses(mutable_tx.tx.as_ref()));
        }
        Ok(())
    }

    fn validate_mempool_transactions_in_parallel(
        &self,
        transactions: &mut [MutableTransaction],
        _: &TransactionValidationBatchArgs,
    ) -> Vec<TxResult<()>> {
        transactions.iter_mut().map(|x| self.validate_mempool_transaction(x, &Default::default())).collect()
    }

    fn populate_mempool_transactions_in_parallel(&self, transactions: &mut [MutableTransaction]) -> Vec<TxResult<()>> {
        transactions.iter_mut().map(|x| self.validate_mempool_transaction(x, &Default::default())).collect()
    }

    fn calculate_transaction_non_contextual_masses(&self, transaction: &CellTx) -> NonContextualMasses {
        let mass = if transaction.is_coinbase() { 0 } else { cell_tx_estimated_serialized_size(transaction) };
        NonContextualMasses::new(mass, mass)
    }

    fn calculate_transaction_contextual_masses(&self, _transaction: &MutableTransaction) -> Option<ContextualMasses> {
        Some(ContextualMasses::new(0))
    }

    fn get_virtual_daa_score(&self) -> u64 {
        0
    }

    fn get_virtual_state_approx_id(&self) -> VirtualStateApproxId {
        VirtualStateApproxId::new(self.get_virtual_daa_score(), 0.into(), ZERO_HASH)
    }

    fn modify_coinbase_payload(&self, payload: Vec<u8>, miner_data: &MinerData) -> CoinbaseResult<Vec<u8>> {
        let coinbase_manager = CoinbaseManagerMock::new();
        Ok(coinbase_manager.modify_coinbase_payload(payload, miner_data))
    }

    fn calc_transaction_hash_merkle_root(&self, txs: &[Transaction], _pov_daa_score: u64) -> Hash {
        calc_hash_merkle_root(txs.iter(), false)
    }

    fn get_transaction(&self, hash: Hash) -> ConsensusResult<Transaction> {
        if let Some(transaction) = self.legacy_transactions.read().get(&hash) {
            return Ok(transaction.clone());
        }
        if let Some(legacy_id) = self.legacy_ids_by_cell_id.read().get(&hash).copied() {
            if let Some(transaction) = self.legacy_transactions.read().get(&legacy_id) {
                return Ok(transaction.clone());
            }
        }
        if let Some(transaction) = self.transactions.read().get(&hash) {
            return Ok(legacy_compat_transaction_from_cell_tx(transaction.as_ref()));
        }
        if let Some(transaction) = self.cell_transactions.read().get(&hash) {
            return Ok(legacy_compat_transaction_from_cell_tx(transaction.as_ref()));
        }
        Err(ConsensusError::TransactionNotFound(hash.to_string()))
    }
}
