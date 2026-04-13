use crate::pb as protowire;
use spora_consensus_core::{header::Header, BlueWorkType};
use spora_hashes::Hash;

use super::error::ConversionError;
use super::option::TryIntoOptionEx;

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<&Header> for protowire::BlockHeader {
    fn from(item: &Header) -> Self {
        Self {
            version: item.version.into(),
            parents: item.parents_by_level.iter().map(protowire::BlockLevelParents::from).collect(),
            hash_merkle_root: Some(item.hash_merkle_root.into()),
            accepted_id_merkle_root: Some(item.accepted_id_merkle_root.into()),
            cell_commitment: Some(item.cell_commitment.into()),
            cell_root: Some(item.cell_root.into()),
            segment_root: Some(item.segment_root.into()),
            timestamp: item.timestamp.try_into().expect("timestamp is always convertible to i64"),
            bits: item.bits,
            nonce: item.nonce,
            daa_score: item.daa_score,
            // We follow the golang specification of variable big-endian here
            blue_work: item.blue_work.to_be_bytes_var(),
            blue_score: item.blue_score,
            pruning_point: Some(item.pruning_point.into()),
        }
    }
}

impl From<&Vec<Hash>> for protowire::BlockLevelParents {
    fn from(item: &Vec<Hash>) -> Self {
        Self { parent_hashes: item.iter().map(|h| h.into()).collect() }
    }
}

// ----------------------------------------------------------------------------
// protowire to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<protowire::BlockHeader> for Header {
    type Error = ConversionError;
    fn try_from(item: protowire::BlockHeader) -> Result<Self, Self::Error> {
        Ok(Self::new_finalized(
            item.version,
            item.parents.into_iter().map(Vec::<Hash>::try_from).collect::<Result<Vec<Vec<Hash>>, ConversionError>>()?,
            item.hash_merkle_root.try_into_ex()?,
            item.accepted_id_merkle_root.try_into_ex()?,
            item.cell_commitment.try_into_ex()?,
            item.cell_root.try_into_ex()?,
            item.segment_root.try_into_ex()?,
            item.timestamp.try_into()?,
            item.bits,
            item.nonce,
            item.daa_score,
            // We follow the golang specification of variable big-endian here
            BlueWorkType::from_be_bytes_var(&item.blue_work)?,
            item.blue_score,
            item.pruning_point.try_into_ex()?,
        ))
    }
}

impl TryFrom<protowire::BlockLevelParents> for Vec<Hash> {
    type Error = ConversionError;
    fn try_from(item: protowire::BlockLevelParents) -> Result<Self, Self::Error> {
        item.parent_hashes.into_iter().map(|x| x.try_into()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash_from_byte(byte: u8) -> Hash {
        Hash::from_bytes([byte; 32])
    }

    #[test]
    fn test_block_header_roundtrip_preserves_cell_root() {
        let header = Header::new_finalized(
            1,
            vec![vec![hash_from_byte(1), hash_from_byte(2)], vec![hash_from_byte(3)]],
            hash_from_byte(4),
            hash_from_byte(5),
            hash_from_byte(6),
            hash_from_byte(7),
            hash_from_byte(8),
            123,
            456,
            789,
            1011,
            1213.into(),
            1415,
            hash_from_byte(9),
        );

        let wire: protowire::BlockHeader = (&header).into();
        let decoded = Header::try_from(wire).unwrap();

        assert_eq!(decoded.cell_root, header.cell_root);
        assert_eq!(decoded.segment_root, header.segment_root);
        assert_eq!(decoded.hash, header.hash);
    }
}
