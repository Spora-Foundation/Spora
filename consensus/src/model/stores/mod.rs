pub mod acceptance_data;
pub mod block_transactions;
pub mod block_window_cache;
pub mod children;
pub mod daa;
pub mod depth;
pub mod ghostdag;
pub mod headers;
pub mod headers_selected_tip;
pub mod past_pruning_points;
pub mod pruning;
pub mod pruning_samples;
pub mod pruning_utxoset;
pub mod reachability;
pub mod relations;
pub mod selected_chain;
pub mod statuses;
pub mod tips;
pub mod utxo_diffs;  // Deprecated
pub mod utxo_multisets;  // Deprecated
pub mod utxo_set;  // Deprecated
pub mod cell_diffs;  // NEW: Cell model
pub mod cell_roots;  // NEW: Cell model
pub mod virtual_state;

use std::fmt::Display;
pub use tondi_database;
pub use tondi_database::prelude::DB;

#[derive(PartialEq, Eq, Clone, Copy, Hash)]
pub(crate) struct U64Key([u8; size_of::<u64>()]);

impl From<u64> for U64Key {
    fn from(value: u64) -> Self {
        Self(value.to_le_bytes()) // TODO: Consider using big-endian for future ordering.
    }
}

impl AsRef<[u8]> for U64Key {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Display for U64Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", u64::from_le_bytes(self.0))
    }
}
