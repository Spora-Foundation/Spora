pub mod block_depth;
pub mod cell_validator;
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
// UTXO transaction_validator temporarily enabled for compilation
// TODO(spora-critical): Replace with cell_validator after virtual_processor refactoring
#[deprecated(note = "transaction_validator is deprecated. Use cell_validator instead")]
#[path = "transaction_validator.deprecated/mod.rs"]
pub mod transaction_validator;
pub mod traversal_manager;
pub(crate) mod utils;
pub mod window;
