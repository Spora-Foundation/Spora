// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Load witness syscall

use super::utils::{store_data, INDEX_OUT_OF_BOUND, SUCCESS};
use super::Source;
use super::LOAD_WITNESS_SYSCALL_NUMBER;
use crate::celltx::CellTx;
use ckb_vm::{
    registers::{A0, A3, A4, A7},
    Error as VMError, Register, SupportMachine, Syscalls,
};
use std::sync::Arc;

/// Syscall: Load Witness
///
/// Syscall number: 2074
pub struct LoadWitness {
    tx: Arc<CellTx>,
    group_input_indices: Vec<usize>,
}

impl LoadWitness {
    pub fn new(tx: Arc<CellTx>, group_input_indices: Vec<usize>) -> Self {
        Self { tx, group_input_indices }
    }

    fn get_witness(&self, source: u64, index: usize) -> Option<&[u8]> {
        match Source::parse(source)? {
            Source::Input => self.tx.witnesses.get(index).map(|w| w.as_slice()),
            Source::GroupInput => {
                self.group_input_indices.get(index).and_then(|&idx| self.tx.witnesses.get(idx).map(|w| w.as_slice()))
            }
            _ => None,
        }
    }
}

impl<M: SupportMachine> Syscalls<M> for LoadWitness {
    fn initialize(&mut self, _machine: &mut M) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut M) -> Result<bool, VMError> {
        let syscall_number = machine.registers()[A7].to_u64();

        // LOAD_WITNESS = 2074
        if syscall_number != LOAD_WITNESS_SYSCALL_NUMBER {
            return Ok(false);
        }

        let index = machine.registers()[A3].to_u64() as usize;
        let source = machine.registers()[A4].to_u64();

        // Get witness data
        let witness = match self.get_witness(source, index) {
            Some(w) => w,
            None => {
                machine.set_register(A0, M::REG::from_u8(INDEX_OUT_OF_BOUND));
                return Ok(true);
            }
        };

        // Store data using CKB-style store_data
        store_data(machine, witness)?;
        machine.set_register(A0, M::REG::from_u8(SUCCESS));

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::ScriptVersion;
    use ckb_vm::{
        registers::{A1, A2},
        CoreMachine, Memory, Register,
    };

    const BUFFER_ADDR: u64 = 0x1000;
    const SIZE_ADDR: u64 = 0x2000;

    #[test]
    fn test_load_witness_supports_partial_reads() {
        let tx = Arc::new(CellTx {
            version: 0xC001,
            inputs: vec![],
            cell_deps: vec![],
            header_deps: vec![],
            outputs: vec![],
            outputs_data: vec![],
            witnesses: vec![vec![1, 2, 3, 4, 5]],
        });

        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.memory_mut().store64(&SIZE_ADDR, &2u64).unwrap();
        machine.set_register(A0, BUFFER_ADDR);
        machine.set_register(A1, SIZE_ADDR);
        machine.set_register(A2, 2);
        machine.set_register(A3, 0);
        machine.set_register(A4, 0x01);
        machine.set_register(A7, LOAD_WITNESS_SYSCALL_NUMBER);

        let mut syscall = LoadWitness::new(tx, vec![]);
        let handled = syscall.ecall(&mut machine).expect("load witness syscall should succeed");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), SUCCESS as u64);
        assert_eq!(machine.memory_mut().load64(&SIZE_ADDR).unwrap().to_u64(), 3);
        assert_eq!(machine.memory_mut().load_bytes(BUFFER_ADDR, 2).unwrap().as_ref(), &[3, 4]);
    }

    #[test]
    fn test_load_witness_rejects_invalid_source() {
        let tx = Arc::new(CellTx {
            version: 0xC001,
            inputs: vec![],
            cell_deps: vec![],
            header_deps: vec![],
            outputs: vec![],
            outputs_data: vec![],
            witnesses: vec![vec![1, 2, 3]],
        });

        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.memory_mut().store64(&SIZE_ADDR, &3u64).unwrap();
        machine.set_register(A0, BUFFER_ADDR);
        machine.set_register(A1, SIZE_ADDR);
        machine.set_register(A2, 0);
        machine.set_register(A3, 0);
        machine.set_register(A4, 0x99);
        machine.set_register(A7, LOAD_WITNESS_SYSCALL_NUMBER);

        let mut syscall = LoadWitness::new(tx, vec![]);
        let handled = syscall.ecall(&mut machine).expect("load witness syscall should be handled");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), INDEX_OUT_OF_BOUND as u64);
    }
}
