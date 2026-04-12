use serde::{Deserialize, Serialize};
use spora_consensus_core::tx::{CellEntry, ScriptPublicKey, TransactionOutpoint};
use spora_utils::mem_size::MemSizeEstimator;
use std::collections::HashMap;

// TODO: explore potential optimization via custom TransactionOutpoint hasher for below,
// One possible implementation: u64 of transaction id xor'd with 4 bytes of transaction index.
pub type CompactCellCollection = HashMap<TransactionOutpoint, CompactCellEntry>;

/// A collection of live cells indexed via [`ScriptPublicKey`] => [`TransactionOutpoint`] => [`CompactCellEntry`].
pub type CellSetByScriptPublicKey = HashMap<ScriptPublicKey, CompactCellCollection>;

/// A map of balance by script public key.
pub type BalanceByScriptPublicKey = HashMap<ScriptPublicKey, u64>;

/// Compact query-side cell metadata keyed by script public key and outpoint.
#[derive(Clone, Copy, Deserialize, Serialize, Debug)]
pub struct CompactCellEntry {
    pub amount: u64,
    pub capacity: u64,
    pub data_bytes: u64,
    pub lock_hash: [u8; 32],
    pub type_hash: Option<[u8; 32]>,
    pub data_hash: [u8; 32],
    pub block_daa_score: u64,
    pub is_coinbase: bool,
}

impl CompactCellEntry {
    pub fn new(
        capacity: u64,
        data_bytes: u64,
        lock_hash: [u8; 32],
        type_hash: Option<[u8; 32]>,
        data_hash: [u8; 32],
        block_daa_score: u64,
        is_coinbase: bool,
    ) -> Self {
        Self { amount: capacity, capacity, data_bytes, lock_hash, type_hash, data_hash, block_daa_score, is_coinbase }
    }
}

impl MemSizeEstimator for CompactCellEntry {}

impl From<CellEntry> for CompactCellEntry {
    fn from(entry: CellEntry) -> Self {
        let metadata = entry.embedded_cell_metadata();
        Self {
            amount: entry.amount(),
            capacity: entry.capacity(),
            data_bytes: metadata.map(|m| m.data_bytes).unwrap_or_default(),
            lock_hash: metadata.map(|m| m.lock_hash).unwrap_or([0; 32]),
            type_hash: metadata.and_then(|m| m.type_hash),
            data_hash: metadata.map(|m| m.data_hash).unwrap_or([0; 32]),
            block_daa_score: entry.block_daa_score,
            is_coinbase: entry.is_cellbase,
        }
    }
}

/// A struct holding live-cell set changes keyed by script public key.
#[derive(Debug, Clone)]
pub struct CellChanges {
    pub added: CellSetByScriptPublicKey,
    pub removed: CellSetByScriptPublicKey,
}

impl CellChanges {
    pub fn new(added: CellSetByScriptPublicKey, removed: CellSetByScriptPublicKey) -> Self {
        Self { added, removed }
    }
}
