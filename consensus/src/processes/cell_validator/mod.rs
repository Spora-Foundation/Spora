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
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[cfg(feature = "vm")]
use spora_consensus_core::{
    cell_metadata::CellMetadata,
    hashing::sighash::{calc_ecdsa_signature_hash, calc_schnorr_signature_hash, SigHashReusedValuesUnsync},
    sign::{key_id20, parse_standard_witness_envelope, KEY_ID_DOMAIN_ECDSA, KEY_ID_DOMAIN_SCHNORR},
    tx::{classify_script, MutableTransaction, ScriptClass, VerifiableTransaction},
};
use spora_exec::DepGroupDataAbi;
#[cfg(feature = "vm")]
use spora_exec::{
    vm::{
        syscalls::{LOAD_SIGNATURE_HASH_BASE_CYCLES, SECP256K1_VERIFY_BASE_CYCLES},
        transferred_byte_cycles, CellDataProvider, ResolvedCell, ResolvedHeader, ScriptError, TransactionScriptVerifier,
        TransactionState, VMError, VerifyResult,
    },
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
    /// Cell data ABI used when expanding `DepType::DepGroup`.
    ///
    /// Spora defaults to its existing count-prefixed ABI, while CKB-targeted
    /// validation can select Molecule `OutPointVec` through this explicit knob.
    pub dep_group_data_abi: DepGroupDataAbi,
}

impl Default for CellConsensusParams {
    fn default() -> Self {
        Self {
            cellbase_maturity: 100,       // 100 DAA scores
            max_tx_cycles: 10_000_000,    // 10M cycles per transaction
            max_block_cycles: 70_000_000, // 70M cycles (same as CKB)
            max_tx_size: 500 * 1024,      // 500KB
            max_cell_data_size: 500 * 1024,
            dep_group_data_abi: DepGroupDataAbi::Spora,
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
const NATIVE_STANDARD_LOCK_SIG_HASH_BYTES: usize = 32;
#[cfg(feature = "vm")]
const NATIVE_STANDARD_LOCK_VERIFY_TRANSFER_BYTES: usize = 20 + 65 + 32;

#[cfg(feature = "vm")]
fn native_standard_lock_cycles_per_input() -> u64 {
    LOAD_SIGNATURE_HASH_BASE_CYCLES
        .saturating_add(transferred_byte_cycles(NATIVE_STANDARD_LOCK_SIG_HASH_BYTES))
        .saturating_add(SECP256K1_VERIFY_BASE_CYCLES)
        .saturating_add(transferred_byte_cycles(NATIVE_STANDARD_LOCK_VERIFY_TRANSFER_BYTES))
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
    cell_headers: HashMap<([u8; 32], u32), [u8; 32]>,
}

#[cfg(feature = "vm")]
struct ScriptVerificationPlan {
    resolved_inputs: Vec<CellMetadata>,
    per_tx_cycles_limit: u64,
    use_native_standard_locks: bool,
    requires_vm_type_validation: bool,
    native_standard_input_indices: Vec<usize>,
}

#[cfg(feature = "vm")]
impl ScriptVerificationPlan {
    fn native_cycles_budget(&self) -> u64 {
        native_standard_lock_cycles_per_input().saturating_mul(self.native_standard_input_indices.len() as u64)
    }

    fn should_verify_native_standard_locks(&self) -> bool {
        !self.native_standard_input_indices.is_empty()
    }

    fn native_only(&self) -> bool {
        self.should_verify_native_standard_locks() && self.use_native_standard_locks && !self.requires_vm_type_validation
    }

    fn verify_native_standard_locks(&self, tx: &CellTx) -> Result<u64, CellValidationError> {
        verify_native_standard_locks(tx, self.resolved_inputs.clone(), &self.native_standard_input_indices, self.per_tx_cycles_limit)
    }
}

#[cfg(feature = "vm")]
#[derive(Clone, Debug)]
pub enum CellScriptVerificationPhase {
    NativePending,
    Vm(TransactionState),
}

#[cfg(feature = "vm")]
#[derive(Clone, Debug)]
pub struct CellScriptVerificationState {
    pub native_cycles: u64,
    pub limit_cycles: u64,
    pub phase: CellScriptVerificationPhase,
}

#[cfg(feature = "vm")]
impl CellScriptVerificationState {
    fn native_pending(native_cycles: u64, limit_cycles: u64) -> Self {
        Self { native_cycles, limit_cycles, phase: CellScriptVerificationPhase::NativePending }
    }

    fn vm(native_cycles: u64, limit_cycles: u64, state: TransactionState) -> Self {
        Self { native_cycles, limit_cycles, phase: CellScriptVerificationPhase::Vm(state) }
    }

    pub fn current_cycles(&self) -> u64 {
        match &self.phase {
            CellScriptVerificationPhase::NativePending => 0,
            CellScriptVerificationPhase::Vm(state) => {
                let vm_cycles = state.state.as_ref().map(|snapshot| snapshot.total_cycles).unwrap_or(state.current_cycles);
                self.native_cycles.saturating_add(vm_cycles)
            }
        }
    }

    pub fn next_limit_cycles(&self, step_cycles: u64, max_cycles: u64) -> (u64, bool) {
        let capped_max_cycles = max_cycles.max(self.current_cycles());
        let next_limit = match &self.phase {
            CellScriptVerificationPhase::NativePending => self.limit_cycles.saturating_add(step_cycles).max(self.native_cycles),
            CellScriptVerificationPhase::Vm(state) => {
                let max_vm_cycles = capped_max_cycles.saturating_sub(self.native_cycles);
                let (next_vm_limit, _) = state.next_limit_cycles(step_cycles, max_vm_cycles);
                self.native_cycles.saturating_add(next_vm_limit)
            }
        };

        if next_limit < capped_max_cycles {
            (next_limit, false)
        } else {
            (capped_max_cycles, true)
        }
    }
}

#[cfg(feature = "vm")]
#[derive(Debug)]
pub enum CellScriptVerifyResult {
    Completed(u64),
    Suspended(CellScriptVerificationState),
}

#[cfg(feature = "vm")]
impl PreparedVmDataProvider {
    fn insert_cell(&mut self, out_point: &OutPoint, cell: ResolvedCell, header_hash: Hash) {
        self.cells.insert((out_point.tx_hash, out_point.index), cell);
        self.cell_headers.insert((out_point.tx_hash, out_point.index), header_hash.as_bytes());
    }

    fn insert_dep(&mut self, dep: &CellDep, cell: ResolvedCell, header_hash: Hash) {
        let data = cell.data.clone().unwrap_or_default();
        let code_hash = *blake3::hash(&data).as_bytes();
        self.scripts.entry(code_hash).or_insert_with(|| data);
        self.insert_cell(&dep.out_point, cell, header_hash);
    }

    fn insert_input(&mut self, out_point: &OutPoint, cell: ResolvedCell, header_hash: Hash) {
        self.insert_cell(out_point, cell, header_hash);
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

    fn load_header_by_outpoint(&self, tx_hash: &[u8; 32], index: u32) -> Option<ResolvedHeader> {
        let header_hash = self.cell_headers.get(&(*tx_hash, index))?;
        self.headers.get(header_hash).cloned()
    }

    fn load_cell_by_header(&self, header_hash: &[u8; 32]) -> Option<ResolvedCell> {
        self.cell_headers
            .iter()
            .find_map(|(out_point, mapped_header_hash)| (*mapped_header_hash == *header_hash).then(|| self.cells.get(out_point)))
            .and_then(|cell| cell.cloned())
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
        cell_validation_in_dag::validate_cell_existence_for_dep_group_abi(
            tx,
            pov,
            self.provider.as_ref(),
            self.params.dep_group_data_abi,
        )?;

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
    pub fn verify_scripts(&self, tx: &CellTx, pov: Hash, current_daa_score: u64) -> Result<(), CellValidationError>
    where
        P: CellScriptDataProvider + DagCellProvider,
    {
        self.verify_scripts_with_cycles(tx, pov, current_daa_score).map(|_| ())
    }

    /// Verify scripts (lock + type scripts) and return the total consumed cycles.
    #[cfg(feature = "vm")]
    pub fn verify_scripts_with_cycles(&self, tx: &CellTx, pov: Hash, _current_daa_score: u64) -> Result<u64, CellValidationError>
    where
        P: CellScriptDataProvider + DagCellProvider,
    {
        let plan = self.prepare_script_verification_plan(tx, pov)?;
        let native_cycles = if plan.should_verify_native_standard_locks() { plan.verify_native_standard_locks(tx)? } else { 0 };

        if plan.native_only() {
            return Ok(native_cycles);
        }

        let verifier = self.build_vm_verifier(tx, pov, &plan, native_cycles)?;
        let vm_cycles =
            verifier.verify_with_cycles().map_err(|err| map_vm_script_error(err, native_cycles, plan.per_tx_cycles_limit))?;
        let total_cycles = native_cycles.saturating_add(vm_cycles);
        ensure_total_cycles_within_limit(total_cycles, plan.per_tx_cycles_limit)?;

        Ok(total_cycles)
    }

    /// Verify scripts with a resumable transaction-level state machine.
    #[cfg(feature = "vm")]
    pub fn verify_scripts_resumable(
        &self,
        tx: &CellTx,
        pov: Hash,
        _current_daa_score: u64,
        limit_cycles: u64,
    ) -> Result<CellScriptVerifyResult, CellValidationError>
    where
        P: CellScriptDataProvider + DagCellProvider,
    {
        let plan = self.prepare_script_verification_plan(tx, pov)?;
        let effective_limit = limit_cycles.min(plan.per_tx_cycles_limit);
        let native_cycles = plan.native_cycles_budget();

        if plan.should_verify_native_standard_locks() && effective_limit < native_cycles {
            return Ok(CellScriptVerifyResult::Suspended(CellScriptVerificationState::native_pending(native_cycles, effective_limit)));
        }

        let verified_native_cycles =
            if plan.should_verify_native_standard_locks() { plan.verify_native_standard_locks(tx)? } else { 0 };

        if plan.native_only() {
            ensure_total_cycles_within_limit(verified_native_cycles, effective_limit)?;
            return Ok(CellScriptVerifyResult::Completed(verified_native_cycles));
        }

        let verifier = self.build_vm_verifier(tx, pov, &plan, verified_native_cycles)?;
        let vm_limit = effective_limit.saturating_sub(verified_native_cycles);
        match verifier.resumable_verify(vm_limit).map_err(|err| map_vm_script_error(err, verified_native_cycles, effective_limit))? {
            VerifyResult::Completed(vm_cycles) => {
                let total_cycles = verified_native_cycles.saturating_add(vm_cycles);
                ensure_total_cycles_within_limit(total_cycles, effective_limit)?;
                Ok(CellScriptVerifyResult::Completed(total_cycles))
            }
            VerifyResult::Suspended(vm_state) => Ok(CellScriptVerifyResult::Suspended(CellScriptVerificationState::vm(
                verified_native_cycles,
                effective_limit,
                vm_state,
            ))),
        }
    }

    /// Resume script verification from a previous suspended state.
    #[cfg(feature = "vm")]
    pub fn resume_scripts_from_state(
        &self,
        tx: &CellTx,
        pov: Hash,
        _current_daa_score: u64,
        state: &CellScriptVerificationState,
        limit_cycles: u64,
    ) -> Result<CellScriptVerifyResult, CellValidationError>
    where
        P: CellScriptDataProvider + DagCellProvider,
    {
        let plan = self.prepare_script_verification_plan(tx, pov)?;
        let effective_limit = limit_cycles.min(plan.per_tx_cycles_limit);
        let native_cycles = plan.native_cycles_budget();
        if native_cycles != state.native_cycles {
            return Err(CellValidationError::InvalidFormat(format!(
                "resumable script state native cycle mismatch: expected {}, got {}",
                native_cycles, state.native_cycles
            )));
        }

        match &state.phase {
            CellScriptVerificationPhase::NativePending => {
                if effective_limit < native_cycles {
                    return Ok(CellScriptVerifyResult::Suspended(CellScriptVerificationState::native_pending(
                        native_cycles,
                        effective_limit,
                    )));
                }

                let verified_native_cycles =
                    if plan.should_verify_native_standard_locks() { plan.verify_native_standard_locks(tx)? } else { 0 };

                if plan.native_only() {
                    ensure_total_cycles_within_limit(verified_native_cycles, effective_limit)?;
                    return Ok(CellScriptVerifyResult::Completed(verified_native_cycles));
                }

                let verifier = self.build_vm_verifier(tx, pov, &plan, verified_native_cycles)?;
                let vm_limit = effective_limit.saturating_sub(verified_native_cycles);
                match verifier
                    .resumable_verify(vm_limit)
                    .map_err(|err| map_vm_script_error(err, verified_native_cycles, effective_limit))?
                {
                    VerifyResult::Completed(vm_cycles) => {
                        let total_cycles = verified_native_cycles.saturating_add(vm_cycles);
                        ensure_total_cycles_within_limit(total_cycles, effective_limit)?;
                        Ok(CellScriptVerifyResult::Completed(total_cycles))
                    }
                    VerifyResult::Suspended(vm_state) => Ok(CellScriptVerifyResult::Suspended(CellScriptVerificationState::vm(
                        verified_native_cycles,
                        effective_limit,
                        vm_state,
                    ))),
                }
            }
            CellScriptVerificationPhase::Vm(vm_state) => {
                ensure_total_cycles_within_limit(state.current_cycles(), effective_limit)?;
                if plan.native_only() {
                    return Err(CellValidationError::InvalidFormat(
                        "cannot resume VM script state for a native-only verification plan".to_string(),
                    ));
                }

                let verifier = self.build_vm_verifier(tx, pov, &plan, native_cycles)?;
                let vm_limit = effective_limit.saturating_sub(native_cycles);
                match verifier
                    .resume_from_state(vm_state, vm_limit)
                    .map_err(|err| map_vm_script_error(err, native_cycles, effective_limit))?
                {
                    VerifyResult::Completed(vm_cycles) => {
                        let total_cycles = native_cycles.saturating_add(vm_cycles);
                        ensure_total_cycles_within_limit(total_cycles, effective_limit)?;
                        Ok(CellScriptVerifyResult::Completed(total_cycles))
                    }
                    VerifyResult::Suspended(next_vm_state) => Ok(CellScriptVerifyResult::Suspended(CellScriptVerificationState::vm(
                        native_cycles,
                        effective_limit,
                        next_vm_state,
                    ))),
                }
            }
        }
    }

    /// Finish a suspended verification or return a cycles-exceeded error if it still cannot complete.
    #[cfg(feature = "vm")]
    pub fn complete_scripts_from_state(
        &self,
        tx: &CellTx,
        pov: Hash,
        _current_daa_score: u64,
        state: &CellScriptVerificationState,
        max_cycles: u64,
    ) -> Result<u64, CellValidationError>
    where
        P: CellScriptDataProvider + DagCellProvider,
    {
        let plan = self.prepare_script_verification_plan(tx, pov)?;
        let effective_limit = max_cycles.min(plan.per_tx_cycles_limit);
        let native_cycles = plan.native_cycles_budget();
        if native_cycles != state.native_cycles {
            return Err(CellValidationError::InvalidFormat(format!(
                "resumable script state native cycle mismatch: expected {}, got {}",
                native_cycles, state.native_cycles
            )));
        }

        match &state.phase {
            CellScriptVerificationPhase::NativePending => {
                if effective_limit < native_cycles {
                    return Err(CellValidationError::ExceededMaxCycles { total: native_cycles, limit: effective_limit });
                }
                let verified_native_cycles =
                    if plan.should_verify_native_standard_locks() { plan.verify_native_standard_locks(tx)? } else { 0 };
                if plan.native_only() {
                    ensure_total_cycles_within_limit(verified_native_cycles, effective_limit)?;
                    return Ok(verified_native_cycles);
                }

                let verifier = self.build_vm_verifier(tx, pov, &plan, verified_native_cycles)?;
                let vm_cycles =
                    verifier.verify_with_cycles().map_err(|err| map_vm_script_error(err, verified_native_cycles, effective_limit))?;
                let total_cycles = verified_native_cycles.saturating_add(vm_cycles);
                ensure_total_cycles_within_limit(total_cycles, effective_limit)?;
                Ok(total_cycles)
            }
            CellScriptVerificationPhase::Vm(vm_state) => {
                ensure_total_cycles_within_limit(state.current_cycles(), effective_limit)?;
                if plan.native_only() {
                    return Err(CellValidationError::InvalidFormat(
                        "cannot complete VM script state for a native-only verification plan".to_string(),
                    ));
                }

                let verifier = self.build_vm_verifier(tx, pov, &plan, native_cycles)?;
                let vm_limit = effective_limit.saturating_sub(native_cycles);
                let vm_cycles =
                    verifier.complete(vm_state, vm_limit).map_err(|err| map_vm_script_error(err, native_cycles, effective_limit))?;
                let total_cycles = native_cycles.saturating_add(vm_cycles);
                ensure_total_cycles_within_limit(total_cycles, effective_limit)?;
                Ok(total_cycles)
            }
        }
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
        let total_cycles = self.verify_scripts_with_cycles(tx, pov, daa_score)?;

        Ok(total_cycles)
    }

    /// Full validation with resumable script verification.
    #[cfg(feature = "vm")]
    pub fn validate_full_with_scripts_resumable(
        &self,
        tx: &spora_exec::CellTx,
        pov: Hash,
        daa_score: u64,
        timestamp: u64,
        limit_cycles: u64,
    ) -> Result<CellScriptVerifyResult, CellValidationError>
    where
        P: cell_validation_in_dag::DagCellProvider + CellScriptDataProvider,
    {
        self.validate_full(tx, pov, daa_score, timestamp)?;
        self.verify_scripts_resumable(tx, pov, daa_score, limit_cycles)
    }

    /// Resume a previous full validation with suspended script verification state.
    #[cfg(feature = "vm")]
    pub fn resume_full_with_scripts_from_state(
        &self,
        tx: &spora_exec::CellTx,
        pov: Hash,
        daa_score: u64,
        timestamp: u64,
        state: &CellScriptVerificationState,
        limit_cycles: u64,
    ) -> Result<CellScriptVerifyResult, CellValidationError>
    where
        P: cell_validation_in_dag::DagCellProvider + CellScriptDataProvider,
    {
        self.validate_full(tx, pov, daa_score, timestamp)?;
        self.resume_scripts_from_state(tx, pov, daa_score, state, limit_cycles)
    }

    /// Finish a previous full validation with suspended script verification state.
    #[cfg(feature = "vm")]
    pub fn complete_full_with_scripts_from_state(
        &self,
        tx: &spora_exec::CellTx,
        pov: Hash,
        daa_score: u64,
        timestamp: u64,
        state: &CellScriptVerificationState,
        max_cycles: u64,
    ) -> Result<u64, CellValidationError>
    where
        P: cell_validation_in_dag::DagCellProvider + CellScriptDataProvider,
    {
        self.validate_full(tx, pov, daa_score, timestamp)?;
        self.complete_scripts_from_state(tx, pov, daa_score, state, max_cycles)
    }

    #[cfg(feature = "vm")]
    fn resolve_input_metadata(&self, tx: &CellTx, pov: Hash) -> Result<Vec<CellMetadata>, CellValidationError>
    where
        P: DagCellProvider,
    {
        tx.inputs
            .iter()
            .map(|input| {
                self.provider
                    .get_cell_at_pov(&input.previous_output, pov)
                    .map_err(CellValidationError::InvalidFormat)?
                    .ok_or(CellValidationError::CellNotFound(input.previous_output.tx_hash))
            })
            .collect()
    }

    #[cfg(feature = "vm")]
    fn prepare_script_verification_plan(&self, tx: &CellTx, pov: Hash) -> Result<ScriptVerificationPlan, CellValidationError>
    where
        P: DagCellProvider,
    {
        let resolved_inputs = self.resolve_input_metadata(tx, pov)?;
        Ok(ScriptVerificationPlan {
            per_tx_cycles_limit: self.params.max_tx_cycles.min(self.params.max_block_cycles),
            use_native_standard_locks: uses_native_standard_signature_verification(tx, &resolved_inputs),
            requires_vm_type_validation: has_any_type_scripts(tx, &resolved_inputs),
            native_standard_input_indices: collect_native_standard_input_indices(tx, &resolved_inputs),
            resolved_inputs,
        })
    }

    #[cfg(feature = "vm")]
    fn build_vm_verifier(
        &self,
        tx: &CellTx,
        pov: Hash,
        plan: &ScriptVerificationPlan,
        native_cycles: u64,
    ) -> Result<TransactionScriptVerifier<PreparedVmDataProvider>, CellValidationError>
    where
        P: CellScriptDataProvider + DagCellProvider,
    {
        let provider = Arc::new(self.prepare_vm_data_provider_with_resolved_inputs(tx, pov, &plan.resolved_inputs)?);
        let vm_cycles_limit = plan.per_tx_cycles_limit.saturating_sub(native_cycles);
        let mut verifier = TransactionScriptVerifier::new(Arc::new(tx.clone()), provider).with_max_cycles(vm_cycles_limit);
        if plan.should_verify_native_standard_locks() {
            let native_standard_lock_hashes =
                collect_lock_hashes_for_indices(&plan.resolved_inputs, &plan.native_standard_input_indices);
            verifier = verifier.with_skip_lock_script_hashes(native_standard_lock_hashes);
        }
        Ok(verifier)
    }

    #[cfg(feature = "vm")]
    fn prepare_vm_data_provider_with_resolved_inputs(
        &self,
        tx: &CellTx,
        pov: Hash,
        resolved_inputs: &[CellMetadata],
    ) -> Result<PreparedVmDataProvider, CellValidationError>
    where
        P: CellScriptDataProvider + DagCellProvider,
    {
        let mut provider = PreparedVmDataProvider::default();

        for dep in &tx.cell_deps {
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
                    provider.insert_dep(dep, metadata_to_resolved_cell(metadata.clone(), Some(data))?, metadata.block_hash);
                }
                DepType::DepGroup => {
                    // Preserve the dep-group cell itself for Source::CellDep syscalls, then
                    // expand the referenced outpoints as code deps for script loading.
                    let group_metadata = self
                        .provider
                        .get_cell_at_pov(&dep.out_point, pov)
                        .map_err(CellValidationError::InvalidFormat)?
                        .ok_or(CellValidationError::CellNotFound(dep.out_point.tx_hash))?;
                    let group_data = self
                        .provider
                        .get_cell_data(&dep.out_point, pov)
                        .map_err(CellValidationError::InvalidFormat)?
                        .ok_or(CellValidationError::CellNotFound(dep.out_point.tx_hash))?;
                    provider.insert_cell(
                        &dep.out_point,
                        metadata_to_resolved_cell(group_metadata.clone(), Some(group_data.clone()))?,
                        group_metadata.block_hash,
                    );
                    let outpoints = spora_exec::parse_dep_group_data_for_abi(&group_data, self.params.dep_group_data_abi)
                        .map_err(CellValidationError::InvalidFormat)?;
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
                        provider.insert_dep(&code_dep, metadata_to_resolved_cell(metadata.clone(), Some(data))?, metadata.block_hash);
                    }
                }
            }
        }

        for (input, metadata) in tx.inputs.iter().zip(resolved_inputs.iter()) {
            let data = self.provider.get_cell_data(&input.previous_output, pov).map_err(CellValidationError::InvalidFormat)?;
            provider.insert_input(&input.previous_output, metadata_to_resolved_cell(metadata.clone(), data)?, metadata.block_hash);
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
fn metadata_to_resolved_cell(metadata: CellMetadata, data: Option<Vec<u8>>) -> Result<ResolvedCell, CellValidationError> {
    let lock_script = metadata
        .lock_script
        .ok_or_else(|| CellValidationError::InvalidFormat(format!("missing lock script for resolved cell {}", metadata.out_point)))?;
    Ok(ResolvedCell {
        cell_output: spora_exec::CellOutput { lock: lock_script, type_: metadata.type_script, capacity: metadata.capacity },
        data: data.or(metadata.data),
    })
}

#[cfg(feature = "vm")]
fn uses_native_standard_signature_verification(tx: &CellTx, resolved_inputs: &[CellMetadata]) -> bool {
    tx.inputs.len() == resolved_inputs.len()
        && resolved_inputs.iter().all(|metadata| {
            metadata.lock_script.as_ref().is_some_and(|lock_script| {
                lock_script.hash_type <= 4
                    && matches!(classify_script(lock_script), ScriptClass::StdSingle | ScriptClass::StdSingleECDSA)
            })
        })
}

#[cfg(feature = "vm")]
fn collect_native_standard_input_indices(tx: &CellTx, resolved_inputs: &[CellMetadata]) -> Vec<usize> {
    if tx.inputs.len() != resolved_inputs.len() {
        return Vec::new();
    }

    resolved_inputs
        .iter()
        .enumerate()
        .filter_map(|(index, metadata)| {
            let lock_script = metadata.lock_script.as_ref()?;
            if lock_script.hash_type <= 4
                && matches!(classify_script(lock_script), ScriptClass::StdSingle | ScriptClass::StdSingleECDSA)
            {
                Some(index)
            } else {
                None
            }
        })
        .collect()
}

#[cfg(feature = "vm")]
fn collect_lock_hashes_for_indices(resolved_inputs: &[CellMetadata], indices: &[usize]) -> HashSet<[u8; 32]> {
    indices
        .iter()
        .filter_map(|index| resolved_inputs.get(*index).and_then(|metadata| metadata.lock_script.as_ref()).map(|script| script.hash()))
        .collect()
}

#[cfg(feature = "vm")]
fn has_any_type_scripts(tx: &CellTx, resolved_inputs: &[CellMetadata]) -> bool {
    tx.outputs.iter().any(|output| output.type_.is_some()) || resolved_inputs.iter().any(|metadata| metadata.type_script.is_some())
}

#[cfg(feature = "vm")]
fn verify_native_standard_locks(
    tx: &CellTx,
    resolved_inputs: Vec<CellMetadata>,
    native_standard_input_indices: &[usize],
    native_cycles_limit: u64,
) -> Result<u64, CellValidationError> {
    if native_standard_input_indices.is_empty() {
        return Ok(0);
    }

    let mut signable_tx = MutableTransaction::with_resolved_metadata(tx.clone(), resolved_inputs.clone());
    signable_tx.entries = resolved_inputs.into_iter().map(|metadata| Some(metadata.to_cell_meta())).collect();
    let verifiable = signable_tx.as_verifiable();
    let reused_values = SigHashReusedValuesUnsync::new();
    let secp = secp256k1::Secp256k1::new();
    let mut native_cycles = 0u64;

    for input_index in native_standard_input_indices.iter().copied() {
        let projected_cycles = native_cycles.saturating_add(native_standard_lock_cycles_per_input());
        if projected_cycles > native_cycles_limit {
            return Err(CellValidationError::ExceededMaxCycles { total: projected_cycles, limit: native_cycles_limit });
        }

        let witness = tx.witnesses.get(input_index).map(Vec::as_slice).unwrap_or_default();
        let parsed = parse_standard_witness_envelope(witness, input_index)
            .map_err(|err| CellValidationError::ScriptVerificationFailed(err.to_string()))?;
        let lock_script = verifiable.cell_metadata(input_index).and_then(|metadata| metadata.lock_script).ok_or_else(|| {
            CellValidationError::ScriptVerificationFailed(format!(
                "cannot verify native standard signature for input {input_index}: missing lock script metadata"
            ))
        })?;

        match classify_script(&lock_script) {
            ScriptClass::StdSingle => {
                if parsed.pubkey.len() != 32 {
                    return Err(CellValidationError::ScriptVerificationFailed(format!(
                        "invalid standard schnorr pubkey length for input {input_index}: expected 32"
                    )));
                }
                if lock_script.args != key_id20(KEY_ID_DOMAIN_SCHNORR, &parsed.pubkey) {
                    return Err(CellValidationError::ScriptVerificationFailed(format!(
                        "standard schnorr key-id mismatch for input {input_index}"
                    )));
                }
                let pk = secp256k1::XOnlyPublicKey::from_slice(&parsed.pubkey).map_err(|_| CellValidationError::InvalidSignature)?;
                let sig =
                    secp256k1::schnorr::Signature::from_slice(&parsed.signature).map_err(|_| CellValidationError::InvalidSignature)?;
                let sig_hash = calc_schnorr_signature_hash(&verifiable, input_index, parsed.hash_type, &reused_values);
                let msg = secp256k1::Message::from_digest_slice(sig_hash.as_bytes().as_slice())
                    .map_err(|_| CellValidationError::InvalidSignature)?;
                sig.verify(&msg, &pk).map_err(|_| CellValidationError::InvalidSignature)?;
            }
            ScriptClass::StdSingleECDSA => {
                if parsed.pubkey.len() != 33 {
                    return Err(CellValidationError::ScriptVerificationFailed(format!(
                        "invalid standard ecdsa pubkey length for input {input_index}: expected 33"
                    )));
                }
                if lock_script.args != key_id20(KEY_ID_DOMAIN_ECDSA, &parsed.pubkey) {
                    return Err(CellValidationError::ScriptVerificationFailed(format!(
                        "standard ecdsa key-id mismatch for input {input_index}"
                    )));
                }
                let pk = secp256k1::PublicKey::from_slice(&parsed.pubkey).map_err(|_| CellValidationError::InvalidSignature)?;
                let mut sig =
                    secp256k1::ecdsa::Signature::from_compact(&parsed.signature).map_err(|_| CellValidationError::InvalidSignature)?;
                let original = sig.serialize_compact();
                sig.normalize_s();
                if sig.serialize_compact() != original {
                    return Err(CellValidationError::ScriptVerificationFailed(format!(
                        "non-canonical ecdsa signature (high-S) for input {input_index}"
                    )));
                }
                let sig_hash = calc_ecdsa_signature_hash(&verifiable, input_index, parsed.hash_type, &reused_values);
                let msg = secp256k1::Message::from_digest_slice(sig_hash.as_bytes().as_slice())
                    .map_err(|_| CellValidationError::InvalidSignature)?;
                secp.verify_ecdsa(&msg, &sig, &pk).map_err(|_| CellValidationError::InvalidSignature)?;
            }
            other => {
                return Err(CellValidationError::ScriptVerificationFailed(format!(
                    "cannot verify native standard signature for input {input_index}: unsupported lock script class {other}"
                )));
            }
        }
        native_cycles = projected_cycles;
    }

    Ok(native_cycles)
}

#[cfg(feature = "vm")]
fn ensure_total_cycles_within_limit(total_cycles: u64, limit: u64) -> Result<(), CellValidationError> {
    if total_cycles > limit {
        return Err(CellValidationError::ExceededMaxCycles { total: total_cycles, limit });
    }
    Ok(())
}

#[cfg(feature = "vm")]
fn map_vm_script_error(error: ScriptError, native_cycles: u64, total_limit: u64) -> CellValidationError {
    match error {
        ScriptError::VM(VMError::CyclesExceeded { actual, .. }) => {
            CellValidationError::ExceededMaxCycles { total: native_cycles.saturating_add(actual), limit: total_limit }
        }
        other => CellValidationError::ScriptVerificationFailed(other.to_string()),
    }
}

#[cfg(all(test, feature = "vm"))]
mod vm_tests {
    use super::*;
    use crate::processes::cell_validator::cell_validation_in_dag::DagCellProvider;
    use spora_consensus_core::tx::TransactionOutpoint;
    use spora_exec::{CellInput, CellOutput, Script};
    use spora_hashes::Hash;
    use std::collections::HashMap;

    struct MockVmProvider {
        cells: HashMap<(Hash, OutPoint), CellMetadata>,
    }

    impl CellStateProvider for MockVmProvider {
        fn is_cell_available(&self, out_point: &OutPoint, pov: Hash) -> Result<bool, String> {
            Ok(self.cells.contains_key(&(pov, *out_point)))
        }

        fn get_cell_capacity(&self, out_point: &OutPoint, pov: Hash) -> Result<Option<u64>, String> {
            Ok(self.cells.get(&(pov, *out_point)).map(|meta| meta.capacity))
        }
    }

    impl DagCellProvider for MockVmProvider {
        fn get_cell_at_pov(&self, out_point: &OutPoint, pov: Hash) -> Result<Option<CellMetadata>, String> {
            Ok(self.cells.get(&(pov, *out_point)).cloned())
        }

        fn get_block_timestamp(&self, _block_hash: Hash) -> Result<u64, String> {
            Ok(0)
        }
    }

    impl CellScriptDataProvider for MockVmProvider {
        fn get_cell_data(&self, out_point: &OutPoint, pov: Hash) -> Result<Option<Vec<u8>>, String> {
            Ok(self.cells.get(&(pov, *out_point)).and_then(|meta| meta.data.clone()))
        }

        fn get_header(&self, block_hash: Hash) -> Result<Option<ResolvedHeader>, String> {
            Ok(Some(ResolvedHeader {
                hash: block_hash.as_bytes(),
                version: 1,
                parents_by_level: vec![],
                hash_merkle_root: [0; 32],
                accepted_id_merkle_root: [0; 32],
                cell_commitment: [0; 32],
                cell_root: [0; 32],
                segment_root: [0; 32],
                timestamp: 0,
                bits: 0,
                nonce: 0,
                daa_score: 0,
                blue_work: [0; 24],
                blue_score: 0,
                pruning_point: [0; 32],
            }))
        }
    }

    fn tx_outpoint(out_point: &OutPoint) -> TransactionOutpoint {
        TransactionOutpoint { tx_hash: out_point.tx_hash, index: out_point.index }
    }

    #[test]
    fn prepared_vm_data_provider_preserves_dep_group_cells_and_expands_code_deps() {
        let pov = Hash::from_bytes([0x11; 32]);
        let block_hash = Hash::from_bytes([0x22; 32]);
        let input_out_point = OutPoint::new([0x31; 32], 0);
        let dep_group_out_point = OutPoint::new([0x32; 32], 0);
        let expanded_dep_out_point = OutPoint::new([0x33; 32], 1);
        let input_lock = Script::new([0x44; 32], 0, vec![]);
        let dep_group_data = spora_exec::encode_dep_group_data(&[expanded_dep_out_point]);
        let expanded_dep_data = vec![0xDE, 0xAD, 0xBE, 0xEF];

        let mut provider = MockVmProvider { cells: HashMap::new() };
        provider.cells.insert(
            (pov, input_out_point),
            CellMetadata {
                out_point: tx_outpoint(&input_out_point),
                capacity: 1_000,
                data_bytes: 0,
                lock_hash: input_lock.hash(),
                type_hash: None,
                data_hash: [0; 32],
                block_daa_score: 0,
                is_cellbase: false,
                block_hash,
                lock_code_hash: None,
                type_code_hash: None,
                lock_script: Some(input_lock.clone()),
                type_script: None,
                data: Some(vec![]),
            },
        );
        provider.cells.insert(
            (pov, dep_group_out_point),
            CellMetadata {
                out_point: tx_outpoint(&dep_group_out_point),
                capacity: 2_000,
                data_bytes: dep_group_data.len() as u64,
                lock_hash: [0; 32],
                type_hash: None,
                data_hash: [0; 32],
                block_daa_score: 0,
                is_cellbase: false,
                block_hash,
                lock_code_hash: None,
                type_code_hash: None,
                lock_script: Some(Script::new([0x55; 32], 0, vec![])),
                type_script: None,
                data: Some(dep_group_data.clone()),
            },
        );
        provider.cells.insert(
            (pov, expanded_dep_out_point),
            CellMetadata {
                out_point: tx_outpoint(&expanded_dep_out_point),
                capacity: 3_000,
                data_bytes: expanded_dep_data.len() as u64,
                lock_hash: [0; 32],
                type_hash: None,
                data_hash: [0; 32],
                block_daa_score: 0,
                is_cellbase: false,
                block_hash,
                lock_code_hash: None,
                type_code_hash: None,
                lock_script: Some(Script::new([0x66; 32], 0, vec![])),
                type_script: None,
                data: Some(expanded_dep_data.clone()),
            },
        );

        let tx = CellTx::new(
            vec![CellInput::new(input_out_point, 0)],
            vec![CellDep { out_point: dep_group_out_point, dep_type: DepType::DepGroup }],
            vec![CellOutput { capacity: 1_000, lock: input_lock, type_: None }],
            vec![vec![]],
            vec![vec![]],
        )
        .unwrap();

        let validator = CellValidator::new(Arc::new(CellConsensusParams::default()), Arc::new(provider));
        let resolved_inputs = validator.resolve_input_metadata(&tx, pov).expect("input metadata should resolve");
        let prepared = validator
            .prepare_vm_data_provider_with_resolved_inputs(&tx, pov, &resolved_inputs)
            .expect("prepared vm data provider should build");

        let dep_group_cell = prepared
            .load_cell_by_outpoint(&dep_group_out_point.tx_hash, dep_group_out_point.index)
            .expect("dep-group cell should remain visible to runtime");
        assert_eq!(dep_group_cell.data, Some(dep_group_data.clone()));

        let expanded_dep_cell = prepared
            .load_cell_by_outpoint(&expanded_dep_out_point.tx_hash, expanded_dep_out_point.index)
            .expect("expanded dep cell should be visible to runtime");
        assert_eq!(expanded_dep_cell.data, Some(expanded_dep_data.clone()));

        let expanded_code_hash = *blake3::hash(&expanded_dep_data).as_bytes();
        assert_eq!(prepared.load_cell_data(&expanded_code_hash), Some(expanded_dep_data));

        let dep_group_code_hash = *blake3::hash(&dep_group_data).as_bytes();
        assert_eq!(prepared.load_cell_data(&dep_group_code_hash), None);
    }
}

impl<P: CellStateProvider> Default for CellValidator<P>
where
    P: Default,
{
    fn default() -> Self {
        Self::new(Arc::new(CellConsensusParams::default()), Arc::new(P::default()))
    }
}
