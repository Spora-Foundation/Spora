use crate::{
    consensus::test_consensus::TestConsensus,
    errors::RuleError,
    model::{
        services::reachability::ReachabilityService,
        stores::{block_transactions::BlockTransactionsStoreReader, headers::HeaderStoreReader},
    },
};
use spora_consensus_core::{
    api::args::{TransactionValidationArgs, TransactionValidationBatchArgs},
    api::ConsensusApi,
    block::{Block, BlockTemplate, MutableBlock, TemplateBuildMode, TemplateTransactionSelector},
    blockhash,
    blockstatus::BlockStatus,
    coinbase::BlockRewardData,
    coinbase::MinerData,
    config::{params::MAINNET_PARAMS, ConfigBuilder},
    errors::tx::TxRuleError,
    merkle::calc_hash_merkle_root_cell,
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{
        cell_meta_from_legacy_output, legacy_compat_transaction_from_cell_tx, MutableTransaction, ScriptPublicKey, ScriptVec,
        Transaction, TransactionInput, TransactionOutpoint, TransactionOutput,
    },
    BlockHashMap, BlockHashSet,
};
use spora_core::assert_match;
use spora_exec::{CellDep, CellOut, CellRef, CellTx, DepType, OutPoint, ScriptRef};
use spora_hashes::Hash;
use std::{collections::VecDeque, thread::JoinHandle};

fn compute_lock_hash(script_public_key: &ScriptPublicKey) -> [u8; 32] {
    use blake3::Hasher;

    let mut hasher = Hasher::new();
    hasher.update(b"spora-cell/lock");
    hasher.update(&script_public_key.version().to_le_bytes());
    hasher.update(script_public_key.script());
    *hasher.finalize().as_bytes()
}

struct OnetimeTxSelector {
    txs: Option<Vec<CellTx>>,
}

impl OnetimeTxSelector {
    fn new(txs: Vec<CellTx>) -> Self {
        Self { txs: Some(txs) }
    }
}

impl TemplateTransactionSelector for OnetimeTxSelector {
    fn select_transactions(&mut self) -> Vec<CellTx> {
        self.txs.take().unwrap()
    }

    fn reject_selection(&mut self, _tx_id: spora_consensus_core::tx::TransactionId) {}

    fn is_successful(&self) -> bool {
        true
    }
}

struct TestContext {
    consensus: TestConsensus,
    join_handles: Vec<JoinHandle<()>>,
    miner_data: MinerData,
    simulated_time: u64,
    current_templates: VecDeque<BlockTemplate>,
    current_tips: BlockHashSet,
}

impl Drop for TestContext {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        let join_handles = std::mem::take(&mut self.join_handles);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.consensus.shutdown(join_handles)));
    }
}

impl TestContext {
    fn new(consensus: TestConsensus) -> Self {
        let join_handles = consensus.init();
        let genesis_hash = consensus.params().genesis.hash;
        let simulated_time = consensus.params().genesis.timestamp;
        Self {
            consensus,
            join_handles,
            miner_data: new_miner_data(),
            simulated_time,
            current_templates: Default::default(),
            current_tips: BlockHashSet::from_iter([genesis_hash]),
        }
    }

    pub fn build_block_template_row(&mut self, nonces: impl Iterator<Item = usize>) -> &mut Self {
        for nonce in nonces {
            self.simulated_time += self.consensus.params().target_time_per_block;
            self.current_templates.push_back(self.build_block_template(nonce as u64, self.simulated_time));
        }
        self
    }

    pub fn assert_row_parents(&mut self) -> &mut Self {
        for t in self.current_templates.iter() {
            assert_eq!(self.current_tips, BlockHashSet::from_iter(t.block.header.direct_parents().iter().copied()));
        }
        self
    }

    pub async fn validate_and_insert_row(&mut self) -> &mut Self {
        self.current_tips.clear();
        while let Some(t) = self.current_templates.pop_front() {
            self.current_tips.insert(t.block.header.hash);
            self.validate_and_insert_block(t.block.to_immutable()).await;
        }
        self
    }

    pub async fn build_and_insert_disqualified_chain(&mut self, mut parents: Vec<Hash>, len: usize) -> Hash {
        // The first block is explicitly invalid. Descendants inherit disqualification through their selected parent.
        for i in 0..len {
            self.simulated_time += self.consensus.params().target_time_per_block;
            let b = if i == 0 {
                self.build_block_with_parents(parents.clone(), 0, self.simulated_time)
            } else {
                self.build_block_with_disqualified_parent(parents.clone(), 0, self.simulated_time)
            };
            parents = vec![b.header.hash];
            self.validate_and_insert_block(b.to_immutable()).await;
        }
        parents[0]
    }

    pub fn build_block_template(&self, nonce: u64, timestamp: u64) -> BlockTemplate {
        let mut t = self
            .consensus
            .build_block_template(
                self.miner_data.clone(),
                Box::new(OnetimeTxSelector::new(Default::default())),
                TemplateBuildMode::Standard,
            )
            .unwrap();
        t.block.header.timestamp = timestamp;
        t.block.header.nonce = nonce;
        t.block.header.finalize();
        t
    }

    pub fn build_block_with_parents(&self, parents: Vec<Hash>, nonce: u64, timestamp: u64) -> MutableBlock {
        let mut b = self.consensus.build_block_with_parents_and_transactions(blockhash::NONE, parents, vec![]);
        b.header.cell_commitment = Hash::from_bytes([0x55; 32]);
        b.header.timestamp = timestamp;
        b.header.nonce = nonce;
        b.header.finalize(); // This overrides the NONE hash we passed earlier with the actual hash
        b
    }

    pub fn build_block_with_disqualified_parent(&self, parents: Vec<Hash>, nonce: u64, timestamp: u64) -> MutableBlock {
        let mut header = self.consensus.build_header_with_parents(blockhash::NONE, parents.clone());
        let ghostdag_data = self.consensus.services.ghostdag_manager.ghostdag(&parents);
        let mut mergeset_rewards = BlockHashMap::default();

        for block_hash in ghostdag_data.mergeset_blues.iter().chain(ghostdag_data.mergeset_reds.iter()).copied() {
            if mergeset_rewards.contains_key(&block_hash) {
                continue;
            }

            let txs = self.consensus.block_transactions_store.get(block_hash).unwrap();
            let block_daa_score = self.consensus.headers_store.get_daa_score(block_hash).unwrap();
            let reward_data = txs
                .first()
                .and_then(|tx| tx.payload())
                .and_then(|payload| self.consensus.services.coinbase_manager.deserialize_coinbase_payload(payload).ok())
                .map(|payload| BlockRewardData::new(payload.subsidy, 0, payload.miner_data.script_public_key.clone()))
                .unwrap_or_else(|| {
                    BlockRewardData::new(
                        self.consensus.services.coinbase_manager.calc_block_subsidy(block_daa_score),
                        0,
                        ScriptPublicKey::from_vec(0, vec![]),
                    )
                });
            mergeset_rewards.insert(block_hash, reward_data);
        }

        let coinbase = self
            .consensus
            .services
            .coinbase_manager
            .expected_coinbase_transaction(
                header.daa_score,
                self.miner_data.clone(),
                &ghostdag_data,
                &mergeset_rewards,
                &Default::default(),
            )
            .unwrap()
            .tx;

        let outputs = coinbase
            .outputs
            .iter()
            .map(|output| CellOut {
                lock: ScriptRef::new(compute_lock_hash(&output.script_public_key), 0, vec![]),
                type_: None,
                capacity: output.value,
            })
            .collect();
        let mut outputs_data = vec![vec![]; coinbase.outputs.len()];
        let witnesses = if let Some(first) = outputs_data.first_mut() {
            *first = coinbase.payload.clone();
            vec![]
        } else {
            vec![coinbase.payload.clone()]
        };
        let coinbase = CellTx::new(vec![], vec![], outputs, outputs_data, witnesses).unwrap();

        let transactions = vec![coinbase];
        header.hash_merkle_root = calc_hash_merkle_root_cell(transactions.iter(), false);
        header.timestamp = u64::max(header.timestamp, timestamp);
        header.nonce = nonce;
        header.finalize();

        MutableBlock::new(header, transactions)
    }

    pub async fn validate_and_insert_block(&mut self, block: Block) -> &mut Self {
        let status = self.consensus.validate_and_insert_block(block).virtual_state_task.await.unwrap();
        assert!(status.has_block_body());
        self
    }

    pub fn assert_tips(&mut self) -> &mut Self {
        assert_eq!(BlockHashSet::from_iter(self.consensus.get_tips().into_iter()), self.current_tips);
        self
    }

    pub fn assert_tips_num(&mut self, expected_num: usize) -> &mut Self {
        assert_eq!(BlockHashSet::from_iter(self.consensus.get_tips().into_iter()).len(), expected_num);
        self
    }

    pub fn assert_virtual_parents_subset(&mut self) -> &mut Self {
        assert!(self.consensus.get_virtual_parents().is_subset(&self.current_tips));
        self
    }

    pub fn assert_valid_cell_tip(&mut self) -> &mut Self {
        // Assert that at least one body tip was resolved with a valid cell state
        assert!(self.consensus.body_tips().iter().copied().any(|h| self.consensus.block_status(h) == BlockStatus::StatusCellValid));
        self
    }
}

#[tokio::test]
async fn template_mining_sanity_test() {
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    let rounds = 10;
    let width = 3;
    for _ in 0..rounds {
        ctx.build_block_template_row(0..width)
            .assert_row_parents()
            .validate_and_insert_row()
            .await
            .assert_tips()
            .assert_virtual_parents_subset()
            .assert_valid_cell_tip();
    }
}

#[tokio::test]
async fn antichain_merge_test() {
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
        })
        .build();

    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // Build a large 32-wide antichain
    ctx.build_block_template_row(0..32)
        .validate_and_insert_row()
        .await
        .assert_tips()
        .assert_virtual_parents_subset()
        .assert_valid_cell_tip();

    // Mine a long enough chain s.t. the antichain is fully merged
    for _ in 0..32 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_cell_tip();
    }
    ctx.assert_tips_num(1);
}

#[tokio::test]
async fn basic_cell_disqualified_test() {
    spora_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
        })
        .build();

    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // Mine a valid chain
    for _ in 0..10 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_cell_tip();
    }

    // Get current sink
    let sink = ctx.consensus.get_sink();

    // Mine a longer disqualified chain
    let disqualified_tip = ctx.build_and_insert_disqualified_chain(vec![config.genesis.hash], 20).await;

    assert_ne!(sink, disqualified_tip);
    assert_eq!(sink, ctx.consensus.get_sink());
    assert_eq!(BlockHashSet::from_iter([sink, disqualified_tip]), BlockHashSet::from_iter(ctx.consensus.get_tips().into_iter()));
    assert!(!ctx.consensus.get_virtual_parents().contains(&disqualified_tip));
}

#[tokio::test]
async fn double_search_disqualified_test() {
    // TODO: add non-coinbase transactions and concurrency in order to complicate the test

    spora_core::log::try_init_logger("info");
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.min_difficulty_window_size = p.sampled_difficulty_window_size;
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));

    // Mine 3 valid blocks over genesis
    ctx.build_block_template_row(0..3)
        .validate_and_insert_row()
        .await
        .assert_tips()
        .assert_virtual_parents_subset()
        .assert_valid_cell_tip();

    // Mark the one expected to remain on virtual chain
    let original_sink = ctx.consensus.get_sink();

    // Find the roots to be used for the disqualified chains
    let mut virtual_parents = ctx.consensus.get_virtual_parents();
    assert!(virtual_parents.remove(&original_sink));
    let mut iter = virtual_parents.into_iter();
    let root_1 = iter.next().unwrap();
    let root_2 = iter.next().unwrap();
    assert_eq!(iter.next(), None);

    // Mine a valid chain
    for _ in 0..10 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_cell_tip();
    }

    // Get current sink
    let sink = ctx.consensus.get_sink();

    assert!(ctx.consensus.reachability_service().is_chain_ancestor_of(original_sink, sink));

    // Mine a long disqualified chain
    let disqualified_tip_1 = ctx.build_and_insert_disqualified_chain(vec![root_1], 30).await;

    // And another shorter disqualified chain
    let disqualified_tip_2 = ctx.build_and_insert_disqualified_chain(vec![root_2], 20).await;

    assert_eq!(ctx.consensus.get_block_status(root_1), Some(BlockStatus::StatusCellValid));
    assert_eq!(ctx.consensus.get_block_status(root_2), Some(BlockStatus::StatusCellValid));

    assert_ne!(sink, disqualified_tip_1);
    assert_ne!(sink, disqualified_tip_2);
    assert_eq!(sink, ctx.consensus.get_sink());
    assert_eq!(
        BlockHashSet::from_iter([sink, disqualified_tip_1, disqualified_tip_2]),
        BlockHashSet::from_iter(ctx.consensus.get_tips().into_iter())
    );
    assert!(!ctx.consensus.get_virtual_parents().contains(&disqualified_tip_1));
    assert!(!ctx.consensus.get_virtual_parents().contains(&disqualified_tip_2));

    // Mine a long enough valid chain. Disqualified body tips should remain excluded from virtual parent selection.
    for _ in 0..30 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_cell_tip();
    }
    ctx.assert_tips_num(3);
    assert!(!ctx.consensus.get_virtual_parents().contains(&disqualified_tip_1));
    assert!(!ctx.consensus.get_virtual_parents().contains(&disqualified_tip_2));
}

fn new_miner_data() -> MinerData {
    let secp = secp256k1::Secp256k1::new();
    let mut rng = rand::thread_rng();
    let (_sk, pk) = secp.generate_keypair(&mut rng);
    let script = ScriptVec::from_slice(&pk.serialize());
    MinerData::new(ScriptPublicKey::new(0, script), vec![])
}

fn build_spend_tx(previous_outpoint: TransactionOutpoint, value: u64) -> Transaction {
    Transaction::new(
        0,
        vec![TransactionInput::new(previous_outpoint, vec![], u64::MAX, 0)],
        vec![TransactionOutput { value, script_public_key: ScriptPublicKey::from_vec(0, vec![]) }],
        0,
        SUBNETWORK_ID_NATIVE,
        0,
        vec![],
    )
}

fn build_cell_spend_tx(previous_outpoint: OutPoint, value: u64) -> CellTx {
    let lock = ScriptRef::new([0; 32], 0, vec![]);
    CellTx::new(
        vec![CellRef::new(previous_outpoint, 0)],
        vec![],
        vec![CellOut { lock, type_: None, capacity: value }],
        vec![vec![]],
        vec![],
    )
    .unwrap()
}

fn build_cell_spend_tx_with_dep(previous_outpoint: OutPoint, dep_outpoint: OutPoint, value: u64) -> CellTx {
    let lock = ScriptRef::new([0; 32], 0, vec![]);
    CellTx::new(
        vec![CellRef::new(previous_outpoint, 0)],
        vec![CellDep { out_point: dep_outpoint, dep_type: DepType::Code }],
        vec![CellOut { lock, type_: None, capacity: value }],
        vec![vec![]],
        vec![],
    )
    .unwrap()
}

fn build_cell_spend_tx_with_header_dep(previous_outpoint: OutPoint, header_dep: [u8; 32], value: u64) -> CellTx {
    let lock = ScriptRef::new([0; 32], 0, vec![]);
    CellTx::new_with_header_deps(
        vec![CellRef::new(previous_outpoint, 0)],
        vec![],
        vec![header_dep],
        vec![CellOut { lock, type_: None, capacity: value }],
        vec![vec![]],
        vec![],
    )
    .unwrap()
}

fn build_block_with_extra_transactions(consensus: &TestConsensus, hash: Hash, parents: Vec<Hash>, extra_txs: Vec<CellTx>) -> Block {
    let mut block = consensus.build_block_with_parents_and_transactions(hash, parents, vec![]);
    block.transactions.extend(extra_txs);
    block.header.hash_merkle_root = calc_hash_merkle_root_cell(block.transactions.iter(), false);
    block.to_immutable()
}

#[tokio::test]
async fn rejects_missing_outpoints_in_virtual_state() {
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();

    let parent = consensus
        .build_block_template(
            MinerData::new(ScriptPublicKey::from_vec(0, vec![]), vec![]),
            Box::new(OnetimeTxSelector::new(vec![])),
            TemplateBuildMode::Standard,
        )
        .unwrap();
    let parent_hash = parent.block.header.hash;
    consensus.validate_and_insert_block(parent.block.to_immutable()).virtual_state_task.await.unwrap();

    let invalid_tx = build_cell_spend_tx(OutPoint::new([0xAA; 32], 0), 1_000);
    let block = build_block_with_extra_transactions(&consensus, 2.into(), vec![parent_hash], vec![invalid_tx]);

    assert_match!(
        consensus.validate_and_insert_block(block).virtual_state_task.await,
        Err(RuleError::TxInContextFailed(_, TxRuleError::MissingTxOutpoints))
    );

    consensus.shutdown(wait_handles);
}

#[tokio::test]
async fn rejects_double_spend_in_same_block_with_cell_inputs() {
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|params| {
            params.coinbase_maturity = 0;
        })
        .build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();

    let reward_source = consensus
        .build_block_template(
            MinerData::new(ScriptPublicKey::from_vec(0, vec![]), vec![]),
            Box::new(OnetimeTxSelector::new(vec![])),
            TemplateBuildMode::Standard,
        )
        .unwrap();
    let reward_source_hash = reward_source.block.header.hash;
    consensus.validate_and_insert_block(reward_source.block.to_immutable()).virtual_state_task.await.unwrap();

    let parent = consensus
        .build_block_template(
            MinerData::new(ScriptPublicKey::from_vec(0, vec![]), vec![]),
            Box::new(OnetimeTxSelector::new(vec![])),
            TemplateBuildMode::Standard,
        )
        .unwrap();
    let parent_hash = parent.block.header.hash;
    let parent_coinbase_id = parent.block.transactions[0].id();
    assert_eq!(parent.block.header.direct_parents(), &[reward_source_hash]);
    assert!(!parent.block.transactions[0].outputs.is_empty(), "reward-paying coinbase should create at least one spendable cell");
    consensus.validate_and_insert_block(parent.block.to_immutable()).virtual_state_task.await.unwrap();

    let outpoint = OutPoint::new(parent_coinbase_id, 0);
    let tx1 = build_cell_spend_tx(outpoint.clone(), 1_000);
    let tx2 = build_cell_spend_tx(outpoint, 1_001);
    let block = build_block_with_extra_transactions(&consensus, 3.into(), vec![parent_hash], vec![tx1, tx2]);

    assert_match!(consensus.validate_and_insert_block(block).virtual_state_task.await, Err(RuleError::DoubleSpendInSameBlock(_)));

    consensus.shutdown(wait_handles);
}

#[cfg(not(feature = "vm"))]
#[tokio::test]
async fn rejects_mergeset_history_when_a_blue_block_dep_was_spent_on_selected_parent_chain() {
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|params| {
            params.coinbase_maturity = 0;
        })
        .build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();
    let miner_data = MinerData::new(ScriptPublicKey::from_vec(0, vec![]), vec![]);

    let warmup = consensus
        .build_block_template(miner_data.clone(), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard)
        .unwrap();
    consensus.validate_and_insert_block(warmup.block.to_immutable()).virtual_state_task.await.unwrap();

    let funding_a = consensus
        .build_block_template(miner_data.clone(), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard)
        .unwrap();
    let funding_a_coinbase = funding_a.block.transactions[0].clone();
    consensus.validate_and_insert_block(funding_a.block.to_immutable()).virtual_state_task.await.unwrap();

    let funding_b = consensus
        .build_block_template(miner_data.clone(), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard)
        .unwrap();
    let funding_b_coinbase = funding_b.block.transactions[0].clone();
    consensus.validate_and_insert_block(funding_b.block.to_immutable()).virtual_state_task.await.unwrap();

    let parent = consensus
        .build_block_template(miner_data.clone(), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard)
        .unwrap();
    let parent_hash = parent.block.header.hash;
    consensus.validate_and_insert_block(parent.block.to_immutable()).virtual_state_task.await.unwrap();

    assert!(!funding_a_coinbase.outputs.is_empty(), "second mined block should have a reward-paying coinbase output");
    assert!(!funding_b_coinbase.outputs.is_empty(), "third mined block should have a reward-paying coinbase output");

    let dep_outpoint = OutPoint::new(funding_a_coinbase.id(), 0);
    let spend_outpoint = OutPoint::new(funding_b_coinbase.id(), 0);
    let dep_capacity = funding_a_coinbase.outputs[0].capacity;
    let spend_capacity = funding_b_coinbase.outputs[0].capacity;

    let consume_dep_block = consensus
        .build_block_template(
            miner_data.clone(),
            Box::new(OnetimeTxSelector::new(vec![build_cell_spend_tx(dep_outpoint.clone(), dep_capacity)])),
            TemplateBuildMode::Standard,
        )
        .unwrap();
    let consume_dep_hash = consume_dep_block.block.header.hash;
    assert_eq!(consume_dep_block.block.header.direct_parents(), &[parent_hash]);

    let use_dep_block = consensus
        .build_block_template(
            miner_data.clone(),
            Box::new(OnetimeTxSelector::new(vec![build_cell_spend_tx_with_dep(spend_outpoint, dep_outpoint, spend_capacity)])),
            TemplateBuildMode::Standard,
        )
        .unwrap();
    let use_dep_hash = use_dep_block.block.header.hash;
    assert_eq!(use_dep_block.block.header.direct_parents(), &[parent_hash]);

    consensus.validate_and_insert_block(consume_dep_block.block.to_immutable()).virtual_state_task.await.unwrap();
    let selected_parent_tip = consensus
        .build_block_template(miner_data.clone(), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard)
        .unwrap();
    let selected_parent_tip_hash = selected_parent_tip.block.header.hash;
    assert_eq!(selected_parent_tip.block.header.direct_parents(), &[consume_dep_hash]);
    consensus.validate_and_insert_block(selected_parent_tip.block.to_immutable()).virtual_state_task.await.unwrap();

    consensus.validate_and_insert_block(use_dep_block.block.to_immutable()).virtual_state_task.await.unwrap();

    let merger =
        consensus.build_cell_valid_block_with_parents(11.into(), vec![selected_parent_tip_hash, use_dep_hash], miner_data, vec![]);

    assert_match!(
        consensus.validate_and_insert_block(merger.to_immutable()).virtual_state_task.await,
        Ok(BlockStatus::StatusDisqualifiedFromChain)
    );

    consensus.shutdown(wait_handles);
}

#[cfg(not(feature = "vm"))]
#[tokio::test]
async fn validates_mempool_transaction_against_virtual_state_and_sets_fee() {
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();

    let spend_tx = build_spend_tx(TransactionOutpoint { tx_hash: [0x44; 32], index: 0 }, 9_000);
    let mut mutable_tx = MutableTransaction::from_tx(spend_tx);
    mutable_tx.entries[0] = Some(cell_meta_from_legacy_output(10_000, &ScriptPublicKey::from_vec(0, vec![]), 0, false));

    consensus
        .validate_mempool_transaction(&mut mutable_tx, &TransactionValidationArgs::default())
        .expect("mempool validation should accept populated parent entries");

    assert_eq!(mutable_tx.calculated_fee, Some(1_000));

    consensus.shutdown(wait_handles);
}

#[cfg(not(feature = "vm"))]
#[tokio::test]
async fn rejects_missing_outpoints_in_mempool_validation() {
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();

    let missing_outpoint = TransactionOutpoint { tx_hash: [0xCC; 32], index: 0 };
    let mut mutable_tx = MutableTransaction::from_tx(build_spend_tx(missing_outpoint, 1_000));

    assert_eq!(
        consensus.validate_mempool_transaction(&mut mutable_tx, &TransactionValidationArgs::default()),
        Err(TxRuleError::MissingTxOutpoints)
    );

    let mut transactions = vec![
        MutableTransaction::from_tx(build_spend_tx(TransactionOutpoint { tx_hash: [0xDD; 32], index: 0 }, 1_000)),
        MutableTransaction::from_tx(build_spend_tx(TransactionOutpoint { tx_hash: [0xEE; 32], index: 0 }, 2_000)),
    ];
    let results = consensus.validate_mempool_transactions_in_parallel(&mut transactions, &TransactionValidationBatchArgs::default());
    assert_eq!(results, vec![Err(TxRuleError::MissingTxOutpoints), Err(TxRuleError::MissingTxOutpoints)]);

    consensus.shutdown(wait_handles);
}

#[cfg(not(feature = "vm"))]
#[tokio::test]
async fn rejects_relative_daa_sequence_lock_in_mempool_validation() {
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();
    let current_daa = consensus.virtual_processor().lkg_virtual_state.load_full().daa_score;

    let spend_tx = Transaction::new(
        0,
        vec![TransactionInput::new(TransactionOutpoint { tx_hash: [0x55; 32], index: 0 }, vec![], 1, 0)],
        vec![TransactionOutput { value: 9_000, script_public_key: ScriptPublicKey::from_vec(0, vec![]) }],
        0,
        SUBNETWORK_ID_NATIVE,
        0,
        vec![],
    );
    let mut mutable_tx = MutableTransaction::from_tx(spend_tx);
    mutable_tx.entries[0] = Some(cell_meta_from_legacy_output(10_000, &ScriptPublicKey::from_vec(0, vec![]), current_daa, false));

    assert_eq!(
        consensus.validate_mempool_transaction(&mut mutable_tx, &TransactionValidationArgs::default()),
        Err(TxRuleError::SequenceLockConditionsAreNotMet)
    );

    consensus.shutdown(wait_handles);
}

#[cfg(feature = "vm")]
#[tokio::test]
async fn rejects_legacy_mempool_validation_when_vm_enabled() {
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();

    let spend_tx = build_spend_tx(TransactionOutpoint { tx_hash: [0x44; 32], index: 0 }, 9_000);
    let mut mutable_tx = MutableTransaction::from_tx(spend_tx);
    mutable_tx.entries[0] = Some(cell_meta_from_legacy_output(10_000, &ScriptPublicKey::from_vec(0, vec![]), 0, false));

    assert_match!(
        consensus.validate_mempool_transaction(&mut mutable_tx, &TransactionValidationArgs::default()),
        Err(TxRuleError::CellValidationFailed(msg))
            if msg.contains("legacy mempool submission is disabled when the vm feature is enabled")
    );

    let mut batch = vec![mutable_tx];
    let results = consensus.validate_mempool_transactions_in_parallel(&mut batch, &TransactionValidationBatchArgs::default());
    assert_match!(
        results.as_slice(),
        [Err(TxRuleError::CellValidationFailed(msg))]
            if msg.contains("legacy mempool submission is disabled when the vm feature is enabled")
    );

    consensus.shutdown(wait_handles);
}

#[tokio::test]
async fn build_block_template_rejects_invalid_selected_transactions_in_standard_mode() {
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();
    let miner_data = MinerData::new(ScriptPublicKey::from_vec(0, vec![]), vec![]);
    let invalid_tx = build_cell_spend_tx(OutPoint::new([0xAB; 32], 0), 10_000);

    assert_match!(
        consensus.build_block_template(
            miner_data,
            Box::new(OnetimeTxSelector::new(vec![invalid_tx])),
            TemplateBuildMode::Standard,
        ),
        Err(RuleError::InvalidTransactionsInNewBlock(invalid_transactions))
            if invalid_transactions.values().all(|err| *err == TxRuleError::MissingTxOutpoints)
    );

    consensus.shutdown(wait_handles);
}

#[tokio::test]
async fn build_block_template_rejects_isolation_invalid_selected_transactions_in_standard_mode() {
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();
    let miner_data = MinerData::new(ScriptPublicKey::from_vec(0, vec![]), vec![]);
    let mut invalid_tx = build_cell_spend_tx(OutPoint::new([0xA1; 32], 0), 10_000);
    invalid_tx.ver = 0;

    assert_match!(
        consensus.build_block_template(
            miner_data,
            Box::new(OnetimeTxSelector::new(vec![invalid_tx])),
            TemplateBuildMode::Standard,
        ),
        Err(RuleError::InvalidTransactionsInNewBlock(invalid_transactions))
            if invalid_transactions.values().all(|err| matches!(
                err,
                TxRuleError::CellValidationFailed(msg) if msg.contains("Invalid version")
            ))
    );

    consensus.shutdown(wait_handles);
}

#[cfg(not(feature = "vm"))]
#[tokio::test]
async fn build_block_template_in_infallible_mode_filters_invalid_transactions_and_calculates_fees() {
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|params| {
            params.coinbase_maturity = 0;
        })
        .build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();
    let miner_data = MinerData::new(ScriptPublicKey::from_vec(0, vec![]), vec![]);

    let warmup = consensus
        .build_block_template(miner_data.clone(), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard)
        .unwrap();
    consensus.validate_and_insert_block(warmup.block.to_immutable()).virtual_state_task.await.unwrap();

    let funding = consensus
        .build_block_template(miner_data.clone(), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard)
        .unwrap();
    let funding_coinbase = funding.block.transactions[0].clone();
    consensus.validate_and_insert_block(funding.block.to_immutable()).virtual_state_task.await.unwrap();

    assert!(!funding_coinbase.outputs.is_empty(), "funding block should create a spendable coinbase output");
    let funding_capacity = funding_coinbase.outputs[0].capacity;
    let valid_tx = build_cell_spend_tx(OutPoint::new(funding_coinbase.id(), 0), funding_capacity - 1_000);
    let invalid_tx = build_cell_spend_tx(OutPoint::new([0xCD; 32], 0), 10_000);
    let valid_only_template = consensus
        .build_block_template(
            miner_data.clone(),
            Box::new(OnetimeTxSelector::new(vec![valid_tx.clone()])),
            TemplateBuildMode::Standard,
        )
        .unwrap();
    assert_eq!(valid_only_template.block.transactions.len(), 2);
    assert_eq!(valid_only_template.calculated_fees, vec![1_000]);

    let template = consensus
        .build_block_template(
            miner_data,
            Box::new(OnetimeTxSelector::new(vec![valid_tx.clone(), invalid_tx])),
            TemplateBuildMode::Infallible,
        )
        .unwrap();

    assert_eq!(template.block.transactions.len(), 2, "coinbase plus the single valid transaction should remain");
    assert_eq!(template.block.transactions[1].id(), valid_tx.id());
    assert_eq!(template.calculated_fees, vec![1_000]);

    consensus.shutdown(wait_handles);
}

#[cfg(not(feature = "vm"))]
#[tokio::test]
async fn build_block_template_rejects_conflicting_selected_transactions_in_standard_mode() {
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|params| {
            params.coinbase_maturity = 0;
        })
        .build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();
    let miner_data = MinerData::new(ScriptPublicKey::from_vec(0, vec![]), vec![]);

    let warmup = consensus
        .build_block_template(miner_data.clone(), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard)
        .unwrap();
    consensus.validate_and_insert_block(warmup.block.to_immutable()).virtual_state_task.await.unwrap();

    let funding = consensus
        .build_block_template(miner_data.clone(), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard)
        .unwrap();
    let funding_coinbase = funding.block.transactions[0].clone();
    consensus.validate_and_insert_block(funding.block.to_immutable()).virtual_state_task.await.unwrap();

    let funding_outpoint = OutPoint::new(funding_coinbase.id(), 0);
    let winner_tx = build_cell_spend_tx(funding_outpoint.clone(), funding_coinbase.outputs[0].capacity - 1_000);
    let loser_tx = build_cell_spend_tx(funding_outpoint, funding_coinbase.outputs[0].capacity - 2_000);

    assert_match!(
        consensus.build_block_template(
            miner_data,
            Box::new(OnetimeTxSelector::new(vec![winner_tx.clone(), loser_tx.clone()])),
            TemplateBuildMode::Standard,
        ),
        Err(RuleError::InvalidTransactionsInNewBlock(invalid_transactions))
            if invalid_transactions.len() == 1
                && invalid_transactions.contains_key(&Hash::from_bytes(loser_tx.id()))
                && matches!(
                    invalid_transactions.get(&Hash::from_bytes(loser_tx.id())),
                    Some(TxRuleError::CellValidationFailed(msg)) if msg.contains("conflicts with another selected transaction")
                )
    );

    consensus.shutdown(wait_handles);
}

#[cfg(not(feature = "vm"))]
#[tokio::test]
async fn build_block_template_in_infallible_mode_filters_conflicting_selected_transactions() {
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|params| {
            params.coinbase_maturity = 0;
        })
        .build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();
    let miner_data = MinerData::new(ScriptPublicKey::from_vec(0, vec![]), vec![]);

    let warmup = consensus
        .build_block_template(miner_data.clone(), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard)
        .unwrap();
    consensus.validate_and_insert_block(warmup.block.to_immutable()).virtual_state_task.await.unwrap();

    let funding = consensus
        .build_block_template(miner_data.clone(), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard)
        .unwrap();
    let funding_coinbase = funding.block.transactions[0].clone();
    consensus.validate_and_insert_block(funding.block.to_immutable()).virtual_state_task.await.unwrap();

    let funding_outpoint = OutPoint::new(funding_coinbase.id(), 0);
    let winner_tx = build_cell_spend_tx(funding_outpoint.clone(), funding_coinbase.outputs[0].capacity - 1_000);
    let loser_tx = build_cell_spend_tx(funding_outpoint, funding_coinbase.outputs[0].capacity - 2_000);

    let template = consensus
        .build_block_template(
            miner_data,
            Box::new(OnetimeTxSelector::new(vec![winner_tx.clone(), loser_tx])),
            TemplateBuildMode::Infallible,
        )
        .unwrap();

    assert_eq!(template.block.transactions.len(), 2);
    assert_eq!(template.block.transactions[1].id(), winner_tx.id());
    assert_eq!(template.calculated_fees, vec![1_000]);

    consensus.shutdown(wait_handles);
}

#[cfg(not(feature = "vm"))]
#[tokio::test]
async fn build_block_template_in_infallible_mode_filters_isolation_invalid_transactions() {
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|params| {
            params.coinbase_maturity = 0;
        })
        .build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();
    let miner_data = MinerData::new(ScriptPublicKey::from_vec(0, vec![]), vec![]);

    let warmup = consensus
        .build_block_template(miner_data.clone(), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard)
        .unwrap();
    consensus.validate_and_insert_block(warmup.block.to_immutable()).virtual_state_task.await.unwrap();

    let funding = consensus
        .build_block_template(miner_data.clone(), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard)
        .unwrap();
    let funding_coinbase = funding.block.transactions[0].clone();
    consensus.validate_and_insert_block(funding.block.to_immutable()).virtual_state_task.await.unwrap();

    let funding_capacity = funding_coinbase.outputs[0].capacity;
    let valid_tx = build_cell_spend_tx(OutPoint::new(funding_coinbase.id(), 0), funding_capacity - 1_000);
    let mut invalid_tx = build_cell_spend_tx(OutPoint::new([0xA2; 32], 0), 10_000);
    invalid_tx.ver = 0;

    let template = consensus
        .build_block_template(
            miner_data,
            Box::new(OnetimeTxSelector::new(vec![valid_tx.clone(), invalid_tx])),
            TemplateBuildMode::Infallible,
        )
        .unwrap();

    assert_eq!(template.block.transactions.len(), 2, "coinbase plus the single valid transaction should remain");
    assert_eq!(template.block.transactions[1].id(), valid_tx.id());
    assert_eq!(template.calculated_fees, vec![1_000]);

    consensus.shutdown(wait_handles);
}

#[cfg(feature = "vm")]
#[tokio::test]
async fn build_block_template_rejects_missing_header_deps_in_standard_mode() {
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|params| {
            params.coinbase_maturity = 0;
        })
        .build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();
    let miner_data = MinerData::new(ScriptPublicKey::from_vec(0, vec![]), vec![]);

    let warmup = consensus
        .build_block_template(miner_data.clone(), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard)
        .unwrap();
    consensus.validate_and_insert_block(warmup.block.to_immutable()).virtual_state_task.await.unwrap();

    let funding = consensus
        .build_block_template(miner_data.clone(), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard)
        .unwrap();
    let funding_coinbase = funding.block.transactions[0].clone();
    consensus.validate_and_insert_block(funding.block.to_immutable()).virtual_state_task.await.unwrap();

    let invalid_tx = build_cell_spend_tx_with_header_dep(
        OutPoint::new(funding_coinbase.id(), 0),
        [0xEF; 32],
        funding_coinbase.outputs[0].capacity - 1_000,
    );

    assert_match!(
        consensus.build_block_template(
            miner_data,
            Box::new(OnetimeTxSelector::new(vec![invalid_tx])),
            TemplateBuildMode::Standard,
        ),
        Err(RuleError::InvalidTransactionsInNewBlock(invalid_transactions))
            if invalid_transactions.values().all(|err| matches!(
                err,
                TxRuleError::CellValidationFailed(msg) if msg.contains("missing header dependency")
            ))
    );

    consensus.shutdown(wait_handles);
}

#[cfg(feature = "vm")]
#[tokio::test]
async fn build_block_template_in_infallible_mode_filters_missing_header_deps_when_vm_enabled() {
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|params| {
            params.coinbase_maturity = 0;
        })
        .build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();
    let miner_data = MinerData::new(ScriptPublicKey::from_vec(0, vec![]), vec![]);

    let warmup = consensus
        .build_block_template(miner_data.clone(), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard)
        .unwrap();
    consensus.validate_and_insert_block(warmup.block.to_immutable()).virtual_state_task.await.unwrap();

    let funding = consensus
        .build_block_template(miner_data.clone(), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard)
        .unwrap();
    let funding_coinbase = funding.block.transactions[0].clone();
    consensus.validate_and_insert_block(funding.block.to_immutable()).virtual_state_task.await.unwrap();

    let invalid_tx = build_cell_spend_tx_with_header_dep(
        OutPoint::new(funding_coinbase.id(), 0),
        [0xFE; 32],
        funding_coinbase.outputs[0].capacity - 1_000,
    );

    let template = consensus
        .build_block_template(miner_data, Box::new(OnetimeTxSelector::new(vec![invalid_tx])), TemplateBuildMode::Infallible)
        .unwrap();

    assert_eq!(template.block.transactions.len(), 1, "infallible mode should drop the VM-invalid transaction");
    assert!(template.calculated_fees.is_empty());

    consensus.shutdown(wait_handles);
}

#[cfg(feature = "vm")]
#[tokio::test]
async fn validates_direct_cell_mempool_transaction_when_vm_enabled() {
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|params| {
            params.coinbase_maturity = 0;
        })
        .build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();
    let miner_data = MinerData::new(ScriptPublicKey::from_vec(0, vec![]), vec![]);

    let warmup = consensus
        .build_block_template(miner_data.clone(), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard)
        .unwrap();
    consensus.validate_and_insert_block(warmup.block.to_immutable()).virtual_state_task.await.unwrap();

    let funding = consensus
        .build_block_template(miner_data.clone(), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard)
        .unwrap();
    let funding_header_hash = *funding.block.header.hash.as_bytes();
    let funding_coinbase = funding.block.transactions[0].clone();
    consensus.validate_and_insert_block(funding.block.to_immutable()).virtual_state_task.await.unwrap();

    let cell_tx = build_cell_spend_tx_with_header_dep(
        OutPoint::new(funding_coinbase.id(), 0),
        funding_header_hash,
        funding_coinbase.outputs[0].capacity - 1_000,
    );
    let mut mirror = MutableTransaction::from_tx(legacy_compat_transaction_from_cell_tx(&cell_tx));

    consensus
        .validate_mempool_cell_transaction(&mut mirror, &cell_tx, &TransactionValidationArgs::default())
        .expect("canonical CellTx mempool validation should succeed when deps/header_deps are complete");

    assert_eq!(mirror.calculated_fee, Some(1_000));
    assert!(mirror.resolved_cell_metadata[0].is_some(), "resolved input metadata should be backfilled for the legacy mirror");

    consensus.shutdown(wait_handles);
}
