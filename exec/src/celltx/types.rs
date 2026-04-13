// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Cell transaction core types (CKB-inspired, DAG-adapted)
//
// Reference: ckb/util/types/src/core/cell.rs

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Serde helpers for serializing `[u8; 32]` as a hex string under the key `transactionId`
/// for human-readable formats (JSON), or raw bytes for binary formats (bincode).
mod outpoint_serde {
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(tx_hash: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            let hex: String = tx_hash.iter().map(|b| format!("{:02x}", b)).collect();
            serializer.serialize_str(&hex)
        } else {
            serde::Serialize::serialize(tx_hash, serializer)
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            if s.len() != 64 {
                return Err(serde::de::Error::custom(format!("expected 64 hex chars, got {}", s.len())));
            }
            let mut bytes = [0u8; 32];
            for i in 0..32 {
                bytes[i] = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).map_err(serde::de::Error::custom)?;
            }
            Ok(bytes)
        } else {
            <[u8; 32]>::deserialize(deserializer)
        }
    }
}

/// Cell transaction version: 0xC001
pub const CELL_TX_VERSION: u32 = 0xC001;
/// Additional bytes a live-cell state entry needs beyond the raw output body.
const CELL_ENTRY_OVERHEAD_EXCLUDING_OUTPUT_BODY: u64 = 32 + 4 + 8 + 1;
/// Static transient-mass factor used before block-context VM cycles are known.
///
/// This intentionally mirrors the consensus-side transient-byte policy until the
/// Cell-native mass model is fully centralized.
const TRANSIENT_BYTE_TO_MASS_FACTOR: u64 = 4;
/// Static surcharge applied to execution-facing surfaces before runtime cycles exist.
///
/// This is intentionally conservative: witnesses, deps, output data and type-script
/// arguments all expand the deterministic work surface of a CellTx even before VM
/// execution is measured.
const EXECUTION_SURFACE_BYTE_TO_COMPUTE_FACTOR: u64 = 1;

/// OutPoint: uniquely identifies a Cell (tx_hash || output_index)
///
/// Reference: CKB OutPoint
#[derive(
    Clone, Copy, Default, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub struct OutPoint {
    /// Transaction hash (32 bytes), serialized as hex string `transactionId` in JSON
    #[serde(rename = "transactionId", with = "outpoint_serde")]
    pub tx_hash: [u8; 32],
    /// Output index (u32)
    pub index: u32,
}

impl OutPoint {
    /// Create a new OutPoint
    pub fn new(tx_hash: [u8; 32], index: u32) -> Self {
        Self { tx_hash, index }
    }

    /// Encode to 36-byte key for indexing
    pub fn to_key(&self) -> [u8; 36] {
        let mut key = [0u8; 36];
        key[..32].copy_from_slice(&self.tx_hash);
        key[32..].copy_from_slice(&self.index.to_le_bytes());
        key
    }

    /// Decode from 36-byte key
    pub fn from_key(key: &[u8; 36]) -> Self {
        let mut tx_hash = [0u8; 32];
        tx_hash.copy_from_slice(&key[..32]);
        let index = u32::from_le_bytes([key[32], key[33], key[34], key[35]]);
        Self { tx_hash, index }
    }
}

impl fmt::Display for OutPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.tx_hash {
            write!(f, "{:02x}", byte)?;
        }
        write!(f, ":{}", self.index)
    }
}

/// Script reference (Lock or Type script)
///
/// Reference: CKB Script
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct Script {
    /// Script code hash (points to a Cell's data)
    pub code_hash: [u8; 32],
    /// Hash type: 0=Data, 1=Type, 2=Data1, 4=Data2
    ///
    /// NOTE: Aligned with CKB ScriptHashType encoding:
    /// - Data = 0
    /// - Type = 1  
    /// - Data1 = 2
    /// - Data2 = 4 (NOT 3, to maintain CKB compatibility)
    pub hash_type: u8,
    /// Script arguments (passed to VM)
    pub args: Vec<u8>,
}

impl Script {
    /// Create a new script reference
    pub fn new(code_hash: [u8; 32], hash_type: u8, args: Vec<u8>) -> Self {
        Self { code_hash, hash_type, args }
    }

    /// Calculate script hash (for indexing)
    pub fn hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&self.code_hash);
        hasher.update(&[self.hash_type]);
        hasher.update(&self.args);
        *hasher.finalize().as_bytes()
    }

    /// Serialize the script reference to bytes.
    ///
    /// Format: code_hash (32) || hash_type (1) || args (variable)
    /// This is used by txscript opcodes that inspect output script data.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(33 + self.args.len());
        bytes.extend_from_slice(&self.code_hash);
        bytes.push(self.hash_type);
        bytes.extend_from_slice(&self.args);
        bytes
    }
}

/// Cell output structure
///
/// Note: data field is separated to CellTx.outputs_data (CKB optimization)
///
/// Reference: CKB CellOutput
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct CellOutput {
    /// Lock script: defines who can spend this Cell
    pub lock: Script,
    /// Type script (optional): defines state transition constraints
    pub type_: Option<Script>,
    /// Capacity (saus): amount + storage cost
    pub capacity: u64,
    // ⚠️ NO data field here! Data is in CellTx.outputs_data
}

impl CellOutput {
    /// Calculate occupied capacity (minimum required)
    pub fn occupied_capacity(&self, data_len: usize) -> u64 {
        let mut size = 8; // capacity field
        size += 32 + 1 + self.lock.args.len(); // lock script
        if let Some(ref type_script) = self.type_ {
            size += 32 + 1 + type_script.args.len(); // type script
        }
        size += data_len; // data
        size as u64
    }

    /// Verify capacity is sufficient
    pub fn verify_capacity(&self, data_len: usize) -> Result<(), &'static str> {
        let occupied = self.occupied_capacity(data_len);
        if self.capacity < occupied {
            return Err("Insufficient capacity");
        }
        Ok(())
    }
}

/// Cell input reference
///
/// Reference: CKB CellInput
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct CellInput {
    /// Previous output: which Cell to spend (CKB calls this previous_output)
    pub previous_output: OutPoint,
    /// Since: time lock (relative/absolute, timestamp/DAA)
    /// Bit 63: 0=absolute, 1=relative
    /// Bit 62: 0=timestamp, 1=DAA score
    /// Bit 61-0: lock value
    pub since: u64,
}

impl CellInput {
    /// Create a new cell reference
    pub fn new(previous_output: OutPoint, since: u64) -> Self {
        Self { previous_output, since }
    }

    /// Check if this is a relative time lock
    pub fn is_relative_lock(&self) -> bool {
        (self.since & 0x8000_0000_0000_0000) != 0
    }

    /// Check if this uses DAA score (vs timestamp)
    pub fn is_daa_lock(&self) -> bool {
        (self.since & 0x4000_0000_0000_0000) != 0
    }

    /// Get the lock value
    pub fn lock_value(&self) -> u64 {
        self.since & 0x3FFF_FFFF_FFFF_FFFF
    }
}

/// Cell dependency
///
/// Reference: CKB CellDep
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct CellDep {
    /// OutPoint: which Cell to depend on
    pub out_point: OutPoint,
    /// Dependency type
    pub dep_type: DepType,
}

/// Dependency type
///
/// Reference: CKB DepType
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum DepType {
    /// Code: single Cell as script code
    Code = 0,
    /// DepGroup: a Cell containing multiple OutPoints (batch dependency)
    DepGroup = 1,
}

/// Parse DepGroup cell data as a list of OutPoints.
///
/// Format: 4-byte LE count, then count × 36-byte entries
/// (32-byte tx_hash + 4-byte LE index per OutPoint).
pub fn parse_dep_group_data(data: &[u8]) -> Result<Vec<OutPoint>, String> {
    if data.len() < 4 {
        return Err("DepGroup data too short for count header".into());
    }
    let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let expected = 4 + count * 36;
    if data.len() != expected {
        return Err(format!("DepGroup data length mismatch: expected {} bytes for {} outpoints, got {}", expected, count, data.len()));
    }
    let mut outpoints = Vec::with_capacity(count);
    for i in 0..count {
        let offset = 4 + i * 36;
        let key: &[u8; 36] = data[offset..offset + 36].try_into().map_err(|_| "slice conversion failed")?;
        outpoints.push(OutPoint::from_key(key));
    }
    Ok(outpoints)
}

/// Encode a list of OutPoints into DepGroup cell data format.
///
/// This is the inverse of [`parse_dep_group_data`].
pub fn encode_dep_group_data(outpoints: &[OutPoint]) -> Vec<u8> {
    let mut data = Vec::with_capacity(4 + outpoints.len() * 36);
    data.extend_from_slice(&(outpoints.len() as u32).to_le_bytes());
    for op in outpoints {
        data.extend_from_slice(&op.to_key());
    }
    data
}

/// Cell transaction (complete structure)
///
/// Reference: CKB Transaction
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct CellTx {
    /// Transaction version: 0xC001 (Cell v1)
    pub version: u32,
    /// Inputs: Cells to spend
    pub inputs: Vec<CellInput>,
    /// Cell dependencies: read-only Cells (e.g., script code)
    pub cell_deps: Vec<CellDep>,
    /// Header dependencies available to VM scripts.
    pub header_deps: Vec<[u8; 32]>,
    /// Outputs: new Cells to create
    pub outputs: Vec<CellOutput>,
    /// Output data (1:1 with outputs)
    /// Note: CKB separates outputs and data for verification optimization
    pub outputs_data: Vec<Vec<u8>>,
    /// Witnesses: signatures, multi-sig scripts, etc.
    pub witnesses: Vec<Vec<u8>>,
}

impl CellTx {
    /// Create a new Cell transaction
    pub fn new(
        inputs: Vec<CellInput>,
        cell_deps: Vec<CellDep>,
        outputs: Vec<CellOutput>,
        outputs_data: Vec<Vec<u8>>,
        witnesses: Vec<Vec<u8>>,
    ) -> Result<Self, &'static str> {
        Self::new_with_header_deps(inputs, cell_deps, vec![], outputs, outputs_data, witnesses)
    }

    /// Create a new Cell transaction with explicit header dependencies.
    pub fn new_with_header_deps(
        inputs: Vec<CellInput>,
        cell_deps: Vec<CellDep>,
        header_deps: Vec<[u8; 32]>,
        outputs: Vec<CellOutput>,
        outputs_data: Vec<Vec<u8>>,
        witnesses: Vec<Vec<u8>>,
    ) -> Result<Self, &'static str> {
        if outputs.len() != outputs_data.len() {
            return Err("outputs and outputs_data length mismatch");
        }
        Ok(Self { version: CELL_TX_VERSION, inputs, cell_deps, header_deps, outputs, outputs_data, witnesses })
    }

    /// Get transaction ID (same as compute_txid)
    ///
    /// This is for compatibility with Transaction interface
    pub fn id(&self) -> [u8; 32] {
        crate::celltx::compute_txid(self)
    }

    /// Get transaction version
    ///
    /// This is for compatibility with Transaction interface
    pub fn version(&self) -> u32 {
        self.version
    }

    /// Check if this is a cellbase (coinbase) transaction
    ///
    /// Cellbase transactions have no inputs (mining reward)
    pub fn is_coinbase(&self) -> bool {
        self.inputs.is_empty()
    }

    /// Get the compute-side mass hint of the transaction.
    ///
    /// This is a deterministic pre-VM compute hint composed of serialized size
    /// plus a conservative surcharge for execution-facing surfaces.
    pub fn compute_mass(&self) -> u64 {
        let serialized_size = self.serialized_size() as u64;
        let execution_surface = self.execution_surface_bytes();
        serialized_size.saturating_add(execution_surface.saturating_mul(EXECUTION_SURFACE_BYTE_TO_COMPUTE_FACTOR))
    }

    /// Get the transient-storage mass of the transaction.
    ///
    /// This tracks temporary mempool/relay footprint using a deterministic
    /// serialized-size based factor before contextual execution data exists.
    pub fn transient_mass(&self) -> u64 {
        (self.serialized_size() as u64).saturating_mul(TRANSIENT_BYTE_TO_MASS_FACTOR)
    }

    fn execution_surface_bytes(&self) -> u64 {
        let witness_bytes = self.witnesses.iter().map(|witness| witness.len() as u64).sum::<u64>();
        let dep_bytes = self.cell_deps.len() as u64 * 37;
        let header_dep_bytes = self.header_deps.len() as u64 * 32;
        let output_data_bytes = self.outputs_data.iter().map(|data| data.len() as u64).sum::<u64>();
        let type_script_arg_bytes =
            self.outputs.iter().map(|output| output.type_.as_ref().map_or(0, |script| script.args.len() as u64)).sum::<u64>();

        witness_bytes
            .saturating_add(dep_bytes)
            .saturating_add(header_dep_bytes)
            .saturating_add(output_data_bytes)
            .saturating_add(type_script_arg_bytes)
    }

    /// Get the storage-side mass of the transaction.
    ///
    /// This tracks the persistent live-cell footprint created by outputs,
    /// including per-entry overhead in the state commitment layer.
    pub fn storage_mass(&self) -> u64 {
        self.outputs
            .iter()
            .zip(self.outputs_data.iter())
            .map(|(output, data)| CELL_ENTRY_OVERHEAD_EXCLUDING_OUTPUT_BODY + output.occupied_capacity(data.len()))
            .sum()
    }

    /// Get the persisted mass commitment of the transaction.
    ///
    /// This is the storage-side mass, kept under the legacy `mass()` name so the
    /// compatibility bridge keeps writing the right semantic value.
    pub fn mass(&self) -> u64 {
        self.storage_mass()
    }

    /// Get cellbase payload (first output data for coinbase tx)
    ///
    /// This is for compatibility with old Transaction.payload field.
    /// When a legacy coinbase has no reward outputs, we preserve its payload in
    /// the first witness so it remains available and hash-committed.
    pub fn payload(&self) -> Option<&[u8]> {
        if self.is_coinbase() && !self.outputs_data.is_empty() {
            Some(&self.outputs_data[0])
        } else if self.is_coinbase() && self.outputs.is_empty() {
            self.witnesses.first().map(Vec::as_slice)
        } else {
            None
        }
    }

    /// Estimate serialized size (approximate)
    pub fn serialized_size(&self) -> usize {
        // Simplified estimation
        let mut size = 2; // ver
        size += 4 + self.inputs.len() * 40; // inputs
        size += 4 + self.cell_deps.len() * 37; // cell_deps
        size += 4 + self.header_deps.len() * 32; // header deps
        size += 4 + self
            .outputs
            .iter()
            .map(|o| 8 + 33 + o.lock.args.len() + o.type_.as_ref().map_or(0, |t| 33 + t.args.len()))
            .sum::<usize>();
        size += 4 + self.outputs_data.iter().map(|d| d.len()).sum::<usize>();
        size += 4 + self.witnesses.iter().map(|w| w.len()).sum::<usize>();
        size
    }

    /// Calculate total input capacity (requires resolved inputs)
    pub fn input_capacity(&self, resolved_inputs: &[CellMeta]) -> u64 {
        resolved_inputs.iter().map(|m| m.cell_output.capacity).sum()
    }

    /// Calculate total output capacity
    pub fn output_capacity(&self) -> u64 {
        self.outputs.iter().map(|o| o.capacity).sum()
    }

    /// Calculate fee (input_capacity - output_capacity)
    pub fn fee(&self, resolved_inputs: &[CellMeta]) -> u64 {
        self.input_capacity(resolved_inputs).saturating_sub(self.output_capacity())
    }
}

/// Cell metadata (DAG-aware)
///
/// Reference: CKB CellMeta
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellMeta {
    /// Cell output structure
    pub cell_output: CellOutput,
    /// OutPoint
    pub out_point: OutPoint,
    /// DAG transaction info
    pub transaction_info: Option<TransactionInfo>,
    /// Data size (bytes)
    pub data_bytes: u64,
    /// In-memory cell data cache
    pub mem_cell_data: Option<Vec<u8>>,
    /// In-memory cell data hash cache
    pub mem_cell_data_hash: Option<[u8; 32]>,
}

impl CellMeta {
    /// Check if this is a cellbase (mining reward)
    pub fn is_cellbase(&self) -> bool {
        self.transaction_info.as_ref().map(|info| info.is_cellbase).unwrap_or(false)
    }

    /// Get capacity
    pub fn capacity(&self) -> u64 {
        self.cell_output.capacity
    }
}

/// DAG transaction information
///
/// CKB uses BlockNumber, Spora uses DAA Score
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransactionInfo {
    /// Transaction hash
    pub tx_hash: [u8; 32],
    /// DAA score (GhostDAG blue score)
    pub daa_score: u64,
    /// Block hash (may be in multiple blocks in DAG)
    pub block_hash: [u8; 32],
    /// Is this a cellbase transaction?
    pub is_cellbase: bool,
}

/// Cell status (for queries)
///
/// Reference: CKB CellStatus
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellStatus {
    /// Cell exists and is unspent
    Live(Box<CellMeta>),
    /// Cell has been spent (at given DAA score)
    Dead(u64),
    /// Cell not found in index
    Unknown,
}

/// Resolved Cell transaction (all inputs/deps loaded)
///
/// Reference: CKB ResolvedTransaction
#[derive(Clone, Debug)]
pub struct ResolvedCellTx {
    /// The transaction
    pub transaction: CellTx,
    /// Resolved inputs
    pub resolved_inputs: Vec<CellMeta>,
    /// Resolved dependencies
    pub resolved_deps: Vec<CellMeta>,
}

impl AsRef<CellTx> for CellTx {
    fn as_ref(&self) -> &CellTx {
        self
    }
}

impl ResolvedCellTx {
    /// Calculate fee
    pub fn fee(&self) -> u64 {
        self.transaction.fee(&self.resolved_inputs)
    }

    /// Calculate effective fee rate (considering size and cycles)
    pub fn effective_fee_rate(&self, cycles: u64) -> f64 {
        const CYCLES_PER_BYTE: f64 = 100.0;
        let size = self.transaction.serialized_size() as f64;
        let cycles_size = cycles as f64 / CYCLES_PER_BYTE;
        let effective_size = size.max(cycles_size);
        self.fee() as f64 / effective_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_outpoint_key_encoding() {
        let op = OutPoint::new([0x42; 32], 0x12345678);
        let key = op.to_key();
        let decoded = OutPoint::from_key(&key);
        assert_eq!(op, decoded);
    }

    #[test]
    fn test_script_hash() {
        let script = Script::new([0x11; 32], 1, vec![0xAA, 0xBB]);
        let hash = script.hash();
        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_cell_out_capacity() {
        let lock = Script::new([0x00; 32], 0, vec![0; 20]);
        let cell = CellOutput { lock, type_: None, capacity: 1000 };
        let occupied = cell.occupied_capacity(100);
        assert!(occupied > 0);
        assert!(cell.verify_capacity(100).is_ok());
    }

    #[test]
    fn test_time_lock_flags() {
        let relative_daa_lock = CellInput::new(
            OutPoint::new([0; 32], 0),
            0xC000_0000_0000_0064, // relative + DAA + value=100
        );
        assert!(relative_daa_lock.is_relative_lock());
        assert!(relative_daa_lock.is_daa_lock());
        assert_eq!(relative_daa_lock.lock_value(), 100);
    }

    #[test]
    fn test_celltx_creation() {
        let inputs = vec![CellInput::new(OutPoint::new([0; 32], 0), 0)];
        let deps = vec![];
        let lock = Script::new([0x00; 32], 0, vec![]);
        let outputs = vec![CellOutput { lock, type_: None, capacity: 1000 }];
        let outputs_data = vec![vec![]];
        let witnesses = vec![vec![0; 65]];

        let tx = CellTx::new(inputs, deps, outputs, outputs_data, witnesses);
        assert!(tx.is_ok());
        let tx = tx.unwrap();
        assert_eq!(tx.version, CELL_TX_VERSION);
    }

    #[test]
    fn test_celltx_compute_and_storage_mass_are_distinct() {
        let inputs = vec![CellInput::new(OutPoint::new([0; 32], 0), 0)];
        let deps = vec![];
        let lock = Script::new([0x10; 32], 1, vec![1; 20]);
        let outputs = vec![CellOutput { lock, type_: None, capacity: 10_000 }];
        let outputs_data = vec![vec![7; 128]];
        let witnesses = vec![vec![0; 65]];

        let tx = CellTx::new(inputs, deps, outputs, outputs_data, witnesses).unwrap();
        assert!(tx.compute_mass() > tx.serialized_size() as u64);
        assert!(tx.compute_mass() > 0);
        assert!(tx.transient_mass() > 0);
        assert!(tx.storage_mass() > 0);
        assert_eq!(tx.transient_mass(), (tx.serialized_size() as u64) * TRANSIENT_BYTE_TO_MASS_FACTOR);
        assert_eq!(tx.mass(), tx.storage_mass());
        assert_ne!(tx.compute_mass(), tx.storage_mass());
    }

    #[test]
    fn test_dep_group_roundtrip() {
        let ops = vec![OutPoint::new([0x11; 32], 0), OutPoint::new([0x22; 32], 7), OutPoint::new([0x33; 32], u32::MAX)];
        let data = encode_dep_group_data(&ops);
        let parsed = parse_dep_group_data(&data).unwrap();
        assert_eq!(parsed, ops);
    }

    #[test]
    fn test_dep_group_empty() {
        let data = encode_dep_group_data(&[]);
        assert_eq!(data, [0, 0, 0, 0]);
        let parsed = parse_dep_group_data(&data).unwrap();
        assert!(parsed.is_empty());
    }

    #[test]
    fn test_dep_group_invalid_data() {
        assert!(parse_dep_group_data(&[]).is_err());
        assert!(parse_dep_group_data(&[1, 0, 0, 0]).is_err()); // count=1 but no data
        assert!(parse_dep_group_data(&[1, 0, 0, 0, 0]).is_err()); // count=1 but only 1 byte
    }
}
