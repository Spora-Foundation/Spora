// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Block-level access summary for MPE parallelization dependency analysis

use std::collections::{BTreeMap, BTreeSet};

use spora_consensus_core::tx::TransactionOutpoint;
use spora_exec::celltx::{
    CellScriptSchedulerWitness, CellScriptSchedulerWitnessError, CellTx, CELLSCRIPT_SCHEDULER_EFFECT_PURE,
    CELLSCRIPT_SCHEDULER_EFFECT_READ_ONLY, CELLSCRIPT_SCHEDULER_SOURCE_CELL_DEP, CELLSCRIPT_SCHEDULER_SOURCE_INPUT,
    CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
};
use spora_hashes::Hash;
use thiserror::Error;

#[derive(Debug, Error)]
pub(super) enum BlockAccessSummaryError {
    #[error("transaction {tx_id} has invalid CellScript scheduler witness: {source}")]
    CellScriptSchedulerWitness {
        tx_id: Hash,
        #[source]
        source: CellScriptSchedulerWitnessError,
    },
    #[error("transaction {tx_id} carries CellScript scheduler witness but no trusted access set was provided")]
    MissingTrustedCellScriptAccessSet { tx_id: Hash },
    #[error("transaction {tx_id} has a stale trusted CellScript scheduler access set but no scheduler witness")]
    StaleTrustedCellScriptAccessSet { tx_id: Hash },
}

/// Trusted full scheduler summaries keyed by CellTx id.
///
/// The intended producers are transaction-builder output or authenticated
/// compiled metadata. The transaction witness remains untrusted until it
/// matches the full scheduler summary exactly.
pub(super) type TrustedCellScriptSchedulerAccessSets = BTreeMap<Hash, CellScriptSchedulerWitness>;

/// 块级访问摘要，记录一个 blue block 的完整读写集。
/// 用于判断两个 blue blocks 之间是否存在数据依赖，
/// 从而决定它们能否在同一层并行分析。
#[derive(Debug)]
pub(super) struct BlockAccessSummary {
    /// 该块消费的 outpoints（inputs）
    pub spent_outpoints: BTreeSet<TransactionOutpoint>,
    /// 该块创建的 outpoints（outputs）
    pub created_outpoints: BTreeSet<TransactionOutpoint>,
    /// 该块读取依赖的 outpoints（cell_deps，非消费性引用）
    pub read_deps: BTreeSet<TransactionOutpoint>,
    /// 该块包含的交易 ID
    pub tx_ids: BTreeSet<Hash>,
    /// CellScript scheduler-visible shared read domains.
    pub cellscript_shared_reads: BTreeSet<Hash>,
    /// CellScript scheduler-visible shared write domains.
    pub cellscript_shared_writes: BTreeSet<Hash>,
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
    pub fn from_block_txs(_block_hash: Hash, block_txs: &[CellTx]) -> Self {
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

        Self {
            spent_outpoints,
            created_outpoints,
            read_deps,
            tx_ids,
            cellscript_shared_reads: BTreeSet::new(),
            cellscript_shared_writes: BTreeSet::new(),
        }
    }

    /// Extract a block access summary and merge admitted CellScript scheduler witnesses.
    ///
    /// The structural input/output/cell_dep extraction remains the conservative
    /// baseline. Scheduler witnesses add shared-state contention domains only
    /// after CellTx-level envelope/source/index admission succeeds.
    pub fn try_from_block_txs_with_cellscript_scheduler(
        block_hash: Hash,
        block_txs: &[CellTx],
    ) -> Result<Self, BlockAccessSummaryError> {
        Self::try_from_block_txs_with_cellscript_scheduler_policy(block_hash, block_txs, None)
    }

    /// Extract a block access summary with strict trusted-summary matching.
    ///
    /// This is the runtime bridge for transaction-builder or compiled-metadata
    /// summaries: every transaction that carries a CellScript scheduler witness
    /// must have a trusted expected scheduler summary, and the decoded witness
    /// must match it exactly before the witness contributes to MPE scheduling.
    pub fn try_from_block_txs_with_trusted_cellscript_scheduler_accesses(
        block_hash: Hash,
        block_txs: &[CellTx],
        trusted_access_sets: &TrustedCellScriptSchedulerAccessSets,
    ) -> Result<Self, BlockAccessSummaryError> {
        Self::try_from_block_txs_with_cellscript_scheduler_policy(block_hash, block_txs, Some(trusted_access_sets))
    }

    fn try_from_block_txs_with_cellscript_scheduler_policy(
        block_hash: Hash,
        block_txs: &[CellTx],
        trusted_access_sets: Option<&TrustedCellScriptSchedulerAccessSets>,
    ) -> Result<Self, BlockAccessSummaryError> {
        let mut summary = Self::from_block_txs(block_hash, block_txs);
        for tx in block_txs {
            let tx_id = Hash::from_bytes(tx.id());
            let scheduler_witness_count = tx.cellscript_scheduler_witnesses().count();
            if scheduler_witness_count > 1 {
                return Err(BlockAccessSummaryError::CellScriptSchedulerWitness {
                    tx_id,
                    source: CellScriptSchedulerWitnessError::DuplicateSchedulerWitness { count: scheduler_witness_count },
                });
            }
            if let Some(trusted_access_sets) = trusted_access_sets {
                if trusted_access_sets.contains_key(&tx_id) && scheduler_witness_count == 0 {
                    return Err(BlockAccessSummaryError::StaleTrustedCellScriptAccessSet { tx_id });
                }
            }
            for witness in tx.admitted_cellscript_scheduler_witnesses() {
                let witness = witness.map_err(|source| BlockAccessSummaryError::CellScriptSchedulerWitness { tx_id, source })?;
                if let Some(trusted_access_sets) = trusted_access_sets {
                    let expected_summary =
                        trusted_access_sets.get(&tx_id).ok_or(BlockAccessSummaryError::MissingTrustedCellScriptAccessSet { tx_id })?;
                    witness
                        .validate_summary(expected_summary)
                        .map_err(|source| BlockAccessSummaryError::CellScriptSchedulerWitness { tx_id, source })?;
                }
                summary.merge_cellscript_scheduler_witness(tx, &witness);
            }
        }
        Ok(summary)
    }

    fn merge_cellscript_scheduler_witness(&mut self, tx: &CellTx, witness: &CellScriptSchedulerWitness) {
        let shared_target = if matches!(witness.effect_class, CELLSCRIPT_SCHEDULER_EFFECT_PURE | CELLSCRIPT_SCHEDULER_EFFECT_READ_ONLY)
        {
            &mut self.cellscript_shared_reads
        } else {
            &mut self.cellscript_shared_writes
        };
        for touch in &witness.touches_shared {
            shared_target.insert(Hash::from_bytes(*touch));
        }

        let tx_id = tx.id();
        for access in &witness.accesses {
            match access.source {
                CELLSCRIPT_SCHEDULER_SOURCE_INPUT => {
                    if let Some(input) = tx.inputs.get(access.index as usize) {
                        self.spent_outpoints.insert(TransactionOutpoint {
                            tx_hash: input.previous_output.tx_hash,
                            index: input.previous_output.index,
                        });
                    }
                }
                CELLSCRIPT_SCHEDULER_SOURCE_CELL_DEP => {
                    if let Some(dep) = tx.cell_deps.get(access.index as usize) {
                        self.read_deps.insert(TransactionOutpoint { tx_hash: dep.out_point.tx_hash, index: dep.out_point.index });
                    }
                }
                CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT => {
                    if tx.outputs.get(access.index as usize).is_some() {
                        self.created_outpoints.insert(TransactionOutpoint { tx_hash: tx_id, index: access.index });
                    }
                }
                _ => {}
            }
        }
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
    /// 4. self.read_deps ∩ earlier.created_outpoints ≠ ∅（读 earlier 创建的 cell）
    /// 5. self.read_deps ∩ earlier.spent_outpoints ≠ ∅（读 earlier 已消费的 cell）
    /// 6. CellScript shared write/read domains conflict.
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

        // 5. 读-消费依赖：self 的 cell_deps 引用了 earlier 消费的 cell。
        //
        // 串行执行中 earlier 会先消费该 cell，self 随后的 cell_dep 校验必须
        // 失败或跳过；因此两者不能在同一个冻结 snapshot 下并行分析。
        if self.read_deps.intersection(&earlier.spent_outpoints).next().is_some() {
            return true;
        }

        // 6. CellScript shared-state contention. read/read is parallelizable;
        // any write/read or write/write overlap must be serialized.
        if self.cellscript_shared_writes.intersection(&earlier.cellscript_shared_writes).next().is_some() {
            return true;
        }
        if self.cellscript_shared_writes.intersection(&earlier.cellscript_shared_reads).next().is_some() {
            return true;
        }
        if self.cellscript_shared_reads.intersection(&earlier.cellscript_shared_writes).next().is_some() {
            return true;
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spora_consensus_core::tx::TransactionOutpoint;
    use spora_exec::celltx::{
        encode_cellscript_scheduler_witness_molecule, CellScriptSchedulerAccessWitness, CellScriptSchedulerWitness,
        CELLSCRIPT_SCHEDULER_EFFECT_CREATING, CELLSCRIPT_SCHEDULER_EFFECT_MUTATING, CELLSCRIPT_SCHEDULER_EFFECT_READ_ONLY,
        CELLSCRIPT_SCHEDULER_OP_CREATE, CELLSCRIPT_SCHEDULER_OP_READ_REF, CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
        CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
    };
    use spora_exec::{CellDep, CellInput, CellOutput, CellTx, DepType, OutPoint, Script};
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
            cellscript_shared_reads: BTreeSet::new(),
            cellscript_shared_writes: BTreeSet::new(),
        }
    }

    fn make_summary_with_shared(_block: u8, shared_reads: &[Hash], shared_writes: &[Hash]) -> BlockAccessSummary {
        BlockAccessSummary {
            spent_outpoints: BTreeSet::new(),
            created_outpoints: BTreeSet::new(),
            read_deps: BTreeSet::new(),
            tx_ids: BTreeSet::new(),
            cellscript_shared_reads: shared_reads.iter().copied().collect(),
            cellscript_shared_writes: shared_writes.iter().copied().collect(),
        }
    }

    fn test_output() -> CellOutput {
        CellOutput { lock: Script::new([0x00; 32], 0, vec![]), type_: None, capacity: 1000 }
    }

    fn test_tx(inputs: Vec<CellInput>, cell_deps: Vec<CellDep>, output_count: usize, witnesses: Vec<Vec<u8>>) -> CellTx {
        CellTx::new(inputs, cell_deps, vec![test_output(); output_count], vec![vec![]; output_count], witnesses).unwrap()
    }

    fn scheduler_witness_bytes(
        effect_class: u8,
        touches_shared: Vec<[u8; 32]>,
        accesses: Vec<CellScriptSchedulerAccessWitness>,
    ) -> Vec<u8> {
        encode_cellscript_scheduler_witness_molecule(&scheduler_witness(effect_class, touches_shared, accesses))
    }

    fn scheduler_witness(
        effect_class: u8,
        touches_shared: Vec<[u8; 32]>,
        accesses: Vec<CellScriptSchedulerAccessWitness>,
    ) -> CellScriptSchedulerWitness {
        CellScriptSchedulerWitness {
            magic: 0xCE11,
            version: CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
            effect_class,
            parallelizable: false,
            touches_shared_count: touches_shared.len() as u32,
            touches_shared,
            estimated_cycles: 64,
            access_count: accesses.len() as u32,
            accesses,
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

    #[test]
    fn read_dep_on_spent_cell_serializes() {
        let earlier = make_summary(1, &[outpoint(1, 0)], &[], &[], &[]);
        let later = make_summary(2, &[], &[], &[outpoint(1, 0)], &[]);
        assert!(later.has_dependency_on(&earlier));
    }

    #[test]
    fn cellscript_shared_write_conflict_serializes_blocks() {
        let shared = hash(0x42);
        let earlier = make_summary_with_shared(1, &[], &[shared]);
        let later = make_summary_with_shared(2, &[], &[shared]);
        assert!(later.has_dependency_on(&earlier));
    }

    #[test]
    fn cellscript_shared_read_after_write_serializes_blocks() {
        let shared = hash(0x42);
        let earlier = make_summary_with_shared(1, &[], &[shared]);
        let later = make_summary_with_shared(2, &[shared], &[]);
        assert!(later.has_dependency_on(&earlier));
    }

    #[test]
    fn cellscript_shared_read_read_does_not_serialize_blocks() {
        let shared = hash(0x42);
        let earlier = make_summary_with_shared(1, &[shared], &[]);
        let later = make_summary_with_shared(2, &[shared], &[]);
        assert!(!later.has_dependency_on(&earlier));
    }

    #[test]
    fn block_summary_consumes_admitted_cellscript_scheduler_witness() {
        let mut tx = test_tx(vec![], vec![], 1, vec![]);
        let witness = scheduler_witness_bytes(
            CELLSCRIPT_SCHEDULER_EFFECT_MUTATING,
            vec![[0x42; 32]],
            vec![CellScriptSchedulerAccessWitness {
                operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
                source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
                index: 0,
                binding_hash: [0x24; 32],
            }],
        );
        tx.push_cellscript_scheduler_witness(witness).unwrap();

        let summary = BlockAccessSummary::try_from_block_txs_with_cellscript_scheduler(hash(0x01), &[tx.clone()]).unwrap();
        assert!(summary.cellscript_shared_writes.contains(&Hash::from_bytes([0x42; 32])));
        assert!(summary.created_outpoints.contains(&TransactionOutpoint { tx_hash: tx.id(), index: 0 }));
    }

    #[test]
    fn block_summary_rejects_malformed_cellscript_scheduler_witness() {
        let tx = test_tx(vec![], vec![], 1, vec![vec![0x11, 0xCE, 0x01]]);

        let error = BlockAccessSummary::try_from_block_txs_with_cellscript_scheduler(hash(0x01), &[tx]).unwrap_err();
        assert!(error.to_string().contains("invalid CellScript scheduler witness"));
    }

    #[test]
    fn block_summary_rejects_duplicate_cellscript_scheduler_witnesses() {
        let witness = scheduler_witness_bytes(
            CELLSCRIPT_SCHEDULER_EFFECT_CREATING,
            vec![],
            vec![CellScriptSchedulerAccessWitness {
                operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
                source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
                index: 0,
                binding_hash: [0x24; 32],
            }],
        );
        let tx = test_tx(vec![], vec![], 1, vec![witness.clone(), witness]);

        let error = BlockAccessSummary::try_from_block_txs_with_cellscript_scheduler(hash(0x01), &[tx]).unwrap_err();

        assert!(error.to_string().contains("duplicate CellScript scheduler witnesses"));
    }

    #[test]
    fn block_summary_rejects_scheduler_witness_with_illegal_operation_source() {
        let mut tx = test_tx(vec![], vec![], 1, vec![]);
        let witness = scheduler_witness_bytes(
            CELLSCRIPT_SCHEDULER_EFFECT_READ_ONLY,
            vec![],
            vec![CellScriptSchedulerAccessWitness {
                operation: CELLSCRIPT_SCHEDULER_OP_READ_REF,
                source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
                index: 0,
                binding_hash: [0x24; 32],
            }],
        );
        tx.push_cellscript_scheduler_witness(witness).unwrap();

        let error = BlockAccessSummary::try_from_block_txs_with_cellscript_scheduler(hash(0x01), &[tx]).unwrap_err();
        assert!(error.to_string().contains("cannot target source"));
    }

    #[test]
    fn block_summary_rejects_scheduler_witness_with_out_of_bounds_source_index() {
        let mut tx = test_tx(vec![], vec![], 0, vec![]);
        let witness = scheduler_witness_bytes(
            CELLSCRIPT_SCHEDULER_EFFECT_CREATING,
            vec![],
            vec![CellScriptSchedulerAccessWitness {
                operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
                source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
                index: 0,
                binding_hash: [0x24; 32],
            }],
        );
        tx.push_cellscript_scheduler_witness(witness).unwrap();

        let error = BlockAccessSummary::try_from_block_txs_with_cellscript_scheduler(hash(0x01), &[tx]).unwrap_err();
        assert!(error.to_string().contains("out of bounds"));
    }

    #[test]
    fn missing_scheduler_witness_cannot_hide_structural_double_spend() {
        let shared_input = OutPoint::new([0x33; 32], 0);
        let first_tx = test_tx(vec![CellInput::new(shared_input, 0)], vec![], 0, vec![]);
        let second_tx = test_tx(vec![CellInput::new(shared_input, 0)], vec![], 0, vec![]);

        let first = BlockAccessSummary::try_from_block_txs_with_cellscript_scheduler(hash(0x01), &[first_tx]).unwrap();
        let second = BlockAccessSummary::try_from_block_txs_with_cellscript_scheduler(hash(0x02), &[second_tx]).unwrap();

        assert!(second.has_dependency_on(&first));
    }

    #[test]
    fn underreported_scheduler_witness_cannot_hide_structural_read_dependency() {
        let creating_tx = test_tx(vec![], vec![], 1, vec![]);
        let created = OutPoint::new(creating_tx.id(), 0);
        let forged_read_only = scheduler_witness_bytes(CELLSCRIPT_SCHEDULER_EFFECT_READ_ONLY, vec![], vec![]);
        let reader_tx = test_tx(vec![], vec![CellDep { out_point: created, dep_type: DepType::Code }], 0, vec![forged_read_only]);

        let first = BlockAccessSummary::try_from_block_txs_with_cellscript_scheduler(hash(0x01), &[creating_tx]).unwrap();
        let second = BlockAccessSummary::try_from_block_txs_with_cellscript_scheduler(hash(0x02), &[reader_tx]).unwrap();

        assert!(second.has_dependency_on(&first));
    }

    #[test]
    fn forged_extra_shared_write_touch_only_adds_serialization() {
        let first_witness = scheduler_witness_bytes(CELLSCRIPT_SCHEDULER_EFFECT_MUTATING, vec![[0x42; 32]], vec![]);
        let second_witness = scheduler_witness_bytes(CELLSCRIPT_SCHEDULER_EFFECT_MUTATING, vec![[0x42; 32]], vec![]);
        let first_tx = test_tx(vec![], vec![], 0, vec![first_witness]);
        let second_tx = test_tx(vec![], vec![], 0, vec![second_witness]);

        let first = BlockAccessSummary::try_from_block_txs_with_cellscript_scheduler(hash(0x01), &[first_tx]).unwrap();
        let second = BlockAccessSummary::try_from_block_txs_with_cellscript_scheduler(hash(0x02), &[second_tx]).unwrap();

        assert!(second.has_dependency_on(&first));
        assert!(first.spent_outpoints.is_empty());
        assert!(first.created_outpoints.is_empty());
        assert!(first.read_deps.is_empty());
    }

    #[test]
    fn admitted_read_only_shared_touches_do_not_serialize_each_other() {
        let first_witness = scheduler_witness_bytes(CELLSCRIPT_SCHEDULER_EFFECT_READ_ONLY, vec![[0x42; 32]], vec![]);
        let second_witness = scheduler_witness_bytes(CELLSCRIPT_SCHEDULER_EFFECT_READ_ONLY, vec![[0x42; 32]], vec![]);
        let first_tx = test_tx(vec![CellInput::new(OutPoint::new([0x41; 32], 0), 0)], vec![], 0, vec![first_witness]);
        let second_tx = test_tx(vec![CellInput::new(OutPoint::new([0x43; 32], 0), 0)], vec![], 0, vec![second_witness]);

        let first = BlockAccessSummary::try_from_block_txs_with_cellscript_scheduler(hash(0x01), &[first_tx]).unwrap();
        let second = BlockAccessSummary::try_from_block_txs_with_cellscript_scheduler(hash(0x02), &[second_tx]).unwrap();

        assert!(!second.has_dependency_on(&first));
        assert!(first.cellscript_shared_reads.contains(&Hash::from_bytes([0x42; 32])));
        assert!(second.cellscript_shared_reads.contains(&Hash::from_bytes([0x42; 32])));
    }

    #[test]
    fn trusted_access_set_path_accepts_matching_compiled_summary() {
        let expected_access = CellScriptSchedulerAccessWitness {
            operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
            source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
            index: 0,
            binding_hash: [0x24; 32],
        };
        let trusted_summary = scheduler_witness(CELLSCRIPT_SCHEDULER_EFFECT_CREATING, vec![[0x42; 32]], vec![expected_access.clone()]);
        let witness = encode_cellscript_scheduler_witness_molecule(&trusted_summary);
        let tx = test_tx(vec![], vec![], 1, vec![witness]);
        let mut trusted = TrustedCellScriptSchedulerAccessSets::new();
        trusted.insert(Hash::from_bytes(tx.id()), trusted_summary);

        let summary =
            BlockAccessSummary::try_from_block_txs_with_trusted_cellscript_scheduler_accesses(hash(0x01), &[tx.clone()], &trusted)
                .unwrap();

        assert!(summary.cellscript_shared_writes.contains(&Hash::from_bytes([0x42; 32])));
        assert!(summary.created_outpoints.contains(&TransactionOutpoint { tx_hash: tx.id(), index: 0 }));
    }

    #[test]
    fn trusted_access_set_path_rejects_missing_compiled_summary() {
        let witness = scheduler_witness_bytes(CELLSCRIPT_SCHEDULER_EFFECT_CREATING, vec![], vec![]);
        let tx = test_tx(vec![], vec![], 0, vec![witness]);
        let trusted = TrustedCellScriptSchedulerAccessSets::new();

        let error = BlockAccessSummary::try_from_block_txs_with_trusted_cellscript_scheduler_accesses(hash(0x01), &[tx], &trusted)
            .unwrap_err();

        assert!(error.to_string().contains("no trusted access set"));
    }

    #[test]
    fn trusted_access_set_path_rejects_mismatched_compiled_summary() {
        let actual_access = CellScriptSchedulerAccessWitness {
            operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
            source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
            index: 0,
            binding_hash: [0x24; 32],
        };
        let expected_access = CellScriptSchedulerAccessWitness {
            operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
            source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
            index: 0,
            binding_hash: [0x25; 32],
        };
        let witness = scheduler_witness_bytes(CELLSCRIPT_SCHEDULER_EFFECT_CREATING, vec![], vec![actual_access]);
        let tx = test_tx(vec![], vec![], 1, vec![witness]);
        let mut trusted = TrustedCellScriptSchedulerAccessSets::new();
        trusted
            .insert(Hash::from_bytes(tx.id()), scheduler_witness(CELLSCRIPT_SCHEDULER_EFFECT_CREATING, vec![], vec![expected_access]));

        let error = BlockAccessSummary::try_from_block_txs_with_trusted_cellscript_scheduler_accesses(hash(0x01), &[tx], &trusted)
            .unwrap_err();

        assert!(error.to_string().contains("access set mismatch"));
    }

    #[test]
    fn trusted_access_set_path_rejects_shared_touch_tampering() {
        let access = CellScriptSchedulerAccessWitness {
            operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
            source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
            index: 0,
            binding_hash: [0x24; 32],
        };
        let trusted_summary = scheduler_witness(CELLSCRIPT_SCHEDULER_EFFECT_CREATING, vec![[0x42; 32]], vec![access.clone()]);
        let tampered_witness = scheduler_witness_bytes(CELLSCRIPT_SCHEDULER_EFFECT_CREATING, vec![], vec![access]);
        let tx = test_tx(vec![], vec![], 1, vec![tampered_witness]);
        let mut trusted = TrustedCellScriptSchedulerAccessSets::new();
        trusted.insert(Hash::from_bytes(tx.id()), trusted_summary);

        let error = BlockAccessSummary::try_from_block_txs_with_trusted_cellscript_scheduler_accesses(hash(0x01), &[tx], &trusted)
            .unwrap_err();

        assert!(error.to_string().contains("trusted summary mismatch"));
        assert!(error.to_string().contains("touches_shared"));
    }

    #[test]
    fn trusted_access_set_path_rejects_extra_untrusted_scheduler_witness() {
        let expected_access = CellScriptSchedulerAccessWitness {
            operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
            source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
            index: 0,
            binding_hash: [0x24; 32],
        };
        let unexpected_access = CellScriptSchedulerAccessWitness {
            operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
            source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
            index: 0,
            binding_hash: [0x25; 32],
        };
        let trusted_summary = scheduler_witness(CELLSCRIPT_SCHEDULER_EFFECT_CREATING, vec![], vec![expected_access.clone()]);
        let expected_witness = encode_cellscript_scheduler_witness_molecule(&trusted_summary);
        let unexpected_witness = scheduler_witness_bytes(CELLSCRIPT_SCHEDULER_EFFECT_CREATING, vec![], vec![unexpected_access]);
        let tx = test_tx(vec![], vec![], 1, vec![expected_witness, unexpected_witness]);
        let mut trusted = TrustedCellScriptSchedulerAccessSets::new();
        trusted.insert(Hash::from_bytes(tx.id()), trusted_summary);

        let error = BlockAccessSummary::try_from_block_txs_with_trusted_cellscript_scheduler_accesses(hash(0x01), &[tx], &trusted)
            .unwrap_err();

        assert!(error.to_string().contains("duplicate CellScript scheduler witnesses"));
    }

    #[test]
    fn trusted_access_set_path_rejects_duplicate_matching_scheduler_witnesses() {
        let expected_access = CellScriptSchedulerAccessWitness {
            operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
            source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
            index: 0,
            binding_hash: [0x24; 32],
        };
        let trusted_summary = scheduler_witness(CELLSCRIPT_SCHEDULER_EFFECT_CREATING, vec![], vec![expected_access]);
        let expected_witness = encode_cellscript_scheduler_witness_molecule(&trusted_summary);
        let tx = test_tx(vec![], vec![], 1, vec![expected_witness.clone(), expected_witness]);
        let mut trusted = TrustedCellScriptSchedulerAccessSets::new();
        trusted.insert(Hash::from_bytes(tx.id()), trusted_summary);

        let error = BlockAccessSummary::try_from_block_txs_with_trusted_cellscript_scheduler_accesses(hash(0x01), &[tx], &trusted)
            .unwrap_err();

        assert!(error.to_string().contains("duplicate CellScript scheduler witnesses"));
    }

    #[test]
    fn trusted_access_set_path_allows_transactions_without_cellscript_witnesses() {
        let tx = test_tx(vec![CellInput::new(OutPoint::new([0x51; 32], 0), 0)], vec![], 0, vec![]);
        let trusted = TrustedCellScriptSchedulerAccessSets::new();

        let summary =
            BlockAccessSummary::try_from_block_txs_with_trusted_cellscript_scheduler_accesses(hash(0x01), &[tx], &trusted).unwrap();

        assert_eq!(summary.spent_outpoints, BTreeSet::from([outpoint(0x51, 0)]));
        assert!(summary.cellscript_shared_reads.is_empty());
        assert!(summary.cellscript_shared_writes.is_empty());
    }

    #[test]
    fn trusted_access_set_path_rejects_stale_summary_for_plain_transaction() {
        let tx = test_tx(vec![CellInput::new(OutPoint::new([0x52; 32], 0), 0)], vec![], 1, vec![]);
        let mut trusted = TrustedCellScriptSchedulerAccessSets::new();
        trusted.insert(Hash::from_bytes(tx.id()), scheduler_witness(CELLSCRIPT_SCHEDULER_EFFECT_CREATING, vec![], vec![]));

        let error = BlockAccessSummary::try_from_block_txs_with_trusted_cellscript_scheduler_accesses(hash(0x01), &[tx], &trusted)
            .unwrap_err();

        assert!(error.to_string().contains("stale trusted CellScript scheduler access set"));
    }
}
