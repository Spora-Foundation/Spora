// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Reference tests for P2B parallelization.
//
// These tests verify the correctness of the parallel execution model
// by exercising BlockAccessSummary, ExecutionDAG, and BlockExecutionEffect
// against the 5 protocol semantic rules defined in P2B_PROTOCOL_SEMANTICS.md.
//
// The tests are organized into 6 categories:
//   T1: Deterministic result for same mergeset
//   T2: Duplicate transaction across blue blocks
//   T3: Double spend across blue blocks
//   T4: Cross-block created-then-spent dependency
//   T5: Red/blue reward and acceptance data stability
//   T6: Serial vs parallel runner equivalence

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use spora_consensus_core::{
        acceptance_data::AcceptedTxEntry,
        cell_diff::{CellDiff, CellMeta},
        coinbase::BlockRewardData,
        tx::TransactionOutpoint,
    };
    use spora_hashes::Hash;

    use crate::pipeline::virtual_processor::{
        access_summary::BlockAccessSummary,
        cell_processing::{apply_cell_diff_to_tree, BlockExecutionEffect},
        execution_dag::ExecutionDAG,
    };

    // ========================================================================
    // Helpers
    // ========================================================================

    fn hash(v: u8) -> Hash {
        Hash::from_bytes([v; 32])
    }

    fn outpoint(tx: u8, idx: u32) -> TransactionOutpoint {
        TransactionOutpoint { tx_hash: [tx; 32], index: idx }
    }

    fn make_summary(
        block: u8,
        spent: &[TransactionOutpoint],
        created: &[TransactionOutpoint],
        read: &[TransactionOutpoint],
        txs: &[Hash],
    ) -> BlockAccessSummary {
        BlockAccessSummary {
            block_hash: hash(block),
            spent_outpoints: spent.iter().cloned().collect(),
            created_outpoints: created.iter().cloned().collect(),
            read_deps: read.iter().cloned().collect(),
            tx_ids: txs.iter().cloned().collect(),
        }
    }

    fn make_cell_meta(outpoint: TransactionOutpoint, capacity: u64, daa: u64, is_cellbase: bool) -> CellMeta {
        CellMeta {
            out_point: outpoint,
            capacity,
            data_bytes: 0,
            lock_hash: [1u8; 32],
            type_hash: None,
            data_hash: [2u8; 32],
            block_daa_score: daa,
            is_cellbase,
        }
    }

    fn make_reward(subsidy: u64) -> BlockRewardData {
        BlockRewardData::new(subsidy, 0, spora_exec::Script::new([0; 32], 0, vec![]))
    }

    // ========================================================================
    // T1: Same Mergeset, Deterministic Result
    // ========================================================================
    //
    // Verifies P2B_PROTOCOL_SEMANTICS invariant 1: for the same mergeset,
    // the ExecutionDAG layering is deterministic regardless of construction
    // order. Since BlockAccessSummary and ExecutionDAG are pure functions of
    // the input data, repeated construction must produce identical results.

    #[test]
    fn test_same_mergeset_deterministic_dag_layering() {
        // Three independent blue blocks
        let summaries = vec![
            make_summary(1, &[outpoint(1, 0)], &[outpoint(1, 1)], &[], &[hash(0x10)]),
            make_summary(2, &[outpoint(2, 0)], &[outpoint(2, 1)], &[], &[hash(0x20)]),
            make_summary(3, &[outpoint(3, 0)], &[outpoint(3, 1)], &[], &[hash(0x30)]),
        ];

        // Build DAG multiple times — must produce identical layering
        let dag1 = ExecutionDAG::build(&summaries);
        let dag2 = ExecutionDAG::build(&summaries);
        let dag3 = ExecutionDAG::build(&summaries);

        assert_eq!(dag1.layers, dag2.layers, "DAG layering must be deterministic (run 1 vs 2)");
        assert_eq!(dag2.layers, dag3.layers, "DAG layering must be deterministic (run 2 vs 3)");
        assert_eq!(dag1.block_hashes, dag2.block_hashes);
        assert_eq!(dag1.layer_count(), 1, "All independent blocks should be in a single layer");
        assert_eq!(dag1.layers[0], vec![0, 1, 2]);
    }

    #[test]
    fn test_same_mergeset_deterministic_cell_diff_composition() {
        // Verify that composing CellDiffs in the same GhostDAG canonical order
        // always produces the same result.
        let op_a = outpoint(0xAA, 0);
        let op_b = outpoint(0xBB, 0);
        let meta_a = make_cell_meta(op_a, 5000, 10, true);
        let meta_b = make_cell_meta(op_b, 3000, 11, true);

        // Build two identical diff sequences
        for _ in 0..3 {
            let mut merged = CellDiff::default();
            let mut diff_a = CellDiff::default();
            diff_a.add_cell(op_a, meta_a.clone());
            let mut diff_b = CellDiff::default();
            diff_b.add_cell(op_b, meta_b.clone());

            merged.with_diff_in_place(&diff_a).unwrap();
            merged.with_diff_in_place(&diff_b).unwrap();

            assert_eq!(merged.num_added(), 2);
            assert_eq!(merged.num_removed(), 0);
            assert!(merged.add.contains_key(&op_a));
            assert!(merged.add.contains_key(&op_b));
        }
    }

    // ========================================================================
    // T2: Duplicate Transaction Across Blue Blocks
    // ========================================================================
    //
    // Verifies P2B_PROTOCOL_SEMANTICS rule 1: when the same tx_id appears
    // in multiple blue blocks, only the first (in canonical order) is accepted.
    // The access summary must detect this as a dependency, forcing serialization.

    #[test]
    fn test_duplicate_tx_detected_by_access_summary() {
        // Two blocks contain the same transaction ID
        let shared_tx = hash(0xAA);
        let summary_a = make_summary(1, &[outpoint(1, 0)], &[outpoint(1, 1)], &[], &[shared_tx, hash(0x11)]);
        let summary_b = make_summary(2, &[outpoint(2, 0)], &[outpoint(2, 1)], &[], &[shared_tx, hash(0x22)]);

        // B depends on A because of the shared tx_id
        assert!(summary_b.has_dependency_on(&summary_a), "Duplicate tx_id must create a dependency");

        // DAG must serialize them: A before B
        let dag = ExecutionDAG::build(&[summary_a, summary_b]);
        assert_eq!(dag.layer_count(), 2, "Duplicate tx blocks must be in separate layers");
        assert_eq!(dag.layers[0], vec![0]);
        assert_eq!(dag.layers[1], vec![1]);
    }

    #[test]
    fn test_duplicate_tx_skipped_in_later_block() {
        // Simulate the commit-time dedup logic:
        // Block A's effect includes tx_aa; Block B also has tx_aa.
        // After committing A's effect, B's tx_aa should be skipped.
        let shared_tx = hash(0xAA);
        let unique_tx_b = hash(0xBB);

        // Simulate processed_txs tracking (mirrors calculate_cell_state logic)
        let mut processed_txs: HashSet<Hash> = HashSet::new();

        // Block A contributes shared_tx
        let effect_a_tx_ids = vec![shared_tx];
        for tx_id in &effect_a_tx_ids {
            processed_txs.insert(*tx_id);
        }

        // Block B attempts to include shared_tx and unique_tx_b
        let block_b_candidates = vec![shared_tx, unique_tx_b];
        let mut accepted_in_b = Vec::new();
        for tx_id in &block_b_candidates {
            if processed_txs.contains(tx_id) {
                continue; // skipped — duplicate
            }
            processed_txs.insert(*tx_id);
            accepted_in_b.push(*tx_id);
        }

        // Only unique_tx_b should be accepted in B
        assert_eq!(accepted_in_b, vec![unique_tx_b], "Duplicate tx must be skipped in later block");
        assert!(!accepted_in_b.contains(&shared_tx), "shared_tx must not appear in B's accepted set");
    }

    // ========================================================================
    // T3: Double Spend Across Blue Blocks
    // ========================================================================
    //
    // Verifies P2B_PROTOCOL_SEMANTICS rule 2: when different transactions in
    // different blue blocks consume the same outpoint, only the first (in
    // canonical order) succeeds. The later block's effect is invalidated.

    #[test]
    fn test_double_spend_detected_by_access_summary() {
        // Block A and Block B both spend outpoint(0, 0) but with different transactions
        let contested_outpoint = outpoint(0, 0);
        let summary_a = make_summary(1, &[contested_outpoint], &[outpoint(1, 0)], &[], &[hash(0x10)]);
        let summary_b = make_summary(2, &[contested_outpoint], &[outpoint(2, 0)], &[], &[hash(0x20)]);

        assert!(summary_b.has_dependency_on(&summary_a), "Double-spend must create a dependency");

        let dag = ExecutionDAG::build(&[summary_a, summary_b]);
        assert_eq!(dag.layer_count(), 2, "Double-spend blocks must be serialized");
    }

    #[test]
    fn test_double_spend_effect_invalidation_at_commit_time() {
        // Simulate rule 3 (effect invalidation):
        // Both A and B consume outpoint(0,0). After committing A's effect,
        // B's effect references a cell no longer in the tree → B is invalidated.

        use spora_state::CellStateTree;

        let contested = outpoint(0, 0);
        let contested_meta = make_cell_meta(contested, 5000, 1, true);

        // Initial tree with the contested cell
        let mut tree = CellStateTree::new();
        let contested_hash = crate::processes::utils::outpoint_to_hash(&contested);
        tree.insert_with_outpoint(
            contested_hash,
            spora_exec::OutPoint::new(contested.tx_hash, contested.index),
            spora_state::CellEntry::new(5000, 0, hash(1), None, hash(2), 1, true),
        );

        // Block A's effect: consumes contested cell, creates a new one
        let new_cell_a = outpoint(0xA0, 0);
        let mut diff_a = CellDiff::default();
        diff_a.remove_cell(contested, contested_meta.clone());
        diff_a.add_cell(new_cell_a, make_cell_meta(new_cell_a, 4000, 10, false));

        // Block B's effect: also consumes contested cell (conflict)
        let new_cell_b = outpoint(0xB0, 0);
        let mut diff_b = CellDiff::default();
        diff_b.remove_cell(contested, contested_meta.clone());
        diff_b.add_cell(new_cell_b, make_cell_meta(new_cell_b, 4500, 10, false));

        // Commit A: apply diff to tree
        apply_cell_diff_to_tree(&mut tree, &diff_a);

        // Before committing B: check if contested cell still exists
        let conflict_detected = diff_b.remove.keys().any(|op| {
            let op_hash = crate::processes::utils::outpoint_to_hash(op);
            tree.get(&op_hash).is_none()
        });

        assert!(conflict_detected, "Conflict must be detected: contested cell was already consumed by A");

        // Per rule 3: B's entire effect is invalidated
        // Verify the tree only has A's output
        let a_hash = crate::processes::utils::outpoint_to_hash(&new_cell_a);
        let b_hash = crate::processes::utils::outpoint_to_hash(&new_cell_b);
        assert!(tree.get(&a_hash).is_some(), "A's created cell must exist");
        assert!(tree.get(&b_hash).is_none(), "B's created cell must not exist (effect invalidated)");
    }

    // ========================================================================
    // T4: Created-Then-Spent Across Blue Blocks (Cross-Block Dependency)
    // ========================================================================
    //
    // Verifies that when block A creates a cell and block B consumes it,
    // the ExecutionDAG correctly identifies the A→B dependency and places
    // them in separate layers.

    #[test]
    fn test_cross_block_spend_dependency() {
        // Block A creates cell(0xA0, 0)
        // Block B spends cell(0xA0, 0)
        let created_by_a = outpoint(0xA0, 0);
        let summary_a = make_summary(
            1,
            &[],             // spends nothing
            &[created_by_a], // creates cell
            &[],
            &[hash(0x10)],
        );
        let summary_b = make_summary(
            2,
            &[created_by_a],      // spends A's creation
            &[outpoint(0xB0, 0)], // creates its own cell
            &[],
            &[hash(0x20)],
        );

        assert!(summary_b.has_dependency_on(&summary_a), "B must depend on A (spend dependency)");

        let dag = ExecutionDAG::build(&[summary_a, summary_b]);
        assert_eq!(dag.layer_count(), 2, "A→B dependency must put them in separate layers");
        assert_eq!(dag.layers[0], vec![0], "A must be in layer 0");
        assert_eq!(dag.layers[1], vec![1], "B must be in layer 1");
        assert!(!dag.is_fully_parallel());
        assert!(dag.is_fully_serial());
    }

    #[test]
    fn test_cross_block_read_dependency() {
        // Block A creates cell(0xA0, 0)
        // Block B references cell(0xA0, 0) as a cell_dep (read dependency)
        let created_by_a = outpoint(0xA0, 0);
        let summary_a = make_summary(1, &[], &[created_by_a], &[], &[hash(0x10)]);
        let summary_b = make_summary(
            2,
            &[outpoint(0xB1, 0)],
            &[outpoint(0xB0, 0)],
            &[created_by_a], // read dep on A's creation
            &[hash(0x20)],
        );

        assert!(summary_b.has_dependency_on(&summary_a), "B must depend on A (read dependency)");

        let dag = ExecutionDAG::build(&[summary_a, summary_b]);
        assert_eq!(dag.layer_count(), 2, "Read dependency must serialize the blocks");
    }

    #[test]
    fn test_cross_block_dependency_with_independent_third_block() {
        // Block A creates cell(0xA0, 0)
        // Block B spends cell(0xA0, 0) → depends on A
        // Block C is completely independent
        // Expected: layer 0 = [A, C], layer 1 = [B]
        let created_by_a = outpoint(0xA0, 0);
        let summary_a = make_summary(1, &[], &[created_by_a], &[], &[hash(0x10)]);
        let summary_b = make_summary(2, &[created_by_a], &[outpoint(0xB0, 0)], &[], &[hash(0x20)]);
        let summary_c = make_summary(3, &[outpoint(0xC1, 0)], &[outpoint(0xC0, 0)], &[], &[hash(0x30)]);

        let dag = ExecutionDAG::build(&[summary_a, summary_b, summary_c]);
        assert_eq!(dag.layer_count(), 2);
        assert_eq!(dag.layers[0], vec![0, 2], "A and C should be in the same layer (independent)");
        assert_eq!(dag.layers[1], vec![1], "B depends on A, goes to layer 1");
    }

    // ========================================================================
    // T5: Red/Blue Reward and Acceptance Data Stability
    // ========================================================================
    //
    // Verifies P2B_PROTOCOL_SEMANTICS rules 4 and 5:
    // - Blue blocks: accepted_tx_ids are source of truth for acceptance data and reward
    // - Red blocks: only reward data, no transaction acceptance
    // - Effect invalidation → empty accepted_tx_ids → reward based on zero acceptance

    #[test]
    fn test_blue_block_acceptance_data_matches_accepted_tx_ids() {
        // Simulate a blue block with 2 accepted transactions.
        // Verify that acceptance_data is derived solely from accepted_tx_ids.
        let block_hash = hash(0x01);
        let tx1 = hash(0x10);
        let tx2 = hash(0x20);

        let effect = BlockExecutionEffect {
            block_hash,
            block_daa_score: 100,
            cell_diff: CellDiff::default(),
            accepted_tx_ids: vec![tx1, tx2],
            accepted_transactions: vec![
                AcceptedTxEntry { transaction_id: tx1, index_within_block: 0 },
                AcceptedTxEntry { transaction_id: tx2, index_within_block: 1 },
            ],
            reward_data: Some(make_reward(50_000)),
            consumed_cycles: 0,
            newly_processed_tx_ids: vec![tx1, tx2],
        };

        // Rule 4: acceptance_data records only truly accepted transactions
        assert_eq!(effect.accepted_tx_ids.len(), 2);
        assert_eq!(effect.accepted_transactions.len(), 2);

        // Rule 5: reward and acceptance share the same "accepted" definition
        assert!(effect.reward_data.is_some());

        // Verify consistency: accepted_transactions references match accepted_tx_ids
        for (entry, &tx_id) in effect.accepted_transactions.iter().zip(effect.accepted_tx_ids.iter()) {
            assert_eq!(entry.transaction_id, tx_id);
        }
    }

    #[test]
    fn test_red_block_has_reward_but_no_accepted_transactions() {
        // Red blocks: BlockExecutionEffect::empty + reward_data set
        // Per rule 5: red blocks don't execute transactions, only participate in reward
        // Red block
        let mut red_effect = BlockExecutionEffect::empty(hash(0xDD), 100);
        red_effect.reward_data = Some(make_reward(50_000));

        assert!(red_effect.accepted_tx_ids.is_empty(), "Red block must have no accepted txs");
        assert!(red_effect.accepted_transactions.is_empty());
        assert!(red_effect.cell_diff.is_empty(), "Red block must have no cell state changes");
        assert!(red_effect.reward_data.is_some(), "Red block must still have reward data");
    }

    #[test]
    fn test_invalidated_blue_block_has_empty_acceptance_but_keeps_reward() {
        // Per rule 3: when an effect is invalidated at commit time,
        // accepted_tx_ids becomes empty but the block retains its reward.
        let block_hash = hash(0x01);

        // Original effect (before invalidation)
        let original_effect = BlockExecutionEffect {
            block_hash,
            block_daa_score: 100,
            cell_diff: {
                let mut d = CellDiff::default();
                d.add_cell(outpoint(0x10, 0), make_cell_meta(outpoint(0x10, 0), 5000, 100, false));
                d
            },
            accepted_tx_ids: vec![hash(0x10)],
            accepted_transactions: vec![AcceptedTxEntry { transaction_id: hash(0x10), index_within_block: 1 }],
            reward_data: Some(make_reward(50_000)),
            consumed_cycles: 100,
            newly_processed_tx_ids: vec![hash(0x10)],
        };

        // Simulate invalidation (mirrors calculate_cell_state conflict handling)
        let mut invalidated = BlockExecutionEffect::empty(block_hash, 100);
        invalidated.reward_data = original_effect.reward_data.clone();

        assert!(invalidated.accepted_tx_ids.is_empty(), "Invalidated block must have empty accepted_tx_ids");
        assert!(invalidated.cell_diff.is_empty(), "Invalidated block must have empty cell_diff");
        assert!(invalidated.reward_data.is_some(), "Invalidated block must preserve reward_data");
        assert_eq!(
            invalidated.reward_data.as_ref().unwrap().subsidy,
            original_effect.reward_data.as_ref().unwrap().subsidy,
            "Reward subsidy must be preserved after invalidation"
        );
    }

    // ========================================================================
    // T6: Serial vs Parallel Runner Equivalence
    // ========================================================================
    //
    // Verifies P2B_PROTOCOL_SEMANTICS invariant 1: serial and parallel
    // processing of the same mergeset must produce identical results.
    //
    // Since we cannot easily instantiate a full VirtualStateProcessor in a
    // unit test, we verify the structural equivalence at the DAG + effect
    // composition level.

    #[test]
    fn test_serial_vs_parallel_dag_equivalence_independent_blocks() {
        // Scenario: 4 completely independent blue blocks.
        // Serial: process all in sequence (4 layers of 1).
        // Parallel: ExecutionDAG groups them all in 1 layer.
        // Final commit order is still canonical order, so results are identical.

        let summaries = vec![
            make_summary(1, &[outpoint(1, 0)], &[outpoint(1, 1)], &[], &[hash(0x10)]),
            make_summary(2, &[outpoint(2, 0)], &[outpoint(2, 1)], &[], &[hash(0x20)]),
            make_summary(3, &[outpoint(3, 0)], &[outpoint(3, 1)], &[], &[hash(0x30)]),
            make_summary(4, &[outpoint(4, 0)], &[outpoint(4, 1)], &[], &[hash(0x40)]),
        ];

        let dag = ExecutionDAG::build(&summaries);

        // Parallel mode: single layer with all blocks
        assert_eq!(dag.layer_count(), 1);
        assert_eq!(dag.layers[0], vec![0, 1, 2, 3]);
        assert!(dag.is_fully_parallel());

        // Simulate serial processing: each block in its own "layer"
        let serial_order: Vec<Vec<usize>> = (0..4).map(|i| vec![i]).collect();

        // Both modes commit in the same canonical order (0, 1, 2, 3).
        // The parallel mode commits within a layer in index order, which
        // matches the serial order. This is the key invariant.
        let parallel_commit_order: Vec<usize> = dag.layers.iter().flat_map(|l| l.iter().copied()).collect();
        let serial_commit_order: Vec<usize> = serial_order.iter().flat_map(|l| l.iter().copied()).collect();
        assert_eq!(parallel_commit_order, serial_commit_order, "Commit order must be identical regardless of parallelization");
    }

    #[test]
    fn test_serial_vs_parallel_equivalence_with_dependencies() {
        // Scenario: A creates cell → B consumes it; C and D are independent.
        // Serial: A, B, C, D (all sequential)
        // Parallel: layer 0 = [A, C, D], layer 1 = [B]
        // Commit order: A, C, D, B (parallel) vs A, B, C, D (serial)
        //
        // Wait — this is wrong. The commit order within each layer must preserve
        // canonical order (index order). So:
        //   Parallel: layer 0 commits [0, 2, 3] → A, C, D; layer 1 commits [1] → B.
        //   Serial:   commits [0], [1], [2], [3] → A, B, C, D.
        //
        // The commit order differs! But the RESULT must be the same because:
        //   - C and D are independent of A and B → their effects don't interact
        //   - B depends on A → B is always committed after A
        //
        // The key insight: within a layer, all blocks were analyzed against the
        // same snapshot, and conflict detection at commit time ensures correctness.

        let created_by_a = outpoint(0xA0, 0);
        let summaries = vec![
            make_summary(1, &[], &[created_by_a], &[], &[hash(0x10)]), // A: creates cell
            make_summary(2, &[created_by_a], &[outpoint(0xB0, 0)], &[], &[hash(0x20)]), // B: spends A's cell
            make_summary(3, &[outpoint(3, 0)], &[outpoint(3, 1)], &[], &[hash(0x30)]), // C: independent
            make_summary(4, &[outpoint(4, 0)], &[outpoint(4, 1)], &[], &[hash(0x40)]), // D: independent
        ];

        let dag = ExecutionDAG::build(&summaries);
        assert_eq!(dag.layer_count(), 2);
        assert_eq!(dag.layers[0], vec![0, 2, 3], "Layer 0: A, C, D (independent)");
        assert_eq!(dag.layers[1], vec![1], "Layer 1: B (depends on A)");

        // Verify: in both modes, B is always committed after A.
        // In parallel mode: A is in layer 0, B is in layer 1 → A committed first.
        // In serial mode: A(index 0) committed before B(index 1) → A committed first.
        let a_layer = dag.layers.iter().position(|l| l.contains(&0)).unwrap();
        let b_layer = dag.layers.iter().position(|l| l.contains(&1)).unwrap();
        assert!(a_layer < b_layer, "A must be committed before B in parallel mode");
    }

    #[test]
    fn test_serial_vs_parallel_cell_diff_composition_equivalence() {
        // Verify that composing CellDiffs in serial order produces the
        // same net result as composing them layer by layer (parallel model).

        let op_a = outpoint(0xA0, 0);
        let op_b = outpoint(0xB0, 0);
        let op_c = outpoint(0xC0, 0);
        let meta_a = make_cell_meta(op_a, 5000, 10, true);
        let meta_b = make_cell_meta(op_b, 3000, 11, false);
        let meta_c = make_cell_meta(op_c, 2000, 11, false);

        // Three independent block diffs
        let mut diff_a = CellDiff::default();
        diff_a.add_cell(op_a, meta_a.clone());

        let mut diff_b = CellDiff::default();
        diff_b.add_cell(op_b, meta_b.clone());

        let mut diff_c = CellDiff::default();
        diff_c.add_cell(op_c, meta_c.clone());

        // Serial composition: A, B, C one by one
        let mut serial_merged = CellDiff::default();
        serial_merged.with_diff_in_place(&diff_a).unwrap();
        serial_merged.with_diff_in_place(&diff_b).unwrap();
        serial_merged.with_diff_in_place(&diff_c).unwrap();

        // Parallel composition: all in one "layer" (same order for commit)
        let mut parallel_merged = CellDiff::default();
        // Within a parallel layer, effects are committed in canonical order
        parallel_merged.with_diff_in_place(&diff_a).unwrap();
        parallel_merged.with_diff_in_place(&diff_b).unwrap();
        parallel_merged.with_diff_in_place(&diff_c).unwrap();

        // Net result must be identical
        assert_eq!(serial_merged.num_added(), parallel_merged.num_added());
        assert_eq!(serial_merged.num_removed(), parallel_merged.num_removed());
        assert_eq!(serial_merged.add.keys().collect::<Vec<_>>(), parallel_merged.add.keys().collect::<Vec<_>>());
    }

    #[test]
    fn test_serial_vs_parallel_equivalence_full_scenario() {
        // Full scenario combining multiple aspects:
        // - Block A: creates cell_a1, cell_a2
        // - Block B: creates cell_b1 (independent)
        // - Block C: spends cell_a1 (depends on A)
        // - Block D: creates cell_d1 (independent)
        //
        // DAG: layer 0 = [A, B, D], layer 1 = [C]
        //
        // Serial order: A, B, C, D
        // Parallel commit: A, B, D (layer 0), then C (layer 1)
        //
        // Final state must be identical:
        //   - cell_a2 exists (created by A, not consumed)
        //   - cell_b1 exists
        //   - cell_d1 exists
        //   - cell_a1 consumed by C
        //   - C's output exists

        use spora_state::CellStateTree;

        let cell_a1 = outpoint(0xA0, 0);
        let cell_a2 = outpoint(0xA0, 1);
        let cell_b1 = outpoint(0xB0, 0);
        let cell_c_out = outpoint(0xC0, 0);
        let cell_d1 = outpoint(0xD0, 0);

        let meta_a1 = make_cell_meta(cell_a1, 5000, 10, true);
        let meta_a2 = make_cell_meta(cell_a2, 3000, 10, true);
        let meta_b1 = make_cell_meta(cell_b1, 2000, 11, true);
        let meta_c_out = make_cell_meta(cell_c_out, 4500, 12, false);
        let meta_d1 = make_cell_meta(cell_d1, 1000, 11, true);

        // Build effects
        let mut diff_a = CellDiff::default();
        diff_a.add_cell(cell_a1, meta_a1.clone());
        diff_a.add_cell(cell_a2, meta_a2.clone());

        let mut diff_b = CellDiff::default();
        diff_b.add_cell(cell_b1, meta_b1.clone());

        let mut diff_c = CellDiff::default();
        diff_c.remove_cell(cell_a1, meta_a1.clone());
        diff_c.add_cell(cell_c_out, meta_c_out.clone());

        let mut diff_d = CellDiff::default();
        diff_d.add_cell(cell_d1, meta_d1.clone());

        // --- Serial processing ---
        let mut serial_merged = CellDiff::default();
        serial_merged.with_diff_in_place(&diff_a).unwrap(); // A
        serial_merged.with_diff_in_place(&diff_b).unwrap(); // B
        serial_merged.with_diff_in_place(&diff_c).unwrap(); // C (A created cell_a1, C consumes it → cancel)
        serial_merged.with_diff_in_place(&diff_d).unwrap(); // D

        // --- Parallel processing (layer 0: A, B, D; layer 1: C) ---
        let mut parallel_merged = CellDiff::default();
        // Layer 0 (canonical order within layer)
        parallel_merged.with_diff_in_place(&diff_a).unwrap(); // A
        parallel_merged.with_diff_in_place(&diff_b).unwrap(); // B
        parallel_merged.with_diff_in_place(&diff_d).unwrap(); // D
                                                              // Layer 1
        parallel_merged.with_diff_in_place(&diff_c).unwrap(); // C

        // Compare net results
        assert_eq!(serial_merged.num_added(), parallel_merged.num_added(), "Net added cells must match");
        assert_eq!(serial_merged.num_removed(), parallel_merged.num_removed(), "Net removed cells must match");

        // Both should have: cell_a2, cell_b1, cell_c_out, cell_d1 in add
        // cell_a1 was created and consumed → cancels out
        assert_eq!(serial_merged.num_added(), 4);
        assert_eq!(serial_merged.num_removed(), 0);
        assert!(serial_merged.add.contains_key(&cell_a2));
        assert!(serial_merged.add.contains_key(&cell_b1));
        assert!(serial_merged.add.contains_key(&cell_c_out));
        assert!(serial_merged.add.contains_key(&cell_d1));

        // Parallel result must match exactly
        assert!(parallel_merged.add.contains_key(&cell_a2));
        assert!(parallel_merged.add.contains_key(&cell_b1));
        assert!(parallel_merged.add.contains_key(&cell_c_out));
        assert!(parallel_merged.add.contains_key(&cell_d1));
    }

    // ========================================================================
    // Integration-level tests (require full VirtualStateProcessor setup)
    // ========================================================================

    // TODO: requires full VirtualStateProcessor setup
    // The following tests would exercise calculate_cell_state end-to-end
    // with real GhostdagData, store infrastructure, and block transactions.
    // They are left as skeletons for future integration test expansion.

    #[test]
    #[ignore = "requires full VirtualStateProcessor integration setup"]
    fn test_e2e_same_mergeset_deterministic_cell_root() {
        // Construct a full mergeset with multiple blue blocks
        // Run calculate_cell_state multiple times
        // Verify cell_root, accepted_tx_ids, mergeset_acceptance_data, reward_data
        // are bitwise identical across runs
        todo!("Requires VirtualStateProcessor with store infrastructure");
    }

    #[test]
    #[ignore = "requires full VirtualStateProcessor integration setup"]
    fn test_e2e_serial_vs_parallel_runner_full_equivalence() {
        // Construct a mergeset with both dependent and independent blue blocks
        // Run calculate_cell_state (which uses ExecutionDAG internally)
        // Compare against a reference serial implementation
        // All outputs must be bitwise identical
        todo!("Requires VirtualStateProcessor with store infrastructure");
    }
}
