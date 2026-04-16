// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Load script syscall

use super::utils::store_data;
use super::{LOAD_SCRIPT_HASH_SYSCALL_NUMBER, LOAD_SCRIPT_SYSCALL_NUMBER};
use crate::celltx::Script;
use crate::serialization::molecule_compat::serialize_script_molecule;
use crate::serialization::vm_abi::serialize_script;
use crate::serialization::VmAbiFormat;
use crate::vm::transferred_byte_cycles;
use ckb_vm::{
    registers::{A0, A7},
    Error as VMError, Register, SupportMachine, Syscalls,
};
use std::sync::Arc;

/// Syscall: Load Script
///
/// Syscall number: 2075
///
/// Loads the current script being executed
pub struct LoadScript {
    script: Arc<Script>,
    abi_format: VmAbiFormat,
}

impl LoadScript {
    pub fn new(script: Arc<Script>) -> Self {
        Self { script, abi_format: VmAbiFormat::Legacy }
    }

    /// Select the VM ABI wire format used by full script loads.
    pub fn with_abi_format(mut self, abi_format: VmAbiFormat) -> Self {
        self.abi_format = abi_format;
        self
    }

    fn serialize_script(&self) -> Result<Vec<u8>, VMError> {
        match self.abi_format {
            VmAbiFormat::Legacy => Ok(serialize_script(&self.script)),
            VmAbiFormat::Molecule => serialize_script_molecule(&self.script).map_err(|e| VMError::External(e.to_string())),
        }
    }
}

impl<M: SupportMachine> Syscalls<M> for LoadScript {
    fn initialize(&mut self, _machine: &mut M) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut M) -> Result<bool, VMError> {
        let syscall_number = machine.registers()[A7].to_u64();

        // LOAD_SCRIPT = 2075 or LOAD_SCRIPT_HASH = 2062
        if syscall_number != LOAD_SCRIPT_SYSCALL_NUMBER && syscall_number != LOAD_SCRIPT_HASH_SYSCALL_NUMBER {
            return Ok(false);
        }

        let data = if syscall_number == LOAD_SCRIPT_HASH_SYSCALL_NUMBER {
            // LOAD_SCRIPT_HASH
            self.script.hash().to_vec()
        } else {
            // LOAD_SCRIPT (full script)
            self.serialize_script()?
        };

        // Store data using CKB-style store_data
        let result = store_data(machine, &data)?;
        machine.add_cycles_no_checking(transferred_byte_cycles(result.written_size))?;
        machine.set_register(A0, M::REG::from_u8(result.return_code));

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serialization::molecule_compat::serialize_script_molecule;
    use crate::serialization::VmAbiFormat;
    use crate::vm::syscalls::SUCCESS;
    use crate::vm::ScriptVersion;
    use ckb_vm::{
        registers::{A1, A2},
        CoreMachine, Memory, Register,
    };

    const BUFFER_ADDR: u64 = 0x1000;
    const SIZE_ADDR: u64 = 0x2000;

    #[test]
    fn test_load_script_supports_partial_reads() {
        let script = Arc::new(Script::new([0xAA; 32], 1, vec![0x10, 0x20, 0x30]));
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.memory_mut().store64(&SIZE_ADDR, &7u64).unwrap();
        machine.set_register(A0, BUFFER_ADDR);
        machine.set_register(A1, SIZE_ADDR);
        machine.set_register(A2, 33);
        machine.set_register(A7, LOAD_SCRIPT_SYSCALL_NUMBER);

        let mut syscall = LoadScript::new(script);
        let handled = syscall.ecall(&mut machine).expect("load script syscall should succeed");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), SUCCESS as u64);
        assert_eq!(machine.memory_mut().load64(&SIZE_ADDR).unwrap().to_u64(), 7);
        assert_eq!(machine.memory_mut().load_bytes(BUFFER_ADDR, 7).unwrap().as_ref(), &[3, 0, 0, 0, 0x10, 0x20, 0x30]);
    }

    #[test]
    fn test_load_script_molecule_abi_full_load() {
        let script = Arc::new(Script::new([0xAA; 32], 1, vec![0x10, 0x20, 0x30]));
        let expected = serialize_script_molecule(&script).unwrap();
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.memory_mut().store64(&SIZE_ADDR, &(expected.len() as u64)).unwrap();
        machine.set_register(A0, BUFFER_ADDR);
        machine.set_register(A1, SIZE_ADDR);
        machine.set_register(A2, 0);
        machine.set_register(A7, LOAD_SCRIPT_SYSCALL_NUMBER);

        let mut syscall = LoadScript::new(script).with_abi_format(VmAbiFormat::Molecule);
        let handled = syscall.ecall(&mut machine).expect("load script syscall should succeed");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), SUCCESS as u64);
        assert_eq!(machine.memory_mut().load64(&SIZE_ADDR).unwrap().to_u64(), expected.len() as u64);
        assert_eq!(machine.memory_mut().load_bytes(BUFFER_ADDR, expected.len() as u64).unwrap().as_ref(), expected.as_slice());
    }

    #[test]
    fn test_load_script_hash_supports_partial_reads() {
        let script = Arc::new(Script::new([0xAA; 32], 1, vec![0x10, 0x20, 0x30]));
        let expected_hash = script.hash();
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.memory_mut().store64(&SIZE_ADDR, &6u64).unwrap();
        machine.set_register(A0, BUFFER_ADDR);
        machine.set_register(A1, SIZE_ADDR);
        machine.set_register(A2, 10);
        machine.set_register(A7, LOAD_SCRIPT_HASH_SYSCALL_NUMBER);

        let mut syscall = LoadScript::new(script);
        let handled = syscall.ecall(&mut machine).expect("load script hash syscall should succeed");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), SUCCESS as u64);
        assert_eq!(machine.memory_mut().load64(&SIZE_ADDR).unwrap().to_u64(), 22);
        assert_eq!(machine.memory_mut().load_bytes(BUFFER_ADDR, 6).unwrap().as_ref(), &expected_hash[10..16]);
    }
}
