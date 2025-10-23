// SPDX-License-Identifier: ISC
// Copyright (C) 2025 Spora developers
//
// Load header syscall (DAG-aware)

use super::utils::INDEX_OUT_OF_BOUND;
use ckb_vm::{
    registers::{A0, A7},
    Error as VMError, Register, SupportMachine, Syscalls,
};

/// Syscall: Load Header
///
/// Syscall number: 2072
///
/// Note: In DAG, headers are more complex (multi-parent)
/// This is a simplified implementation
pub struct LoadHeader;

impl LoadHeader {
    pub fn new() -> Self {
        Self
    }
}

impl<M: SupportMachine> Syscalls<M> for LoadHeader {
    fn initialize(&mut self, _machine: &mut M) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut M) -> Result<bool, VMError> {
        let syscall_number = machine.registers()[A7].to_u64();

        // LOAD_HEADER = 2072
        if syscall_number != 2072 {
            return Ok(false);
        }

        // TODO: Implement header loading
        // In DAG, this needs access to block headers by hash
        // For now, return INDEX_OUT_OF_BOUND

        machine.set_register(A0, M::REG::from_u8(INDEX_OUT_OF_BOUND));

        Ok(true)
    }
}
