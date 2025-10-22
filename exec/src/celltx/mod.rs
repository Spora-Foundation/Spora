// SPDX-License-Identifier: ISC
// Copyright (C) 2024 Tondi developers
//
// Cell transaction types (CKB-inspired)

pub mod types;
pub mod sighash;
// pub mod codec;  // Phase 1.5 - Molecule serialization

pub use types::{
    CellTx, CellRef, CellOut, ScriptRef, OutPoint, CellDep, DepType,
    CellMeta, ResolvedCellTx, TransactionInfo, CellStatus,
};
pub use sighash::{compute_txid, compute_wtxid, compute_sighash, pubkey_hash};

