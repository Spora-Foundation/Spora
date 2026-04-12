// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Current cycles syscall

use ckb_vm::{
    registers::{A0, A7},
    Error as VMError, Register, SupportMachine, Syscalls,
};

/// Syscall: Current Cycles
///
/// Syscall number: 2042
///
/// Returns the current cycle count in A0 register
pub struct CurrentCycles;

impl CurrentCycles {
    pub fn new() -> Self {
        Self
    }
}

impl<M: SupportMachine> Syscalls<M> for CurrentCycles {
    fn initialize(&mut self, _machine: &mut M) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut M) -> Result<bool, VMError> {
        let syscall_number = machine.registers()[A7].to_u64();

        // CURRENT_CYCLES = 2042
        if syscall_number != 2042 {
            return Ok(false);
        }

        // Get current cycles from machine
        // Note: For TraceMachine, cycles() returns total cycles
        let cycles = 0u64; // Placeholder - will be implemented when machine tracking is added

        // Return cycles in A0
        machine.set_register(A0, M::REG::from_u64(cycles));

        Ok(true)
    }
}
