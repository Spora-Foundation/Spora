// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Spora developers
//
// Block-level execution DAG for MPE parallelization

use super::access_summary::BlockAccessSummary;

/// Block-level 执行 DAG，将 mergeset 中的 blue blocks 按读写依赖分层。
/// 同一层内的 blocks 互不冲突，可以并行分析。
/// 不同层之间有 barrier，必须按层顺序执行。
///
/// 注意：这个 DAG 是执行层 DAG，不是 GhostDAG。
/// 它不改变最终 canonical order，只表示"哪些 blocks 可以在同一前置状态下分析"。
///
/// # Snapshot 一致性不变量
///
/// 同一层内所有 blocks 的 `analyze_blue_block` 调用必须基于同一个 frozen snapshot。
/// 这是执行等价性的核心保证：并行分析产生的 effects 与串行执行等价，
/// 因为所有同层 blocks 看到的是同一个不可变状态视图。
/// 违反此不变量会导致执行等价性被破坏。
pub(super) struct ExecutionDAG {
    /// 分层结果，每层包含可并行分析的 block 索引
    /// 索引对应输入 summaries 数组的位置
    pub layers: Vec<Vec<usize>>,
}

impl ExecutionDAG {
    /// 从 BlockAccessSummary 列表构建执行 DAG。
    ///
    /// summaries 必须按 GhostDAG canonical order 排列。
    ///
    /// 算法：
    /// 1. 从 GhostDAG canonical order 的当前位置开始构建一个连续层。
    /// 2. 只要下一个 block 不依赖当前层中任一 block，就加入当前层。
    /// 3. 一旦遇到依赖当前层的 block，在它之前切层。
    ///
    /// 这样每层内部仍可共享一个 snapshot 并行分析，同时所有层拼接后的
    /// commit 顺序严格等于输入的 canonical order。后序独立块不会被提前
    /// 提交到更早 canonical block 之前，避免 `accepted_tx_ids` /
    /// acceptance_data 的顺序和串行 reference runner 分叉。
    pub fn build(summaries: &[BlockAccessSummary]) -> Self {
        let n = summaries.len();

        if n == 0 {
            return Self { layers: Vec::new() };
        }

        let mut layers = Vec::new();
        let mut layer_start = 0usize;

        while layer_start < n {
            let mut layer = vec![layer_start];
            let mut next = layer_start + 1;

            while next < n {
                if layer.iter().any(|&earlier_in_layer| summaries[next].has_dependency_on(&summaries[earlier_in_layer])) {
                    break;
                }

                layer.push(next);
                next += 1;
            }

            layers.push(layer);
            layer_start = next;
        }

        Self { layers }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spora_consensus_core::tx::TransactionOutpoint;
    use spora_hashes::Hash;

    fn outpoint(tx: u8, idx: u32) -> TransactionOutpoint {
        TransactionOutpoint { tx_hash: [tx; 32], index: idx }
    }

    fn hash(v: u8) -> Hash {
        Hash::from_bytes([v; 32])
    }

    fn make_summary(
        _block: u8,
        spent: &[TransactionOutpoint],
        created: &[TransactionOutpoint],
        read: &[TransactionOutpoint],
        txs: &[Hash],
    ) -> BlockAccessSummary {
        BlockAccessSummary {
            spent_outpoints: spent.iter().cloned().collect(),
            created_outpoints: created.iter().cloned().collect(),
            read_deps: read.iter().cloned().collect(),
            tx_ids: txs.iter().cloned().collect(),
            cellscript_shared_reads: Default::default(),
            cellscript_shared_writes: Default::default(),
        }
    }

    #[test]
    fn empty_summaries() {
        let dag = ExecutionDAG::build(&[]);
        assert_eq!(dag.layers.len(), 0);
        assert_eq!(dag.layers.iter().map(Vec::len).sum::<usize>(), 0);
        assert!(dag.layers.len() <= 1);
    }

    #[test]
    fn single_block() {
        let summaries = vec![make_summary(1, &[outpoint(0, 0)], &[outpoint(1, 0)], &[], &[hash(0x10)])];
        let dag = ExecutionDAG::build(&summaries);
        assert_eq!(dag.layers.len(), 1);
        assert_eq!(dag.layers.iter().map(Vec::len).sum::<usize>(), 1);
        assert!(dag.layers.len() <= 1);
        assert!(dag.layers.iter().all(|layer| layer.len() <= 1));
    }

    #[test]
    fn all_independent_blocks_single_layer() {
        // 三个完全独立的 block，读写集无交集
        let summaries = vec![
            make_summary(1, &[outpoint(1, 0)], &[outpoint(1, 1)], &[], &[hash(0x10)]),
            make_summary(2, &[outpoint(2, 0)], &[outpoint(2, 1)], &[], &[hash(0x20)]),
            make_summary(3, &[outpoint(3, 0)], &[outpoint(3, 1)], &[], &[hash(0x30)]),
        ];
        let dag = ExecutionDAG::build(&summaries);
        assert_eq!(dag.layers.len(), 1);
        assert_eq!(dag.layers[0], vec![0, 1, 2]);
        assert!(dag.layers.len() <= 1);
        assert!(!dag.layers.iter().all(|layer| layer.len() <= 1));
    }

    #[test]
    fn fully_serial_chain() {
        // A 创建 cell → B 消费 → B 创建 → C 消费
        let summaries = vec![
            make_summary(1, &[], &[outpoint(1, 0)], &[], &[hash(0x10)]),
            make_summary(2, &[outpoint(1, 0)], &[outpoint(2, 0)], &[], &[hash(0x20)]),
            make_summary(3, &[outpoint(2, 0)], &[], &[], &[hash(0x30)]),
        ];
        let dag = ExecutionDAG::build(&summaries);
        assert_eq!(dag.layers.len(), 3);
        assert_eq!(dag.layers, vec![vec![0], vec![1], vec![2]]);
        assert!(dag.layers.iter().all(|layer| layer.len() <= 1));
        assert!(dag.layers.len() > 1);
    }

    #[test]
    fn mixed_dependencies() {
        // Block 0: 创建 cell(1,0) 和 cell(1,1)
        // Block 1: 消费 cell(1,0) → 依赖 block 0
        // Block 2: 消费 cell(1,1) → 依赖 block 0
        // Block 3: 独立，但在 canonical order 中位于依赖 block 之后；
        // 为保持全局 commit order，它不能提前到 block 1/2 之前提交。
        // 期望：layer 0 = [0], layer 1 = [1, 2, 3]
        let summaries = vec![
            make_summary(1, &[], &[outpoint(1, 0), outpoint(1, 1)], &[], &[hash(0x10)]),
            make_summary(2, &[outpoint(1, 0)], &[], &[], &[hash(0x20)]),
            make_summary(3, &[outpoint(1, 1)], &[], &[], &[hash(0x30)]),
            make_summary(4, &[outpoint(4, 0)], &[], &[], &[hash(0x40)]),
        ];
        let dag = ExecutionDAG::build(&summaries);
        assert_eq!(dag.layers.len(), 2);
        assert_eq!(dag.layers[0], vec![0]);
        assert_eq!(dag.layers[1], vec![1, 2, 3]);
    }

    #[test]
    fn diamond_dependency() {
        // A → B, A → C, B → D, C → D
        // A 创建 cell(1,0) 和 cell(1,1)
        // B 消费 cell(1,0)，创建 cell(2,0)
        // C 消费 cell(1,1)，创建 cell(3,0)
        // D 消费 cell(2,0) 和 cell(3,0)
        let summaries = vec![
            make_summary(1, &[], &[outpoint(1, 0), outpoint(1, 1)], &[], &[hash(0x10)]),
            make_summary(2, &[outpoint(1, 0)], &[outpoint(2, 0)], &[], &[hash(0x20)]),
            make_summary(3, &[outpoint(1, 1)], &[outpoint(3, 0)], &[], &[hash(0x30)]),
            make_summary(4, &[outpoint(2, 0), outpoint(3, 0)], &[], &[], &[hash(0x40)]),
        ];
        let dag = ExecutionDAG::build(&summaries);
        assert_eq!(dag.layers.len(), 3);
        assert_eq!(dag.layers[0], vec![0]);
        assert_eq!(dag.layers[1], vec![1, 2]);
        assert_eq!(dag.layers[2], vec![3]);
    }

    #[test]
    fn double_spend_conflict_serialized() {
        // Block 0 和 Block 1 消费同一个 cell → double spend 冲突 → 串行
        let summaries = vec![
            make_summary(1, &[outpoint(0, 0)], &[], &[], &[hash(0x10)]),
            make_summary(2, &[outpoint(0, 0)], &[], &[], &[hash(0x20)]),
        ];
        let dag = ExecutionDAG::build(&summaries);
        assert_eq!(dag.layers.len(), 2);
        assert_eq!(dag.layers, vec![vec![0], vec![1]]);
    }

    #[test]
    fn duplicate_tx_conflict_serialized() {
        // Block 0 和 Block 1 包含相同 tx → 串行
        let shared = hash(0xAA);
        let summaries = vec![make_summary(1, &[], &[], &[], &[shared]), make_summary(2, &[], &[], &[], &[shared])];
        let dag = ExecutionDAG::build(&summaries);
        assert_eq!(dag.layers.len(), 2);
        assert_eq!(dag.layers, vec![vec![0], vec![1]]);
    }

    #[test]
    fn read_dep_creates_dependency() {
        // Block 0 创建 cell(1,0)，Block 1 的 cell_deps 引用 cell(1,0) → 依赖
        let summaries = vec![
            make_summary(1, &[], &[outpoint(1, 0)], &[], &[hash(0x10)]),
            make_summary(2, &[], &[], &[outpoint(1, 0)], &[hash(0x20)]),
        ];
        let dag = ExecutionDAG::build(&summaries);
        assert_eq!(dag.layers.len(), 2);
        assert_eq!(dag.layers, vec![vec![0], vec![1]]);
    }

    #[test]
    fn read_dep_on_spent_cell_serialized() {
        // Block 0 消费 cell(1,0)，Block 1 的 cell_deps 引用 cell(1,0) → 依赖
        let summaries = vec![
            make_summary(1, &[outpoint(1, 0)], &[], &[], &[hash(0x10)]),
            make_summary(2, &[], &[], &[outpoint(1, 0)], &[hash(0x20)]),
        ];
        let dag = ExecutionDAG::build(&summaries);
        assert_eq!(dag.layers.len(), 2);
        assert_eq!(dag.layers, vec![vec![0], vec![1]]);
    }

    #[test]
    fn shared_write_touch_serializes_blocks() {
        let shared = hash(0x42);
        let mut first = make_summary(1, &[], &[], &[], &[hash(0x10)]);
        first.cellscript_shared_writes.insert(shared);
        let mut second = make_summary(2, &[], &[], &[], &[hash(0x20)]);
        second.cellscript_shared_writes.insert(shared);

        let dag = ExecutionDAG::build(&[first, second]);
        assert_eq!(dag.layers.len(), 2);
        assert_eq!(dag.layers, vec![vec![0], vec![1]]);
    }
}
