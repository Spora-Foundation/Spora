// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Load input syscall
// Reference: ckb/script/src/syscalls/load_input.rs

use super::utils::{store_data, INDEX_OUT_OF_BOUND};
use super::{InputField, Source, LOAD_INPUT_BY_FIELD_SYSCALL_NUMBER, LOAD_INPUT_SYSCALL_NUMBER};
use crate::celltx::{CellInput, CellTx};
use crate::serialization::vm_abi::{serialize_cell_input, serialize_outpoint};
use crate::vm::transferred_byte_cycles;
use ckb_vm::{
    registers::{A0, A3, A4, A5, A7},
    Error as VMError, Register, SupportMachine, Syscalls,
};
use std::sync::Arc;

/// Syscall: Load Input
///
/// Syscall number: 2073
pub struct LoadInput {
    tx: Arc<CellTx>,
    group_input_indices: Vec<usize>,
}

impl LoadInput {
    pub fn new(tx: Arc<CellTx>, group_input_indices: Vec<usize>) -> Self {
        Self { tx, group_input_indices }
    }

    fn get_input(&self, source: Source, index: usize) -> Option<&CellInput> {
        match source {
            Source::Input => self.tx.inputs.get(index),
            Source::GroupInput => self.group_input_indices.get(index).and_then(|&idx| self.tx.inputs.get(idx)),
            _ => None,
        }
    }

    fn serialize_input_field(&self, input: &CellInput, field: u64) -> Result<Vec<u8>, VMError> {
        match InputField::parse_from_u64(field)? {
            InputField::OutPoint => Ok(serialize_outpoint(&input.previous_output)),
            InputField::Since => Ok(input.since.to_le_bytes().to_vec()),
        }
    }
}

impl<M: SupportMachine> Syscalls<M> for LoadInput {
    fn initialize(&mut self, _machine: &mut M) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut M) -> Result<bool, VMError> {
        let syscall_number = machine.registers()[A7].to_u64();

        // LOAD_INPUT = 2073 or LOAD_INPUT_BY_FIELD = 2083
        if syscall_number != LOAD_INPUT_SYSCALL_NUMBER && syscall_number != LOAD_INPUT_BY_FIELD_SYSCALL_NUMBER {
            return Ok(false);
        }

        let index = machine.registers()[A3].to_u64() as usize;
        let source = Source::parse_from_u64(machine.registers()[A4].to_u64())?;

        // Get input
        let input = match self.get_input(source, index) {
            Some(i) => i,
            None => {
                machine.set_register(A0, M::REG::from_u8(INDEX_OUT_OF_BOUND));
                return Ok(true);
            }
        };

        // Get field data
        let data = if syscall_number == LOAD_INPUT_BY_FIELD_SYSCALL_NUMBER {
            // LOAD_INPUT_BY_FIELD
            let field = machine.registers()[A5].to_u64();
            self.serialize_input_field(input, field)?
        } else {
            // LOAD_INPUT (full input = outpoint + since = 44 bytes)
            let mut data = Vec::with_capacity(44);
            data.extend_from_slice(&input.previous_output.tx_hash);
            data.extend_from_slice(&input.previous_output.index.to_le_bytes());
            data.extend_from_slice(&input.since.to_le_bytes());
            data
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
    use crate::vm::syscalls::SUCCESS;
    use crate::vm::ScriptVersion;
    use ckb_vm::{
        registers::{A1, A2},
        CoreMachine, Memory, Register,
    };

    const BUFFER_ADDR: u64 = 0x1000;
    const SIZE_ADDR: u64 = 0x2000;

    #[test]
    fn test_load_input_supports_partial_reads() {
        let input = CellInput::new(crate::celltx::OutPoint::new([0xAB; 32], 7), 0x1122_3344_5566_7788);
        let tx = Arc::new(CellTx {
            version: 0xC001,
            inputs: vec![input.clone()],
            cell_deps: vec![],
            header_deps: vec![],
            outputs: vec![],
            outputs_data: vec![],
            witnesses: vec![],
        });

        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.memory_mut().store64(&SIZE_ADDR, &8u64).unwrap();
        machine.set_register(A0, BUFFER_ADDR);
        machine.set_register(A1, SIZE_ADDR);
        machine.set_register(A2, 36);
        machine.set_register(A3, 0);
        machine.set_register(A4, 0x01);
        machine.set_register(A7, LOAD_INPUT_SYSCALL_NUMBER);

        let mut syscall = LoadInput::new(tx, vec![0]);
        let handled = syscall.ecall(&mut machine).expect("load input syscall should succeed");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), SUCCESS as u64);
        assert_eq!(machine.memory_mut().load64(&SIZE_ADDR).unwrap().to_u64(), 8);
        assert_eq!(machine.memory_mut().load_bytes(BUFFER_ADDR, 8).unwrap().as_ref(), &0x1122_3344_5566_7788u64.to_le_bytes());
    }

    #[test]
    fn test_load_input_by_field_rejects_unknown_field() {
        let input = CellInput::new(crate::celltx::OutPoint::new([0xAB; 32], 7), 0x1122_3344_5566_7788);
        let tx = Arc::new(CellTx {
            version: 0xC001,
            inputs: vec![input],
            cell_deps: vec![],
            header_deps: vec![],
            outputs: vec![],
            outputs_data: vec![],
            witnesses: vec![],
        });

        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.memory_mut().store64(&SIZE_ADDR, &8u64).unwrap();
        machine.set_register(A0, BUFFER_ADDR);
        machine.set_register(A1, SIZE_ADDR);
        machine.set_register(A2, 0);
        machine.set_register(A3, 0);
        machine.set_register(A4, Source::Input as u64);
        machine.set_register(A5, 99);
        machine.set_register(A7, LOAD_INPUT_BY_FIELD_SYSCALL_NUMBER);

        let mut syscall = LoadInput::new(tx, vec![0]);
        let err = syscall.ecall(&mut machine).expect_err("unknown field should trap");

        assert_eq!(err, VMError::External("InputField parse_from_u64 99".to_string()));
    }
}
