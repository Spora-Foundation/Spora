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
/// Domain used by the versioned script hash format.
pub const SCRIPT_HASH_V1_DOMAIN: &[u8] = b"spora-cell/script-hash";
/// Additional bytes a live-cell state entry needs beyond the raw output body.
const CELL_ENTRY_OVERHEAD_EXCLUDING_OUTPUT_BODY: u64 = 32 + 4 + 8 + 1;
/// Static transient-mass factor used before block-context VM cycles are known.
///
/// This intentionally mirrors the consensus-side transient-byte policy until the
/// Cell-native mass model is fully centralized.
const TRANSIENT_BYTE_TO_MASS_FACTOR: u64 = 4;
/// Mass coefficient for each serialized transaction byte.
///
/// Kept in sync with the consensus-side default params so pre-VM estimates in the
/// exec crate match the non-contextual compute mass policy.
const MASS_PER_TX_BYTE: u64 = 1;
/// Mass coefficient for output lock/type script bytes.
const MASS_PER_SCRIPT_PUB_KEY_BYTE: u64 = 10;
/// Mass coefficient for each implicit input sigop.
const MASS_PER_SIG_OP: u64 = 1000;

/// Estimated serialized size of a `CellTx`.
///
/// This is the canonical estimator shared by exec-side estimate helpers and
/// consensus-side mass calculation. Keep this logic centralized to avoid
/// drift between compatibility estimates and the authoritative mass path.
pub fn cell_tx_estimated_serialized_size(tx: &CellTx) -> u64 {
    let mut size: u64 = 0;
    size += 2; // ver (u16)

    // Inputs: each CellInput = outpoint (32+4) + since (8) = 44 bytes
    size += 8; // number of inputs
    size += tx.inputs.len() as u64 * 44;

    // Deps: each CellDep = outpoint (32+4) + dep_type (1) = 37 bytes
    size += 8; // number of deps
    size += tx.cell_deps.len() as u64 * 37;

    // Header deps: each is a 32-byte hash
    size += 8; // number of header_deps
    size += tx.header_deps.len() as u64 * 32;

    // Outputs: each CellOutput = lock script + optional type script + capacity
    size += 8; // number of outputs
    for output in &tx.outputs {
        size += 32 + 1 + 8; // lock.code_hash + lock.hash_type + len(lock.args)
        size += output.lock.args.len() as u64;
        if let Some(ref type_script) = output.type_ {
            size += 1 + 32 + 1 + 8; // flag + code_hash + hash_type + len(args)
            size += type_script.args.len() as u64;
        } else {
            size += 1; // no-type flag
        }
        size += 8; // capacity
    }

    // Outputs data
    size += 8; // number of outputs_data
    for data in &tx.outputs_data {
        size += 8; // length prefix
        size += data.len() as u64;
    }

    // Witnesses
    size += 8; // number of witnesses
    for witness in &tx.witnesses {
        size += 8; // length prefix
        size += witness.len() as u64;
    }

    size
}

/// Structured capacity validation error shared by Cell outputs and Cell metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CapacityError {
    /// The declared capacity is below the minimum occupied capacity.
    #[error("insufficient capacity: required {required}, available {available}")]
    InsufficientCapacity {
        /// Minimum occupied capacity required by the cell shape.
        required: u64,
        /// Capacity declared by the offending value.
        available: u64,
    },
}

/// Script hash format selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptHashVersion {
    /// Domain-separated format with an explicit version byte.
    V1,
}

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

    /// Calculate the canonical script hash currently used by the protocol.
    pub fn hash(&self) -> [u8; 32] {
        self.hash_v1()
    }

    /// Calculate the V1 script hash with explicit domain separation and versioning.
    pub fn hash_v1(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(SCRIPT_HASH_V1_DOMAIN);
        hasher.update(&[1u8]);
        hasher.update(&self.code_hash);
        hasher.update(&[self.hash_type]);
        hasher.update(&(self.args.len() as u32).to_le_bytes());
        hasher.update(&self.args);
        *hasher.finalize().as_bytes()
    }

    /// Calculate the script hash using an explicit format version.
    pub fn hash_with_version(&self, version: ScriptHashVersion) -> [u8; 32] {
        match version {
            ScriptHashVersion::V1 => self.hash_v1(),
        }
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
    pub fn verify_capacity(&self, data_len: usize) -> Result<(), CapacityError> {
        let occupied = self.occupied_capacity(data_len);
        if self.capacity < occupied {
            return Err(CapacityError::InsufficientCapacity { required: occupied, available: self.capacity });
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
    pub fn id(&self) -> [u8; 32] {
        crate::celltx::compute_txid(self)
    }

    /// Get transaction version
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
    /// This is a deterministic pre-VM compute hint aligned with the
    /// consensus-side non-contextual mass policy:
    ///
    /// - serialized bytes
    /// - output lock/type script bytes
    /// - one implicit sigop per input
    ///
    /// It does not include actual VM-verified cycles, so consensus and mempool
    /// callers must continue using the unified mass pipeline when they need the
    /// authoritative `effective_compute_mass`.
    pub fn estimated_compute_mass(&self) -> u64 {
        let serialized_size = self.serialized_size() as u64;
        let size_mass = serialized_size.saturating_mul(MASS_PER_TX_BYTE);
        let script_mass = self.total_output_script_bytes().saturating_mul(MASS_PER_SCRIPT_PUB_KEY_BYTE);
        let sigops_mass = (self.inputs.len() as u64).saturating_mul(MASS_PER_SIG_OP);
        size_mass.saturating_add(script_mass).saturating_add(sigops_mass)
    }

    /// Get the transient-storage mass of the transaction.
    ///
    /// This tracks temporary mempool/relay footprint using a deterministic
    /// serialized-size based factor before contextual execution data exists.
    pub fn estimated_transient_mass(&self) -> u64 {
        (self.serialized_size() as u64).saturating_mul(TRANSIENT_BYTE_TO_MASS_FACTOR)
    }

    fn total_output_script_bytes(&self) -> u64 {
        self.outputs
            .iter()
            .map(|output| {
                let mut script_size = 32 + 1 + output.lock.args.len() as u64;
                if let Some(ref type_script) = output.type_ {
                    script_size = script_size.saturating_add(32 + 1 + type_script.args.len() as u64);
                }
                script_size
            })
            .sum()
    }

    /// Get the storage-side mass of the transaction.
    ///
    /// This tracks the persistent live-cell footprint created by outputs,
    /// including per-entry overhead in the state commitment layer.
    ///
    /// It remains an output-footprint estimate and is not the contextual
    /// KIP-0009 storage truth used after input resolution.
    pub fn estimated_storage_mass(&self) -> u64 {
        self.outputs
            .iter()
            .zip(self.outputs_data.iter())
            .map(|(output, data)| CELL_ENTRY_OVERHEAD_EXCLUDING_OUTPUT_BODY + output.occupied_capacity(data.len()))
            .sum()
    }

    /// Get cellbase payload (first output data for coinbase tx).
    pub fn payload(&self) -> Option<&[u8]> {
        if !self.is_coinbase() {
            return None;
        }

        if let Some(first_output_data) = self.outputs_data.first() {
            return Some(first_output_data);
        }

        self.witnesses.first().map(Vec::as_slice)
    }

    /// Estimate serialized size using the canonical shared estimator.
    pub fn serialized_size(&self) -> usize {
        cell_tx_estimated_serialized_size(self) as usize
    }

    /// Calculate total input capacity (requires resolved inputs)
    pub fn input_capacity(&self, resolved_inputs: &[ResolvedCellMeta]) -> u64 {
        resolved_inputs.iter().map(|m| m.cell_output.capacity).sum()
    }

    /// Calculate total output capacity
    pub fn output_capacity(&self) -> u64 {
        self.outputs.iter().map(|o| o.capacity).sum()
    }

    /// Calculate fee (input_capacity - output_capacity)
    pub fn fee(&self, resolved_inputs: &[ResolvedCellMeta]) -> u64 {
        self.input_capacity(resolved_inputs).saturating_sub(self.output_capacity())
    }
}

/// Cell metadata (DAG-aware)
///
/// Reference: CKB CellMeta, specialized for resolved execution inputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedCellMeta {
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

impl ResolvedCellMeta {
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
    Live(Box<ResolvedCellMeta>),
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
    pub resolved_inputs: Vec<ResolvedCellMeta>,
    /// Resolved dependencies
    pub resolved_deps: Vec<ResolvedCellMeta>,
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
    fn test_script_hash_v1_is_versioned_and_distinct() {
        let script = Script::new([0x11; 32], 1, vec![0xAA, 0xBB]);
        let canonical = script.hash();
        let versioned = script.hash_v1();

        assert_eq!(canonical, versioned);
        assert_eq!(versioned, script.hash_with_version(ScriptHashVersion::V1));
    }

    #[test]
    fn test_cell_out_capacity() {
        let lock = Script::new([0x00; 32], 0, vec![0; 20]);
        let cell = CellOutput { lock, type_: None, capacity: 1000 };
        let occupied = cell.occupied_capacity(100);
        assert!(occupied > 0);
        assert!(cell.verify_capacity(100).is_ok());
        assert_eq!(
            CellOutput { lock: Script::new([0x00; 32], 0, vec![0; 20]), type_: None, capacity: 10 }.verify_capacity(100),
            Err(CapacityError::InsufficientCapacity { required: occupied, available: 10 })
        );
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
        assert!(tx.estimated_compute_mass() > tx.serialized_size() as u64);
        assert!(tx.estimated_compute_mass() > 0);
        assert!(tx.estimated_transient_mass() > 0);
        assert!(tx.estimated_storage_mass() > 0);
        assert_eq!(tx.estimated_transient_mass(), (tx.serialized_size() as u64) * TRANSIENT_BYTE_TO_MASS_FACTOR);
        assert_ne!(tx.estimated_compute_mass(), tx.estimated_storage_mass());
    }

    #[test]
    fn test_celltx_compute_mass_matches_non_contextual_formula() {
        let inputs = vec![CellInput::new(OutPoint::new([0x01; 32], 0), 0), CellInput::new(OutPoint::new([0x02; 32], 1), 0)];
        let deps = vec![];
        let outputs = vec![
            CellOutput { lock: Script::new([0x10; 32], 1, vec![1; 20]), type_: None, capacity: 10_000 },
            CellOutput {
                lock: Script::new([0x20; 32], 1, vec![2; 32]),
                type_: Some(Script::new([0x30; 32], 1, vec![3; 12])),
                capacity: 20_000,
            },
        ];
        let outputs_data = vec![vec![0xAA; 16], vec![0xBB; 8]];
        let witnesses = vec![vec![0xCC; 65], vec![0xDD; 32]];

        let tx = CellTx::new(inputs, deps, outputs, outputs_data, witnesses).unwrap();
        let serialized_size = tx.serialized_size() as u64;
        let total_output_script_bytes = (32 + 1 + 20) + (32 + 1 + 32) + (32 + 1 + 12);
        let expected =
            serialized_size * MASS_PER_TX_BYTE + total_output_script_bytes as u64 * MASS_PER_SCRIPT_PUB_KEY_BYTE + 2 * MASS_PER_SIG_OP;

        assert_eq!(tx.estimated_compute_mass(), expected);
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
