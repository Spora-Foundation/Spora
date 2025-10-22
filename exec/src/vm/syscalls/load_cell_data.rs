// SPDX-License-Identifier: ISC
// Copyright (C) 2025Tondi developers
//
// Load cell data syscall
// Adapted from CKB script/src/syscalls/load_cell_data.rs
// ⚠️ Modified: Blake2b → Blake3, adapted for SPORA

use super::utils::store_data;
use super::{
    LOAD_CELL_DATA_SYSCALL_NUMBER, SUCCESS, INDEX_OUT_OF_BOUND, Source,
};
use crate::celltx::types::OutPoint;
use crate::vm::cost_model::transferred_byte_cycles;
use ckb_vm::{
    Error as VMError, Register, SupportMachine, Syscalls,
    registers::{A0, A3, A4, A7},
};

/// Cell metadata with data (SYSCALL-LOCAL)
/// 
/// ⚠️ NOTE: This is a module-local `CellMeta`, not exported.
/// Used only within load_cell_data syscall.
#[derive(Debug, Clone)]
pub struct CellMeta {
    pub out_point: OutPoint,
    pub data: Vec<u8>,
}

/// Cell data provider trait
pub trait CellDataProvider: Send + Sync {
    /// Get cell data by OutPoint
    fn get_cell_data(&self, out_point: &OutPoint) -> Option<Vec<u8>>;
}

/// Load cell data syscall
/// 
/// Loads the data field from a Cell. The data is the arbitrary payload
/// that can be stored in a Cell, used for storing smart contract state,
/// tokens, or any other application data.
///
/// # Arguments (via registers)
/// - A0: Output - return code (0 = success)
/// - A1: Output address - where to write the data
/// - A2: Output length - max bytes to write
/// - A3: Input index - which cell to load
/// - A4: Input source - where to load from (input/output/cell_dep)
/// - A7: Syscall number (2092)
///
/// # Returns
/// - SUCCESS (0): Data loaded successfully
/// - INDEX_OUT_OF_BOUND (1): Invalid index
#[derive(Debug)]
pub struct LoadCellData<DL> {
    inputs_data: Vec<Vec<u8>>,
    outputs_data: Vec<Vec<u8>>,
    deps_data: Vec<Vec<u8>>,
    data_loader: DL,
}

impl<DL: CellDataProvider> LoadCellData<DL> {
    /// Create a new LoadCellData syscall
    pub fn new(
        inputs_data: Vec<Vec<u8>>,
        outputs_data: Vec<Vec<u8>>,
        deps_data: Vec<Vec<u8>>,
        data_loader: DL,
    ) -> Self {
        Self {
            inputs_data,
            outputs_data,
            deps_data,
            data_loader,
        }
    }

    /// Fetch cell data from source
    fn fetch_data(&self, source: Source, index: usize) -> Result<&[u8], u8> {
        match source {
            Source::Input => {
                self.inputs_data
                    .get(index)
                    .map(|d| d.as_slice())
                    .ok_or(INDEX_OUT_OF_BOUND)
            }
            Source::Output => {
                self.outputs_data
                    .get(index)
                    .map(|d| d.as_slice())
                    .ok_or(INDEX_OUT_OF_BOUND)
            }
            Source::CellDep => {
                self.deps_data
                    .get(index)
                    .map(|d| d.as_slice())
                    .ok_or(INDEX_OUT_OF_BOUND)
            }
            _ => Err(INDEX_OUT_OF_BOUND),
        }
    }
}

impl<Mac: SupportMachine, DL: CellDataProvider> Syscalls<Mac> for LoadCellData<DL> {
    fn initialize(&mut self, _machine: &mut Mac) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut Mac) -> Result<bool, VMError> {
        let syscall_number = machine.registers()[A7].to_u64();
        
        if syscall_number != LOAD_CELL_DATA_SYSCALL_NUMBER {
            return Ok(false);
        }

        let index = machine.registers()[A3].to_u64() as usize;
        let source_value = machine.registers()[A4].to_u64();
        
        let source = Source::parse(source_value)
            .ok_or_else(|| VMError::Unexpected("Invalid source".to_string()))?;

        // Fetch the cell data
        let data = match self.fetch_data(source, index) {
            Ok(d) => d,
            Err(code) => {
                machine.set_register(A0, Mac::REG::from_u8(code));
                return Ok(true); // Handled but returned error code
            }
        };

        // Store data to VM memory
        let wrote_size = store_data(machine, data)?;

        // Charge cycles for data transfer
        machine.add_cycles_no_checking(transferred_byte_cycles(wrote_size))?;
        machine.set_register(A0, Mac::REG::from_u8(SUCCESS));
        
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockDataProvider;

    impl CellDataProvider for MockDataProvider {
        fn get_cell_data(&self, _out_point: &OutPoint) -> Option<Vec<u8>> {
            Some(vec![1, 2, 3, 4])
        }
    }

    #[test]
    fn test_load_cell_data_creation() {
        let provider = MockDataProvider;
        let inputs = vec![vec![0x01, 0x02, 0x03]];
        let outputs = vec![vec![0x04, 0x05, 0x06]];
        let deps = vec![];
        
        let syscall = LoadCellData::new(inputs, outputs, deps, provider);
        assert_eq!(syscall.inputs_data.len(), 1);
        assert_eq!(syscall.outputs_data.len(), 1);
    }

    #[test]
    fn test_fetch_data() {
        let provider = MockDataProvider;
        let inputs = vec![vec![0x01, 0x02, 0x03]];
        let outputs = vec![vec![0x04, 0x05, 0x06]];
        let deps = vec![vec![0x07, 0x08]];
        
        let syscall = LoadCellData::new(inputs, outputs, deps, provider);
        
        // Test input
        assert_eq!(syscall.fetch_data(Source::Input, 0).unwrap(), &[0x01, 0x02, 0x03]);
        assert!(syscall.fetch_data(Source::Input, 1).is_err());
        
        // Test output
        assert_eq!(syscall.fetch_data(Source::Output, 0).unwrap(), &[0x04, 0x05, 0x06]);
        
        // Test deps
        assert_eq!(syscall.fetch_data(Source::CellDep, 0).unwrap(), &[0x07, 0x08]);
    }
}

