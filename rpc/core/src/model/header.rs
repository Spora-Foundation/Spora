use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use spora_consensus_core::{header::Header, BlueWorkType};
use spora_hashes::Hash;
use workflow_serializer::prelude::*;

/// Raw Rpc header type - without a cached header hash.
/// Used for mining APIs (get_block_template & submit_block)
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcRawHeader {
    pub version: u16,
    pub parents_by_level: Vec<Vec<Hash>>,
    pub hash_merkle_root: Hash,
    pub accepted_id_merkle_root: Hash,
    pub cell_commitment: Hash,
    pub cell_root: Hash,
    /// Timestamp is in milliseconds
    pub timestamp: u64,
    pub bits: u32,
    pub nonce: u64,
    pub daa_score: u64,
    pub blue_work: BlueWorkType,
    pub blue_score: u64,
    pub pruning_point: Hash,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcHeader {
    /// Cached hash
    pub hash: Hash,
    pub version: u16,
    pub parents_by_level: Vec<Vec<Hash>>,
    pub hash_merkle_root: Hash,
    pub accepted_id_merkle_root: Hash,
    pub cell_commitment: Hash,
    pub cell_root: Hash,
    /// Timestamp is in milliseconds
    pub timestamp: u64,
    pub bits: u32,
    pub nonce: u64,
    pub daa_score: u64,
    pub blue_work: BlueWorkType,
    pub blue_score: u64,
    pub pruning_point: Hash,
}

impl RpcHeader {
    pub fn direct_parents(&self) -> &[Hash] {
        if self.parents_by_level.is_empty() {
            &[]
        } else {
            &self.parents_by_level[0]
        }
    }
}

impl AsRef<RpcHeader> for RpcHeader {
    fn as_ref(&self) -> &RpcHeader {
        self
    }
}

impl From<Header> for RpcHeader {
    fn from(header: Header) -> Self {
        Self {
            hash: header.hash,
            version: header.version,
            parents_by_level: header.parents_by_level,
            hash_merkle_root: header.hash_merkle_root,
            accepted_id_merkle_root: header.accepted_id_merkle_root,
            cell_commitment: header.cell_commitment,
            cell_root: header.cell_root,
            timestamp: header.timestamp,
            bits: header.bits,
            nonce: header.nonce,
            daa_score: header.daa_score,
            blue_work: header.blue_work,
            blue_score: header.blue_score,
            pruning_point: header.pruning_point,
        }
    }
}

impl From<&Header> for RpcHeader {
    fn from(header: &Header) -> Self {
        Self {
            hash: header.hash,
            version: header.version,
            parents_by_level: header.parents_by_level.clone(),
            hash_merkle_root: header.hash_merkle_root,
            accepted_id_merkle_root: header.accepted_id_merkle_root,
            cell_commitment: header.cell_commitment,
            cell_root: header.cell_root,
            timestamp: header.timestamp,
            bits: header.bits,
            nonce: header.nonce,
            daa_score: header.daa_score,
            blue_work: header.blue_work,
            blue_score: header.blue_score,
            pruning_point: header.pruning_point,
        }
    }
}

impl From<RpcHeader> for Header {
    fn from(header: RpcHeader) -> Self {
        Self {
            hash: header.hash,
            version: header.version,
            parents_by_level: header.parents_by_level,
            hash_merkle_root: header.hash_merkle_root,
            accepted_id_merkle_root: header.accepted_id_merkle_root,
            cell_commitment: header.cell_commitment,
            cell_root: header.cell_root,
            timestamp: header.timestamp,
            bits: header.bits,
            nonce: header.nonce,
            daa_score: header.daa_score,
            blue_work: header.blue_work,
            blue_score: header.blue_score,
            pruning_point: header.pruning_point,
        }
    }
}

impl From<&RpcHeader> for Header {
    fn from(header: &RpcHeader) -> Self {
        Self {
            hash: header.hash,
            version: header.version,
            parents_by_level: header.parents_by_level.clone(),
            hash_merkle_root: header.hash_merkle_root,
            accepted_id_merkle_root: header.accepted_id_merkle_root,
            cell_commitment: header.cell_commitment,
            cell_root: header.cell_root,
            timestamp: header.timestamp,
            bits: header.bits,
            nonce: header.nonce,
            daa_score: header.daa_score,
            blue_work: header.blue_work,
            blue_score: header.blue_score,
            pruning_point: header.pruning_point,
        }
    }
}

impl Serializer for RpcHeader {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &2, writer)?;

        store!(Hash, &self.hash, writer)?;
        store!(u16, &self.version, writer)?;
        store!(Vec<Vec<Hash>>, &self.parents_by_level, writer)?;
        store!(Hash, &self.hash_merkle_root, writer)?;
        store!(Hash, &self.accepted_id_merkle_root, writer)?;
        store!(Hash, &self.cell_commitment, writer)?;
        store!(Hash, &self.cell_root, writer)?;
        store!(u64, &self.timestamp, writer)?;
        store!(u32, &self.bits, writer)?;
        store!(u64, &self.nonce, writer)?;
        store!(u64, &self.daa_score, writer)?;
        store!(BlueWorkType, &self.blue_work, writer)?;
        store!(u64, &self.blue_score, writer)?;
        store!(Hash, &self.pruning_point, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcHeader {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let payload_version = load!(u16, reader)?;
        if payload_version != 2 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unsupported RpcHeader version {payload_version}, expected 2"),
            ));
        }

        let hash = load!(Hash, reader)?;
        let version = load!(u16, reader)?;
        let parents_by_level = load!(Vec<Vec<Hash>>, reader)?;
        let hash_merkle_root = load!(Hash, reader)?;
        let accepted_id_merkle_root = load!(Hash, reader)?;
        let cell_commitment = load!(Hash, reader)?;
        let cell_root = load!(Hash, reader)?;
        let timestamp = load!(u64, reader)?;
        let bits = load!(u32, reader)?;
        let nonce = load!(u64, reader)?;
        let daa_score = load!(u64, reader)?;
        let blue_work = load!(BlueWorkType, reader)?;
        let blue_score = load!(u64, reader)?;
        let pruning_point = load!(Hash, reader)?;

        Ok(Self {
            hash,
            version,
            parents_by_level,
            hash_merkle_root,
            accepted_id_merkle_root,
            cell_commitment,
            cell_root,
            timestamp,
            bits,
            nonce,
            daa_score,
            blue_work,
            blue_score,
            pruning_point,
        })
    }
}

impl From<RpcRawHeader> for Header {
    fn from(header: RpcRawHeader) -> Self {
        Self::new_finalized(
            header.version,
            header.parents_by_level,
            header.hash_merkle_root,
            header.accepted_id_merkle_root,
            header.cell_commitment,
            header.cell_root,
            header.timestamp,
            header.bits,
            header.nonce,
            header.daa_score,
            header.blue_work,
            header.blue_score,
            header.pruning_point,
        )
    }
}

impl From<&RpcRawHeader> for Header {
    fn from(header: &RpcRawHeader) -> Self {
        Self::new_finalized(
            header.version,
            header.parents_by_level.clone(),
            header.hash_merkle_root,
            header.accepted_id_merkle_root,
            header.cell_commitment,
            header.cell_root,
            header.timestamp,
            header.bits,
            header.nonce,
            header.daa_score,
            header.blue_work,
            header.blue_score,
            header.pruning_point,
        )
    }
}

impl From<&Header> for RpcRawHeader {
    fn from(header: &Header) -> Self {
        Self {
            version: header.version,
            parents_by_level: header.parents_by_level.clone(),
            hash_merkle_root: header.hash_merkle_root,
            accepted_id_merkle_root: header.accepted_id_merkle_root,
            cell_commitment: header.cell_commitment,
            cell_root: header.cell_root,
            timestamp: header.timestamp,
            bits: header.bits,
            nonce: header.nonce,
            daa_score: header.daa_score,
            blue_work: header.blue_work,
            blue_score: header.blue_score,
            pruning_point: header.pruning_point,
        }
    }
}

impl From<Header> for RpcRawHeader {
    fn from(header: Header) -> Self {
        Self {
            version: header.version,
            parents_by_level: header.parents_by_level,
            hash_merkle_root: header.hash_merkle_root,
            accepted_id_merkle_root: header.accepted_id_merkle_root,
            cell_commitment: header.cell_commitment,
            cell_root: header.cell_root,
            timestamp: header.timestamp,
            bits: header.bits,
            nonce: header.nonce,
            daa_score: header.daa_score,
            blue_work: header.blue_work,
            blue_score: header.blue_score,
            pruning_point: header.pruning_point,
        }
    }
}

impl Serializer for RpcRawHeader {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &2, writer)?;

        store!(u16, &self.version, writer)?;
        store!(Vec<Vec<Hash>>, &self.parents_by_level, writer)?;
        store!(Hash, &self.hash_merkle_root, writer)?;
        store!(Hash, &self.accepted_id_merkle_root, writer)?;
        store!(Hash, &self.cell_commitment, writer)?;
        store!(Hash, &self.cell_root, writer)?;
        store!(u64, &self.timestamp, writer)?;
        store!(u32, &self.bits, writer)?;
        store!(u64, &self.nonce, writer)?;
        store!(u64, &self.daa_score, writer)?;
        store!(BlueWorkType, &self.blue_work, writer)?;
        store!(u64, &self.blue_score, writer)?;
        store!(Hash, &self.pruning_point, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcRawHeader {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let payload_version = load!(u16, reader)?;
        if payload_version != 2 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unsupported RpcRawHeader version {payload_version}, expected 2"),
            ));
        }

        let version = load!(u16, reader)?;
        let parents_by_level = load!(Vec<Vec<Hash>>, reader)?;
        let hash_merkle_root = load!(Hash, reader)?;
        let accepted_id_merkle_root = load!(Hash, reader)?;
        let cell_commitment = load!(Hash, reader)?;
        let cell_root = load!(Hash, reader)?;
        let timestamp = load!(u64, reader)?;
        let bits = load!(u32, reader)?;
        let nonce = load!(u64, reader)?;
        let daa_score = load!(u64, reader)?;
        let blue_work = load!(BlueWorkType, reader)?;
        let blue_score = load!(u64, reader)?;
        let pruning_point = load!(Hash, reader)?;

        Ok(Self {
            version,
            parents_by_level,
            hash_merkle_root,
            accepted_id_merkle_root,
            cell_commitment,
            cell_root,
            timestamp,
            bits,
            nonce,
            daa_score,
            blue_work,
            blue_score,
            pruning_point,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use workflow_serializer::prelude::{Deserializer, Serializer};

    fn hash_from_byte(byte: u8) -> Hash {
        Hash::from_bytes([byte; 32])
    }

    #[test]
    fn test_rpc_header_roundtrip_preserves_cell_root() {
        let header = RpcHeader {
            hash: hash_from_byte(1),
            version: 2,
            parents_by_level: vec![vec![hash_from_byte(2)]],
            hash_merkle_root: hash_from_byte(3),
            accepted_id_merkle_root: hash_from_byte(4),
            cell_commitment: hash_from_byte(5),
            cell_root: hash_from_byte(6),
            timestamp: 7,
            bits: 8,
            nonce: 9,
            daa_score: 10,
            blue_work: 11.into(),
            blue_score: 12,
            pruning_point: hash_from_byte(13),
        };

        let mut buffer = Vec::new();
        Serializer::serialize(&header, &mut buffer).unwrap();
        let decoded = <RpcHeader as Deserializer>::deserialize(&mut Cursor::new(&buffer)).unwrap();

        assert_eq!(decoded.cell_root, header.cell_root);
        assert_eq!(decoded.hash, header.hash);
        assert_eq!(decoded.cell_commitment, header.cell_commitment);
        assert_eq!(decoded.pruning_point, header.pruning_point);
    }

    #[test]
    fn test_rpc_header_rejects_v1_payload() {
        let mut buffer = Vec::new();
        store!(u16, &1, &mut buffer).unwrap();
        store!(Hash, &hash_from_byte(1), &mut buffer).unwrap();
        store!(u16, &2, &mut buffer).unwrap();
        store!(Vec<Vec<Hash>>, &vec![vec![hash_from_byte(2)]], &mut buffer).unwrap();
        store!(Hash, &hash_from_byte(3), &mut buffer).unwrap();
        store!(Hash, &hash_from_byte(4), &mut buffer).unwrap();
        store!(Hash, &hash_from_byte(5), &mut buffer).unwrap();
        store!(u64, &7, &mut buffer).unwrap();
        store!(u32, &8, &mut buffer).unwrap();
        store!(u64, &9, &mut buffer).unwrap();
        store!(u64, &10, &mut buffer).unwrap();
        store!(BlueWorkType, &BlueWorkType::from(11u64), &mut buffer).unwrap();
        store!(u64, &12, &mut buffer).unwrap();
        store!(Hash, &hash_from_byte(13), &mut buffer).unwrap();

        let err = <RpcHeader as Deserializer>::deserialize(&mut Cursor::new(&buffer)).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }
}
