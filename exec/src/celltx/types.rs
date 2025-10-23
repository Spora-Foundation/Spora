// SPDX-License-Identifier: ISC
// Copyright (C) 2025 Spora developers
//
// Cell transaction core types (CKB-inspired, DAG-adapted)
//
// Reference: ckb/util/types/src/core/cell.rs

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// Cell transaction version: 0xC001
pub const CELL_TX_VERSION: u16 = 0xC001;

/// OutPoint: uniquely identifies a Cell (tx_hash || output_index)
///
/// Reference: CKB OutPoint
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct OutPoint {
    /// Transaction hash (32 bytes)
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

/// Script reference (Lock or Type script)
///
/// Reference: CKB Script
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct ScriptRef {
    /// Script code hash (points to a Cell's data)
    pub code_hash: [u8; 32],
    /// Hash type: 0=Data, 1=Type, 2=Data1, 3=Data2
    pub hash_type: u8,
    /// Script arguments (passed to VM)
    pub args: Vec<u8>,
}

impl ScriptRef {
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
}

/// Cell output structure
///
/// Note: data field is separated to CellTx.outputs_data (CKB optimization)
///
/// Reference: CKB CellOutput
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct CellOut {
    /// Lock script: defines who can spend this Cell
    pub lock: ScriptRef,
    /// Type script (optional): defines state transition constraints
    pub type_: Option<ScriptRef>,
    /// Capacity (saus): amount + storage cost
    pub capacity: u64,
    // ⚠️ NO data field here! Data is in CellTx.outputs_data
}

impl CellOut {
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
pub struct CellRef {
    /// OutPoint: which Cell to spend
    pub out_point: OutPoint,
    /// Since: time lock (relative/absolute, timestamp/DAA)
    /// Bit 63: 0=absolute, 1=relative
    /// Bit 62: 0=timestamp, 1=DAA score
    /// Bit 61-0: lock value
    pub since: u64,
}

impl CellRef {
    /// Create a new cell reference
    pub fn new(out_point: OutPoint, since: u64) -> Self {
        Self { out_point, since }
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

/// Cell transaction (complete structure)
///
/// Reference: CKB Transaction
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct CellTx {
    /// Transaction version: 0xC001 (Cell v1)
    pub ver: u16,
    /// Inputs: Cells to spend
    pub inputs: Vec<CellRef>,
    /// Dependencies: read-only Cells (e.g., script code)
    pub deps: Vec<CellDep>,
    /// Outputs: new Cells to create
    pub outputs: Vec<CellOut>,
    /// Output data (1:1 with outputs)
    /// Note: CKB separates outputs and data for verification optimization
    pub outputs_data: Vec<Vec<u8>>,
    /// Witnesses: signatures, multi-sig scripts, etc.
    pub witnesses: Vec<Vec<u8>>,
}

impl CellTx {
    /// Create a new Cell transaction
    pub fn new(
        inputs: Vec<CellRef>,
        deps: Vec<CellDep>,
        outputs: Vec<CellOut>,
        outputs_data: Vec<Vec<u8>>,
        witnesses: Vec<Vec<u8>>,
    ) -> Result<Self, &'static str> {
        if outputs.len() != outputs_data.len() {
            return Err("outputs and outputs_data length mismatch");
        }
        Ok(Self {
            ver: CELL_TX_VERSION,
            inputs,
            deps,
            outputs,
            outputs_data,
            witnesses,
        })
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
    pub fn version(&self) -> u16 {
        self.ver
    }
    
    /// Check if this is a cellbase (coinbase) transaction
    ///
    /// Cellbase transactions have no inputs (mining reward)
    pub fn is_coinbase(&self) -> bool {
        self.inputs.is_empty()
    }
    
    /// Get mass (storage weight) of the transaction
    ///
    /// In Cell model, mass = serialized_size for now
    /// TODO(spora): Implement proper mass calculation based on storage cost
    pub fn mass(&self) -> u64 {
        self.serialized_size() as u64
    }
    
    /// Get cellbase payload (first output data for coinbase tx)
    ///
    /// This is for compatibility with old Transaction.payload field
    /// Returns None if not a coinbase or no outputs
    pub fn payload(&self) -> Option<&[u8]> {
        if self.is_coinbase() && !self.outputs_data.is_empty() {
            Some(&self.outputs_data[0])
        } else {
            None
        }
    }
    
    /// Estimate serialized size (approximate)
    pub fn serialized_size(&self) -> usize {
        // Simplified estimation
        let mut size = 2; // ver
        size += 4 + self.inputs.len() * 40; // inputs
        size += 4 + self.deps.len() * 37; // deps
        size += 4 + self.outputs.iter()
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
    pub cell_output: CellOut,
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
        self.transaction_info
            .as_ref()
            .map(|info| info.is_cellbase)
            .unwrap_or(false)
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
        let script = ScriptRef::new([0x11; 32], 1, vec![0xAA, 0xBB]);
        let hash = script.hash();
        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_cell_out_capacity() {
        let lock = ScriptRef::new([0x00; 32], 0, vec![0; 20]);
        let cell = CellOut {
            lock,
            type_: None,
            capacity: 1000,
        };
        let occupied = cell.occupied_capacity(100);
        assert!(occupied > 0);
        assert!(cell.verify_capacity(100).is_ok());
    }

    #[test]
    fn test_time_lock_flags() {
        let relative_daa_lock = CellRef::new(
            OutPoint::new([0; 32], 0),
            0xC000_0000_0000_0064, // relative + DAA + value=100
        );
        assert!(relative_daa_lock.is_relative_lock());
        assert!(relative_daa_lock.is_daa_lock());
        assert_eq!(relative_daa_lock.lock_value(), 100);
    }

    #[test]
    fn test_celltx_creation() {
        let inputs = vec![CellRef::new(OutPoint::new([0; 32], 0), 0)];
        let deps = vec![];
        let lock = ScriptRef::new([0x00; 32], 0, vec![]);
        let outputs = vec![CellOut { lock, type_: None, capacity: 1000 }];
        let outputs_data = vec![vec![]];
        let witnesses = vec![vec![0; 65]];

        let tx = CellTx::new(inputs, deps, outputs, outputs_data, witnesses);
        assert!(tx.is_ok());
        let tx = tx.unwrap();
        assert_eq!(tx.ver, CELL_TX_VERSION);
    }
}

