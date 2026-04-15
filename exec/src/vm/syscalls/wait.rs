// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Wait syscall placeholder

use super::{SPAWN_YIELD_CYCLES_BASE, WAIT_FAILURE, WAIT_SYSCALL_NUMBER};
use ckb_vm::{
    registers::{A0, A7},
    Error as VMError, Register, SupportMachine, Syscalls,
};

/// Syscall: Wait
///
/// Syscall number: 2602
///
/// Multi-VM wait semantics are not available without spawn scheduler support.
#[derive(Debug, Default, Clone, Copy)]
pub struct Wait;

impl Wait {
    pub fn new() -> Self {
        Self
    }
}

impl<M: SupportMachine> Syscalls<M> for Wait {
    fn initialize(&mut self, _machine: &mut M) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut M) -> Result<bool, VMError> {
        if machine.registers()[A7].to_u64() != WAIT_SYSCALL_NUMBER {
            return Ok(false);
        }

        machine.add_cycles_no_checking(SPAWN_YIELD_CYCLES_BASE)?;
        machine.set_register(A0, M::REG::from_u8(WAIT_FAILURE));
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::ScriptVersion;
    use ckb_vm::{CoreMachine, Register};

    #[test]
    fn test_wait_returns_wait_failure() {
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.set_register(A7, WAIT_SYSCALL_NUMBER);

        let mut syscall = Wait::new();
        let handled = syscall.ecall(&mut machine).expect("wait syscall should succeed");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), WAIT_FAILURE as u64);
        assert_eq!(machine.cycles(), SPAWN_YIELD_CYCLES_BASE);
    }

    #[test]
    fn test_wait_ignores_other_syscalls() {
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.set_register(A7, 9999);

        let mut syscall = Wait::new();
        let handled = syscall.ecall(&mut machine).expect("non-wait syscall should not fail");

        assert!(!handled);
    }
}
