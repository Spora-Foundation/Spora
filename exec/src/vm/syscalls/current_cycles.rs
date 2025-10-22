// SPDX-License-Identifier: ISC
// Copyright (C) 2025Tondi developers
//
// Current cycles syscall
// Directly copied from CKB script/src/syscalls/current_cycles.rs

use super::CURRENT_CYCLES_SYSCALL_NUMBER;
use ckb_vm::{
    Error as VMError, Register, SupportMachine, Syscalls,
    registers::{A0, A7},
};

/// Current cycles syscall
///
/// Returns the current cycle count
#[derive(Debug)]
pub struct CurrentCycles;

impl CurrentCycles {
    /// Create a new CurrentCycles syscall
    pub fn new() -> Self {
        Self
    }
}

impl Default for CurrentCycles {
    fn default() -> Self {
        Self::new()
    }
}

impl<Mac: SupportMachine> Syscalls<Mac> for CurrentCycles {
    fn initialize(&mut self, _machine: &mut Mac) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut Mac) -> Result<bool, VMError> {
        let syscall_number = machine.registers()[A7].to_u64();
        
        if syscall_number != CURRENT_CYCLES_SYSCALL_NUMBER {
            return Ok(false);
        }

        // Get current cycles
        let cycles = machine.cycles();
        
        // Return cycles in A0
        machine.set_register(A0, Mac::REG::from_u64(cycles));
        
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_cycles_creation() {
        let syscall = CurrentCycles::new();
        // Just test creation
        let _ = syscall;
    }

    #[test]
    fn test_current_cycles_default() {
        let syscall = CurrentCycles::default();
        let _ = syscall;
    }
}

