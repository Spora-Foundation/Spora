// SPDX-License-Identifier: ISC
// Copyright (C) 2024 Tondi developers
//
// Cell indexing service with RPC API

//! Cell Index Service
//!
//! This crate provides indexing and query services for Cells:
//! - **CellIndexer**: Main indexing service
//! - **QueryAPI**: RPC-like query interface
//! - **FilterAPI**: Filter Cells by lock/type/capacity

#![warn(missing_docs)]

pub mod api;
pub mod indexer;

pub use api::{CellQuery, CellQueryResult, CellFilter};
pub use indexer::CellIndexer;

/// Cell index errors
#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    /// State error
    #[error("State error: {0}")]
    State(#[from] tondi_state::StateError),
    
    /// Query failed
    #[error("Query failed: {0}")]
    QueryFailed(String),
    
    /// Invalid filter
    #[error("Invalid filter: {0}")]
    InvalidFilter(String),
}

/// Result type for indexing operations
pub type Result<T> = std::result::Result<T, IndexError>;
