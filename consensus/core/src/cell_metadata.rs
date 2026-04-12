// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Complete Cell metadata for validation and querying (GHOSTDAG-aware)

use crate::{
    cell_diff::CellMeta,
    tx::{CellEntry, ScriptPublicKey, ScriptRef},
};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use spora_hashes::Hash;

const CELL_METADATA_PLACEHOLDER_PREFIX: [u8; 2] = [0xCE, 0x11];
const CELL_METADATA_PLACEHOLDER_V1_TAG: u8 = 0x01;
const LEGACY_PLACEHOLDER_SCRIPT_LEN: usize = CELL_METADATA_PLACEHOLDER_PREFIX.len() + 32;
const V1_PLACEHOLDER_SCRIPT_LEN: usize = CELL_METADATA_PLACEHOLDER_PREFIX.len() + 1 + 32 + 1 + 32 + 32 + 8;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct PlaceholderCellMetadata {
    pub lock_hash: [u8; 32],
    pub type_hash: Option<[u8; 32]>,
    pub data_hash: [u8; 32],
    pub data_bytes: u64,
}

/// Complete Cell metadata (用于验证器和查询)
///
/// This extends the lightweight `CellMeta` with additional information
/// needed for validation, VM execution, and queries.
///
/// Design rationale:
/// - `CellMeta` (in cell_diff.rs): Lightweight, used in diffs and merkle tree  
/// - `CellMetadata`: Complete, used in validators and indexers
///
/// **CKB Compatibility**: Now includes all CellMeta fields plus DAG extensions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellMetadata {
    // Base information (from CellMeta) - now includes all fields
    /// OutPoint identifying this cell
    pub out_point: crate::tx::TransactionOutpoint,
    /// Cell capacity in saus
    pub capacity: u64,
    /// Data length in bytes
    pub data_bytes: u64,
    /// Lock script hash
    pub lock_hash: [u8; 32],
    /// Type script hash (if present)
    pub type_hash: Option<[u8; 32]>,
    /// Data hash
    pub data_hash: [u8; 32],
    /// Block DAA score where this cell was created
    pub block_daa_score: u64,

    // DAG-specific extensions
    /// Whether this Cell is from a cellbase transaction
    pub is_cellbase: bool,
    /// Block hash containing this Cell
    pub block_hash: Hash,

    // Optional: full data for VM execution (loaded on-demand)
    /// Lock script code hash (for VM execution)
    pub lock_code_hash: Option<[u8; 32]>,
    /// Type script code hash (for VM execution)
    pub type_code_hash: Option<[u8; 32]>,
    /// Full lock script when available from the current data source.
    pub lock_script: Option<ScriptRef>,
    /// Full type script when available from the current data source.
    pub type_script: Option<ScriptRef>,
    /// Cell data (loaded on-demand, can be large)
    pub data: Option<Vec<u8>>,
}

impl CellMetadata {
    /// Create metadata from CellMeta with DAG information
    pub fn from_cell_meta(meta: &CellMeta, is_cellbase: bool, block_hash: Hash) -> Self {
        Self {
            out_point: meta.out_point.clone(),
            capacity: meta.capacity,
            data_bytes: meta.data_bytes,
            lock_hash: meta.lock_hash,
            type_hash: meta.type_hash,
            data_hash: meta.data_hash,
            block_daa_score: meta.block_daa_score,
            is_cellbase: is_cellbase || meta.is_cellbase,
            block_hash,
            lock_code_hash: None,
            type_code_hash: None,
            lock_script: None,
            type_script: None,
            data: None,
        }
    }

    /// Convert to lightweight CellMeta (for diffs)
    pub fn to_cell_meta(&self) -> CellMeta {
        CellMeta {
            out_point: self.out_point.clone(),
            capacity: self.capacity,
            data_bytes: self.data_bytes,
            lock_hash: self.lock_hash,
            type_hash: self.type_hash,
            data_hash: self.data_hash,
            block_daa_score: self.block_daa_score,
            is_cellbase: self.is_cellbase,
        }
    }

    /// Create with full data (for VM execution)
    pub fn with_data(mut self, data: Vec<u8>) -> Self {
        self.data = Some(data);
        self
    }

    /// Create with script code hashes
    pub fn with_scripts(mut self, lock_code_hash: [u8; 32], type_code_hash: Option<[u8; 32]>) -> Self {
        self.lock_code_hash = Some(lock_code_hash);
        self.type_code_hash = type_code_hash;
        self
    }

    /// Build a synthetic Cell entry for legacy mempool bridges.
    ///
    /// During the Cell migration, mempool paths still expect `CellEntry`-shaped
    /// inputs. We encode a deterministic placeholder script carrying complete cell
    /// metadata so those paths can distinguish Cell-backed entries from real legacy
    /// scripts without throwing away lock/type/data semantics.
    pub fn to_placeholder_cell_entry(&self) -> CellEntry {
        CellEntry::from_cell_metadata(
            self.capacity,
            self.data_bytes,
            self.lock_hash,
            self.type_hash,
            self.data_hash,
            self.block_daa_score,
            self.is_cellbase,
        )
    }
}

pub fn cell_metadata_placeholder_script_public_key(lock_hash: [u8; 32]) -> ScriptPublicKey {
    let mut script = Vec::with_capacity(CELL_METADATA_PLACEHOLDER_PREFIX.len() + lock_hash.len());
    script.extend_from_slice(&CELL_METADATA_PLACEHOLDER_PREFIX);
    script.extend_from_slice(&lock_hash);
    ScriptPublicKey::from_vec(0, script)
}

pub fn cell_metadata_placeholder_script_public_key_with_metadata(
    lock_hash: [u8; 32],
    type_hash: Option<[u8; 32]>,
    data_hash: [u8; 32],
    data_bytes: u64,
) -> ScriptPublicKey {
    let mut script = Vec::with_capacity(V1_PLACEHOLDER_SCRIPT_LEN);
    script.extend_from_slice(&CELL_METADATA_PLACEHOLDER_PREFIX);
    script.push(CELL_METADATA_PLACEHOLDER_V1_TAG);
    script.extend_from_slice(&lock_hash);
    script.push(u8::from(type_hash.is_some()));
    script.extend_from_slice(&type_hash.unwrap_or([0; 32]));
    script.extend_from_slice(&data_hash);
    script.extend_from_slice(&data_bytes.to_le_bytes());
    ScriptPublicKey::from_vec(0, script)
}

pub fn parse_cell_metadata_placeholder_script_public_key(script_public_key: &ScriptPublicKey) -> Option<PlaceholderCellMetadata> {
    let script = script_public_key.script();
    if script_public_key.version() != 0 || !script.starts_with(&CELL_METADATA_PLACEHOLDER_PREFIX) {
        return None;
    }

    if script.len() == LEGACY_PLACEHOLDER_SCRIPT_LEN {
        let mut lock_hash = [0u8; 32];
        lock_hash.copy_from_slice(&script[CELL_METADATA_PLACEHOLDER_PREFIX.len()..]);
        return Some(PlaceholderCellMetadata { lock_hash, type_hash: None, data_hash: [0; 32], data_bytes: 0 });
    }

    if script.len() != V1_PLACEHOLDER_SCRIPT_LEN || script[CELL_METADATA_PLACEHOLDER_PREFIX.len()] != CELL_METADATA_PLACEHOLDER_V1_TAG
    {
        return None;
    }

    let mut offset = CELL_METADATA_PLACEHOLDER_PREFIX.len() + 1;

    let mut lock_hash = [0u8; 32];
    lock_hash.copy_from_slice(&script[offset..offset + 32]);
    offset += 32;

    let type_present = match script[offset] {
        0 => false,
        1 => true,
        _ => return None,
    };
    offset += 1;

    let mut encoded_type_hash = [0u8; 32];
    encoded_type_hash.copy_from_slice(&script[offset..offset + 32]);
    let type_hash = type_present.then_some(encoded_type_hash);
    offset += 32;

    let mut data_hash = [0u8; 32];
    data_hash.copy_from_slice(&script[offset..offset + 32]);
    offset += 32;

    let mut data_bytes_buf = [0u8; 8];
    data_bytes_buf.copy_from_slice(&script[offset..offset + 8]);
    let data_bytes = u64::from_le_bytes(data_bytes_buf);

    Some(PlaceholderCellMetadata { lock_hash, type_hash, data_hash, data_bytes })
}

pub fn is_cell_metadata_placeholder_script_public_key(script_public_key: &ScriptPublicKey) -> bool {
    parse_cell_metadata_placeholder_script_public_key(script_public_key).is_some()
}

impl From<&CellMeta> for CellMetadata {
    fn from(meta: &CellMeta) -> Self {
        Self {
            out_point: meta.out_point.clone(),
            capacity: meta.capacity,
            data_bytes: meta.data_bytes,
            lock_hash: meta.lock_hash,
            type_hash: meta.type_hash,
            data_hash: meta.data_hash,
            block_daa_score: meta.block_daa_score,
            is_cellbase: meta.is_cellbase,
            block_hash: Hash::default(),
            lock_code_hash: None,
            type_code_hash: None,
            lock_script: None,
            type_script: None,
            data: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cell_metadata_creation() {
        use crate::tx::TransactionOutpoint;

        let meta = CellMeta {
            out_point: TransactionOutpoint::new([0u8; 32], 0),
            capacity: 100_000,
            data_bytes: 100,
            lock_hash: [1u8; 32],
            type_hash: Some([2u8; 32]),
            data_hash: [3u8; 32],
            block_daa_score: 1000,
            is_cellbase: true,
        };

        let metadata = CellMetadata::from_cell_meta(&meta, true, Hash::from_bytes([4u8; 32]));

        assert_eq!(metadata.capacity, 100_000);
        assert_eq!(metadata.data_bytes, 100);
        assert_eq!(metadata.lock_hash, [1u8; 32]);
        assert_eq!(metadata.is_cellbase, true);
        assert_eq!(metadata.block_hash, Hash::from_bytes([4u8; 32]));
    }

    #[test]
    fn test_roundtrip_conversion() {
        use crate::tx::TransactionOutpoint;

        let meta = CellMeta {
            out_point: TransactionOutpoint::new([5u8; 32], 1),
            capacity: 50_000,
            data_bytes: 50,
            lock_hash: [5u8; 32],
            type_hash: None,
            data_hash: [6u8; 32],
            block_daa_score: 2000,
            is_cellbase: false,
        };

        let metadata = CellMetadata::from(&meta);
        let recovered = metadata.to_cell_meta();

        assert_eq!(recovered.capacity, meta.capacity);
        assert_eq!(recovered.data_bytes, meta.data_bytes);
        assert_eq!(recovered.lock_hash, meta.lock_hash);
        assert_eq!(recovered.data_hash, meta.data_hash);
        assert_eq!(recovered.out_point, meta.out_point);
    }

    #[test]
    fn test_with_data() {
        use crate::tx::TransactionOutpoint;

        let meta = CellMeta {
            out_point: TransactionOutpoint::new([1u8; 32], 0),
            capacity: 100_000,
            data_bytes: 4,
            lock_hash: [1u8; 32],
            type_hash: None,
            data_hash: [3u8; 32],
            block_daa_score: 1000,
            is_cellbase: false,
        };

        let metadata = CellMetadata::from(&meta).with_data(vec![0xde, 0xad, 0xbe, 0xef]);

        assert_eq!(metadata.data, Some(vec![0xde, 0xad, 0xbe, 0xef]));
        assert_eq!(metadata.data_bytes, 4);
    }

    #[test]
    fn test_placeholder_script_roundtrip() {
        let lock_hash = [0x42; 32];
        let data_hash = [0x24; 32];
        let type_hash = Some([0x11; 32]);
        let script_public_key = cell_metadata_placeholder_script_public_key_with_metadata(lock_hash, type_hash, data_hash, 512);

        assert!(is_cell_metadata_placeholder_script_public_key(&script_public_key));
        let parsed = parse_cell_metadata_placeholder_script_public_key(&script_public_key).unwrap();
        assert_eq!(parsed.lock_hash, lock_hash);
        assert_eq!(parsed.type_hash, type_hash);
        assert_eq!(parsed.data_hash, data_hash);
        assert_eq!(parsed.data_bytes, 512);
    }

    #[test]
    fn test_legacy_placeholder_script_roundtrip() {
        let lock_hash = [0x33; 32];
        let script_public_key = cell_metadata_placeholder_script_public_key(lock_hash);

        let parsed = parse_cell_metadata_placeholder_script_public_key(&script_public_key).unwrap();
        assert_eq!(parsed.lock_hash, lock_hash);
        assert_eq!(parsed.type_hash, None);
        assert_eq!(parsed.data_hash, [0; 32]);
        assert_eq!(parsed.data_bytes, 0);
    }
}
