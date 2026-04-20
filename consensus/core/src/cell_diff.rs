// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Spora developers
//
// Cell state difference - tracks additions and removals of cells

use crate::cell_metadata::EmbeddedCellMetadata;
use crate::tx::TransactionOutpoint;
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use spora_exec::celltx::CapacityError;
use spora_hashes::Hash;
use spora_utils::mem_size::MemSizeEstimator;
use std::collections::BTreeMap;

/// Cell metadata (for diff tracking and state commitment)
///
/// **CKB Compatibility**: Aligned with CKB's CellMeta while adapted for GhostDAG
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct CellMeta {
    /// OutPoint: uniquely identifies this cell
    /// Added for CKB compatibility - essential for cell identification
    pub out_point: TransactionOutpoint,

    /// Cell capacity in saus
    pub capacity: u64,

    /// Data length in bytes
    /// Added for CKB compatibility - needed for occupied_capacity calculation
    pub data_bytes: u64,

    /// Lock script hash
    pub lock_hash: [u8; 32],

    /// Type script hash (if present)
    pub type_hash: Option<[u8; 32]>,

    /// Data hash
    pub data_hash: [u8; 32],

    /// Block DAA score where this cell was created
    /// GhostDAG extension: replaces block_number for DAG compatibility
    pub block_daa_score: u64,

    /// Whether this cell was created by a cellbase transaction
    pub is_cellbase: bool,
}

/// Collection of cells (OutPoint → CellMeta)
///
/// **Determinism**: BTreeMap ensures deterministic iteration order for consensus
pub type CellCollection = BTreeMap<TransactionOutpoint, CellMeta>;

/// Per-block cell diff journal entry.
///
/// Carries the concrete block that created/consumed the cells in `cell_diff`,
/// allowing downstream consumers to reconstruct a canonical history journal
/// instead of only a live accumulated diff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockCellDiff {
    pub block_hash: Hash,
    pub block_daa_score: u64,
    pub cell_diff: CellDiff,
}

impl BlockCellDiff {
    pub fn new(block_hash: Hash, block_daa_score: u64, cell_diff: CellDiff) -> Self {
        Self { block_hash, block_daa_score, cell_diff }
    }
}

/// Cell state difference
///
/// Represents the difference between two cell states:
/// - `add`: Cells created (new outputs)
/// - `remove`: Cells consumed (spent inputs)
///
/// This is the Cell model equivalent of the old transaction-output diff.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CellDiff {
    /// Cells added (created outputs)
    pub add: CellCollection,
    /// Cells removed (consumed inputs)
    pub remove: CellCollection,
}

impl CellMeta {
    /// Calculate occupied capacity (minimum required)
    /// Aligned with CKB's occupied_capacity calculation
    pub fn occupied_capacity(&self) -> u64 {
        let mut size = 8; // capacity field (u64)
        size += 32; // lock_hash
        if self.type_hash.is_some() {
            size += 32; // type_hash
        }
        size += 32; // data_hash
        size += self.data_bytes as usize; // actual data
        size as u64
    }

    /// Verify capacity is sufficient
    pub fn verify_capacity(&self) -> Result<(), CapacityError> {
        let occupied = self.occupied_capacity();
        if self.capacity < occupied {
            return Err(CapacityError::InsufficientCapacity { required: occupied, available: self.capacity });
        }
        Ok(())
    }

    /// Convenience constructor (without outpoint, which defaults to zero).
    pub fn from_cell_metadata(
        capacity: u64,
        data_bytes: u64,
        lock_hash: [u8; 32],
        type_hash: Option<[u8; 32]>,
        data_hash: [u8; 32],
        block_daa_score: u64,
        is_cellbase: bool,
    ) -> Self {
        Self {
            out_point: TransactionOutpoint::default(),
            capacity,
            data_bytes,
            lock_hash,
            type_hash,
            data_hash,
            block_daa_score,
            is_cellbase,
        }
    }

    /// Returns the compact metadata view embedded in this cell entry.
    pub fn embedded_cell_metadata(&self) -> Option<EmbeddedCellMetadata> {
        Some(EmbeddedCellMetadata {
            lock_hash: self.lock_hash,
            type_hash: self.type_hash,
            data_hash: self.data_hash,
            data_bytes: self.data_bytes,
        })
    }

    /// Returns capacity (same as the `capacity` field).
    pub fn capacity(&self) -> u64 {
        self.capacity
    }

    /// Returns capacity using the account-facing `amount` name.
    pub fn amount(&self) -> u64 {
        self.capacity
    }
}

impl MemSizeEstimator for CellMeta {
    fn estimate_mem_bytes(&self) -> usize {
        // Track retained metadata bytes rather than the padded Rust struct size so cache
        // budgeting follows the actual Cell payload surface more closely.
        std::mem::size_of::<TransactionOutpoint>()
            + self.occupied_capacity() as usize
            + std::mem::size_of::<u64>() // block_daa_score
            + std::mem::size_of::<bool>() // is_cellbase
    }
}

impl MemSizeEstimator for CellDiff {
    fn estimate_mem_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self
                .add
                .iter()
                .chain(self.remove.iter())
                .map(|(outpoint, meta)| std::mem::size_of_val(outpoint) + meta.estimate_mem_bytes())
                .sum::<usize>()
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
        Self { add: BTreeMap::new(), remove: BTreeMap::new() }
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
        self.with_diff_in_place(&other).expect("cell diff merge must preserve a valid state transition");
    }

    /// Reverse the diff (swap add and remove)
    pub fn reverse(self) -> Self {
        Self { add: self.remove, remove: self.add }
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

    /// Apply another diff to this diff (in-place composition)
    /// Similar to the old transaction-output diff `with_diff_in_place`
    pub fn with_diff_in_place(&mut self, other: &CellDiff) -> Result<(), String> {
        // Apply removals from other
        for (outpoint, meta) in &other.remove {
            if self.add.remove(outpoint).is_some() {
                // Cell was created and later consumed within the composed range -> net no-op.
                continue;
            }

            if self.remove.contains_key(outpoint) {
                return Err(format!("cell diff composition tried to remove outpoint {outpoint} twice"));
            } else {
                // Direct removal
                self.remove.insert(outpoint.clone(), meta.clone());
            }
        }

        // Apply additions from other
        for (outpoint, meta) in &other.add {
            if self.remove.remove(outpoint).is_some() {
                // Cell existed in the base state and still exists after the composed range -> net no-op.
                continue;
            }

            if self.add.contains_key(outpoint) {
                return Err(format!("cell diff composition tried to add outpoint {outpoint} twice"));
            } else {
                // Direct addition
                self.add.insert(outpoint.clone(), meta.clone());
            }
        }

        Ok(())
    }

    /// Create a reversed view of this diff (non-consuming)
    /// Similar to the old transaction-output diff `as_reversed`
    pub fn as_reversed(&self) -> Self {
        Self { add: self.remove.clone(), remove: self.add.clone() }
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
    use spora_exec::celltx::CapacityError;

    fn create_test_cell(capacity: u64, index: u32) -> CellMeta {
        CellMeta {
            out_point: create_test_outpoint(index),
            capacity,
            data_bytes: 0,
            lock_hash: [1u8; 32],
            type_hash: None,
            data_hash: [2u8; 32],
            block_daa_score: 100,
            is_cellbase: false,
        }
    }

    fn create_test_outpoint(index: u32) -> TransactionOutpoint {
        TransactionOutpoint::new([3u8; 32], index)
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
        let cell1 = create_test_cell(1000, 0);
        let cell2 = create_test_cell(2000, 1);

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

        diff1.add_cell(create_test_outpoint(0), create_test_cell(1000, 0));
        diff2.add_cell(create_test_outpoint(1), create_test_cell(2000, 1));

        diff1.merge(diff2);
        assert_eq!(diff1.num_added(), 2);
    }

    #[test]
    fn test_reverse_diff() {
        let mut diff = CellDiff::new();
        diff.add_cell(create_test_outpoint(0), create_test_cell(1000, 0));
        diff.remove_cell(create_test_outpoint(1), create_test_cell(2000, 1));

        let reversed = diff.reverse();
        assert_eq!(reversed.num_added(), 1);
        assert_eq!(reversed.num_removed(), 1);
    }

    #[test]
    fn test_apply_diff() {
        let mut base = BTreeMap::new();
        let outpoint1 = create_test_outpoint(0);
        let outpoint2 = create_test_outpoint(1);

        base.insert(outpoint1.clone(), create_test_cell(1000, 0));

        let mut diff = CellDiff::new();
        diff.remove_cell(outpoint1.clone(), create_test_cell(1000, 0));
        diff.add_cell(outpoint2.clone(), create_test_cell(2000, 1));

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

        old.insert(outpoint1.clone(), create_test_cell(1000, 0));
        new.insert(outpoint2.clone(), create_test_cell(2000, 1));

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
        diff1.add_cell(outpoint1.clone(), create_test_cell(1000, 0));
        diff1.remove_cell(outpoint2.clone(), create_test_cell(2000, 1));

        // diff2: remove outpoint1 (cancels add), add outpoint3
        diff2.remove_cell(outpoint1.clone(), create_test_cell(1000, 0));
        diff2.add_cell(outpoint3.clone(), create_test_cell(3000, 2));

        diff1.with_diff_in_place(&diff2).unwrap();

        // Result: outpoint1 cancels out, outpoint2 remains removed, outpoint3 remains added.
        assert_eq!(diff1.num_added(), 1);
        assert_eq!(diff1.num_removed(), 1);
        assert!(diff1.add.contains_key(&outpoint3));
        assert!(diff1.remove.contains_key(&outpoint2));
        assert!(!diff1.add.contains_key(&outpoint1));
        assert!(!diff1.remove.contains_key(&outpoint1));
    }

    #[test]
    fn test_with_diff_in_place_remove_then_add_cancels() {
        let mut diff1 = CellDiff::new();
        let mut diff2 = CellDiff::new();

        let outpoint = create_test_outpoint(0);
        let meta = create_test_cell(1000, 0);

        diff1.remove_cell(outpoint.clone(), meta.clone());
        diff2.add_cell(outpoint.clone(), meta);

        diff1.with_diff_in_place(&diff2).unwrap();

        assert!(diff1.is_empty());
    }

    #[test]
    fn test_with_diff_in_place_matches_collection_replay_for_complex_chain() {
        let outpoint_a = create_test_outpoint(0);
        let outpoint_b = create_test_outpoint(1);
        let outpoint_c = create_test_outpoint(2);
        let outpoint_d = create_test_outpoint(3);
        let outpoint_e = create_test_outpoint(4);

        let cell_a = create_test_cell(1000, 0);
        let cell_b = create_test_cell(2000, 1);
        let cell_c = create_test_cell(3000, 2);
        let cell_d = create_test_cell(4000, 3);
        let cell_e = create_test_cell(5000, 4);

        let mut base = BTreeMap::new();
        base.insert(outpoint_a.clone(), cell_a.clone());
        base.insert(outpoint_b.clone(), cell_b.clone());

        let mut diff1 = CellDiff::new();
        diff1.remove_cell(outpoint_a.clone(), cell_a.clone());
        diff1.add_cell(outpoint_c.clone(), cell_c.clone());

        let mut diff2 = CellDiff::new();
        diff2.remove_cell(outpoint_c.clone(), cell_c.clone());
        diff2.add_cell(outpoint_d.clone(), cell_d.clone());

        let mut diff3 = CellDiff::new();
        diff3.remove_cell(outpoint_b.clone(), cell_b.clone());
        diff3.add_cell(outpoint_e.clone(), cell_e.clone());

        let mut replayed = base.clone();
        diff1.apply_to(&mut replayed);
        diff2.apply_to(&mut replayed);
        diff3.apply_to(&mut replayed);

        let mut composed = CellDiff::new();
        composed.with_diff_in_place(&diff1).unwrap();
        composed.with_diff_in_place(&diff2).unwrap();
        composed.with_diff_in_place(&diff3).unwrap();

        let mut composed_applied = base.clone();
        composed.apply_to(&mut composed_applied);

        assert_eq!(composed_applied, replayed);
        assert_eq!(composed.num_added(), 2);
        assert_eq!(composed.num_removed(), 2);
        assert!(composed.add.contains_key(&outpoint_d));
        assert!(composed.add.contains_key(&outpoint_e));
        assert!(composed.remove.contains_key(&outpoint_a));
        assert!(composed.remove.contains_key(&outpoint_b));
        assert!(!composed.add.contains_key(&outpoint_c));
        assert!(!composed.remove.contains_key(&outpoint_c));

        let mut rolled_back = composed_applied.clone();
        composed.as_reversed().apply_to(&mut rolled_back);
        assert_eq!(rolled_back, base);
    }

    #[test]
    fn test_with_diff_in_place_cancels_transient_chain_across_multiple_diffs() {
        let outpoint_x = create_test_outpoint(10);
        let outpoint_y = create_test_outpoint(11);
        let cell_x = create_test_cell(1500, 10);
        let cell_y = create_test_cell(1700, 11);

        let mut diff1 = CellDiff::new();
        diff1.add_cell(outpoint_x.clone(), cell_x.clone());

        let mut diff2 = CellDiff::new();
        diff2.remove_cell(outpoint_x.clone(), cell_x);
        diff2.add_cell(outpoint_y.clone(), cell_y.clone());

        let mut diff3 = CellDiff::new();
        diff3.remove_cell(outpoint_y.clone(), cell_y);

        let mut composed = CellDiff::new();
        composed.with_diff_in_place(&diff1).unwrap();
        composed.with_diff_in_place(&diff2).unwrap();
        composed.with_diff_in_place(&diff3).unwrap();

        assert!(composed.is_empty(), "create/spend chains should collapse to a net no-op");
    }

    #[test]
    fn test_as_reversed() {
        let mut diff = CellDiff::new();
        diff.add_cell(create_test_outpoint(0), create_test_cell(1000, 0));
        diff.remove_cell(create_test_outpoint(1), create_test_cell(2000, 1));

        let reversed = diff.as_reversed();

        assert_eq!(reversed.num_added(), 1);
        assert_eq!(reversed.num_removed(), 1);
        assert!(reversed.add.contains_key(&create_test_outpoint(1)));
        assert!(reversed.remove.contains_key(&create_test_outpoint(0)));
    }

    #[test]
    fn test_capacity_delta() {
        let mut diff = CellDiff::new();
        diff.add_cell(create_test_outpoint(0), create_test_cell(5000, 0));
        diff.remove_cell(create_test_outpoint(1), create_test_cell(2000, 1));

        assert_eq!(diff.capacity_delta(), 3000); // 5000 - 2000
    }

    #[test]
    fn test_occupied_capacity() {
        let cell = create_test_cell(10000, 0);
        let occupied = cell.occupied_capacity();
        // 8 (capacity) + 32 (lock) + 32 (data_hash) + 0 (data_bytes) = 72
        assert_eq!(occupied, 72);
    }

    #[test]
    fn test_cell_meta_mem_size_tracks_retained_payload_bytes() {
        let mut cell = create_test_cell(1000, 0);
        cell.data_bytes = 64;
        cell.type_hash = Some([9u8; 32]);

        let expected = std::mem::size_of::<TransactionOutpoint>() + cell.occupied_capacity() as usize + std::mem::size_of::<u64>() + 1;
        assert_eq!(cell.estimate_mem_bytes(), expected);
    }

    #[test]
    fn test_cell_diff_mem_size_uses_exact_entry_estimates() {
        let mut diff = CellDiff::new();
        let mut added = create_test_cell(1000, 0);
        added.data_bytes = 32;
        let removed = create_test_cell(2000, 1);

        let added_key = create_test_outpoint(0);
        let removed_key = create_test_outpoint(1);
        diff.add_cell(added_key, added.clone());
        diff.remove_cell(removed_key, removed.clone());

        let expected = std::mem::size_of::<CellDiff>()
            + std::mem::size_of_val(&added_key)
            + added.estimate_mem_bytes()
            + std::mem::size_of_val(&removed_key)
            + removed.estimate_mem_bytes();
        assert_eq!(diff.estimate_mem_bytes(), expected);
    }

    #[test]
    fn test_verify_capacity() {
        let mut cell = create_test_cell(1000, 0);
        cell.data_bytes = 100;

        // Should pass: 1000 >= occupied (72 + 100 = 172)
        assert!(cell.verify_capacity().is_ok());

        // Should fail: insufficient capacity
        cell.capacity = 50;
        assert_eq!(
            cell.verify_capacity(),
            Err(CapacityError::InsufficientCapacity { required: cell.occupied_capacity(), available: 50 })
        );
    }
}
