//!
//! # Client-side consensus primitives.
//!
//! This crate offers client-side primitives mirroring the consensus layer of the Spora p2p node.
//! It declares structs such as [`Transaction`], [`TransactionInput`], [`TransactionOutput`],
//! [`TransactionOutpoint`], [`CellEntry`], and [`CellEntryReference`]
//! that are used by the Wallet subsystem as well as WASM bindings.
//!
//! Unlike raw consensus primitives (used for high-performance DAG processing) the primitives
//! offered in this crate are designed to be used in client-side applications. Their internal
//! data is typically wrapped into `Arc<Mutex<T>>`, allowing for easy sharing between
//! async / threaded environments and WASM bindings.
//!

mod cell;
pub mod error;
mod imports;
mod input;
mod outpoint;
mod output;
pub mod result;
mod serializable;
mod standard_script;
mod transaction;

pub use cell::*;
pub use input::*;
pub use outpoint::*;
pub use output::*;
pub use serializable::*;
pub use standard_script::{
    address_to_builtin_standard_lock, address_to_full_script_lock, address_to_lock_script, classify_script,
    extract_address_from_script, pay_to_address_lock_script, LockScriptClass,
};
pub use transaction::*;

cfg_if::cfg_if! {
    if #[cfg(feature = "wasm32-sdk")] {
        mod header;
        mod utils;
        mod hash;
        mod sign;

        pub use header::*;
        pub use utils::*;
        pub use hash::*;
        pub use sign::sign_with_multiple;
    }
}
