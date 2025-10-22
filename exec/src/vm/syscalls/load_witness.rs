// SPDX-License-Identifier: ISC
// Copyright (C) 2025Tondi developers
//
// Load witness syscall
// Adapted from CKB script/src/syscalls/load_witness.rs

use super::utils::store_data;
use super::{LOAD_WITNESS_SYSCALL_NUMBER, SUCCESS, INDEX_OUT_OF_BOUND};
use crate::vm::cost_model::transferred_byte_cycles;
use ckb_vm::{
    Error as VMError, Register, SupportMachine, Syscalls,
    registers::{A0, A3, A7},
};

/// Load witness syscall
#[derive(Debug)]
pub struct LoadWitness {
    witnesses: Vec<Vec<u8>>,
}

impl LoadWitness {
    /// Create a new LoadWitness syscall
    pub fn new(witnesses: Vec<Vec<u8>>) -> Self {
        Self { witnesses }
    }
}

impl<Mac: SupportMachine> Syscalls<Mac> for LoadWitness {
    fn initialize(&mut self, _machine: &mut Mac) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut Mac) -> Result<bool, VMError> {
        let syscall_number = machine.registers()[A7].to_u64();
        
        if syscall_number != LOAD_WITNESS_SYSCALL_NUMBER {
            return Ok(false);
        }

        let index = machine.registers()[A3].to_u64() as usize;

        // Fetch witness by index
        let witness = match self.witnesses.get(index) {
            Some(w) => w,
            None => {
                machine.set_register(A0, Mac::REG::from_u8(INDEX_OUT_OF_BOUND));
                return Ok(true);
            }
        };

        // Store witness to VM memory
        let wrote_size = store_data(machine, witness)?;

        // Add cycles cost
        machine.add_cycles_no_checking(transferred_byte_cycles(wrote_size))?;
        machine.set_register(A0, Mac::REG::from_u8(SUCCESS));
        
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_witness_creation() {
        let witnesses = vec![vec![1, 2, 3], vec![4, 5, 6]];
        let syscall = LoadWitness::new(witnesses.clone());
        assert_eq!(syscall.witnesses.len(), 2);
    }
}

