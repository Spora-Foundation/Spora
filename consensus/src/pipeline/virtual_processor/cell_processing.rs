// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Cell processing context for virtual processor
// Replaces legacy transaction-output processing logic with the pure Cell model

use super::VirtualStateProcessor;

use crate::consensus::cell_provider::{ConsensusCellProvider, OverlayCellProvider};
use crate::model::stores::{
    block_transactions::BlockTransactionsStoreReader, ghostdag::GhostdagData, statuses::StatusesStoreBatchExtensions,
};
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
/// This replaces the legacy processing context with the pure Cell model.
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
    pub fn verify_cell_root(&mut self, expected_root: Hash) -> Result<(), String> {
        let calculated_root = self.get_cell_root();
        if calculated_root == expected_root {
            Ok(())
        } else {
            Err(format!("Cell root mismatch: expected {:?}, got {:?}", expected_root, calculated_root))
        }
    }
}

#[derive(Debug, Clone)]
struct SelectedParentCoinbaseEffect {
    accepted_tx_id: Option<TransactionId>,
    cell_diff: CellDiff,
    reward_data: Option<BlockRewardData>,
}

#[derive(Debug, Clone)]
struct BlockCellProcessingEffect {
    block_hash: Hash,
    accepted_tx_ids: Vec<TransactionId>,
    accepted_transactions: Vec<AcceptedTxEntry>,
    cell_diff: CellDiff,
    reward_data: Option<BlockRewardData>,
}

impl BlockCellProcessingEffect {
    fn acceptance_data(&self) -> MergesetBlockAcceptanceData {
        MergesetBlockAcceptanceData { block_hash: self.block_hash, accepted_transactions: self.accepted_transactions.clone() }
    }
}

/// Convert TransactionOutpoint to Hash for tree indexing (helper function)
fn outpoint_to_hash(outpoint: &spora_consensus_core::tx::TransactionOutpoint) -> Hash {
    use blake3::Hasher;

    let mut hasher = Hasher::new();
    hasher.update(b"spora-cell/outpoint"); // Domain separation
    hasher.update(&outpoint.tx_hash);
    hasher.update(&outpoint.index.to_le_bytes());

    Hash::from_bytes(*hasher.finalize().as_bytes())
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
    }
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

struct ReplayValidationContext {
    snapshot_pov: Hash,
    current_daa_score: u64,
    current_timestamp: u64,
    params: Arc<CellConsensusParams>,
    provider: ReplayOverlayProvider,
}

impl ReplayValidationContext {
    fn validate_tx(&self, tx: &spora_exec::CellTx) -> Result<(), RuleError> {
        // Isolation (stateless) — no provider needed
        cell_validation_in_isolation::validate_cell_tx_in_isolation(tx, self.params.max_cell_data_size)
            .map_err(|err| self.map_validation_error(tx, err))?;

        // Check tx serialized size
        let tx_size =
            borsh::to_vec(tx).map_err(|e| self.map_validation_error(tx, CellValidationError::InvalidFormat(e.to_string())))?.len();
        if tx_size > self.params.max_tx_size {
            return Err(self.map_validation_error(
                tx,
                CellValidationError::InvalidFormat(format!("Transaction too large: {} > {}", tx_size, self.params.max_tx_size)),
            ));
        }

        // DAG existence check (takes &P reference, no clone)
        cell_validation_in_dag::validate_cell_existence(tx, self.snapshot_pov, &self.provider)
            .map_err(|err| self.map_validation_error(tx, err))?;

        // Context validation — capacity conservation (takes &P reference, no clone)
        cell_validation_in_context::validate_cell_tx_in_context(tx, self.snapshot_pov, self.current_daa_score, &self.provider)
            .map_err(|err| self.map_validation_error(tx, err))?;

        // Time locks (takes &P reference, no clone)
        cell_validation_in_dag::validate_time_locks(
            tx,
            self.snapshot_pov,
            self.current_daa_score,
            self.current_timestamp,
            &self.provider,
        )
        .map_err(|err| self.map_validation_error(tx, err))?;

        // Cellbase maturity (takes &P reference, no clone)
        cell_validation_in_dag::validate_cellbase_maturity(
            tx,
            self.snapshot_pov,
            self.current_daa_score,
            self.params.cellbase_maturity,
            &self.provider,
        )
        .map_err(|err| self.map_validation_error(tx, err))?;

        // VM script verification (only when vm feature enabled — still needs clone for Arc<CellDataProvider>)
        #[cfg(feature = "vm")]
        {
            let validator = CellValidator::new(self.params.clone(), Arc::new(self.provider.clone()));
            validator.verify_scripts(tx, self.snapshot_pov).map_err(|err| self.map_validation_error(tx, err))?;
        }

        Ok(())
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
                    let metadata = self.provider.get_cell_metadata(&input.out_point).ok().flatten();
                    if let Some(metadata) = metadata {
                        if metadata.is_cellbase && self.current_daa_score < metadata.block_daa_score.saturating_add(maturity) {
                            return RuleError::TxInContextFailed(
                                tx_id,
                                TxRuleError::ImmatureCoinbaseSpend(
                                    input_index,
                                    TransactionOutpoint::new(input.out_point.tx_hash, input.out_point.index),
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
            self.statuses_store.clone(),
        );
        ReplayValidationContext {
            snapshot_pov,
            current_daa_score,
            current_timestamp,
            params: Arc::new(CellConsensusParams { cellbase_maturity: self.coinbase_maturity, ..CellConsensusParams::default() }),
            provider: OverlayCellProvider::new(base_provider, snapshot_pov, base_pov),
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
                spora_consensus_core::tx::ScriptPublicKey::from_vec(0, vec![]),
            )
        };

        Some(
            block_txs
                .first()
                .and_then(|tx| tx.payload())
                .and_then(|payload| self.coinbase_manager.deserialize_coinbase_payload(payload).ok())
                .map(|coinbase_data| {
                    BlockRewardData::new(coinbase_data.subsidy, 0, coinbase_data.miner_data.script_public_key.clone())
                })
                .unwrap_or_else(fallback),
        )
    }

    fn commit_selected_parent_coinbase_effect(
        &self,
        ctx: &mut CellProcessingContext,
        block_hash: Hash,
        block_daa_score: u64,
        effect: SelectedParentCoinbaseEffect,
    ) -> Result<(), RuleError> {
        if !effect.cell_diff.is_empty() {
            ctx.block_cell_diffs.push(BlockCellDiff::new(block_hash, block_daa_score, effect.cell_diff.clone()));
        }
        if let Some(reward_data) = effect.reward_data {
            ctx.mergeset_rewards.entry(block_hash).or_insert(reward_data);
        }
        if let Some(accepted_tx_id) = effect.accepted_tx_id {
            ctx.accepted_tx_ids.push(accepted_tx_id);
        }
        ctx.mergeset_cell_diff.with_diff_in_place(&effect.cell_diff).map_err(RuleError::CellValidationError)
    }

    fn commit_block_effect(
        &self,
        ctx: &mut CellProcessingContext,
        block_daa_score: u64,
        effect: BlockCellProcessingEffect,
    ) -> Result<(), RuleError> {
        if !effect.cell_diff.is_empty() {
            ctx.block_cell_diffs.push(BlockCellDiff::new(effect.block_hash, block_daa_score, effect.cell_diff.clone()));
        }
        let acceptance_data = effect.acceptance_data();
        let BlockCellProcessingEffect { block_hash, accepted_tx_ids, cell_diff, reward_data, .. } = effect;
        if let Some(reward_data) = reward_data {
            ctx.mergeset_rewards.entry(block_hash).or_insert(reward_data);
        }
        ctx.accepted_tx_ids.extend(accepted_tx_ids);
        ctx.mergeset_acceptance_data.push(acceptance_data);
        ctx.mergeset_cell_diff.with_diff_in_place(&cell_diff).map_err(RuleError::CellValidationError)
    }

    fn process_selected_parent_coinbase(
        &self,
        tree: &mut CellStateTree,
        selected_parent: Hash,
        selected_parent_txs: &[spora_exec::CellTx],
        selected_parent_daa_score: u64,
        replay_validation: &mut ReplayValidationContext,
    ) -> Result<SelectedParentCoinbaseEffect, RuleError> {
        let mut effect = SelectedParentCoinbaseEffect {
            accepted_tx_id: None,
            cell_diff: CellDiff::default(),
            reward_data: self.build_block_reward_data(selected_parent_txs, selected_parent_daa_score),
        };

        if selected_parent_txs.is_empty() {
            return Ok(effect);
        }

        let coinbase_id = selected_parent_txs[0].id();
        effect.accepted_tx_id = Some(coinbase_id.into());

        for (index, output) in selected_parent_txs[0].outputs.iter().enumerate() {
            let outpoint = TransactionOutpoint { tx_hash: coinbase_id, index: index as u32 };
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
            };

            let outpoint_hash = outpoint_to_hash(&outpoint);
            if tree.get(&outpoint_hash).is_some() || effect.cell_diff.add.contains_key(&outpoint) {
                return Err(RuleError::CellValidationError(format!(
                    "selected parent coinbase tried to create duplicate outpoint {outpoint}"
                )));
            }

            tree.insert_with_outpoint(outpoint_hash, exec_outpoint(&outpoint), cell_meta_to_entry(&cell_meta));
            effect.cell_diff.add_cell(outpoint, cell_meta);
            replay_validation.add_cell(cell_metadata_from_output(
                selected_parent,
                selected_parent_daa_score,
                true,
                coinbase_id,
                index as u32,
                output,
                output_data,
            ))?;
        }

        Ok(effect)
    }

    fn process_blue_block(
        &self,
        tree: &mut CellStateTree,
        blue_block: Hash,
        block_txs: &[spora_exec::CellTx],
        blue_block_daa_score: u64,
        processed_txs: &mut std::collections::HashSet<Hash>,
        replay_validation: &mut ReplayValidationContext,
    ) -> Result<BlockCellProcessingEffect, RuleError> {
        let mut effect = BlockCellProcessingEffect {
            block_hash: blue_block,
            accepted_tx_ids: Vec::new(),
            accepted_transactions: Vec::with_capacity(block_txs.len()),
            cell_diff: CellDiff::default(),
            reward_data: self.build_block_reward_data(block_txs, blue_block_daa_score),
        };

        for (tx_index, tx) in block_txs.iter().enumerate() {
            let tx_id = tx.id();
            let is_coinbase = tx_index == 0 && tx.is_coinbase();
            let mut input_capacity = 0u64;
            let mut output_capacity = 0u64;

            if processed_txs.contains(&tx_id.into()) {
                continue;
            }

            processed_txs.insert(tx_id.into());
            effect.accepted_tx_ids.push(tx_id.into());
            effect.accepted_transactions.push(AcceptedTxEntry { transaction_id: tx_id.into(), index_within_block: tx_index as u32 });

            if !is_coinbase {
                replay_validation.validate_tx(tx)?;
            }

            for input in &tx.inputs {
                let outpoint = TransactionOutpoint { tx_hash: input.out_point.tx_hash, index: input.out_point.index };

                let outpoint_hash = outpoint_to_hash(&outpoint);
                if effect.cell_diff.remove.contains_key(&outpoint) {
                    return Err(RuleError::DoubleSpendInSameBlock(outpoint));
                }
                let removed_entry = tree
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
                replay_validation.spend_cell(&input.out_point)?;
            }

            for (index, output) in tx.outputs.iter().enumerate() {
                output_capacity = output_capacity
                    .checked_add(output.capacity)
                    .ok_or_else(|| RuleError::TxInContextFailed(tx_id.into(), TxRuleError::OutputsValueOverflow))?;
                if output_capacity > MAX_SAU {
                    return Err(RuleError::TxInContextFailed(tx_id.into(), TxRuleError::TotalTxOutTooHigh));
                }

                let outpoint = TransactionOutpoint { tx_hash: tx_id, index: index as u32 };
                let outpoint_hash = outpoint_to_hash(&outpoint);

                if tree.get(&outpoint_hash).is_some()
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
                };

                tree.insert_with_outpoint(outpoint_hash, exec_outpoint(&outpoint), cell_meta_to_entry(&cell_meta));
                effect.cell_diff.add_cell(outpoint, cell_meta);
                replay_validation.add_cell(cell_metadata_from_output(
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

        Ok(effect)
    }

    /// Calculate the Cell state for a block
    ///
    /// This processes the mergeset of a block and updates the Cell state tree.
    /// Calculates Cell state using pure Cell model logic.
    ///
    /// # GHOSTDAG-aware Process
    /// 1. Process selected parent coinbase
    /// 2. Process mergeset blocks in GHOSTDAG topological order (blues)
    /// 3. For each transaction:
    ///    - Skip if already processed (duplicate in mergeset)
    ///    - Consume inputs (remove cells)
    ///    - Create outputs (add cells)
    /// 4. Apply accumulated diff to tree
    /// 5. Calculate cell_root (Merkle root)
    pub(super) fn calculate_cell_state(
        &self,
        ctx: &mut CellProcessingContext,
        pov_daa_score: u64,
        snapshot_pov: Hash,
        current_timestamp: u64,
    ) -> Result<(), RuleError> {
        use std::collections::HashSet;

        // Track processed transactions to avoid duplicates in mergeset
        let mut processed_txs: HashSet<Hash> = HashSet::new();

        // STEP 1: Process selected parent coinbase
        let selected_parent = ctx.ghostdag_data.selected_parent;
        let mut replay_validation =
            self.build_replay_validation_context(snapshot_pov, selected_parent, pov_daa_score, current_timestamp);
        let selected_parent_txs = self.block_transactions_store.get(selected_parent).unwrap();
        let selected_parent_daa_score = self.headers_store.get_daa_score(selected_parent).expect("selected parent header must exist");
        let selected_parent_effect = self.process_selected_parent_coinbase(
            &mut ctx.cell_state_tree,
            selected_parent,
            selected_parent_txs.as_slice(),
            selected_parent_daa_score,
            &mut replay_validation,
        )?;
        if let Some(accepted_tx_id) = selected_parent_effect.accepted_tx_id {
            processed_txs.insert(accepted_tx_id);
        }
        self.commit_selected_parent_coinbase_effect(ctx, selected_parent, selected_parent_daa_score, selected_parent_effect)?;
        // STEP 2: Process mergeset blues in GHOSTDAG topological order
        // Note: mergeset_blues already in topological order from GHOSTDAG
        let mergeset_blues = ctx.ghostdag_data.mergeset_blues.iter().copied().collect::<Vec<_>>();
        for blue_block in mergeset_blues {
            let block_txs = self.block_transactions_store.get(blue_block).unwrap();
            let blue_block_daa_score = self.headers_store.get_daa_score(blue_block).expect("blue block header must exist");
            let effect = self.process_blue_block(
                &mut ctx.cell_state_tree,
                blue_block,
                block_txs.as_slice(),
                blue_block_daa_score,
                &mut processed_txs,
                &mut replay_validation,
            )?;
            self.commit_block_effect(ctx, blue_block_daa_score, effect)?;
        }

        // STEP 3: Process red blocks — record rewards only, reject all transactions.
        //
        // Red blocks are blocks that GHOSTDAG classified as non-blue (i.e. they are
        // not on the selected chain). Their transactions are NOT accepted because:
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

            self.commit_block_effect(
                ctx,
                red_block_daa_score,
                BlockCellProcessingEffect {
                    block_hash: red_block,
                    accepted_tx_ids: Vec::new(),
                    accepted_transactions: Vec::new(),
                    cell_diff: CellDiff::default(),
                    reward_data: self.build_block_reward_data(block_txs.as_slice(), red_block_daa_score),
                },
            )?;
        }

        Ok(())
    }

    /// Compute lock script hash from ScriptPublicKey
    pub(super) fn compute_lock_hash(&self, script_public_key: &spora_consensus_core::tx::ScriptPublicKey) -> [u8; 32] {
        use blake3::Hasher;

        let mut hasher = Hasher::new();
        hasher.update(b"spora-cell/lock"); // Domain separation
        hasher.update(&script_public_key.version().to_le_bytes());
        hasher.update(script_public_key.script());

        *hasher.finalize().as_bytes()
    }

    /// Compute data hash for cell output
    ///
    /// Hashes the cell data using blake3
    fn compute_data_hash(&self, data: &[u8]) -> [u8; 32] {
        if data.is_empty() {
            // Empty data has zero hash
            [0u8; 32]
        } else {
            use blake3::Hasher;

            let mut hasher = Hasher::new();
            hasher.update(b"spora-cell/data"); // Domain separation
            hasher.update(data);

            *hasher.finalize().as_bytes()
        }
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
    use super::{ensure_red_block_outputs_absent, outpoint_to_hash};
    use crate::errors::RuleError;
    use spora_consensus_core::tx::TransactionOutpoint;
    use spora_exec::{CellOut, CellTx, OutPoint, ScriptRef};
    use spora_hashes::Hash;
    use spora_state::{CellEntry, CellStateTree};

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

    #[test]
    fn red_block_outputs_must_not_exist_in_live_tree() {
        let lock = ScriptRef::new([0x22; 32], 0, vec![]);
        let red_tx = CellTx::new(vec![], vec![], vec![CellOut { lock, type_: None, capacity: 1_000 }], vec![vec![]], vec![]).unwrap();
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
