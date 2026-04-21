// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Spora developers
//
// Cell processing context for virtual processor
// Replaces transaction-output processing logic with the pure Cell model

use super::VirtualStateProcessor;

use crate::consensus::cell_provider::{ConsensusCellProvider, OverlayCellProvider};
use crate::model::stores::{
    block_transactions::BlockTransactionsStoreReader, ghostdag::GhostdagData, statuses::StatusesStoreBatchExtensions,
};
#[cfg(feature = "vm")]
use crate::processes::cell_validator::{CellScriptVerificationState, CellScriptVerifyResult};
use crate::processes::utils::{compute_data_hash, outpoint_to_hash};
#[cfg(feature = "vm")]
use crate::processes::CellValidator;
use crate::{
    errors::RuleError,
    model::stores::headers::HeaderStoreReader,
    processes::{
        cell_validator::{cell_validation_in_context, cell_validation_in_dag, cell_validation_in_isolation, CellValidationError},
        CellConsensusParams,
    },
};
use spora_consensus_core::{
    acceptance_data::{AcceptedTxEntry, MergesetBlockAcceptanceData},
    cell_diff::{BlockCellDiff, CellDiff, CellMeta},
    cell_metadata::CellMetadata,
    coinbase::BlockRewardData,
    constants::MAX_SAU,
    errors::tx::TxRuleError,
    tx::{TransactionId, TransactionOutpoint},
    BlockHashMap, HashMapCustomHasher,
};
use spora_database::prelude::StoreResultEmptyTuple;
use spora_exec::OutPoint;
use spora_hashes::Hash;
use spora_state::{CellEntry, CellStateTree};
use spora_utils::refs::Refs;
use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

type ReplayConsensusCellProvider = ConsensusCellProvider<
    crate::model::stores::ghostdag::DbGhostdagStore,
    crate::model::stores::reachability::DbReachabilityStore,
    crate::model::stores::headers::DbHeadersStore,
    crate::model::stores::cell_diffs::DbCellDiffsStore,
    crate::model::stores::cell_roots::DbCellRootsStore,
    crate::model::stores::block_transactions::DbBlockTransactionsStore,
    crate::model::stores::statuses::DbStatusesStore,
>;

type ReplayOverlayProvider = OverlayCellProvider<ReplayConsensusCellProvider>;

/// A context for processing the Cell state of a block with respect to its selected parent.
/// This replaces the previous processing context with the pure Cell model.
pub(super) struct CellProcessingContext<'a> {
    pub ghostdag_data: Refs<'a, GhostdagData>,
    /// Cell state tree (replaces multiset_hash)
    pub cell_state_tree: CellStateTree,
    /// Cell diff for this mergeset (replaces mergeset_diff)
    pub mergeset_cell_diff: CellDiff,
    pub block_cell_diffs: Vec<BlockCellDiff>,
    pub accepted_tx_ids: Vec<TransactionId>,
    pub mergeset_acceptance_data: Vec<MergesetBlockAcceptanceData>,
    pub mergeset_rewards: BlockHashMap<BlockRewardData>,
    pub pruning_sample_from_pov: Option<Hash>,
}

impl<'a> CellProcessingContext<'a> {
    /// Create a new cell processing context
    pub fn new(ghostdag_data: Refs<'a, GhostdagData>, selected_parent_cell_tree: CellStateTree) -> Self {
        let mergeset_size = ghostdag_data.mergeset_size();
        Self {
            ghostdag_data,
            cell_state_tree: selected_parent_cell_tree,
            mergeset_cell_diff: CellDiff::default(),
            block_cell_diffs: Vec::with_capacity(mergeset_size + 1),
            accepted_tx_ids: Vec::with_capacity(1), // At least the selected parent coinbase tx
            mergeset_rewards: BlockHashMap::with_capacity(mergeset_size),
            mergeset_acceptance_data: Vec::with_capacity(mergeset_size),
            pruning_sample_from_pov: Default::default(),
        }
    }

    /// Get the current cell root
    pub fn get_cell_root(&mut self) -> Hash {
        self.cell_state_tree.root()
    }

    /// Verify that the calculated cell root matches expected
    #[allow(dead_code)]
    pub fn verify_cell_root(&mut self, expected_root: Hash) -> Result<(), String> {
        let calculated_root = self.get_cell_root();
        if calculated_root == expected_root {
            Ok(())
        } else {
            Err(format!("Cell root mismatch: expected {:?}, got {:?}", expected_root, calculated_root))
        }
    }
}

#[cfg(feature = "vm")]
impl CellProcessingContext<'static> {
    pub fn new_owned(ghostdag_data: Arc<GhostdagData>, selected_parent_cell_tree: CellStateTree) -> Self {
        Self::new(ghostdag_data.into(), selected_parent_cell_tree)
    }
}

/// Block-level execution effect containing all information needed to commit to canonical state.
/// Produced by the pure analysis phase (analyze_blue_block) without any state mutation.
#[derive(Debug, Clone)]
pub(super) struct BlockExecutionEffect {
    /// Block hash
    pub block_hash: Hash,
    /// Block DAA score
    pub block_daa_score: u64,
    /// Cell state diff (created and consumed cells)
    pub cell_diff: CellDiff,
    /// Accepted transaction cell_deps that must still be available at commit time.
    pub read_deps: BTreeSet<TransactionOutpoint>,
    /// Accepted transaction ID list
    pub accepted_tx_ids: Vec<TransactionId>,
    /// Accepted transaction entries (with in-block index)
    pub accepted_transactions: Vec<AcceptedTxEntry>,
    /// Block reward data
    pub reward_data: Option<BlockRewardData>,
    /// Script execution cycles consumed during analysis
    pub consumed_cycles: u64,
    /// Newly processed transaction IDs (for snapshot update / cross-block dedup)
    pub newly_processed_tx_ids: Vec<Hash>,
}

impl BlockExecutionEffect {
    /// Create an empty effect (used when an effect is invalidated or for red blocks).
    pub fn empty(block_hash: Hash, block_daa_score: u64) -> Self {
        Self {
            block_hash,
            block_daa_score,
            cell_diff: CellDiff::default(),
            read_deps: BTreeSet::new(),
            accepted_tx_ids: Vec::new(),
            accepted_transactions: Vec::new(),
            reward_data: None,
            consumed_cycles: 0,
            newly_processed_tx_ids: Vec::new(),
        }
    }
}

#[cfg(feature = "vm")]
#[derive(Clone)]
pub(super) struct BlueBlockExecutionState {
    current: usize,
    effect: BlockExecutionEffect,
    local_tree: CellStateTree,
    replay_ctx: ReplayValidationContext,
    tx_script_state: CellScriptVerificationState,
}

#[cfg(feature = "vm")]
impl BlueBlockExecutionState {
    fn new(
        current: usize,
        effect: BlockExecutionEffect,
        local_tree: CellStateTree,
        replay_ctx: ReplayValidationContext,
        tx_script_state: CellScriptVerificationState,
    ) -> Self {
        Self { current, effect, local_tree, replay_ctx, tx_script_state }
    }

    pub fn current_cycles(&self) -> u64 {
        self.replay_ctx.accumulated_cycles.saturating_add(self.tx_script_state.current_cycles())
    }

    pub fn next_limit_cycles(&self, step_cycles: u64, max_cycles: u64) -> (u64, bool) {
        let completed_cycles = self.replay_ctx.accumulated_cycles;
        let capped_max_cycles = max_cycles.max(self.current_cycles());
        let remaining_cycles = capped_max_cycles.saturating_sub(completed_cycles);
        let (next_tx_limit, _) = self.tx_script_state.next_limit_cycles(step_cycles, remaining_cycles);
        let next_limit = completed_cycles.saturating_add(next_tx_limit);
        if next_limit < capped_max_cycles {
            (next_limit, false)
        } else {
            (capped_max_cycles, true)
        }
    }
}

#[cfg(feature = "vm")]
pub(super) enum BlueBlockExecutionResult {
    Completed(BlockExecutionEffect),
    Suspended(BlueBlockExecutionState),
}

#[cfg(feature = "vm")]
#[derive(Clone)]
pub(super) struct CellStateCalculationState {
    blue_execution_pos: usize,
    processed_txs: HashSet<Hash>,
    replay_ctx: ReplayValidationContext,
    block_state: BlueBlockExecutionState,
}

#[cfg(feature = "vm")]
impl CellStateCalculationState {
    fn new(
        blue_execution_pos: usize,
        processed_txs: HashSet<Hash>,
        replay_ctx: ReplayValidationContext,
        block_state: BlueBlockExecutionState,
    ) -> Self {
        Self { blue_execution_pos, processed_txs, replay_ctx, block_state }
    }

    pub fn current_cycles(&self) -> u64 {
        self.block_state.current_cycles()
    }

    pub fn next_limit_cycles(&self, step_cycles: u64, max_cycles: u64) -> (u64, bool) {
        self.block_state.next_limit_cycles(step_cycles, max_cycles)
    }
}

#[cfg(feature = "vm")]
pub(super) enum CellStateCalculationResult {
    Completed,
    Suspended(CellStateCalculationState),
}

pub(super) fn exec_outpoint(outpoint: &TransactionOutpoint) -> OutPoint {
    OutPoint::new(outpoint.tx_hash, outpoint.index)
}

pub(super) fn apply_cell_diff_to_tree(tree: &mut CellStateTree, diff: &CellDiff) {
    for outpoint in diff.remove.keys() {
        let outpoint_hash = outpoint_to_hash(outpoint);
        tree.remove(&outpoint_hash);
    }

    for (outpoint, meta) in &diff.add {
        let outpoint_hash = outpoint_to_hash(outpoint);
        tree.insert_with_outpoint(outpoint_hash, exec_outpoint(outpoint), cell_meta_to_entry(meta));
    }
}

fn record_tx_read_deps(read_deps: &mut BTreeSet<TransactionOutpoint>, tx: &spora_exec::CellTx) {
    for dep in &tx.cell_deps {
        read_deps.insert(TransactionOutpoint { tx_hash: dep.out_point.tx_hash, index: dep.out_point.index });
    }
}

fn outpoint_exists_in_tree(tree: &CellStateTree, outpoint: &TransactionOutpoint) -> bool {
    let outpoint_hash = outpoint_to_hash(outpoint);
    tree.get(&outpoint_hash).is_some()
}

pub(super) fn execution_effect_conflicts_with_current_state(tree: &CellStateTree, effect: &BlockExecutionEffect) -> bool {
    effect.cell_diff.remove.keys().any(|outpoint| !outpoint_exists_in_tree(tree, outpoint))
        || effect
            .read_deps
            .iter()
            .any(|outpoint| !effect.cell_diff.add.contains_key(outpoint) && !outpoint_exists_in_tree(tree, outpoint))
}

fn ensure_execution_effect_available_for_commit(
    tree: &CellStateTree,
    effect: &BlockExecutionEffect,
    block_hash: Hash,
) -> Result<(), RuleError> {
    if execution_effect_conflicts_with_current_state(tree, effect) {
        return Err(RuleError::CellValidationError(format!(
            "blue block {block_hash} produced an execution effect whose consumed cells or cell_dep reads are unavailable at commit"
        )));
    }

    Ok(())
}

fn invalidate_effect_preserving_reward(effect: &BlockExecutionEffect, block_hash: Hash, block_daa_score: u64) -> BlockExecutionEffect {
    let mut empty = BlockExecutionEffect::empty(block_hash, block_daa_score);
    empty.reward_data = effect.reward_data.clone();
    empty
}

fn ensure_red_block_outputs_absent(tree: &CellStateTree, red_block: Hash, block_txs: &[spora_exec::CellTx]) -> Result<(), RuleError> {
    for tx in block_txs {
        for (index, _) in tx.outputs.iter().enumerate() {
            let outpoint = TransactionOutpoint { tx_hash: tx.id(), index: index as u32 };
            let outpoint_hash = outpoint_to_hash(&outpoint);
            if tree.get(&outpoint_hash).is_some() {
                return Err(RuleError::CellValidationError(format!(
                    "red block {red_block} leaked output {outpoint} into the live CellStateTree"
                )));
            }
        }
    }

    Ok(())
}

fn cell_meta_to_entry(meta: &CellMeta) -> CellEntry {
    CellEntry::new(
        meta.capacity,
        meta.data_bytes,
        Hash::from_bytes(meta.lock_hash),
        meta.type_hash.map(Hash::from_bytes),
        Hash::from_bytes(meta.data_hash),
        meta.block_daa_score,
        meta.is_cellbase,
    )
    .with_resolved_metadata(meta.lock_script.clone(), meta.type_script.clone(), meta.data.clone())
}

fn cell_entry_to_meta(out_point: &TransactionOutpoint, entry: &CellEntry) -> CellMeta {
    CellMeta {
        out_point: out_point.clone(),
        capacity: entry.capacity,
        data_bytes: entry.data_bytes,
        lock_hash: entry.lock_hash.as_bytes().try_into().expect("hash size is fixed"),
        type_hash: entry.type_hash.map(|h| h.as_bytes().try_into().expect("hash size is fixed")),
        data_hash: entry.data_hash.as_bytes().try_into().expect("hash size is fixed"),
        block_daa_score: entry.block_daa_score,
        is_cellbase: entry.is_cellbase,
        lock_script: entry.lock_script.clone(),
        type_script: entry.type_script.clone(),
        data: entry.data.clone(),
    }
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
        data_hash: if output_data.is_empty() {
            [0u8; 32]
        } else {
            let mut hasher = blake3::Hasher::new();
            hasher.update(b"spora-cell/data");
            hasher.update(output_data);
            *hasher.finalize().as_bytes()
        },
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

/// Replay validation context for the virtual processor's Cell state calculation.
///
/// # Architecture: Four-Layer Validation
///
/// This context implements **complete, independent four-layer transaction validation**
/// during virtual state replay. Each non-coinbase transaction in the mergeset is
/// validated through all four layers before its effects are applied to the state tree:
///
/// | Layer | Name       | Function                                                       |
/// |-------|------------|----------------------------------------------------------------|
/// | L1    | Isolation  | Stateless structural checks (format, size, output constraints) |
/// | L2    | Context    | Capacity conservation, Cell availability against POV snapshot  |
/// | L3    | DAG        | Cell existence, time-lock satisfaction, cellbase maturity      |
/// | L4    | Scripts    | VM-based lock/type script verification (when `vm` feature on) |
///
/// # Design Intent
///
/// The virtual processor maintains its own **point-of-view snapshot** (`snapshot_pov`)
/// which may differ from both the body processor's and the mempool's view of the DAG.
/// This intentional re-validation ensures that:
///
/// - State transitions are consistent with the virtual chain tip, not just the
///   block's original validation context.
/// - Reorgs and mergeset reorderings cannot introduce invalid state.
/// - The validation is **deliberately redundant** with body_processor and mempool
///   validation to guarantee consensus-layer soundness.
///
/// # Cycles Tracking
///
/// The `accumulated_cycles` field tracks the total VM cycles consumed by
/// transactions within the current block being processed. When the accumulated
/// cycles exceed `MAX_BLOCK_CYCLES` (from [`CellConsensusParams`]), subsequent
/// transactions in that block are pruned (skipped without applying to state).
#[derive(Clone)]
struct ReplayValidationContext {
    snapshot_pov: Hash,
    current_daa_score: u64,
    current_timestamp: u64,
    params: Arc<CellConsensusParams>,
    provider: ReplayOverlayProvider,
    /// Accumulated VM script execution cycles for the current block.
    /// Reset at the start of each block analysis.
    /// Used to enforce the per-block cycles limit (`max_block_cycles`).
    accumulated_cycles: u64,
}

/// Read-only execution snapshot for the parallel analysis phase.
///
/// Contains all state that `analyze_blue_block`
/// reads during block analysis, without holding any `&mut` references.
/// The snapshot is created from the current mutable context at a point in time
/// and can be sent to worker threads for parallel analysis.
#[derive(Clone)]
pub(super) struct ExecutionSnapshot {
    /// Immutable clone of the Cell state tree
    pub cell_state_tree: CellStateTree,
    /// Set of already-processed transaction IDs (for cross-block dedup)
    pub processed_txs: HashSet<Hash>,
    /// POV hash for replay validation
    pub snapshot_pov: Hash,
    /// Current DAA score at the time of snapshot
    pub current_daa_score: u64,
    /// Current timestamp at the time of snapshot
    pub current_timestamp: u64,
    /// Cell consensus parameters (shared, immutable)
    pub params: Arc<CellConsensusParams>,
    /// Cloned overlay provider state for replay validation
    pub provider: ReplayOverlayProvider,
}

impl ExecutionSnapshot {
    /// Create a read-only snapshot from the current mutable processing state.
    fn from_current_state(
        cell_state_tree: &CellStateTree,
        processed_txs: &HashSet<Hash>,
        replay_ctx: &ReplayValidationContext,
    ) -> Self {
        Self {
            cell_state_tree: cell_state_tree.clone(),
            processed_txs: processed_txs.clone(),
            snapshot_pov: replay_ctx.snapshot_pov,
            current_daa_score: replay_ctx.current_daa_score,
            current_timestamp: replay_ctx.current_timestamp,
            params: replay_ctx.params.clone(),
            provider: replay_ctx.provider.clone(),
        }
    }
}

impl ReplayValidationContext {
    fn seed_base_overrides_from_tree(&mut self, cell_state_tree: &CellStateTree) -> Result<(), RuleError> {
        for (outpoint, _, entry) in cell_state_tree.iter_by_outpoint() {
            if entry.lock_script.is_none() && entry.type_script.is_none() && entry.data.is_none() {
                continue;
            }

            let tx_outpoint = TransactionOutpoint::new(outpoint.tx_hash, outpoint.index);
            let mut metadata = CellMetadata::from(&cell_entry_to_meta(&tx_outpoint, entry));
            metadata.block_hash = self.snapshot_pov;
            self.provider
                .add_base_override(exec_outpoint(&tx_outpoint), metadata)
                .map_err(|err| RuleError::CellValidationError(format!("virtual replay overlay seed error: {err}")))?;
        }

        Ok(())
    }

    fn validate_tx_pre_scripts(&self, tx: &spora_exec::CellTx) -> Result<(), RuleError> {
        // L1: Isolation (stateless) — no provider needed
        cell_validation_in_isolation::validate_cell_tx_in_isolation(tx, self.params.max_cell_data_size)
            .map_err(|err| self.map_validation_error(tx, err))?;

        // L1 (cont.): Check tx serialized size
        let tx_size =
            borsh::to_vec(tx).map_err(|e| self.map_validation_error(tx, CellValidationError::InvalidFormat(e.to_string())))?.len();
        if tx_size > self.params.max_tx_size {
            return Err(self.map_validation_error(
                tx,
                CellValidationError::InvalidFormat(format!("Transaction too large: {} > {}", tx_size, self.params.max_tx_size)),
            ));
        }

        // L3: DAG existence check (takes &P reference, no clone)
        cell_validation_in_dag::validate_cell_existence(tx, self.snapshot_pov, &self.provider)
            .map_err(|err| self.map_validation_error(tx, err))?;

        // L2: Context validation — capacity conservation (takes &P reference, no clone)
        cell_validation_in_context::validate_cell_tx_in_context(tx, self.snapshot_pov, self.current_daa_score, &self.provider)
            .map_err(|err| self.map_validation_error(tx, err))?;

        // L3 (cont.): Time locks (takes &P reference, no clone)
        cell_validation_in_dag::validate_time_locks(
            tx,
            self.snapshot_pov,
            self.current_daa_score,
            self.current_timestamp,
            &self.provider,
        )
        .map_err(|err| self.map_validation_error(tx, err))?;

        // L3 (cont.): Cellbase maturity (takes &P reference, no clone)
        cell_validation_in_dag::validate_cellbase_maturity(
            tx,
            self.snapshot_pov,
            self.current_daa_score,
            self.params.cellbase_maturity,
            &self.provider,
        )
        .map_err(|err| self.map_validation_error(tx, err))?;

        Ok(())
    }

    /// Perform complete four-layer validation on a non-coinbase transaction.
    ///
    /// Returns the number of VM cycles consumed by script verification (0 when
    /// the `vm` feature is disabled).
    ///
    /// # Validation Order
    ///
    /// 1. **L1 — Isolation (stateless):** structural format checks, output constraints,
    ///    and serialized size limit via [`cell_validation_in_isolation::validate_cell_tx_in_isolation`].
    /// 2. **L2 — Context (state-dependent):** capacity conservation and Cell availability
    ///    against the POV snapshot via [`cell_validation_in_context::validate_cell_tx_in_context`].
    /// 3. **L3 — DAG (topology-dependent):** Cell existence in the DAG state, `since`
    ///    time-lock satisfaction, and cellbase maturity via `cell_validation_in_dag::*`.
    /// 4. **L4 — Scripts (VM):** lock and type script execution via [`CellValidator::verify_scripts_with_cycles`]
    ///    (only when the `vm` feature is enabled).
    ///
    /// Any failure at a layer short-circuits the remaining checks.
    fn validate_tx(&self, tx: &spora_exec::CellTx) -> Result<u64, RuleError> {
        self.validate_tx_pre_scripts(tx)?;

        // L4: VM script verification (only when vm feature enabled)
        // Returns consumed cycles; 0 when VM is disabled.
        let mut _tx_cycles: u64 = 0;
        #[cfg(feature = "vm")]
        {
            let validator = CellValidator::new(self.params.clone(), Arc::new(self.provider.clone()));
            _tx_cycles = validator
                .verify_scripts_with_cycles(tx, self.snapshot_pov, self.current_daa_score)
                .map_err(|err| self.map_validation_error(tx, err))?;
        }

        Ok(_tx_cycles)
    }

    #[cfg(feature = "vm")]
    fn validate_tx_resumable(
        &self,
        tx: &spora_exec::CellTx,
        state: Option<&CellScriptVerificationState>,
        limit_cycles: u64,
    ) -> Result<CellScriptVerifyResult, RuleError> {
        self.validate_tx_pre_scripts(tx)?;

        let validator = CellValidator::new(self.params.clone(), Arc::new(self.provider.clone()));
        let result = match state {
            Some(state) => validator.resume_scripts_from_state(tx, self.snapshot_pov, self.current_daa_score, state, limit_cycles),
            None => validator.verify_scripts_resumable(tx, self.snapshot_pov, self.current_daa_score, limit_cycles),
        };

        result.map_err(|err| self.map_validation_error(tx, err))
    }

    fn spend_cell(&mut self, out_point: &OutPoint) -> Result<(), RuleError> {
        self.provider
            .spend_cell(out_point)
            .map_err(|err| RuleError::CellValidationError(format!("virtual replay overlay error: {err}")))
    }

    fn add_cell(&mut self, metadata: CellMetadata) -> Result<(), RuleError> {
        self.provider
            .add_cell(exec_outpoint(&metadata.out_point), metadata)
            .map_err(|err| RuleError::CellValidationError(format!("virtual replay overlay error: {err}")))
    }

    fn map_validation_error(&self, tx: &spora_exec::CellTx, error: CellValidationError) -> RuleError {
        let tx_id: Hash = tx.id().into();

        match error {
            CellValidationError::CellNotFound(_)
            | CellValidationError::DepCellNotFound(_)
            | CellValidationError::CellAlreadySpent(_) => RuleError::TxInContextFailed(tx_id, TxRuleError::MissingTxOutpoints),
            CellValidationError::CapacityOverflow => RuleError::TxInContextFailed(tx_id, TxRuleError::InputAmountOverflow),
            CellValidationError::InsufficientCapacity { required, available } => {
                RuleError::TxInContextFailed(tx_id, TxRuleError::SpendTooHigh(required, available))
            }
            CellValidationError::TimeLockNotSatisfied { .. } => {
                RuleError::TxInContextFailed(tx_id, TxRuleError::SequenceLockConditionsAreNotMet)
            }
            CellValidationError::CellbaseNotMature { .. } => {
                let maturity = self.params.cellbase_maturity;

                for (input_index, input) in tx.inputs.iter().enumerate() {
                    let metadata = self.provider.get_cell_metadata(&input.previous_output).ok().flatten();
                    if let Some(metadata) = metadata {
                        if metadata.is_cellbase && self.current_daa_score < metadata.block_daa_score.saturating_add(maturity) {
                            return RuleError::TxInContextFailed(
                                tx_id,
                                TxRuleError::ImmatureCoinbaseSpend(
                                    input_index,
                                    TransactionOutpoint::new(input.previous_output.tx_hash, input.previous_output.index),
                                    metadata.block_daa_score,
                                    self.current_daa_score,
                                    maturity,
                                ),
                            );
                        }
                    }
                }

                RuleError::TxInContextFailed(tx_id, TxRuleError::MissingTxOutpoints)
            }
            CellValidationError::InvalidFormat(msg)
                if msg.contains("lookup error") || msg.contains("Status lookup error") || msg.contains("unexpected POV") =>
            {
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
            other => RuleError::CellValidationError(format!("Context validation failed for tx {:?}: {other}", tx_id)),
        }
    }
}

impl VirtualStateProcessor {
    fn build_replay_validation_context(
        &self,
        snapshot_pov: Hash,
        base_pov: Hash,
        current_daa_score: u64,
        current_timestamp: u64,
    ) -> ReplayValidationContext {
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
        ReplayValidationContext {
            snapshot_pov,
            current_daa_score,
            current_timestamp,
            params: Arc::new(CellConsensusParams { cellbase_maturity: self.coinbase_maturity, ..CellConsensusParams::default() }),
            provider: OverlayCellProvider::new(base_provider, snapshot_pov, base_pov),
            accumulated_cycles: 0,
        }
    }

    fn build_block_reward_data(&self, block_txs: &[spora_exec::CellTx], block_daa_score: u64) -> Option<BlockRewardData> {
        if block_txs.is_empty() {
            return None;
        }
        let fallback = || {
            BlockRewardData::new(
                self.coinbase_manager.calc_block_subsidy(block_daa_score),
                0,
                spora_exec::Script::new([0; 32], 0, vec![]),
            )
        };

        Some(
            block_txs
                .first()
                .and_then(|tx| tx.payload())
                .and_then(|payload| self.coinbase_manager.deserialize_coinbase_payload(payload).ok())
                .map(|coinbase_data| BlockRewardData::new(coinbase_data.subsidy, 0, coinbase_data.miner_data.lock_script.clone()))
                .unwrap_or_else(fallback),
        )
    }

    // ────────────────────────────────────────────────────────────────────────
    // Pure analysis functions (effect-only, no shared state mutation)
    // ────────────────────────────────────────────────────────────────────────

    /// Analyze the selected parent's coinbase transaction and produce a
    /// [`BlockExecutionEffect`] that captures all state changes without
    /// mutating the snapshot.  The caller is responsible for committing the
    /// effect via [`commit_execution_effect`].
    fn analyze_selected_parent_coinbase(
        &self,
        snapshot: &ExecutionSnapshot,
        selected_parent: Hash,
        selected_parent_txs: &[spora_exec::CellTx],
        selected_parent_daa_score: u64,
    ) -> Result<BlockExecutionEffect, RuleError> {
        let mut effect = BlockExecutionEffect {
            block_hash: selected_parent,
            block_daa_score: selected_parent_daa_score,
            cell_diff: CellDiff::default(),
            read_deps: BTreeSet::new(),
            accepted_tx_ids: Vec::new(),
            accepted_transactions: Vec::new(),
            reward_data: self.build_block_reward_data(selected_parent_txs, selected_parent_daa_score),
            consumed_cycles: 0,
            newly_processed_tx_ids: Vec::new(),
        };

        if selected_parent_txs.is_empty() {
            return Ok(effect);
        }

        let coinbase_id = selected_parent_txs[0].id();
        effect.accepted_tx_ids.push(coinbase_id.into());
        effect.newly_processed_tx_ids.push(coinbase_id.into());

        // Local mutable clone of the tree for duplicate-output detection
        // (we need to check against the snapshot tree but never write back).
        let local_tree = &snapshot.cell_state_tree;

        for (index, output) in selected_parent_txs[0].outputs.iter().enumerate() {
            let outpoint = TransactionOutpoint { tx_hash: coinbase_id, index: index as u32 };
            let outpoint_hash = outpoint_to_hash(&outpoint);

            // Duplicate check: against the snapshot tree AND the accumulated diff
            if local_tree.get(&outpoint_hash).is_some() || effect.cell_diff.add.contains_key(&outpoint) {
                return Err(RuleError::CellValidationError(format!(
                    "selected parent coinbase tried to create duplicate outpoint {outpoint}"
                )));
            }

            let output_data = selected_parent_txs[0].outputs_data.get(index).map(|d| d.as_slice()).unwrap_or(&[]);
            let cell_meta = CellMeta {
                out_point: outpoint.clone(),
                capacity: output.capacity,
                data_bytes: output_data.len() as u64,
                lock_hash: output.lock.hash(),
                type_hash: output.type_.as_ref().map(|ts| ts.hash()),
                data_hash: self.compute_data_hash(output_data),
                block_daa_score: selected_parent_daa_score,
                is_cellbase: true,
                lock_script: Some(output.lock.clone()),
                type_script: output.type_.clone(),
                data: Some(output_data.to_vec()),
            };

            effect.cell_diff.add_cell(outpoint, cell_meta);
        }

        Ok(effect)
    }

    /// Analyze a single blue block's transactions and produce a
    /// [`BlockExecutionEffect`] that captures **all** state changes (cell
    /// creates / consumes, acceptance bookkeeping, cycles) without mutating
    /// any shared state.
    ///
    /// Internally the function creates **local mutable copies** of the cell
    /// state tree and the replay overlay so that input consumption / output
    /// creation can be tracked for validation purposes, but these copies are
    /// discarded when the function returns — only the `BlockExecutionEffect`
    /// survives.
    fn analyze_blue_block(
        &self,
        snapshot: &ExecutionSnapshot,
        blue_block: Hash,
        block_txs: &[spora_exec::CellTx],
        blue_block_daa_score: u64,
    ) -> Result<BlockExecutionEffect, RuleError> {
        let mut effect = BlockExecutionEffect {
            block_hash: blue_block,
            block_daa_score: blue_block_daa_score,
            cell_diff: CellDiff::default(),
            read_deps: BTreeSet::new(),
            accepted_tx_ids: Vec::new(),
            accepted_transactions: Vec::with_capacity(block_txs.len()),
            reward_data: self.build_block_reward_data(block_txs, blue_block_daa_score),
            consumed_cycles: 0,
            newly_processed_tx_ids: Vec::new(),
        };

        // ── Local mutable copies (discarded at the end) ──────────────────
        let mut local_tree = snapshot.cell_state_tree.clone();
        let mut local_replay = ReplayValidationContext {
            snapshot_pov: snapshot.snapshot_pov,
            current_daa_score: snapshot.current_daa_score,
            current_timestamp: snapshot.current_timestamp,
            params: snapshot.params.clone(),
            provider: snapshot.provider.clone(),
            accumulated_cycles: 0, // reset per block
        };
        let max_block_cycles = local_replay.params.max_block_cycles;

        for (tx_index, tx) in block_txs.iter().enumerate() {
            let tx_id = tx.id();
            let is_coinbase = tx_index == 0 && tx.is_coinbase();

            // Skip duplicates already present in the snapshot
            if snapshot.processed_txs.contains(&tx_id.into()) {
                continue;
            }

            effect.newly_processed_tx_ids.push(tx_id.into());
            effect.accepted_tx_ids.push(tx_id.into());
            effect.accepted_transactions.push(AcceptedTxEntry { transaction_id: tx_id.into(), index_within_block: tx_index as u32 });

            // ── Four-layer validation (non-coinbase) ─────────────────
            if !is_coinbase {
                let tx_cycles = local_replay.validate_tx(tx)?;

                local_replay.accumulated_cycles = local_replay.accumulated_cycles.saturating_add(tx_cycles);
                if local_replay.accumulated_cycles > max_block_cycles {
                    // Undo acceptance bookkeeping for the tx that exceeded the limit
                    effect.newly_processed_tx_ids.pop();
                    effect.accepted_tx_ids.pop();
                    effect.accepted_transactions.pop();
                    break;
                }
            }

            record_tx_read_deps(&mut effect.read_deps, tx);

            // ── Consume inputs ───────────────────────────────────────
            let mut input_capacity = 0u64;
            for input in &tx.inputs {
                let outpoint = TransactionOutpoint { tx_hash: input.previous_output.tx_hash, index: input.previous_output.index };
                let outpoint_hash = outpoint_to_hash(&outpoint);

                if effect.cell_diff.remove.contains_key(&outpoint) {
                    return Err(RuleError::DoubleSpendInSameBlock(outpoint));
                }

                let removed_entry = local_tree
                    .remove(&outpoint_hash)
                    .ok_or_else(|| RuleError::TxInContextFailed(tx_id.into(), TxRuleError::MissingTxOutpoints))?;
                let removed_meta = cell_entry_to_meta(&outpoint, &removed_entry);
                input_capacity = input_capacity
                    .checked_add(removed_meta.capacity)
                    .ok_or_else(|| RuleError::TxInContextFailed(tx_id.into(), TxRuleError::InputAmountOverflow))?;
                if input_capacity > MAX_SAU {
                    return Err(RuleError::TxInContextFailed(tx_id.into(), TxRuleError::InputAmountTooHigh));
                }

                // Mirror the original diff bookkeeping:
                // If the consumed cell was created earlier in this same block,
                // simply remove it from the `add` set; otherwise record it in
                // the `remove` set.
                if effect.cell_diff.add.remove(&removed_meta.out_point).is_none() {
                    if effect.cell_diff.remove.insert(removed_meta.out_point.clone(), removed_meta).is_some() {
                        return Err(RuleError::DoubleSpendInSameBlock(outpoint));
                    }
                }
                local_replay.spend_cell(&input.previous_output)?;
            }

            // ── Create outputs ───────────────────────────────────────
            let mut output_capacity = 0u64;
            for (index, output) in tx.outputs.iter().enumerate() {
                output_capacity = output_capacity
                    .checked_add(output.capacity)
                    .ok_or_else(|| RuleError::TxInContextFailed(tx_id.into(), TxRuleError::OutputsValueOverflow))?;
                if output_capacity > MAX_SAU {
                    return Err(RuleError::TxInContextFailed(tx_id.into(), TxRuleError::TotalTxOutTooHigh));
                }

                let outpoint = TransactionOutpoint { tx_hash: tx_id, index: index as u32 };
                let outpoint_hash = outpoint_to_hash(&outpoint);

                if local_tree.get(&outpoint_hash).is_some()
                    || effect.cell_diff.add.contains_key(&outpoint)
                    || effect.cell_diff.remove.contains_key(&outpoint)
                {
                    return Err(RuleError::CellValidationError(format!(
                        "transaction {} tried to create duplicate outpoint {outpoint}",
                        Hash::from_bytes(outpoint.tx_hash),
                    )));
                }

                let type_hash = output.type_.as_ref().map(|ts| ts.hash());
                let output_data = tx.outputs_data.get(index).map(|d| d.as_slice()).unwrap_or(&[]);
                let data_hash = self.compute_data_hash(output_data);

                let cell_meta = CellMeta {
                    out_point: outpoint.clone(),
                    capacity: output.capacity,
                    data_bytes: output_data.len() as u64,
                    lock_hash: output.lock.hash(),
                    type_hash,
                    data_hash,
                    block_daa_score: blue_block_daa_score,
                    is_cellbase: tx_index == 0 && tx.is_coinbase(),
                    lock_script: Some(output.lock.clone()),
                    type_script: output.type_.clone(),
                    data: Some(output_data.to_vec()),
                };

                // Update local tree so subsequent txs in the same block can
                // reference this output.
                local_tree.insert_with_outpoint(outpoint_hash, exec_outpoint(&outpoint), cell_meta_to_entry(&cell_meta));
                effect.cell_diff.add_cell(outpoint, cell_meta);
                local_replay.add_cell(cell_metadata_from_output(
                    blue_block,
                    blue_block_daa_score,
                    tx_index == 0 && tx.is_coinbase(),
                    tx_id,
                    index as u32,
                    output,
                    output_data,
                ))?;
            }
        }

        // Record the total cycles consumed in this block.
        effect.consumed_cycles = local_replay.accumulated_cycles;

        Ok(effect)
    }

    #[cfg(feature = "vm")]
    fn analyze_blue_block_resumable(
        &self,
        snapshot: &ExecutionSnapshot,
        blue_block: Hash,
        block_txs: &[spora_exec::CellTx],
        blue_block_daa_score: u64,
        limit_cycles: u64,
    ) -> Result<BlueBlockExecutionResult, RuleError> {
        self.analyze_blue_block_chunk(snapshot, blue_block, block_txs, blue_block_daa_score, None, limit_cycles)
    }

    #[cfg(feature = "vm")]
    fn resume_analyze_blue_block_from_state(
        &self,
        snapshot: &ExecutionSnapshot,
        blue_block: Hash,
        block_txs: &[spora_exec::CellTx],
        blue_block_daa_score: u64,
        state: &BlueBlockExecutionState,
        limit_cycles: u64,
    ) -> Result<BlueBlockExecutionResult, RuleError> {
        self.analyze_blue_block_chunk(snapshot, blue_block, block_txs, blue_block_daa_score, Some(state), limit_cycles)
    }

    #[cfg(all(feature = "vm", test))]
    fn complete_blue_block_from_state(
        &self,
        snapshot: &ExecutionSnapshot,
        blue_block: Hash,
        block_txs: &[spora_exec::CellTx],
        blue_block_daa_score: u64,
        state: &BlueBlockExecutionState,
        max_cycles: u64,
    ) -> Result<BlockExecutionEffect, RuleError> {
        match self.resume_analyze_blue_block_from_state(snapshot, blue_block, block_txs, blue_block_daa_score, state, max_cycles)? {
            BlueBlockExecutionResult::Completed(effect) => Ok(effect),
            BlueBlockExecutionResult::Suspended(next_state) => Err(RuleError::CellValidationError(format!(
                "blue block analysis suspended before completion: block {}, tx index {}, total cycles {}, limit {}",
                blue_block,
                next_state.current,
                next_state.current_cycles(),
                max_cycles
            ))),
        }
    }

    #[cfg(feature = "vm")]
    fn analyze_blue_block_chunk(
        &self,
        snapshot: &ExecutionSnapshot,
        blue_block: Hash,
        block_txs: &[spora_exec::CellTx],
        blue_block_daa_score: u64,
        state: Option<&BlueBlockExecutionState>,
        limit_cycles: u64,
    ) -> Result<BlueBlockExecutionResult, RuleError> {
        let (mut effect, mut local_tree, mut local_replay, start_index) = if let Some(state) = state {
            if state.current > block_txs.len() {
                return Err(RuleError::CellValidationError(format!(
                    "resumable blue-block analysis state out of range: current {}, tx count {}",
                    state.current,
                    block_txs.len()
                )));
            }
            if state.current_cycles() > limit_cycles {
                return Err(RuleError::CellValidationError(format!(
                    "blue block analysis cycles exceeded limit while resuming: block {}, total {}, limit {}",
                    blue_block,
                    state.current_cycles(),
                    limit_cycles
                )));
            }

            (state.effect.clone(), state.local_tree.clone(), state.replay_ctx.clone(), state.current)
        } else {
            (
                BlockExecutionEffect {
                    block_hash: blue_block,
                    block_daa_score: blue_block_daa_score,
                    cell_diff: CellDiff::default(),
                    read_deps: BTreeSet::new(),
                    accepted_tx_ids: Vec::new(),
                    accepted_transactions: Vec::with_capacity(block_txs.len()),
                    reward_data: self.build_block_reward_data(block_txs, blue_block_daa_score),
                    consumed_cycles: 0,
                    newly_processed_tx_ids: Vec::new(),
                },
                snapshot.cell_state_tree.clone(),
                ReplayValidationContext {
                    snapshot_pov: snapshot.snapshot_pov,
                    current_daa_score: snapshot.current_daa_score,
                    current_timestamp: snapshot.current_timestamp,
                    params: snapshot.params.clone(),
                    provider: snapshot.provider.clone(),
                    accumulated_cycles: 0,
                },
                0,
            )
        };
        let max_block_cycles = local_replay.params.max_block_cycles;

        for tx_index in start_index..block_txs.len() {
            let tx = &block_txs[tx_index];
            let tx_id = tx.id();
            let is_coinbase = tx_index == 0 && tx.is_coinbase();

            if snapshot.processed_txs.contains(&tx_id.into()) {
                continue;
            }

            if !is_coinbase {
                let remaining_cycles = limit_cycles.saturating_sub(local_replay.accumulated_cycles);
                let current_tx_state = state.filter(|state| state.current == tx_index).map(|state| &state.tx_script_state);
                match local_replay.validate_tx_resumable(tx, current_tx_state, remaining_cycles)? {
                    CellScriptVerifyResult::Completed(tx_cycles) => {
                        local_replay.accumulated_cycles = local_replay.accumulated_cycles.saturating_add(tx_cycles);
                        if local_replay.accumulated_cycles > max_block_cycles {
                            break;
                        }
                    }
                    CellScriptVerifyResult::Suspended(tx_script_state) => {
                        return Ok(BlueBlockExecutionResult::Suspended(BlueBlockExecutionState::new(
                            tx_index,
                            effect,
                            local_tree,
                            local_replay,
                            tx_script_state,
                        )));
                    }
                }
            }

            effect.newly_processed_tx_ids.push(tx_id.into());
            effect.accepted_tx_ids.push(tx_id.into());
            effect.accepted_transactions.push(AcceptedTxEntry { transaction_id: tx_id.into(), index_within_block: tx_index as u32 });
            record_tx_read_deps(&mut effect.read_deps, tx);

            let mut input_capacity = 0u64;
            for input in &tx.inputs {
                let outpoint = TransactionOutpoint { tx_hash: input.previous_output.tx_hash, index: input.previous_output.index };
                let outpoint_hash = outpoint_to_hash(&outpoint);

                if effect.cell_diff.remove.contains_key(&outpoint) {
                    return Err(RuleError::DoubleSpendInSameBlock(outpoint));
                }

                let removed_entry = local_tree
                    .remove(&outpoint_hash)
                    .ok_or_else(|| RuleError::TxInContextFailed(tx_id.into(), TxRuleError::MissingTxOutpoints))?;
                let removed_meta = cell_entry_to_meta(&outpoint, &removed_entry);
                input_capacity = input_capacity
                    .checked_add(removed_meta.capacity)
                    .ok_or_else(|| RuleError::TxInContextFailed(tx_id.into(), TxRuleError::InputAmountOverflow))?;
                if input_capacity > MAX_SAU {
                    return Err(RuleError::TxInContextFailed(tx_id.into(), TxRuleError::InputAmountTooHigh));
                }

                if effect.cell_diff.add.remove(&removed_meta.out_point).is_none() {
                    if effect.cell_diff.remove.insert(removed_meta.out_point.clone(), removed_meta).is_some() {
                        return Err(RuleError::DoubleSpendInSameBlock(outpoint));
                    }
                }
                local_replay.spend_cell(&input.previous_output)?;
            }

            let mut output_capacity = 0u64;
            for (index, output) in tx.outputs.iter().enumerate() {
                output_capacity = output_capacity
                    .checked_add(output.capacity)
                    .ok_or_else(|| RuleError::TxInContextFailed(tx_id.into(), TxRuleError::OutputsValueOverflow))?;
                if output_capacity > MAX_SAU {
                    return Err(RuleError::TxInContextFailed(tx_id.into(), TxRuleError::TotalTxOutTooHigh));
                }

                let outpoint = TransactionOutpoint { tx_hash: tx_id, index: index as u32 };
                let outpoint_hash = outpoint_to_hash(&outpoint);

                if local_tree.get(&outpoint_hash).is_some()
                    || effect.cell_diff.add.contains_key(&outpoint)
                    || effect.cell_diff.remove.contains_key(&outpoint)
                {
                    return Err(RuleError::CellValidationError(format!(
                        "transaction {} tried to create duplicate outpoint {outpoint}",
                        Hash::from_bytes(outpoint.tx_hash),
                    )));
                }

                let type_hash = output.type_.as_ref().map(|ts| ts.hash());
                let output_data = tx.outputs_data.get(index).map(|d| d.as_slice()).unwrap_or(&[]);
                let data_hash = self.compute_data_hash(output_data);

                let cell_meta = CellMeta {
                    out_point: outpoint.clone(),
                    capacity: output.capacity,
                    data_bytes: output_data.len() as u64,
                    lock_hash: output.lock.hash(),
                    type_hash,
                    data_hash,
                    block_daa_score: blue_block_daa_score,
                    is_cellbase: tx_index == 0 && tx.is_coinbase(),
                    lock_script: Some(output.lock.clone()),
                    type_script: output.type_.clone(),
                    data: Some(output_data.to_vec()),
                };

                local_tree.insert_with_outpoint(outpoint_hash, exec_outpoint(&outpoint), cell_meta_to_entry(&cell_meta));
                effect.cell_diff.add_cell(outpoint, cell_meta);
                local_replay.add_cell(cell_metadata_from_output(
                    blue_block,
                    blue_block_daa_score,
                    tx_index == 0 && tx.is_coinbase(),
                    tx_id,
                    index as u32,
                    output,
                    output_data,
                ))?;
            }
        }

        effect.consumed_cycles = local_replay.accumulated_cycles;
        Ok(BlueBlockExecutionResult::Completed(effect))
    }

    /// Commit a [`BlockExecutionEffect`] produced by the pure analysis phase
    /// into the mutable [`CellProcessingContext`].
    ///
    /// This is the **only** place where shared mutable state is updated after
    /// the analysis functions run.  The separation ensures that analysis can
    /// be parallelised in the future while commits remain strictly sequential.
    fn commit_execution_effect(&self, ctx: &mut CellProcessingContext, effect: BlockExecutionEffect) -> Result<(), RuleError> {
        // 1. Apply cell diff to the canonical state tree
        apply_cell_diff_to_tree(&mut ctx.cell_state_tree, &effect.cell_diff);

        // 2. Record per-block cell diff (if non-empty)
        if !effect.cell_diff.is_empty() {
            ctx.block_cell_diffs.push(BlockCellDiff::new(effect.block_hash, effect.block_daa_score, effect.cell_diff.clone()));
        }

        // 3. Merge acceptance data
        if !effect.accepted_transactions.is_empty() {
            ctx.mergeset_acceptance_data.push(MergesetBlockAcceptanceData {
                block_hash: effect.block_hash,
                accepted_transactions: effect.accepted_transactions,
            });
        }

        // 4. Extend accepted tx ids
        ctx.accepted_tx_ids.extend(effect.accepted_tx_ids);

        // 5. Record reward data
        if let Some(reward_data) = effect.reward_data {
            ctx.mergeset_rewards.entry(effect.block_hash).or_insert(reward_data);
        }

        // 6. Merge cell diff into the mergeset-level diff
        ctx.mergeset_cell_diff.with_diff_in_place(&effect.cell_diff).map_err(RuleError::CellValidationError)
    }

    // ────────────────────────────────────────────────────────────────────────
    // End of pure analysis functions
    // ────────────────────────────────────────────────────────────────────────

    /// Apply the net cell-diff from a [`BlockExecutionEffect`] to the
    /// [`ReplayValidationContext`]'s overlay provider so that subsequent
    /// blocks can validate against the updated state.
    ///
    /// For each consumed cell (in `cell_diff.remove`), we call `spend_cell`
    /// on the overlay.  For each created cell (in `cell_diff.add`), we look
    /// up the originating transaction in `block_txs` to reconstruct the full
    /// [`CellMetadata`] needed by the overlay.
    fn apply_effect_to_overlay(
        &self,
        replay_ctx: &mut ReplayValidationContext,
        effect: &BlockExecutionEffect,
        block_hash: Hash,
        block_txs: &[spora_exec::CellTx],
    ) -> Result<(), RuleError> {
        // Spend consumed cells
        for outpoint in effect.cell_diff.remove.keys() {
            let exec_op = exec_outpoint(outpoint);
            replay_ctx.spend_cell(&exec_op)?;
        }

        // Add created cells — reconstruct full CellMetadata from block transactions
        for (outpoint, cell_meta) in &effect.cell_diff.add {
            let tx = block_txs
                .iter()
                .find(|t| t.id() == outpoint.tx_hash)
                .expect("created cell must originate from a transaction in the same block");
            let output = &tx.outputs[outpoint.index as usize];
            let output_data = tx.outputs_data.get(outpoint.index as usize).map(|d| d.as_slice()).unwrap_or(&[]);
            let is_coinbase = cell_meta.is_cellbase;
            let metadata = cell_metadata_from_output(
                block_hash,
                cell_meta.block_daa_score,
                is_coinbase,
                outpoint.tx_hash,
                outpoint.index,
                output,
                output_data,
            );
            replay_ctx.add_cell(metadata)?;
        }

        Ok(())
    }

    #[cfg(feature = "vm")]
    fn process_selected_parent_coinbase(
        &self,
        ctx: &mut CellProcessingContext,
        processed_txs: &mut HashSet<Hash>,
        replay_validation: &mut ReplayValidationContext,
    ) -> Result<(), RuleError> {
        let selected_parent = ctx.ghostdag_data.selected_parent;
        let selected_parent_txs = self.block_transactions_store.get(selected_parent).unwrap();
        let selected_parent_daa_score = self.headers_store.get_daa_score(selected_parent).expect("selected parent header must exist");

        let snapshot = ExecutionSnapshot::from_current_state(&ctx.cell_state_tree, processed_txs, replay_validation);
        let sp_effect = self.analyze_selected_parent_coinbase(
            &snapshot,
            selected_parent,
            selected_parent_txs.as_slice(),
            selected_parent_daa_score,
        )?;

        for tx_id in &sp_effect.newly_processed_tx_ids {
            processed_txs.insert(*tx_id);
        }

        self.apply_effect_to_overlay(replay_validation, &sp_effect, selected_parent, selected_parent_txs.as_slice())?;
        self.commit_execution_effect(ctx, sp_effect)?;
        Ok(())
    }

    #[cfg(feature = "vm")]
    fn process_red_blocks(&self, ctx: &mut CellProcessingContext) -> Result<(), RuleError> {
        let mergeset_reds = ctx.ghostdag_data.mergeset_reds.iter().copied().collect::<Vec<_>>();
        for red_block in mergeset_reds {
            let block_txs = self.block_transactions_store.get(red_block).unwrap();
            let red_block_daa_score = self.headers_store.get_daa_score(red_block).expect("red block header must exist");
            ensure_red_block_outputs_absent(&ctx.cell_state_tree, red_block, block_txs.as_slice())?;

            ctx.mergeset_acceptance_data
                .push(MergesetBlockAcceptanceData { block_hash: red_block, accepted_transactions: Vec::new() });

            let mut red_effect = BlockExecutionEffect::empty(red_block, red_block_daa_score);
            red_effect.reward_data = self.build_block_reward_data(block_txs.as_slice(), red_block_daa_score);
            self.commit_execution_effect(ctx, red_effect)?;
        }

        Ok(())
    }

    #[cfg(feature = "vm")]
    fn calculate_cell_state_blue_blocks_resumable(
        &self,
        ctx: &mut CellProcessingContext,
        mut processed_txs: HashSet<Hash>,
        mut replay_validation: ReplayValidationContext,
        start_blue_execution_pos: usize,
        current_block_state: Option<&BlueBlockExecutionState>,
        limit_cycles: u64,
    ) -> Result<CellStateCalculationResult, RuleError> {
        use super::access_summary::BlockAccessSummary;
        use super::execution_dag::ExecutionDAG;

        let mergeset_blues = ctx.ghostdag_data.mergeset_blues.iter().copied().collect::<Vec<_>>();
        let mut summaries = Vec::with_capacity(mergeset_blues.len());

        for blue_block in &mergeset_blues {
            let block_txs = self.block_transactions_store.get(*blue_block).unwrap();
            let summary = BlockAccessSummary::try_from_block_txs_with_cellscript_scheduler(*blue_block, block_txs.as_slice())
                .map_err(|err| RuleError::CellValidationError(err.to_string()))?;
            summaries.push(summary);
        }

        let dag = ExecutionDAG::build(&summaries);
        let execution_order = dag.layers.iter().flat_map(|layer| layer.iter().copied()).collect::<Vec<_>>();

        // Resumable execution keeps a single suspension cursor, so it uses the
        // same canonical DAG order and commit validation as MPE but executes one
        // block at a time.
        for execution_pos in start_blue_execution_pos..execution_order.len() {
            let blue_block_index = execution_order[execution_pos];
            let blue_block = mergeset_blues[blue_block_index];
            let block_txs = self.block_transactions_store.get(blue_block).unwrap();
            let blue_block_daa_score = self.headers_store.get_daa_score(blue_block).expect("blue block header must exist");
            let snapshot = ExecutionSnapshot::from_current_state(&ctx.cell_state_tree, &processed_txs, &replay_validation);

            let result = if execution_pos == start_blue_execution_pos {
                match current_block_state {
                    Some(block_state) => self.resume_analyze_blue_block_from_state(
                        &snapshot,
                        blue_block,
                        block_txs.as_slice(),
                        blue_block_daa_score,
                        block_state,
                        limit_cycles,
                    )?,
                    None => self.analyze_blue_block_resumable(
                        &snapshot,
                        blue_block,
                        block_txs.as_slice(),
                        blue_block_daa_score,
                        limit_cycles,
                    )?,
                }
            } else {
                self.analyze_blue_block_resumable(&snapshot, blue_block, block_txs.as_slice(), blue_block_daa_score, limit_cycles)?
            };

            let effect = match result {
                BlueBlockExecutionResult::Completed(effect) => effect,
                BlueBlockExecutionResult::Suspended(block_state) => {
                    return Ok(CellStateCalculationResult::Suspended(CellStateCalculationState::new(
                        execution_pos,
                        processed_txs,
                        replay_validation,
                        block_state,
                    )));
                }
            };

            ensure_execution_effect_available_for_commit(&ctx.cell_state_tree, &effect, blue_block)?;
            let final_effect = effect;

            for tx_id in &final_effect.newly_processed_tx_ids {
                processed_txs.insert(*tx_id);
            }

            self.apply_effect_to_overlay(&mut replay_validation, &final_effect, blue_block, block_txs.as_slice())?;
            replay_validation.accumulated_cycles = final_effect.consumed_cycles;
            self.commit_execution_effect(ctx, final_effect)?;
        }

        self.process_red_blocks(ctx)?;
        Ok(CellStateCalculationResult::Completed)
    }

    #[cfg(feature = "vm")]
    pub(super) fn calculate_cell_state_resumable(
        &self,
        ctx: &mut CellProcessingContext,
        pov_daa_score: u64,
        snapshot_pov: Hash,
        current_timestamp: u64,
        limit_cycles: u64,
    ) -> Result<CellStateCalculationResult, RuleError> {
        let mut processed_txs = HashSet::new();
        let mut replay_validation =
            self.build_replay_validation_context(snapshot_pov, ctx.ghostdag_data.selected_parent, pov_daa_score, current_timestamp);
        replay_validation.seed_base_overrides_from_tree(&ctx.cell_state_tree)?;

        self.process_selected_parent_coinbase(ctx, &mut processed_txs, &mut replay_validation)?;
        self.calculate_cell_state_blue_blocks_resumable(ctx, processed_txs, replay_validation, 0, None, limit_cycles)
    }

    #[cfg(feature = "vm")]
    pub(super) fn resume_calculate_cell_state_from_state(
        &self,
        ctx: &mut CellProcessingContext,
        state: &CellStateCalculationState,
        limit_cycles: u64,
    ) -> Result<CellStateCalculationResult, RuleError> {
        self.calculate_cell_state_blue_blocks_resumable(
            ctx,
            state.processed_txs.clone(),
            state.replay_ctx.clone(),
            state.blue_execution_pos,
            Some(&state.block_state),
            limit_cycles,
        )
    }

    #[cfg(feature = "vm")]
    pub(super) fn complete_calculate_cell_state_from_state(
        &self,
        ctx: &mut CellProcessingContext,
        state: &CellStateCalculationState,
        max_cycles: u64,
    ) -> Result<(), RuleError> {
        match self.resume_calculate_cell_state_from_state(ctx, state, max_cycles)? {
            CellStateCalculationResult::Completed => Ok(()),
            CellStateCalculationResult::Suspended(next_state) => Err(RuleError::CellValidationError(format!(
                "cell state calculation suspended before completion: blue execution position {}, total cycles {}, limit {}",
                next_state.blue_execution_pos,
                next_state.current_cycles(),
                max_cycles
            ))),
        }
    }

    /// Calculate the Cell state for a block using the **layer-parallel
    /// analyze → sequential commit** execution model.
    ///
    /// # Design Intent
    ///
    /// Each block in the mergeset is processed in two distinct phases:
    ///
    /// 1. **Analyze** — read from an immutable [`ExecutionSnapshot`] and
    ///    produce a [`BlockExecutionEffect`] that captures all state changes
    ///    (cell creates/consumes, acceptance data, cycles, reward) without
    ///    mutating any shared state.
    /// 2. **Commit** — apply the effect to the mutable
    ///    [`CellProcessingContext`] via [`commit_execution_effect`], then
    ///    update the snapshot so the next block's analysis sees the latest
    ///    canonical state.
    ///
    /// For mergeset blues, this implements **MPE parallelization**:
    /// - Statically extract [`BlockAccessSummary`] from each blue block's
    ///   raw transactions.
    /// - Build an [`ExecutionDAG`] that groups independent blocks into layers.
    /// - For each layer: freeze a shared snapshot, analyze all blocks in
    ///   the layer **in parallel** via `rayon::par_iter()`, then commit
    ///   effects **sequentially** in GhostDAG canonical order.
    /// - Before each commit, run conflict detection: if a consumed cell
    ///   has been removed by a preceding same-layer commit, the effect
    ///   is invalidated per MPE_PROTOCOL_SEMANTICS rule 3.
    ///
    /// # GHOSTDAG-aware Process
    /// 1. Process selected parent coinbase (analyze → commit)
    /// 2. Process mergeset blues via layer-parallel model
    /// 3. Process red blocks — record rewards only, reject all transactions
    pub(super) fn calculate_cell_state(
        &self,
        ctx: &mut CellProcessingContext,
        pov_daa_score: u64,
        snapshot_pov: Hash,
        current_timestamp: u64,
    ) -> Result<(), RuleError> {
        // Track processed transactions to avoid duplicates across mergeset blocks
        let mut processed_txs: HashSet<Hash> = HashSet::new();

        let selected_parent = ctx.ghostdag_data.selected_parent;

        // Build the replay validation context (mutable — updated after each commit
        // so that subsequent blocks validate against the latest overlay state).
        let mut replay_validation =
            self.build_replay_validation_context(snapshot_pov, selected_parent, pov_daa_score, current_timestamp);
        replay_validation.seed_base_overrides_from_tree(&ctx.cell_state_tree)?;

        // ── STEP 1: Selected parent coinbase (analyze → commit) ──────
        let selected_parent_txs = self.block_transactions_store.get(selected_parent).unwrap();
        let selected_parent_daa_score = self.headers_store.get_daa_score(selected_parent).expect("selected parent header must exist");

        // Phase 1 — Analyze: produce effect from immutable snapshot
        let snapshot = ExecutionSnapshot::from_current_state(&ctx.cell_state_tree, &processed_txs, &replay_validation);
        let sp_effect = self.analyze_selected_parent_coinbase(
            &snapshot,
            selected_parent,
            selected_parent_txs.as_slice(),
            selected_parent_daa_score,
        )?;

        // Update cross-block bookkeeping
        for tx_id in &sp_effect.newly_processed_tx_ids {
            processed_txs.insert(*tx_id);
        }

        // Sync overlay with the effect so subsequent blocks see these cells
        self.apply_effect_to_overlay(&mut replay_validation, &sp_effect, selected_parent, selected_parent_txs.as_slice())?;

        // Phase 2 — Commit: apply effect to mutable context
        self.commit_execution_effect(ctx, sp_effect)?;

        // ── STEP 2: Mergeset blues — layer-parallel analyze, sequential commit ──
        //
        // Design: MPE parallelization via ExecutionDAG
        //
        // 1. Pre-scan: statically extract BlockAccessSummary from each blue block's
        //    raw transactions (no full analysis needed — just read the inputs/outputs/deps).
        // 2. Build DAG: construct an ExecutionDAG that groups independent
        //    canonical-contiguous blocks into layers.
        // 3. Execute per layer:
        //    - Freeze a shared ExecutionSnapshot at the current canonical state.
        //    - Analyze all blocks in the layer in parallel via rayon `par_iter()`.
        //    - After all analyses complete, commit effects sequentially in
        //      GhostDAG canonical order (= index order within blue_block_data).
        //    - Before each commit, run conflict detection: if any consumed cell
        //      or read-only cell_dep prerequisite has already been removed by a
        //      preceding same-layer commit, the effect is invalidated (replaced
        //      with an empty effect) per MPE_PROTOCOL_SEMANTICS rule 3.
        //
        // Invariant: final commit order is identical to the original serial order,
        // so cell_root, accepted_tx_ids, mergeset_acceptance_data, and reward_data
        // are bitwise identical to the serial two-phase model.
        {
            use super::access_summary::BlockAccessSummary;
            use super::execution_dag::ExecutionDAG;
            use rayon::prelude::*;

            // 2a. Collect all blue blocks' transactions, DAA scores, and access summaries
            let mergeset_blues = ctx.ghostdag_data.mergeset_blues.iter().copied().collect::<Vec<_>>();
            let mut blue_block_data: Vec<(Hash, Arc<Vec<spora_exec::CellTx>>, u64)> = Vec::with_capacity(mergeset_blues.len());
            let mut summaries: Vec<BlockAccessSummary> = Vec::with_capacity(mergeset_blues.len());

            for blue_block in &mergeset_blues {
                let block_txs = self.block_transactions_store.get(*blue_block).unwrap();
                let blue_block_daa_score = self.headers_store.get_daa_score(*blue_block).expect("blue block header must exist");

                let summary = BlockAccessSummary::try_from_block_txs_with_cellscript_scheduler(*blue_block, block_txs.as_slice())
                    .map_err(|err| RuleError::CellValidationError(err.to_string()))?;
                summaries.push(summary);
                blue_block_data.push((*blue_block, block_txs, blue_block_daa_score));
            }

            // 2b. Build ExecutionDAG — group independent blocks into parallel layers
            let dag = ExecutionDAG::build(&summaries);

            // 2c. Execute layer by layer: intra-layer parallel, inter-layer sequential
            for layer in &dag.layers {
                if layer.len() == 1 {
                    // ── Single block in layer — serial path (avoid parallel overhead) ──
                    let idx = layer[0];
                    let (blue_block, ref block_txs, blue_block_daa_score) = blue_block_data[idx];

                    let snapshot = ExecutionSnapshot::from_current_state(&ctx.cell_state_tree, &processed_txs, &replay_validation);
                    let effect = self.analyze_blue_block(&snapshot, blue_block, block_txs.as_slice(), blue_block_daa_score)?;
                    ensure_execution_effect_available_for_commit(&ctx.cell_state_tree, &effect, blue_block)?;
                    let final_effect = effect;

                    // Update cross-block bookkeeping
                    for tx_id in &final_effect.newly_processed_tx_ids {
                        processed_txs.insert(*tx_id);
                    }

                    // Sync overlay for subsequent blocks
                    self.apply_effect_to_overlay(&mut replay_validation, &final_effect, blue_block, block_txs.as_slice())?;

                    replay_validation.accumulated_cycles = final_effect.consumed_cycles;

                    // Commit effect to mutable context
                    self.commit_execution_effect(ctx, final_effect)?;
                } else {
                    // ── Multiple blocks in layer — freeze snapshot, parallel analyze ──
                    //
                    // All blocks in this layer share the same frozen snapshot.
                    // They are analyzed concurrently on independent local copies,
                    // then committed sequentially in canonical order.
                    let snapshot = ExecutionSnapshot::from_current_state(&ctx.cell_state_tree, &processed_txs, &replay_validation);

                    // Parallel analysis via rayon par_iter
                    let effects: Result<Vec<BlockExecutionEffect>, RuleError> = layer
                        .par_iter()
                        .map(|&idx| {
                            let (blue_block, ref block_txs, blue_block_daa_score) = blue_block_data[idx];
                            self.analyze_blue_block(&snapshot, blue_block, block_txs.as_slice(), blue_block_daa_score)
                        })
                        .collect();
                    let effects = effects?;

                    // Sequential commit in canonical order (layer index order
                    // corresponds to GhostDAG canonical order because
                    // blue_block_data preserves mergeset_blues ordering and
                    // ExecutionDAG::build preserves input indices).
                    for (layer_pos, &idx) in layer.iter().enumerate() {
                        let (blue_block, ref block_txs, blue_block_daa_score) = blue_block_data[idx];
                        let effect = &effects[layer_pos];

                        // ── Conflict detection (MPE_PROTOCOL_SEMANTICS rule 3) ──
                        // If any cell this effect consumes or reads as a cell_dep
                        // has already been removed from the canonical tree by a
                        // preceding same-layer commit, the entire effect is
                        // invalidated.
                        let has_conflict = execution_effect_conflicts_with_current_state(&ctx.cell_state_tree, effect);

                        let final_effect = if has_conflict {
                            // Replace with empty effect — block's txs are rejected
                            // but reward data is preserved (miner still gets reward
                            // for the block existing in the DAG).
                            invalidate_effect_preserving_reward(effect, blue_block, blue_block_daa_score)
                        } else {
                            effects[layer_pos].clone()
                        };

                        // Update cross-block bookkeeping
                        for tx_id in &final_effect.newly_processed_tx_ids {
                            processed_txs.insert(*tx_id);
                        }

                        // Sync overlay
                        self.apply_effect_to_overlay(&mut replay_validation, &final_effect, blue_block, block_txs.as_slice())?;

                        replay_validation.accumulated_cycles = final_effect.consumed_cycles;

                        // Commit to mutable context
                        self.commit_execution_effect(ctx, final_effect)?;
                    }
                }
            }
        }

        // ── STEP 3: Red blocks — rewards only, no transaction acceptance ─
        //
        // Red blocks are blocks that GHOSTDAG classified as non-blue (i.e.
        // not on the selected chain). Their transactions are NOT accepted
        // because:
        //   - A red block may conflict with blue blocks
        //   - Accepting red-block transactions would break deterministic state
        //
        // SAFETY: Red block outputs are never inserted into the CellStateTree,
        // so they cannot be referenced as inputs or CellDeps by any subsequent
        // transaction. Treat any leak as a hard consensus invariant violation.
        let mergeset_reds = ctx.ghostdag_data.mergeset_reds.iter().copied().collect::<Vec<_>>();
        for red_block in mergeset_reds {
            let block_txs = self.block_transactions_store.get(red_block).unwrap();
            let red_block_daa_score = self.headers_store.get_daa_score(red_block).expect("red block header must exist");
            ensure_red_block_outputs_absent(&ctx.cell_state_tree, red_block, block_txs.as_slice())?;

            // Red blocks always get an acceptance data entry (even though empty)
            // to maintain parity with the original per-block acceptance tracking.
            ctx.mergeset_acceptance_data
                .push(MergesetBlockAcceptanceData { block_hash: red_block, accepted_transactions: Vec::new() });

            let mut red_effect = BlockExecutionEffect::empty(red_block, red_block_daa_score);
            red_effect.reward_data = self.build_block_reward_data(block_txs.as_slice(), red_block_daa_score);
            self.commit_execution_effect(ctx, red_effect)?;
        }

        Ok(())
    }

    /// Compute lock script hash from the canonical lock script
    #[allow(dead_code)]
    pub(super) fn compute_lock_hash(&self, lock_script: &spora_consensus_core::tx::Script) -> [u8; 32] {
        lock_script.hash()
    }

    /// Compute data hash for cell output
    fn compute_data_hash(&self, data: &[u8]) -> [u8; 32] {
        compute_data_hash(data)
    }

    /// Commit the Cell state for a chain block
    ///
    /// This stores the Cell state diff and root to database.
    /// Commits Cell state using the Cell model.
    pub(super) fn commit_cell_state(
        &self,
        hash: Hash,
        _cell_diff: CellDiff,
        _cell_root: Hash,
        acceptance_data: Vec<MergesetBlockAcceptanceData>,
        pruning_sample: Hash,
    ) {
        use rocksdb::WriteBatch;
        use spora_consensus_core::blockstatus::BlockStatus::StatusCellValid;
        use std::sync::Arc;

        let mut batch = WriteBatch::default();

        // Store cell_diff to dedicated store
        self.cell_diffs_store.insert_batch(&mut batch, hash, Arc::new(_cell_diff)).unwrap();

        // Store cell_root to dedicated store
        self.cell_roots_store.insert_batch(&mut batch, hash, _cell_root).unwrap();

        // Store acceptance data (unchanged)
        self.acceptance_data_store.insert_batch(&mut batch, hash, Arc::new(acceptance_data)).unwrap();

        // Store pruning sample (unchanged)
        self.pruning_samples_store.insert_batch(&mut batch, hash, pruning_sample).unwrap_or_exists();

        let write_guard = self.statuses_store.set_batch(&mut batch, hash, StatusCellValid).unwrap();

        self.db.write(batch).unwrap();
        drop(write_guard);
    }
}

#[cfg(test)]
mod tests {
    use super::ensure_red_block_outputs_absent;
    #[cfg(feature = "vm")]
    use super::{BlueBlockExecutionResult, ExecutionSnapshot};
    use crate::errors::RuleError;
    use crate::processes::utils::outpoint_to_hash;
    #[cfg(feature = "vm")]
    use crate::{config::ConfigBuilder, consensus::test_consensus::TestConsensus};
    #[cfg(feature = "vm")]
    use secp256k1::Keypair;
    #[cfg(feature = "vm")]
    use spora_addresses::{Address, Prefix};
    use spora_consensus_core::tx::TransactionOutpoint;
    #[cfg(feature = "vm")]
    use spora_consensus_core::{api::ConsensusApi, sign::sign, tx::pay_to_address_lock_script};
    #[cfg(feature = "vm")]
    use spora_consensus_core::{
        block::{TemplateBuildMode, TemplateTransactionSelector},
        coinbase::MinerData,
        config::params::MAINNET_PARAMS,
        tx::MutableTransaction,
    };
    use spora_exec::{CellOutput, CellTx, OutPoint, Script};
    use spora_hashes::Hash;
    use spora_state::{CellEntry, CellStateTree};
    #[cfg(feature = "vm")]
    use std::collections::HashSet;

    #[cfg(feature = "vm")]
    struct OnetimeTxSelector {
        txs: Option<Vec<CellTx>>,
    }

    #[cfg(feature = "vm")]
    impl OnetimeTxSelector {
        fn new(txs: Vec<CellTx>) -> Self {
            Self { txs: Some(txs) }
        }
    }

    #[cfg(feature = "vm")]
    impl TemplateTransactionSelector for OnetimeTxSelector {
        fn select_transactions(&mut self) -> Vec<CellTx> {
            self.txs.take().unwrap_or_default()
        }

        fn reject_selection(&mut self, _tx_id: spora_consensus_core::tx::TransactionId) {}

        fn is_successful(&self) -> bool {
            true
        }
    }

    #[cfg(feature = "vm")]
    fn metadata_from_tx_output(
        block_hash: Hash,
        block_daa_score: u64,
        is_cellbase: bool,
        tx: &CellTx,
        output_index: u32,
    ) -> spora_consensus_core::cell_metadata::CellMetadata {
        let output = &tx.outputs[output_index as usize];
        let output_data = tx.outputs_data.get(output_index as usize).map(Vec::as_slice).unwrap_or(&[]);
        spora_consensus_core::cell_metadata::CellMetadata {
            out_point: TransactionOutpoint { tx_hash: tx.id(), index: output_index },
            capacity: output.capacity,
            data_bytes: output_data.len() as u64,
            lock_hash: output.lock.hash(),
            type_hash: output.type_.as_ref().map(|script| script.hash()),
            data_hash: crate::processes::utils::compute_data_hash(output_data),
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

    #[test]
    fn outpoint_hash_distinguishes_transaction_id_and_index() {
        let tx_a_0 = TransactionOutpoint { tx_hash: [0xAA; 32], index: 0 };
        let tx_a_1 = TransactionOutpoint { tx_hash: [0xAA; 32], index: 1 };
        let tx_b_0 = TransactionOutpoint { tx_hash: [0xBB; 32], index: 0 };

        assert_ne!(outpoint_to_hash(&tx_a_0), outpoint_to_hash(&tx_a_1));
        assert_ne!(outpoint_to_hash(&tx_a_0), outpoint_to_hash(&tx_b_0));
    }

    #[test]
    fn outpoint_hash_matches_exec_outpoint_identity() {
        let tx = OutPoint::new([0x11; 32], 7);
        let consensus_outpoint = TransactionOutpoint { tx_hash: tx.tx_hash, index: tx.index };
        let same = TransactionOutpoint { tx_hash: tx.tx_hash, index: tx.index };

        assert_eq!(outpoint_to_hash(&consensus_outpoint), outpoint_to_hash(&same));
    }

    #[cfg(feature = "vm")]
    #[tokio::test]
    async fn analyze_blue_block_resumable_matches_direct_for_native_pubkey_candidate() {
        let config = ConfigBuilder::new(MAINNET_PARAMS)
            .skip_proof_of_work()
            .edit_consensus_params(|params| {
                params.coinbase_maturity = 0;
            })
            .build();
        let consensus = TestConsensus::new(&config);
        let wait_handles = consensus.init();
        let virtual_processor = consensus.virtual_processor().clone();

        let keypair = Keypair::from_seckey_slice(secp256k1::SECP256K1, &[0x71; 32]).expect("valid secret key");
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

        let staging = consensus
            .build_block_template(miner_data.clone(), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard)
            .unwrap();
        consensus.validate_and_insert_block(staging.block.to_immutable()).virtual_state_task.await.unwrap();

        let input_outpoint = OutPoint::new(funding_coinbase.id(), 0);
        let spend_capacity = funding_coinbase.outputs[0].capacity.checked_sub(1_000).expect("coinbase output should be large enough");
        let unsigned_tx = CellTx::new(
            vec![spora_exec::CellInput::new(input_outpoint, 0)],
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
        let selected_parent = block.header.direct_parents()[0];
        let virtual_state = virtual_processor.lkg_virtual_state.load();
        let replay_ctx = virtual_processor.build_replay_validation_context(
            selected_parent,
            selected_parent,
            block.header.daa_score,
            block.header.timestamp,
        );
        let snapshot = ExecutionSnapshot::from_current_state(&virtual_state.cell_state_tree, &HashSet::new(), &replay_ctx);

        let direct = virtual_processor
            .analyze_blue_block(&snapshot, block.hash(), block.transactions.as_slice(), block.header.daa_score)
            .expect("direct blue block analysis should succeed");
        let initial = virtual_processor
            .analyze_blue_block_resumable(&snapshot, block.hash(), block.transactions.as_slice(), block.header.daa_score, 1)
            .expect("initial resumable analysis should succeed");
        let state = match initial {
            BlueBlockExecutionResult::Suspended(state) => state,
            BlueBlockExecutionResult::Completed(effect) => {
                panic!("expected suspension for tiny cycle budget, got completed effect with {} cycles", effect.consumed_cycles)
            }
        };
        assert_eq!(state.current, 1, "candidate block should suspend on the first non-coinbase tx");
        let current_cycles = state.current_cycles();
        assert!(current_cycles <= direct.consumed_cycles, "suspended cycles must not exceed the completed direct budget");
        let (next_limit, exhausted) = state.next_limit_cycles(1, direct.consumed_cycles.saturating_add(1));
        assert!(
            exhausted || next_limit >= current_cycles,
            "resumable limit progression must stay monotonic even when the budget is exhausted"
        );
        assert!(next_limit >= current_cycles, "next resumable limit must not move backwards");

        let resumed = virtual_processor
            .resume_analyze_blue_block_from_state(
                &snapshot,
                block.hash(),
                block.transactions.as_slice(),
                block.header.daa_score,
                &state,
                direct.consumed_cycles,
            )
            .expect("resumed blue block analysis should succeed");
        let resumed_effect = match resumed {
            BlueBlockExecutionResult::Completed(effect) => effect,
            BlueBlockExecutionResult::Suspended(next_state) => virtual_processor
                .complete_blue_block_from_state(
                    &snapshot,
                    block.hash(),
                    block.transactions.as_slice(),
                    block.header.daa_score,
                    &next_state,
                    direct.consumed_cycles,
                )
                .expect("complete blue block analysis from resumed state"),
        };

        assert_eq!(resumed_effect.block_hash, direct.block_hash);
        assert_eq!(resumed_effect.block_daa_score, direct.block_daa_score);
        assert_eq!(resumed_effect.accepted_tx_ids, direct.accepted_tx_ids);
        assert_eq!(resumed_effect.newly_processed_tx_ids, direct.newly_processed_tx_ids);
        assert_eq!(resumed_effect.consumed_cycles, direct.consumed_cycles);
        assert_eq!(resumed_effect.cell_diff.add, direct.cell_diff.add);
        assert_eq!(resumed_effect.cell_diff.remove, direct.cell_diff.remove);
        assert_eq!(resumed_effect.accepted_transactions.len(), direct.accepted_transactions.len());
        for (lhs, rhs) in resumed_effect.accepted_transactions.iter().zip(direct.accepted_transactions.iter()) {
            assert_eq!(lhs.transaction_id, rhs.transaction_id);
            assert_eq!(lhs.index_within_block, rhs.index_within_block);
        }
        match (&resumed_effect.reward_data, &direct.reward_data) {
            (Some(lhs), Some(rhs)) => {
                assert_eq!(lhs.subsidy, rhs.subsidy);
                assert_eq!(lhs.total_fees, rhs.total_fees);
                assert_eq!(lhs.lock_script.hash(), rhs.lock_script.hash());
            }
            (None, None) => {}
            other => panic!("reward_data mismatch: {other:?}"),
        }

        consensus.shutdown(wait_handles);
    }

    #[test]
    fn red_block_outputs_must_not_exist_in_live_tree() {
        let lock = Script::new([0x22; 32], 0, vec![]);
        let red_tx =
            CellTx::new(vec![], vec![], vec![CellOutput { lock, type_: None, capacity: 1_000 }], vec![vec![]], vec![]).unwrap();
        let leaked_outpoint = TransactionOutpoint { tx_hash: red_tx.id(), index: 0 };
        let leaked_hash = outpoint_to_hash(&leaked_outpoint);

        let mut tree = CellStateTree::new();
        tree.insert_with_outpoint(
            leaked_hash,
            OutPoint::new(leaked_outpoint.tx_hash, leaked_outpoint.index),
            CellEntry::new(1_000, 0, Hash::from_bytes([1u8; 32]), None, Hash::from_bytes([2u8; 32]), 1, false),
        );

        assert!(matches!(
            ensure_red_block_outputs_absent(&tree, Hash::from_bytes([0x33; 32]), &[red_tx]),
            Err(RuleError::CellValidationError(msg)) if msg.contains("leaked output")
        ));
    }
}
