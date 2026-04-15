// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Close syscall placeholder

use super::{CLOSE_SYSCALL_NUMBER, INVALID_FD, SPAWN_YIELD_CYCLES_BASE};
use ckb_vm::{
    registers::{A0, A7},
    Error as VMError, Register, SupportMachine, Syscalls,
};

/// Syscall: Close
///
/// Syscall number: 2608
///
/// Spora currently exposes no VM-level FD table, so close always fails.
#[derive(Debug, Default, Clone, Copy)]
pub struct Close;

impl Close {
    pub fn new() -> Self {
        Self
    }
}

impl<M: SupportMachine> Syscalls<M> for Close {
    fn initialize(&mut self, _machine: &mut M) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut M) -> Result<bool, VMError> {
        if machine.registers()[A7].to_u64() != CLOSE_SYSCALL_NUMBER {
            return Ok(false);
        }

        machine.add_cycles_no_checking(SPAWN_YIELD_CYCLES_BASE)?;
        machine.set_register(A0, M::REG::from_u8(INVALID_FD));
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::ScriptVersion;
    use ckb_vm::{CoreMachine, Register};

    #[test]
    fn test_close_returns_invalid_fd() {
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.set_register(A7, CLOSE_SYSCALL_NUMBER);

        let mut syscall = Close::new();
        let handled = syscall.ecall(&mut machine).expect("close syscall should succeed");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), INVALID_FD as u64);
        assert_eq!(machine.cycles(), SPAWN_YIELD_CYCLES_BASE);
    }

    #[test]
    fn test_close_ignores_other_syscalls() {
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.set_register(A7, 9999);

        let mut syscall = Close::new();
        let handled = syscall.ecall(&mut machine).expect("non-close syscall should not fail");

        assert!(!handled);
    }
}
