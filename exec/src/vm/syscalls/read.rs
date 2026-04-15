// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Read syscall placeholder

use super::{INVALID_FD, READ_SYSCALL_NUMBER};
use ckb_vm::{
    registers::{A0, A7},
    Error as VMError, Register, SupportMachine, Syscalls,
};

/// Syscall: Read
///
/// Syscall number: 2606
///
/// File-descriptor read is unavailable until spawn/pipe runtime is implemented.
#[derive(Debug, Default, Clone, Copy)]
pub struct Read;

impl Read {
    pub fn new() -> Self {
        Self
    }
}

impl<M: SupportMachine> Syscalls<M> for Read {
    fn initialize(&mut self, _machine: &mut M) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut M) -> Result<bool, VMError> {
        if machine.registers()[A7].to_u64() != READ_SYSCALL_NUMBER {
            return Ok(false);
        }

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
    fn test_read_returns_invalid_fd() {
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.set_register(A7, READ_SYSCALL_NUMBER);

        let mut syscall = Read::new();
        let handled = syscall.ecall(&mut machine).expect("read syscall should succeed");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), INVALID_FD as u64);
    }

    #[test]
    fn test_read_ignores_other_syscalls() {
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.set_register(A7, 9999);

        let mut syscall = Read::new();
        let handled = syscall.ecall(&mut machine).expect("non-read syscall should not fail");

        assert!(!handled);
    }
}
