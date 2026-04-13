//!
//! # Consensus Core
//!
//! This crate implements primitives used in the Spora node consensus processing.
//!

extern crate alloc;
extern crate core;
extern crate self as consensus_core;

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasher, Hasher};

pub use spora_hashes::Hash;

pub mod acceptance_data;
pub mod api;
pub mod block;
pub mod blockhash;
pub mod blockstatus;
pub mod cell_diff;
pub mod cell_metadata;
// cell_state module removed - use spora_state::CellStateTree instead
pub mod coinbase;
pub mod config;
pub mod constants;
pub mod daa_score_timestamp;
pub mod errors;
pub mod hashing;
pub mod header;
pub mod mass;
pub mod merkle;
pub mod mining_rules;
pub mod muhash;
pub mod network;
pub mod pruning;
pub mod sign;
pub mod trusted;
pub mod tx;

// The transaction-output model was fully replaced by the Cell model.
// See: exec/celltx for Cell transaction types
// See: state/ for Cell state management

/// Integer type for accumulated PoW of blue blocks. We expect no more than
/// 2^128 work in a single block (btc has ~2^80), and no more than 2^64
/// overall blocks, so 2^192 is definitely a justified upper-bound.
pub type BlueWorkType = spora_math::Uint192;

/// The extends directly from the expectation above about having no more than
/// 2^128 work in a single block
pub const MAX_WORK_LEVEL: BlockLevel = 128;

/// The type used to represent the GHOSTDAG K parameter
pub type KType = u16;

/// Map from Block hash to K type
pub type HashKTypeMap = std::sync::Arc<BlockHashMap<KType>>;

/// This HashMap skips the hashing of the key and uses the key directly as the hash.
/// Should only be used for block hashes that have correct DAA,
/// otherwise it is susceptible to DOS attacks via hash collisions.
pub type BlockHashMap<V> = HashMap<Hash, V, BlockHasher>;

/// Same as `BlockHashMap` but a `HashSet`.
pub type BlockHashSet = HashSet<Hash, BlockHasher>;

pub trait HashMapCustomHasher {
    fn new() -> Self;
    fn with_capacity(capacity: usize) -> Self;
}

// HashMap::new and HashMap::with_capacity are only implemented on Hasher=RandomState
// to avoid type inference problems, so we need to provide our own versions.
impl<V> HashMapCustomHasher for BlockHashMap<V> {
    #[inline(always)]
    fn new() -> Self {
        Self::with_hasher(BlockHasher::new())
    }
    #[inline(always)]
    fn with_capacity(cap: usize) -> Self {
        Self::with_capacity_and_hasher(cap, BlockHasher::new())
    }
}

impl HashMapCustomHasher for BlockHashSet {
    #[inline(always)]
    fn new() -> Self {
        Self::with_hasher(BlockHasher::new())
    }
    #[inline(always)]
    fn with_capacity(cap: usize) -> Self {
        Self::with_capacity_and_hasher(cap, BlockHasher::new())
    }
}

#[derive(Default, Debug)]
pub struct ChainPath {
    pub added: Vec<Hash>,
    pub removed: Vec<Hash>,
}

/// `hashes::Hash` writes 4 u64s so we just use the last one as the hash here
#[derive(Default, Clone, Copy)]
pub struct BlockHasher(u64);

impl BlockHasher {
    #[inline(always)]
    pub const fn new() -> Self {
        Self(0)
    }
}

impl Hasher for BlockHasher {
    #[inline(always)]
    fn finish(&self) -> u64 {
        self.0
    }
    #[inline(always)]
    fn write_u64(&mut self, v: u64) {
        self.0 = v;
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut tail = [0u8; 8];
        let copy_len = bytes.len().min(8);
        tail[..copy_len].copy_from_slice(&bytes[bytes.len().saturating_sub(copy_len)..]);
        self.0 = u64::from_le_bytes(tail);
    }
}

impl BuildHasher for BlockHasher {
    type Hasher = Self;

    #[inline(always)]
    fn build_hasher(&self) -> Self::Hasher {
        Self(0)
    }
}

pub type BlockLevel = u8;

#[cfg(test)]
mod tests {
    use super::BlockHasher;
    use spora_hashes::Hash;
    use std::hash::{Hash as _, Hasher as _};
    #[test]
    fn test_block_hasher() {
        let hash = Hash::from_le_u64([1, 2, 3, 4]);
        let mut hasher = BlockHasher::default();
        hash.hash(&mut hasher);
        assert_eq!(hasher.finish(), 4);
    }

    #[test]
    fn test_block_hasher_write_bytes_falls_back_to_tail_u64() {
        let mut hasher = BlockHasher::default();
        hasher.write(&[0x10, 0x20, 0x30, 0x40]);
        assert_eq!(hasher.finish(), 0x4030_2010);

        hasher.write(&[1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert_eq!(hasher.finish(), u64::from_le_bytes([2, 3, 4, 5, 6, 7, 8, 9]));
    }
}
