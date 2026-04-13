use super::BlockBodyProcessor;
use crate::{
    consensus::cell_provider::{ConsensusCellProvider, OverlayCellProvider},
    errors::{BlockProcessResult, RuleError},
    model::stores::{
        block_transactions::BlockTransactionsStoreReader, ghostdag::GhostdagStoreReader, headers::HeaderStoreReader,
        statuses::StatusesStoreReader,
    },
    processes::{cell_validator::CellValidationError, CellConsensusParams, CellValidator, DagCellProvider},
};
use spora_consensus_core::{
    block::Block,
    cell_metadata::CellMetadata,
    errors::tx::TxRuleError,
    mass::{ContextualMasses, Mass, NonContextualMasses},
    tx::{MutableTransaction, TransactionOutpoint},
};
use spora_database::prelude::StoreResultExtensions;
#[cfg(feature = "vm")]
use spora_exec::vm::VmLimits;
use spora_exec::{DepType, OutPoint};
use spora_hashes::Hash;
use std::{collections::HashSet, sync::Arc};

use rayon::prelude::*;

type BodyConsensusCellProvider = ConsensusCellProvider<
    crate::model::stores::ghostdag::DbGhostdagStore,
    crate::model::stores::reachability::DbReachabilityStore,
    crate::model::stores::headers::DbHeadersStore,
    crate::model::stores::cell_diffs::DbCellDiffsStore,
    crate::model::stores::cell_roots::DbCellRootsStore,
    crate::model::stores::block_transactions::DbBlockTransactionsStore,
    crate::model::stores::statuses::DbStatusesStore,
>;
type BodyValidationOverlayProvider<B> = OverlayCellProvider<B>;

impl BlockBodyProcessor {
    pub fn validate_body_in_context(self: &Arc<Self>, block: &Block) -> BlockProcessResult<Mass> {
        self.check_parent_bodies_exist(block)?;
        self.check_coinbase_outputs_limit(block)?;
        self.check_coinbase_blue_score_and_subsidy(block)?;
        self.check_block_transactions_in_context(block)
    }

    fn check_block_transactions_in_context(self: &Arc<Self>, block: &Block) -> BlockProcessResult<Mass> {
        if block.transactions.iter().all(|tx| tx.is_coinbase()) {
            return Ok((NonContextualMasses::new(0, 0), ContextualMasses::new(0)));
        }

        let provider = Arc::new(self.build_body_validation_provider(block)?);
        let validator = CellValidator::new(
            Arc::new(CellConsensusParams { cellbase_maturity: self.coinbase_maturity, ..CellConsensusParams::default() }),
            provider.clone(),
        );

        // Collect non-coinbase transactions (preserving block-original order by index).
        let non_coinbase_txs: Vec<(usize, &spora_exec::CellTx)> =
            block.transactions.iter().enumerate().filter(|(_, tx)| !tx.is_coinbase()).collect();

        // --- Parallel validation, deterministic convergence ---
        //
        // Each tx validation reads from the frozen Arc<OverlayCellProvider>;
        // no shared mutable state exists during this phase.
        //
        // Results are collected into a Vec whose indices correspond 1-to-1
        // with `non_coinbase_txs` (i.e. block-original order), so error
        // reporting and cycles/mass accumulation are fully deterministic.
        #[cfg(feature = "vm")]
        {
            let block_hash = block.hash();
            let daa_score = block.header.daa_score;
            let timestamp = block.header.timestamp;

            let results: Vec<(usize, Result<(NonContextualMasses, u64, u64), RuleError>)> = non_coinbase_txs
                .par_iter()
                .map(|&(idx, tx)| {
                    let non_contextual_masses = self.mass_calculator.calc_non_contextual_masses_cell(tx);
                    let res = validator
                        .validate_full_with_scripts_and_cycles(tx, block_hash, daa_score, timestamp)
                        .map_err(|e| self.map_cell_validation_error(tx, block_hash, daa_score, provider.as_ref(), e))
                        .and_then(|verified_cycles| {
                            let resolved_inputs = self
                                .resolve_cell_tx_inputs_from_provider(tx, provider.as_ref(), block_hash)
                                .map_err(|_| RuleError::TxInContextFailed(tx.id().into(), TxRuleError::MissingTxOutpoints))?;
                            let resolved_tx = MutableTransaction::with_resolved_metadata(tx.clone(), resolved_inputs);
                            let storage_mass = self
                                .mass_calculator
                                .calc_contextual_masses(&resolved_tx.as_verifiable())
                                .map(|contextual_masses| contextual_masses.storage_mass)
                                .ok_or_else(|| RuleError::TxInContextFailed(tx.id().into(), TxRuleError::MissingTxOutpoints))?;
                            Ok((non_contextual_masses, storage_mass, verified_cycles))
                        });
                    (idx, res)
                })
                .collect();

            let vm_limits = VmLimits::default();
            let max_cycles = CellConsensusParams::default().max_block_cycles;
            let mut total_block_cycles = 0u64;
            let mut total_compute_mass = 0u64;
            let mut total_transient_mass = 0u64;
            let mut total_storage_mass = 0u64;

            for (idx, res) in results {
                let (non_contextual_masses, storage_mass, tx_cycles) = res?;
                total_block_cycles = total_block_cycles.saturating_add(tx_cycles);
                if total_block_cycles > max_cycles {
                    return Err(RuleError::CellValidationError(format!(
                        "block script cycles exceeded limit at tx index {}: total {}, limit {}",
                        idx, total_block_cycles, max_cycles
                    )));
                }

                let effective_compute_mass = non_contextual_masses.compute_mass.max(vm_limits.effective_size(
                    spora_consensus_core::mass::cell_tx_estimated_serialized_size(&block.transactions[idx]) as usize,
                    tx_cycles,
                ) as u64);
                total_compute_mass = total_compute_mass.saturating_add(effective_compute_mass);
                total_transient_mass = total_transient_mass.saturating_add(non_contextual_masses.transient_mass);
                total_storage_mass = total_storage_mass.saturating_add(storage_mass);

                if total_compute_mass > self.max_block_mass {
                    return Err(RuleError::ExceedsComputeMassLimit(total_compute_mass, self.max_block_mass));
                }
                if total_transient_mass > self.max_block_mass {
                    return Err(RuleError::ExceedsTransientMassLimit(total_transient_mass, self.max_block_mass));
                }
                if total_storage_mass > self.max_block_mass {
                    return Err(RuleError::ExceedsStorageMassLimit(total_storage_mass, self.max_block_mass));
                }
            }

            return Ok((
                NonContextualMasses::new(total_compute_mass, total_transient_mass),
                ContextualMasses::new(total_storage_mass),
            ));
        }

        #[cfg(not(feature = "vm"))]
        {
            let block_hash = block.hash();
            let daa_score = block.header.daa_score;
            let timestamp = block.header.timestamp;

            let results: Vec<Result<(NonContextualMasses, u64), RuleError>> = non_coinbase_txs
                .par_iter()
                .map(|&(_, tx)| {
                    let non_contextual_masses = self.mass_calculator.calc_non_contextual_masses_cell(tx);
                    validator
                        .validate_in_dag(tx, block_hash, daa_score, timestamp)
                        .map_err(|e| self.map_cell_validation_error(tx, block_hash, daa_score, provider.as_ref(), e))
                        .and_then(|_| {
                            let resolved_inputs = self
                                .resolve_cell_tx_inputs_from_provider(tx, provider.as_ref(), block_hash)
                                .map_err(|_| RuleError::TxInContextFailed(tx.id().into(), TxRuleError::MissingTxOutpoints))?;
                            let resolved_tx = MutableTransaction::with_resolved_metadata(tx.clone(), resolved_inputs);
                            let storage_mass = self
                                .mass_calculator
                                .calc_contextual_masses(&resolved_tx.as_verifiable())
                                .map(|contextual_masses| contextual_masses.storage_mass)
                                .ok_or_else(|| RuleError::TxInContextFailed(tx.id().into(), TxRuleError::MissingTxOutpoints))?;
                            Ok((non_contextual_masses, storage_mass))
                        })
                })
                .collect();

            let mut total_compute_mass = 0u64;
            let mut total_transient_mass = 0u64;
            let mut total_storage_mass = 0u64;
            for res in results {
                let (non_contextual_masses, storage_mass) = res?;
                total_compute_mass = total_compute_mass.saturating_add(non_contextual_masses.compute_mass);
                total_transient_mass = total_transient_mass.saturating_add(non_contextual_masses.transient_mass);
                total_storage_mass = total_storage_mass.saturating_add(storage_mass);

                if total_compute_mass > self.max_block_mass {
                    return Err(RuleError::ExceedsComputeMassLimit(total_compute_mass, self.max_block_mass));
                }
                if total_transient_mass > self.max_block_mass {
                    return Err(RuleError::ExceedsTransientMassLimit(total_transient_mass, self.max_block_mass));
                }
                if total_storage_mass > self.max_block_mass {
                    return Err(RuleError::ExceedsStorageMassLimit(total_storage_mass, self.max_block_mass));
                }
            }

            return Ok((
                NonContextualMasses::new(total_compute_mass, total_transient_mass),
                ContextualMasses::new(total_storage_mass),
            ));
        }
    }

    fn build_body_validation_provider(
        self: &Arc<Self>,
        block: &Block,
    ) -> BlockProcessResult<BodyValidationOverlayProvider<BodyConsensusCellProvider>> {
        let ghostdag_data = self
            .ghostdag_store
            .get_data(block.hash())
            .ok()
            .or_else(|| Some(Arc::new(self.ghostdag_manager.ghostdag(block.header.direct_parents()))));
        let selected_parent = ghostdag_data
            .as_ref()
            .map(|data| data.selected_parent)
            .or_else(|| self.ghostdag_store.get_selected_parent(block.hash()).ok())
            .or_else(|| block.header.direct_parents().iter().copied().next())
            .ok_or(RuleError::NoParents)?;

        let base_provider = ConsensusCellProvider::new(
            self.ghostdag_store.clone(),
            self.reachability_service.clone(),
            self.headers_store.clone(),
            self.cell_diffs_store.clone(),
            self.cell_roots_store.clone(),
            self.block_transactions_store.clone(),
            self.cell_data_store.clone(),
            self.cell_data_segment_reader.clone(),
            self.statuses_store.clone(),
        );
        let mut provider = BodyValidationOverlayProvider::new(base_provider, block.hash(), selected_parent);
        let mut processed_txs = HashSet::new();

        let selected_parent_transactions = self
            .block_transactions_store
            .get(selected_parent)
            .map_err(|e| RuleError::Store(format!("block tx lookup failed for selected parent {}: {}", selected_parent, e)))?;
        let selected_parent_daa_score = self
            .headers_store
            .get_daa_score(selected_parent)
            .map_err(|e| RuleError::Store(format!("header daa lookup failed for selected parent {}: {}", selected_parent, e)))?;
        if let Some(coinbase) = selected_parent_transactions.first() {
            processed_txs.insert(Hash::from_bytes(coinbase.id()));
            for (output_index, output) in coinbase.outputs.iter().enumerate() {
                let out_point = OutPoint::new(coinbase.id(), output_index as u32);
                let metadata = Self::cell_metadata_from_output(
                    selected_parent,
                    selected_parent_daa_score,
                    true,
                    coinbase.id(),
                    output_index as u32,
                    output,
                    coinbase.outputs_data.get(output_index).map(|data| data.as_slice()).unwrap_or(&[]),
                );
                provider
                    .add_cell(out_point, metadata)
                    .map_err(|e| RuleError::CellValidationError(format!("failed to build body validation overlay: {e}")))?;
            }
        }

        let mergeset_blues =
            ghostdag_data.as_ref().map(|data| data.mergeset_blues.iter().copied().collect::<Vec<_>>()).unwrap_or_default();

        for blue_block in mergeset_blues {
            let transactions = self
                .block_transactions_store
                .get(blue_block)
                .map_err(|e| RuleError::Store(format!("block tx lookup failed for {}: {}", blue_block, e)))?;
            let blue_daa_score = self
                .headers_store
                .get_daa_score(blue_block)
                .map_err(|e| RuleError::Store(format!("header daa lookup failed for {}: {}", blue_block, e)))?;

            for (tx_index, tx) in transactions.iter().enumerate() {
                if !processed_txs.insert(Hash::from_bytes(tx.id())) {
                    continue;
                }

                for dep in &tx.deps {
                    provider
                        .ensure_dep_available(&dep.out_point)
                        .map_err(|e| RuleError::CellValidationError(format!("failed to build body validation overlay: {e}")))?;

                    if dep.dep_type == DepType::DepGroup {
                        // Expand the DepGroup: read its data, parse OutPoint list, ensure each is available
                        let meta = provider
                            .get_cell_metadata(&dep.out_point)
                            .map_err(|e| RuleError::CellValidationError(format!("failed to read dep group metadata: {e}")))?;
                        if let Some(meta) = meta {
                            if let Some(ref data) = meta.data {
                                let outpoints = spora_exec::parse_dep_group_data(data)
                                    .map_err(|e| RuleError::CellValidationError(format!("invalid DepGroup data: {e}")))?;
                                for op in &outpoints {
                                    provider.ensure_dep_available(op).map_err(|e| {
                                        RuleError::CellValidationError(format!("DepGroup expanded dep unavailable: {e}"))
                                    })?;
                                }
                            }
                        }
                    }
                }

                for input in &tx.inputs {
                    provider
                        .spend_cell(&input.out_point)
                        .map_err(|e| RuleError::CellValidationError(format!("failed to build body validation overlay: {e}")))?;
                }

                for (output_index, output) in tx.outputs.iter().enumerate() {
                    let out_point = OutPoint::new(tx.id(), output_index as u32);
                    let metadata = Self::cell_metadata_from_output(
                        blue_block,
                        blue_daa_score,
                        tx_index == 0 && tx.is_coinbase(),
                        tx.id(),
                        output_index as u32,
                        output,
                        tx.outputs_data.get(output_index).map(|data| data.as_slice()).unwrap_or(&[]),
                    );
                    provider
                        .add_cell(out_point, metadata)
                        .map_err(|e| RuleError::CellValidationError(format!("failed to build body validation overlay: {e}")))?;
                }
            }
        }

        Ok(provider)
    }

    fn cell_metadata_from_output(
        block_hash: Hash,
        block_daa_score: u64,
        is_cellbase: bool,
        tx_id: [u8; 32],
        output_index: u32,
        output: &spora_exec::CellOut,
        output_data: &[u8],
    ) -> CellMetadata {
        CellMetadata {
            out_point: TransactionOutpoint { tx_hash: tx_id, index: output_index },
            capacity: output.capacity,
            data_bytes: output_data.len() as u64,
            lock_hash: output.lock.hash(),
            type_hash: output.type_.as_ref().map(|script| script.hash()),
            data_hash: Self::compute_output_data_hash(output_data),
            block_daa_score,
            is_cellbase,
            block_hash,
            lock_code_hash: Some(output.lock.code_hash),
            type_code_hash: output.type_.as_ref().map(|script| script.code_hash),
            lock_script: Some(output.lock.clone()),
            type_script: output.type_.clone(),
            data: Some(output_data.to_vec()),
        }
    }

    fn compute_output_data_hash(data: &[u8]) -> [u8; 32] {
        if data.is_empty() {
            return [0u8; 32];
        }

        use blake3::Hasher;

        let mut hasher = Hasher::new();
        hasher.update(b"spora-cell/data");
        hasher.update(data);
        *hasher.finalize().as_bytes()
    }

    fn resolve_cell_tx_inputs_from_provider<P: DagCellProvider>(
        &self,
        tx: &spora_exec::CellTx,
        provider: &P,
        pov: Hash,
    ) -> Result<Vec<CellMetadata>, String> {
        tx.inputs
            .iter()
            .map(|input| {
                provider.get_cell_at_pov(&input.out_point, pov)?.ok_or_else(|| format!("missing input cell {:?}", input.out_point))
            })
            .collect()
    }

    fn map_cell_validation_error<P: DagCellProvider>(
        &self,
        tx: &spora_exec::CellTx,
        pov: Hash,
        current_daa: u64,
        provider: &P,
        error: CellValidationError,
    ) -> RuleError {
        let tx_id = tx.id().into();

        match error {
            CellValidationError::CellNotFound(_)
            | CellValidationError::DepCellNotFound(_)
            | CellValidationError::CellAlreadySpent(_) => RuleError::TxInContextFailed(tx_id, TxRuleError::MissingTxOutpoints),
            CellValidationError::InvalidFormat(msg)
                if msg.contains("lookup error") || msg.contains("Status lookup error") || msg.contains("unexpected POV") =>
            {
                RuleError::TxInContextFailed(tx_id, TxRuleError::MissingTxOutpoints)
            }
            CellValidationError::CapacityOverflow => RuleError::TxInContextFailed(tx_id, TxRuleError::InputAmountOverflow),
            CellValidationError::InsufficientCapacity { required, available } => {
                RuleError::TxInContextFailed(tx_id, TxRuleError::SpendTooHigh(required, available))
            }
            CellValidationError::TimeLockNotSatisfied { .. } => {
                RuleError::TxInContextFailed(tx_id, TxRuleError::SequenceLockConditionsAreNotMet)
            }
            CellValidationError::CellbaseNotMature { .. } => {
                let maturity = self.coinbase_maturity;

                for (input_index, input) in tx.inputs.iter().enumerate() {
                    let metadata = provider.get_cell_at_pov(&input.out_point, pov).ok().flatten();
                    if let Some(metadata) = metadata {
                        if metadata.is_cellbase && current_daa < metadata.block_daa_score.saturating_add(maturity) {
                            return RuleError::TxInContextFailed(
                                tx_id,
                                TxRuleError::ImmatureCoinbaseSpend(
                                    input_index,
                                    TransactionOutpoint::new(input.out_point.tx_hash, input.out_point.index),
                                    metadata.block_daa_score,
                                    current_daa,
                                    maturity,
                                ),
                            );
                        }
                    }
                }

                RuleError::TxInContextFailed(tx_id, TxRuleError::MissingTxOutpoints)
            }
            CellValidationError::InvalidFormat(msg) => {
                RuleError::CellValidationError(format!("Context validation failed for tx {:?}: {}", tx_id, msg))
            }
            other => RuleError::CellValidationError(format!("Context validation failed for tx {:?}: {other}", tx_id)),
        }
    }

    fn check_parent_bodies_exist(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        let statuses_read_guard = self.statuses_store.read();
        let missing: Vec<Hash> = block
            .header
            .direct_parents()
            .iter()
            .copied()
            .filter(|parent| {
                let status_option = statuses_read_guard.get(*parent).unwrap_option();
                status_option.is_none_or(|s| !s.has_block_body())
            })
            .collect();
        if !missing.is_empty() {
            return Err(RuleError::MissingParents(missing));
        }

        Ok(())
    }

    fn check_coinbase_outputs_limit(&self, block: &Block) -> BlockProcessResult<()> {
        // [Crescendo]: coinbase_outputs_limit depends on ghostdag k and thus depends on fork activation
        // which makes it header contextual.
        //
        // TODO (post HF): move this check back to transaction in isolation validation

        // [Crescendo]: Ghostdag k activation is decided based on selected parent DAA score
        // so we follow the same methodology for coinbase output limit (which is driven from the
        // actual bound on the number of blue blocks in the mergeset).
        //
        // Note that body validation in context is not called for trusted blocks, so we can safely assume
        // the selected parent exists and its daa score is accessible
        let selected_parent = self
            .ghostdag_store
            .get_selected_parent(block.hash())
            .ok()
            .or_else(|| block.header.direct_parents().iter().copied().next())
            .expect("block must have a selected parent or at least one direct parent");
        let _selected_parent_daa_score = self.headers_store.get_daa_score(selected_parent).unwrap();
        let coinbase_outputs_limit = self.ghostdag_k as u64 + 2;

        let tx = &block.transactions[0];
        if tx.outputs.len() as u64 > coinbase_outputs_limit {
            return Err(RuleError::TxInIsolationValidationFailed(
                tx.id().into(),
                TxRuleError::CoinbaseTooManyOutputs(tx.outputs.len(), coinbase_outputs_limit),
            ));
        }
        Ok(())
    }

    fn check_coinbase_blue_score_and_subsidy(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        // CellTx coinbase payload is in outputs_data[0]
        let payload = block.transactions[0].payload().unwrap_or(&[]);
        match self.coinbase_manager.deserialize_coinbase_payload(payload) {
            Ok(data) => {
                if data.blue_score != block.header.blue_score {
                    return Err(RuleError::BadCoinbasePayloadBlueScore(data.blue_score, block.header.blue_score));
                }

                let expected_subsidy = self.coinbase_manager.calc_block_subsidy(block.header.daa_score);

                if data.subsidy != expected_subsidy {
                    return Err(RuleError::WrongSubsidy(expected_subsidy, data.subsidy));
                }

                Ok(())
            }
            Err(e) => Err(RuleError::BadCoinbasePayload(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BodyValidationOverlayProvider;
    use crate::processes::{CellStateProvider, DagCellProvider};
    use crate::{config::ConfigBuilder, consensus::test_consensus::TestConsensus, errors::RuleError, params::DEVNET_PARAMS};
    use spora_consensus_core::{
        api::ConsensusApi, block::Block, cell_metadata::CellMetadata, config::params::MAINNET_PARAMS, errors::tx::TxRuleError,
        merkle::calc_hash_merkle_root_cell as calc_hash_merkle_root_with_options, tx::TransactionOutpoint,
    };
    use spora_core::assert_match;
    use spora_exec::{CellDep, CellOut, CellRef, CellTx, DepType, OutPoint, ScriptRef};
    use spora_hashes::Hash;
    use std::collections::HashMap;

    fn calc_hash_merkle_root<'a>(txs: impl ExactSizeIterator<Item = &'a CellTx>) -> Hash {
        calc_hash_merkle_root_with_options(txs, false)
    }

    fn build_block_with_extra_transactions(
        consensus: &TestConsensus,
        hash: Hash,
        parents: Vec<Hash>,
        extra_txs: Vec<CellTx>,
    ) -> Block {
        let mut block = consensus.build_block_with_parents_and_transactions(hash, parents, vec![]);
        block.transactions.extend(extra_txs);
        block.header.hash_merkle_root = calc_hash_merkle_root(block.transactions.iter());
        block.to_immutable()
    }

    fn build_spend_tx(outpoint: TransactionOutpoint, output_capacity: u64) -> CellTx {
        let lock = ScriptRef::new([0; 32], 0, vec![]);
        CellTx::new(
            vec![CellRef::new(OutPoint::new(outpoint.tx_hash, outpoint.index), 0)],
            vec![],
            vec![CellOut { lock, type_: None, capacity: output_capacity }],
            vec![vec![]],
            vec![],
        )
        .unwrap()
    }

    fn build_spend_tx_with_dep(outpoint: TransactionOutpoint, dep_outpoint: TransactionOutpoint, output_capacity: u64) -> CellTx {
        let lock = ScriptRef::new([0; 32], 0, vec![]);
        CellTx::new(
            vec![CellRef::new(OutPoint::new(outpoint.tx_hash, outpoint.index), 0)],
            vec![CellDep { out_point: OutPoint::new(dep_outpoint.tx_hash, dep_outpoint.index), dep_type: DepType::Code }],
            vec![CellOut { lock, type_: None, capacity: output_capacity }],
            vec![vec![]],
            vec![],
        )
        .unwrap()
    }

    #[derive(Default)]
    struct MockDagProvider {
        cells: HashMap<OutPoint, CellMetadata>,
    }

    impl CellStateProvider for MockDagProvider {
        fn is_cell_available(&self, out_point: &OutPoint, _pov: Hash) -> Result<bool, String> {
            Ok(self.cells.contains_key(out_point))
        }

        fn get_cell_capacity(&self, out_point: &OutPoint, _pov: Hash) -> Result<Option<u64>, String> {
            Ok(self.cells.get(out_point).map(|meta| meta.capacity))
        }
    }

    impl DagCellProvider for MockDagProvider {
        fn get_cell_at_pov(&self, out_point: &OutPoint, _pov: Hash) -> Result<Option<CellMetadata>, String> {
            Ok(self.cells.get(out_point).cloned())
        }

        fn get_block_timestamp(&self, _block_hash: Hash) -> Result<u64, String> {
            Ok(0)
        }
    }

    #[tokio::test]
    async fn validate_body_in_context_test() {
        let config = ConfigBuilder::new(DEVNET_PARAMS)
            .skip_proof_of_work()
            .edit_consensus_params(|p| {
                p.deflationary_phase_daa_score = 2;
            })
            .build();
        let consensus = TestConsensus::new(&config);
        let wait_handles = consensus.init();
        let body_processor = consensus.block_body_processor();

        consensus.add_block_with_parents(1.into(), vec![config.genesis.hash]).await.unwrap();

        {
            let block = consensus.build_block_with_parents_and_transactions(2.into(), vec![1.into()], vec![]);
            // `add_block_with_parents` now produces a fully inserted parent in this test harness,
            // so the context check should pass for its child.
            assert_match!(body_processor.validate_body_in_context(&block.to_immutable()), Ok(_));
        }

        let valid_block = consensus.build_block_with_parents_and_transactions(3.into(), vec![config.genesis.hash], vec![]);
        consensus.validate_and_insert_block(valid_block.to_immutable()).virtual_state_task.await.unwrap();
        {
            let mut block = consensus.build_block_with_parents_and_transactions(2.into(), vec![3.into()], vec![]);
            block.transactions[0].outputs_data[0][8..16].copy_from_slice(&(5_u64).to_le_bytes());
            block.header.hash_merkle_root = calc_hash_merkle_root(block.transactions.iter());

            assert_match!(
                consensus.validate_and_insert_block(block.clone().to_immutable()).virtual_state_task.await, Err(RuleError::WrongSubsidy(expected,_)) if expected == 11400000000);

            // The second time we send an invalid block we expect it to be a known invalid.
            assert_match!(
                consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await,
                Err(RuleError::KnownInvalid)
            );
        }

        {
            let mut block = consensus.build_block_with_parents_and_transactions(4.into(), vec![3.into()], vec![]);
            block.transactions[0].outputs_data[0][0..8].copy_from_slice(&(100_u64).to_le_bytes());
            block.header.hash_merkle_root = calc_hash_merkle_root(block.transactions.iter());

            assert_match!(
                consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await,
                Err(RuleError::BadCoinbasePayloadBlueScore(_, _))
            );
        }

        {
            let mut block = consensus.build_block_with_parents_and_transactions(5.into(), vec![3.into()], vec![]);
            block.transactions[0].outputs_data[0] = vec![];
            block.header.hash_merkle_root = calc_hash_merkle_root(block.transactions.iter());

            assert_match!(
                consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await,
                Err(RuleError::BadCoinbasePayload(_))
            );
        }

        let valid_block_child = consensus.build_block_with_parents_and_transactions(6.into(), vec![3.into()], vec![]);
        consensus.validate_and_insert_block(valid_block_child.clone().to_immutable()).virtual_state_task.await.unwrap();
        {
            // The block DAA score is 2, so the subsidy should be calculated according to the deflationary stage.
            let mut block = consensus.build_block_with_parents_and_transactions(7.into(), vec![6.into()], vec![]);
            block.transactions[0].outputs_data[0][8..16].copy_from_slice(&(5_u64).to_le_bytes());
            block.header.hash_merkle_root = calc_hash_merkle_root(block.transactions.iter());
            assert_match!(consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await, Err(RuleError::WrongSubsidy(expected,_)) if expected == 4500000000);
        }

        consensus.shutdown(wait_handles);
    }

    #[tokio::test]
    async fn rejects_missing_outpoints_during_body_context_validation() {
        let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
        let consensus = TestConsensus::new(&config);
        let wait_handles = consensus.init();
        let body_processor = consensus.block_body_processor();

        let parent = consensus.build_block_with_parents_and_transactions(1.into(), vec![config.genesis.hash], vec![]);
        consensus.validate_and_insert_block(parent.to_immutable()).virtual_state_task.await.unwrap();

        let missing_outpoint = TransactionOutpoint { tx_hash: [0xAA; 32], index: 0 };
        let invalid_tx = build_spend_tx(missing_outpoint, 1_000);
        let block = build_block_with_extra_transactions(&consensus, 2.into(), vec![1.into()], vec![invalid_tx]);

        assert_match!(
            body_processor.validate_body_in_context(&block),
            Err(RuleError::TxInContextFailed(_, TxRuleError::MissingTxOutpoints))
        );

        consensus.shutdown(wait_handles);
    }

    #[tokio::test]
    async fn rejects_missing_deps_during_body_context_validation() {
        let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
        let consensus = TestConsensus::new(&config);
        let wait_handles = consensus.init();
        let body_processor = consensus.block_body_processor();

        let parent = consensus.build_block_with_parents_and_transactions(1.into(), vec![config.genesis.hash], vec![]);
        let spendable_outpoint = TransactionOutpoint { tx_hash: parent.transactions[0].id(), index: 0 };
        consensus.validate_and_insert_block(parent.to_immutable()).virtual_state_task.await.unwrap();

        let missing_dep_outpoint = TransactionOutpoint { tx_hash: [0xBB; 32], index: 0 };
        let invalid_tx = build_spend_tx_with_dep(spendable_outpoint, missing_dep_outpoint, 1_000);
        let block = build_block_with_extra_transactions(&consensus, 2.into(), vec![1.into()], vec![invalid_tx]);

        assert_match!(
            body_processor.validate_body_in_context(&block),
            Err(RuleError::TxInContextFailed(_, TxRuleError::MissingTxOutpoints))
        );

        consensus.shutdown(wait_handles);
    }

    #[tokio::test]
    async fn rejects_immature_coinbase_spend_during_body_context_validation() {
        let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
        let consensus = TestConsensus::new(&config);
        let wait_handles = consensus.init();
        let body_processor = consensus.block_body_processor();

        let parent = consensus.build_block_with_parents_and_transactions(10.into(), vec![config.genesis.hash], vec![]);
        let parent_coinbase_id = parent.transactions[0].id();
        consensus.validate_and_insert_block(parent.to_immutable()).virtual_state_task.await.unwrap();

        let immature_outpoint = TransactionOutpoint { tx_hash: parent_coinbase_id, index: 0 };
        let invalid_tx = build_spend_tx(immature_outpoint, 1_000);
        let block = build_block_with_extra_transactions(&consensus, 11.into(), vec![10.into()], vec![invalid_tx]);

        assert_match!(
            body_processor.validate_body_in_context(&block),
            Err(RuleError::TxInContextFailed(_, TxRuleError::ImmatureCoinbaseSpend(..)))
        );

        consensus.shutdown(wait_handles);
    }

    #[tokio::test]
    async fn accepts_child_spend_of_parent_non_coinbase_output_during_body_context_validation() {
        let config = ConfigBuilder::new(MAINNET_PARAMS)
            .skip_proof_of_work()
            .edit_consensus_params(|params| {
                params.coinbase_maturity = 0;
            })
            .build();
        let consensus = TestConsensus::new(&config);
        let wait_handles = consensus.init();
        let body_processor = consensus.block_body_processor();

        let reward_source = consensus.build_block_with_parents_and_transactions(1.into(), vec![config.genesis.hash], vec![]);
        let reward_source_hash = reward_source.header.hash;
        let reward_source_coinbase = reward_source.transactions[0].clone();
        let reward_outpoint = TransactionOutpoint { tx_hash: reward_source_coinbase.id(), index: 0 };
        let reward_capacity = reward_source_coinbase.outputs[0].capacity;
        consensus.validate_and_insert_block(reward_source.to_immutable()).virtual_state_task.await.unwrap();

        let parent_tx = build_spend_tx(reward_outpoint, reward_capacity - 1_000);
        let parent_tx_id = parent_tx.id();
        let parent_output_capacity = parent_tx.outputs[0].capacity;
        let parent = build_block_with_extra_transactions(&consensus, 2.into(), vec![reward_source_hash], vec![parent_tx]);
        let parent_hash = parent.header.hash;
        consensus.validate_and_insert_block(parent).virtual_state_task.await.unwrap();

        let child_tx = build_spend_tx(TransactionOutpoint { tx_hash: parent_tx_id, index: 0 }, parent_output_capacity - 1_000);
        let child = build_block_with_extra_transactions(&consensus, 3.into(), vec![parent_hash], vec![child_tx]);

        body_processor
            .validate_body_in_context(&child)
            .expect("body validation overlay should resolve outputs created by the selected parent block");
        consensus.validate_and_insert_block(child).virtual_state_task.await.unwrap();

        consensus.shutdown(wait_handles);
    }

    #[test]
    fn overlay_marks_spent_dep_as_unavailable() {
        let snapshot_pov = Hash::from_bytes([1; 32]);
        let base_pov = Hash::from_bytes([2; 32]);
        let dep_outpoint = OutPoint::new([3; 32], 0);
        let metadata = CellMetadata {
            out_point: TransactionOutpoint { tx_hash: dep_outpoint.tx_hash, index: dep_outpoint.index },
            capacity: 1_000,
            data_bytes: 0,
            lock_hash: [0; 32],
            type_hash: None,
            data_hash: [0; 32],
            block_daa_score: 0,
            is_cellbase: false,
            block_hash: Hash::from_bytes([4; 32]),
            lock_code_hash: None,
            type_code_hash: None,
            lock_script: None,
            type_script: None,
            data: None,
        };

        let mut overlay = BodyValidationOverlayProvider::new(
            MockDagProvider { cells: HashMap::from([(dep_outpoint.clone(), metadata)]) },
            snapshot_pov,
            base_pov,
        );

        assert!(overlay.ensure_dep_available(&dep_outpoint).is_ok());
        overlay.spend_cell(&dep_outpoint).unwrap();
        assert!(overlay.ensure_dep_available(&dep_outpoint).is_err());
    }
}
