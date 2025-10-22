// SPDX-License-Identifier: ISC
// Copyright (C) 2025 Tondi developers
//
// Consensus Cell Provider - GHOSTDAG-aware Cell state queries

use crate::{
    model::{
        services::reachability::MTReachabilityService,
        stores::{
            block_transactions::BlockTransactionsStoreReader,
            cell_diffs::CellDiffsStoreReader,
            cell_roots::CellRootsStoreReader,
            ghostdag::GhostdagStoreReader,
            headers::HeaderStoreReader,
            reachability::ReachabilityStoreReader,
            statuses::StatusesStoreReader,
        },
    },
    processes::{CellStateProvider, DagCellProvider},
};
use parking_lot::RwLock;
use std::sync::Arc;
use tondi_consensus_core::{
    cell_diff::{CellDiff, CellMeta},
    cell_metadata::CellMetadata,
    tx::TransactionOutpoint,
};
use tondi_exec::OutPoint;
use tondi_hashes::Hash;

/// Consensus Cell Provider
/// 
/// Provides Cell state queries with GHOSTDAG awareness:
/// - Current Cell availability
/// - Historical Cell state at specific DAA scores
/// - Cell metadata lookup
pub struct ConsensusCellProvider<
    T: GhostdagStoreReader,
    U: ReachabilityStoreReader,
    V: HeaderStoreReader,
    W: CellDiffsStoreReader,
    X: CellRootsStoreReader,
    Y: BlockTransactionsStoreReader,
    Z: StatusesStoreReader,
> {
    ghostdag_store: Arc<T>,
    reachability_service: MTReachabilityService<U>,
    headers_store: Arc<V>,
    cell_diffs_store: Arc<W>,
    cell_roots_store: Arc<X>,
    block_transactions_store: Arc<Y>,
    statuses_store: Arc<RwLock<Z>>,
}

impl<
    T: GhostdagStoreReader,
    U: ReachabilityStoreReader,
    V: HeaderStoreReader,
    W: CellDiffsStoreReader,
    X: CellRootsStoreReader,
    Y: BlockTransactionsStoreReader,
    Z: StatusesStoreReader,
> ConsensusCellProvider<T, U, V, W, X, Y, Z>
{
    /// Create a new consensus cell provider
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ghostdag_store: Arc<T>,
        reachability_service: MTReachabilityService<U>,
        headers_store: Arc<V>,
        cell_diffs_store: Arc<W>,
        cell_roots_store: Arc<X>,
        block_transactions_store: Arc<Y>,
        statuses_store: Arc<RwLock<Z>>,
    ) -> Self {
        Self {
            ghostdag_store,
            reachability_service,
            headers_store,
            cell_diffs_store,
            cell_roots_store,
            block_transactions_store,
            statuses_store,
        }
    }

    /// Find the block that created this Cell
    /// 
    /// Searches through the DAG to find the transaction output.
    /// Returns (block_hash, tx_index, output_index)
    fn find_cell_creator(
        &self,
        outpoint: &TransactionOutpoint,
    ) -> Result<Option<(Hash, usize, usize)>, String> {
        // TODO: Implement efficient cell creator lookup
        // For now, return None (would need transaction index)
        Ok(None)
    }

    /// Check if a Cell exists in the DAG's past
    /// 
    /// GHOSTDAG-aware: verifies the creating block is in the past
    #[allow(dead_code)]
    fn is_cell_in_dag_past(
        &self,
        cell_block: Hash,
        current_tip: Hash,
    ) -> Result<bool, String> {
        // Check reachability: is cell_block in the past of current_tip?
        // Use the reachability service's chain iterator to verify
        let mut iter = self.reachability_service.backward_chain_iterator(current_tip, cell_block, true);
        
        // If we can iterate from current_tip back to cell_block, it's in the past
        Ok(iter.any(|h| h == cell_block))
    }
}

impl<
    T: GhostdagStoreReader,
    U: ReachabilityStoreReader,
    V: HeaderStoreReader,
    W: CellDiffsStoreReader,
    X: CellRootsStoreReader,
    Y: BlockTransactionsStoreReader,
    Z: StatusesStoreReader,
> CellStateProvider for ConsensusCellProvider<T, U, V, W, X, Y, Z>
{
    /// Check if a Cell is available (exists and unspent) at given DAA score
    /// 
    /// GHOSTDAG-aware implementation:
    /// 1. Find the block that created this Cell
    /// 2. Check if that block is in the past of the virtual tip at this DAA
    /// 3. Check if the Cell has been spent in any subsequent blocks
    fn is_cell_available(&self, out_point: &OutPoint, daa: u64) -> Result<bool, String> {
        // Convert exec::OutPoint to consensus-core::TransactionOutpoint
        let outpoint = TransactionOutpoint {
            transaction_id: tondi_consensus_core::Hash::from_bytes(out_point.tx_hash).into(),
            index: out_point.index,
        };

        // Find the block that created this Cell
        let creator_info = self.find_cell_creator(&outpoint)?;
        
        if creator_info.is_none() {
            // Cell not found in DAG
            return Ok(false);
        }

        let (creator_block, _tx_idx, _out_idx) = creator_info.unwrap();

        // Get the creator block's DAA score
        let creator_header = self.headers_store.get_header(creator_block)
            .map_err(|e| format!("Header lookup error: {}", e))?;
        
        // Cell must have been created before or at the query DAA
        if creator_header.daa_score > daa {
            return Ok(false);
        }

        // TODO: Check if Cell has been spent
        // For now, we assume if it was created, it's still available
        // This needs proper spent tracking via cell_diffs

        Ok(true)
    }

    /// Get Cell capacity
    /// 
    /// Looks up the Cell in the transaction outputs
    fn get_cell_capacity(&self, out_point: &OutPoint) -> Result<Option<u64>, String> {
        // Convert to TransactionOutpoint
        let outpoint = TransactionOutpoint {
            transaction_id: tondi_consensus_core::Hash::from_bytes(out_point.tx_hash).into(),
            index: out_point.index,
        };

        // Find creator
        let creator_info = self.find_cell_creator(&outpoint)?;
        if creator_info.is_none() {
            return Ok(None);
        }

        let (creator_block, tx_idx, out_idx) = creator_info.unwrap();

        // Get the transaction
        let transactions = self.block_transactions_store.get(creator_block)
            .map_err(|e| format!("Transaction lookup error: {}", e))?;

        if tx_idx >= transactions.len() {
            return Ok(None);
        }

        let tx = &transactions[tx_idx];
        if out_idx >= tx.outputs.len() {
            return Ok(None);
        }

        Ok(Some(tx.outputs[out_idx].value))
    }
}

impl<
    T: GhostdagStoreReader,
    U: ReachabilityStoreReader,
    V: HeaderStoreReader,
    W: CellDiffsStoreReader,
    X: CellRootsStoreReader,
    Y: BlockTransactionsStoreReader,
    Z: StatusesStoreReader,
> DagCellProvider for ConsensusCellProvider<T, U, V, W, X, Y, Z>
{
    /// Get complete Cell metadata
    /// 
    /// Includes DAG-specific information (is_cellbase, block_hash)
    fn get_cell_metadata(&self, out_point: &OutPoint) -> Result<Option<CellMetadata>, String> {
        // Convert to TransactionOutpoint
        let outpoint = TransactionOutpoint {
            transaction_id: tondi_consensus_core::Hash::from_bytes(out_point.tx_hash).into(),
            index: out_point.index,
        };

        // Find creator
        let creator_info = self.find_cell_creator(&outpoint)?;
        if creator_info.is_none() {
            return Ok(None);
        }

        let (creator_block, tx_idx, out_idx) = creator_info.unwrap();

        // Get the transaction
        let transactions = self.block_transactions_store.get(creator_block)
            .map_err(|e| format!("Transaction lookup error: {}", e))?;

        if tx_idx >= transactions.len() {
            return Ok(None);
        }

        let tx = &transactions[tx_idx];
        if out_idx >= tx.outputs.len() {
            return Ok(None);
        }

        let output = &tx.outputs[out_idx];
        
        // Get block header for DAA score
        let header = self.headers_store.get_header(creator_block)
            .map_err(|e| format!("Header lookup error: {}", e))?;

        // Build CellMetadata
        let metadata = CellMetadata {
            capacity: output.value,
            lock_hash: Self::compute_lock_hash(&output.script_public_key),
            type_hash: None, // TODO: Extract from script if present
            data_hash: [0u8; 32], // TODO: Hash output data
            block_daa_score: header.daa_score,
            is_cellbase: tx_idx == 0, // First tx in block is coinbase
            block_hash: creator_block,
            lock_code_hash: None,
            type_code_hash: None,
            data: None,
        };

        Ok(Some(metadata))
    }

    /// Get Cell state at a specific DAA score (GHOSTDAG-aware)
    /// 
    /// This is the key method for reorg-safe queries.
    /// Returns the Cell metadata as it existed at the given DAA score.
    fn get_cell_at_daa(&self, out_point: &OutPoint, target_daa: u64) -> Result<Option<CellMetadata>, String> {
        // First check if cell exists
        let metadata = self.get_cell_metadata(out_point)?;
        
        if metadata.is_none() {
            return Ok(None);
        }

        let metadata = metadata.unwrap();

        // Check if Cell was created before target DAA
        if metadata.block_daa_score > target_daa {
            // Cell didn't exist yet at target DAA
            return Ok(None);
        }

        // TODO: Check if Cell was spent before target DAA
        // This requires traversing cell_diffs from creator to target DAA

        // For now, if it was created, assume it exists
        Ok(Some(metadata))
    }
}

impl<
    T: GhostdagStoreReader,
    U: ReachabilityStoreReader,
    V: HeaderStoreReader,
    W: CellDiffsStoreReader,
    X: CellRootsStoreReader,
    Y: BlockTransactionsStoreReader,
    Z: StatusesStoreReader,
> ConsensusCellProvider<T, U, V, W, X, Y, Z>
{
    /// Compute lock script hash from ScriptPublicKey
    fn compute_lock_hash(script_public_key: &tondi_consensus_core::tx::ScriptPublicKey) -> [u8; 32] {
        use blake3::Hasher;
        
        let mut hasher = Hasher::new();
        hasher.update(b"tondi-cell/lock"); // Domain separation
        hasher.update(&script_public_key.version().to_le_bytes());
        hasher.update(script_public_key.script());
        
        *hasher.finalize().as_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tondi_consensus_core::tx::ScriptPublicKey;
    use crate::model::stores::ghostdag::DbGhostdagStore;
    use crate::model::stores::reachability::DbReachabilityStore;
    use crate::model::stores::headers::DbHeadersStore;
    use crate::model::stores::cell_diffs::DbCellDiffsStore;
    use crate::model::stores::cell_roots::DbCellRootsStore;
    use crate::model::stores::block_transactions::DbBlockTransactionsStore;
    use crate::model::stores::statuses::DbStatusesStore;
    
    // Type alias for the full provider type
    type TestProvider = ConsensusCellProvider<
        DbGhostdagStore,
        DbReachabilityStore,
        DbHeadersStore,
        DbCellDiffsStore,
        DbCellRootsStore,
        DbBlockTransactionsStore,
        DbStatusesStore,
    >;
    
    // Note: Full integration tests require mock stores
    // Unit tests focus on logic verification
    
    #[test]
    fn test_compute_lock_hash_deterministic() {
        let script = ScriptPublicKey::from_vec(0, vec![0x76, 0xa9, 0x14]); // Example P2PKH prefix
        let hash1 = TestProvider::compute_lock_hash(&script);
        let hash2 = TestProvider::compute_lock_hash(&script);
        
        assert_eq!(hash1, hash2, "Lock hash should be deterministic");
    }
}

