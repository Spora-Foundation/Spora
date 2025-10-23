// SPDX-License-Identifier: ISC
// Copyright (C) 2025 Spora developers
//
// Cell query API

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use spora_exec::OutPoint;
use spora_state::index::CellMeta;
use crate::indexer::CellIndexer;

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
        Self {
            filter: CellFilter::ByLock(lock_hash),
            limit,
            min_capacity: None,
            max_capacity: None,
        }
    }
    
    /// Create a query by type script hash
    pub fn by_type(type_hash: [u8; 32], limit: usize) -> Self {
        Self {
            filter: CellFilter::ByType(type_hash),
            limit,
            min_capacity: None,
            max_capacity: None,
        }
    }
    
    /// Create a query by OutPoint
    pub fn by_outpoint(out_point: OutPoint) -> Self {
        Self {
            filter: CellFilter::ByOutPoint(out_point),
            limit: 1,
            min_capacity: None,
            max_capacity: None,
        }
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
    
    /// Update index with Cell diff (async version)
    /// 
    /// GHOSTDAG-aware: processes accumulated Cell diff from consensus notifications
    pub async fn update_with_diff(&self, diff: &spora_consensus_core::cell_diff::CellDiff) -> crate::Result<()> {
        // Run in blocking thread pool since DB operations are sync
        let indexer = self.indexer.clone();
        let diff = diff.clone();
        
        tokio::task::spawn_blocking(move || {
            indexer.update_with_diff(&diff)
        })
        .await
        .map_err(|e| crate::errors::CellIndexError::Internal(format!("Async task error: {}", e)))?
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
        let query = CellQuery::by_lock([0x42; 32], 10)
            .with_capacity_range(Some(1000), Some(5000));
        
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
}

