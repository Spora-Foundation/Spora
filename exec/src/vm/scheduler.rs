// SPDX-License-Identifier: ISC
// Copyright (C) 2025Tondi developers
//
// VM scheduler for script execution
// Reference: CKB script/src/scheduler.rs and script/src/verify.rs

use crate::celltx::types::{CellTx, ScriptRef, ResolvedCellTx};
use crate::vm::cost_model::Cycle;
use crate::vm::machine::{Machine, SporaVmVersion};
use std::collections::BTreeMap;
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

    /// Group scripts for execution (CKB-compatible)
    ///
    /// Grouping rules (from CKB):
    /// 1. Lock scripts: Group inputs by lock script hash
    ///    - Each unique lock script forms one group
    ///    - Only inputs are included (outputs don't have lock verification)
    /// 2. Type scripts: Group inputs + outputs by type script hash  
    ///    - Inputs: validate cell destruction
    ///    - Outputs: validate cell creation
    ///
    /// **Determinism**: BTreeMap ensures consistent iteration order
    pub fn group_scripts(&self, resolved_tx: &ResolvedCellTx) -> Vec<ScriptGroup> {
        let mut lock_groups: BTreeMap<[u8; 32], ScriptGroup> = BTreeMap::new();
        let mut type_groups: BTreeMap<[u8; 32], ScriptGroup> = BTreeMap::new();

        // Group lock scripts from inputs
        for (i, input_meta) in resolved_tx.resolved_inputs.iter().enumerate() {
            let lock_hash = input_meta.cell_output.lock.hash();
            
            lock_groups.entry(lock_hash)
                .or_insert_with(|| ScriptGroup {
                    script: input_meta.cell_output.lock.clone(),
                    group_type: ScriptGroupType::Lock,
                    input_indices: Vec::new(),
                    output_indices: Vec::new(),
                })
                .input_indices.push(i);
        }

        // Group type scripts from inputs
        for (i, input_meta) in resolved_tx.resolved_inputs.iter().enumerate() {
            if let Some(ref type_script) = input_meta.cell_output.type_ {
                let type_hash = type_script.hash();
                
                type_groups.entry(type_hash)
                    .or_insert_with(|| ScriptGroup {
                        script: type_script.clone(),
                        group_type: ScriptGroupType::Type,
                        input_indices: Vec::new(),
                        output_indices: Vec::new(),
                    })
                    .input_indices.push(i);
            }
        }

        // Group type scripts from outputs
        for (i, output) in resolved_tx.transaction.outputs.iter().enumerate() {
            if let Some(ref type_script) = output.type_ {
                let type_hash = type_script.hash();
                
                type_groups.entry(type_hash)
                    .or_insert_with(|| ScriptGroup {
                        script: type_script.clone(),
                        group_type: ScriptGroupType::Type,
                        input_indices: Vec::new(),
                        output_indices: Vec::new(),
                    })
                    .output_indices.push(i);
            }
        }

        // Collect all groups in deterministic order (BTreeMap iteration is sorted)
        let mut all_groups = Vec::new();
        
        // Lock groups first (CKB convention)
        all_groups.extend(lock_groups.into_values());
        
        // Then type groups
        all_groups.extend(type_groups.into_values());

        all_groups
    }

    /// Verify a single script group
    /// 
    /// TODO: Full CKB-VM execution
    /// For now, this is a placeholder that returns success
    pub fn verify_script_group(
        &self,
        group: &ScriptGroup,
        _resolved_tx: &Arc<ResolvedCellTx>,
    ) -> VerifyResult {
        // Placeholder cycles based on group size
        let base_cycles = 1000u64;
        let per_cell_cycles = 100u64;
        let num_cells = group.input_indices.len() + group.output_indices.len();
        let estimated_cycles = base_cycles + (per_cell_cycles * num_cells as u64);
        
        // TODO: Actual implementation:
        // 1. Load script code from deps
        // 2. Initialize CKB-VM machine with script
        // 3. Set up syscalls (LoadCell, LoadInput, LoadWitness, etc.)
        // 4. Execute VM until completion or cycles limit
        // 5. Check return code (0 = success)
        
        VerifyResult {
            cycles: estimated_cycles,
            success: true,
            error: None,
        }
    }

    /// Verify all scripts (sequential for now, parallel in future)
    /// 
    /// Note: CKB executes script groups in parallel, we execute sequentially
    /// for simplicity. Parallel execution can be added as optimization later.
    pub fn verify_all(&self, resolved_tx: &Arc<ResolvedCellTx>) -> VerifyResult {
        let groups = self.group_scripts(resolved_tx);

        let mut total_cycles = 0;

        // Execute each group sequentially
        // TODO(optimization): Parallel execution of independent groups
        for group in &groups {
            let result = self.verify_script_group(group, resolved_tx);
            if !result.success {
                return result;
            }
            
            // Accumulate cycles (with overflow check)
            total_cycles = match total_cycles.checked_add(result.cycles) {
                Some(sum) => sum,
                None => {
                    return VerifyResult {
                        cycles: u64::MAX,
                        success: false,
                        error: Some("Cycles overflow".to_string()),
                    };
                }
            };

            // Check cycles limit
            if total_cycles > self.max_cycles {
                return VerifyResult {
                    cycles: total_cycles,
                    success: false,
                    error: Some(format!("Exceeded max cycles: {} > {}", total_cycles, self.max_cycles)),
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
    use crate::celltx::types::{CellRef, CellOut, CellMeta, ScriptRef, OutPoint, TransactionInfo};

    fn create_test_resolved_tx() -> ResolvedCellTx {
        let lock = ScriptRef::new([0x00; 32], 0, vec![0; 20]);
        let type_script = ScriptRef::new([0x01; 32], 0, vec![0; 10]);
        
        let tx = CellTx::new(
            vec![CellRef::new(OutPoint::new([0; 32], 0), 0)],
            vec![],
            vec![CellOut { lock: lock.clone(), type_: Some(type_script), capacity: 10000 }],
            vec![vec![]],
            vec![],
        ).unwrap();

        // Create a resolved input with lock script
        let input_cell_out = CellOut { lock: lock.clone(), type_: None, capacity: 10000 };
        let input_meta = CellMeta {
            cell_output: input_cell_out,
            out_point: OutPoint::new([0; 32], 0),
            transaction_info: Some(TransactionInfo {
                tx_hash: [0; 32],
                daa_score: 100,
                block_hash: [1; 32],
                is_cellbase: false,
            }),
            data_bytes: 0,
            mem_cell_data: None,
            mem_cell_data_hash: None,
        };

        ResolvedCellTx {
            transaction: tx,
            resolved_inputs: vec![input_meta],
            resolved_deps: vec![],
        }
    }

    #[test]
    fn test_scheduler_creation() {
        let scheduler = VmScheduler::new(1_000_000);
        assert_eq!(scheduler.max_cycles, 1_000_000);
    }

    #[test]
    fn test_group_scripts() {
        let scheduler = VmScheduler::default();
        let resolved_tx = create_test_resolved_tx();
        let groups = scheduler.group_scripts(&resolved_tx);
        
        // Should have lock group (from input) and type group (from output)
        assert!(groups.len() >= 2);
        
        // First should be lock group
        assert_eq!(groups[0].group_type, ScriptGroupType::Lock);
        assert_eq!(groups[0].input_indices, vec![0]);
        
        // Second should be type group
        assert_eq!(groups[1].group_type, ScriptGroupType::Type);
        assert_eq!(groups[1].output_indices, vec![0]);
    }

    #[test]
    fn test_verify_all() {
        let scheduler = VmScheduler::default();
        let resolved_tx = Arc::new(create_test_resolved_tx());
        let result = scheduler.verify_all(&resolved_tx);
        
        assert!(result.success);
        assert!(result.cycles > 0);
    }

    #[test]
    fn test_cycles_overflow() {
        let scheduler = VmScheduler::new(100); // Very low limit
        let resolved_tx = Arc::new(create_test_resolved_tx());
        let result = scheduler.verify_all(&resolved_tx);
        
        // Should fail due to exceeding cycles
        assert!(!result.success);
        assert!(result.error.is_some());
    }
}

