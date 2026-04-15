// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Inherited FD syscall placeholder

use super::{INHERITED_FD_SYSCALL_NUMBER, INVALID_FD, SPAWN_YIELD_CYCLES_BASE};
use ckb_vm::{
    registers::{A0, A7},
    Error as VMError, Register, SupportMachine, Syscalls,
};

/// Syscall: Inherited FD
///
/// Syscall number: 2607
///
/// Spora currently has no parent VM FD inheritance context.
#[derive(Debug, Default, Clone, Copy)]
pub struct InheritedFd;

impl InheritedFd {
    pub fn new() -> Self {
        Self
    }
}

impl<M: SupportMachine> Syscalls<M> for InheritedFd {
    fn initialize(&mut self, _machine: &mut M) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut M) -> Result<bool, VMError> {
        if machine.registers()[A7].to_u64() != INHERITED_FD_SYSCALL_NUMBER {
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
    fn test_inherited_fd_returns_invalid_fd() {
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.set_register(A7, INHERITED_FD_SYSCALL_NUMBER);

        let mut syscall = InheritedFd::new();
        let handled = syscall.ecall(&mut machine).expect("inherited-fd syscall should succeed");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), INVALID_FD as u64);
        assert_eq!(machine.cycles(), SPAWN_YIELD_CYCLES_BASE);
    }

    #[test]
    fn test_inherited_fd_ignores_other_syscalls() {
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.set_register(A7, 9999);

        let mut syscall = InheritedFd::new();
        let handled = syscall.ecall(&mut machine).expect("non-inherited-fd syscall should not fail");

        assert!(!handled);
    }
}
