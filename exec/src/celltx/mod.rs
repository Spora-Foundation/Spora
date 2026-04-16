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

pub use sighash::{compute_rw_bound_sighash, compute_txid, compute_wtxid, pubkey_hash};
pub use types::{
    cell_tx_estimated_serialized_size, encode_dep_group_data, parse_dep_group_data, CapacityError, CellDep, CellInput, CellOutput,
    CellStatus, CellTx, DepType, OutPoint, ResolvedCellMeta, ResolvedCellTx, Script, ScriptHashVersion, TransactionInfo,
    CELLTX_SCHEMA_VERSION,
};

// Re-export VersionedSerializable implementations for storage layer types
pub use types::{
    ResolvedCellMeta as ResolvedCellMetaVersioned, ResolvedCellTx as ResolvedCellTxVersioned,
    TransactionInfo as TransactionInfoVersioned,
};
