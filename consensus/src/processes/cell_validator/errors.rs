// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Cell validation errors

use thiserror::Error;

/// Cell validation errors
#[derive(Debug, Error, Clone)]
pub enum CellValidationError {
    /// Invalid transaction format
    #[error("Invalid transaction format: {0}")]
    InvalidFormat(String),

    /// Script verification failed
    #[error("Script verification failed: {0}")]
    ScriptVerificationFailed(String),

    /// Script execution exceeded the configured cycles budget
    #[error("Script cycles exceeded limit: total {total}, limit {limit}")]
    ExceededMaxCycles { total: u64, limit: u64 },

    /// Cell not found
    #[error("Cell not found: {0:?}")]
    CellNotFound([u8; 32]),

    /// Cell dependency not found
    #[error("Cell dependency not found: {0:?}")]
    DepCellNotFound([u8; 32]),

    /// Cell already spent
    #[error("Cell already spent: {0:?}")]
    CellAlreadySpent([u8; 32]),

    /// Cell not yet created (reorg scenario)
    #[error("Cell not yet created: created at DAA {created_daa}, spent at DAA {spent_at_daa}")]
    CellNotYetCreated { created_daa: u64, spent_at_daa: u64 },

    /// Cellbase not mature
    #[error("Cellbase not mature: created at DAA {created_daa}, current DAA {current_daa}, required DAA {required_daa}")]
    CellbaseNotMature { created_daa: u64, current_daa: u64, required_daa: u64 },

    /// Capacity overflow
    #[error("Capacity overflow")]
    CapacityOverflow,

    /// Insufficient capacity
    #[error("Insufficient capacity: required {required}, available {available}")]
    InsufficientCapacity { required: u64, available: u64 },

    /// Time lock not satisfied
    #[error("Time lock not satisfied: lock value {lock_value}, current {current}")]
    TimeLockNotSatisfied { lock_value: u64, current: u64 },

    /// Script execution failed
    #[error("Script execution failed: {0}")]
    ScriptFailed(String),

    /// Invalid signature
    #[error("Invalid signature")]
    InvalidSignature,
}
