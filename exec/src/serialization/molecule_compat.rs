// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// VM Molecule ABI compatibility layer.

//! VM-visible Molecule ABI support.
//!
//! This module implements the canonical Molecule wire layout for Spora's
//! CKB-style VM-facing structures without changing the default Borsh v1 ABI.
//! The default syscall path can continue serving legacy scripts while ABI
//! negotiation exposes `0x8001` as an available Molecule format.

use crate::celltx::{CellInput, CellOutput, OutPoint, Script};
use crate::serialization::{SerializationError, VmAbiError};
use crate::vm::{ResolvedCell, ResolvedHeader};

const NUMBER_SIZE: usize = 4;

/// Molecule serializer for VM-facing data.
pub struct MoleculeSerializer;

impl MoleculeSerializer {
    /// Molecule VM ABI support is available in this build.
    pub const fn is_available() -> bool {
        true
    }

    /// Molecule-based VM ABI v1.
    pub const fn abi_version() -> u16 {
        0x8001
    }
}

/// Molecule serialization errors.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum MoleculeError {
    /// Input bytes do not match the expected Molecule layout.
    #[error("invalid molecule bytes for {ty}: {reason}")]
    InvalidFormat {
        /// Type name being decoded.
        ty: &'static str,
        /// Concrete validation failure.
        reason: String,
    },
    /// Schema does not match the expected Spora VM ABI schema.
    #[error("schema mismatch: {0}")]
    SchemaMismatch(String),
    /// Validation failed after decoding.
    #[error("validation failed: {0}")]
    ValidationFailed(String),
}

impl From<MoleculeError> for SerializationError {
    fn from(e: MoleculeError) -> Self {
        SerializationError::DeserializationFailed(e.to_string())
    }
}

impl From<MoleculeError> for VmAbiError {
    fn from(e: MoleculeError) -> Self {
        VmAbiError::DeserializationFailed(e.to_string())
    }
}

/// Serialize `ResolvedHeader` as a Spora Molecule table.
pub fn serialize_resolved_header_molecule(header: &ResolvedHeader) -> Result<Vec<u8>, MoleculeError> {
    Ok(encode_table(&[
        header.hash.to_vec(),
        encode_u32(header.version),
        encode_parents_by_level(&header.parents_by_level),
        header.hash_merkle_root.to_vec(),
        header.accepted_id_merkle_root.to_vec(),
        header.cell_commitment.to_vec(),
        header.cell_root.to_vec(),
        header.segment_root.to_vec(),
        encode_u64(header.timestamp),
        encode_u32(header.bits),
        encode_u64(header.nonce),
        encode_u64(header.daa_score),
        header.blue_work.to_vec(),
        encode_u64(header.blue_score),
        header.pruning_point.to_vec(),
    ]))
}

/// Deserialize `ResolvedHeader` from the Spora Molecule table layout.
pub fn deserialize_resolved_header_molecule(bytes: &[u8]) -> Result<ResolvedHeader, MoleculeError> {
    let fields = decode_table(bytes, 15, "ResolvedHeader")?;
    Ok(ResolvedHeader {
        hash: decode_array_32(fields[0], "ResolvedHeader.hash")?,
        version: decode_u32(fields[1], "ResolvedHeader.version")?,
        parents_by_level: decode_parents_by_level(fields[2])?,
        hash_merkle_root: decode_array_32(fields[3], "ResolvedHeader.hash_merkle_root")?,
        accepted_id_merkle_root: decode_array_32(fields[4], "ResolvedHeader.accepted_id_merkle_root")?,
        cell_commitment: decode_array_32(fields[5], "ResolvedHeader.cell_commitment")?,
        cell_root: decode_array_32(fields[6], "ResolvedHeader.cell_root")?,
        segment_root: decode_array_32(fields[7], "ResolvedHeader.segment_root")?,
        timestamp: decode_u64(fields[8], "ResolvedHeader.timestamp")?,
        bits: decode_u32(fields[9], "ResolvedHeader.bits")?,
        nonce: decode_u64(fields[10], "ResolvedHeader.nonce")?,
        daa_score: decode_u64(fields[11], "ResolvedHeader.daa_score")?,
        blue_work: decode_array_24(fields[12], "ResolvedHeader.blue_work")?,
        blue_score: decode_u64(fields[13], "ResolvedHeader.blue_score")?,
        pruning_point: decode_array_32(fields[14], "ResolvedHeader.pruning_point")?,
    })
}

/// Serialize `ResolvedCell` as a Spora Molecule table.
pub fn serialize_resolved_cell_molecule(cell: &ResolvedCell) -> Result<Vec<u8>, MoleculeError> {
    Ok(encode_table(&[serialize_cell_output_molecule(&cell.cell_output)?, encode_bytes_opt(cell.data.as_deref())]))
}

/// Deserialize `ResolvedCell` from the Spora Molecule table layout.
pub fn deserialize_resolved_cell_molecule(bytes: &[u8]) -> Result<ResolvedCell, MoleculeError> {
    let fields = decode_table(bytes, 2, "ResolvedCell")?;
    Ok(ResolvedCell { cell_output: deserialize_cell_output_molecule(fields[0])?, data: decode_bytes_opt(fields[1])? })
}

/// Serialize `Script` using the CKB Molecule `Script` layout.
pub fn serialize_script_molecule(script: &Script) -> Result<Vec<u8>, MoleculeError> {
    Ok(encode_table(&[script.code_hash.to_vec(), vec![script.hash_type], encode_bytes(&script.args)]))
}

/// Deserialize `Script` from the CKB Molecule `Script` layout.
pub fn deserialize_script_molecule(bytes: &[u8]) -> Result<Script, MoleculeError> {
    let fields = decode_table(bytes, 3, "Script")?;
    Ok(Script {
        code_hash: decode_array_32(fields[0], "Script.code_hash")?,
        hash_type: decode_byte(fields[1], "Script.hash_type")?,
        args: decode_bytes(fields[2])?,
    })
}

/// Serialize `OutPoint` using the CKB Molecule `OutPoint` struct layout.
pub fn serialize_outpoint_molecule(outpoint: &OutPoint) -> Result<Vec<u8>, MoleculeError> {
    let mut out = Vec::with_capacity(36);
    out.extend_from_slice(&outpoint.tx_hash);
    out.extend_from_slice(&outpoint.index.to_le_bytes());
    Ok(out)
}

/// Deserialize `OutPoint` from the CKB Molecule `OutPoint` struct layout.
pub fn deserialize_outpoint_molecule(bytes: &[u8]) -> Result<OutPoint, MoleculeError> {
    if bytes.len() != 36 {
        return invalid("OutPoint", format!("expected 36 bytes, got {}", bytes.len()));
    }
    Ok(OutPoint { tx_hash: decode_array_32(&bytes[..32], "OutPoint.tx_hash")?, index: decode_u32(&bytes[32..36], "OutPoint.index")? })
}

/// Serialize `CellInput` using the CKB Molecule `CellInput` struct layout.
pub fn serialize_cell_input_molecule(input: &CellInput) -> Result<Vec<u8>, MoleculeError> {
    let mut out = Vec::with_capacity(44);
    out.extend_from_slice(&input.since.to_le_bytes());
    out.extend_from_slice(&serialize_outpoint_molecule(&input.previous_output)?);
    Ok(out)
}

/// Deserialize `CellInput` from the CKB Molecule `CellInput` struct layout.
pub fn deserialize_cell_input_molecule(bytes: &[u8]) -> Result<CellInput, MoleculeError> {
    if bytes.len() != 44 {
        return invalid("CellInput", format!("expected 44 bytes, got {}", bytes.len()));
    }
    Ok(CellInput {
        since: decode_u64(&bytes[..8], "CellInput.since")?,
        previous_output: deserialize_outpoint_molecule(&bytes[8..44])?,
    })
}

/// Serialize `CellOutput` using the CKB Molecule `CellOutput` table layout.
pub fn serialize_cell_output_molecule(output: &CellOutput) -> Result<Vec<u8>, MoleculeError> {
    Ok(encode_table(&[
        encode_u64(output.capacity),
        serialize_script_molecule(&output.lock)?,
        encode_script_opt(output.type_.as_ref())?,
    ]))
}

/// Deserialize `CellOutput` from the CKB Molecule `CellOutput` table layout.
pub fn deserialize_cell_output_molecule(bytes: &[u8]) -> Result<CellOutput, MoleculeError> {
    let fields = decode_table(bytes, 3, "CellOutput")?;
    Ok(CellOutput {
        capacity: decode_u64(fields[0], "CellOutput.capacity")?,
        lock: deserialize_script_molecule(fields[1])?,
        type_: decode_script_opt(fields[2])?,
    })
}

fn encode_u32(value: u32) -> Vec<u8> {
    value.to_le_bytes().to_vec()
}

fn encode_u64(value: u64) -> Vec<u8> {
    value.to_le_bytes().to_vec()
}

fn pack_number(value: usize) -> [u8; NUMBER_SIZE] {
    (value as u32).to_le_bytes()
}

fn unpack_number(bytes: &[u8], ty: &'static str) -> Result<usize, MoleculeError> {
    if bytes.len() < NUMBER_SIZE {
        return invalid(ty, format!("expected at least 4 bytes for number, got {}", bytes.len()));
    }
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize)
}

fn encode_table(fields: &[Vec<u8>]) -> Vec<u8> {
    let header_size = NUMBER_SIZE * (fields.len() + 1);
    let total_size = header_size + fields.iter().map(Vec::len).sum::<usize>();
    let mut out = Vec::with_capacity(total_size);
    out.extend_from_slice(&pack_number(total_size));

    let mut offset = header_size;
    for field in fields {
        out.extend_from_slice(&pack_number(offset));
        offset += field.len();
    }
    for field in fields {
        out.extend_from_slice(field);
    }
    out
}

fn decode_table<'a>(bytes: &'a [u8], expected_fields: usize, ty: &'static str) -> Result<Vec<&'a [u8]>, MoleculeError> {
    if bytes.len() < NUMBER_SIZE * 2 {
        return invalid(ty, format!("table header is too short: {}", bytes.len()));
    }
    let total_size = unpack_number(bytes, ty)?;
    if total_size != bytes.len() {
        return invalid(ty, format!("total size mismatch: header {total_size}, actual {}", bytes.len()));
    }

    let first_offset = unpack_number(&bytes[NUMBER_SIZE..], ty)?;
    if first_offset % NUMBER_SIZE != 0 || first_offset < NUMBER_SIZE * 2 || first_offset > bytes.len() {
        return invalid(ty, format!("invalid first field offset {first_offset}"));
    }

    let field_count = first_offset / NUMBER_SIZE - 1;
    if field_count != expected_fields {
        return Err(MoleculeError::SchemaMismatch(format!("{ty}: expected {expected_fields} fields, got {field_count}")));
    }

    let mut offsets = Vec::with_capacity(field_count + 1);
    for chunk in bytes[NUMBER_SIZE..first_offset].chunks_exact(NUMBER_SIZE) {
        offsets.push(unpack_number(chunk, ty)?);
    }
    offsets.push(total_size);

    if offsets.windows(2).any(|pair| pair[0] > pair[1]) {
        return invalid(ty, "field offsets are not monotonic");
    }
    if offsets.iter().any(|offset| *offset < first_offset || *offset > total_size) {
        return invalid(ty, "field offset is outside table payload");
    }

    Ok(offsets.windows(2).map(|pair| &bytes[pair[0]..pair[1]]).collect())
}

fn encode_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(NUMBER_SIZE + bytes.len());
    out.extend_from_slice(&pack_number(bytes.len()));
    out.extend_from_slice(bytes);
    out
}

fn decode_bytes(bytes: &[u8]) -> Result<Vec<u8>, MoleculeError> {
    let len = unpack_number(bytes, "Bytes")?;
    let expected = NUMBER_SIZE + len;
    if bytes.len() != expected {
        return invalid("Bytes", format!("expected {expected} bytes, got {}", bytes.len()));
    }
    Ok(bytes[NUMBER_SIZE..].to_vec())
}

fn encode_bytes_opt(bytes: Option<&[u8]>) -> Vec<u8> {
    bytes.map(encode_bytes).unwrap_or_default()
}

fn decode_bytes_opt(bytes: &[u8]) -> Result<Option<Vec<u8>>, MoleculeError> {
    if bytes.is_empty() {
        Ok(None)
    } else {
        decode_bytes(bytes).map(Some)
    }
}

fn encode_script_opt(script: Option<&Script>) -> Result<Vec<u8>, MoleculeError> {
    script.map(serialize_script_molecule).transpose().map(Option::unwrap_or_default)
}

fn decode_script_opt(bytes: &[u8]) -> Result<Option<Script>, MoleculeError> {
    if bytes.is_empty() {
        Ok(None)
    } else {
        deserialize_script_molecule(bytes).map(Some)
    }
}

fn encode_fixvec_byte32(values: &[[u8; 32]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(NUMBER_SIZE + values.len() * 32);
    out.extend_from_slice(&pack_number(values.len()));
    for value in values {
        out.extend_from_slice(value);
    }
    out
}

fn decode_fixvec_byte32(bytes: &[u8], ty: &'static str) -> Result<Vec<[u8; 32]>, MoleculeError> {
    let count = unpack_number(bytes, ty)?;
    let expected = NUMBER_SIZE + count * 32;
    if bytes.len() != expected {
        return invalid(ty, format!("expected {expected} bytes, got {}", bytes.len()));
    }

    bytes[NUMBER_SIZE..].chunks_exact(32).map(|chunk| decode_array_32(chunk, ty)).collect()
}

fn encode_dynvec(items: &[Vec<u8>]) -> Vec<u8> {
    if items.is_empty() {
        return pack_number(NUMBER_SIZE).to_vec();
    }
    encode_table(items)
}

fn decode_dynvec<'a>(bytes: &'a [u8], ty: &'static str) -> Result<Vec<&'a [u8]>, MoleculeError> {
    if bytes.len() < NUMBER_SIZE {
        return invalid(ty, format!("dynvec header is too short: {}", bytes.len()));
    }
    let total_size = unpack_number(bytes, ty)?;
    if total_size != bytes.len() {
        return invalid(ty, format!("total size mismatch: header {total_size}, actual {}", bytes.len()));
    }
    if total_size == NUMBER_SIZE {
        return Ok(Vec::new());
    }
    if bytes.len() < NUMBER_SIZE * 2 {
        return invalid(ty, "non-empty dynvec missing first offset");
    }

    let first_offset = unpack_number(&bytes[NUMBER_SIZE..], ty)?;
    if first_offset % NUMBER_SIZE != 0 || first_offset < NUMBER_SIZE * 2 || first_offset > bytes.len() {
        return invalid(ty, format!("invalid first item offset {first_offset}"));
    }

    let item_count = first_offset / NUMBER_SIZE - 1;
    let mut offsets = Vec::with_capacity(item_count + 1);
    for chunk in bytes[NUMBER_SIZE..first_offset].chunks_exact(NUMBER_SIZE) {
        offsets.push(unpack_number(chunk, ty)?);
    }
    offsets.push(total_size);

    if offsets.windows(2).any(|pair| pair[0] > pair[1]) {
        return invalid(ty, "item offsets are not monotonic");
    }
    if offsets.iter().any(|offset| *offset < first_offset || *offset > total_size) {
        return invalid(ty, "item offset is outside dynvec payload");
    }

    Ok(offsets.windows(2).map(|pair| &bytes[pair[0]..pair[1]]).collect())
}

fn encode_parents_by_level(levels: &[Vec<[u8; 32]>]) -> Vec<u8> {
    let encoded_levels = levels.iter().map(|level| encode_fixvec_byte32(level)).collect::<Vec<_>>();
    encode_dynvec(&encoded_levels)
}

fn decode_parents_by_level(bytes: &[u8]) -> Result<Vec<Vec<[u8; 32]>>, MoleculeError> {
    decode_dynvec(bytes, "ParentsByLevel")?.into_iter().map(|level| decode_fixvec_byte32(level, "ParentLevel")).collect()
}

fn decode_u32(bytes: &[u8], ty: &'static str) -> Result<u32, MoleculeError> {
    if bytes.len() != 4 {
        return invalid(ty, format!("expected 4 bytes, got {}", bytes.len()));
    }
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn decode_u64(bytes: &[u8], ty: &'static str) -> Result<u64, MoleculeError> {
    if bytes.len() != 8 {
        return invalid(ty, format!("expected 8 bytes, got {}", bytes.len()));
    }
    Ok(u64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]))
}

fn decode_byte(bytes: &[u8], ty: &'static str) -> Result<u8, MoleculeError> {
    if bytes.len() != 1 {
        return invalid(ty, format!("expected 1 byte, got {}", bytes.len()));
    }
    Ok(bytes[0])
}

fn decode_array_32(bytes: &[u8], ty: &'static str) -> Result<[u8; 32], MoleculeError> {
    if bytes.len() != 32 {
        return invalid(ty, format!("expected 32 bytes, got {}", bytes.len()));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(bytes);
    Ok(out)
}

fn decode_array_24(bytes: &[u8], ty: &'static str) -> Result<[u8; 24], MoleculeError> {
    if bytes.len() != 24 {
        return invalid(ty, format!("expected 24 bytes, got {}", bytes.len()));
    }
    let mut out = [0u8; 24];
    out.copy_from_slice(bytes);
    Ok(out)
}

fn invalid<T>(ty: &'static str, reason: impl Into<String>) -> Result<T, MoleculeError> {
    Err(MoleculeError::InvalidFormat { ty, reason: reason.into() })
}

/// Schema constants for the handwritten Molecule layouts.
pub mod schema {
    /// ResolvedHeader schema version.
    pub const RESOLVED_HEADER_SCHEMA_VERSION: u8 = 1;

    /// ResolvedCell schema version.
    pub const RESOLVED_CELL_SCHEMA_VERSION: u8 = 1;

    /// Number of fields in the Spora Molecule `ResolvedHeader` table.
    pub const RESOLVED_HEADER_FIELD_COUNT: usize = 15;

    /// Number of fields in the Spora Molecule `ResolvedCell` table.
    pub const RESOLVED_CELL_FIELD_COUNT: usize = 2;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_script(tag: u8, args: Vec<u8>) -> Script {
        Script::new([tag; 32], tag, args)
    }

    fn sample_header() -> ResolvedHeader {
        ResolvedHeader {
            hash: [1; 32],
            version: 2,
            parents_by_level: vec![vec![[3; 32], [4; 32]], vec![[5; 32]]],
            hash_merkle_root: [6; 32],
            accepted_id_merkle_root: [7; 32],
            cell_commitment: [8; 32],
            cell_root: [9; 32],
            segment_root: [10; 32],
            timestamp: 11,
            bits: 12,
            nonce: 13,
            daa_score: 14,
            blue_work: [15; 24],
            blue_score: 16,
            pruning_point: [17; 32],
        }
    }

    #[test]
    fn molecule_is_available() {
        assert!(MoleculeSerializer::is_available());
        assert_eq!(MoleculeSerializer::abi_version(), 0x8001);
    }

    #[test]
    fn script_encoding_matches_ckb_molecule_default_layout() {
        let script = Script::new([0; 32], 0, vec![]);
        let bytes = serialize_script_molecule(&script).unwrap();

        assert_eq!(
            bytes,
            vec![
                53, 0, 0, 0, 16, 0, 0, 0, 48, 0, 0, 0, 49, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ]
        );
        assert_eq!(deserialize_script_molecule(&bytes).unwrap(), script);
    }

    #[test]
    fn ckb_style_core_types_roundtrip() {
        let outpoint = OutPoint::new([0xAA; 32], 7);
        let input = CellInput::new(outpoint, 0x1122_3344_5566_7788);
        let lock = sample_script(1, vec![1, 2, 3]);
        let type_ = sample_script(2, vec![4, 5]);
        let output = CellOutput { lock, type_: Some(type_), capacity: 1000 };

        assert_eq!(deserialize_outpoint_molecule(&serialize_outpoint_molecule(&outpoint).unwrap()).unwrap(), outpoint);
        assert_eq!(deserialize_cell_input_molecule(&serialize_cell_input_molecule(&input).unwrap()).unwrap(), input);
        assert_eq!(deserialize_cell_output_molecule(&serialize_cell_output_molecule(&output).unwrap()).unwrap(), output);
    }

    #[test]
    fn resolved_cell_roundtrip_preserves_optional_data() {
        let cell = ResolvedCell {
            cell_output: CellOutput { lock: sample_script(3, vec![8, 9]), type_: None, capacity: 42 },
            data: Some(vec![0xAB, 0xCD]),
        };
        let bytes = serialize_resolved_cell_molecule(&cell).unwrap();
        assert_eq!(deserialize_resolved_cell_molecule(&bytes).unwrap(), cell);
    }

    #[test]
    fn resolved_header_roundtrip_preserves_dag_parent_levels() {
        let header = sample_header();
        let bytes = serialize_resolved_header_molecule(&header).unwrap();
        assert_eq!(deserialize_resolved_header_molecule(&bytes).unwrap(), header);
    }

    #[test]
    fn rejects_malformed_table_size() {
        let mut bytes = serialize_script_molecule(&sample_script(1, vec![])).unwrap();
        bytes[0] = bytes[0].wrapping_add(1);
        let err = deserialize_script_molecule(&bytes).unwrap_err();
        assert!(matches!(err, MoleculeError::InvalidFormat { .. }));
    }
}
