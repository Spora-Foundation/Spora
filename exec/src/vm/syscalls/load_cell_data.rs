// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Load cell data syscall

use super::utils::{store_data, INDEX_OUT_OF_BOUND, SUCCESS};
use crate::celltx::CellTx;
use crate::vm::CellDataProvider;
use ckb_vm::{
    registers::{A0, A2, A3, A4, A7},
    Error as VMError, Register, SupportMachine, Syscalls,
};
use std::sync::Arc;

/// Syscall: Load Cell Data
///
/// Syscall number: 2092
pub struct LoadCellData<D: CellDataProvider> {
    tx: Arc<CellTx>,
    provider: Arc<D>,
    group_input_indices: Vec<usize>,
    group_output_indices: Vec<usize>,
}

impl<D: CellDataProvider> LoadCellData<D> {
    pub fn new(tx: Arc<CellTx>, provider: Arc<D>, group_input_indices: Vec<usize>, group_output_indices: Vec<usize>) -> Self {
        Self { tx, provider, group_input_indices, group_output_indices }
    }

    fn get_cell_data(&self, source: u64, index: usize) -> Option<Vec<u8>> {
        match source {
            0x01 => {
                let input = self.tx.inputs.get(index)?;
                self.provider.load_cell_by_outpoint(&input.out_point.tx_hash, input.out_point.index)?.data
            }
            0x02 => {
                // Output
                self.tx.outputs_data.get(index).cloned()
            }
            0x03 => {
                let dep = self.tx.deps.get(index)?;
                self.provider.load_cell_by_outpoint(&dep.out_point.tx_hash, dep.out_point.index)?.data
            }
            0x0100 => {
                let input_index = *self.group_input_indices.get(index)?;
                let input = self.tx.inputs.get(input_index)?;
                self.provider.load_cell_by_outpoint(&input.out_point.tx_hash, input.out_point.index)?.data
            }
            0x0200 => {
                // GroupOutput
                self.group_output_indices.get(index).and_then(|&idx| self.tx.outputs_data.get(idx).cloned())
            }
            _ => None,
        }
    }
}

impl<D: CellDataProvider, M: SupportMachine> Syscalls<M> for LoadCellData<D> {
    fn initialize(&mut self, _machine: &mut M) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut M) -> Result<bool, VMError> {
        let syscall_number = machine.registers()[A7].to_u64();

        // LOAD_CELL_DATA = 2092
        if syscall_number != 2092 {
            return Ok(false);
        }

        let offset = machine.registers()[A2].to_u64();
        let index = machine.registers()[A3].to_u64() as usize;
        let source = machine.registers()[A4].to_u64();

        // Get cell data
        let cell_data = match self.get_cell_data(source, index) {
            Some(d) => d,
            None => {
                machine.set_register(A0, M::REG::from_u8(INDEX_OUT_OF_BOUND));
                return Ok(true);
            }
        };

        // Handle offset (for partial reads)
        let data_to_store = if (offset as usize) < cell_data.len() { &cell_data[(offset as usize)..] } else { &[] };

        // Store data using CKB-style store_data
        store_data(machine, data_to_store)?;
        machine.set_register(A0, M::REG::from_u8(SUCCESS));

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::celltx::{CellDep, CellOut, CellRef, DepType, OutPoint, ScriptRef};
    use crate::vm::{ResolvedCell, SimpleDataProvider};

    #[test]
    fn test_load_cell_data_resolves_input_and_dep_sources() {
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

        let syscall = LoadCellData::new(tx, Arc::new(provider), vec![0], vec![0]);
        assert_eq!(syscall.get_cell_data(0x01, 0).unwrap(), vec![0x10, 0x20]);
        assert_eq!(syscall.get_cell_data(0x03, 0).unwrap(), vec![0x30, 0x40, 0x50]);
        assert_eq!(syscall.get_cell_data(0x02, 0).unwrap(), vec![0xAA; 10]);
    }
}
