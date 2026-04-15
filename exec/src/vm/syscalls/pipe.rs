// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Pipe syscall placeholder

use super::{MAX_FDS_CREATED, PIPE_SYSCALL_NUMBER, SPAWN_YIELD_CYCLES_BASE};
use ckb_vm::{
    registers::{A0, A7},
    Error as VMError, Register, SupportMachine, Syscalls,
};

/// Syscall: Pipe
///
/// Syscall number: 2604
///
/// Spora does not yet expose VM-level FD pipe primitives, so this returns a
/// deterministic allocation failure code.
#[derive(Debug, Default, Clone, Copy)]
pub struct Pipe;

impl Pipe {
    pub fn new() -> Self {
        Self
    }
}

impl<M: SupportMachine> Syscalls<M> for Pipe {
    fn initialize(&mut self, _machine: &mut M) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut M) -> Result<bool, VMError> {
        if machine.registers()[A7].to_u64() != PIPE_SYSCALL_NUMBER {
            return Ok(false);
        }

        machine.add_cycles_no_checking(SPAWN_YIELD_CYCLES_BASE)?;
        machine.set_register(A0, M::REG::from_u8(MAX_FDS_CREATED));
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::ScriptVersion;
    use ckb_vm::{CoreMachine, Register};

    #[test]
    fn test_pipe_returns_max_fds_created() {
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.set_register(A7, PIPE_SYSCALL_NUMBER);

        let mut syscall = Pipe::new();
        let handled = syscall.ecall(&mut machine).expect("pipe syscall should succeed");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), MAX_FDS_CREATED as u64);
        assert_eq!(machine.cycles(), SPAWN_YIELD_CYCLES_BASE);
    }

    #[test]
    fn test_pipe_ignores_other_syscalls() {
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.set_register(A7, 9999);

        let mut syscall = Pipe::new();
        let handled = syscall.ecall(&mut machine).expect("non-pipe syscall should not fail");

        assert!(!handled);
    }
}
