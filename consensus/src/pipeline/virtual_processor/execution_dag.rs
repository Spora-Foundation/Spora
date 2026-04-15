// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Block-level execution DAG for P2B parallelization

use std::collections::BTreeMap;

use spora_hashes::Hash;

use super::access_summary::BlockAccessSummary;

/// Block-level 执行 DAG，将 mergeset 中的 blue blocks 按读写依赖分层。
/// 同一层内的 blocks 互不冲突，可以并行分析。
/// 不同层之间有 barrier，必须按层顺序执行。
///
/// 注意：这个 DAG 是执行层 DAG，不是 GhostDAG。
/// 它不改变最终 canonical order，只表示"哪些 blocks 可以在同一前置状态下分析"。
pub(super) struct ExecutionDAG {
    /// 分层结果，每层包含可并行分析的 block 索引
    /// 索引对应输入 summaries 数组的位置
    pub layers: Vec<Vec<usize>>,
    /// 原始 block hashes，按 GhostDAG canonical order
    pub block_hashes: Vec<Hash>,
}

impl ExecutionDAG {
    /// 从 BlockAccessSummary 列表构建执行 DAG。
    ///
    /// summaries 必须按 GhostDAG canonical order 排列。
    ///
    /// 算法：
    /// 1. 构建依赖图：如果 block[j] 依赖 block[i]（i < j），则 i → j 有边
    /// 2. Kahn 拓扑排序得到分层
    /// 3. 同一层内的 blocks 互不冲突
    pub fn build(summaries: &[BlockAccessSummary]) -> Self {
        let n = summaries.len();
        let block_hashes: Vec<Hash> = summaries.iter().map(|s| s.block_hash).collect();

        if n == 0 {
            return Self { layers: Vec::new(), block_hashes };
        }

        // Step 1: 构建邻接表和入度表
        let mut adj: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        let mut in_degree = vec![0usize; n];

        for j in 0..n {
            for i in 0..j {
                if summaries[j].has_dependency_on(&summaries[i]) {
                    adj.entry(i).or_default().push(j);
                    in_degree[j] += 1;
                }
            }
        }

        // Step 2: Kahn 算法分层
        let mut layers = Vec::new();
        let mut current_layer: Vec<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();

        let mut processed = 0usize;

        while !current_layer.is_empty() {
            current_layer.sort_unstable();
            processed += current_layer.len();
            let mut next_layer = Vec::new();

            for &node in &current_layer {
                if let Some(successors) = adj.get(&node) {
                    for &succ in successors {
                        in_degree[succ] -= 1;
                        if in_degree[succ] == 0 {
                            next_layer.push(succ);
                        }
                    }
                }
            }

            layers.push(std::mem::take(&mut current_layer));
            current_layer = next_layer;
        }

        // Block-level DAG 不应有环（canonical order 保证 i < j 的单向边）
        debug_assert_eq!(processed, n, "ExecutionDAG: cycle detected, processed {processed} but expected {n}");

        Self { layers, block_hashes }
    }

    /// 返回层数
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// 返回总 block 数
    pub fn block_count(&self) -> usize {
        self.block_hashes.len()
    }

    /// 判断 DAG 是否为"全串行"（每层只有一个 block）
    /// 用于决定是否值得使用并行模式
    pub fn is_fully_serial(&self) -> bool {
        self.layers.iter().all(|layer| layer.len() <= 1)
    }

    /// 判断 DAG 是否为"全并行"（只有一层）
    pub fn is_fully_parallel(&self) -> bool {
        self.layers.len() <= 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spora_consensus_core::tx::TransactionOutpoint;

    fn outpoint(tx: u8, idx: u32) -> TransactionOutpoint {
        TransactionOutpoint { tx_hash: [tx; 32], index: idx }
    }

    fn hash(v: u8) -> Hash {
        Hash::from_bytes([v; 32])
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

    #[test]
    fn empty_summaries() {
        let dag = ExecutionDAG::build(&[]);
        assert_eq!(dag.layer_count(), 0);
        assert_eq!(dag.block_count(), 0);
        assert!(dag.is_fully_parallel());
    }

    #[test]
    fn single_block() {
        let summaries = vec![make_summary(1, &[outpoint(0, 0)], &[outpoint(1, 0)], &[], &[hash(0x10)])];
        let dag = ExecutionDAG::build(&summaries);
        assert_eq!(dag.layer_count(), 1);
        assert_eq!(dag.block_count(), 1);
        assert!(dag.is_fully_parallel());
        assert!(dag.is_fully_serial());
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
        assert_eq!(dag.layer_count(), 1);
        assert_eq!(dag.layers[0], vec![0, 1, 2]);
        assert!(dag.is_fully_parallel());
        assert!(!dag.is_fully_serial());
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
        assert_eq!(dag.layer_count(), 3);
        assert_eq!(dag.layers, vec![vec![0], vec![1], vec![2]]);
        assert!(dag.is_fully_serial());
        assert!(!dag.is_fully_parallel());
    }

    #[test]
    fn mixed_dependencies() {
        // Block 0: 创建 cell(1,0) 和 cell(1,1)
        // Block 1: 消费 cell(1,0) → 依赖 block 0
        // Block 2: 消费 cell(1,1) → 依赖 block 0
        // Block 3: 独立
        // 期望：layer 0 = [0, 3], layer 1 = [1, 2]
        let summaries = vec![
            make_summary(1, &[], &[outpoint(1, 0), outpoint(1, 1)], &[], &[hash(0x10)]),
            make_summary(2, &[outpoint(1, 0)], &[], &[], &[hash(0x20)]),
            make_summary(3, &[outpoint(1, 1)], &[], &[], &[hash(0x30)]),
            make_summary(4, &[outpoint(4, 0)], &[], &[], &[hash(0x40)]),
        ];
        let dag = ExecutionDAG::build(&summaries);
        assert_eq!(dag.layer_count(), 2);
        assert_eq!(dag.layers[0], vec![0, 3]);
        assert_eq!(dag.layers[1], vec![1, 2]);
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
        assert_eq!(dag.layer_count(), 3);
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
        assert_eq!(dag.layer_count(), 2);
        assert_eq!(dag.layers, vec![vec![0], vec![1]]);
    }

    #[test]
    fn duplicate_tx_conflict_serialized() {
        // Block 0 和 Block 1 包含相同 tx → 串行
        let shared = hash(0xAA);
        let summaries = vec![
            make_summary(1, &[], &[], &[], &[shared]),
            make_summary(2, &[], &[], &[], &[shared]),
        ];
        let dag = ExecutionDAG::build(&summaries);
        assert_eq!(dag.layer_count(), 2);
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
        assert_eq!(dag.layer_count(), 2);
        assert_eq!(dag.layers, vec![vec![0], vec![1]]);
    }
}
