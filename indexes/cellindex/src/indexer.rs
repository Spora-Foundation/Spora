// SPDX-License-Identifier: ISC
// Copyright (C) 2025Tondi developers
//
// Cell indexer service

use crate::{Result, IndexError, CellQuery, CellQueryResult, CellFilter};
use parking_lot::RwLock;
use std::path::Path;
use std::sync::Arc;
use tondi_exec::{CellTx, OutPoint};
use tondi_state::{CellDB, ScriptIndex, CellMeta};

/// Cell indexer service
///
/// Provides high-level indexing and query operations
pub struct CellIndexer {
    /// Cell database
    cell_db: Arc<CellDB>,
    
    /// Script index
    script_index: Arc<ScriptIndex>,
    
    /// Processing statistics
    stats: Arc<RwLock<IndexerStats>>,
}

/// Indexer statistics
#[derive(Debug, Clone, Default)]
pub struct IndexerStats {
    /// Total blocks processed
    pub blocks_processed: u64,
    
    /// Total transactions indexed
    pub txs_indexed: u64,
    
    /// Total Cells indexed
    pub cells_indexed: u64,
    
    /// Total queries served
    pub queries_served: u64,
}

impl CellIndexer {
    /// Create a new Cell indexer
    pub fn new<P: AsRef<Path>>(cell_db_path: P, script_index_path: P) -> Result<Self> {
        let cell_db = CellDB::open(cell_db_path)?;
        let script_index = ScriptIndex::open(script_index_path)?;
        
        Ok(Self {
            cell_db: Arc::new(cell_db),
            script_index: Arc::new(script_index),
            stats: Arc::new(RwLock::new(IndexerStats::default())),
        })
    }
    
    /// Index a transaction
    ///
    /// Adds all outputs to the index and marks inputs as spent
    pub fn index_transaction(
        &self,
        tx: &CellTx,
        daa_score: u64,
        block_hash: [u8; 32],
        is_cellbase: bool,
    ) -> Result<()> {
        let tx_hash = tondi_exec::celltx::sighash::compute_wtxid(tx);
        
        // Index outputs (new Cells)
        for (idx, output) in tx.outputs.iter().enumerate() {
            let out_point = OutPoint::new(tx_hash, idx as u32);
            let cell_data = tx.outputs_data.get(idx).cloned().unwrap_or_default();
            
            let meta = CellMeta {
                cell_output: output.clone(),
                cell_data,
                daa_score,
                block_hash,
                is_cellbase,
                segment_info: None, // TODO: link to DA segment
            };
            
            self.cell_db.put(&out_point, &meta)?;
            
            // Add to script index
            let lock_hash = output.lock.hash();
            self.script_index.add_lock(&lock_hash, &out_point)?;
            
            if let Some(ref type_script) = output.type_ {
                let type_hash = type_script.hash();
                self.script_index.add_type(&type_hash, &out_point)?;
            }
        }
        
        // Mark inputs as spent
        for input in &tx.inputs {
            self.cell_db.spend(&input.out_point, daa_score)?;
            
            // Remove from script index
            if let Some(meta) = self.cell_db.get(&input.out_point)? {
                let lock_hash = meta.cell_output.lock.hash();
                self.script_index.remove_lock(&lock_hash, &input.out_point)?;
                
                if let Some(ref type_script) = meta.cell_output.type_ {
                    let type_hash = type_script.hash();
                    self.script_index.remove_type(&type_hash, &input.out_point)?;
                }
            }
        }
        
        // Update stats
        let mut stats = self.stats.write();
        stats.txs_indexed += 1;
        stats.cells_indexed += tx.outputs.len() as u64;
        
        Ok(())
    }
    
    /// Query Cells by filter
    pub fn query(&self, query: &CellQuery) -> Result<CellQueryResult> {
        let out_points = match &query.filter {
            CellFilter::ByLock(lock_hash) => {
                self.script_index.get_by_lock(lock_hash)?
            }
            CellFilter::ByType(type_hash) => {
                self.script_index.get_by_type(type_hash)?
            }
            CellFilter::ByOutPoint(out_point) => {
                vec![out_point.clone()]
            }
        };
        
        let mut cells = Vec::new();
        
        for out_point in out_points.iter().take(query.limit) {
            if let Some(meta) = self.cell_db.get(out_point)? {
                // Apply capacity filter
                if let Some(min_cap) = query.min_capacity {
                    if meta.cell_output.capacity < min_cap {
                        continue;
                    }
                }
                if let Some(max_cap) = query.max_capacity {
                    if meta.cell_output.capacity > max_cap {
                        continue;
                    }
                }
                
                cells.push((out_point.clone(), meta));
            }
        }
        
        // Update stats
        self.stats.write().queries_served += 1;
        
        Ok(CellQueryResult {
            cells,
            total_count: cells.len(),
        })
    }
    
    /// Get a single Cell by OutPoint
    pub fn get_cell(&self, out_point: &OutPoint) -> Result<Option<CellMeta>> {
        Ok(self.cell_db.get(out_point)?)
    }
    
    /// Check if a Cell is spent
    pub fn is_spent(&self, out_point: &OutPoint) -> Result<Option<u64>> {
        Ok(self.cell_db.is_spent(out_point)?)
    }
    
    /// Get indexer statistics
    pub fn stats(&self) -> IndexerStats {
        self.stats.read().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tondi_exec::{CellRef, CellOut, ScriptRef};

    fn create_test_tx() -> CellTx {
        let lock = ScriptRef::new([0x00; 32], 0, vec![0; 20]);
        CellTx::new(
            vec![],
            vec![],
            vec![CellOut { lock, type_: None, capacity: 1000 }],
            vec![vec![0xAA; 100]],
            vec![],
        ).unwrap()
    }

    #[test]
    fn test_indexer_creation() {
        let tmp_db = TempDir::new().unwrap();
        let tmp_script = TempDir::new().unwrap();
        
        let _indexer = CellIndexer::new(tmp_db.path(), tmp_script.path()).unwrap();
    }

    #[test]
    fn test_index_transaction() {
        let tmp_db = TempDir::new().unwrap();
        let tmp_script = TempDir::new().unwrap();
        let indexer = CellIndexer::new(tmp_db.path(), tmp_script.path()).unwrap();
        
        let tx = create_test_tx();
        indexer.index_transaction(&tx, 100, [0x42; 32], false).unwrap();
        
        let stats = indexer.stats();
        assert_eq!(stats.txs_indexed, 1);
        assert_eq!(stats.cells_indexed, 1);
    }

    #[test]
    fn test_query_by_lock() {
        let tmp_db = TempDir::new().unwrap();
        let tmp_script = TempDir::new().unwrap();
        let indexer = CellIndexer::new(tmp_db.path(), tmp_script.path()).unwrap();
        
        let lock = ScriptRef::new([0x00; 32], 0, vec![0; 20]);
        let tx = create_test_tx();
        
        indexer.index_transaction(&tx, 100, [0x42; 32], false).unwrap();
        
        let lock_hash = lock.hash();
        let query = CellQuery {
            filter: CellFilter::ByLock(lock_hash),
            limit: 10,
            min_capacity: None,
            max_capacity: None,
        };
        
        let result = indexer.query(&query).unwrap();
        assert_eq!(result.total_count, 1);
    }

    #[test]
    fn test_capacity_filter() {
        let tmp_db = TempDir::new().unwrap();
        let tmp_script = TempDir::new().unwrap();
        let indexer = CellIndexer::new(tmp_db.path(), tmp_script.path()).unwrap();
        
        let lock = ScriptRef::new([0x00; 32], 0, vec![0; 20]);
        let tx = create_test_tx();
        
        indexer.index_transaction(&tx, 100, [0x42; 32], false).unwrap();
        
        let lock_hash = lock.hash();
        
        // Query with min_capacity filter
        let query = CellQuery {
            filter: CellFilter::ByLock(lock_hash),
            limit: 10,
            min_capacity: Some(500),
            max_capacity: Some(2000),
        };
        
        let result = indexer.query(&query).unwrap();
        assert_eq!(result.total_count, 1); // Should match (capacity=1000)
        
        // Query with too high min_capacity
        let query_high = CellQuery {
            filter: CellFilter::ByLock(lock_hash),
            limit: 10,
            min_capacity: Some(5000),
            max_capacity: None,
        };
        
        let result_high = indexer.query(&query_high).unwrap();
        assert_eq!(result_high.total_count, 0); // Should not match
    }
}

