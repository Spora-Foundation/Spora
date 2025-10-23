// SPDX-License-Identifier: ISC
// Copyright (C) 2025 Spora developers
//
// Cell transaction script verifier
// Reference: ckb/script/src/verify.rs

use super::error::{ScriptError, ScriptResult, VMError};
use super::machine::{Machine, ScriptVersion, VmContext, run_script};
use super::syscalls::{LoadTx};
use crate::celltx::{CellTx, ScriptRef};
use std::sync::Arc;
use ckb_vm::{DefaultMachineRunner, Syscalls};

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
    pub script: ScriptRef,
    /// Group type
    pub group_type: ScriptGroupType,
    /// Input indices referencing this script
    pub input_indices: Vec<usize>,
    /// Output indices referencing this script
    pub output_indices: Vec<usize>,
}

/// Cell data provider trait (for loading cell data)
pub trait CellDataProvider {
    /// Load cell data by script code hash
    fn load_cell_data(&self, code_hash: &[u8; 32]) -> Option<Vec<u8>>;
    
    /// Load cell by outpoint (for deps)
    fn load_cell_by_outpoint(&self, tx_hash: &[u8; 32], index: u32) -> Option<Vec<u8>>;
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
}

impl<D: CellDataProvider> TransactionScriptVerifier<D> {
    /// Create a new verifier
    pub fn new(
        tx: Arc<CellTx>,
        data_provider: Arc<D>,
    ) -> Self {
        Self {
            tx,
            data_provider,
            version: ScriptVersion::latest(),
            max_cycles: 10_000_000, // 10M cycles default
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

    /// Extract script groups from transaction
    pub fn extract_script_groups(&self) -> Vec<ScriptGroup> {
        use std::collections::HashMap;
        
        let mut lock_groups: HashMap<[u8; 32], ScriptGroup> = HashMap::new();
        let mut type_groups: HashMap<[u8; 32], ScriptGroup> = HashMap::new();

        // Group inputs by lock script
        // Note: In Cell model, we need to resolve inputs to get their lock scripts
        // For now, we'll work with outputs which we have direct access to
        for (i, output) in self.tx.outputs.iter().enumerate() {
            let lock_hash = output.lock.hash();
            
            lock_groups.entry(lock_hash)
                .or_insert_with(|| ScriptGroup {
                    script: output.lock.clone(),
                    group_type: ScriptGroupType::Lock,
                    input_indices: vec![],
                    output_indices: vec![],
                })
                .output_indices.push(i);
        }

        // Group by type script
        for (i, output) in self.tx.outputs.iter().enumerate() {
            if let Some(ref type_script) = output.type_ {
                let type_hash = type_script.hash();
                
                type_groups.entry(type_hash)
                    .or_insert_with(|| ScriptGroup {
                        script: type_script.clone(),
                        group_type: ScriptGroupType::Type,
                        input_indices: vec![],
                        output_indices: vec![],
                    })
                    .output_indices.push(i);
            }
        }

        // Combine all groups
        lock_groups.into_values()
            .chain(type_groups.into_values())
            .collect()
    }

    /// Verify all scripts in the transaction
    pub fn verify(&self) -> ScriptResult<()> {
        let script_groups = self.extract_script_groups();

        // Verify each script group
        for group in script_groups {
            self.verify_script_group(&group)?;
        }

        Ok(())
    }

    /// Verify a single script group
    fn verify_script_group(&self, group: &ScriptGroup) -> ScriptResult<()> {
        // Load script code from data provider
        let script_code = self.data_provider
            .load_cell_data(&group.script.code_hash)
            .ok_or_else(|| ScriptError::ScriptNotFound(group.script.code_hash))?;

        // Prepare arguments
        let args = vec![group.script.args.clone()];

        // Create syscalls
        let syscalls = self.build_syscalls(group);

        // Create VM context
        let context = VmContext::new(self.version, self.max_cycles);

        // Run script
        let cycles = run_script(
            &script_code,
            &args,
            syscalls,
            &context,
        ).map_err(ScriptError::VM)?;

        log::debug!(
            "Script group {:?} verified successfully, cycles: {}",
            group.group_type,
            cycles
        );

        Ok(())
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
            group.input_indices.clone(),
            group.output_indices.clone(),
        )));
        syscalls.push(Box::new(LoadCellData::new(
            Arc::clone(&self.tx),
            group.output_indices.clone(),
        )));
        syscalls.push(Box::new(LoadInput::new(
            Arc::clone(&self.tx),
            group.input_indices.clone(),
        )));
        syscalls.push(Box::new(LoadWitness::new(
            Arc::clone(&self.tx),
            group.input_indices.clone(),
        )));
        syscalls.push(Box::new(LoadScript::new(
            Arc::new(group.script.clone()),
        )));
        syscalls.push(Box::new(LoadHeader::new()));
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
}

impl SimpleDataProvider {
    pub fn new() -> Self {
        Self {
            scripts: std::collections::HashMap::new(),
        }
    }

    pub fn add_script(&mut self, code_hash: [u8; 32], code: Vec<u8>) {
        self.scripts.insert(code_hash, code);
    }
}

impl CellDataProvider for SimpleDataProvider {
    fn load_cell_data(&self, code_hash: &[u8; 32]) -> Option<Vec<u8>> {
        self.scripts.get(code_hash).cloned()
    }

    fn load_cell_by_outpoint(&self, _tx_hash: &[u8; 32], _index: u32) -> Option<Vec<u8>> {
        None // TODO: implement
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::celltx::{CellTx, CellRef, CellOut, OutPoint};

    #[test]
    fn test_verifier_creation() {
        let tx = Arc::new(CellTx {
            ver: 0xC001,
            inputs: vec![],
            deps: vec![],
            outputs: vec![],
            outputs_data: vec![],
            witnesses: vec![],
        });

        let provider = Arc::new(SimpleDataProvider::new());
        let verifier = TransactionScriptVerifier::new(tx, provider);

        assert_eq!(verifier.version, ScriptVersion::latest());
        assert_eq!(verifier.max_cycles, 10_000_000);
    }
}

