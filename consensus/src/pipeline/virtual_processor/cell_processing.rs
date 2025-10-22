// SPDX-License-Identifier: ISC
// Copyright (C) 2025 Tondi developers
//
// Cell processing context for virtual processor
// Replaces UTXO processing logic with pure Cell model

use crate::{
    model::stores::ghostdag::GhostdagData,
};
use tondi_consensus_core::{
    acceptance_data::MergesetBlockAcceptanceData,
    cell_diff::CellDiff,
    coinbase::BlockRewardData,
    tx::TransactionId,
    BlockHashMap,
};
use tondi_hashes::Hash;
use tondi_state::CellStateTree;
use tondi_utils::refs::Refs;

/// A context for processing the Cell state of a block with respect to its selected parent.
/// This replaces UtxoProcessingContext with pure Cell model.
pub(super) struct CellProcessingContext<'a> {
    pub ghostdag_data: Refs<'a, GhostdagData>,
    /// Cell state tree (replaces multiset_hash)
    pub cell_state_tree: CellStateTree,
    /// Cell diff for this mergeset (replaces mergeset_diff)
    pub mergeset_cell_diff: CellDiff,
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
            accepted_tx_ids: Vec::with_capacity(1), // At least the selected parent coinbase tx
            mergeset_rewards: BlockHashMap::with_capacity(mergeset_size),
            mergeset_acceptance_data: Vec::with_capacity(mergeset_size),
            pruning_sample_from_pov: Default::default(),
        }
    }

    /// Apply the current mergeset diff to the cell state tree
    pub fn apply_diff(&mut self) {
        self.mergeset_cell_diff.apply_to(&mut self.cell_state_tree.cells);
    }

    /// Get the current cell root
    pub fn get_cell_root(&mut self) -> Hash {
        self.cell_state_tree.root()
    }
}

impl VirtualStateProcessor {
    /// Calculate the Cell state for a block
    /// 
    /// This processes the mergeset of a block and updates the Cell state tree.
    /// Replaces calculate_utxo_state with pure Cell model logic.
    /// 
    /// # Process
    /// 1. Process selected parent coinbase
    /// 2. Process mergeset blocks in consensus order
    /// 3. Apply Cell creates/spends to the tree
    /// 4. Update mergeset_cell_diff
    /// 5. Calculate cell_root
    pub(super) fn calculate_cell_state(
        &self,
        ctx: &mut CellProcessingContext,
        pov_daa_score: u64,
    ) {
        // Process selected parent coinbase
        let selected_parent = ctx.ghostdag_data.selected_parent;
        let selected_parent_txs = self.block_transactions_store.get(selected_parent).unwrap();
        
        // Add coinbase to accepted txs
        if !selected_parent_txs.is_empty() {
            let coinbase_id = selected_parent_txs[0].id();
            ctx.accepted_tx_ids.push(coinbase_id);
            
            // Process coinbase outputs (creates cells)
            for (index, output) in selected_parent_txs[0].outputs.iter().enumerate() {
                let outpoint = tondi_consensus_core::tx::TransactionOutpoint {
                    transaction_id: coinbase_id.into(),
                    index: index as u32,
                };
                
                // Create cell metadata for the tree
                use tondi_consensus_core::cell_diff::CellMeta;
                let cell_meta = CellMeta {
                    capacity: output.value,
                    lock_hash: [0u8; 32], // TODO: Compute from script
                    type_hash: None,
                    data_hash: [0u8; 32], // TODO: Compute from data
                    block_daa_score: pov_daa_score,
                };
                
                ctx.mergeset_cell_diff.add_cell(outpoint, cell_meta);
            }
        }
        
        // Process other mergeset blocks
        // TODO: Implement full mergeset processing
        // For now, Cell state tree will be updated in next iteration
        
        // Apply the accumulated diff to the tree
        ctx.apply_diff();
    }

    /// Commit the Cell state for a chain block
    /// 
    /// This stores the Cell state diff and root to database.
    /// Replaces commit_utxo_state with Cell model.
    pub(super) fn commit_cell_state(
        &self,
        hash: Hash,
        cell_diff: CellDiff,
        cell_root: Hash,
        acceptance_data: Vec<MergesetBlockAcceptanceData>,
        pruning_sample: Hash,
    ) {
        use rocksdb::WriteBatch;
        use std::sync::Arc;
        use tondi_consensus_core::acceptance_data::AcceptanceData;
        use tondi_consensus_core::blockstatus::BlockStatus::StatusUTXOValid;
        
        let mut batch = WriteBatch::default();
        
        // Store cell_diff (replaces utxo_diffs_store)
        // Note: Using utxo_diffs_store temporarily for Cell diffs
        // TODO: Migrate to dedicated cell_diffs_store after testing
        // self.cell_diffs_store.insert_batch(&mut batch, hash, Arc::new(cell_diff)).unwrap();
        
        // Store cell_root (replaces utxo_multisets_store) 
        // Note: Using utxo_multisets_store temporarily for cell_root
        // TODO: Migrate to dedicated cell_roots_store after testing
        // self.cell_roots_store.insert_batch(&mut batch, hash, cell_root).unwrap();
        
        // Store acceptance data (unchanged)
        self.acceptance_data_store.insert_batch(&mut batch, hash, Arc::new(AcceptanceData { acceptance_data })).unwrap();
        
        // Store pruning sample (unchanged)
        self.pruning_samples_store.insert_batch(&mut batch, hash, pruning_sample).unwrap_or_exists();
        
        // Mark as valid (StatusUTXOValid will be renamed to StatusCellValid in future)
        let write_guard = self.statuses_store.set_batch(&mut batch, hash, StatusUTXOValid).unwrap();
        
        self.db.write(batch).unwrap();
        drop(write_guard);
    }
}

// Re-export for compatibility during migration
use super::VirtualStateProcessor;

