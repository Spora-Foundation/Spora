use super::BlockBodyProcessor;
#[cfg(feature = "vm")]
use crate::processes::cell_validator::{CellScriptVerificationState, CellScriptVerifyResult};
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

#[cfg(feature = "vm")]
#[derive(Clone, Debug)]
pub struct BodyValidationContextState {
    pub current: usize,
    pub script_state: CellScriptVerificationState,
    pub total_block_cycles: u64,
    pub total_compute_mass: u64,
    pub total_transient_mass: u64,
    pub total_storage_mass: u64,
    pub limit_cycles: u64,
}

#[cfg(feature = "vm")]
impl BodyValidationContextState {
    fn new(
        current: usize,
        script_state: CellScriptVerificationState,
        total_block_cycles: u64,
        total_compute_mass: u64,
        total_transient_mass: u64,
        total_storage_mass: u64,
        limit_cycles: u64,
    ) -> Self {
        Self { current, script_state, total_block_cycles, total_compute_mass, total_transient_mass, total_storage_mass, limit_cycles }
    }

    pub fn current_cycles(&self) -> u64 {
        self.total_block_cycles.saturating_add(self.script_state.current_cycles())
    }

    pub fn next_limit_cycles(&self, step_cycles: u64, max_cycles: u64) -> (u64, bool) {
        let current_cycles = self.current_cycles();
        let capped_max_cycles = max_cycles.max(current_cycles);
        let next_limit = self.limit_cycles.saturating_add(step_cycles).max(current_cycles);
        if next_limit < capped_max_cycles {
            (next_limit, false)
        } else {
            (capped_max_cycles, true)
        }
    }
}

#[cfg(feature = "vm")]
#[derive(Debug)]
pub enum BodyValidationContextResult {
    Completed(Mass),
    Suspended(BodyValidationContextState),
}

#[cfg(feature = "vm")]
#[derive(Clone, Copy, Debug, Default)]
struct BodyValidationAccumulators {
    block_cycles: u64,
    compute_mass: u64,
    transient_mass: u64,
    storage_mass: u64,
}

#[cfg(feature = "vm")]
impl BodyValidationAccumulators {
    fn into_mass(self) -> Mass {
        (NonContextualMasses::new(self.compute_mass, self.transient_mass), ContextualMasses::new(self.storage_mass))
    }
}

impl BlockBodyProcessor {
    pub fn validate_body_in_context(self: &Arc<Self>, block: &Block) -> BlockProcessResult<Mass> {
        self.check_parent_bodies_exist(block)?;
        self.check_coinbase_outputs_limit(block)?;
        self.check_coinbase_blue_score_and_subsidy(block)?;
        self.check_block_transactions_in_context(block)
    }

    #[cfg(feature = "vm")]
    pub fn validate_body_in_context_resumable(
        self: &Arc<Self>,
        block: &Block,
        limit_cycles: u64,
    ) -> BlockProcessResult<BodyValidationContextResult> {
        self.check_parent_bodies_exist(block)?;
        self.check_coinbase_outputs_limit(block)?;
        self.check_coinbase_blue_score_and_subsidy(block)?;
        self.check_block_transactions_in_context_resumable(block, None, limit_cycles)
    }

    #[cfg(feature = "vm")]
    pub fn resume_body_in_context_from_state(
        self: &Arc<Self>,
        block: &Block,
        state: &BodyValidationContextState,
        limit_cycles: u64,
    ) -> BlockProcessResult<BodyValidationContextResult> {
        self.check_parent_bodies_exist(block)?;
        self.check_coinbase_outputs_limit(block)?;
        self.check_coinbase_blue_score_and_subsidy(block)?;
        self.check_block_transactions_in_context_resumable(block, Some(state), limit_cycles)
    }

    #[cfg(feature = "vm")]
    pub fn complete_body_in_context_from_state(
        self: &Arc<Self>,
        block: &Block,
        state: &BodyValidationContextState,
        max_cycles: u64,
    ) -> BlockProcessResult<Mass> {
        match self.resume_body_in_context_from_state(block, state, max_cycles)? {
            BodyValidationContextResult::Completed(mass) => Ok(mass),
            BodyValidationContextResult::Suspended(next_state) => Err(RuleError::CellValidationError(format!(
                "body script cycles exceeded limit while completing: total {}, limit {}",
                next_state.current_cycles(),
                max_cycles
            ))),
        }
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
        //
        // NOTE: Isolation checks (format, capacity, data-size) are already
        // performed by `validate_body_in_isolation` which the caller
        // (`validate_body`) is required to invoke first. We therefore skip
        // `validate_in_isolation` here and go straight to DAG-context +
        // script verification.
        #[cfg(feature = "vm")]
        {
            let block_hash = block.hash();
            let daa_score = block.header.daa_score;
            let timestamp = block.header.timestamp;

            let results: Vec<(usize, Result<(NonContextualMasses, u64, u64), RuleError>)> = non_coinbase_txs
                .par_iter()
                .map(|&(idx, tx)| {
                    let non_contextual_masses = self.mass_calculator.calc_non_contextual_masses_cell(tx);
                    // DAG-context validation (existence, capacity, maturity, time-locks)
                    // followed by script verification. Isolation validation is intentionally
                    // omitted — it was already executed by `validate_body_in_isolation`.
                    let res = validator
                        .validate_in_dag(tx, block_hash, daa_score, timestamp)
                        .and_then(|_| validator.verify_scripts_with_cycles(tx, block_hash, daa_score))
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
                    // DAG-context validation only; isolation was already done
                    // by `validate_body_in_isolation`.
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

    #[cfg(feature = "vm")]
    fn check_block_transactions_in_context_resumable(
        self: &Arc<Self>,
        block: &Block,
        state: Option<&BodyValidationContextState>,
        limit_cycles: u64,
    ) -> BlockProcessResult<BodyValidationContextResult> {
        if block.transactions.iter().all(|tx| tx.is_coinbase()) {
            return Ok(BodyValidationContextResult::Completed((NonContextualMasses::new(0, 0), ContextualMasses::new(0))));
        }

        let provider = Arc::new(self.build_body_validation_provider(block)?);
        let validator = CellValidator::new(
            Arc::new(CellConsensusParams { cellbase_maturity: self.coinbase_maturity, ..CellConsensusParams::default() }),
            provider.clone(),
        );
        let non_coinbase_txs: Vec<(usize, &spora_exec::CellTx)> =
            block.transactions.iter().enumerate().filter(|(_, tx)| !tx.is_coinbase()).collect();
        let block_hash = block.hash();
        let daa_score = block.header.daa_score;
        let timestamp = block.header.timestamp;
        let vm_limits = VmLimits::default();
        let max_block_cycles = CellConsensusParams::default().max_block_cycles;
        let effective_limit = limit_cycles.min(max_block_cycles);

        let mut accum = state
            .map(|state| BodyValidationAccumulators {
                block_cycles: state.total_block_cycles,
                compute_mass: state.total_compute_mass,
                transient_mass: state.total_transient_mass,
                storage_mass: state.total_storage_mass,
            })
            .unwrap_or_default();
        let start = state.map(|state| state.current).unwrap_or(0);

        if start > non_coinbase_txs.len() {
            return Err(RuleError::CellValidationError(format!(
                "resumable body validation state out of range: current {}, tx count {}",
                start,
                non_coinbase_txs.len()
            )));
        }
        if let Some(state) = state {
            if state.current_cycles() > effective_limit {
                return Err(RuleError::CellValidationError(format!(
                    "body script cycles exceeded limit while resuming: total {}, limit {}",
                    state.current_cycles(),
                    effective_limit
                )));
            }
        }

        for position in start..non_coinbase_txs.len() {
            let (_, tx) = non_coinbase_txs[position];
            let non_contextual_masses = self.mass_calculator.calc_non_contextual_masses_cell(tx);
            self.validate_body_tx_in_context(tx, block_hash, daa_score, timestamp, provider.as_ref(), &validator)?;
            let storage_mass = self.resolve_body_tx_storage_mass(tx, provider.as_ref(), block_hash)?;

            let remaining_block_cycles = effective_limit.saturating_sub(accum.block_cycles);
            let script_result = match state.filter(|state| state.current == position) {
                Some(state) => {
                    validator.resume_scripts_from_state(tx, block_hash, daa_score, &state.script_state, remaining_block_cycles)
                }
                None => validator.verify_scripts_resumable(tx, block_hash, daa_score, remaining_block_cycles),
            };

            match script_result.map_err(|err| self.map_cell_validation_error(tx, block_hash, daa_score, provider.as_ref(), err))? {
                CellScriptVerifyResult::Completed(tx_cycles) => {
                    self.accumulate_body_tx_validation(tx, non_contextual_masses, storage_mass, tx_cycles, &mut accum, &vm_limits)?;
                }
                CellScriptVerifyResult::Suspended(script_state) => {
                    return Ok(BodyValidationContextResult::Suspended(BodyValidationContextState::new(
                        position,
                        script_state,
                        accum.block_cycles,
                        accum.compute_mass,
                        accum.transient_mass,
                        accum.storage_mass,
                        effective_limit,
                    )));
                }
            }
        }

        Ok(BodyValidationContextResult::Completed(accum.into_mass()))
    }

    #[cfg(feature = "vm")]
    fn validate_body_tx_in_context(
        &self,
        tx: &spora_exec::CellTx,
        block_hash: Hash,
        daa_score: u64,
        timestamp: u64,
        provider: &BodyValidationOverlayProvider<BodyConsensusCellProvider>,
        validator: &CellValidator<BodyValidationOverlayProvider<BodyConsensusCellProvider>>,
    ) -> BlockProcessResult<()> {
        validator
            .validate_in_dag(tx, block_hash, daa_score, timestamp)
            .map_err(|err| self.map_cell_validation_error(tx, block_hash, daa_score, provider, err))
    }

    #[cfg(feature = "vm")]
    fn resolve_body_tx_storage_mass(
        &self,
        tx: &spora_exec::CellTx,
        provider: &BodyValidationOverlayProvider<BodyConsensusCellProvider>,
        block_hash: Hash,
    ) -> BlockProcessResult<u64> {
        let resolved_inputs = self
            .resolve_cell_tx_inputs_from_provider(tx, provider, block_hash)
            .map_err(|_| RuleError::TxInContextFailed(tx.id().into(), TxRuleError::MissingTxOutpoints))?;
        let resolved_tx = MutableTransaction::with_resolved_metadata(tx.clone(), resolved_inputs);
        let verifiable = resolved_tx.as_verifiable();
        self.mass_calculator
            .calc_contextual_masses(&verifiable)
            .map(|contextual_masses| contextual_masses.storage_mass)
            .ok_or_else(|| RuleError::TxInContextFailed(tx.id().into(), TxRuleError::MissingTxOutpoints))
    }

    #[cfg(feature = "vm")]
    fn accumulate_body_tx_validation(
        &self,
        tx: &spora_exec::CellTx,
        non_contextual_masses: NonContextualMasses,
        storage_mass: u64,
        tx_cycles: u64,
        accum: &mut BodyValidationAccumulators,
        vm_limits: &VmLimits,
    ) -> BlockProcessResult<()> {
        let max_block_cycles = CellConsensusParams::default().max_block_cycles;
        accum.block_cycles = accum.block_cycles.saturating_add(tx_cycles);
        if accum.block_cycles > max_block_cycles {
            return Err(RuleError::CellValidationError(format!(
                "block script cycles exceeded limit: total {}, limit {}",
                accum.block_cycles, max_block_cycles
            )));
        }

        let effective_compute_mass =
            non_contextual_masses
                .compute_mass
                .max(vm_limits.effective_size(spora_consensus_core::mass::cell_tx_estimated_serialized_size(tx) as usize, tx_cycles)
                    as u64);
        accum.compute_mass = accum.compute_mass.saturating_add(effective_compute_mass);
        accum.transient_mass = accum.transient_mass.saturating_add(non_contextual_masses.transient_mass);
        accum.storage_mass = accum.storage_mass.saturating_add(storage_mass);

        if accum.compute_mass > self.max_block_mass {
            return Err(RuleError::ExceedsComputeMassLimit(accum.compute_mass, self.max_block_mass));
        }
        if accum.transient_mass > self.max_block_mass {
            return Err(RuleError::ExceedsTransientMassLimit(accum.transient_mass, self.max_block_mass));
        }
        if accum.storage_mass > self.max_block_mass {
            return Err(RuleError::ExceedsStorageMassLimit(accum.storage_mass, self.max_block_mass));
        }

        Ok(())
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

                for dep in &tx.cell_deps {
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
                        .spend_cell(&input.previous_output)
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
        output: &spora_exec::CellOutput,
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
                provider
                    .get_cell_at_pov(&input.previous_output, pov)?
                    .ok_or_else(|| format!("missing input cell {:?}", input.previous_output))
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
                    let metadata = provider.get_cell_at_pov(&input.previous_output, pov).ok().flatten();
                    if let Some(metadata) = metadata {
                        if metadata.is_cellbase && current_daa < metadata.block_daa_score.saturating_add(maturity) {
                            return RuleError::TxInContextFailed(
                                tx_id,
                                TxRuleError::ImmatureCoinbaseSpend(
                                    input_index,
                                    TransactionOutpoint::new(input.previous_output.tx_hash, input.previous_output.index),
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
            CellValidationError::ScriptVerificationFailed(msg) | CellValidationError::ScriptFailed(msg) => {
                RuleError::TxInContextFailed(tx_id, TxRuleError::CellValidationFailed(msg))
            }
            CellValidationError::ExceededMaxCycles { total, limit } => RuleError::TxInContextFailed(
                tx_id,
                TxRuleError::CellValidationFailed(format!("script cycles exceeded limit: total {total}, limit {limit}")),
            ),
            CellValidationError::InvalidSignature => {
                RuleError::TxInContextFailed(tx_id, TxRuleError::CellValidationFailed("invalid signature".to_string()))
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
    #[cfg(feature = "vm")]
    use super::BodyValidationContextResult;
    use super::BodyValidationOverlayProvider;
    use crate::processes::{CellStateProvider, DagCellProvider};
    use crate::{config::ConfigBuilder, consensus::test_consensus::TestConsensus, errors::RuleError, params::DEVNET_PARAMS};
    #[cfg(feature = "vm")]
    use secp256k1::Keypair;
    #[cfg(feature = "vm")]
    use spora_addresses::{Address, Prefix};
    use spora_consensus_core::{
        api::ConsensusApi,
        block::{Block, TemplateBuildMode, TemplateTransactionSelector},
        cell_metadata::CellMetadata,
        coinbase::MinerData,
        config::params::MAINNET_PARAMS,
        errors::tx::TxRuleError,
        merkle::calc_hash_merkle_root_cell as calc_hash_merkle_root_with_options,
        tx::TransactionOutpoint,
    };
    #[cfg(feature = "vm")]
    use spora_consensus_core::{
        sign::sign,
        tx::{pay_to_address_lock_script, MutableTransaction},
    };
    use spora_core::assert_match;
    #[cfg(feature = "vm")]
    use spora_exec::scripts::always_success_code_hash;
    use spora_exec::{CellDep, CellInput, CellOutput, CellTx, DepType, OutPoint, Script};
    use spora_hashes::Hash;
    use std::collections::HashMap;

    fn calc_hash_merkle_root<'a>(txs: impl ExactSizeIterator<Item = &'a CellTx>) -> Hash {
        calc_hash_merkle_root_with_options(txs, false)
    }

    struct OnetimeTxSelector {
        txs: Option<Vec<CellTx>>,
    }

    impl OnetimeTxSelector {
        fn new(txs: Vec<CellTx>) -> Self {
            Self { txs: Some(txs) }
        }
    }

    impl TemplateTransactionSelector for OnetimeTxSelector {
        fn select_transactions(&mut self) -> Vec<CellTx> {
            self.txs.take().unwrap_or_default()
        }

        fn reject_selection(&mut self, _tx_id: spora_consensus_core::tx::TransactionId) {}

        fn is_successful(&self) -> bool {
            true
        }
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

    fn test_lock_script() -> Script {
        #[cfg(feature = "vm")]
        {
            return Script::new(always_success_code_hash(), 0, vec![]);
        }

        #[cfg(not(feature = "vm"))]
        {
            Script::new([0; 32], 0, vec![])
        }
    }

    fn test_miner_data() -> MinerData {
        MinerData::new(test_lock_script(), vec![])
    }

    fn build_template_block(consensus: &TestConsensus, txs: Vec<CellTx>) -> Block {
        consensus
            .build_block_template(test_miner_data(), Box::new(OnetimeTxSelector::new(txs)), TemplateBuildMode::Standard)
            .unwrap()
            .block
            .to_immutable()
    }

    fn build_spend_tx(outpoint: TransactionOutpoint, output_capacity: u64) -> CellTx {
        let lock = test_lock_script();
        CellTx::new(
            vec![CellInput::new(OutPoint::new(outpoint.tx_hash, outpoint.index), 0)],
            vec![],
            vec![CellOutput { lock, type_: None, capacity: output_capacity }],
            vec![vec![]],
            vec![],
        )
        .unwrap()
    }

    fn build_spend_tx_with_dep(outpoint: TransactionOutpoint, dep_outpoint: TransactionOutpoint, output_capacity: u64) -> CellTx {
        let lock = test_lock_script();
        CellTx::new(
            vec![CellInput::new(OutPoint::new(outpoint.tx_hash, outpoint.index), 0)],
            vec![CellDep { out_point: OutPoint::new(dep_outpoint.tx_hash, dep_outpoint.index), dep_type: DepType::Code }],
            vec![CellOutput { lock, type_: None, capacity: output_capacity }],
            vec![vec![]],
            vec![],
        )
        .unwrap()
    }

    #[cfg(feature = "vm")]
    fn metadata_from_tx_output(
        block_hash: Hash,
        block_daa_score: u64,
        is_cellbase: bool,
        tx: &CellTx,
        output_index: u32,
    ) -> CellMetadata {
        let output = &tx.outputs[output_index as usize];
        let output_data = tx.outputs_data.get(output_index as usize).map(Vec::as_slice).unwrap_or(&[]);
        CellMetadata {
            out_point: TransactionOutpoint { tx_hash: tx.id(), index: output_index },
            capacity: output.capacity,
            data_bytes: output_data.len() as u64,
            lock_hash: output.lock.hash(),
            type_hash: output.type_.as_ref().map(|script| script.hash()),
            data_hash: super::BlockBodyProcessor::compute_output_data_hash(output_data),
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
                consensus.validate_and_insert_block(block.clone().to_immutable()).virtual_state_task.await, Err(RuleError::WrongSubsidy(expected,_)) if expected == 114000000000);

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
            assert_match!(consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await, Err(RuleError::WrongSubsidy(expected,_)) if expected == 45000000000);
        }

        consensus.shutdown(wait_handles);
    }

    #[cfg(feature = "vm")]
    #[tokio::test]
    async fn validate_body_in_context_resumable_matches_direct_for_native_pubkey_block() {
        let config = ConfigBuilder::new(MAINNET_PARAMS)
            .skip_proof_of_work()
            .edit_consensus_params(|params| {
                params.coinbase_maturity = 0;
            })
            .build();
        let consensus = TestConsensus::new(&config);
        let wait_handles = consensus.init();
        let body_processor = consensus.block_body_processor();

        let keypair = Keypair::from_seckey_slice(secp256k1::SECP256K1, &[0x69; 32]).expect("valid secret key");
        let pubkey = keypair.public_key().x_only_public_key().0.serialize();
        let address = Address::new_std_single(Prefix::Testnet, &pubkey).expect("valid address");
        let lock_script = pay_to_address_lock_script(&address);
        let miner_data = MinerData::new(lock_script.clone(), vec![]);

        let warmup = consensus
            .build_block_template(miner_data.clone(), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard)
            .unwrap();
        consensus.validate_and_insert_block(warmup.block.to_immutable()).virtual_state_task.await.unwrap();

        let funding = consensus
            .build_block_template(miner_data.clone(), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard)
            .unwrap();
        let funding_coinbase = funding.block.transactions[0].clone();
        let funding_block_hash = funding.block.header.hash;
        let funding_block_daa = funding.block.header.daa_score;
        consensus.validate_and_insert_block(funding.block.to_immutable()).virtual_state_task.await.unwrap();

        let input_outpoint = OutPoint::new(funding_coinbase.id(), 0);
        let spend_capacity = funding_coinbase.outputs[0].capacity.checked_sub(1_000).expect("coinbase output should be large enough");
        let unsigned_tx = CellTx::new(
            vec![CellInput::new(input_outpoint, 0)],
            vec![],
            vec![CellOutput { lock: lock_script.clone(), type_: None, capacity: spend_capacity }],
            vec![vec![]],
            vec![vec![]],
        )
        .unwrap();
        let resolved_input = metadata_from_tx_output(funding_block_hash, funding_block_daa, true, &funding_coinbase, 0);
        let signed_tx = sign(MutableTransaction::with_resolved_metadata(unsigned_tx, vec![resolved_input]), keypair).tx;

        let template = consensus
            .build_block_template(miner_data, Box::new(OnetimeTxSelector::new(vec![signed_tx.clone()])), TemplateBuildMode::Standard)
            .expect("standard mode should accept native stdsingle candidate");
        let block = template.block.to_immutable();

        let direct_mass =
            body_processor.validate_body_in_context(&block).expect("direct body validation should succeed for native stdsingle block");
        let initial =
            body_processor.validate_body_in_context_resumable(&block, 1).expect("initial resumable body validation should succeed");
        let state = match initial {
            BodyValidationContextResult::Suspended(state) => state,
            BodyValidationContextResult::Completed(mass) => {
                panic!("expected suspension for tiny cycle budget, got completed mass {mass:?}")
            }
        };
        assert_eq!(state.current, 0, "single tx block should suspend on the first non-coinbase tx");

        let resumed = body_processor
            .resume_body_in_context_from_state(&block, &state, crate::processes::CellConsensusParams::default().max_block_cycles)
            .expect("resumed body validation should succeed");
        let resumed_mass = match resumed {
            BodyValidationContextResult::Completed(mass) => mass,
            BodyValidationContextResult::Suspended(next_state) => body_processor
                .complete_body_in_context_from_state(
                    &block,
                    &next_state,
                    crate::processes::CellConsensusParams::default().max_block_cycles,
                )
                .expect("completing body validation from resumed state"),
        };

        assert_eq!(resumed_mass, direct_mass, "resumed block validation should match direct mass accounting");

        consensus.shutdown(wait_handles);
    }

    #[tokio::test]
    async fn rejects_missing_outpoints_during_body_context_validation() {
        let config = ConfigBuilder::new(DEVNET_PARAMS).skip_proof_of_work().build();
        let consensus = TestConsensus::new(&config);
        let wait_handles = consensus.init();
        let body_processor = consensus.block_body_processor();

        let parent = build_template_block(&consensus, vec![]);
        let parent_hash = parent.hash();
        consensus.validate_and_insert_block(parent).virtual_state_task.await.unwrap();

        let missing_outpoint = TransactionOutpoint { tx_hash: [0xAA; 32], index: 0 };
        let invalid_tx = build_spend_tx(missing_outpoint, 1_000);
        let block = build_block_with_extra_transactions(&consensus, Hash::from_bytes([0xA1; 32]), vec![parent_hash], vec![invalid_tx]);

        assert_match!(
            body_processor.validate_body_in_context(&block),
            Err(RuleError::TxInContextFailed(_, TxRuleError::MissingTxOutpoints))
        );

        consensus.shutdown(wait_handles);
    }

    #[tokio::test]
    async fn rejects_missing_deps_during_body_context_validation() {
        let config = ConfigBuilder::new(DEVNET_PARAMS).skip_proof_of_work().build();
        let consensus = TestConsensus::new(&config);
        let wait_handles = consensus.init();
        let body_processor = consensus.block_body_processor();

        let warmup = build_template_block(&consensus, vec![]);
        consensus.validate_and_insert_block(warmup).virtual_state_task.await.unwrap();

        let parent = build_template_block(&consensus, vec![]);
        let parent_hash = parent.hash();
        let spendable_outpoint = TransactionOutpoint { tx_hash: parent.transactions[0].id(), index: 0 };
        consensus.validate_and_insert_block(parent).virtual_state_task.await.unwrap();

        let missing_dep_outpoint = TransactionOutpoint { tx_hash: [0xBB; 32], index: 0 };
        let invalid_tx = build_spend_tx_with_dep(spendable_outpoint, missing_dep_outpoint, 1_000);
        let block = build_block_with_extra_transactions(&consensus, Hash::from_bytes([0xB2; 32]), vec![parent_hash], vec![invalid_tx]);

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

    #[cfg(not(feature = "vm"))]
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

        let warmup = build_template_block(&consensus, vec![]);
        consensus.validate_and_insert_block(warmup).virtual_state_task.await.unwrap();

        let reward_source = build_template_block(&consensus, vec![]);
        let reward_source_coinbase = reward_source.transactions[0].clone();
        let reward_outpoint = TransactionOutpoint { tx_hash: reward_source_coinbase.id(), index: 0 };
        let reward_capacity = reward_source_coinbase.outputs[0].capacity;
        consensus.validate_and_insert_block(reward_source).virtual_state_task.await.unwrap();

        let parent_tx = build_spend_tx(reward_outpoint, reward_capacity - 1_000);
        let parent_tx_id = parent_tx.id();
        let parent_output_capacity = parent_tx.outputs[0].capacity;
        let parent = build_template_block(&consensus, vec![parent_tx]);
        consensus.validate_and_insert_block(parent).virtual_state_task.await.unwrap();

        let child_tx = build_spend_tx(TransactionOutpoint { tx_hash: parent_tx_id, index: 0 }, parent_output_capacity - 1_000);
        let child = build_template_block(&consensus, vec![child_tx]);

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
