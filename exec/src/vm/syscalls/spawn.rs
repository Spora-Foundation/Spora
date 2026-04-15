// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Spawn syscall placeholder

use super::{MAX_VMS_SPAWNED, SPAWN_EXTRA_CYCLES_BASE, SPAWN_SYSCALL_NUMBER, SPAWN_YIELD_CYCLES_BASE};
use ckb_vm::{
    registers::{A0, A7},
    Error as VMError, Register, SupportMachine, Syscalls,
};

/// Syscall: Spawn
///
/// Syscall number: 2601
///
/// Full multi-VM scheduling is not implemented in Spora's verifier runtime.
/// This placeholder returns a deterministic CKB-style error code so
/// contracts can branch on failure instead of trapping on an unknown syscall.
#[derive(Debug, Default, Clone, Copy)]
pub struct Spawn;

impl Spawn {
    pub fn new() -> Self {
        Self
    }
}

impl<M: SupportMachine> Syscalls<M> for Spawn {
    fn initialize(&mut self, _machine: &mut M) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut M) -> Result<bool, VMError> {
        if machine.registers()[A7].to_u64() != SPAWN_SYSCALL_NUMBER {
            return Ok(false);
        }

        machine.add_cycles_no_checking(SPAWN_EXTRA_CYCLES_BASE)?;
        machine.add_cycles_no_checking(SPAWN_YIELD_CYCLES_BASE)?;
        machine.set_register(A0, M::REG::from_u8(MAX_VMS_SPAWNED));
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::ScriptVersion;
    use ckb_vm::{CoreMachine, Register};

    #[test]
    fn test_spawn_returns_max_vms_spawned() {
        let mut machine = ScriptVersion::V2.init_core_machine(200_000);
        machine.set_register(A7, SPAWN_SYSCALL_NUMBER);

        let mut syscall = Spawn::new();
        let handled = syscall.ecall(&mut machine).expect("spawn syscall should succeed");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), MAX_VMS_SPAWNED as u64);
        assert_eq!(machine.cycles(), SPAWN_EXTRA_CYCLES_BASE + SPAWN_YIELD_CYCLES_BASE);
    }

    #[test]
    fn test_spawn_ignores_other_syscalls() {
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.set_register(A7, 9999);

        let mut syscall = Spawn::new();
        let handled = syscall.ecall(&mut machine).expect("non-spawn syscall should not fail");

        assert!(!handled);
    }
}
