// SPDX-License-Identifier: ISC
// Copyright (C) 2025 Spora developers
//
// Load cell data syscall

use super::utils::{store_data, INDEX_OUT_OF_BOUND, SUCCESS};
use crate::celltx::CellTx;
use ckb_vm::{
    registers::{A0, A2, A3, A4, A7},
    Error as VMError, Register, SupportMachine, Syscalls,
};
use std::sync::Arc;

/// Syscall: Load Cell Data
///
/// Syscall number: 2092
pub struct LoadCellData {
    tx: Arc<CellTx>,
    group_output_indices: Vec<usize>,
}

impl LoadCellData {
    pub fn new(tx: Arc<CellTx>, group_output_indices: Vec<usize>) -> Self {
        Self { tx, group_output_indices }
    }

    fn get_cell_data(&self, source: u64, index: usize) -> Option<&[u8]> {
        match source {
            0x02 => {
                // Output
                self.tx.outputs_data.get(index).map(|d| d.as_slice())
            }
            0x0200 => {
                // GroupOutput
                self.group_output_indices.get(index).and_then(|&idx| self.tx.outputs_data.get(idx).map(|d| d.as_slice()))
            }
            _ => None,
        }
    }
}

impl<M: SupportMachine> Syscalls<M> for LoadCellData {
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
