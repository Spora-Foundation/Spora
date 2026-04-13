// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Cell indexing service with RPC API

//! Cell Index Service
//!
//! This crate provides indexing and query services for Cells:
//! - **CellIndexer**: Main indexing service
//! - **QueryAPI**: RPC-like query interface
//! - **FilterAPI**: Filter Cells by lock/type/capacity

#![allow(missing_docs)]

pub mod api;
pub mod errors;
pub mod indexer;

pub use api::{CellFilter, CellIndexApi, CellIndexProxy, CellQuery, CellQueryResult};
pub use errors::{CellIndexError, Result};
pub use indexer::{CellDataProof, CellIndexer as CellIndex, CellIndexer};
