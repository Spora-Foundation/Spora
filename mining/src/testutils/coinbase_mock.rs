use spora_consensus_core::{
    coinbase::{CoinbaseData, CoinbaseTransactionTemplate, MinerData, COINBASE_MASS_COMMITMENT_MAGIC},
    constants::SAU_PER_SPORA,
    tx::{CellOut, CellTx},
};

const LENGTH_OF_BLUE_SCORE: usize = size_of::<u64>();
const LENGTH_OF_SUBSIDY: usize = size_of::<u64>();

pub(super) struct CoinbaseManagerMock {}

impl CoinbaseManagerMock {
    pub(super) fn new() -> Self {
        Self {}
    }

    pub(super) fn expected_coinbase_transaction(&self, miner_data: MinerData) -> CoinbaseTransactionTemplate {
        const SUBSIDY: u64 = 500 * SAU_PER_SPORA;
        let payload = self.serialize_coinbase_payload(&CoinbaseData {
            blue_score: 1,
            subsidy: SUBSIDY,
            mass_commitment: 0,
            miner_data: miner_data.clone(),
        });
        let outputs = vec![CellOut { capacity: SUBSIDY, lock: miner_data.lock_script.clone(), type_: None }];
        let outputs_data = vec![payload];

        CoinbaseTransactionTemplate {
            tx: CellTx::new(vec![], vec![], outputs, outputs_data, vec![]).expect("mock coinbase must be a valid CellTx"),
            has_red_reward: false,
        }
    }

    pub(super) fn serialize_coinbase_payload(&self, data: &CoinbaseData) -> Vec<u8> {
        let lock_args_len = data.miner_data.lock_script.args.len();
        let payload: Vec<u8> = data.blue_score.to_le_bytes().iter().copied()                    // Blue score                   (u64)
            .chain(data.subsidy.to_le_bytes().iter().copied())                                  // Subsidy                      (u64)
            .chain(data.miner_data.lock_script.code_hash.iter().copied())                       // Lock script code hash        (32)
            .chain(data.miner_data.lock_script.hash_type.to_le_bytes().iter().copied())         // Lock script hash type        (u8)
            .chain((lock_args_len as u8).to_le_bytes().iter().copied())                         // Lock script args length      (u8)
            .chain(data.miner_data.lock_script.args.iter().copied())                            // Lock script args
            .chain(COINBASE_MASS_COMMITMENT_MAGIC.iter().copied())                              // Mass commitment marker
            .chain(data.mass_commitment.to_le_bytes().iter().copied())                          // Mass commitment              (u64)
            .chain(data.miner_data.extra_data.iter().copied())                                  // Extra data
            .collect();

        payload
    }

    pub fn modify_coinbase_payload(&self, payload: Vec<u8>, miner_data: &MinerData) -> Vec<u8> {
        let blue_score = u64::from_le_bytes(payload[..LENGTH_OF_BLUE_SCORE].try_into().expect("mock payload must contain blue score"));
        let subsidy = u64::from_le_bytes(
            payload[LENGTH_OF_BLUE_SCORE..LENGTH_OF_BLUE_SCORE + LENGTH_OF_SUBSIDY]
                .try_into()
                .expect("mock payload must contain subsidy"),
        );
        self.serialize_coinbase_payload(&CoinbaseData { blue_score, subsidy, mass_commitment: 0, miner_data: miner_data.clone() })
    }
}
