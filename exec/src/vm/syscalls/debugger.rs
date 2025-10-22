// SPDX-License-Identifier: ISC
// Copyright (C) 2025Tondi developers
//
// Debug print syscall
// Adapted from CKB script/src/syscalls/debugger.rs

use super::DEBUG_PRINT_SYSCALL_NUMBER;
use ckb_vm::{
    Error as VMError, Memory, Register, SupportMachine, Syscalls,
    registers::{A0, A1, A7},
};

/// Debug print syscall
///
/// Allows scripts to print debug messages
#[derive(Debug)]
pub struct Debugger {
    enabled: bool,
}

impl Debugger {
    /// Create a new Debugger syscall
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }
}

impl Default for Debugger {
    fn default() -> Self {
        Self::new(cfg!(debug_assertions))
    }
}

impl<Mac: SupportMachine> Syscalls<Mac> for Debugger {
    fn initialize(&mut self, _machine: &mut Mac) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut Mac) -> Result<bool, VMError> {
        let syscall_number = machine.registers()[A7].to_u64();
        
        if syscall_number != DEBUG_PRINT_SYSCALL_NUMBER {
            return Ok(false);
        }

        if !self.enabled {
            // Debug disabled, return success but do nothing
            machine.set_register(A0, Mac::REG::from_u8(0));
            return Ok(true);
        }

        // Read message from memory
        let addr = machine.registers()[A0].to_u64();
        let len = machine.registers()[A1].to_u64() as usize;

        let message = machine.memory_mut().load_bytes(addr, len as u64)?;

        // Print debug message
        if let Ok(msg_str) = String::from_utf8(message.to_vec()) {
            eprintln!("[VM DEBUG] {}", msg_str);
        } else {
            eprintln!("[VM DEBUG] (binary data, {} bytes)", len);
        }

        machine.set_register(A0, Mac::REG::from_u8(0));
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debugger_creation() {
        let debugger = Debugger::new(true);
        assert!(debugger.enabled);

        let debugger = Debugger::new(false);
        assert!(!debugger.enabled);
    }

    #[test]
    fn test_debugger_default() {
        let debugger = Debugger::default();
        // In debug mode should be enabled
        #[cfg(debug_assertions)]
        assert!(debugger.enabled);
        
        // In release mode should be disabled
        #[cfg(not(debug_assertions))]
        assert!(!debugger.enabled);
    }
}

