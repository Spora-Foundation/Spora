// SPDX-License-Identifier: ISC
// Copyright (C) 2025 Spora developers
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
pub mod errors;
pub mod indexer;

pub use api::{CellFilter, CellIndexProxy, CellQuery, CellQueryResult};
pub use errors::{CellIndexError, Result};
pub use indexer::CellIndexer;

/// Cell index errors (re-export from errors module)
#[deprecated(note = "Use errors::CellIndexError instead")]
pub type IndexError = CellIndexError;
