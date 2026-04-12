// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Load cell syscall
// Reference: ckb/script/src/syscalls/load_cell.rs

use super::utils::{store_data, INDEX_OUT_OF_BOUND, ITEM_MISSING, SUCCESS};
use crate::celltx::{CellTx, ScriptRef};
use crate::vm::{CellDataProvider, ResolvedCell};
use ckb_vm::{
    registers::{A0, A2, A3, A4, A5, A7},
    Error as VMError, Register, SupportMachine, Syscalls,
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
pub struct LoadCell<D: CellDataProvider> {
    tx: Arc<CellTx>,
    provider: Arc<D>,
    group_input_indices: Vec<usize>,
    group_output_indices: Vec<usize>,
}

impl<D: CellDataProvider> LoadCell<D> {
    pub fn new(tx: Arc<CellTx>, provider: Arc<D>, group_input_indices: Vec<usize>, group_output_indices: Vec<usize>) -> Self {
        Self { tx, provider, group_input_indices, group_output_indices }
    }

    fn resolve_cell(&self, source: u64, index: usize) -> Option<ResolvedCell> {
        match source {
            0x01 => {
                let input = self.tx.inputs.get(index)?;
                self.provider.load_cell_by_outpoint(&input.out_point.tx_hash, input.out_point.index)
            }
            0x02 => self
                .tx
                .outputs
                .get(index)
                .cloned()
                .map(|cell_output| ResolvedCell { cell_output, data: self.tx.outputs_data.get(index).cloned() }),
            0x03 => {
                let dep = self.tx.deps.get(index)?;
                self.provider.load_cell_by_outpoint(&dep.out_point.tx_hash, dep.out_point.index)
            }
            0x0100 => {
                let input_index = *self.group_input_indices.get(index)?;
                let input = self.tx.inputs.get(input_index)?;
                self.provider.load_cell_by_outpoint(&input.out_point.tx_hash, input.out_point.index)
            }
            0x0200 => {
                let output_index = *self.group_output_indices.get(index)?;
                self.tx
                    .outputs
                    .get(output_index)
                    .cloned()
                    .map(|cell_output| ResolvedCell { cell_output, data: self.tx.outputs_data.get(output_index).cloned() })
            }
            _ => None,
        }
    }

    fn serialize_cell_field(&self, cell: &ResolvedCell, field: u64) -> Option<Vec<u8>> {
        match field {
            0 => {
                // Capacity
                Some(cell.cell_output.capacity.to_le_bytes().to_vec())
            }
            1 => {
                // DataHash (need output data)
                let data = cell.data.as_deref().unwrap_or(&[]);
                Some(if data.is_empty() {
                    [0u8; 32].to_vec()
                } else {
                    let mut hasher = blake3::Hasher::new();
                    hasher.update(b"spora-cell/data");
                    hasher.update(data);
                    hasher.finalize().as_bytes().to_vec()
                })
            }
            2 => {
                // Lock script
                Some(self.serialize_script(&cell.cell_output.lock))
            }
            3 => {
                // LockHash
                Some(cell.cell_output.lock.hash().to_vec())
            }
            4 => {
                // Type script
                cell.cell_output.type_.as_ref().map(|s| self.serialize_script(s))
            }
            5 => {
                // TypeHash
                cell.cell_output.type_.as_ref().map(|s| s.hash().to_vec())
            }
            6 => {
                // OccupiedCapacity
                let data_len = cell.data.as_ref().map_or(0, Vec::len);
                Some(cell.cell_output.occupied_capacity(data_len).to_le_bytes().to_vec())
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

impl<D: CellDataProvider, M: SupportMachine> Syscalls<M> for LoadCell<D> {
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
        let cell = match self.resolve_cell(source, index) {
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
            match self.serialize_cell_field(&cell, field) {
                Some(d) => d,
                None => {
                    machine.set_register(A0, M::REG::from_u8(ITEM_MISSING));
                    return Ok(true);
                }
            }
        } else {
            // LOAD_CELL (full cell data)
            borsh::to_vec(&cell.cell_output).map_err(|e| VMError::Unexpected(format!("Failed to serialize CellOut: {e}")))?
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
    use crate::celltx::{CellDep, CellOut, CellRef, DepType, OutPoint};
    use crate::vm::{ResolvedCell, SimpleDataProvider};

    #[test]
    fn test_load_cell_creation() {
        let input_out_point = OutPoint::new([7u8; 32], 0);
        let dep_out_point = OutPoint::new([8u8; 32], 1);
        let tx = Arc::new(CellTx {
            ver: 0xC001,
            inputs: vec![CellRef::new(input_out_point.clone(), 0)],
            deps: vec![CellDep { out_point: dep_out_point.clone(), dep_type: DepType::Code }],
            header_deps: vec![],
            outputs: vec![CellOut { capacity: 1000, lock: ScriptRef::new([1u8; 32], 0, vec![]), type_: None }],
            outputs_data: vec![vec![0xAA; 10]],
            witnesses: vec![],
        });
        let provider = Arc::new(SimpleDataProvider::new());

        let _syscall = LoadCell::new(tx, provider, vec![0], vec![0]);
        // Just ensure it compiles
    }

    #[test]
    fn test_load_cell_resolves_input_and_dep_sources() {
        let input_out_point = OutPoint::new([7u8; 32], 0);
        let dep_out_point = OutPoint::new([8u8; 32], 1);
        let tx = Arc::new(CellTx {
            ver: 0xC001,
            inputs: vec![CellRef::new(input_out_point.clone(), 0)],
            deps: vec![CellDep { out_point: dep_out_point.clone(), dep_type: DepType::Code }],
            header_deps: vec![],
            outputs: vec![CellOut { capacity: 1000, lock: ScriptRef::new([1u8; 32], 0, vec![]), type_: None }],
            outputs_data: vec![vec![0xAA; 10]],
            witnesses: vec![],
        });
        let mut provider = SimpleDataProvider::new();
        provider.add_cell(
            input_out_point.tx_hash,
            input_out_point.index,
            ResolvedCell {
                cell_output: CellOut { capacity: 2000, lock: ScriptRef::new([2u8; 32], 0, vec![0x11]), type_: None },
                data: Some(vec![0x10, 0x20]),
            },
        );
        provider.add_cell(
            dep_out_point.tx_hash,
            dep_out_point.index,
            ResolvedCell {
                cell_output: CellOut { capacity: 3000, lock: ScriptRef::new([3u8; 32], 0, vec![0x22]), type_: None },
                data: Some(vec![0x30, 0x40, 0x50]),
            },
        );

        let syscall = LoadCell::new(tx, Arc::new(provider), vec![0], vec![0]);
        let input_cell = syscall.resolve_cell(0x01, 0).expect("resolved input cell");
        let dep_cell = syscall.resolve_cell(0x03, 0).expect("resolved dep cell");
        assert_eq!(input_cell.cell_output.capacity, 2000);
        assert_eq!(dep_cell.cell_output.capacity, 3000);
        assert_eq!(syscall.serialize_cell_field(&input_cell, 6).unwrap(), input_cell.cell_output.occupied_capacity(2).to_le_bytes());
    }
}
