// SPDX-License-Identifier: ISC
// Copyright (C) 2025 Spora developers
//
// VM scheduler for parallel script execution (placeholder)
//
// NOTE: This file is a placeholder for future parallel script group execution.
// The actual script group execution is currently in verifier.rs

// Re-export script group types from verifier
pub use super::verifier::{ScriptGroup, ScriptGroupType};

/// Script verification result
#[derive(Debug)]
pub struct VerifyResult {
    /// Total cycles consumed
    pub cycles: u64,
    /// Whether verification succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
}

impl VerifyResult {
    /// Create a successful result
    pub fn success(cycles: u64) -> Self {
        Self { cycles, success: true, error: None }
    }

    /// Create a failed result
    pub fn fail(cycles: u64, error: String) -> Self {
        Self { cycles, success: false, error: Some(error) }
    }
}

// Future: Parallel script group execution scheduler
// - Schedule script groups in dependency order
// - Execute groups in parallel where possible
// - Aggregate results
