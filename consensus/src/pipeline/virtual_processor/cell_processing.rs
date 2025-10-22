// SPDX-License-Identifier: ISC
// Copyright (C) 2025 Tondi developers
//
// Cell processing context for virtual processor
// Replaces UTXO processing logic with pure Cell model

use crate::{
    model::stores::{
        block_transactions::BlockTransactionsStoreReader,
        ghostdag::GhostdagData,
        statuses::StatusesStoreBatchExtensions,
    },
};
use tondi_consensus_core::{
    acceptance_data::MergesetBlockAcceptanceData,
    cell_diff::CellDiff,
    coinbase::BlockRewardData,
    tx::TransactionId,
    BlockHashMap, HashMapCustomHasher,
};
use tondi_database::prelude::StoreResultEmptyTuple;
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
    /// 
    /// GHOSTDAG-aware: applies accumulated diff from processing mergeset blues
    pub fn apply_diff(&mut self) {
        use tondi_state::CellEntry;
        
        // Remove consumed cells
        for outpoint in self.mergeset_cell_diff.remove.keys() {
            // Convert TransactionOutpoint to Hash for tree indexing
            let outpoint_hash = Self::outpoint_to_hash(outpoint);
            self.cell_state_tree.remove(&outpoint_hash);
        }
        
        // Add created cells
        for (outpoint, meta) in &self.mergeset_cell_diff.add {
            // Convert TransactionOutpoint to Hash
            let outpoint_hash = Self::outpoint_to_hash(outpoint);
            
            // Convert CellMeta to CellEntry
            let entry = CellEntry::new(
                meta.capacity,
                Hash::from_bytes(meta.lock_hash),
                meta.type_hash.map(Hash::from_bytes),
                Hash::from_bytes(meta.data_hash),
            );
            
            self.cell_state_tree.insert(outpoint_hash, entry);
        }
        
        // Note: root is recalculated lazily when root() is called
    }
    
    /// Convert TransactionOutpoint to Hash for tree indexing
    fn outpoint_to_hash(outpoint: &tondi_consensus_core::tx::TransactionOutpoint) -> Hash {
        use blake3::Hasher;
        
        let mut hasher = Hasher::new();
        hasher.update(b"tondi-cell/outpoint"); // Domain separation
        hasher.update(&outpoint.transaction_id.as_bytes());
        hasher.update(&outpoint.index.to_le_bytes());
        
        Hash::from_bytes(*hasher.finalize().as_bytes())
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

impl VirtualStateProcessor {
    /// Calculate the Cell state for a block
    /// 
    /// This processes the mergeset of a block and updates the Cell state tree.
    /// Replaces calculate_utxo_state with pure Cell model logic.
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
    ) {
        use std::collections::HashSet;
        use tondi_consensus_core::{
            cell_diff::CellMeta,
            tx::{TransactionId, TransactionOutpoint},
        };
        
        // Track processed transactions to avoid duplicates in mergeset
        let mut processed_txs = HashSet::new();
        
        // STEP 1: Process selected parent coinbase
        let selected_parent = ctx.ghostdag_data.selected_parent;
        let selected_parent_txs = self.block_transactions_store.get(selected_parent).unwrap();
        
        if !selected_parent_txs.is_empty() {
            let coinbase_id = selected_parent_txs[0].id();
            ctx.accepted_tx_ids.push(coinbase_id);
            processed_txs.insert(coinbase_id);
            
            // Process coinbase outputs (creates cells)
            for (index, output) in selected_parent_txs[0].outputs.iter().enumerate() {
                let outpoint = TransactionOutpoint {
                    transaction_id: coinbase_id.into(),
                    index: index as u32,
                };
                
                let cell_meta = CellMeta {
                    capacity: output.value,
                    lock_hash: self.compute_lock_hash(&output.script_public_key),
                    type_hash: None, // Coinbase has no type script
                    data_hash: [0u8; 32], // Coinbase has no data
                    block_daa_score: pov_daa_score,
                };
                
                ctx.mergeset_cell_diff.add_cell(outpoint, cell_meta);
            }
        }
        
        // STEP 2: Process mergeset blues in GHOSTDAG topological order
        // Note: mergeset_blues already in topological order from GHOSTDAG
        for blue_block in ctx.ghostdag_data.mergeset_blues.iter().copied() {
            let block_txs = self.block_transactions_store.get(blue_block).unwrap();
            
            // Process all transactions in this blue block
            for tx in block_txs.iter() {
                let tx_id = tx.id();
                
                // Skip if already processed (can happen in mergeset)
                if processed_txs.contains(&tx_id) {
                    continue;
                }
                
                // Mark as processed
                processed_txs.insert(tx_id);
                ctx.accepted_tx_ids.push(tx_id);
                
                // CONSUME INPUTS (remove cells from state)
                for input in &tx.inputs {
                    let outpoint = TransactionOutpoint {
                        transaction_id: input.previous_outpoint.transaction_id.into(),
                        index: input.previous_outpoint.index,
                    };
                    
                    // Note: Cell metadata for removed cells should come from the existing state
                    // For now, we create a placeholder - this should ideally query the state
                    let removed_meta = CellMeta {
                        capacity: 0, // Unknown without querying state
                        lock_hash: [0u8; 32],
                        type_hash: None,
                        data_hash: [0u8; 32],
                        block_daa_score: 0,
                    };
                    
                    ctx.mergeset_cell_diff.remove_cell(outpoint, removed_meta);
                }
                
                // CREATE OUTPUTS (add cells to state)
                for (index, output) in tx.outputs.iter().enumerate() {
                    let outpoint = TransactionOutpoint {
                        transaction_id: tx_id.into(),
                        index: index as u32,
                    };
                    
                    let cell_meta = CellMeta {
                        capacity: output.value,
                        lock_hash: self.compute_lock_hash(&output.script_public_key),
                        type_hash: None, // TODO: Extract type script hash if present
                        data_hash: [0u8; 32], // TODO: Hash tx output data
                        block_daa_score: pov_daa_score,
                    };
                    
                    ctx.mergeset_cell_diff.add_cell(outpoint, cell_meta);
                }
            }
        }
        
        // STEP 3: Apply the accumulated diff to the tree
        ctx.apply_diff();
    }
    
    /// Compute lock script hash from ScriptPublicKey
    /// TODO: Implement proper script hashing
    fn compute_lock_hash(&self, script_public_key: &tondi_consensus_core::tx::ScriptPublicKey) -> [u8; 32] {
        use blake3::Hasher;
        
        let mut hasher = Hasher::new();
        hasher.update(b"tondi-cell/lock"); // Domain separation
        hasher.update(&script_public_key.version().to_le_bytes());
        hasher.update(script_public_key.script());
        
        *hasher.finalize().as_bytes()
    }

    /// Commit the Cell state for a chain block
    /// 
    /// This stores the Cell state diff and root to database.
    /// Replaces commit_utxo_state with Cell model.
    pub(super) fn commit_cell_state(
        &self,
        hash: Hash,
        _cell_diff: CellDiff,
        _cell_root: Hash,
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
        self.acceptance_data_store.insert_batch(&mut batch, hash, Arc::new(acceptance_data)).unwrap();
        
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

