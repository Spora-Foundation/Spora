// SPDX-License-Identifier: ISC
// Copyright (C) 2025 Spora developers
//
// Debug print syscall

use ckb_vm::{
    registers::{A0, A1, A7},
    Error as VMError, Memory, Register, SupportMachine, Syscalls,
};

/// Syscall: Debug Print
///
/// Syscall number: 2177
///
/// Prints debug message (only in debug builds)
pub struct Debugger {
    script_hash: [u8; 32],
}

impl Debugger {
    pub fn new(script_hash: [u8; 32]) -> Self {
        Self { script_hash }
    }
}

impl<M: SupportMachine> Syscalls<M> for Debugger {
    fn initialize(&mut self, _machine: &mut M) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut M) -> Result<bool, VMError> {
        let syscall_number = machine.registers()[A7].to_u64();

        // DEBUG_PRINT = 2177
        if syscall_number != 2177 {
            return Ok(false);
        }

        let addr = machine.registers()[A0].to_u64();
        let len = machine.registers()[A1].to_u64() as usize;

        // Read debug message from VM memory
        let mut message = vec![0u8; len];
        machine.memory_mut().store_bytes(addr, &mut message)?;

        // Print debug message (only in debug mode)
        #[cfg(debug_assertions)]
        {
            let msg_str = String::from_utf8_lossy(&message);
            log::debug!("Script {:?} DEBUG: {}", hex::encode(&self.script_hash[..8]), msg_str);
        }

        // Return success
        machine.set_register(A0, M::REG::from_u8(0));

        Ok(true)
    }
}
