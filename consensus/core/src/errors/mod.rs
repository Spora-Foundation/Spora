pub mod block;
pub mod coinbase;
pub mod config;
pub mod consensus;
pub mod difficulty;
pub mod pruning;
pub mod sync;
pub mod traversal;
pub mod tx;
// UTXO errors temporarily enabled for compilation
// TODO(spora-critical): Remove after consensus migration to Cell model
#[deprecated(note = "UTXO errors are deprecated. Use Cell validation errors instead")]
#[path = "utxo.deprecated/mod.rs"]
pub mod utxo;
