pub use super::{
    bps::{Bps, OneBps, TenBps},
    constants::consensus::*,
    genesis::{GenesisBlock, DEVNET_GENESIS, GENESIS, SIMNET_GENESIS, TESTNET11_GENESIS, TESTNET_GENESIS},
};
use crate::{
    constants::STORAGE_MASS_PARAMETER,
    network::{NetworkId, NetworkType},
    BlockLevel, KType,
};
use spora_addresses::Prefix;
use spora_math::Uint256;
use std::cmp::min;

/// Consensus parameters. Contains settings and configurations which are consensus-sensitive.
/// Changing one of these on a network node would exclude and prevent it from reaching consensus
/// with the other unmodified nodes.
#[derive(Clone, Debug)]
pub struct Params {
    pub dns_seeders: &'static [&'static str],
    pub net: NetworkId,
    pub genesis: GenesisBlock,
    pub ghostdag_k: KType,

    /// Timestamp deviation tolerance (in seconds)
    pub timestamp_deviation_tolerance: u64,

    /// Target time per block (in milliseconds)
    pub target_time_per_block: u64,

    /// Defines the highest allowed proof of work difficulty value for a block as a [`Uint256`]
    pub max_difficulty_target: Uint256,

    /// Highest allowed proof of work difficulty as a floating number
    pub max_difficulty_target_f64: f64,

    /// Size of the sampled difficulty window
    pub sampled_difficulty_window_size: usize,

    /// The minimum size a difficulty window must have to trigger a DAA calculation
    pub min_difficulty_window_size: usize,

    /// The difficulty sample rate
    pub difficulty_sample_rate: u64,

    /// Size of the sampled past median time window
    pub past_median_time_sampled_window_size: usize,

    /// The past median time sample rate
    pub past_median_time_sample_rate: u64,

    pub max_block_parents: u8,
    pub mergeset_size_limit: u64,
    pub merge_depth: u64,
    pub finality_depth: u64,
    pub pruning_depth: u64,

    pub coinbase_payload_script_public_key_max_len: u8,
    pub max_coinbase_payload_len: usize,

    pub max_tx_inputs: usize,
    pub max_tx_outputs: usize,
    pub max_witness_script_len: usize,
    pub max_script_public_key_len: usize,

    pub mass_per_tx_byte: u64,
    pub mass_per_script_pub_key_byte: u64,
    pub mass_per_sig_op: u64,
    pub max_block_mass: u64,

    /// The parameter for scaling inverse SPORA value to mass units (KIP-0009)
    pub storage_mass_parameter: u64,

    /// DAA score after which the pre-deflationary period switches to the deflationary period
    pub deflationary_phase_daa_score: u64,

    pub pre_deflationary_phase_base_subsidy: u64,
    pub coinbase_maturity: u64,
    pub skip_proof_of_work: bool,
    pub max_block_level: BlockLevel,
    pub pruning_proof_m: u64,
}

impl Params {
    /// Returns the size of the blocks window that is inspected to calculate the past median time.
    #[inline]
    #[must_use]
    pub fn past_median_time_window_size(&self) -> usize {
        self.past_median_time_sampled_window_size
    }

    /// Returns the past median time sample rate
    #[inline]
    #[must_use]
    pub fn past_median_time_sample_rate(&self) -> u64 {
        self.past_median_time_sample_rate
    }

    /// Returns the size of the blocks window that is inspected to calculate the difficulty
    #[inline]
    #[must_use]
    pub fn difficulty_window_size(&self) -> usize {
        self.sampled_difficulty_window_size
    }

    /// Returns the difficulty sample rate
    #[inline]
    #[must_use]
    pub fn difficulty_sample_rate(&self) -> u64 {
        self.difficulty_sample_rate
    }

    /// Returns the target time per block
    #[inline]
    #[must_use]
    pub fn target_time_per_block(&self) -> u64 {
        self.target_time_per_block
    }

    /// Returns the expected number of blocks per second
    #[inline]
    #[must_use]
    pub fn bps(&self) -> u64 {
        1000 / self.target_time_per_block
    }

    pub fn ghostdag_k(&self) -> KType {
        self.ghostdag_k
    }

    pub fn max_block_parents(&self) -> u8 {
        self.max_block_parents
    }

    pub fn mergeset_size_limit(&self) -> u64 {
        self.mergeset_size_limit
    }

    pub fn merge_depth(&self) -> u64 {
        self.merge_depth
    }

    pub fn finality_depth(&self) -> u64 {
        self.finality_depth
    }

    pub fn pruning_depth(&self) -> u64 {
        self.pruning_depth
    }

    pub fn coinbase_maturity(&self) -> u64 {
        self.coinbase_maturity
    }

    pub fn finality_duration_in_milliseconds(&self) -> u64 {
        self.target_time_per_block * self.finality_depth
    }

    pub fn difficulty_window_duration_in_block_units(&self) -> u64 {
        self.difficulty_sample_rate * self.sampled_difficulty_window_size as u64
    }

    pub fn expected_difficulty_window_duration_in_milliseconds(&self) -> u64 {
        self.target_time_per_block * self.difficulty_sample_rate * self.sampled_difficulty_window_size as u64
    }

    /// Returns the depth at which the anticone of a chain block is final (i.e., is a permanently closed set).
    /// Based on the analysis at <https://github.com/sporanet/docs/blob/main/Reference/prunality/Prunality.pdf>
    /// and on the decomposition of merge depth (rule R-I therein) from finality depth (φ)
    pub fn anticone_finalization_depth(&self) -> u64 {
        let anticone_finalization_depth = self.finality_depth
            + self.merge_depth
            + 4 * self.mergeset_size_limit * self.ghostdag_k as u64
            + 2 * self.ghostdag_k as u64
            + 2;

        // In mainnet it's guaranteed that `self.pruning_depth` is greater
        // than `anticone_finalization_depth`, but for some tests we use
        // a smaller (unsafe) pruning depth, so we return the minimum of
        // the two to avoid a situation where a block can be pruned and
        // not finalized.
        min(self.pruning_depth, anticone_finalization_depth)
    }

    pub fn max_tx_inputs(&self) -> usize {
        self.max_tx_inputs
    }

    pub fn max_tx_outputs(&self) -> usize {
        self.max_tx_outputs
    }

    pub fn max_witness_script_len(&self) -> usize {
        self.max_witness_script_len
    }

    pub fn max_script_public_key_len(&self) -> usize {
        self.max_script_public_key_len
    }

    pub fn network_name(&self) -> String {
        self.net.to_prefixed()
    }

    pub fn prefix(&self) -> Prefix {
        self.net.into()
    }

    pub fn default_p2p_port(&self) -> u16 {
        self.net.default_p2p_port()
    }

    pub fn default_rpc_port(&self) -> u16 {
        self.net.default_rpc_port()
    }
}

impl From<NetworkType> for Params {
    fn from(value: NetworkType) -> Self {
        match value {
            NetworkType::Mainnet => MAINNET_PARAMS,
            NetworkType::Testnet => TESTNET_PARAMS,
            NetworkType::Devnet => DEVNET_PARAMS,
            NetworkType::Simnet => SIMNET_PARAMS,
        }
    }
}

impl From<NetworkId> for Params {
    fn from(value: NetworkId) -> Self {
        match value.network_type {
            NetworkType::Mainnet => MAINNET_PARAMS,
            NetworkType::Testnet => match value.suffix {
                Some(10) => TESTNET_PARAMS,
                Some(x) => panic!("Testnet suffix {} is not supported", x),
                None => panic!("Testnet suffix not provided"),
            },
            NetworkType::Devnet => DEVNET_PARAMS,
            NetworkType::Simnet => SIMNET_PARAMS,
        }
    }
}

pub const MAINNET_PARAMS: Params = Params {
    dns_seeders: &[
        // This DNS seeder is run by Denis Mashkevich
        "mainnet-dnsseed-1.sporanet.org",
        // This DNS seeder is run by Denis Mashkevich
        "mainnet-dnsseed-2.sporanet.org",
        // This DNS seeder is run by Georges Künzli
        "seeder1.sporad.net",
        // This DNS seeder is run by Georges Künzli
        "seeder2.sporad.net",
        // This DNS seeder is run by Georges Künzli
        "seeder3.sporad.net",
        // This DNS seeder is run by Georges Künzli
        "seeder4.sporad.net",
        // This DNS seeder is run by Tim
        "sporadns.sporacalc.net",
        // This DNS seeder is run by supertypo
        "n-mainnet.spora.ws",
        // This DNS seeder is run by -gerri-
        "dnsseeder-spora-mainnet.x-con.at",
        // This DNS seeder is run by H@H
        "ns-mainnet.spora-dnsseeder.net",
    ],
    net: NetworkId::new(NetworkType::Mainnet),
    genesis: GENESIS,
    ghostdag_k: OneBps::ghostdag_k(),
    timestamp_deviation_tolerance: TIMESTAMP_DEVIATION_TOLERANCE,
    target_time_per_block: OneBps::target_time_per_block(),
    max_difficulty_target: MAX_DIFFICULTY_TARGET,
    max_difficulty_target_f64: MAX_DIFFICULTY_TARGET_AS_F64,
    sampled_difficulty_window_size: DIFFICULTY_SAMPLED_WINDOW_SIZE as usize,
    min_difficulty_window_size: MIN_DIFFICULTY_WINDOW_SIZE,
    difficulty_sample_rate: OneBps::difficulty_adjustment_sample_rate(),
    past_median_time_sampled_window_size: MEDIAN_TIME_SAMPLED_WINDOW_SIZE as usize,
    past_median_time_sample_rate: OneBps::past_median_time_sample_rate(),
    max_block_parents: OneBps::max_block_parents(),
    mergeset_size_limit: OneBps::mergeset_size_limit(),
    merge_depth: OneBps::merge_depth_bound(),
    finality_depth: OneBps::finality_depth(),
    pruning_depth: OneBps::pruning_depth(),
    coinbase_payload_script_public_key_max_len: 150,
    max_coinbase_payload_len: 216,

    max_tx_inputs: 1000,
    max_tx_outputs: 1000,
    max_witness_script_len: 10_000,
    max_script_public_key_len: 10_000,

    mass_per_tx_byte: 1,
    mass_per_script_pub_key_byte: 10,
    mass_per_sig_op: 1000,
    max_block_mass: 500_000,

    storage_mass_parameter: STORAGE_MASS_PARAMETER,

    deflationary_phase_daa_score: OneBps::deflationary_phase_daa_score(),
    pre_deflationary_phase_base_subsidy: 50000000000,
    coinbase_maturity: OneBps::coinbase_maturity(),
    skip_proof_of_work: false,
    max_block_level: 225,
    pruning_proof_m: 1000,
};

pub const TESTNET_PARAMS: Params = Params {
    dns_seeders: &["discover.spora.org", "nodes.discover.spora.org"],
    net: NetworkId::with_suffix(NetworkType::Testnet, 10),
    genesis: TESTNET_GENESIS,
    ghostdag_k: OneBps::ghostdag_k(),
    timestamp_deviation_tolerance: TIMESTAMP_DEVIATION_TOLERANCE,
    target_time_per_block: OneBps::target_time_per_block(),
    max_difficulty_target: MAX_DIFFICULTY_TARGET,
    max_difficulty_target_f64: MAX_DIFFICULTY_TARGET_AS_F64,
    sampled_difficulty_window_size: DIFFICULTY_SAMPLED_WINDOW_SIZE as usize,
    min_difficulty_window_size: MIN_DIFFICULTY_WINDOW_SIZE,
    difficulty_sample_rate: OneBps::difficulty_adjustment_sample_rate(),
    past_median_time_sampled_window_size: MEDIAN_TIME_SAMPLED_WINDOW_SIZE as usize,
    past_median_time_sample_rate: OneBps::past_median_time_sample_rate(),
    max_block_parents: OneBps::max_block_parents(),
    mergeset_size_limit: OneBps::mergeset_size_limit(),
    merge_depth: OneBps::merge_depth_bound(),
    finality_depth: OneBps::finality_depth(),
    pruning_depth: OneBps::pruning_depth(),
    coinbase_payload_script_public_key_max_len: 150,
    max_coinbase_payload_len: 216,

    max_tx_inputs: 1000,
    max_tx_outputs: 1000,
    max_witness_script_len: 10_000,
    max_script_public_key_len: 10_000,

    mass_per_tx_byte: 1,
    mass_per_script_pub_key_byte: 10,
    mass_per_sig_op: 1000,
    max_block_mass: 500_000,

    storage_mass_parameter: STORAGE_MASS_PARAMETER,
    deflationary_phase_daa_score: OneBps::deflationary_phase_daa_score(),
    pre_deflationary_phase_base_subsidy: OneBps::pre_deflationary_phase_base_subsidy(),
    coinbase_maturity: OneBps::coinbase_maturity(),
    skip_proof_of_work: false,
    max_block_level: 250,
    pruning_proof_m: 1000,
};

pub const SIMNET_PARAMS: Params = Params {
    dns_seeders: &[],
    net: NetworkId::new(NetworkType::Simnet),
    genesis: SIMNET_GENESIS,
    timestamp_deviation_tolerance: TIMESTAMP_DEVIATION_TOLERANCE,
    max_difficulty_target: MAX_DIFFICULTY_TARGET,
    max_difficulty_target_f64: MAX_DIFFICULTY_TARGET_AS_F64,
    min_difficulty_window_size: MIN_DIFFICULTY_WINDOW_SIZE,

    //
    // ~~~~~~~~~~~~~~~~~~ BPS dependent constants ~~~~~~~~~~~~~~~~~~
    //
    // Note we use a 1 BPS configuration for simnet
    ghostdag_k: OneBps::ghostdag_k(),
    target_time_per_block: OneBps::target_time_per_block(),
    sampled_difficulty_window_size: DIFFICULTY_SAMPLED_WINDOW_SIZE as usize,
    difficulty_sample_rate: OneBps::difficulty_adjustment_sample_rate(),
    past_median_time_sampled_window_size: MEDIAN_TIME_SAMPLED_WINDOW_SIZE as usize,
    past_median_time_sample_rate: OneBps::past_median_time_sample_rate(),
    // For simnet, we deviate from TN11 configuration and allow at least 64 parents in order to support mempool benchmarks out of the box
    max_block_parents: if OneBps::max_block_parents() > 64 { OneBps::max_block_parents() } else { 64 },
    mergeset_size_limit: OneBps::mergeset_size_limit(),
    merge_depth: OneBps::merge_depth_bound(),
    finality_depth: OneBps::finality_depth(),
    pruning_depth: OneBps::pruning_depth(),
    deflationary_phase_daa_score: OneBps::deflationary_phase_daa_score(),
    pre_deflationary_phase_base_subsidy: OneBps::pre_deflationary_phase_base_subsidy(),
    coinbase_maturity: OneBps::coinbase_maturity(),

    coinbase_payload_script_public_key_max_len: 150,
    max_coinbase_payload_len: 216,

    max_tx_inputs: 10_000,
    max_tx_outputs: 10_000,
    max_witness_script_len: 1_000_000,
    max_script_public_key_len: 1_000_000,

    mass_per_tx_byte: 1,
    mass_per_script_pub_key_byte: 10,
    mass_per_sig_op: 1000,
    max_block_mass: 500_000,

    storage_mass_parameter: STORAGE_MASS_PARAMETER,

    skip_proof_of_work: true, // For simnet only, PoW can be simulated by default
    max_block_level: 250,
    pruning_proof_m: PRUNING_PROOF_M,
};

pub const DEVNET_PARAMS: Params = Params {
    dns_seeders: &[],
    net: NetworkId::new(NetworkType::Devnet),
    genesis: DEVNET_GENESIS,
    ghostdag_k: OneBps::ghostdag_k(),
    timestamp_deviation_tolerance: TIMESTAMP_DEVIATION_TOLERANCE,
    target_time_per_block: OneBps::target_time_per_block(),
    max_difficulty_target: MAX_DIFFICULTY_TARGET,
    max_difficulty_target_f64: MAX_DIFFICULTY_TARGET_AS_F64,
    sampled_difficulty_window_size: DIFFICULTY_SAMPLED_WINDOW_SIZE as usize,
    min_difficulty_window_size: MIN_DIFFICULTY_WINDOW_SIZE,
    difficulty_sample_rate: OneBps::difficulty_adjustment_sample_rate(),
    past_median_time_sampled_window_size: MEDIAN_TIME_SAMPLED_WINDOW_SIZE as usize,
    past_median_time_sample_rate: OneBps::past_median_time_sample_rate(),
    max_block_parents: OneBps::max_block_parents(),
    mergeset_size_limit: OneBps::mergeset_size_limit(),
    merge_depth: OneBps::merge_depth_bound(),
    finality_depth: OneBps::finality_depth(),
    pruning_depth: OneBps::pruning_depth(),
    coinbase_payload_script_public_key_max_len: 150,
    max_coinbase_payload_len: 216,

    max_tx_inputs: 1000,
    max_tx_outputs: 1000,
    max_witness_script_len: 10_000,
    max_script_public_key_len: 10_000,

    mass_per_tx_byte: 1,
    mass_per_script_pub_key_byte: 10,
    mass_per_sig_op: 1000,
    max_block_mass: 500_000,

    storage_mass_parameter: STORAGE_MASS_PARAMETER,

    deflationary_phase_daa_score: OneBps::deflationary_phase_daa_score(),
    pre_deflationary_phase_base_subsidy: OneBps::pre_deflationary_phase_base_subsidy(),
    coinbase_maturity: OneBps::coinbase_maturity(),
    skip_proof_of_work: false,
    max_block_level: 250,
    pruning_proof_m: 1000,
};
