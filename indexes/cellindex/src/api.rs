// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Spora developers
//
// Cell query API

use crate::indexer::{CellDataProof, CellIndexer};
use serde::{Deserialize, Serialize};
use spora_consensus_core::cell_diff::{BlockCellDiff, CellCollection};
use spora_exec::{CellTx, OutPoint};
use spora_state::index::CellMeta;
use std::sync::Arc;

/// Cell query request
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CellQuery {
    /// Filter criteria
    pub filter: CellFilter,

    /// Maximum results
    pub limit: usize,

    /// Minimum capacity filter (optional)
    pub min_capacity: Option<u64>,

    /// Maximum capacity filter (optional)
    pub max_capacity: Option<u64>,
}

/// Cell filter criteria
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CellFilter {
    /// Filter by lock script hash
    ByLock([u8; 32]),

    /// Filter by type script hash
    ByType([u8; 32]),

    /// Get specific Cell by OutPoint
    ByOutPoint(OutPoint),
}

/// Cell query result
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CellQueryResult {
    /// Matching Cells: (OutPoint, CellMeta)
    pub cells: Vec<(OutPoint, CellMeta)>,

    /// Total count
    pub total_count: usize,
}

impl CellQuery {
    /// Create a query by lock script hash
    pub fn by_lock(lock_hash: [u8; 32], limit: usize) -> Self {
        Self { filter: CellFilter::ByLock(lock_hash), limit, min_capacity: None, max_capacity: None }
    }

    /// Create a query by type script hash
    pub fn by_type(type_hash: [u8; 32], limit: usize) -> Self {
        Self { filter: CellFilter::ByType(type_hash), limit, min_capacity: None, max_capacity: None }
    }

    /// Create a query by OutPoint
    pub fn by_outpoint(out_point: OutPoint) -> Self {
        Self { filter: CellFilter::ByOutPoint(out_point), limit: 1, min_capacity: None, max_capacity: None }
    }

    /// Add capacity range filter
    pub fn with_capacity_range(mut self, min: Option<u64>, max: Option<u64>) -> Self {
        self.min_capacity = min;
        self.max_capacity = max;
        self
    }
}

/// Cell index proxy for async operations
///
/// Provides a thread-safe wrapper around CellIndexer for use in services
#[derive(Clone)]
pub struct CellIndexProxy {
    indexer: Arc<CellIndexer>,
}

impl CellIndexProxy {
    /// Create a new Cell index proxy
    pub fn new(indexer: Arc<CellIndexer>) -> Self {
        Self { indexer }
    }

    /// Query cells
    pub fn query(&self, query: &CellQuery) -> crate::Result<CellQueryResult> {
        self.indexer.query(query)
    }

    /// Get a single cell by OutPoint
    pub fn get_cell(&self, out_point: &OutPoint) -> crate::Result<Option<CellMeta>> {
        self.indexer.get_cell(out_point)
    }

    /// Build a DA proof for a segment-backed cell payload.
    pub fn get_cell_data_proof(&self, out_point: &OutPoint) -> crate::Result<Option<CellDataProof>> {
        self.indexer.get_cell_data_proof(out_point)
    }

    /// Sum the capacity of all currently live cells tracked by the index.
    pub fn total_live_capacity(&self) -> crate::Result<u64> {
        self.indexer.total_live_capacity()
    }

    /// Returns true when the index contains no live cells yet.
    pub fn is_empty(&self) -> crate::Result<bool> {
        self.indexer.is_empty()
    }

    /// Seed the index directly from a live cell collection.
    pub fn seed_cell_collection(&self, cells: &CellCollection, block_hash: [u8; 32]) -> crate::Result<()> {
        self.indexer.seed_cell_collection(cells, block_hash)
    }

    /// Index a single transaction synchronously.
    pub fn index_transaction(&self, tx: &CellTx, daa_score: u64, block_hash: [u8; 32], is_cellbase: bool) -> crate::Result<()> {
        self.indexer.index_transaction(tx, daa_score, block_hash, is_cellbase)
    }

    /// Update index with Cell diff (async version)
    ///
    /// GHOSTDAG-aware: processes accumulated Cell diff from consensus notifications
    pub async fn update_with_diff(&self, diff: &spora_consensus_core::cell_diff::CellDiff) -> crate::Result<()> {
        // Run in blocking thread pool since DB operations are sync
        let indexer = self.indexer.clone();
        let diff = diff.clone();

        tokio::task::spawn_blocking(move || indexer.update_with_diff(&diff))
            .await
            .map_err(|e| crate::errors::CellIndexError::Internal(format!("Async task error: {}", e)))?
    }

    /// Update index with block-aware Cell diff provenance (async version).
    pub async fn update_with_block_diffs(&self, block_diffs: &[BlockCellDiff]) -> crate::Result<()> {
        let indexer = self.indexer.clone();
        let block_diffs = block_diffs.to_vec();

        tokio::task::spawn_blocking(move || indexer.update_with_block_diffs(&block_diffs))
            .await
            .map_err(|e| crate::errors::CellIndexError::Internal(format!("Async task error: {}", e)))?
    }
}

pub trait CellIndexApi {
    fn query(&self, query: &CellQuery) -> crate::Result<CellQueryResult>;
    fn get_cell(&self, out_point: &OutPoint) -> crate::Result<Option<CellMeta>>;
    fn get_cell_data_proof(&self, out_point: &OutPoint) -> crate::Result<Option<CellDataProof>>;
    fn total_live_capacity(&self) -> crate::Result<u64>;
    fn is_empty(&self) -> crate::Result<bool>;
}

impl CellIndexApi for CellIndexProxy {
    fn query(&self, query: &CellQuery) -> crate::Result<CellQueryResult> {
        CellIndexProxy::query(self, query)
    }

    fn get_cell(&self, out_point: &OutPoint) -> crate::Result<Option<CellMeta>> {
        CellIndexProxy::get_cell(self, out_point)
    }

    fn get_cell_data_proof(&self, out_point: &OutPoint) -> crate::Result<Option<CellDataProof>> {
        CellIndexProxy::get_cell_data_proof(self, out_point)
    }

    fn total_live_capacity(&self) -> crate::Result<u64> {
        CellIndexProxy::total_live_capacity(self)
    }

    fn is_empty(&self) -> crate::Result<bool> {
        CellIndexProxy::is_empty(self)
    }
}

impl std::fmt::Debug for CellIndexProxy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CellIndexProxy").finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spora_exec::{CellOutput, CellTx, Script};
    use std::sync::Arc;
    use tempfile::TempDir;

    #[test]
    fn test_cell_query_creation() {
        let query = CellQuery::by_lock([0x42; 32], 10);

        match query.filter {
            CellFilter::ByLock(hash) => assert_eq!(hash, [0x42; 32]),
            _ => panic!("Wrong filter type"),
        }

        assert_eq!(query.limit, 10);
    }

    #[test]
    fn test_capacity_filter() {
        let query = CellQuery::by_lock([0x42; 32], 10).with_capacity_range(Some(1000), Some(5000));

        assert_eq!(query.min_capacity, Some(1000));
        assert_eq!(query.max_capacity, Some(5000));
    }

    #[test]
    fn test_query_by_outpoint() {
        let out_point = OutPoint::new([0x11; 32], 0);
        let query = CellQuery::by_outpoint(out_point.clone());

        match query.filter {
            CellFilter::ByOutPoint(op) => assert_eq!(op, out_point),
            _ => panic!("Wrong filter type"),
        }

        assert_eq!(query.limit, 1);
    }

    #[test]
    fn test_proxy_get_cell_data_proof() {
        let tmp_db = TempDir::new().unwrap();
        let tmp_script = TempDir::new().unwrap();
        let indexer = Arc::new(CellIndexer::new(tmp_db.path(), tmp_script.path()).unwrap());
        let proxy = CellIndexProxy::new(indexer);

        let lock = Script::new([0x00; 32], 0, vec![0; 20]);
        let tx =
            CellTx::new(vec![], vec![], vec![CellOutput { lock, type_: None, capacity: 1000 }], vec![vec![0xAB; 32]], vec![]).unwrap();
        proxy.index_transaction(&tx, 100, [0x44; 32], false).unwrap();

        let out_point = OutPoint::new(spora_exec::celltx::sighash::compute_wtxid(&tx), 0);
        let data_proof = proxy.get_cell_data_proof(&out_point).unwrap().unwrap();

        assert_eq!(data_proof.out_point, out_point);
        assert_eq!(data_proof.payload, vec![0xAB; 32]);
        assert!(data_proof.proof.verify().unwrap());
    }
}
