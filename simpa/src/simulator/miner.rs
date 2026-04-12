use indexmap::IndexSet;
use itertools::Itertools;
use rand::rngs::ThreadRng;
use rand::Rng;
use rand_distr::{Distribution, Exp};
use spora_consensus::consensus::Consensus;
use spora_consensus::model::stores::virtual_state::VirtualState;
use spora_consensus::params::Params;
use spora_consensus_core::api::ConsensusApi;
use spora_consensus_core::block::{Block, TemplateBuildMode};
use spora_consensus_core::coinbase::MinerData;
use spora_consensus_core::sign::sign;
use spora_consensus_core::tx::{
    legacy_sequence_to_cell_since, CellEntry, CellOut, CellRef, CellTx, MutableTransaction, OutPoint, ScriptPublicKey, ScriptRef,
    ScriptVec, TransactionOutpoint,
};
use spora_core::trace;
use spora_hashes::Hash;
use spora_utils::sim::{Environment, Process, Resumption, Suspension};
use std::cmp::max;
use std::iter::once;
use std::sync::Arc;

pub struct Miner {
    // ID
    pub(super) id: u64,

    // Consensus
    pub(super) consensus: Arc<Consensus>,
    pub(super) params: Params,

    // Miner data
    miner_data: MinerData,
    _secret_key: secp256k1::SecretKey,

    // Cell data related to this miner
    possible_unspent_outpoints: IndexSet<TransactionOutpoint>,
    reserved_outpoints: IndexSet<TransactionOutpoint>,

    // Rand
    dist: Exp<f64>, // The time interval between Poisson(lambda) events distributes ~Exp(lambda)
    rng: ThreadRng,

    // Counters
    num_blocks: u64,
    sim_time: u64,

    // Config
    target_txs_per_block: u64,
    target_blocks: Option<u64>,
    max_cached_outpoints: usize,
    long_payload: bool,
}

impl Miner {
    pub fn new(
        id: u64,
        bps: f64,
        hashrate: f64,
        sk: secp256k1::SecretKey,
        pk: secp256k1::PublicKey,
        consensus: Arc<Consensus>,
        params: &Params,
        target_txs_per_block: u64,
        target_blocks: Option<u64>,
        long_payload: bool,
    ) -> Self {
        let (schnorr_public_key, _) = pk.x_only_public_key();
        let script_pub_key_script = once(0x20).chain(schnorr_public_key.serialize()).chain(once(0xac)).collect_vec(); // TODO: Use script builder when available to create p2pk properly
        let script_pub_key_script_vec = ScriptVec::from_slice(&script_pub_key_script);
        Self {
            id,
            consensus,
            params: params.clone(),
            miner_data: MinerData::new(ScriptPublicKey::new(0, ScriptVec::from_slice(&script_pub_key_script_vec)), Vec::new()),
            _secret_key: sk,
            possible_unspent_outpoints: IndexSet::new(),
            reserved_outpoints: IndexSet::new(),
            dist: Exp::new(bps * hashrate).unwrap(),
            rng: rand::thread_rng(),
            num_blocks: 0,
            sim_time: 0,
            target_txs_per_block,
            target_blocks,
            max_cached_outpoints: 10_000,
            long_payload,
        }
    }

    fn build_new_block(&mut self, timestamp: u64) -> Block {
        let nonce = self.id;
        let consensus = self.consensus.clone();
        let mut miner_data = self.miner_data.clone();
        miner_data.extra_data.extend_from_slice(&self.id.to_le_bytes());
        miner_data.extra_data.extend_from_slice(&self.num_blocks.to_le_bytes());
        miner_data.extra_data.extend_from_slice(&timestamp.to_le_bytes());
        let mut block_template = consensus
            .build_block_template_with_cell_tx_selector(miner_data, TemplateBuildMode::Standard, |virtual_state| {
                self.build_txs(virtual_state)
            })
            .expect("simulation txs are selected in sync with virtual state and are expected to be valid");
        block_template.block.header.timestamp = timestamp; // Use simulation time rather than real time
        block_template.block.header.nonce = nonce;
        block_template.block.header.finalize();
        block_template.block.to_immutable()
    }

    fn compute_lock_hash(script_public_key: &ScriptPublicKey) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"spora-cell/lock");
        hasher.update(&script_public_key.version().to_le_bytes());
        hasher.update(script_public_key.script());
        *hasher.finalize().as_bytes()
    }

    fn miner_lock_script_hash(&self) -> [u8; 32] {
        ScriptRef::new(Self::compute_lock_hash(&self.miner_data.script_public_key), 0, vec![]).hash()
    }

    fn outpoint_to_cell_tree_hash(outpoint: &TransactionOutpoint) -> Hash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"spora-cell/outpoint");
        hasher.update(&outpoint.tx_hash);
        hasher.update(&outpoint.index.to_le_bytes());
        Hash::from_bytes(*hasher.finalize().as_bytes())
    }

    fn transaction_outpoint_from_exec(outpoint: &OutPoint) -> TransactionOutpoint {
        *outpoint
    }

    fn consensus_cell_entry_from_state(entry: &spora_state::CellEntry) -> CellEntry {
        CellEntry::from_cell_metadata(
            entry.capacity,
            entry.data_bytes,
            entry.lock_hash.as_bytes(),
            entry.type_hash.map(|hash| hash.as_bytes()),
            entry.data_hash.as_bytes(),
            entry.block_daa_score,
            entry.is_cellbase,
        )
    }

    fn get_spendable_entry(
        &self,
        outpoint: TransactionOutpoint,
        virtual_daa_score: u64,
        maturity: u64,
        tree: &spora_state::CellStateTree,
    ) -> Option<CellEntry> {
        let entry = tree.get(&Self::outpoint_to_cell_tree_hash(&outpoint))?;
        if entry.capacity < 2 {
            return None;
        }
        if entry.is_cellbase && virtual_daa_score < entry.block_daa_score.saturating_add(maturity) {
            return None;
        }
        Some(Self::consensus_cell_entry_from_state(entry))
    }

    fn collect_possible_unspent_outpoints(&self, tree: &spora_state::CellStateTree) -> IndexSet<TransactionOutpoint> {
        let miner_lock_hash = self.miner_lock_script_hash();
        let mut outpoints = IndexSet::new();

        for (outpoint, _, entry) in tree.iter_by_outpoint() {
            if entry.lock_hash.as_bytes() != miner_lock_hash {
                continue;
            }
            let candidate = Self::transaction_outpoint_from_exec(outpoint);
            if self.reserved_outpoints.contains(&candidate) {
                continue;
            }

            outpoints.insert(candidate);
            if outpoints.len() >= self.max_cached_outpoints {
                break;
            }
        }

        outpoints
    }

    fn build_txs(&mut self, virtual_state: &VirtualState) -> Vec<CellTx> {
        let refreshed_outpoints = self.collect_possible_unspent_outpoints(&virtual_state.cell_state_tree);
        self.possible_unspent_outpoints = refreshed_outpoints;
        let multiple_outputs = !self.long_payload && self.possible_unspent_outpoints.len() < 5_000;
        let schnorr_key = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, &self._secret_key.secret_bytes()).unwrap();
        let maturity = self.params.coinbase_maturity();

        let mut stale_outpoints = Vec::new();
        let mut spent_outpoints = Vec::new();
        let mut txs = Vec::new();

        for outpoint in self.possible_unspent_outpoints.iter().copied() {
            let Some(entry) = self.get_spendable_entry(outpoint, virtual_state.daa_score, maturity, &virtual_state.cell_state_tree)
            else {
                let entry_exists = virtual_state.cell_state_tree.get(&Self::outpoint_to_cell_tree_hash(&outpoint)).is_some();
                if !entry_exists {
                    stale_outpoints.push(outpoint);
                }
                continue;
            };

            let mutable_tx =
                MutableTransaction::with_entries(self.create_unsigned_tx(outpoint, entry.amount(), multiple_outputs), vec![entry]);
            let signed_tx = sign(mutable_tx, schnorr_key);
            txs.push(signed_tx.tx);
            spent_outpoints.push(outpoint);
            self.reserved_outpoints.insert(outpoint);

            if txs.len() >= self.target_txs_per_block as usize {
                break;
            }
        }

        for outpoint in stale_outpoints.into_iter().chain(spent_outpoints) {
            self.possible_unspent_outpoints.swap_remove(&outpoint);
        }

        trace!(
            "Miner {} built {} txs from {} cached outpoints at virtual daa {}",
            self.id,
            txs.len(),
            self.possible_unspent_outpoints.len(),
            virtual_state.daa_score
        );

        txs
    }

    #[allow(dead_code)]
    fn create_unsigned_tx(&self, outpoint: TransactionOutpoint, input_amount: u64, multiple_outputs: bool) -> CellTx {
        let inputs = vec![CellRef::new(outpoint, legacy_sequence_to_cell_since(0))];
        let outputs = if multiple_outputs && input_amount > 4 {
            vec![
                CellOut {
                    lock: ScriptRef::new(Self::compute_lock_hash(&self.miner_data.script_public_key), 0, vec![]),
                    type_: None,
                    capacity: input_amount / 2,
                },
                CellOut {
                    lock: ScriptRef::new(Self::compute_lock_hash(&self.miner_data.script_public_key), 0, vec![]),
                    type_: None,
                    capacity: input_amount / 2 - 1,
                },
            ]
        } else {
            vec![CellOut {
                lock: ScriptRef::new(Self::compute_lock_hash(&self.miner_data.script_public_key), 0, vec![]),
                type_: None,
                capacity: input_amount - 1,
            }]
        };
        let mut outputs_data = vec![vec![]; outputs.len()];
        if self.long_payload {
            if let Some(first) = outputs_data.first_mut() {
                *first = vec![0; 90_000];
            }
        }

        CellTx::new(inputs, vec![], outputs, outputs_data, vec![vec![]])
            .expect("simulator-produced transactions must be Cell-constructible")
    }

    pub fn mine(&mut self, env: &mut Environment<Block>) -> Suspension {
        let block = self.build_new_block(env.now());
        match self.process_block(block.clone(), env) {
            Suspension::Idle => {
                env.broadcast(self.id, block);
                self.sample_mining_interval()
            }
            Suspension::Halt => Suspension::Halt,
            Suspension::Timeout(timeout) => Suspension::Timeout(timeout),
        }
    }

    fn sample_mining_interval(&mut self) -> Suspension {
        Suspension::Timeout(max((self.dist.sample(&mut self.rng) * 1000.0) as u64, 1))
    }

    fn process_block(&mut self, block: Block, env: &mut Environment<Block>) -> Suspension {
        if self.report_progress(env) {
            Suspension::Halt
        } else {
            let session = self.consensus.acquire_session();
            let inserted_block = block.clone();
            let status = futures::executor::block_on(self.consensus.validate_and_insert_block(block).virtual_state_task).unwrap();
            assert!(status.is_cell_valid_or_pending());
            drop(session);

            let miner_lock_hash = Self::compute_lock_hash(&self.miner_data.script_public_key);
            let mut added_outputs = 0usize;
            for tx in inserted_block.transactions.iter() {
                for input in &tx.inputs {
                    let spent = input.out_point;
                    self.possible_unspent_outpoints.swap_remove(&spent);
                    self.reserved_outpoints.swap_remove(&spent);
                }

                for (index, output) in tx.outputs.iter().enumerate() {
                    if output.lock.code_hash != miner_lock_hash {
                        continue;
                    }
                    if self.possible_unspent_outpoints.len() == self.max_cached_outpoints {
                        self.possible_unspent_outpoints.swap_remove_index(self.rng.gen_range(0..self.max_cached_outpoints));
                    }
                    self.possible_unspent_outpoints.insert(TransactionOutpoint::new(tx.id(), index as u32));
                    added_outputs += 1;
                }
            }
            trace!(
                "Miner {} cached {} new outputs and now tracks {} outpoints after block {}",
                self.id,
                added_outputs,
                self.possible_unspent_outpoints.len(),
                inserted_block.header.hash
            );

            Suspension::Idle
        }
    }

    fn report_progress(&mut self, env: &mut Environment<Block>) -> bool {
        self.num_blocks += 1;
        if let Some(target_blocks) = self.target_blocks {
            if self.num_blocks > target_blocks {
                return true; // Exit
            }
        }
        if self.id != 0 {
            return false;
        }
        if self.num_blocks % 50 == 0 || self.sim_time / 5000 != env.now() / 5000 {
            trace!("Simulation time: {}\tGenerated {} blocks", env.now() as f64 / 1000.0, self.num_blocks);
        }
        self.sim_time = env.now();
        false
    }
}

impl Process<Block> for Miner {
    fn resume(&mut self, resumption: Resumption<Block>, env: &mut Environment<Block>) -> Suspension {
        match resumption {
            Resumption::Initial => self.sample_mining_interval(),
            Resumption::Scheduled => self.mine(env),
            Resumption::Message(block) => self.process_block(block, env),
        }
    }
}
