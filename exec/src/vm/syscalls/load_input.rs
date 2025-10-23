// SPDX-License-Identifier: ISC
// Copyright (C) 2025 Spora developers
//
// Load input syscall
// Reference: ckb/script/src/syscalls/load_input.rs

use super::utils::{store_data, SUCCESS, INDEX_OUT_OF_BOUND};
use crate::celltx::{CellTx, CellRef};
use ckb_vm::{
    Register, Syscalls, SupportMachine,
    Error as VMError,
    registers::{A0, A2, A3, A4, A5, A7},
};
use std::sync::Arc;

/// Input field selector
#[repr(u64)]
pub enum InputField {
    OutPoint = 0,
    Since = 1,
}

/// Syscall: Load Input
///
/// Syscall number: 2073
pub struct LoadInput {
    tx: Arc<CellTx>,
    group_input_indices: Vec<usize>,
}

impl LoadInput {
    pub fn new(tx: Arc<CellTx>, group_input_indices: Vec<usize>) -> Self {
        Self { tx, group_input_indices }
    }

    fn get_input(&self, source: u64, index: usize) -> Option<&CellRef> {
        match source {
            0x01 => {
                // Input
                self.tx.inputs.get(index)
            }
            0x0100 => {
                // GroupInput
                self.group_input_indices.get(index)
                    .and_then(|&idx| self.tx.inputs.get(idx))
            }
            _ => None,
        }
    }

    fn serialize_input_field(&self, input: &CellRef, field: u64) -> Option<Vec<u8>> {
        match field {
            0 => {
                // OutPoint (tx_hash + index = 36 bytes)
                let mut data = Vec::with_capacity(36);
                data.extend_from_slice(&input.out_point.tx_hash);
                data.extend_from_slice(&input.out_point.index.to_le_bytes());
                Some(data)
            }
            1 => {
                // Since (8 bytes)
                Some(input.since.to_le_bytes().to_vec())
            }
            _ => None,
        }
    }
}

impl<M: SupportMachine> Syscalls<M> for LoadInput {
    fn initialize(&mut self, _machine: &mut M) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut M) -> Result<bool, VMError> {
        let syscall_number = machine.registers()[A7].to_u64();
        
        // LOAD_INPUT = 2073 or LOAD_INPUT_BY_FIELD = 2083
        if syscall_number != 2073 && syscall_number != 2083 {
            return Ok(false);
        }

        let offset = machine.registers()[A2].to_u64();
        let index = machine.registers()[A3].to_u64() as usize;
        let source = machine.registers()[A4].to_u64();

        if offset != 0 {
            machine.set_register(A0, M::REG::from_u8(INDEX_OUT_OF_BOUND));
            return Ok(true);
        }

        // Get input
        let input = match self.get_input(source, index) {
            Some(i) => i,
            None => {
                machine.set_register(A0, M::REG::from_u8(INDEX_OUT_OF_BOUND));
                return Ok(true);
            }
        };

        // Get field data
        let data = if syscall_number == 2083 {
            // LOAD_INPUT_BY_FIELD
            let field = machine.registers()[A5].to_u64();
            match self.serialize_input_field(input, field) {
                Some(d) => d,
                None => {
                    machine.set_register(A0, M::REG::from_u8(INDEX_OUT_OF_BOUND));
                    return Ok(true);
                }
            }
        } else {
            // LOAD_INPUT (full input = outpoint + since = 44 bytes)
            let mut data = Vec::with_capacity(44);
            data.extend_from_slice(&input.out_point.tx_hash);
            data.extend_from_slice(&input.out_point.index.to_le_bytes());
            data.extend_from_slice(&input.since.to_le_bytes());
            data
        };

        // Store data using CKB-style store_data
        store_data(machine, &data)?;
        machine.set_register(A0, M::REG::from_u8(SUCCESS));
        
        Ok(true)
    }
}
