pub mod block;
pub mod coinbase;
pub mod config;
pub mod consensus;
pub mod difficulty;
pub mod pruning;
pub mod script;
pub mod sync;
pub mod traversal;
pub mod tx;

// Transaction-output algebra errors were removed during the Cell migration.
// See: consensus/src/processes/cell_validator/errors.rs for Cell validation errors.
