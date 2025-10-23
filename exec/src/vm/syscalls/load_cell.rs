// SPDX-License-Identifier: ISC
// Copyright (C) 2025 Spora developers
//
// Load cell syscall
// Reference: ckb/script/src/syscalls/load_cell.rs

use super::utils::{store_data, SUCCESS, INDEX_OUT_OF_BOUND, ITEM_MISSING};
use crate::celltx::{CellTx, CellOut, ScriptRef};
use ckb_vm::{
    Register, Syscalls, SupportMachine,
    Error as VMError,
    registers::{A0, A2, A3, A4, A5, A7},
};
use std::sync::Arc;

/// Cell field selector
#[repr(u64)]
pub enum CellField {
    Capacity = 0,
    DataHash = 1,
    Lock = 2,
    LockHash = 3,
    Type = 4,
    TypeHash = 5,
    OccupiedCapacity = 6,
}

/// Source type
#[repr(u64)]
pub enum Source {
    Input = 0x01,
    Output = 0x02,
    CellDep = 0x03,
    HeaderDep = 0x04,
    GroupInput = 0x0100,
    GroupOutput = 0x0200,
}

/// Syscall: Load Cell
///
/// Syscall number: 2071
///
/// Load cell data by source and index
pub struct LoadCell {
    tx: Arc<CellTx>,
    group_input_indices: Vec<usize>,
    group_output_indices: Vec<usize>,
}

impl LoadCell {
    pub fn new(
        tx: Arc<CellTx>,
        group_input_indices: Vec<usize>,
        group_output_indices: Vec<usize>,
    ) -> Self {
        Self {
            tx,
            group_input_indices,
            group_output_indices,
        }
    }

    fn get_cell_output(&self, source: u64, index: usize) -> Option<&CellOut> {
        match source {
            0x01 => {
                // Input: need to resolve from deps or state
                // TODO: implement input cell resolution
                None
            }
            0x02 => {
                // Output
                self.tx.outputs.get(index)
            }
            0x0100 => {
                // GroupInput
                self.group_input_indices.get(index)
                    .and_then(|&idx| {
                        // TODO: resolve input cell
                        None
                    })
            }
            0x0200 => {
                // GroupOutput
                self.group_output_indices.get(index)
                    .and_then(|&idx| self.tx.outputs.get(idx))
            }
            _ => None,
        }
    }

    fn serialize_cell_field(&self, cell: &CellOut, field: u64) -> Option<Vec<u8>> {
        match field {
            0 => {
                // Capacity
                Some(cell.capacity.to_le_bytes().to_vec())
            }
            1 => {
                // DataHash (need output data)
                // TODO: hash the corresponding output_data
                None
            }
            2 => {
                // Lock script
                Some(self.serialize_script(&cell.lock))
            }
            3 => {
                // LockHash
                Some(cell.lock.hash().to_vec())
            }
            4 => {
                // Type script
                cell.type_.as_ref().map(|s| self.serialize_script(s))
            }
            5 => {
                // TypeHash
                cell.type_.as_ref().map(|s| s.hash().to_vec())
            }
            6 => {
                // OccupiedCapacity
                // TODO: calculate occupied capacity
                Some(0u64.to_le_bytes().to_vec())
            }
            _ => None,
        }
    }

    fn serialize_script(&self, script: &ScriptRef) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&script.code_hash);
        data.push(script.hash_type);
        data.extend_from_slice(&(script.args.len() as u32).to_le_bytes());
        data.extend_from_slice(&script.args);
        data
    }
}

impl<M: SupportMachine> Syscalls<M> for LoadCell {
    fn initialize(&mut self, _machine: &mut M) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut M) -> Result<bool, VMError> {
        let syscall_number = machine.registers()[A7].to_u64();
        
        // LOAD_CELL = 2071 or LOAD_CELL_BY_FIELD = 2081
        if syscall_number != 2071 && syscall_number != 2081 {
            return Ok(false);
        }

        // Args (store_data reads A0, A1, A2):
        // A2: offset
        // A3: index
        // A4: source
        // A5: field (only for 2081)
        let offset = machine.registers()[A2].to_u64();
        let index = machine.registers()[A3].to_u64() as usize;
        let source = machine.registers()[A4].to_u64();

        if offset != 0 {
            machine.set_register(A0, M::REG::from_u8(INDEX_OUT_OF_BOUND));
            return Ok(true);
        }

        // Get cell
        let cell = match self.get_cell_output(source, index) {
            Some(c) => c,
            None => {
                machine.set_register(A0, M::REG::from_u8(INDEX_OUT_OF_BOUND));
                return Ok(true);
            }
        };

        // Get field data
        let data = if syscall_number == 2081 {
            // LOAD_CELL_BY_FIELD
            let field = machine.registers()[A5].to_u64();
            match self.serialize_cell_field(cell, field) {
                Some(d) => d,
                None => {
                    machine.set_register(A0, M::REG::from_u8(ITEM_MISSING));
                    return Ok(true);
                }
            }
        } else {
            // LOAD_CELL (full cell data)
            // TODO: serialize full CellOut
            Vec::new()
        };

        // Store data using CKB-style store_data
        store_data(machine, &data)?;
        machine.set_register(A0, M::REG::from_u8(SUCCESS));
        
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::celltx::{CellTx, CellOut, ScriptRef};

    #[test]
    fn test_load_cell_creation() {
        let tx = Arc::new(CellTx {
            ver: 0xC001,
            inputs: vec![],
            deps: vec![],
            outputs: vec![
                CellOut {
                    capacity: 1000,
                    lock: ScriptRef::new([1u8; 32], 0, vec![]),
                    type_: None,
                }
            ],
            outputs_data: vec![vec![0xAA; 10]],
            witnesses: vec![],
        });

        let syscall = LoadCell::new(tx, vec![], vec![0]);
        // Just ensure it compiles
    }
}
