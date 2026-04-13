// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Load cell data syscall

use super::utils::{store_data, INDEX_OUT_OF_BOUND, SUCCESS};
use super::Source;
use super::LOAD_CELL_DATA_SYSCALL_NUMBER;
use crate::celltx::CellTx;
use crate::vm::CellDataProvider;
use ckb_vm::{
    registers::{A0, A3, A4, A7},
    Error as VMError, Register, SupportMachine, Syscalls,
};
use std::sync::Arc;

/// Syscall: Load Cell Data
///
/// Syscall number: 2092
pub struct LoadCellData<D: CellDataProvider> {
    tx: Arc<CellTx>,
    provider: Arc<D>,
    group_input_indices: Vec<usize>,
    group_output_indices: Vec<usize>,
}

impl<D: CellDataProvider> LoadCellData<D> {
    pub fn new(tx: Arc<CellTx>, provider: Arc<D>, group_input_indices: Vec<usize>, group_output_indices: Vec<usize>) -> Self {
        Self { tx, provider, group_input_indices, group_output_indices }
    }

    fn get_cell_data(&self, source: u64, index: usize) -> Option<Vec<u8>> {
        match Source::parse(source)? {
            Source::Input => {
                let input = self.tx.inputs.get(index)?;
                self.provider.load_cell_by_outpoint(&input.out_point.tx_hash, input.out_point.index)?.data
            }
            Source::Output => self.tx.outputs_data.get(index).cloned(),
            Source::CellDep => {
                let dep = self.tx.deps.get(index)?;
                self.provider.load_cell_by_outpoint(&dep.out_point.tx_hash, dep.out_point.index)?.data
            }
            Source::GroupInput => {
                let input_index = *self.group_input_indices.get(index)?;
                let input = self.tx.inputs.get(input_index)?;
                self.provider.load_cell_by_outpoint(&input.out_point.tx_hash, input.out_point.index)?.data
            }
            Source::GroupOutput => self.group_output_indices.get(index).and_then(|&idx| self.tx.outputs_data.get(idx).cloned()),
            _ => None,
        }
    }
}

impl<D: CellDataProvider, M: SupportMachine> Syscalls<M> for LoadCellData<D> {
    fn initialize(&mut self, _machine: &mut M) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut M) -> Result<bool, VMError> {
        let syscall_number = machine.registers()[A7].to_u64();

        // LOAD_CELL_DATA = 2092
        if syscall_number != LOAD_CELL_DATA_SYSCALL_NUMBER {
            return Ok(false);
        }

        let index = machine.registers()[A3].to_u64() as usize;
        let source = machine.registers()[A4].to_u64();

        // Get cell data
        let cell_data = match self.get_cell_data(source, index) {
            Some(d) => d,
            None => {
                machine.set_register(A0, M::REG::from_u8(INDEX_OUT_OF_BOUND));
                return Ok(true);
            }
        };

        // Store data using CKB-style store_data
        store_data(machine, &cell_data)?;
        machine.set_register(A0, M::REG::from_u8(SUCCESS));

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::celltx::{CellDep, CellOut, CellRef, DepType, OutPoint, ScriptRef};
    use crate::vm::{ResolvedCell, ScriptVersion, SimpleDataProvider};
    use ckb_vm::{
        registers::{A1, A2},
        CoreMachine, Memory, Register,
    };

    const BUFFER_ADDR: u64 = 0x1000;
    const SIZE_ADDR: u64 = 0x2000;

    #[test]
    fn test_load_cell_data_resolves_input_and_dep_sources() {
        let input_out_point = OutPoint::new([7u8; 32], 0);
        let dep_out_point = OutPoint::new([8u8; 32], 1);
        let tx = Arc::new(CellTx {
            ver: 0xC001,
            inputs: vec![CellRef::new(input_out_point.clone(), 0)],
            deps: vec![CellDep { out_point: dep_out_point.clone(), dep_type: DepType::Code }],
            header_deps: vec![],
            outputs: vec![CellOut { capacity: 1000, lock: ScriptRef::new([1u8; 32], 0, vec![]), type_: None }],
            outputs_data: vec![vec![0xAA; 10]],
            witnesses: vec![],
        });
        let mut provider = SimpleDataProvider::new();
        provider.add_cell(
            input_out_point.tx_hash,
            input_out_point.index,
            ResolvedCell {
                cell_output: CellOut { capacity: 2000, lock: ScriptRef::new([2u8; 32], 0, vec![0x11]), type_: None },
                data: Some(vec![0x10, 0x20]),
            },
        );
        provider.add_cell(
            dep_out_point.tx_hash,
            dep_out_point.index,
            ResolvedCell {
                cell_output: CellOut { capacity: 3000, lock: ScriptRef::new([3u8; 32], 0, vec![0x22]), type_: None },
                data: Some(vec![0x30, 0x40, 0x50]),
            },
        );

        let syscall = LoadCellData::new(tx, Arc::new(provider), vec![0], vec![0]);
        assert_eq!(syscall.get_cell_data(0x01, 0).unwrap(), vec![0x10, 0x20]);
        assert_eq!(syscall.get_cell_data(0x03, 0).unwrap(), vec![0x30, 0x40, 0x50]);
        assert_eq!(syscall.get_cell_data(0x02, 0).unwrap(), vec![0xAA; 10]);
    }

    #[test]
    fn test_load_cell_data_supports_partial_reads() {
        let dep_out_point = OutPoint::new([8u8; 32], 1);
        let tx = Arc::new(CellTx {
            ver: 0xC001,
            inputs: vec![],
            deps: vec![CellDep { out_point: dep_out_point.clone(), dep_type: DepType::Code }],
            header_deps: vec![],
            outputs: vec![],
            outputs_data: vec![],
            witnesses: vec![],
        });
        let mut provider = SimpleDataProvider::new();
        provider.add_cell(
            dep_out_point.tx_hash,
            dep_out_point.index,
            ResolvedCell {
                cell_output: CellOut { capacity: 3000, lock: ScriptRef::new([3u8; 32], 0, vec![]), type_: None },
                data: Some(vec![0x30, 0x40, 0x50]),
            },
        );

        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.memory_mut().store64(&SIZE_ADDR, &2u64).unwrap();
        machine.set_register(A0, BUFFER_ADDR);
        machine.set_register(A1, SIZE_ADDR);
        machine.set_register(A2, 1);
        machine.set_register(A3, 0);
        machine.set_register(A4, 0x03);
        machine.set_register(A7, LOAD_CELL_DATA_SYSCALL_NUMBER);

        let mut syscall = LoadCellData::new(tx, Arc::new(provider), vec![], vec![]);
        let handled = syscall.ecall(&mut machine).expect("load cell data syscall should succeed");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), SUCCESS as u64);
        assert_eq!(machine.memory_mut().load64(&SIZE_ADDR).unwrap().to_u64(), 2);
        assert_eq!(machine.memory_mut().load_bytes(BUFFER_ADDR, 2).unwrap().as_ref(), &[0x40, 0x50]);
    }

    #[test]
    fn test_load_cell_data_rejects_invalid_source() {
        let tx = Arc::new(CellTx {
            ver: 0xC001,
            inputs: vec![],
            deps: vec![],
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
        machine.set_register(A4, 0x99);
        machine.set_register(A7, LOAD_CELL_DATA_SYSCALL_NUMBER);

        let mut syscall = LoadCellData::new(tx, Arc::new(SimpleDataProvider::new()), vec![], vec![]);
        let handled = syscall.ecall(&mut machine).expect("load cell data syscall should be handled");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), INDEX_OUT_OF_BOUND as u64);
    }
}
