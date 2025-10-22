// SPDX-License-Identifier: ISC
// Copyright (C) 2024 Tondi developers
//
// Cell validation errors

use thiserror::Error;

/// Cell validation errors
#[derive(Debug, Error, Clone)]
pub enum CellValidationError {
    /// Invalid transaction format
    #[error("Invalid transaction format: {0}")]
    InvalidFormat(String),
    
    /// Cell not found
    #[error("Cell not found: {0:?}")]
    CellNotFound([u8; 32]),
    
    /// Cell already spent
    #[error("Cell already spent: {0:?}")]
    CellAlreadySpent([u8; 32]),
    
    /// Capacity overflow
    #[error("Capacity overflow")]
    CapacityOverflow,
    
    /// Insufficient capacity
    #[error("Insufficient capacity: required {required}, available {available}")]
    InsufficientCapacity {
        required: u64,
        available: u64,
    },
    
    /// Time lock not satisfied
    #[error("Time lock not satisfied: lock value {lock_value}, current {current}")]
    TimeLockNotSatisfied {
        lock_value: u64,
        current: u64,
    },
    
    /// Script execution failed
    #[error("Script execution failed: {0}")]
    ScriptFailed(String),
    
    /// Invalid signature
    #[error("Invalid signature")]
    InvalidSignature,
}

