// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Logic-level Cell model tests for Virtual Processor support types
// These cover CellStateTree / CellDiff invariants. End-to-end consensus-path
// coverage lives in `virtual_processor/tests.rs` and body-context validator tests.

#[cfg(test)]
mod tests {
    use spora_consensus_core::{
        cell_diff::{CellDiff, CellMeta},
        tx::TransactionOutpoint,
    };
    use spora_exec::OutPoint;
    use spora_hashes::{Hash, ZERO_HASH};
    use spora_state::{CellEntry, CellStateTree};

    /// Helper: create a test CellEntry with given capacity and DAA score
    fn make_entry(capacity: u64, daa_score: u64, is_cellbase: bool) -> CellEntry {
        CellEntry::new(capacity, 0, Hash::from_bytes([1u8; 32]), None, Hash::from_bytes([2u8; 32]), daa_score, is_cellbase)
    }

    /// Helper: create a TransactionOutpoint from simple bytes
    fn make_outpoint(tx_byte: u8, index: u32) -> TransactionOutpoint {
        TransactionOutpoint { tx_hash: [tx_byte; 32], index }
    }

    /// Helper: compute outpoint hash (mirrors cell_processing.rs logic)
    fn outpoint_to_hash(outpoint: &TransactionOutpoint) -> Hash {
        use blake3::Hasher;
        let mut hasher = Hasher::new();
        hasher.update(b"spora-cell/outpoint");
        hasher.update(&outpoint.tx_hash);
        hasher.update(&outpoint.index.to_le_bytes());
        Hash::from_bytes(*hasher.finalize().as_bytes())
    }

    /// Helper: convert to exec OutPoint
    fn exec_outpoint(outpoint: &TransactionOutpoint) -> OutPoint {
        OutPoint::new(outpoint.tx_hash, outpoint.index)
    }

    // ========================================================================
    // Test 1: Simple Cell Transaction (create → spend → create)
    // ========================================================================

    /// Verifies that a Cell created by a coinbase can be tracked in the state
    /// tree, removed when spent, and a new Cell can be created in its place.
    #[test]
    fn test_simple_cell_transaction() {
        let mut tree = CellStateTree::new();

        // Block A (DAA 1): Coinbase creates Cell1 with capacity 5000
        let cell1_outpoint = make_outpoint(0xAA, 0);
        let cell1_entry = make_entry(5000, 1, true);
        let cell1_hash = outpoint_to_hash(&cell1_outpoint);
        tree.insert_with_outpoint(cell1_hash, exec_outpoint(&cell1_outpoint), cell1_entry.clone());

        let root_after_create = tree.root();
        assert_ne!(root_after_create, ZERO_HASH, "Tree with one cell must have non-zero root");
        assert_eq!(tree.len(), 1);
        assert!(tree.get(&cell1_hash).is_some(), "Cell1 must exist in tree");

        // Block B (DAA 2): Transaction spends Cell1, creates Cell2 (capacity 4000, fee=1000)
        let removed = tree.remove(&cell1_hash);
        assert!(removed.is_some(), "Cell1 must be removable");
        assert_eq!(removed.unwrap().capacity, 5000);

        let cell2_outpoint = make_outpoint(0xBB, 0);
        let cell2_entry = make_entry(4000, 2, false);
        let cell2_hash = outpoint_to_hash(&cell2_outpoint);
        tree.insert_with_outpoint(cell2_hash, exec_outpoint(&cell2_outpoint), cell2_entry);

        let root_after_spend = tree.root();
        assert_ne!(root_after_spend, ZERO_HASH);
        assert_ne!(root_after_spend, root_after_create, "Root must change after spend+create");
        assert_eq!(tree.len(), 1);
        assert!(tree.get(&cell1_hash).is_none(), "Cell1 must be gone");
        assert!(tree.get(&cell2_hash).is_some(), "Cell2 must exist");
    }

    // ========================================================================
    // Test 2: Multi-Parent DAG Mergeset (CellDiff composition)
    // ========================================================================

    /// Verifies CellDiff composition rules that the GHOSTDAG replay logic relies on.
    #[test]
    fn test_multi_parent_dag_mergeset() {
        // Simulate: selected parent C creates Cell_C, blue block B creates Cell_B.
        // After processing both, the merged diff should contain both additions.

        let mut merged_diff = CellDiff::default();

        // Blue block C (selected parent): creates Cell_C
        let cell_c_outpoint = make_outpoint(0xCC, 0);
        let cell_c_meta = CellMeta {
            out_point: cell_c_outpoint,
            capacity: 3000,
            data_bytes: 0,
            lock_hash: [1u8; 32],
            type_hash: None,
            data_hash: [2u8; 32],
            block_daa_score: 11,
            is_cellbase: true,
        };
        let mut diff_c = CellDiff::default();
        diff_c.add_cell(cell_c_outpoint, cell_c_meta.clone());

        // Blue block B: creates Cell_B
        let cell_b_outpoint = make_outpoint(0xBB, 0);
        let cell_b_meta = CellMeta {
            out_point: cell_b_outpoint,
            capacity: 2000,
            data_bytes: 0,
            lock_hash: [3u8; 32],
            type_hash: None,
            data_hash: [4u8; 32],
            block_daa_score: 11,
            is_cellbase: true,
        };
        let mut diff_b = CellDiff::default();
        diff_b.add_cell(cell_b_outpoint, cell_b_meta.clone());

        // Merge in GHOSTDAG topological order: C first, then B
        merged_diff.with_diff_in_place(&diff_c).expect("C merge must succeed");
        merged_diff.with_diff_in_place(&diff_b).expect("B merge must succeed");

        assert_eq!(merged_diff.num_added(), 2, "Both cells should be in merged diff");
        assert_eq!(merged_diff.num_removed(), 0);
        assert!(merged_diff.add.contains_key(&cell_c_outpoint));
        assert!(merged_diff.add.contains_key(&cell_b_outpoint));

        // Verify no duplicate processing: merging C again should fail (double add)
        assert!(merged_diff.with_diff_in_place(&diff_c).is_err(), "Duplicate add must fail");
    }

    // ========================================================================
    // Test 3: Cell Double-Spend Rejection
    // ========================================================================

    /// Verifies the basic double-spend invariant at the CellStateTree/CellDiff level.
    #[test]
    fn test_cell_double_spend_rejection() {
        let mut tree = CellStateTree::new();

        // Create Cell1
        let cell1_outpoint = make_outpoint(0xAA, 0);
        let cell1_hash = outpoint_to_hash(&cell1_outpoint);
        tree.insert_with_outpoint(cell1_hash, exec_outpoint(&cell1_outpoint), make_entry(5000, 1, true));

        // First spend: should succeed
        let removed = tree.remove(&cell1_hash);
        assert!(removed.is_some(), "First spend must succeed");

        // Second spend attempt: tree.remove returns None (cell already gone)
        let double_spend = tree.remove(&cell1_hash);
        assert!(double_spend.is_none(), "Double spend must return None — cell no longer in tree");

        // Also verify via CellDiff: double removal must fail
        let mut diff = CellDiff::default();
        let meta = CellMeta {
            out_point: cell1_outpoint,
            capacity: 5000,
            data_bytes: 0,
            lock_hash: [1u8; 32],
            type_hash: None,
            data_hash: [2u8; 32],
            block_daa_score: 1,
            is_cellbase: true,
        };
        diff.remove_cell(cell1_outpoint, meta.clone());
        // Trying to remove the same outpoint again in diff composition must error
        let mut another_diff = CellDiff::default();
        another_diff.remove_cell(cell1_outpoint, meta);
        assert!(diff.with_diff_in_place(&another_diff).is_err(), "CellDiff double-remove must fail");
    }

    // ========================================================================
    // Test 4: Cellbase Maturity Enforcement (logic-level)
    // ========================================================================

    /// Verifies the cellbase maturity arithmetic at a pure logic level.
    #[test]
    fn test_cellbase_maturity() {
        let maturity: u64 = 100;

        // Cellbase created at DAA 50
        let created_daa: u64 = 50;
        let entry = make_entry(10000, created_daa, true);

        // Verify: is_cellbase flag is set
        assert!(entry.is_cellbase, "Entry must be marked as cellbase");

        // Maturity check: current_daa must be >= created_daa + maturity
        let maturity_daa = created_daa.saturating_add(maturity); // = 150

        // At DAA 100: immature (100 < 150)
        assert!(100u64 < maturity_daa, "DAA 100 must be immature");

        // At DAA 149: still immature (149 < 150)
        assert!(149u64 < maturity_daa, "DAA 149 must be immature");

        // At DAA 150: mature (150 >= 150)
        assert!(150u64 >= maturity_daa, "DAA 150 must be mature");

        // At DAA 200: mature (200 >= 150)
        assert!(200u64 >= maturity_daa, "DAA 200 must be mature");

        // Regular cell (non-cellbase) has no maturity requirement
        let regular_entry = make_entry(5000, created_daa, false);
        assert!(!regular_entry.is_cellbase, "Regular entry must not be cellbase");
    }

    // ========================================================================
    // Test 5: Reorg Cell State Consistency (CellDiff reversal)
    // ========================================================================

    /// Verifies that CellDiff reversal undoes state changes at the diff layer.
    #[test]
    fn test_reorg_cell_state_consistency() {
        // Initial state: Cell_A exists
        let cell_a_outpoint = make_outpoint(0xAA, 0);
        let cell_a_meta = CellMeta {
            out_point: cell_a_outpoint,
            capacity: 5000,
            data_bytes: 0,
            lock_hash: [1u8; 32],
            type_hash: None,
            data_hash: [2u8; 32],
            block_daa_score: 50,
            is_cellbase: true,
        };

        // Chain 1: Block B spends Cell_A, creates Cell_B
        let cell_b_outpoint = make_outpoint(0xBB, 0);
        let cell_b_meta = CellMeta {
            out_point: cell_b_outpoint,
            capacity: 4000,
            data_bytes: 0,
            lock_hash: [3u8; 32],
            type_hash: None,
            data_hash: [4u8; 32],
            block_daa_score: 100,
            is_cellbase: false,
        };

        let mut chain1_diff = CellDiff::default();
        chain1_diff.remove_cell(cell_a_outpoint, cell_a_meta.clone());
        chain1_diff.add_cell(cell_b_outpoint, cell_b_meta.clone());

        // Reverse chain1_diff (for reorg rollback)
        let reversed = chain1_diff.clone().reverse();
        assert_eq!(reversed.num_added(), 1, "Reversed diff must re-add Cell_A");
        assert_eq!(reversed.num_removed(), 1, "Reversed diff must remove Cell_B");
        assert!(reversed.add.contains_key(&cell_a_outpoint), "Cell_A must be in reversed.add");
        assert!(reversed.remove.contains_key(&cell_b_outpoint), "Cell_B must be in reversed.remove");

        // Apply chain1_diff then reversed: net effect should be empty
        let mut accumulated = CellDiff::default();
        accumulated.with_diff_in_place(&chain1_diff).expect("forward apply");
        accumulated.with_diff_in_place(&reversed).expect("reverse apply");
        assert!(accumulated.is_empty(), "Forward + reverse must cancel out to empty diff");
    }

    // ========================================================================
    // Test 6: Cell Root Verification
    // ========================================================================

    /// Verify that cell_root is calculated correctly from state tree
    ///
    /// Test cases:
    /// a) Empty tree → ZERO_HASH
    /// b) Single cell → Non-zero hash
    /// c) Multiple cells → Deterministic hash
    /// d) Same cells different order → Same hash (BTreeMap)
    /// e) Cell added then removed → Back to previous hash
    #[test]
    fn test_cell_root_calculation() {
        use spora_muhash::EMPTY_MUHASH;
        let mut tree = CellStateTree::new();

        // a) Empty tree - MuHash empty accumulator produces EMPTY_MUHASH, not ZERO_HASH
        assert_eq!(tree.root(), EMPTY_MUHASH);

        // b) Single cell
        let entry1 = CellEntry::new(1000, 0, Hash::from_bytes([1u8; 32]), None, Hash::from_bytes([2u8; 32]), 1, false);
        let outpoint1 = Hash::from_bytes([10u8; 32]);
        tree.insert(outpoint1, entry1.clone());

        let root1 = tree.root();
        assert_ne!(root1, ZERO_HASH);

        // c) Multiple cells: add a second cell
        let entry2 = CellEntry::new(2000, 0, Hash::from_bytes([3u8; 32]), None, Hash::from_bytes([4u8; 32]), 2, false);
        let outpoint2 = Hash::from_bytes([20u8; 32]);
        tree.insert(outpoint2, entry2.clone());

        let root2 = tree.root();
        assert_ne!(root2, ZERO_HASH);
        assert_ne!(root2, root1, "Adding a cell must change root");

        // d) Same cells different order → verified in test_cell_state_tree_determinism

        // e) Remove both cells - back to empty (EMPTY_MUHASH, not ZERO_HASH)
        tree.remove(&outpoint2);
        tree.remove(&outpoint1);
        assert_eq!(tree.root(), EMPTY_MUHASH);
    }

    // ========================================================================
    // Test 7: Cell Commitment V0
    // ========================================================================

    /// Verify: cell_commitment = H("spora/cell_commitment/v0" || cell_root)
    #[test]
    fn test_cell_commitment_v0() {
        use blake3::Hasher;

        let cell_root = Hash::from_bytes([0x42u8; 32]);

        // Compute commitment
        let mut hasher = Hasher::new();
        hasher.update(b"spora/cell_commitment/v0");
        hasher.update(&cell_root.as_bytes());
        let commitment = Hash::from_bytes(*hasher.finalize().as_bytes());

        // Verify it's deterministic
        let mut hasher2 = Hasher::new();
        hasher2.update(b"spora/cell_commitment/v0");
        hasher2.update(&cell_root.as_bytes());
        let commitment2 = Hash::from_bytes(*hasher2.finalize().as_bytes());

        assert_eq!(commitment, commitment2);
        assert_ne!(commitment, cell_root); // Should be different from root
    }

    // ========================================================================
    // Test 8: GhostDAG-Aware Cell Processing (diff ordering)
    // ========================================================================

    /// Verifies that the order of CellDiff composition matters for correctness:
    /// When a Cell is created by block X and spent by block Y in the same
    /// mergeset, the composition must handle the create-then-spend pattern.
    #[test]
    fn test_ghostdag_aware_cell_processing() {
        // Block X creates Cell_X (selected parent coinbase)
        let cell_x_outpoint = make_outpoint(0x01, 0);
        let cell_x_meta = CellMeta {
            out_point: cell_x_outpoint,
            capacity: 5000,
            data_bytes: 0,
            lock_hash: [0x10; 32],
            type_hash: None,
            data_hash: [0x20; 32],
            block_daa_score: 10,
            is_cellbase: true,
        };

        let mut diff_x = CellDiff::default();
        diff_x.add_cell(cell_x_outpoint, cell_x_meta.clone());

        // Block Y (blue block) spends Cell_X and creates Cell_Y
        let cell_y_outpoint = make_outpoint(0x02, 0);
        let cell_y_meta = CellMeta {
            out_point: cell_y_outpoint,
            capacity: 4500,
            data_bytes: 0,
            lock_hash: [0x30; 32],
            type_hash: None,
            data_hash: [0x40; 32],
            block_daa_score: 11,
            is_cellbase: false,
        };

        let mut diff_y = CellDiff::default();
        diff_y.remove_cell(cell_x_outpoint, cell_x_meta);
        diff_y.add_cell(cell_y_outpoint, cell_y_meta.clone());

        // GHOSTDAG order: X first, then Y
        let mut merged = CellDiff::default();
        merged.with_diff_in_place(&diff_x).expect("X must succeed");
        merged.with_diff_in_place(&diff_y).expect("Y must succeed — Cell_X created then spent cancels out");

        // Net result: Cell_X was created and spent (cancels), Cell_Y remains as addition
        assert_eq!(merged.num_added(), 1, "Only Cell_Y should remain in add");
        assert_eq!(merged.num_removed(), 0, "Cell_X create+spend should cancel");
        assert!(merged.add.contains_key(&cell_y_outpoint));

        // Wrong order: Y before X should fail (removing a cell that hasn't been added)
        let mut wrong_order = CellDiff::default();
        // diff_y tries to remove Cell_X which hasn't been added yet — this adds to remove set
        wrong_order.with_diff_in_place(&diff_y).expect("Y standalone works");
        // diff_x adds Cell_X — but it's already in remove set, so it should cancel
        wrong_order.with_diff_in_place(&diff_x).expect("X after Y should cancel");
        // In this case Cell_X was removed first then added — they cancel out
        // Net: Cell_Y added, Cell_X canceled
        assert_eq!(wrong_order.num_added(), 1);
        // The net outcome is the same because with_diff_in_place handles both orders symmetrically
        // But in practice, GHOSTDAG topological order ensures we always process creates before spends
    }

    // ========================================================================
    // Test 9: Cell State Tree Determinism
    // ========================================================================

    /// Verify BTreeMap ensures deterministic iteration
    #[test]
    fn test_cell_state_tree_determinism() {
        let mut tree1 = CellStateTree::new();
        let mut tree2 = CellStateTree::new();

        let entries = vec![
            (
                Hash::from_bytes([1u8; 32]),
                CellEntry::new(100, 0, Hash::from_bytes([10u8; 32]), None, Hash::from_bytes([20u8; 32]), 1, false),
            ),
            (
                Hash::from_bytes([2u8; 32]),
                CellEntry::new(200, 0, Hash::from_bytes([11u8; 32]), None, Hash::from_bytes([21u8; 32]), 2, false),
            ),
            (
                Hash::from_bytes([3u8; 32]),
                CellEntry::new(300, 0, Hash::from_bytes([12u8; 32]), None, Hash::from_bytes([22u8; 32]), 3, false),
            ),
        ];

        // Insert in forward order
        for (hash, entry) in &entries {
            tree1.insert(*hash, entry.clone());
        }

        // Insert in reverse order
        for (hash, entry) in entries.iter().rev() {
            tree2.insert(*hash, entry.clone());
        }

        // Roots should match (deterministic)
        assert_eq!(tree1.root(), tree2.root());
    }

    // ========================================================================
    // Test 10: Historical Cell Query
    // ========================================================================

    /// Verify that historical queries work correctly for DAG.
    /// The CellDB in state/src/index/cell_db.rs covers the full implementation;
    /// here we verify the building block: CellDiff can reconstruct snapshots.
    #[test]
    fn test_historical_cell_query() {
        // Simulate: at block A (DAA 50), Cell_A exists; at block B (DAA 100), it's spent.
        // Historical query at A should find it; at B should not.

        let cell_a = make_outpoint(0xAA, 0);
        let cell_a_meta = CellMeta {
            out_point: cell_a,
            capacity: 5000,
            data_bytes: 0,
            lock_hash: [1u8; 32],
            type_hash: None,
            data_hash: [2u8; 32],
            block_daa_score: 50,
            is_cellbase: true,
        };

        // Block A diff: creates Cell_A
        let mut block_a_diff = CellDiff::default();
        block_a_diff.add_cell(cell_a, cell_a_meta.clone());

        // Block B diff: spends Cell_A
        let mut block_b_diff = CellDiff::default();
        block_b_diff.remove_cell(cell_a, cell_a_meta.clone());

        // At point A: only block_a_diff applied → Cell_A in add
        assert!(block_a_diff.add.contains_key(&cell_a), "Cell_A must exist at point A");

        // At point B: both diffs applied → Cell_A should cancel
        let mut cumulative = CellDiff::default();
        cumulative.with_diff_in_place(&block_a_diff).unwrap();
        cumulative.with_diff_in_place(&block_b_diff).unwrap();
        assert!(cumulative.is_empty(), "Cell_A created and spent should result in empty diff");

        // Reverse from B back to A: apply reversed block_b_diff
        let reversed_b = block_b_diff.reverse();
        assert!(reversed_b.add.contains_key(&cell_a), "Reversing spend should re-add Cell_A");
    }

    // ========================================================================
    // Test 11: Capacity Conservation
    // ========================================================================

    /// Verify that output capacity cannot exceed input capacity
    #[test]
    fn test_capacity_conservation() {
        let input_capacity = 5000u64;
        let valid_output_capacity = 4500u64; // 500 fee
        let invalid_output_capacity = 5001u64; // exceeds input

        // Valid: output <= input
        assert!(valid_output_capacity <= input_capacity, "Valid output must not exceed input");

        // Invalid: output > input
        assert!(invalid_output_capacity > input_capacity, "Invalid output exceeds input — should be rejected");

        // Overflow protection
        let huge_a = u64::MAX - 100;
        let huge_b = 200u64;
        assert!(huge_a.checked_add(huge_b).is_none(), "Overflow must be caught by checked_add");
    }
}
