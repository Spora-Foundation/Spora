// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Spora developers
//
// Cell index errors

/// Cell index errors
#[derive(Debug, thiserror::Error)]
pub enum CellIndexError {
    /// State error
    #[error("State error: {0}")]
    State(#[from] spora_state::StateError),

    /// Query failed
    #[error("Query failed: {0}")]
    QueryFailed(String),

    /// Invalid filter
    #[error("Invalid filter: {0}")]
    InvalidFilter(String),

    /// Database error
    #[error("Database error: {0}")]
    DatabaseError(String),

    /// Not found
    #[error("Cell not found")]
    NotFound,

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type for Cell indexing operations
pub type Result<T> = std::result::Result<T, CellIndexError>;
