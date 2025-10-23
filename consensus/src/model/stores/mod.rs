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
// pruning_utxoset removed - Cell state managed directly in VirtualState
pub mod reachability;
pub mod relations;
pub mod selected_chain;
pub mod statuses;
pub mod tips;
// Cell model stores
pub mod cell_diffs;
pub mod cell_roots;
pub mod virtual_state;

pub use spora_database;
pub use spora_database::prelude::DB;
use std::fmt::Display;

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
