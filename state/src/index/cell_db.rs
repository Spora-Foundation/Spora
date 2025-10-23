// SPDX-License-Identifier: ISC
// Copyright (C) 2025 Spora developers
//
// CellDB: Cell indexing database (OutPoint → CellMeta)

use crate::{Result, StateError};
use borsh::{BorshDeserialize, BorshSerialize};
use parking_lot::RwLock;
use rocksdb::{ColumnFamilyDescriptor, Options, WriteBatch, DB};
use serde::{Deserialize, Serialize};
use spora_exec::{CellOut, OutPoint};
use std::path::Path;
use std::sync::Arc;

/// Column families
const CF_CELLS: &str = "cells";
const CF_SPENT: &str = "spent";
const CF_SPEND_JOURNAL: &str = "spend_journal"; // Full metadata for historical queries

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

/// Spend record for historical queries
///
/// Stores both when a Cell was spent and its original metadata
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SpendRecord {
    /// When the Cell was spent
    pub spent_at_daa: u64,
    /// Original Cell metadata (for historical reconstruction)
    pub cell_meta: CellMeta,
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
        let cf_spend_journal = ColumnFamilyDescriptor::new(CF_SPEND_JOURNAL, Options::default());

        let db = DB::open_cf_descriptors(&opts, path, vec![cf_cells, cf_spent, cf_spend_journal])
            .map_err(|e| StateError::Database(e.to_string()))?;

        Ok(Self { db: Arc::new(db), write_lock: Arc::new(RwLock::new(())) })
    }

    /// Get a Cell by OutPoint
    ///
    /// Returns:
    /// - Some(CellMeta) if Cell is live
    /// - None if Cell is spent or doesn't exist
    pub fn get(&self, out_point: &OutPoint) -> Result<Option<CellMeta>> {
        let cf = self.db.cf_handle(CF_CELLS).ok_or_else(|| StateError::Database("CF_CELLS not found".to_string()))?;

        let key = out_point.to_key();

        match self.db.get_cf(&cf, &key).map_err(|e| StateError::Database(e.to_string()))? {
            Some(data) => {
                let meta = CellMeta::try_from_slice(&data).map_err(|e| StateError::Serialization(e.to_string()))?;
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

        let cf = self.db.cf_handle(CF_CELLS).ok_or_else(|| StateError::Database("CF_CELLS not found".to_string()))?;

        let key = out_point.to_key();
        let value = borsh::to_vec(meta).map_err(|e| StateError::Serialization(e.to_string()))?;

        self.db.put_cf(&cf, &key, &value).map_err(|e| StateError::Database(e.to_string()))?;

        Ok(())
    }

    /// Spend a Cell
    ///
    /// Moves a Cell from live set to spent set, preserving metadata for historical queries
    pub fn spend(&self, out_point: &OutPoint, spent_at_daa: u64) -> Result<()> {
        let _lock = self.write_lock.write();

        let cf_cells = self.db.cf_handle(CF_CELLS).ok_or_else(|| StateError::Database("CF_CELLS not found".to_string()))?;
        let cf_spent = self.db.cf_handle(CF_SPENT).ok_or_else(|| StateError::Database("CF_SPENT not found".to_string()))?;
        let cf_journal =
            self.db.cf_handle(CF_SPEND_JOURNAL).ok_or_else(|| StateError::Database("CF_SPEND_JOURNAL not found".to_string()))?;

        let key = out_point.to_key();

        // Get Cell metadata before deleting
        let cell_data = self
            .db
            .get_cf(&cf_cells, &key)
            .map_err(|e| StateError::Database(e.to_string()))?
            .ok_or_else(|| StateError::CellNotFound([0; 32]))?;

        let cell_meta = CellMeta::try_from_slice(&cell_data).map_err(|e| StateError::Serialization(e.to_string()))?;

        // Create spend record
        let spend_record = SpendRecord { spent_at_daa, cell_meta };

        // Atomic update: delete from cells, add to spent + journal
        let mut batch = WriteBatch::default();
        batch.delete_cf(&cf_cells, &key);
        batch.put_cf(&cf_spent, &key, &spent_at_daa.to_le_bytes());
        batch.put_cf(&cf_journal, &key, &borsh::to_vec(&spend_record).map_err(|e| StateError::Serialization(e.to_string()))?);

        self.db.write(batch).map_err(|e| StateError::Database(e.to_string()))?;

        Ok(())
    }

    /// Check if a Cell is spent
    pub fn is_spent(&self, out_point: &OutPoint) -> Result<Option<u64>> {
        let cf = self.db.cf_handle(CF_SPENT).ok_or_else(|| StateError::Database("CF_SPENT not found".to_string()))?;

        let key = out_point.to_key();

        match self.db.get_cf(&cf, &key).map_err(|e| StateError::Database(e.to_string()))? {
            Some(data) => {
                if data.len() != 8 {
                    return Err(StateError::Serialization("Invalid DAA score".to_string()));
                }
                let daa = u64::from_le_bytes([data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7]]);
                Ok(Some(daa))
            }
            None => Ok(None),
        }
    }

    /// Batch put Cells (for block processing)
    pub fn batch_put(&self, cells: &[(OutPoint, CellMeta)]) -> Result<()> {
        let _lock = self.write_lock.write();

        let cf = self.db.cf_handle(CF_CELLS).ok_or_else(|| StateError::Database("CF_CELLS not found".to_string()))?;

        let mut batch = WriteBatch::default();

        for (out_point, meta) in cells {
            let key = out_point.to_key();
            let value = borsh::to_vec(meta).map_err(|e| StateError::Serialization(e.to_string()))?;
            batch.put_cf(&cf, &key, &value);
        }

        self.db.write(batch).map_err(|e| StateError::Database(e.to_string()))?;

        Ok(())
    }

    /// Batch spend Cells (for block processing)
    pub fn batch_spend(&self, spends: &[(OutPoint, u64)]) -> Result<()> {
        let _lock = self.write_lock.write();

        let cf_cells = self.db.cf_handle(CF_CELLS).ok_or_else(|| StateError::Database("CF_CELLS not found".to_string()))?;
        let cf_spent = self.db.cf_handle(CF_SPENT).ok_or_else(|| StateError::Database("CF_SPENT not found".to_string()))?;
        let cf_journal =
            self.db.cf_handle(CF_SPEND_JOURNAL).ok_or_else(|| StateError::Database("CF_SPEND_JOURNAL not found".to_string()))?;

        let mut batch = WriteBatch::default();

        for (out_point, spent_at_daa) in spends {
            let key = out_point.to_key();

            // Get Cell metadata before deleting
            if let Some(cell_data) = self.db.get_cf(&cf_cells, &key).map_err(|e| StateError::Database(e.to_string()))? {
                let cell_meta = CellMeta::try_from_slice(&cell_data).map_err(|e| StateError::Serialization(e.to_string()))?;

                // Create spend record
                let spend_record = SpendRecord { spent_at_daa: *spent_at_daa, cell_meta };

                batch.delete_cf(&cf_cells, &key);
                batch.put_cf(&cf_spent, &key, &spent_at_daa.to_le_bytes());
                batch.put_cf(&cf_journal, &key, &borsh::to_vec(&spend_record).map_err(|e| StateError::Serialization(e.to_string()))?);
            }
        }

        self.db.write(batch).map_err(|e| StateError::Database(e.to_string()))?;

        Ok(())
    }

    /// Get Cell state at a specific DAA score (GHOSTDAG-aware historical query)
    ///
    /// Returns the Cell if it was live at the given DAA score.
    ///
    /// Logic:
    /// - Cell must have been created at or before `at_daa`
    /// - Cell must be either:
    ///   a) Still live (in CF_CELLS), OR
    ///   b) Spent after `at_daa` (in CF_SPEND_JOURNAL with spent_at_daa > at_daa)
    ///
    /// This is critical for DAG consensus where we need to validate transactions
    /// at specific DAA scores (e.g., during reorgs).
    ///
    /// # Examples
    ///
    /// ```text
    /// Cell created at DAA 50, spent at DAA 150:
    /// - get_cell_at_daa(cell, 40) → None (not created yet)
    /// - get_cell_at_daa(cell, 100) → Some(cell) (live)
    /// - get_cell_at_daa(cell, 150) → None (spent at this point)
    /// - get_cell_at_daa(cell, 200) → None (already spent)
    /// ```
    pub fn get_cell_at_daa(&self, out_point: &OutPoint, at_daa: u64) -> Result<Option<CellMeta>> {
        let cf_cells = self.db.cf_handle(CF_CELLS).ok_or_else(|| StateError::Database("CF_CELLS not found".to_string()))?;
        let cf_journal =
            self.db.cf_handle(CF_SPEND_JOURNAL).ok_or_else(|| StateError::Database("CF_SPEND_JOURNAL not found".to_string()))?;

        let key = out_point.to_key();

        // CASE 1: Cell is currently live
        if let Some(data) = self.db.get_cf(&cf_cells, &key).map_err(|e| StateError::Database(e.to_string()))? {
            let meta = CellMeta::try_from_slice(&data).map_err(|e| StateError::Serialization(e.to_string()))?;

            // Cell exists and is live. Was it created by at_daa?
            if meta.daa_score <= at_daa {
                return Ok(Some(meta));
            } else {
                // Cell was created after at_daa, so it didn't exist yet
                return Ok(None);
            }
        }

        // CASE 2: Cell has been spent - check spend journal
        if let Some(journal_data) = self.db.get_cf(&cf_journal, &key).map_err(|e| StateError::Database(e.to_string()))? {
            let spend_record = SpendRecord::try_from_slice(&journal_data).map_err(|e| StateError::Serialization(e.to_string()))?;

            // Check creation and spend DAA scores
            let created_at = spend_record.cell_meta.daa_score;
            let spent_at = spend_record.spent_at_daa;

            // Cell was live if: created_at <= at_daa < spent_at
            if created_at <= at_daa && spent_at > at_daa {
                return Ok(Some(spend_record.cell_meta));
            } else {
                // Either not yet created or already spent
                return Ok(None);
            }
        }

        // CASE 3: Cell doesn't exist in any index
        Ok(None)
    }

    /// Batch query Cells at a specific DAA score
    ///
    /// Efficient batch version of get_cell_at_daa for transaction validation
    pub fn batch_get_at_daa(&self, out_points: &[OutPoint], at_daa: u64) -> Result<Vec<Option<CellMeta>>> {
        let mut results = Vec::with_capacity(out_points.len());

        for out_point in out_points {
            results.push(self.get_cell_at_daa(out_point, at_daa)?);
        }

        Ok(results)
    }

    /// Get database statistics
    pub fn stats(&self) -> Result<CellDBStats> {
        // Note: RocksDB's estimate_num_keys is approximate
        let cf_cells = self.db.cf_handle(CF_CELLS).ok_or_else(|| StateError::Database("CF_CELLS not found".to_string()))?;
        let cf_spent = self.db.cf_handle(CF_SPENT).ok_or_else(|| StateError::Database("CF_SPENT not found".to_string()))?;

        // Use property queries (RocksDB internal stats)
        let live_cells = self
            .db
            .property_int_value_cf(&cf_cells, "rocksdb.estimate-num-keys")
            .map_err(|e| StateError::Database(e.to_string()))?
            .unwrap_or(0);

        let spent_cells = self
            .db
            .property_int_value_cf(&cf_spent, "rocksdb.estimate-num-keys")
            .map_err(|e| StateError::Database(e.to_string()))?
            .unwrap_or(0);

        Ok(CellDBStats { live_cells, spent_cells })
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
    use spora_exec::{CellOut, ScriptRef};
    use tempfile::TempDir;

    fn create_test_cell_meta(capacity: u64, daa: u64) -> CellMeta {
        let lock = ScriptRef::new([0x00; 32], 0, vec![0; 20]);
        CellMeta {
            cell_output: CellOut { lock, type_: None, capacity },
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
        let spends = vec![(OutPoint::new([0x01; 32], 0), 200), (OutPoint::new([0x02; 32], 0), 201)];

        db.batch_spend(&spends).unwrap();

        // Verify spends
        assert!(db.get(&OutPoint::new([0x01; 32], 0)).unwrap().is_none());
        assert!(db.get(&OutPoint::new([0x02; 32], 0)).unwrap().is_none());
        assert!(db.get(&OutPoint::new([0x03; 32], 0)).unwrap().is_some());
    }

    #[test]
    fn test_get_cell_at_daa_live_cell() {
        let temp_dir = TempDir::new().unwrap();
        let db = CellDB::open(temp_dir.path()).unwrap();

        let out_point = OutPoint::new([1; 32], 0);
        let meta = create_test_cell_meta(1000, 50); // Created at DAA 50

        db.put(&out_point, &meta).unwrap();

        // Query before creation: should be None
        assert_eq!(db.get_cell_at_daa(&out_point, 40).unwrap(), None);

        // Query at creation: should be Some
        assert_eq!(db.get_cell_at_daa(&out_point, 50).unwrap(), Some(meta.clone()));

        // Query after creation: should be Some (still live)
        assert_eq!(db.get_cell_at_daa(&out_point, 100).unwrap(), Some(meta));
    }

    #[test]
    fn test_get_cell_at_daa_spent_cell() {
        let temp_dir = TempDir::new().unwrap();
        let db = CellDB::open(temp_dir.path()).unwrap();

        let out_point = OutPoint::new([2; 32], 0);
        let meta = create_test_cell_meta(1000, 50); // Created at DAA 50

        db.put(&out_point, &meta).unwrap();
        db.spend(&out_point, 150).unwrap(); // Spent at DAA 150

        // Query before creation: None
        assert_eq!(db.get_cell_at_daa(&out_point, 40).unwrap(), None);

        // Query when live (50 <= 100 < 150): Some
        let result = db.get_cell_at_daa(&out_point, 100).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().capacity, 1000);

        // Query at spend point (DAA 150): None (just spent)
        assert_eq!(db.get_cell_at_daa(&out_point, 150).unwrap(), None);

        // Query after spend: None
        assert_eq!(db.get_cell_at_daa(&out_point, 200).unwrap(), None);
    }

    #[test]
    fn test_get_cell_at_daa_reorg_scenario() {
        let temp_dir = TempDir::new().unwrap();
        let db = CellDB::open(temp_dir.path()).unwrap();

        // Simulate reorg scenario:
        // Branch A: Cell created at DAA 50, spent at DAA 150
        // Branch B: Need to validate tx at DAA 100 (Cell should be live)

        let out_point = OutPoint::new([3; 32], 0);
        let meta = create_test_cell_meta(1000, 50);

        db.put(&out_point, &meta).unwrap();
        db.spend(&out_point, 150).unwrap();

        // Reorg validation: Check if Cell was live at DAA 100
        let cell_at_100 = db.get_cell_at_daa(&out_point, 100).unwrap();
        assert!(cell_at_100.is_some());
        assert_eq!(cell_at_100.unwrap().capacity, 1000);
        assert_eq!(cell_at_100.unwrap().daa_score, 50);
    }

    #[test]
    fn test_get_cell_at_daa_fork_scenario() {
        let temp_dir = TempDir::new().unwrap();
        let db = CellDB::open(temp_dir.path()).unwrap();

        // Fork scenario:
        // Same Cell spent in different branches at different DAA scores
        // Cell created at DAA 50
        // Branch A: spent at DAA 120
        // Branch B: need to check at DAA 100 (should be live)

        let out_point = OutPoint::new([4; 32], 0);
        let meta = create_test_cell_meta(2000, 50);

        db.put(&out_point, &meta).unwrap();
        db.spend(&out_point, 120).unwrap();

        // Query at DAA 100: Cell should be live (50 <= 100 < 120)
        let result = db.get_cell_at_daa(&out_point, 100).unwrap();
        assert!(result.is_some());

        // Query at DAA 130: Cell should be spent (130 >= 120)
        let result = db.get_cell_at_daa(&out_point, 130).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_batch_get_at_daa() {
        let temp_dir = TempDir::new().unwrap();
        let db = CellDB::open(temp_dir.path()).unwrap();

        let out1 = OutPoint::new([6; 32], 0);
        let out2 = OutPoint::new([7; 32], 0);
        let out3 = OutPoint::new([8; 32], 0);

        let meta1 = create_test_cell_meta(1000, 50); // Created at 50
        let meta2 = create_test_cell_meta(2000, 60); // Created at 60
        let meta3 = create_test_cell_meta(3000, 70); // Created at 70

        db.put(&out1, &meta1).unwrap();
        db.put(&out2, &meta2).unwrap();
        db.put(&out3, &meta3).unwrap();

        // Spend meta2 at DAA 100
        db.spend(&out2, 100).unwrap();

        // Batch query at DAA 80
        let results = db.batch_get_at_daa(&[out1.clone(), out2.clone(), out3.clone()], 80).unwrap();

        assert!(results[0].is_some()); // meta1: created at 50, still live
        assert!(results[1].is_some()); // meta2: created at 60, spent at 100 (live at 80)
        assert!(results[2].is_some()); // meta3: created at 70, still live
    }
}
