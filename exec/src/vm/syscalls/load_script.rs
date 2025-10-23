// SPDX-License-Identifier: ISC
// Copyright (C) 2025 Spora developers
//
// Load script syscall

use super::utils::{store_data, INDEX_OUT_OF_BOUND, SUCCESS};
use crate::celltx::ScriptRef;
use ckb_vm::{
    registers::{A0, A2, A7},
    Error as VMError, Register, SupportMachine, Syscalls,
};
use std::sync::Arc;

/// Syscall: Load Script
///
/// Syscall number: 2075
///
/// Loads the current script being executed
pub struct LoadScript {
    script: Arc<ScriptRef>,
}

impl LoadScript {
    pub fn new(script: Arc<ScriptRef>) -> Self {
        Self { script }
    }

    fn serialize_script(&self) -> Vec<u8> {
        let mut data = Vec::new();
        // code_hash (32 bytes)
        data.extend_from_slice(&self.script.code_hash);
        // hash_type (1 byte)
        data.push(self.script.hash_type);
        // args length (4 bytes)
        data.extend_from_slice(&(self.script.args.len() as u32).to_le_bytes());
        // args
        data.extend_from_slice(&self.script.args);
        data
    }
}

impl<M: SupportMachine> Syscalls<M> for LoadScript {
    fn initialize(&mut self, _machine: &mut M) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut M) -> Result<bool, VMError> {
        let syscall_number = machine.registers()[A7].to_u64();

        // LOAD_SCRIPT = 2075 or LOAD_SCRIPT_HASH = 2062
        if syscall_number != 2075 && syscall_number != 2062 {
            return Ok(false);
        }

        let offset = machine.registers()[A2].to_u64();

        if offset != 0 {
            machine.set_register(A0, M::REG::from_u8(INDEX_OUT_OF_BOUND));
            return Ok(true);
        }

        let data = if syscall_number == 2062 {
            // LOAD_SCRIPT_HASH
            self.script.hash().to_vec()
        } else {
            // LOAD_SCRIPT (full script)
            self.serialize_script()
        };

        // Store data using CKB-style store_data
        store_data(machine, &data)?;
        machine.set_register(A0, M::REG::from_u8(SUCCESS));

        Ok(true)
    }
}
