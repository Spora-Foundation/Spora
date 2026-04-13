// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Cell transaction script verifier
// Reference: ckb/script/src/verify.rs

use super::error::{ScriptError, ScriptResult};
use super::machine::{run_script, Machine, ScriptVersion, VmContext};
use super::{MAX_SCRIPT_SIZE, MAX_VM_MEMORY};
use crate::celltx::{CellOutput, CellTx, Script};
use borsh::{BorshDeserialize, BorshSerialize};
use ckb_vm::{DefaultMachineRunner, Syscalls};
use rayon::prelude::*;
use std::sync::Arc;

/// Script group type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptGroupType {
    /// Lock script (who can spend)
    Lock,
    /// Type script (state transition rules)
    Type,
}

/// Script group: cells sharing the same script
#[derive(Debug, Clone)]
pub struct ScriptGroup {
    /// The script
    pub script: Script,
    /// Group type
    pub group_type: ScriptGroupType,
    /// Input indices referencing this script
    pub input_indices: Vec<usize>,
    /// Output indices referencing this script
    pub output_indices: Vec<usize>,
}

/// Fully resolved cell contents available to the VM runtime.
#[derive(Debug, Clone)]
pub struct ResolvedCell {
    /// Full cell output structure.
    pub cell_output: CellOutput,
    /// Optional associated cell data.
    pub data: Option<Vec<u8>>,
}

/// Fully resolved header contents available to the VM runtime.
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct ResolvedHeader {
    /// Header hash.
    pub hash: [u8; 32],
    /// Header version.
    pub version: u32,
    /// Parent hashes grouped by DAG level.
    pub parents_by_level: Vec<Vec<[u8; 32]>>,
    /// Transaction hash merkle root.
    pub hash_merkle_root: [u8; 32],
    /// Accepted transaction ID merkle root.
    pub accepted_id_merkle_root: [u8; 32],
    /// Execution-related state commitment.
    pub cell_commitment: [u8; 32],
    /// Cell state root.
    pub cell_root: [u8; 32],
    /// Data-availability segment root.
    pub segment_root: [u8; 32],
    /// Timestamp in milliseconds.
    pub timestamp: u64,
    /// Compact difficulty bits.
    pub bits: u32,
    /// Mining nonce.
    pub nonce: u64,
    /// DAA score.
    pub daa_score: u64,
    /// Accumulated blue work encoded as little-endian Uint192 bytes.
    pub blue_work: [u8; 24],
    /// Blue score.
    pub blue_score: u64,
    /// Pruning-point hash.
    pub pruning_point: [u8; 32],
}

impl ResolvedHeader {
    /// Returns direct parent hashes (level 0 of the DAG parent set).
    pub fn direct_parents(&self) -> &[[u8; 32]] {
        self.parents_by_level.first().map(Vec::as_slice).unwrap_or(&[])
    }
}

/// Cell data provider trait (for loading cell data)
pub trait CellDataProvider: Send + Sync + 'static {
    /// Load cell data by script code hash
    fn load_cell_data(&self, code_hash: &[u8; 32]) -> Option<Vec<u8>>;

    /// Load a fully resolved cell by outpoint.
    fn load_cell_by_outpoint(&self, tx_hash: &[u8; 32], index: u32) -> Option<ResolvedCell>;

    /// Load a fully resolved header by hash.
    fn load_header(&self, hash: &[u8; 32]) -> Option<ResolvedHeader>;
}

/// Transaction script verifier
pub struct TransactionScriptVerifier<D: CellDataProvider> {
    /// The transaction to verify
    tx: Arc<CellTx>,
    /// Cell data provider
    data_provider: Arc<D>,
    /// Script version
    version: ScriptVersion,
    /// Max cycles per script
    max_cycles: u64,
    /// Max VM memory per script execution.
    max_memory: usize,
    /// Max script binary size.
    max_script_size: usize,
}

impl<D: CellDataProvider> TransactionScriptVerifier<D> {
    /// Create a new verifier
    pub fn new(tx: Arc<CellTx>, data_provider: Arc<D>) -> Self {
        Self {
            tx,
            data_provider,
            version: ScriptVersion::latest(),
            max_cycles: 10_000_000, // 10M cycles default
            max_memory: MAX_VM_MEMORY,
            max_script_size: MAX_SCRIPT_SIZE,
        }
    }

    /// Set script version
    pub fn with_version(mut self, version: ScriptVersion) -> Self {
        self.version = version;
        self
    }

    /// Set max cycles
    pub fn with_max_cycles(mut self, max_cycles: u64) -> Self {
        self.max_cycles = max_cycles;
        self
    }

    /// Set max VM memory
    pub fn with_max_memory(mut self, max_memory: usize) -> Self {
        self.max_memory = max_memory;
        self
    }

    /// Set max script size
    pub fn with_max_script_size(mut self, max_script_size: usize) -> Self {
        self.max_script_size = max_script_size;
        self
    }

    /// Extract script groups from transaction
    pub fn extract_script_groups(&self) -> ScriptResult<Vec<ScriptGroup>> {
        use std::collections::BTreeMap;

        let mut lock_groups: BTreeMap<[u8; 32], ScriptGroup> = BTreeMap::new();
        let mut type_groups: BTreeMap<[u8; 32], ScriptGroup> = BTreeMap::new();

        // Lock scripts execute against resolved input cells.
        for (i, input) in self.tx.inputs.iter().enumerate() {
            let resolved =
                self.data_provider.load_cell_by_outpoint(&input.previous_output.tx_hash, input.previous_output.index).ok_or_else(|| {
                    ScriptError::VM(super::error::VMError::ItemMissing(format!(
                        "missing resolved input cell {:02x?}:{}",
                        input.previous_output.tx_hash, input.previous_output.index
                    )))
                })?;
            let lock_hash = resolved.cell_output.lock.hash();

            lock_groups
                .entry(lock_hash)
                .or_insert_with(|| ScriptGroup {
                    script: resolved.cell_output.lock.clone(),
                    group_type: ScriptGroupType::Lock,
                    input_indices: vec![],
                    output_indices: vec![],
                })
                .input_indices
                .push(i);

            if let Some(ref type_script) = resolved.cell_output.type_ {
                let type_hash = type_script.hash();

                type_groups
                    .entry(type_hash)
                    .or_insert_with(|| ScriptGroup {
                        script: type_script.clone(),
                        group_type: ScriptGroupType::Type,
                        input_indices: vec![],
                        output_indices: vec![],
                    })
                    .input_indices
                    .push(i);
            }
        }

        // Type scripts execute over both consumed and created cells.
        for (i, output) in self.tx.outputs.iter().enumerate() {
            if let Some(ref type_script) = output.type_ {
                let type_hash = type_script.hash();

                type_groups
                    .entry(type_hash)
                    .or_insert_with(|| ScriptGroup {
                        script: type_script.clone(),
                        group_type: ScriptGroupType::Type,
                        input_indices: vec![],
                        output_indices: vec![],
                    })
                    .output_indices
                    .push(i);
            }
        }

        Ok(lock_groups.into_values().chain(type_groups.into_values()).collect())
    }

    /// Verify all scripts in the transaction
    pub fn verify(&self) -> ScriptResult<()> {
        self.verify_with_cycles().map(|_| ())
    }

    /// Verify all scripts in the transaction and return the total consumed cycles.
    pub fn verify_with_cycles(&self) -> ScriptResult<u64> {
        let script_groups = self.extract_script_groups()?;
        let group_results: Vec<ScriptResult<u64>> = script_groups.par_iter().map(|group| self.verify_script_group(group)).collect();

        // Keep error selection deterministic by folding results in the stable
        // script-group order produced by extract_script_groups.
        group_results
            .into_iter()
            .try_fold(0u64, |total_cycles, group_cycles| group_cycles.map(|cycles| total_cycles.saturating_add(cycles)))
    }

    /// Verify a single script group
    fn verify_script_group(&self, group: &ScriptGroup) -> ScriptResult<u64> {
        if group.script.hash_type != 0 {
            return Err(ScriptError::InvalidHashType(group.script.hash_type));
        }

        // Load script code from data provider
        let script_code = self
            .data_provider
            .load_cell_data(&group.script.code_hash)
            .ok_or_else(|| ScriptError::ScriptNotFound(group.script.code_hash))?;

        // Prepare arguments
        let args = vec![group.script.args.clone()];

        // Create syscalls
        let syscalls = self.build_syscalls(group);

        // Create VM context
        let context = VmContext::with_limits(self.version, self.max_cycles, self.max_memory, self.max_script_size);

        // Run script
        let cycles = run_script(&script_code, &args, syscalls, &context).map_err(ScriptError::VM)?;

        log::debug!("Script group {:?} verified successfully, cycles: {}", group.group_type, cycles);

        Ok(cycles)
    }

    /// Build syscalls for a script group
    fn build_syscalls(&self, group: &ScriptGroup) -> Vec<Box<dyn Syscalls<<Machine as DefaultMachineRunner>::Inner>>> {
        use super::syscalls::*;

        let mut syscalls: Vec<Box<dyn Syscalls<<Machine as DefaultMachineRunner>::Inner>>> = Vec::new();

        // CKB standard syscalls
        // Compute tx hash using our sighash function
        let tx_hash = crate::celltx::compute_txid(&self.tx);
        syscalls.push(Box::new(LoadTx::new(tx_hash)));
        syscalls.push(Box::new(LoadCell::new(
            Arc::clone(&self.tx),
            Arc::clone(&self.data_provider),
            group.input_indices.clone(),
            group.output_indices.clone(),
        )));
        syscalls.push(Box::new(LoadCellData::new(
            Arc::clone(&self.tx),
            Arc::clone(&self.data_provider),
            group.input_indices.clone(),
            group.output_indices.clone(),
        )));
        syscalls.push(Box::new(LoadInput::new(Arc::clone(&self.tx), group.input_indices.clone())));
        syscalls.push(Box::new(LoadWitness::new(Arc::clone(&self.tx), group.input_indices.clone())));
        syscalls.push(Box::new(LoadScript::new(Arc::new(group.script.clone()))));
        syscalls.push(Box::new(LoadHeader::new(Arc::clone(&self.tx), Arc::clone(&self.data_provider))));
        syscalls.push(Box::new(CurrentCycles::new()));
        syscalls.push(Box::new(Debugger::new(group.script.code_hash)));

        // Spora extensions
        syscalls.push(Box::new(Blake3Hash::new()));

        syscalls
    }
}

/// Simple in-memory cell data provider (for testing)
pub struct SimpleDataProvider {
    scripts: std::collections::HashMap<[u8; 32], Vec<u8>>,
    cells: std::collections::HashMap<([u8; 32], u32), ResolvedCell>,
    headers: std::collections::HashMap<[u8; 32], ResolvedHeader>,
}

impl SimpleDataProvider {
    pub fn new() -> Self {
        Self {
            scripts: std::collections::HashMap::new(),
            cells: std::collections::HashMap::new(),
            headers: std::collections::HashMap::new(),
        }
    }

    pub fn add_script(&mut self, code_hash: [u8; 32], code: Vec<u8>) {
        self.scripts.insert(code_hash, code);
    }

    pub fn add_cell(&mut self, tx_hash: [u8; 32], index: u32, cell: ResolvedCell) {
        self.cells.insert((tx_hash, index), cell);
    }

    pub fn add_header(&mut self, hash: [u8; 32], header: ResolvedHeader) {
        self.headers.insert(hash, header);
    }
}

impl CellDataProvider for SimpleDataProvider {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::celltx::{CellInput, OutPoint};

    #[test]
    fn test_verifier_creation() {
        let tx = Arc::new(CellTx {
            version: 0xC001,
            inputs: vec![],
            cell_deps: vec![],
            header_deps: vec![],
            outputs: vec![],
            outputs_data: vec![],
            witnesses: vec![],
        });

        let provider = Arc::new(SimpleDataProvider::new());
        let verifier = TransactionScriptVerifier::new(tx, provider);

        assert_eq!(verifier.version, ScriptVersion::latest());
        assert_eq!(verifier.max_cycles, 10_000_000);
    }

    #[test]
    fn test_extract_script_groups_uses_resolved_input_locks() {
        let input_lock = Script::new([1u8; 32], 0, vec![0xAA]);
        let output_lock = Script::new([2u8; 32], 0, vec![0xBB]);
        let input_out_point = OutPoint::new([9u8; 32], 0);
        let tx = Arc::new(
            CellTx::new(
                vec![CellInput::new(input_out_point.clone(), 0)],
                vec![],
                vec![CellOutput { capacity: 1000, lock: output_lock.clone(), type_: None }],
                vec![vec![]],
                vec![],
            )
            .unwrap(),
        );

        let mut provider = SimpleDataProvider::new();
        provider.add_cell(
            input_out_point.tx_hash,
            input_out_point.index,
            ResolvedCell { cell_output: CellOutput { capacity: 1000, lock: input_lock.clone(), type_: None }, data: Some(vec![]) },
        );

        let groups = TransactionScriptVerifier::new(tx, Arc::new(provider)).extract_script_groups().unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].group_type, ScriptGroupType::Lock);
        assert_eq!(groups[0].script, input_lock);
        assert_eq!(groups[0].input_indices, vec![0]);
        assert!(groups[0].output_indices.is_empty());
    }

    #[test]
    fn test_verify_rejects_unsupported_hash_type() {
        let tx = Arc::new(CellTx::new(vec![CellInput::new(OutPoint::new([7u8; 32], 0), 0)], vec![], vec![], vec![], vec![]).unwrap());
        let mut provider = SimpleDataProvider::new();
        provider.add_cell(
            [7u8; 32],
            0,
            ResolvedCell {
                cell_output: CellOutput { capacity: 1000, lock: Script::new([3u8; 32], 1, vec![]), type_: None },
                data: Some(vec![]),
            },
        );

        let err = TransactionScriptVerifier::new(tx, Arc::new(provider)).verify().unwrap_err();
        assert!(matches!(err, ScriptError::InvalidHashType(1)));
    }

    #[test]
    fn test_extract_script_groups_merges_type_inputs_and_outputs() {
        let input_lock = Script::new([1u8; 32], 0, vec![0xAA]);
        let shared_type = Script::new([4u8; 32], 0, vec![0xCC]);
        let input_out_point = OutPoint::new([9u8; 32], 0);
        let tx = Arc::new(
            CellTx::new(
                vec![CellInput::new(input_out_point.clone(), 0)],
                vec![],
                vec![CellOutput { capacity: 1000, lock: Script::new([2u8; 32], 0, vec![0xBB]), type_: Some(shared_type.clone()) }],
                vec![vec![]],
                vec![],
            )
            .unwrap(),
        );

        let mut provider = SimpleDataProvider::new();
        provider.add_cell(
            input_out_point.tx_hash,
            input_out_point.index,
            ResolvedCell {
                cell_output: CellOutput { capacity: 1000, lock: input_lock, type_: Some(shared_type.clone()) },
                data: Some(vec![]),
            },
        );

        let groups = TransactionScriptVerifier::new(tx, Arc::new(provider)).extract_script_groups().unwrap();
        let type_group = groups.into_iter().find(|group| group.group_type == ScriptGroupType::Type).expect("type group");
        assert_eq!(type_group.script, shared_type);
        assert_eq!(type_group.input_indices, vec![0]);
        assert_eq!(type_group.output_indices, vec![0]);
    }
}
