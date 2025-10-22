// SPDX-License-Identifier: ISC
// Copyright (C) 2025Tondi developers
//
// Load script syscall
// Adapted from CKB script/src/syscalls/load_script.rs
// ⚠️ Modified: Blake2b → Blake3

use super::utils::store_data;
use super::{LOAD_SCRIPT_SYSCALL_NUMBER, LOAD_SCRIPT_HASH_SYSCALL_NUMBER, SUCCESS};
use crate::celltx::types::ScriptRef;
use crate::vm::cost_model::transferred_byte_cycles;
use ckb_vm::{
    Error as VMError, Register, SupportMachine, Syscalls,
    registers::{A0, A7},
};

/// Load script syscall
#[derive(Debug)]
pub struct LoadScript {
    script: ScriptRef,
    script_hash: [u8; 32],
}

impl LoadScript {
    /// Create a new LoadScript syscall
    pub fn new(script: ScriptRef) -> Self {
        // Calculate script hash using Blake3 (not Blake2b like CKB)
        let script_hash = script.hash();
        Self { script, script_hash }
    }

    /// Serialize script to bytes
    fn serialize_script(&self) -> Vec<u8> {
        // Simple serialization: code_hash || hash_type || args_len || args
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.script.code_hash);
        bytes.push(self.script.hash_type);
        bytes.extend_from_slice(&(self.script.args.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&self.script.args);
        bytes
    }
}

impl<Mac: SupportMachine> Syscalls<Mac> for LoadScript {
    fn initialize(&mut self, _machine: &mut Mac) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut Mac) -> Result<bool, VMError> {
        let syscall_number = machine.registers()[A7].to_u64();
        
        let wrote_size = match syscall_number {
            LOAD_SCRIPT_SYSCALL_NUMBER => {
                let serialized = self.serialize_script();
                store_data(machine, &serialized)?
            }
            LOAD_SCRIPT_HASH_SYSCALL_NUMBER => {
                store_data(machine, &self.script_hash)?
            }
            _ => return Ok(false),
        };

        // Add cycles cost
        machine.add_cycles_no_checking(transferred_byte_cycles(wrote_size))?;
        machine.set_register(A0, Mac::REG::from_u8(SUCCESS));
        
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_script_creation() {
        let script = ScriptRef::new([0x12; 32], 1, vec![0xAB, 0xCD]);
        let syscall = LoadScript::new(script);
        
        // Script hash should be computed
        assert_ne!(syscall.script_hash, [0; 32]);
    }

    #[test]
    fn test_script_serialization() {
        let script = ScriptRef::new([0x12; 32], 1, vec![0xAB, 0xCD]);
        let syscall = LoadScript::new(script);
        
        let serialized = syscall.serialize_script();
        assert!(serialized.len() > 32); // code_hash + hash_type + args_len + args
    }
}

