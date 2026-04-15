// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Block-level access summary for P2B parallelization dependency analysis

use std::collections::BTreeSet;

use spora_consensus_core::tx::TransactionOutpoint;
use spora_hashes::Hash;

use super::cell_processing::BlockExecutionEffect;

/// 块级访问摘要，记录一个 blue block 的完整读写集。
/// 用于判断两个 blue blocks 之间是否存在数据依赖，
/// 从而决定它们能否在同一层并行分析。
pub(super) struct BlockAccessSummary {
    /// 块哈希
    pub block_hash: Hash,
    /// 该块消费的 outpoints（inputs）
    pub spent_outpoints: BTreeSet<TransactionOutpoint>,
    /// 该块创建的 outpoints（outputs）
    pub created_outpoints: BTreeSet<TransactionOutpoint>,
    /// 该块读取依赖的 outpoints（cell_deps，非消费性引用）
    pub read_deps: BTreeSet<TransactionOutpoint>,
    /// 该块包含的交易 ID
    pub tx_ids: BTreeSet<Hash>,
}

impl BlockAccessSummary {
    /// 从原始交易列表静态提取访问摘要（不需要执行分析）。
    ///
    /// 这用于在分析前构建 ExecutionDAG：先从 block_txs 提取读写集，
    /// 再用读写集构建依赖图和分层结构，最后按层并行执行 analyze_blue_block。
    ///
    /// 注意：静态提取是保守的 —— 它包含块中所有交易的读写集，
    /// 包括可能因 dedup 或 cycles 限制而最终被跳过的交易。
    /// 这只会导致假依赖（更多串行），不会遗漏真依赖。
    pub fn from_block_txs(block_hash: Hash, block_txs: &[spora_exec::CellTx]) -> Self {
        let mut spent_outpoints = BTreeSet::new();
        let mut created_outpoints = BTreeSet::new();
        let mut read_deps = BTreeSet::new();
        let mut tx_ids = BTreeSet::new();

        for tx in block_txs {
            tx_ids.insert(Hash::from_bytes(tx.id()));

            // inputs → spent_outpoints
            for input in &tx.inputs {
                spent_outpoints
                    .insert(TransactionOutpoint { tx_hash: input.previous_output.tx_hash, index: input.previous_output.index });
            }

            // outputs → created_outpoints
            for (idx, _output) in tx.outputs.iter().enumerate() {
                created_outpoints.insert(TransactionOutpoint { tx_hash: tx.id(), index: idx as u32 });
            }

            // cell_deps → read_deps
            for dep in &tx.cell_deps {
                read_deps.insert(TransactionOutpoint { tx_hash: dep.out_point.tx_hash, index: dep.out_point.index });
            }
        }

        Self { block_hash, spent_outpoints, created_outpoints, read_deps, tx_ids }
    }

    /// 从 BlockExecutionEffect 提取访问摘要。
    ///
    /// `block_txs` 用于提取 `cell_deps`（非消费性读依赖），
    /// 因为 `BlockExecutionEffect` 仅记录了 cell_diff，不包含原始交易的 dep 信息。
    pub fn from_effect(effect: &BlockExecutionEffect, block_txs: &[spora_exec::CellTx]) -> Self {
        let spent_outpoints: BTreeSet<TransactionOutpoint> = effect.cell_diff.remove.keys().cloned().collect();
        let created_outpoints: BTreeSet<TransactionOutpoint> = effect.cell_diff.add.keys().cloned().collect();

        // 从原始交易的 cell_deps 提取非消费性读依赖
        let mut read_deps = BTreeSet::new();
        for tx in block_txs {
            for dep in &tx.cell_deps {
                read_deps.insert(TransactionOutpoint { tx_hash: dep.out_point.tx_hash, index: dep.out_point.index });
            }
        }

        // tx_ids = accepted_tx_ids ∪ newly_processed_tx_ids
        let mut tx_ids = BTreeSet::new();
        for id in &effect.accepted_tx_ids {
            tx_ids.insert(*id);
        }
        for id in &effect.newly_processed_tx_ids {
            tx_ids.insert(*id);
        }

        Self { block_hash: effect.block_hash, spent_outpoints, created_outpoints, read_deps, tx_ids }
    }

    /// 判断两个 block 之间是否存在依赖关系。
    /// 有依赖意味着不能并行分析。
    ///
    /// `self` 是时间上较晚的块，`earlier` 是时间上较早的块。
    ///
    /// 依赖条件（任意一个成立即有依赖）：
    /// 1. self.spent_outpoints ∩ earlier.created_outpoints ≠ ∅（消费依赖）
    /// 2. self.spent_outpoints ∩ earlier.spent_outpoints ≠ ∅（double spend 冲突）
    /// 3. self.tx_ids ∩ earlier.tx_ids ≠ ∅（duplicate tx）
    /// 4. self.read_deps ∩ earlier.created_outpoints ≠ ∅（读依赖）
    pub fn has_dependency_on(&self, earlier: &BlockAccessSummary) -> bool {
        // 1. 消费依赖：self 消费了 earlier 创建的 cell
        if self.spent_outpoints.intersection(&earlier.created_outpoints).next().is_some() {
            return true;
        }

        // 2. Double spend 冲突：两个块消费了同一个 cell
        if self.spent_outpoints.intersection(&earlier.spent_outpoints).next().is_some() {
            return true;
        }

        // 3. Duplicate tx：两个块包含相同的交易
        if self.tx_ids.intersection(&earlier.tx_ids).next().is_some() {
            return true;
        }

        // 4. 读依赖：self 的 cell_deps 引用了 earlier 创建的 cell
        if self.read_deps.intersection(&earlier.created_outpoints).next().is_some() {
            return true;
        }

        false
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
    fn no_dependency_when_disjoint() {
        let a = make_summary(1, &[outpoint(1, 0)], &[outpoint(1, 1)], &[], &[hash(0x10)]);
        let b = make_summary(2, &[outpoint(2, 0)], &[outpoint(2, 1)], &[], &[hash(0x20)]);
        assert!(!b.has_dependency_on(&a));
        assert!(!a.has_dependency_on(&b));
    }

    #[test]
    fn spend_dependency_on_created() {
        let earlier = make_summary(1, &[], &[outpoint(1, 0)], &[], &[]);
        let later = make_summary(2, &[outpoint(1, 0)], &[], &[], &[]);
        assert!(later.has_dependency_on(&earlier));
    }

    #[test]
    fn double_spend_conflict() {
        let a = make_summary(1, &[outpoint(0, 0)], &[], &[], &[]);
        let b = make_summary(2, &[outpoint(0, 0)], &[], &[], &[]);
        assert!(b.has_dependency_on(&a));
    }

    #[test]
    fn duplicate_tx_conflict() {
        let shared_tx = hash(0xAA);
        let a = make_summary(1, &[], &[], &[], &[shared_tx]);
        let b = make_summary(2, &[], &[], &[], &[shared_tx]);
        assert!(b.has_dependency_on(&a));
    }

    #[test]
    fn read_dep_on_created() {
        let earlier = make_summary(1, &[], &[outpoint(1, 0)], &[], &[]);
        let later = make_summary(2, &[], &[], &[outpoint(1, 0)], &[]);
        assert!(later.has_dependency_on(&earlier));
    }
}
