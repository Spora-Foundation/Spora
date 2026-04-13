// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Cell transaction types (CKB-inspired)

//! Cell transaction types module

/// Signature hashing functions
pub mod sighash;
/// Cell transaction core types
pub mod types;
// pub mod codec;  // Phase 1.5 - Molecule serialization

pub use sighash::{compute_sighash, compute_txid, compute_wtxid, pubkey_hash};
pub use types::{
    encode_dep_group_data, parse_dep_group_data, CellDep, CellMeta, CellOutput, CellInput, CellStatus, CellTx, DepType, OutPoint,
    ResolvedCellTx, Script, TransactionInfo,
};
