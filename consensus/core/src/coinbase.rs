use crate::tx::{CellTx, ScriptRef};
use serde::{Deserialize, Serialize};

pub const COINBASE_MASS_COMMITMENT_MAGIC: [u8; 4] = *b"SMC1";

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct MinerData<T: AsRef<[u8]> = Vec<u8>> {
    pub lock_script: ScriptRef,
    pub extra_data: T,
}

impl<T: AsRef<[u8]>> MinerData<T> {
    pub fn new(lock_script: ScriptRef, extra_data: T) -> Self {
        Self { lock_script, extra_data }
    }
}

#[derive(PartialEq, Eq, Debug)]
pub struct CoinbaseData<T: AsRef<[u8]> = Vec<u8>> {
    pub blue_score: u64,
    pub subsidy: u64,
    pub mass_commitment: u64,
    pub miner_data: MinerData<T>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct BlockRewardData {
    pub subsidy: u64,
    pub total_fees: u64,
    pub lock_script: ScriptRef,
}

impl BlockRewardData {
    pub fn new(subsidy: u64, total_fees: u64, lock_script: ScriptRef) -> Self {
        Self { subsidy, total_fees, lock_script }
    }
}

/// Holds a coinbase transaction along with meta-data obtained during creation
pub struct CoinbaseTransactionTemplate {
    pub tx: CellTx,
    pub has_red_reward: bool, // Does the last output contain reward for red blocks
}
