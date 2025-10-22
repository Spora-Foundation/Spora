// SPDX-License-Identifier: ISC
// Copyright (C) 2025Tondi developers
//
// CellDB: Cell indexing database (OutPoint → CellMeta)

use crate::{Result, StateError};
use borsh::{BorshDeserialize, BorshSerialize};
use parking_lot::RwLock;
use rocksdb::{ColumnFamilyDescriptor, Options, WriteBatch, DB};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tondi_exec::{CellOut, OutPoint};

/// Column families
const CF_CELLS: &str = "cells";
const CF_SPENT: &str = "spent";

/// Cell metadata (stored in CellDB)
///
/// Maps OutPoint → CellMeta for quick Cell lookups
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct CellMeta {
    /// Cell output structure
    pub cell_output: CellOut,
    /// Cell data (may be large, consider storing separately in DA layer)
    pub cell_data: Vec<u8>,
    /// DAA score at creation
    pub daa_score: u64,
    /// Block hash containing this Cell
    pub block_hash: [u8; 32],
    /// Is this a cellbase?
    pub is_cellbase: bool,
    /// DA segment info (optional)
    pub segment_info: Option<SegmentInfo>,
}

/// Segment storage information
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct SegmentInfo {
    /// Segment ID
    pub segment_id: u32,
    /// Offset within segment
    pub offset: u64,
    /// Length of data
    pub length: u32,
}

/// Cell database
///
/// Responsibilities:
/// - Store live Cells (OutPoint → CellMeta)
/// - Track spent Cells (OutPoint → DAA score)
/// - Support efficient queries
pub struct CellDB {
    /// RocksDB instance
    db: Arc<DB>,
    /// Write lock for atomic updates
    write_lock: Arc<RwLock<()>>,
}

impl CellDB {
    /// Open or create a CellDB
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);
        opts.set_compression_type(rocksdb::DBCompressionType::Snappy);
        opts.increase_parallelism(4);
        
        // Define column families
        let cf_cells = ColumnFamilyDescriptor::new(CF_CELLS, Options::default());
        let cf_spent = ColumnFamilyDescriptor::new(CF_SPENT, Options::default());
        
        let db = DB::open_cf_descriptors(&opts, path, vec![cf_cells, cf_spent])
            .map_err(|e| StateError::Database(e.to_string()))?;
        
        Ok(Self {
            db: Arc::new(db),
            write_lock: Arc::new(RwLock::new(())),
        })
    }
    
    /// Get a Cell by OutPoint
    ///
    /// Returns:
    /// - Some(CellMeta) if Cell is live
    /// - None if Cell is spent or doesn't exist
    pub fn get(&self, out_point: &OutPoint) -> Result<Option<CellMeta>> {
        let cf = self.db.cf_handle(CF_CELLS)
            .ok_or_else(|| StateError::Database("CF_CELLS not found".to_string()))?;
        
        let key = out_point.to_key();
        
        match self.db.get_cf(&cf, &key)
            .map_err(|e| StateError::Database(e.to_string()))? {
            Some(data) => {
                let meta = CellMeta::try_from_slice(&data)
                    .map_err(|e| StateError::Serialization(e.to_string()))?;
                Ok(Some(meta))
            }
            None => Ok(None),
        }
    }
    
    /// Put a new Cell
    ///
    /// Adds a Cell to the live set
    pub fn put(&self, out_point: &OutPoint, meta: &CellMeta) -> Result<()> {
        let _lock = self.write_lock.write();
        
        let cf = self.db.cf_handle(CF_CELLS)
            .ok_or_else(|| StateError::Database("CF_CELLS not found".to_string()))?;
        
        let key = out_point.to_key();
        let value = borsh::to_vec(meta)
            .map_err(|e| StateError::Serialization(e.to_string()))?;
        
        self.db.put_cf(&cf, &key, &value)
            .map_err(|e| StateError::Database(e.to_string()))?;
        
        Ok(())
    }
    
    /// Spend a Cell
    ///
    /// Moves a Cell from live set to spent set
    pub fn spend(&self, out_point: &OutPoint, spent_at_daa: u64) -> Result<()> {
        let _lock = self.write_lock.write();
        
        let cf_cells = self.db.cf_handle(CF_CELLS)
            .ok_or_else(|| StateError::Database("CF_CELLS not found".to_string()))?;
        let cf_spent = self.db.cf_handle(CF_SPENT)
            .ok_or_else(|| StateError::Database("CF_SPENT not found".to_string()))?;
        
        let key = out_point.to_key();
        
        // Check if Cell exists
        if self.db.get_cf(&cf_cells, &key)
            .map_err(|e| StateError::Database(e.to_string()))?.is_none() {
            return Err(StateError::CellNotFound([0; 32])); // FIXME: proper hash
        }
        
        // Atomic update: delete from cells, add to spent
        let mut batch = WriteBatch::default();
        batch.delete_cf(&cf_cells, &key);
        batch.put_cf(&cf_spent, &key, &spent_at_daa.to_le_bytes());
        
        self.db.write(batch)
            .map_err(|e| StateError::Database(e.to_string()))?;
        
        Ok(())
    }
    
    /// Check if a Cell is spent
    pub fn is_spent(&self, out_point: &OutPoint) -> Result<Option<u64>> {
        let cf = self.db.cf_handle(CF_SPENT)
            .ok_or_else(|| StateError::Database("CF_SPENT not found".to_string()))?;
        
        let key = out_point.to_key();
        
        match self.db.get_cf(&cf, &key)
            .map_err(|e| StateError::Database(e.to_string()))? {
            Some(data) => {
                if data.len() != 8 {
                    return Err(StateError::Serialization("Invalid DAA score".to_string()));
                }
                let daa = u64::from_le_bytes([
                    data[0], data[1], data[2], data[3],
                    data[4], data[5], data[6], data[7],
                ]);
                Ok(Some(daa))
            }
            None => Ok(None),
        }
    }
    
    /// Batch put Cells (for block processing)
    pub fn batch_put(&self, cells: &[(OutPoint, CellMeta)]) -> Result<()> {
        let _lock = self.write_lock.write();
        
        let cf = self.db.cf_handle(CF_CELLS)
            .ok_or_else(|| StateError::Database("CF_CELLS not found".to_string()))?;
        
        let mut batch = WriteBatch::default();
        
        for (out_point, meta) in cells {
            let key = out_point.to_key();
            let value = borsh::to_vec(meta)
                .map_err(|e| StateError::Serialization(e.to_string()))?;
            batch.put_cf(&cf, &key, &value);
        }
        
        self.db.write(batch)
            .map_err(|e| StateError::Database(e.to_string()))?;
        
        Ok(())
    }
    
    /// Batch spend Cells (for block processing)
    pub fn batch_spend(&self, spends: &[(OutPoint, u64)]) -> Result<()> {
        let _lock = self.write_lock.write();
        
        let cf_cells = self.db.cf_handle(CF_CELLS)
            .ok_or_else(|| StateError::Database("CF_CELLS not found".to_string()))?;
        let cf_spent = self.db.cf_handle(CF_SPENT)
            .ok_or_else(|| StateError::Database("CF_SPENT not found".to_string()))?;
        
        let mut batch = WriteBatch::default();
        
        for (out_point, spent_at_daa) in spends {
            let key = out_point.to_key();
            batch.delete_cf(&cf_cells, &key);
            batch.put_cf(&cf_spent, &key, &spent_at_daa.to_le_bytes());
        }
        
        self.db.write(batch)
            .map_err(|e| StateError::Database(e.to_string()))?;
        
        Ok(())
    }
    
    /// Get database statistics
    pub fn stats(&self) -> Result<CellDBStats> {
        // Note: RocksDB's estimate_num_keys is approximate
        let cf_cells = self.db.cf_handle(CF_CELLS)
            .ok_or_else(|| StateError::Database("CF_CELLS not found".to_string()))?;
        let cf_spent = self.db.cf_handle(CF_SPENT)
            .ok_or_else(|| StateError::Database("CF_SPENT not found".to_string()))?;
        
        // Use property queries (RocksDB internal stats)
        let live_cells = self.db.property_int_value_cf(&cf_cells, "rocksdb.estimate-num-keys")
            .map_err(|e| StateError::Database(e.to_string()))?
            .unwrap_or(0);
        
        let spent_cells = self.db.property_int_value_cf(&cf_spent, "rocksdb.estimate-num-keys")
            .map_err(|e| StateError::Database(e.to_string()))?
            .unwrap_or(0);
        
        Ok(CellDBStats {
            live_cells,
            spent_cells,
        })
    }
}

/// CellDB statistics
#[derive(Debug, Clone, Copy)]
pub struct CellDBStats {
    /// Number of live Cells
    pub live_cells: u64,
    /// Number of spent Cells
    pub spent_cells: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tondi_exec::{ScriptRef, CellOut};

    fn create_test_cell_meta(capacity: u64, daa: u64) -> CellMeta {
        let lock = ScriptRef::new([0x00; 32], 0, vec![0; 20]);
        CellMeta {
            cell_output: CellOut {
                lock,
                type_: None,
                capacity,
            },
            cell_data: vec![0xAA; 100],
            daa_score: daa,
            block_hash: [0x11; 32],
            is_cellbase: false,
            segment_info: None,
        }
    }

    #[test]
    fn test_cell_db_open() {
        let tmp = TempDir::new().unwrap();
        let db = CellDB::open(tmp.path()).unwrap();
        let stats = db.stats().unwrap();
        assert_eq!(stats.live_cells, 0);
        assert_eq!(stats.spent_cells, 0);
    }

    #[test]
    fn test_put_get_cell() {
        let tmp = TempDir::new().unwrap();
        let db = CellDB::open(tmp.path()).unwrap();
        
        let out_point = OutPoint::new([0x42; 32], 0);
        let meta = create_test_cell_meta(1000, 100);
        
        db.put(&out_point, &meta).unwrap();
        
        let retrieved = db.get(&out_point).unwrap().unwrap();
        assert_eq!(retrieved, meta);
    }

    #[test]
    fn test_spend_cell() {
        let tmp = TempDir::new().unwrap();
        let db = CellDB::open(tmp.path()).unwrap();
        
        let out_point = OutPoint::new([0x42; 32], 0);
        let meta = create_test_cell_meta(1000, 100);
        
        db.put(&out_point, &meta).unwrap();
        db.spend(&out_point, 200).unwrap();
        
        // Cell should no longer be in live set
        assert!(db.get(&out_point).unwrap().is_none());
        
        // Cell should be marked as spent
        assert_eq!(db.is_spent(&out_point).unwrap(), Some(200));
    }

    #[test]
    fn test_batch_operations() {
        let tmp = TempDir::new().unwrap();
        let db = CellDB::open(tmp.path()).unwrap();
        
        let cells = vec![
            (OutPoint::new([0x01; 32], 0), create_test_cell_meta(1000, 100)),
            (OutPoint::new([0x02; 32], 0), create_test_cell_meta(2000, 101)),
            (OutPoint::new([0x03; 32], 0), create_test_cell_meta(3000, 102)),
        ];
        
        db.batch_put(&cells).unwrap();
        
        // Verify all Cells are stored
        for (out_point, meta) in &cells {
            let retrieved = db.get(out_point).unwrap().unwrap();
            assert_eq!(&retrieved, meta);
        }
        
        // Spend first two Cells
        let spends = vec![
            (OutPoint::new([0x01; 32], 0), 200),
            (OutPoint::new([0x02; 32], 0), 201),
        ];
        
        db.batch_spend(&spends).unwrap();
        
        // Verify spends
        assert!(db.get(&OutPoint::new([0x01; 32], 0)).unwrap().is_none());
        assert!(db.get(&OutPoint::new([0x02; 32], 0)).unwrap().is_none());
        assert!(db.get(&OutPoint::new([0x03; 32], 0)).unwrap().is_some());
    }
}

