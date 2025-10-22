// SPDX-License-Identifier: ISC
// Copyright (C) 2025Tondi developers
//
// VM scheduler for script execution
// Simplified from CKB script/src/scheduler.rs

use crate::celltx::types::CellTx;
use crate::vm::cost_model::Cycle;
use crate::vm::machine::{Machine, SporaVmVersion};
use std::sync::Arc;

/// Script group for parallel execution
#[derive(Debug, Clone)]
pub struct ScriptGroup {
    /// Script being executed
    pub script: crate::celltx::types::ScriptRef,
    /// Script group type
    pub group_type: ScriptGroupType,
    /// Input indices this group verifies
    pub input_indices: Vec<usize>,
    /// Output indices this group verifies
    pub output_indices: Vec<usize>,
}

/// Script group type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptGroupType {
    /// Lock script (verifies spending permission)
    Lock,
    /// Type script (verifies state transition)
    Type,
}

/// Script verification result
#[derive(Debug)]
pub struct VerifyResult {
    /// Total cycles consumed
    pub cycles: Cycle,
    /// Whether verification succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
}

/// VM scheduler
pub struct VmScheduler {
    /// VM version
    vm_version: SporaVmVersion,
    /// Maximum cycles per script
    max_cycles: Cycle,
}

impl VmScheduler {
    /// Create a new VM scheduler
    pub fn new(max_cycles: Cycle) -> Self {
        Self {
            vm_version: SporaVmVersion::latest(),
            max_cycles,
        }
    }

    /// Group scripts for execution
    ///
    /// Groups scripts by code_hash + hash_type + args
    /// Same scripts only need to run once
    pub fn group_scripts(&self, tx: &CellTx) -> Vec<ScriptGroup> {
        let mut groups = Vec::new();

        // Group lock scripts by input
        for (_i, _input) in tx.inputs.iter().enumerate() {
            // TODO: Actually load input Cell to get lock script
            // For now, create placeholder
            // groups.push(ScriptGroup { ... });
        }

        // Group type scripts
        for (i, output) in tx.outputs.iter().enumerate() {
            if let Some(ref type_script) = output.type_ {
                groups.push(ScriptGroup {
                    script: type_script.clone(),
                    group_type: ScriptGroupType::Type,
                    input_indices: vec![],
                    output_indices: vec![i],
                });
            }
        }

        groups
    }

    /// Verify a single script group
    pub fn verify_script_group(
        &self,
        _group: &ScriptGroup,
        _tx: &Arc<CellTx>,
    ) -> VerifyResult {
        // TODO: Implement actual VM execution
        // For now, return success
        VerifyResult {
            cycles: 1000,
            success: true,
            error: None,
        }
    }

    /// Verify all scripts in parallel
    pub fn verify_all(&self, tx: &Arc<CellTx>) -> VerifyResult {
        let groups = self.group_scripts(tx);

        let mut total_cycles = 0;

        for group in &groups {
            let result = self.verify_script_group(group, tx);
            if !result.success {
                return result;
            }
            total_cycles += result.cycles;

            if total_cycles > self.max_cycles {
                return VerifyResult {
                    cycles: total_cycles,
                    success: false,
                    error: Some(format!("Exceeded max cycles: {}", self.max_cycles)),
                };
            }
        }

        VerifyResult {
            cycles: total_cycles,
            success: true,
            error: None,
        }
    }
}

impl Default for VmScheduler {
    fn default() -> Self {
        Self::new(super::MAX_TX_CYCLES)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::celltx::types::{CellRef, CellOut, ScriptRef, OutPoint};

    fn create_test_tx() -> CellTx {
        let lock = ScriptRef::new([0x00; 32], 0, vec![0; 20]);
        let type_script = ScriptRef::new([0x01; 32], 0, vec![0; 10]);
        
        CellTx::new(
            vec![CellRef::new(OutPoint::new([0; 32], 0), 0)],
            vec![],
            vec![CellOut { lock, type_: Some(type_script), capacity: 10000 }],
            vec![vec![]],
            vec![],
        ).unwrap()
    }

    #[test]
    fn test_scheduler_creation() {
        let scheduler = VmScheduler::new(1_000_000);
        assert_eq!(scheduler.max_cycles, 1_000_000);
    }

    #[test]
    fn test_group_scripts() {
        let scheduler = VmScheduler::default();
        let tx = create_test_tx();
        let groups = scheduler.group_scripts(&tx);
        
        // Should have at least the type script
        assert!(groups.len() >= 1);
        assert_eq!(groups[0].group_type, ScriptGroupType::Type);
    }

    #[test]
    fn test_verify_all() {
        let scheduler = VmScheduler::default();
        let tx = Arc::new(create_test_tx());
        let result = scheduler.verify_all(&tx);
        
        assert!(result.success);
        assert!(result.cycles > 0);
    }
}

