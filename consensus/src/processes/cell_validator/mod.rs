// SPDX-License-Identifier: ISC
// Copyright (C) 2025 Spora developers
//
// Cell transaction validator (replaces UTXO transaction validator)

//! Cell Transaction Validation
//!
//! This module validates Cell transactions in the context of the DAG:
//! - Cell availability (not double-spent)
//! - Capacity conservation
//! - Script execution (lock/type scripts)
//! - Time locks (since field)

pub mod errors;
pub mod cell_validation_in_isolation;
pub mod cell_validation_in_context;
pub mod cell_validation_in_dag;
pub mod tests;

pub use errors::CellValidationError;
pub use cell_validation_in_context::CellStateProvider;
pub use cell_validation_in_dag::DagCellProvider;

use std::sync::Arc;

#[cfg(feature = "vm")]
use spora_exec::vm::{TransactionScriptVerifier, SimpleDataProvider, ScriptError};

/// Consensus parameters for Cell validation
#[derive(Clone, Debug)]
pub struct CellConsensusParams {
    /// Cellbase maturity (DAA score)
    pub cellbase_maturity: u64,
    /// Maximum block cycles (CKB-style)
    pub max_block_cycles: u64,
    /// Maximum transaction size (bytes)
    pub max_tx_size: usize,
}

impl Default for CellConsensusParams {
    fn default() -> Self {
        Self {
            cellbase_maturity: 100, // 100 DAA scores
            max_block_cycles: 70_000_000, // 70M cycles (same as CKB)
            max_tx_size: 500 * 1024, // 500KB
        }
    }
}

/// Cell transaction validator
pub struct CellValidator<P> {
    /// Consensus parameters
    params: Arc<CellConsensusParams>,
    /// Cell state provider
    provider: Arc<P>,
}

impl<P: CellStateProvider> CellValidator<P> {
    /// Create a new cell validator
    pub fn new(params: Arc<CellConsensusParams>, provider: Arc<P>) -> Self {
        Self { params, provider }
    }
    
    /// Validate a cell transaction in isolation (stateless)
    ///
    /// Checks:
    /// - Transaction format
    /// - Version validity
    /// - Basic capacity constraints
    /// - Size limits
    pub fn validate_in_isolation(
        &self,
        tx: &spora_exec::CellTx,
    ) -> Result<(), CellValidationError> {
        cell_validation_in_isolation::validate_cell_tx_in_isolation(tx)?;
        
        // Additional checks
        // Check transaction size
        let tx_size = borsh::to_vec(tx)
            .map_err(|e| CellValidationError::InvalidFormat(e.to_string()))?
            .len();
        
        if tx_size > self.params.max_tx_size {
            return Err(CellValidationError::InvalidFormat(
                format!("Transaction too large: {} > {}", tx_size, self.params.max_tx_size)
            ));
        }
        
        Ok(())
    }
    
    /// Validate a cell transaction in DAG context (stateful)
    ///
    /// Checks:
    /// - Cell availability (not spent)
    /// - Capacity conservation
    /// - Time locks (since field)
    pub fn validate_in_context(
        &self,
        tx: &spora_exec::CellTx,
        daa_score: u64,
    ) -> Result<(), CellValidationError> {
        cell_validation_in_context::validate_cell_tx_in_context(
            tx,
            daa_score,
            self.provider.as_ref(),
        )
    }
    
    /// Validate a cell transaction in DAG (includes cellbase maturity)
    ///
    /// Checks:
    /// - All context checks
    /// - Cellbase maturity
    /// - DAG-specific constraints
    pub fn validate_in_dag(
        &self,
        tx: &spora_exec::CellTx,
        daa_score: u64,
    ) -> Result<(), CellValidationError> 
    where
        P: cell_validation_in_dag::DagCellProvider,
    {
        // First validate in context
        self.validate_in_context(tx, daa_score)?;
        
        // Then DAG-specific checks
        cell_validation_in_dag::validate_cellbase_maturity(
            tx,
            daa_score,
            self.params.cellbase_maturity,
            self.provider.as_ref(),
        )
    }
    
    /// Full validation (isolation + context + DAG)
    pub fn validate_full(
        &self,
        tx: &spora_exec::CellTx,
        daa_score: u64,
    ) -> Result<(), CellValidationError>
    where
        P: cell_validation_in_dag::DagCellProvider,
    {
        self.validate_in_isolation(tx)?;
        self.validate_in_dag(tx, daa_score)?;
        Ok(())
    }
    
    /// Verify scripts (lock + type scripts)
    ///
    /// Requires VM feature to be enabled
    #[cfg(feature = "vm")]
    pub fn verify_scripts(
        &self,
        tx: &spora_exec::CellTx,
    ) -> Result<(), CellValidationError> {
        // Create data provider
        // TODO: Use real data provider from consensus storage
        let provider = Arc::new(SimpleDataProvider::new());
        
        // Create verifier
        let verifier = TransactionScriptVerifier::new(
            Arc::new(tx.clone()),
            provider,
        );
        
        // Verify all scripts
        verifier.verify()
            .map_err(|e| CellValidationError::ScriptVerificationFailed(e.to_string()))?;
        
        Ok(())
    }
    
    /// Full validation with scripts (isolation + context + DAG + VM)
    #[cfg(feature = "vm")]
    pub fn validate_full_with_scripts(
        &self,
        tx: &spora_exec::CellTx,
        daa_score: u64,
    ) -> Result<(), CellValidationError>
    where
        P: cell_validation_in_dag::DagCellProvider,
    {
        // Standard validation
        self.validate_full(tx, daa_score)?;
        
        // Script verification
        self.verify_scripts(tx)?;
        
        Ok(())
    }
}

impl<P: CellStateProvider> Default for CellValidator<P> 
where
    P: Default,
{
    fn default() -> Self {
        Self::new(
            Arc::new(CellConsensusParams::default()),
            Arc::new(P::default()),
        )
    }
}

