use spora_consensus_core::{
    coinbase::{CoinbaseData, CoinbaseTransactionTemplate, MinerData},
    constants::SAU_PER_SPORA,
    tx::{cell_out_from_legacy_script_public_key, CellTx, TransactionOutput},
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
        let output = TransactionOutput::new(SUBSIDY, miner_data.script_public_key.clone());

        let payload = self.serialize_coinbase_payload(&CoinbaseData { blue_score: 1, subsidy: SUBSIDY, miner_data });
        let outputs = vec![cell_out_from_legacy_script_public_key(output.value, &output.script_public_key)];
        let outputs_data = vec![payload];

        CoinbaseTransactionTemplate {
            tx: CellTx::new(vec![], vec![], outputs, outputs_data, vec![]).expect("mock coinbase must be a valid CellTx"),
            has_red_reward: false,
        }
    }

    pub(super) fn serialize_coinbase_payload(&self, data: &CoinbaseData) -> Vec<u8> {
        let script_pub_key_len = data.miner_data.script_public_key.script().len();
        let payload: Vec<u8> = data.blue_score.to_le_bytes().iter().copied()                    // Blue score                   (u64)
            .chain(data.subsidy.to_le_bytes().iter().copied())                                  // Subsidy                      (u64)
            .chain(data.miner_data.script_public_key.version().to_le_bytes().iter().copied())   // Script public key version    (u16)
            .chain((script_pub_key_len as u8).to_le_bytes().iter().copied())                    // Script public key length     (u8)
            .chain(data.miner_data.script_public_key.script().iter().copied())                  // Script public key            
            .chain(data.miner_data.extra_data.iter().copied())                                  // Extra data
            .collect();

        payload
    }

    pub fn modify_coinbase_payload(&self, mut payload: Vec<u8>, miner_data: &MinerData) -> Vec<u8> {
        let script_pub_key_len = miner_data.script_public_key.script().len();
        payload.truncate(LENGTH_OF_BLUE_SCORE + LENGTH_OF_SUBSIDY);
        payload.extend(
            miner_data.script_public_key.version().to_le_bytes().iter().copied() // Script public key version (u16)
                .chain((script_pub_key_len as u8).to_le_bytes().iter().copied()) // Script public key length  (u8)
                .chain(miner_data.script_public_key.script().iter().copied())    // Script public key
                .chain(miner_data.extra_data.iter().copied()), // Extra data
        );

        payload
    }
}
