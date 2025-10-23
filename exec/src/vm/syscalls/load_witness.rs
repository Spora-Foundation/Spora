// SPDX-License-Identifier: ISC
// Copyright (C) 2025 Spora developers
//
// Load witness syscall

use super::utils::{store_data, INDEX_OUT_OF_BOUND, SUCCESS};
use crate::celltx::CellTx;
use ckb_vm::{
    registers::{A0, A2, A3, A4, A7},
    Error as VMError, Register, SupportMachine, Syscalls,
};
use std::sync::Arc;

/// Syscall: Load Witness
///
/// Syscall number: 2074
pub struct LoadWitness {
    tx: Arc<CellTx>,
    group_input_indices: Vec<usize>,
}

impl LoadWitness {
    pub fn new(tx: Arc<CellTx>, group_input_indices: Vec<usize>) -> Self {
        Self { tx, group_input_indices }
    }

    fn get_witness(&self, source: u64, index: usize) -> Option<&[u8]> {
        match source {
            0x01 => {
                // Input witnesses
                self.tx.witnesses.get(index).map(|w| w.as_slice())
            }
            0x0100 => {
                // GroupInput witnesses
                self.group_input_indices.get(index).and_then(|&idx| self.tx.witnesses.get(idx).map(|w| w.as_slice()))
            }
            _ => None,
        }
    }
}

impl<M: SupportMachine> Syscalls<M> for LoadWitness {
    fn initialize(&mut self, _machine: &mut M) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut M) -> Result<bool, VMError> {
        let syscall_number = machine.registers()[A7].to_u64();

        // LOAD_WITNESS = 2074
        if syscall_number != 2074 {
            return Ok(false);
        }

        let offset = machine.registers()[A2].to_u64();
        let index = machine.registers()[A3].to_u64() as usize;
        let source = machine.registers()[A4].to_u64();

        if offset != 0 {
            machine.set_register(A0, M::REG::from_u8(INDEX_OUT_OF_BOUND));
            return Ok(true);
        }

        // Get witness data
        let witness = match self.get_witness(source, index) {
            Some(w) => w,
            None => {
                machine.set_register(A0, M::REG::from_u8(INDEX_OUT_OF_BOUND));
                return Ok(true);
            }
        };

        // Store data using CKB-style store_data
        store_data(machine, witness)?;
        machine.set_register(A0, M::REG::from_u8(SUCCESS));

        Ok(true)
    }
}
