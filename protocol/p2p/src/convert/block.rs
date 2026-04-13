use super::{error::ConversionError, option::TryIntoOptionEx};
use crate::pb as protowire;
use spora_consensus_core::block::Block;

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<&Block> for protowire::BlockMessage {
    fn from(block: &Block) -> Self {
        Self { header: Some(block.header.as_ref().into()), transactions: block.transactions.iter().map(Into::into).collect() }
    }
}

// ----------------------------------------------------------------------------
// protowire to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<protowire::BlockMessage> for Block {
    type Error = ConversionError;

    fn try_from(block: protowire::BlockMessage) -> Result<Self, Self::Error> {
        Ok(Self::new(block.header.try_into_ex()?, block.transactions.into_iter().map(TryInto::try_into).collect::<Result<_, _>>()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spora_consensus_core::{
        header::Header,
        tx::{CellOut, CellRef, CellTx, OutPoint, ScriptRef},
        BlueWorkType,
    };
    use spora_hashes::Hash;

    fn hash_from_byte(byte: u8) -> Hash {
        Hash::from_bytes([byte; 32])
    }

    fn sample_header() -> Header {
        Header::new_finalized(
            1,
            vec![vec![hash_from_byte(1)], vec![hash_from_byte(2), hash_from_byte(3)]],
            hash_from_byte(4),
            hash_from_byte(5),
            hash_from_byte(6),
            hash_from_byte(7),
            hash_from_byte(8),
            123,
            456,
            789,
            1011,
            BlueWorkType::from(1213u64),
            1415,
            hash_from_byte(9),
        )
    }

    fn sample_tx(seed: u8) -> CellTx {
        CellTx::new(
            vec![CellRef::new(OutPoint::new([seed; 32], 0), u64::from(seed))],
            vec![],
            vec![CellOut {
                lock: ScriptRef::new([seed + 1; 32], 1, vec![seed, seed + 1]),
                type_: None,
                capacity: 1_000 + u64::from(seed),
            }],
            vec![vec![seed, seed + 2]],
            vec![vec![seed + 3, seed + 4]],
        )
        .expect("sample block transaction")
    }

    #[test]
    fn block_roundtrip_preserves_transactions() {
        let block = Block::new(sample_header(), vec![sample_tx(7), sample_tx(9)]);

        let wire = protowire::BlockMessage::from(&block);
        let decoded = Block::try_from(wire).unwrap();

        assert_eq!(decoded.transactions, block.transactions);
        assert_eq!(decoded.header.hash, block.header.hash);
    }
}
