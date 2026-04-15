// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Write syscall placeholder

use super::{INVALID_FD, WRITE_SYSCALL_NUMBER};
use ckb_vm::{
    registers::{A0, A7},
    Error as VMError, Register, SupportMachine, Syscalls,
};

/// Syscall: Write
///
/// Syscall number: 2605
///
/// File-descriptor write is unavailable until spawn/pipe runtime is implemented.
#[derive(Debug, Default, Clone, Copy)]
pub struct Write;

impl Write {
    pub fn new() -> Self {
        Self
    }
}

impl<M: SupportMachine> Syscalls<M> for Write {
    fn initialize(&mut self, _machine: &mut M) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut M) -> Result<bool, VMError> {
        if machine.registers()[A7].to_u64() != WRITE_SYSCALL_NUMBER {
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
    fn test_write_returns_invalid_fd() {
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.set_register(A7, WRITE_SYSCALL_NUMBER);

        let mut syscall = Write::new();
        let handled = syscall.ecall(&mut machine).expect("write syscall should succeed");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), INVALID_FD as u64);
    }

    #[test]
    fn test_write_ignores_other_syscalls() {
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.set_register(A7, 9999);

        let mut syscall = Write::new();
        let handled = syscall.ecall(&mut machine).expect("non-write syscall should not fail");

        assert!(!handled);
    }
}
