// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Cell transaction validator (replaces the previous transaction validator)

//! Cell Transaction Validation
//!
//! This module validates Cell transactions in the context of the DAG:
//! - Cell availability (not double-spent)
//! - Capacity conservation
//! - Script execution (lock/type scripts)
//! - Time locks (since field)

pub mod cell_validation_in_context;
pub mod cell_validation_in_dag;
pub mod cell_validation_in_isolation;
pub mod errors;
pub mod tests;

pub use cell_validation_in_context::CellStateProvider;
pub use cell_validation_in_dag::DagCellProvider;
pub use errors::CellValidationError;

#[cfg(feature = "vm")]
use std::collections::HashMap;
use std::sync::Arc;

#[cfg(feature = "vm")]
use spora_exec::{
    vm::{CellDataProvider, ResolvedCell, ResolvedHeader, TransactionScriptVerifier},
    CellDep, CellTx, DepType, OutPoint,
};
use spora_hashes::Hash;

/// Consensus parameters for Cell validation
#[derive(Clone, Debug)]
pub struct CellConsensusParams {
    /// Cellbase maturity (DAA score)
    pub cellbase_maturity: u64,
    /// Maximum cycles allowed for a single transaction's scripts.
    pub max_tx_cycles: u64,
    /// Maximum block cycles (CKB-style)
    pub max_block_cycles: u64,
    /// Maximum transaction size (bytes)
    pub max_tx_size: usize,
    /// Maximum bytes allowed in a single output's data payload.
    pub max_cell_data_size: usize,
}

impl Default for CellConsensusParams {
    fn default() -> Self {
        Self {
            cellbase_maturity: 100,       // 100 DAA scores
            max_tx_cycles: 10_000_000,    // 10M cycles per transaction
            max_block_cycles: 70_000_000, // 70M cycles (same as CKB)
            max_tx_size: 500 * 1024,      // 500KB
            max_cell_data_size: 500 * 1024,
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

#[cfg(feature = "vm")]
pub trait CellScriptDataProvider {
    fn get_cell_data(&self, out_point: &OutPoint, pov: Hash) -> Result<Option<Vec<u8>>, String>;
    fn get_header(&self, block_hash: Hash) -> Result<Option<ResolvedHeader>, String>;
}

#[cfg(feature = "vm")]
#[derive(Default)]
struct PreparedVmDataProvider {
    scripts: HashMap<[u8; 32], Vec<u8>>,
    cells: HashMap<([u8; 32], u32), ResolvedCell>,
    headers: HashMap<[u8; 32], ResolvedHeader>,
}

#[cfg(feature = "vm")]
impl PreparedVmDataProvider {
    fn insert_dep(&mut self, dep: &CellDep, cell: ResolvedCell) {
        let data = cell.data.clone().unwrap_or_default();
        let code_hash = *blake3::hash(&data).as_bytes();
        self.scripts.entry(code_hash).or_insert_with(|| data);
        self.cells.insert((dep.out_point.tx_hash, dep.out_point.index), cell);
    }

    fn insert_input(&mut self, out_point: &OutPoint, cell: ResolvedCell) {
        self.cells.insert((out_point.tx_hash, out_point.index), cell);
    }

    fn insert_header(&mut self, header: ResolvedHeader) {
        self.headers.insert(header.hash, header);
    }
}

#[cfg(feature = "vm")]
impl CellDataProvider for PreparedVmDataProvider {
    fn load_cell_data(&self, code_hash: &[u8; 32]) -> Option<Vec<u8>> {
        self.scripts.get(code_hash).cloned()
    }

    fn load_cell_by_outpoint(&self, tx_hash: &[u8; 32], index: u32) -> Option<ResolvedCell> {
        self.cells.get(&(*tx_hash, index)).cloned()
    }

    fn load_header(&self, hash: &[u8; 32]) -> Option<ResolvedHeader> {
        self.headers.get(hash).cloned()
    }
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
    pub fn validate_in_isolation(&self, tx: &spora_exec::CellTx) -> Result<(), CellValidationError> {
        cell_validation_in_isolation::validate_cell_tx_in_isolation(tx, self.params.max_cell_data_size)?;

        // Additional checks
        // Check transaction size
        let tx_size = borsh::to_vec(tx).map_err(|e| CellValidationError::InvalidFormat(e.to_string()))?.len();

        if tx_size > self.params.max_tx_size {
            return Err(CellValidationError::InvalidFormat(format!(
                "Transaction too large: {} > {}",
                tx_size, self.params.max_tx_size
            )));
        }

        Ok(())
    }

    /// Validate a cell transaction in DAG context (stateful)
    ///
    /// Checks:
    /// - Cell availability (not spent)
    /// - Capacity conservation
    ///
    /// Time-based `since` constraints that depend on historical input metadata are
    /// evaluated in `validate_in_dag`.
    pub fn validate_in_context(&self, tx: &spora_exec::CellTx, pov: Hash, daa_score: u64) -> Result<(), CellValidationError> {
        cell_validation_in_context::validate_cell_tx_in_context(tx, pov, daa_score, self.provider.as_ref())
    }

    /// Validate a cell transaction in DAG (includes cellbase maturity)
    ///
    /// Checks:
    /// - All context checks
    /// - Cellbase maturity
    /// - Absolute / relative DAA locks
    /// - Absolute / relative timestamp locks
    /// - DAG-specific constraints
    pub fn validate_in_dag(
        &self,
        tx: &spora_exec::CellTx,
        pov: Hash,
        daa_score: u64,
        timestamp: u64,
    ) -> Result<(), CellValidationError>
    where
        P: cell_validation_in_dag::DagCellProvider,
    {
        cell_validation_in_dag::validate_cell_existence(tx, pov, self.provider.as_ref())?;

        // First validate in context
        self.validate_in_context(tx, pov, daa_score)?;

        // Then validate `since` against the explicit POV state snapshot.
        cell_validation_in_dag::validate_time_locks(tx, pov, daa_score, timestamp, self.provider.as_ref())?;

        // Then DAG-specific checks
        cell_validation_in_dag::validate_cellbase_maturity(tx, pov, daa_score, self.params.cellbase_maturity, self.provider.as_ref())
    }

    /// Full validation (isolation + context + DAG)
    pub fn validate_full(&self, tx: &spora_exec::CellTx, pov: Hash, daa_score: u64, timestamp: u64) -> Result<(), CellValidationError>
    where
        P: cell_validation_in_dag::DagCellProvider,
    {
        self.validate_in_isolation(tx)?;
        self.validate_in_dag(tx, pov, daa_score, timestamp)?;
        Ok(())
    }

    /// Verify scripts (lock + type scripts)
    ///
    /// Requires VM feature to be enabled
    #[cfg(feature = "vm")]
    pub fn verify_scripts(&self, tx: &CellTx, pov: Hash) -> Result<(), CellValidationError>
    where
        P: CellScriptDataProvider + DagCellProvider,
    {
        self.verify_scripts_with_cycles(tx, pov).map(|_| ())
    }

    /// Verify scripts (lock + type scripts) and return the total consumed cycles.
    #[cfg(feature = "vm")]
    pub fn verify_scripts_with_cycles(&self, tx: &CellTx, pov: Hash) -> Result<u64, CellValidationError>
    where
        P: CellScriptDataProvider + DagCellProvider,
    {
        let provider = Arc::new(self.prepare_vm_data_provider(tx, pov)?);
        let per_tx_cycles_limit = self.params.max_tx_cycles.min(self.params.max_block_cycles);

        // Create verifier
        let verifier = TransactionScriptVerifier::new(Arc::new(tx.clone()), provider).with_max_cycles(per_tx_cycles_limit);

        // Verify all scripts
        let total_cycles = verifier.verify_with_cycles().map_err(|e| CellValidationError::ScriptVerificationFailed(e.to_string()))?;
        if total_cycles > per_tx_cycles_limit {
            return Err(CellValidationError::ExceededMaxCycles { total: total_cycles, limit: per_tx_cycles_limit });
        }

        Ok(total_cycles)
    }

    /// Full validation with scripts (isolation + context + DAG + VM)
    #[cfg(feature = "vm")]
    pub fn validate_full_with_scripts(
        &self,
        tx: &spora_exec::CellTx,
        pov: Hash,
        daa_score: u64,
        timestamp: u64,
    ) -> Result<(), CellValidationError>
    where
        P: cell_validation_in_dag::DagCellProvider + CellScriptDataProvider,
    {
        self.validate_full_with_scripts_and_cycles(tx, pov, daa_score, timestamp).map(|_| ())
    }

    /// Full validation with scripts (isolation + context + DAG + VM) returning total script cycles.
    #[cfg(feature = "vm")]
    pub fn validate_full_with_scripts_and_cycles(
        &self,
        tx: &spora_exec::CellTx,
        pov: Hash,
        daa_score: u64,
        timestamp: u64,
    ) -> Result<u64, CellValidationError>
    where
        P: cell_validation_in_dag::DagCellProvider + CellScriptDataProvider,
    {
        // Standard validation
        self.validate_full(tx, pov, daa_score, timestamp)?;

        // Script verification
        let total_cycles = self.verify_scripts_with_cycles(tx, pov)?;

        Ok(total_cycles)
    }

    #[cfg(feature = "vm")]
    fn prepare_vm_data_provider(&self, tx: &CellTx, pov: Hash) -> Result<PreparedVmDataProvider, CellValidationError>
    where
        P: CellScriptDataProvider + DagCellProvider,
    {
        let mut provider = PreparedVmDataProvider::default();

        for dep in &tx.deps {
            match dep.dep_type {
                DepType::Code => {
                    let metadata = self
                        .provider
                        .get_cell_at_pov(&dep.out_point, pov)
                        .map_err(CellValidationError::InvalidFormat)?
                        .ok_or(CellValidationError::CellNotFound(dep.out_point.tx_hash))?;
                    let data = self
                        .provider
                        .get_cell_data(&dep.out_point, pov)
                        .map_err(CellValidationError::InvalidFormat)?
                        .ok_or(CellValidationError::CellNotFound(dep.out_point.tx_hash))?;
                    provider.insert_dep(dep, metadata_to_resolved_cell(metadata, Some(data))?);
                }
                DepType::DepGroup => {
                    // Read the DepGroup cell data, parse it as OutPoint list, expand each as Code dep
                    let group_data = self
                        .provider
                        .get_cell_data(&dep.out_point, pov)
                        .map_err(CellValidationError::InvalidFormat)?
                        .ok_or(CellValidationError::CellNotFound(dep.out_point.tx_hash))?;
                    let outpoints = spora_exec::parse_dep_group_data(&group_data).map_err(CellValidationError::InvalidFormat)?;
                    for op in &outpoints {
                        let code_dep = CellDep { out_point: *op, dep_type: DepType::Code };
                        let metadata = self
                            .provider
                            .get_cell_at_pov(op, pov)
                            .map_err(CellValidationError::InvalidFormat)?
                            .ok_or(CellValidationError::DepCellNotFound(op.tx_hash))?;
                        let data = self
                            .provider
                            .get_cell_data(op, pov)
                            .map_err(CellValidationError::InvalidFormat)?
                            .ok_or(CellValidationError::DepCellNotFound(op.tx_hash))?;
                        provider.insert_dep(&code_dep, metadata_to_resolved_cell(metadata, Some(data))?);
                    }
                }
            }
        }

        for input in &tx.inputs {
            let metadata = self
                .provider
                .get_cell_at_pov(&input.out_point, pov)
                .map_err(CellValidationError::InvalidFormat)?
                .ok_or(CellValidationError::CellNotFound(input.out_point.tx_hash))?;
            let data = self.provider.get_cell_data(&input.out_point, pov).map_err(CellValidationError::InvalidFormat)?;
            provider.insert_input(&input.out_point, metadata_to_resolved_cell(metadata, data)?);
        }

        for header_hash in &tx.header_deps {
            let hash = Hash::from_bytes(*header_hash);
            let header = self
                .provider
                .get_header(hash)
                .map_err(CellValidationError::InvalidFormat)?
                .ok_or_else(|| CellValidationError::InvalidFormat(format!("missing header dependency {}", hash)))?;
            provider.insert_header(header);
        }

        Ok(provider)
    }
}

#[cfg(feature = "vm")]
fn metadata_to_resolved_cell(
    metadata: spora_consensus_core::cell_metadata::CellMetadata,
    data: Option<Vec<u8>>,
) -> Result<ResolvedCell, CellValidationError> {
    let lock_script = metadata
        .lock_script
        .ok_or_else(|| CellValidationError::InvalidFormat(format!("missing lock script for resolved cell {}", metadata.out_point)))?;
    Ok(ResolvedCell {
        cell_output: spora_exec::CellOut { lock: lock_script, type_: metadata.type_script, capacity: metadata.capacity },
        data: data.or(metadata.data),
    })
}

impl<P: CellStateProvider> Default for CellValidator<P>
where
    P: Default,
{
    fn default() -> Self {
        Self::new(Arc::new(CellConsensusParams::default()), Arc::new(P::default()))
    }
}
