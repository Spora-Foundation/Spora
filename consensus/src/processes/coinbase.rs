use spora_consensus_core::{
    coinbase::*,
    errors::coinbase::{CoinbaseError, CoinbaseResult},
    tx::{cell_out_from_legacy_script_public_key, CellTx, ScriptPublicKey, ScriptVec, TransactionOutput},
    BlockHashMap, BlockHashSet,
};
use std::convert::TryInto;

use crate::model::stores::ghostdag::GhostdagData;

const LENGTH_OF_BLUE_SCORE: usize = size_of::<u64>();
const LENGTH_OF_SUBSIDY: usize = size_of::<u64>();
const LENGTH_OF_SCRIPT_PUB_KEY_VERSION: usize = size_of::<u16>();
const LENGTH_OF_SCRIPT_PUB_KEY_LENGTH: usize = size_of::<u8>();

const MIN_PAYLOAD_LENGTH: usize =
    LENGTH_OF_BLUE_SCORE + LENGTH_OF_SUBSIDY + LENGTH_OF_SCRIPT_PUB_KEY_VERSION + LENGTH_OF_SCRIPT_PUB_KEY_LENGTH;

// We define a year as 365.25 days and a month as 365.25 / 12 = 30.4375
// SECONDS_PER_MONTH = 30.4375 * 24 * 60 * 60
const SECONDS_PER_MONTH: u64 = 2629800;

pub const SUBSIDY_BY_MONTH_TABLE_SIZE: usize = 426;
pub type SubsidyByMonthTable = [u64; SUBSIDY_BY_MONTH_TABLE_SIZE];

#[derive(Clone)]
pub struct CoinbaseManager {
    coinbase_payload_script_public_key_max_len: u8,
    max_coinbase_payload_len: usize,
    deflationary_phase_daa_score: u64,
    pre_deflationary_phase_base_subsidy: u64,
    bps: u64,

    /// Precomputed subsidy by month table
    subsidy_by_month_table: SubsidyByMonthTable,
}

/// Struct used to streamline payload parsing
struct PayloadParser<'a> {
    remaining: &'a [u8], // The unparsed remainder
}

impl<'a> PayloadParser<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { remaining: data }
    }

    /// Returns a slice with the first `n` bytes of `remaining`, while setting `remaining` to the remaining part
    fn take(&mut self, n: usize) -> &[u8] {
        let (segment, remaining) = self.remaining.split_at(n);
        self.remaining = remaining;
        segment
    }
}

impl CoinbaseManager {
    fn build_coinbase_cell_tx(&self, outputs: Vec<TransactionOutput>, payload: Vec<u8>) -> CellTx {
        let outputs = outputs
            .into_iter()
            .map(|output| cell_out_from_legacy_script_public_key(output.value, &output.script_public_key))
            .collect::<Vec<_>>();

        let mut outputs_data = vec![Vec::new(); outputs.len()];
        let mut witnesses = Vec::new();
        if outputs_data.is_empty() {
            if !payload.is_empty() {
                witnesses.push(payload);
            }
        } else {
            outputs_data[0] = payload;
        }

        CellTx::new(vec![], vec![], outputs, outputs_data, witnesses).expect("coinbase template must produce a valid CellTx")
    }

    pub fn new(
        coinbase_payload_script_public_key_max_len: u8,
        max_coinbase_payload_len: usize,
        deflationary_phase_daa_score: u64,
        pre_deflationary_phase_base_subsidy: u64,
        bps: u64,
    ) -> Self {
        let subsidy_by_month_table: SubsidyByMonthTable = core::array::from_fn(|i| SUBSIDY_BY_MONTH_TABLE[i].div_ceil(bps));
        Self {
            coinbase_payload_script_public_key_max_len,
            max_coinbase_payload_len,
            deflationary_phase_daa_score,
            pre_deflationary_phase_base_subsidy,
            bps,
            subsidy_by_month_table,
        }
    }

    #[cfg(test)]
    #[inline]
    pub fn bps(&self) -> u64 {
        self.bps
    }

    pub fn expected_coinbase_transaction<T: AsRef<[u8]>>(
        &self,
        daa_score: u64,
        miner_data: MinerData<T>,
        ghostdag_data: &GhostdagData,
        mergeset_rewards: &BlockHashMap<BlockRewardData>,
        mergeset_non_daa: &BlockHashSet,
    ) -> CoinbaseResult<CoinbaseTransactionTemplate> {
        let mut outputs = Vec::with_capacity(ghostdag_data.mergeset_blues.len() + 1); // + 1 for possible red reward

        // Add an output for each mergeset blue block (∩ DAA window), paying to the script reported by the block.
        // Note that combinatorically it is nearly impossible for a blue block to be non-DAA
        for blue in ghostdag_data.mergeset_blues.iter().filter(|h| !mergeset_non_daa.contains(h)) {
            let reward_data = mergeset_rewards.get(blue).unwrap();
            if reward_data.subsidy + reward_data.total_fees > 0 {
                outputs
                    .push(TransactionOutput::new(reward_data.subsidy + reward_data.total_fees, reward_data.script_public_key.clone()));
            }
        }

        // Collect all rewards from mergeset reds ∩ DAA window and create a
        // single output rewarding all to the current block (the "merging" block)
        let mut red_reward = 0u64;

        // bps - always active
        for red in ghostdag_data.mergeset_reds.iter() {
            let reward_data = mergeset_rewards.get(red).unwrap();
            if mergeset_non_daa.contains(red) {
                red_reward += reward_data.total_fees;
            } else {
                red_reward += reward_data.subsidy + reward_data.total_fees;
            }
        }

        if red_reward > 0 {
            outputs.push(TransactionOutput::new(red_reward, miner_data.script_public_key.clone()));
        }

        // Build the current block's payload
        let subsidy = self.calc_block_subsidy(daa_score);
        let payload = self.serialize_coinbase_payload(&CoinbaseData { blue_score: ghostdag_data.blue_score, subsidy, miner_data })?;

        Ok(CoinbaseTransactionTemplate { tx: self.build_coinbase_cell_tx(outputs, payload), has_red_reward: red_reward > 0 })
    }

    pub fn serialize_coinbase_payload<T: AsRef<[u8]>>(&self, data: &CoinbaseData<T>) -> CoinbaseResult<Vec<u8>> {
        let script_pub_key_len = data.miner_data.script_public_key.script().len();
        if script_pub_key_len > self.coinbase_payload_script_public_key_max_len as usize {
            return Err(CoinbaseError::PayloadScriptPublicKeyLenAboveMax(
                script_pub_key_len,
                self.coinbase_payload_script_public_key_max_len,
            ));
        }
        let payload: Vec<u8> = data.blue_score.to_le_bytes().iter().copied()                    // Blue score                   (u64)
            .chain(data.subsidy.to_le_bytes().iter().copied())                                  // Subsidy                      (u64)
            .chain(data.miner_data.script_public_key.version().to_le_bytes().iter().copied())   // Script public key version    (u16)
            .chain((script_pub_key_len as u8).to_le_bytes().iter().copied())                    // Script public key length     (u8)
            .chain(data.miner_data.script_public_key.script().iter().copied())                  // Script public key
            .chain(data.miner_data.extra_data.as_ref().iter().copied())                         // Extra data
            .collect();

        Ok(payload)
    }

    pub fn modify_coinbase_payload<T: AsRef<[u8]>>(&self, mut payload: Vec<u8>, miner_data: &MinerData<T>) -> CoinbaseResult<Vec<u8>> {
        let script_pub_key_len = miner_data.script_public_key.script().len();
        if script_pub_key_len > self.coinbase_payload_script_public_key_max_len as usize {
            return Err(CoinbaseError::PayloadScriptPublicKeyLenAboveMax(
                script_pub_key_len,
                self.coinbase_payload_script_public_key_max_len,
            ));
        }

        // Keep only blue score and subsidy. Note that truncate does not modify capacity, so
        // the usual case where the payloads are the same size will not trigger a reallocation
        payload.truncate(LENGTH_OF_BLUE_SCORE + LENGTH_OF_SUBSIDY);
        payload.extend(
            miner_data.script_public_key.version().to_le_bytes().iter().copied() // Script public key version (u16)
                .chain((script_pub_key_len as u8).to_le_bytes().iter().copied()) // Script public key length  (u8)
                .chain(miner_data.script_public_key.script().iter().copied())    // Script public key
                .chain(miner_data.extra_data.as_ref().iter().copied()), // Extra data
        );

        Ok(payload)
    }

    pub fn deserialize_coinbase_payload<'a>(&self, payload: &'a [u8]) -> CoinbaseResult<CoinbaseData<&'a [u8]>> {
        if payload.len() < MIN_PAYLOAD_LENGTH {
            return Err(CoinbaseError::PayloadLenBelowMin(payload.len(), MIN_PAYLOAD_LENGTH));
        }

        if payload.len() > self.max_coinbase_payload_len {
            return Err(CoinbaseError::PayloadLenAboveMax(payload.len(), self.max_coinbase_payload_len));
        }

        let mut parser = PayloadParser::new(payload);

        let blue_score = u64::from_le_bytes(parser.take(LENGTH_OF_BLUE_SCORE).try_into().unwrap());
        let subsidy = u64::from_le_bytes(parser.take(LENGTH_OF_SUBSIDY).try_into().unwrap());
        let script_pub_key_version = u16::from_le_bytes(parser.take(LENGTH_OF_SCRIPT_PUB_KEY_VERSION).try_into().unwrap());
        let script_pub_key_len = u8::from_le_bytes(parser.take(LENGTH_OF_SCRIPT_PUB_KEY_LENGTH).try_into().unwrap());

        if script_pub_key_len > self.coinbase_payload_script_public_key_max_len {
            return Err(CoinbaseError::PayloadScriptPublicKeyLenAboveMax(
                script_pub_key_len as usize,
                self.coinbase_payload_script_public_key_max_len,
            ));
        }

        if parser.remaining.len() < script_pub_key_len as usize {
            return Err(CoinbaseError::PayloadCantContainScriptPublicKey(
                payload.len(),
                MIN_PAYLOAD_LENGTH + script_pub_key_len as usize,
            ));
        }

        let script_public_key =
            ScriptPublicKey::new(script_pub_key_version, ScriptVec::from_slice(parser.take(script_pub_key_len as usize)));
        let extra_data = parser.remaining;

        Ok(CoinbaseData { blue_score, subsidy, miner_data: MinerData { script_public_key, extra_data } })
    }

    pub fn calc_block_subsidy(&self, daa_score: u64) -> u64 {
        if daa_score < self.deflationary_phase_daa_score {
            return self.pre_deflationary_phase_base_subsidy;
        }

        let subsidy_month = self.subsidy_month(daa_score) as usize;
        self.subsidy_by_month_table[subsidy_month.min(self.subsidy_by_month_table.len() - 1)]
    }

    /// Get the subsidy month as function of the current DAA score.
    ///
    /// Note that this function is called only if daa_score >= self.deflationary_phase_daa_score
    fn subsidy_month(&self, daa_score: u64) -> u64 {
        let seconds_since_deflationary_phase_started = (daa_score - self.deflationary_phase_daa_score) / self.bps;
        seconds_since_deflationary_phase_started / SECONDS_PER_MONTH
    }

    #[cfg(test)]
    pub fn legacy_calc_block_subsidy(&self, daa_score: u64) -> u64 {
        if daa_score < self.deflationary_phase_daa_score {
            return self.pre_deflationary_phase_base_subsidy;
        }

        // Note that this calculation implicitly assumes that block per second = 1 (by assuming daa score diff is in second units).
        let months_since_deflationary_phase_started = (daa_score - self.deflationary_phase_daa_score) / SECONDS_PER_MONTH;
        assert!(months_since_deflationary_phase_started <= usize::MAX as u64);
        let months_since_deflationary_phase_started: usize = months_since_deflationary_phase_started as usize;
        if months_since_deflationary_phase_started >= SUBSIDY_BY_MONTH_TABLE.len() {
            *SUBSIDY_BY_MONTH_TABLE.last().unwrap()
        } else {
            SUBSIDY_BY_MONTH_TABLE[months_since_deflationary_phase_started]
        }
    }
}

/*
    This table was pre-calculated by calling `calcDeflationaryPeriodBlockSubsidyFloatCalc` (in Sporad-go) for all months until reaching 0 subsidy.
    To regenerate this table, run `TestBuildSubsidyTable` in coinbasemanager_test.go (note the `deflationaryPhaseBaseSubsidy` therein).
    These values represent the reward per second for each month (= reward per block for 1 BPS).
*/
#[rustfmt::skip]
const SUBSIDY_BY_MONTH_TABLE: [u64; 426] = [
    45000000000, 42474344070, 40090442316, 37840338686, 35716523669, 33711909229, 31819805153, 30033896718, 28348223622, 26757160087, 25255396086, 23837919623,
    22500000000, 21237172035, 20045221158, 18920169343, 17858261834, 16855954614, 15909902576, 15016948359, 14174111811, 13378580043, 12627698043, 11918959811,
    11250000000, 10618586017, 10022610579,  9460084671,  8929130917,  8427977307,  7954951288,  7508474179,  7087055905,  6689290021,  6313849021,  5959479905,
     5625000000,  5309293008,  5011305289,  4730042335,  4464565458,  4213988653,  3977475644,  3754237089,  3543527952,  3344645010,  3156924510,  2979739952,
     2812500000,  2654646504,  2505652644,  2365021167,  2232282729,  2106994326,  1988737822,  1877118544,  1771763976,  1672322505,  1578462255,  1489869976,
     1406250000,  1327323252,  1252826322,  1182510583,  1116141364,  1053497163,   994368911,   938559272,   885881988,   836161252,   789231127,   744934988,
      703125000,   663661626,   626413161,   591255291,   558070682,   526748581,   497184455,   469279636,   442940994,   418080626,   394615563,   372467494,
      351562500,   331830813,   313206580,   295627645,   279035341,   263374290,   248592227,   234639818,   221470497,   209040313,   197307781,   186233747,
      175781250,   165915406,   156603290,   147813822,   139517670,   131687145,   124296113,   117319909,   110735248,   104520156,    98653890,    93116873,
       87890625,    82957703,    78301645,    73906911,    69758835,    65843572,    62148056,    58659954,    55367624,    52260078,    49326945,    46558436,
       43945312,    41478851,    39150822,    36953455,    34879417,    32921786,    31074028,    29329977,    27683812,    26130039,    24663472,    23279218,
       21972656,    20739425,    19575411,    18476727,    17439708,    16460893,    15537014,    14664988,    13841906,    13065019,    12331736,    11639609,
       10986328,    10369712,     9787705,     9238363,     8719854,     8230446,     7768507,     7332494,     6920953,     6532509,     6165868,     5819804,
        5493164,     5184856,     4893852,     4619181,     4359927,     4115223,     3884253,     3666247,     3460476,     3266254,     3082934,     2909902,
        2746582,     2592428,     2446926,     2309590,     2179963,     2057611,     1942126,     1833123,     1730238,     1633127,     1541467,     1454951,
        1373291,     1296214,     1223463,     1154795,     1089981,     1028805,      971063,      916561,      865119,      816563,      770733,      727475,
         686645,      648107,      611731,      577397,      544990,      514402,      485531,      458280,      432559,      408281,      385366,      363737,
         343322,      324053,      305865,      288698,      272495,      257201,      242765,      229140,      216279,      204140,      192683,      181868,
         171661,      162026,      152932,      144349,      136247,      128600,      121382,      114570,      108139,      102070,       96341,       90934,
          85830,       81013,       76466,       72174,       68123,       64300,       60691,       57285,       54069,       51035,       48170,       45467,
          42915,       40506,       38233,       36087,       34061,       32150,       30345,       28642,       27034,       25517,       24085,       22733,
          21457,       20263,       19116,       18043,       17030,       16075,       15172,       14321,       13517,       12758,       12042,       11366,
          10728,       10126,        9558,        9021,        8515,        8037,        7586,        7160,        6758,        6379,        6021,        5683,
           5364,        5063,        4779,        4510,        4257,        4018,        3793,        3580,        3379,        3189,        3010,        2841,
           2682,        2531,        2389,        2255,        2128,        2009,        1896,        1790,        1689,        1594,        1505,        1420,
           1341,        1265,        1194,        1127,        1064,        1004,         948,         895,         844,         797,         752,         710,
            670,         632,         597,         563,         532,         502,         474,         447,         422,         398,         376,         355,
            335,         316,         298,         281,         266,         251,         237,         223,         211,         199,         188,         177,
            167,         158,         149,         140,         133,         125,         118,         111,         105,          99,          94,          88,
             83,          79,          74,          70,          66,          62,          59,          55,          52,          49,          47,          44,
             41,          39,          37,          35,          33,          31,          29,          27,          26,          24,          23,          22,
             20,          19,          18,          17,          16,          15,          14,          13,          13,          12,          11,          11,
             10,           9,           9,           8,           8,           7,           7,           6,           6,           6,           5,           5,
              5,           4,           4,           4,           4,           3,           3,           3,           3,           3,           2,           2,
              2,           2,           2,           2,           2,           1,           1,           1,           1,           1,           1,           1,
              1,           1,           1,           1,           1,           0
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::MAINNET_PARAMS;
    use spora_consensus_core::{
        config::params::{Params, SIMNET_PARAMS, TESTNET_PARAMS},
        constants::SAU_PER_SPORA,
        network::{NetworkId, NetworkType},
        tx::scriptvec,
    };

    #[test]
    fn test_calc_high_bps_total_rewards() {
        let cbm = create_manager(&TESTNET_PARAMS);
        let pre_deflationary_rewards = cbm.pre_deflationary_phase_base_subsidy * cbm.deflationary_phase_daa_score;
        let bps = cbm.bps();

        let total_rewards: u64 = pre_deflationary_rewards + SUBSIDY_BY_MONTH_TABLE.iter().map(|x| x * SECONDS_PER_MONTH).sum::<u64>();
        let total_high_bps_rewards_rounded_up: u64 =
            pre_deflationary_rewards + SUBSIDY_BY_MONTH_TABLE.iter().map(|x| (x.div_ceil(bps) * bps) * SECONDS_PER_MONTH).sum::<u64>();

        let total_high_bps_rewards: u64 =
            pre_deflationary_rewards + cbm.subsidy_by_month_table.iter().map(|x| x * SECONDS_PER_MONTH * bps).sum::<u64>();
        assert_eq!(total_high_bps_rewards_rounded_up, total_high_bps_rewards, "subsidy adjusted to bps must be rounded up");

        let delta = total_high_bps_rewards as i64 - total_rewards as i64;

        println!("Subsidy Table :{SUBSIDY_BY_MONTH_TABLE:?}");
        println!("Pre Deflationary Rewards :{pre_deflationary_rewards}");
        println!("Total rewards: {} sau => {} SPORA", total_rewards, total_rewards / SAU_PER_SPORA);
        println!("Total high bps rewards: {} sau => {} SPORA", total_high_bps_rewards, total_high_bps_rewards / SAU_PER_SPORA);
        println!("Delta: {} sau => {} SPORA", delta, delta / SAU_PER_SPORA as i64);
    }

    #[test]
    fn calc_high_bps_total_rewards_delta() {
        let legacy_cbm = create_legacy_manager();
        let pre_deflationary_rewards = legacy_cbm.pre_deflationary_phase_base_subsidy * legacy_cbm.deflationary_phase_daa_score;
        let total_rewards: u64 = pre_deflationary_rewards + SUBSIDY_BY_MONTH_TABLE.iter().map(|x| x * SECONDS_PER_MONTH).sum::<u64>();
        let testnet_11_bps = SIMNET_PARAMS.bps();
        let total_high_bps_rewards_rounded_up: u64 = pre_deflationary_rewards
            + SUBSIDY_BY_MONTH_TABLE.iter().map(|x| (x.div_ceil(testnet_11_bps) * testnet_11_bps) * SECONDS_PER_MONTH).sum::<u64>();

        let cbm = create_manager(&SIMNET_PARAMS);
        let total_high_bps_rewards: u64 =
            pre_deflationary_rewards + cbm.subsidy_by_month_table.iter().map(|x| x * SECONDS_PER_MONTH * cbm.bps()).sum::<u64>();
        assert_eq!(total_high_bps_rewards_rounded_up, total_high_bps_rewards, "subsidy adjusted to bps must be rounded up");

        let delta = total_high_bps_rewards as i64 - total_rewards as i64;

        println!("Total rewards: {} sau => {} SPORA", total_rewards, total_rewards / SAU_PER_SPORA);
        println!("Total high bps rewards: {} sau => {} SPORA", total_high_bps_rewards, total_high_bps_rewards / SAU_PER_SPORA);
        println!("Delta: {} sau => {} SPORA", delta, delta / SAU_PER_SPORA as i64);
    }

    #[test]
    fn subsidy_by_month_table_test() {
        let cbm = create_legacy_manager();
        cbm.subsidy_by_month_table.iter().enumerate().for_each(|(i, x)| {
            assert_eq!(SUBSIDY_BY_MONTH_TABLE[i], *x, "for 1 BPS, const table and precomputed values must match");
        });

        for network_id in NetworkId::iter() {
            let cbm = create_manager(&network_id.into());
            cbm.subsidy_by_month_table.iter().enumerate().for_each(|(i, x)| {
                assert_eq!(
                    SUBSIDY_BY_MONTH_TABLE[i].div_ceil(cbm.bps()),
                    *x,
                    "{}: locally computed and precomputed values must match",
                    network_id
                );
            });
        }
    }

    #[test]
    fn verify_emission_schedule() {
        for network_id in [NetworkId::new(NetworkType::Mainnet)] {
            let params: Params = network_id.into();
            let cbm = create_manager(&params);
            let (_epochs, _total) = calculate_emission(cbm);
        }
    }

    fn calculate_emission(cbm: CoinbaseManager) -> (u64, u64) {
        let mut current = 0;
        let mut total = 0;
        let mut epoch = 0u64;
        let mut prev = cbm.calc_block_subsidy(0);
        loop {
            let subsidy = cbm.calc_block_subsidy(current);
            if subsidy == 0 {
                break;
            }
            total += subsidy;
            if subsidy != prev {
                prev = subsidy;
                epoch += 1;
            }
            current += 1;
        }

        (epoch, total)
    }

    #[test]
    fn subsidy_test() {
        const PRE_DEFLATIONARY_PHASE_BASE_SUBSIDY: u64 = 500_00000000;
        const DEFLATIONARY_PHASE_INITIAL_SUBSIDY: u64 = 450_00000000;
        const SECONDS_PER_MONTH: u64 = 2629800;
        const SECONDS_PER_HALVING: u64 = SECONDS_PER_MONTH * 12;

        for network_id in NetworkId::iter() {
            let params: Params = network_id.into();
            let cbm = create_manager(&params);
            let bps = params.bps();
            println!("BPS is: {}", bps);

            // pre_deflationary_phase_base_subsidy is already adjusted for BPS in the network configuration:
            // - MAINNET/TESTNET/DEVNET: uses raw value 50000000000
            // - SIMNET: uses TenBps::pre_deflationary_phase_base_subsidy() = 50000000000 / 10
            // So we don't need to divide by BPS here
            let pre_deflationary_phase_base_subsidy = params.pre_deflationary_phase_base_subsidy;

            // deflationary_phase_initial_subsidy uses the subsidy table which is defined per-second,
            // so we need to divide by BPS to get per-block subsidy
            let deflationary_phase_initial_subsidy = DEFLATIONARY_PHASE_INITIAL_SUBSIDY / bps;
            let blocks_per_halving = SECONDS_PER_HALVING * bps;

            struct Test {
                name: &'static str,
                daa_score: u64,
                expected: u64,
            }

            let tests = vec![
                Test { name: "first mined block", daa_score: 1, expected: pre_deflationary_phase_base_subsidy },
                Test {
                    name: "before deflationary phase",
                    daa_score: params.deflationary_phase_daa_score - 1,
                    expected: pre_deflationary_phase_base_subsidy,
                },
                Test {
                    name: "start of deflationary phase",
                    daa_score: params.deflationary_phase_daa_score,
                    expected: deflationary_phase_initial_subsidy,
                },
                Test {
                    name: "after one halving",
                    daa_score: params.deflationary_phase_daa_score + blocks_per_halving,
                    expected: deflationary_phase_initial_subsidy / 2,
                },
                Test {
                    name: "after 2 halvings",
                    daa_score: params.deflationary_phase_daa_score + 2 * blocks_per_halving,
                    expected: deflationary_phase_initial_subsidy / 4,
                },
                Test {
                    name: "after 5 halvings",
                    daa_score: params.deflationary_phase_daa_score + 5 * blocks_per_halving,
                    expected: deflationary_phase_initial_subsidy / 32,
                },
                Test {
                    name: "after 32 halvings",
                    daa_score: params.deflationary_phase_daa_score + 32 * blocks_per_halving,
                    expected: (DEFLATIONARY_PHASE_INITIAL_SUBSIDY / 2_u64.pow(32)).div_ceil(bps),
                },
                Test {
                    name: "just before subsidy depleted",
                    daa_score: params.deflationary_phase_daa_score + 35 * blocks_per_halving,
                    expected: 1,
                },
                Test {
                    name: "after subsidy depleted",
                    daa_score: params.deflationary_phase_daa_score + 36 * blocks_per_halving,
                    expected: 0,
                },
            ];

            for t in tests {
                println!("Running {} test: '{}'", network_id, t.name);
                assert_eq!(cbm.calc_block_subsidy(t.daa_score), t.expected, "{} test '{}' failed", network_id, t.name);
            }
        }
    }

    #[test]
    fn payload_serialization_test() {
        let cbm = create_manager(&MAINNET_PARAMS);

        let script_data = [33u8, 255];
        let extra_data = [2u8, 3];
        let data = CoinbaseData {
            blue_score: 56,
            subsidy: 44000000000,
            miner_data: MinerData {
                script_public_key: ScriptPublicKey::new(0, ScriptVec::from_slice(&script_data)),
                extra_data: &extra_data as &[u8],
            },
        };

        let payload = cbm.serialize_coinbase_payload(&data).unwrap();
        let deserialized_data = cbm.deserialize_coinbase_payload(&payload).unwrap();

        assert_eq!(data, deserialized_data);

        // Test an actual mainnet payload
        let payload_hex =
            "b612c90100000000041a763e07000000000022202b32443ff740012157716d81216d09aebc39e5493c93a7181d92cb756c02c560ac302e31322e382f";
        let mut payload = vec![0u8; payload_hex.len() / 2];
        faster_hex::hex_decode(payload_hex.as_bytes(), &mut payload).unwrap();
        let deserialized_data = cbm.deserialize_coinbase_payload(&payload).unwrap();

        let expected_data = CoinbaseData {
            blue_score: 29954742,
            subsidy: 31112698372,
            miner_data: MinerData {
                script_public_key: ScriptPublicKey::new(
                    0,
                    scriptvec![
                        32, 43, 50, 68, 63, 247, 64, 1, 33, 87, 113, 109, 129, 33, 109, 9, 174, 188, 57, 229, 73, 60, 147, 167, 24,
                        29, 146, 203, 117, 108, 2, 197, 96, 172,
                    ],
                ),
                extra_data: &[48u8, 46, 49, 50, 46, 56, 47] as &[u8],
            },
        };
        assert_eq!(expected_data, deserialized_data);
    }

    #[test]
    fn modify_payload_test() {
        let cbm = create_manager(&MAINNET_PARAMS);

        let script_data = [33u8, 255];
        let extra_data = [2u8, 3, 23, 98];
        let data = CoinbaseData {
            blue_score: 56345,
            subsidy: 44000000000,
            miner_data: MinerData {
                script_public_key: ScriptPublicKey::new(0, ScriptVec::from_slice(&script_data)),
                extra_data: &extra_data,
            },
        };

        let data2 = CoinbaseData {
            blue_score: data.blue_score,
            subsidy: data.subsidy,
            miner_data: MinerData {
                // Modify only miner data
                script_public_key: ScriptPublicKey::new(0, ScriptVec::from_slice(&[33u8, 255, 33])),
                extra_data: &[2u8, 3, 23, 98, 34, 34] as &[u8],
            },
        };

        let mut payload = cbm.serialize_coinbase_payload(&data).unwrap();
        payload = cbm.modify_coinbase_payload(payload, &data2.miner_data).unwrap(); // Update the payload with the modified miner data
        let deserialized_data = cbm.deserialize_coinbase_payload(&payload).unwrap();

        assert_eq!(data2, deserialized_data);
    }

    fn create_manager(params: &Params) -> CoinbaseManager {
        CoinbaseManager::new(
            params.coinbase_payload_script_public_key_max_len,
            params.max_coinbase_payload_len,
            params.deflationary_phase_daa_score,
            params.pre_deflationary_phase_base_subsidy,
            params.bps(),
        )
    }

    fn create_legacy_manager() -> CoinbaseManager {
        CoinbaseManager::new(150, 204, 15778800 - 259200, 50000000000, 1)
    }
}
