// SPDX-License-Identifier: ISC
// Copyright (C) 2025Tondi developers
//
// Parallel transaction scheduler with RW-Set DAG

//! Parallel Transaction Scheduler
//!
//! This module implements parallel execution of Cell transactions:
//! - **DAG Construction**: RW-Set → CellDAG (dependency and conflict edges)
//! - **Conflict Resolution**: Deterministic ordering (fee_density, blue_pref, wtxid)
//! - **Parallel Execution**: Topological layering with Rayon

/// DAG construction module
pub mod dag;
/// Conflict resolution module
pub mod conflict;
/// Parallel executor module
pub mod executor;

pub use dag::{CellDAG, DagNode, DagEdge};
pub use conflict::{ConflictResolver, ConflictKey, ConflictResolution};
pub use executor::{ParallelExecutor, ExecutionResult, ExecutionReceipt};


