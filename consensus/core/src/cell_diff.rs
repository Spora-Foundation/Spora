// SPDX-License-Identifier: ISC
// Copyright (C) 2025 Spora developers
//
// Cell state difference - tracks additions and removals of cells

use crate::tx::TransactionOutpoint;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use spora_utils::mem_size::MemSizeEstimator;

/// Cell metadata (simplified for diff tracking)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellMeta {
    /// Cell capacity in saus
    pub capacity: u64,
    /// Lock script code hash
    pub lock_hash: [u8; 32],
    /// Type script code hash (if present)
    pub type_hash: Option<[u8; 32]>,
    /// Data hash
    pub data_hash: [u8; 32],
    /// Block DAA score where this cell was created
    pub block_daa_score: u64,
}

/// Collection of cells (OutPoint → CellMeta)
/// 
/// **Determinism**: BTreeMap ensures deterministic iteration order for consensus
pub type CellCollection = BTreeMap<TransactionOutpoint, CellMeta>;

/// Cell state difference
///
/// Represents the difference between two cell states:
/// - `add`: Cells created (new outputs)
/// - `remove`: Cells consumed (spent inputs)
///
/// This is the Cell model equivalent of UtxoDiff.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CellDiff {
    /// Cells added (created outputs)
    pub add: CellCollection,
    /// Cells removed (consumed inputs)
    pub remove: CellCollection,
}

impl MemSizeEstimator for CellDiff {
    fn estimate_mem_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + (self.add.len() + self.remove.len())
                * (std::mem::size_of::<TransactionOutpoint>() + std::mem::size_of::<CellMeta>())
    }
}

impl CellDiff {
    /// Create a new empty diff
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a diff with capacity hint
    /// 
    /// Note: BTreeMap doesn't have with_capacity, this is kept for API compatibility
    pub fn with_capacity(_add_capacity: usize, _remove_capacity: usize) -> Self {
        Self {
            add: BTreeMap::new(),
            remove: BTreeMap::new(),
        }
    }

    /// Add a cell creation
    pub fn add_cell(&mut self, outpoint: TransactionOutpoint, meta: CellMeta) {
        self.add.insert(outpoint, meta);
    }

    /// Add a cell consumption
    pub fn remove_cell(&mut self, outpoint: TransactionOutpoint, meta: CellMeta) {
        self.remove.insert(outpoint, meta);
    }

    /// Check if the diff is empty
    pub fn is_empty(&self) -> bool {
        self.add.is_empty() && self.remove.is_empty()
    }

    /// Get the number of cells added
    pub fn num_added(&self) -> usize {
        self.add.len()
    }

    /// Get the number of cells removed
    pub fn num_removed(&self) -> usize {
        self.remove.len()
    }

    /// Merge another diff into this one
    pub fn merge(&mut self, other: CellDiff) {
        self.add.extend(other.add);
        self.remove.extend(other.remove);
    }

    /// Reverse the diff (swap add and remove)
    pub fn reverse(self) -> Self {
        Self {
            add: self.remove,
            remove: self.add,
        }
    }

    /// Apply this diff to a cell collection (in-place)
    pub fn apply_to(&self, base: &mut CellCollection) {
        // Remove cells first
        for outpoint in self.remove.keys() {
            base.remove(outpoint);
        }
        
        // Then add new cells
        base.extend(self.add.clone());
    }

    // apply_to_tree removed - use apply_diff_placeholder on CellStateTree instead

    /// Apply another diff to this diff (in-place composition)
    /// Similar to UtxoDiff::with_diff_in_place
    pub fn with_diff_in_place(&mut self, other: &CellDiff) -> Result<(), String> {
        // Apply removals from other
        for (outpoint, meta) in &other.remove {
            if let Some(existing_meta) = self.add.remove(outpoint) {
                // Cell was added in self but removed in other -> net removal
                self.remove.insert(outpoint.clone(), existing_meta);
            } else {
                // Direct removal
                self.remove.insert(outpoint.clone(), meta.clone());
            }
        }

        // Apply additions from other
        for (outpoint, meta) in &other.add {
            if let Some(_) = self.remove.remove(outpoint) {
                // Cell was removed in self but added in other -> net addition
                self.add.insert(outpoint.clone(), meta.clone());
            } else {
                // Direct addition
                self.add.insert(outpoint.clone(), meta.clone());
            }
        }

        Ok(())
    }

    /// Create a reversed view of this diff (non-consuming)
    /// Similar to UtxoDiff::as_reversed
    pub fn as_reversed(&self) -> Self {
        Self {
            add: self.remove.clone(),
            remove: self.add.clone(),
        }
    }

    /// Get total capacity change (added - removed)
    pub fn capacity_delta(&self) -> i64 {
        let added: u64 = self.add.values().map(|m| m.capacity).sum();
        let removed: u64 = self.remove.values().map(|m| m.capacity).sum();
        added as i64 - removed as i64
    }

    /// Create a diff from two cell collections
    pub fn from_collections(old: &CellCollection, new: &CellCollection) -> Self {
        let mut diff = Self::new();

        // Find removed cells (in old but not in new)
        for (outpoint, meta) in old.iter() {
            if !new.contains_key(outpoint) {
                diff.remove.insert(outpoint.clone(), meta.clone());
            }
        }

        // Find added cells (in new but not in old)
        for (outpoint, meta) in new.iter() {
            if !old.contains_key(outpoint) {
                diff.add.insert(outpoint.clone(), meta.clone());
            }
        }

        diff
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_cell(capacity: u64) -> CellMeta {
        CellMeta {
            capacity,
            lock_hash: [1u8; 32],
            type_hash: None,
            data_hash: [2u8; 32],
            block_daa_score: 100,
        }
    }

    fn create_test_outpoint(index: u32) -> TransactionOutpoint {
        TransactionOutpoint {
            transaction_id: [3u8; 32].into(),
            index,
        }
    }

    #[test]
    fn test_cell_diff_creation() {
        let diff = CellDiff::new();
        assert!(diff.is_empty());
        assert_eq!(diff.num_added(), 0);
        assert_eq!(diff.num_removed(), 0);
    }

    #[test]
    fn test_add_remove_cells() {
        let mut diff = CellDiff::new();
        let outpoint1 = create_test_outpoint(0);
        let outpoint2 = create_test_outpoint(1);
        let cell1 = create_test_cell(1000);
        let cell2 = create_test_cell(2000);

        diff.add_cell(outpoint1.clone(), cell1);
        diff.remove_cell(outpoint2.clone(), cell2);

        assert!(!diff.is_empty());
        assert_eq!(diff.num_added(), 1);
        assert_eq!(diff.num_removed(), 1);
    }

    #[test]
    fn test_merge_diffs() {
        let mut diff1 = CellDiff::new();
        let mut diff2 = CellDiff::new();

        diff1.add_cell(create_test_outpoint(0), create_test_cell(1000));
        diff2.add_cell(create_test_outpoint(1), create_test_cell(2000));

        diff1.merge(diff2);
        assert_eq!(diff1.num_added(), 2);
    }

    #[test]
    fn test_reverse_diff() {
        let mut diff = CellDiff::new();
        diff.add_cell(create_test_outpoint(0), create_test_cell(1000));
        diff.remove_cell(create_test_outpoint(1), create_test_cell(2000));

        let reversed = diff.reverse();
        assert_eq!(reversed.num_added(), 1);
        assert_eq!(reversed.num_removed(), 1);
    }

    #[test]
    fn test_apply_diff() {
        let mut base = BTreeMap::new();
        let outpoint1 = create_test_outpoint(0);
        let outpoint2 = create_test_outpoint(1);
        
        base.insert(outpoint1.clone(), create_test_cell(1000));

        let mut diff = CellDiff::new();
        diff.remove_cell(outpoint1.clone(), create_test_cell(1000));
        diff.add_cell(outpoint2.clone(), create_test_cell(2000));

        diff.apply_to(&mut base);

        assert!(!base.contains_key(&outpoint1));
        assert!(base.contains_key(&outpoint2));
        assert_eq!(base.len(), 1);
    }

    #[test]
    fn test_from_collections() {
        let mut old = BTreeMap::new();
        let mut new = BTreeMap::new();

        let outpoint1 = create_test_outpoint(0);
        let outpoint2 = create_test_outpoint(1);

        old.insert(outpoint1.clone(), create_test_cell(1000));
        new.insert(outpoint2.clone(), create_test_cell(2000));

        let diff = CellDiff::from_collections(&old, &new);

        assert_eq!(diff.num_removed(), 1);
        assert_eq!(diff.num_added(), 1);
    }

    #[test]
    fn test_with_diff_in_place() {
        let mut diff1 = CellDiff::new();
        let mut diff2 = CellDiff::new();

        let outpoint1 = create_test_outpoint(0);
        let outpoint2 = create_test_outpoint(1);
        let outpoint3 = create_test_outpoint(2);

        // diff1: add outpoint1, remove outpoint2
        diff1.add_cell(outpoint1.clone(), create_test_cell(1000));
        diff1.remove_cell(outpoint2.clone(), create_test_cell(2000));

        // diff2: remove outpoint1 (cancels add), add outpoint3
        diff2.remove_cell(outpoint1.clone(), create_test_cell(1000));
        diff2.add_cell(outpoint3.clone(), create_test_cell(3000));

        diff1.with_diff_in_place(&diff2).unwrap();

        // Result: outpoint1 should be in remove (add then remove)
        // outpoint2 still in remove
        // outpoint3 in add
        assert_eq!(diff1.num_added(), 1);
        assert_eq!(diff1.num_removed(), 2);
        assert!(diff1.add.contains_key(&outpoint3));
        assert!(diff1.remove.contains_key(&outpoint1));
        assert!(diff1.remove.contains_key(&outpoint2));
    }

    #[test]
    fn test_as_reversed() {
        let mut diff = CellDiff::new();
        diff.add_cell(create_test_outpoint(0), create_test_cell(1000));
        diff.remove_cell(create_test_outpoint(1), create_test_cell(2000));

        let reversed = diff.as_reversed();
        
        assert_eq!(reversed.num_added(), 1);
        assert_eq!(reversed.num_removed(), 1);
        assert!(reversed.add.contains_key(&create_test_outpoint(1)));
        assert!(reversed.remove.contains_key(&create_test_outpoint(0)));
    }

    #[test]
    fn test_capacity_delta() {
        let mut diff = CellDiff::new();
        diff.add_cell(create_test_outpoint(0), create_test_cell(5000));
        diff.remove_cell(create_test_outpoint(1), create_test_cell(2000));

        assert_eq!(diff.capacity_delta(), 3000); // 5000 - 2000
    }
}

