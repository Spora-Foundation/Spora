// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Current cycles syscall

use super::CURRENT_CYCLES_SYSCALL_NUMBER;
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
        if syscall_number != CURRENT_CYCLES_SYSCALL_NUMBER {
            return Ok(false);
        }

        // Return the machine's current cycle counter.
        // ckb-vm exposes this through SupportMachine for both core and wrapped machines.
        let cycles = machine.cycles();

        // Return cycles in A0
        machine.set_register(A0, M::REG::from_u64(cycles));

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::ScriptVersion;
    use ckb_vm::{
        registers::{A0, A7},
        CoreMachine, Register, SupportMachine, Syscalls,
    };

    #[test]
    fn test_current_cycles_returns_machine_cycles() {
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.set_cycles(4242);
        machine.set_register(A7, CURRENT_CYCLES_SYSCALL_NUMBER);

        let mut syscall = CurrentCycles::new();
        let handled = syscall.ecall(&mut machine).expect("current cycles syscall should succeed");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), 4242);
    }

    #[test]
    fn test_current_cycles_ignores_other_syscalls() {
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.set_cycles(99);
        machine.set_register(A7, 1);

        let mut syscall = CurrentCycles::new();
        let handled = syscall.ecall(&mut machine).expect("non-matching syscall should not error");

        assert!(!handled);
    }
}
