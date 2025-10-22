// SPDX-License-Identifier: ISC
// Copyright (C) 2025Tondi developers
//
// Cell transaction types (CKB-inspired)

//! Cell transaction types module

/// Cell transaction core types
pub mod types;
/// Signature hashing functions
pub mod sighash;
// pub mod codec;  // Phase 1.5 - Molecule serialization

pub use types::{
    CellTx, CellRef, CellOut, ScriptRef, OutPoint, CellDep, DepType,
    CellMeta, ResolvedCellTx, TransactionInfo, CellStatus,
};
pub use sighash::{compute_txid, compute_wtxid, compute_sighash, pubkey_hash};


