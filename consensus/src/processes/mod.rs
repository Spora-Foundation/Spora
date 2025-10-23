pub mod block_depth;
pub mod cell_validator;

// Re-export cell validator types
pub use cell_validator::{CellConsensusParams, CellStateProvider, CellValidator, DagCellProvider};

// Re-export from consensus-core
pub use spora_consensus_core::{cell_diff::CellMeta, cell_metadata::CellMetadata};
pub mod coinbase;
pub mod difficulty;
pub mod ghostdag;
pub mod parents_builder;
pub mod past_median_time;
pub mod pruning;
pub mod pruning_proof;
pub mod reachability;
pub mod relations;
pub mod sync;

// UTXO transaction_validator fully removed - replaced with cell_validator
// All transaction validation now uses CellValidator (see cell_validator/ module)

pub mod traversal_manager;
pub(crate) mod utils;
pub mod window;
