pub mod block;
pub mod coinbase;
pub mod config;
pub mod consensus;
pub mod difficulty;
pub mod pruning;
pub mod sync;
pub mod traversal;
pub mod tx;

// Legacy transaction-output errors fully removed - migrated to Cell model
// See: consensus/src/processes/cell_validator/errors.rs for Cell validation errors
