// SPDX-License-Identifier: MIT
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
    cell_tx_estimated_serialized_size, cellscript_compiled_scheduler_accesses_for_tx, cellscript_compiled_scheduler_summary_for_tx,
    decode_cellscript_scheduler_witness, decode_cellscript_scheduler_witness_for_tx, decode_cellscript_scheduler_witness_legacy_borsh,
    encode_cellscript_scheduler_witness_molecule, encode_ckb_dep_group_data, encode_dep_group_data, encode_dep_group_data_for_abi,
    is_cellscript_scheduler_witness_bytes, parse_ckb_dep_group_data, parse_dep_group_data, parse_dep_group_data_for_abi,
    validate_cellscript_scheduler_witness_access_set, validate_cellscript_scheduler_witness_against_transaction,
    validate_cellscript_scheduler_witness_summary, CapacityError, CellDep, CellInput, CellOutput, CellScriptSchedulerAccessWitness,
    CellScriptSchedulerWitness, CellScriptSchedulerWitnessError, CellStatus, CellTx, DepGroupDataAbi, DepType, OutPoint,
    ResolvedCellMeta, ResolvedCellTx, Script, ScriptHashVersion, TransactionInfo, CELLSCRIPT_SCHEDULER_EFFECT_CREATING,
    CELLSCRIPT_SCHEDULER_EFFECT_DESTROYING, CELLSCRIPT_SCHEDULER_EFFECT_MUTATING, CELLSCRIPT_SCHEDULER_EFFECT_PURE,
    CELLSCRIPT_SCHEDULER_EFFECT_READ_ONLY, CELLSCRIPT_SCHEDULER_OP_CLAIM, CELLSCRIPT_SCHEDULER_OP_CONSUME,
    CELLSCRIPT_SCHEDULER_OP_CREATE, CELLSCRIPT_SCHEDULER_OP_DESTROY, CELLSCRIPT_SCHEDULER_OP_MUTATE_INPUT,
    CELLSCRIPT_SCHEDULER_OP_MUTATE_OUTPUT, CELLSCRIPT_SCHEDULER_OP_READ_REF, CELLSCRIPT_SCHEDULER_OP_SETTLE,
    CELLSCRIPT_SCHEDULER_OP_TRANSFER, CELLSCRIPT_SCHEDULER_SOURCE_CELL_DEP, CELLSCRIPT_SCHEDULER_SOURCE_INPUT,
    CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT, CELLSCRIPT_SCHEDULER_WITNESS_MAGIC, CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
    CELLTX_SCHEMA_VERSION,
};

// Re-export VersionedSerializable implementations for storage layer types
pub use types::{
    ResolvedCellMeta as ResolvedCellMetaVersioned, ResolvedCellTx as ResolvedCellTxVersioned,
    TransactionInfo as TransactionInfoVersioned,
};
