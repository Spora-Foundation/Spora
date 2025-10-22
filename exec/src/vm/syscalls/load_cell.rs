// SPDX-License-Identifier: ISC
// Copyright (C) 2025Tondi developers
//
// Load cell syscall
// Adapted from CKB script/src/syscalls/load_cell.rs
// ⚠️ Modified: Blake2b → Blake3, adapted for SPORA data provider

use super::utils::store_data;
use super::{
    LOAD_CELL_SYSCALL_NUMBER, LOAD_CELL_BY_FIELD_SYSCALL_NUMBER,
    SUCCESS, INDEX_OUT_OF_BOUND, ITEM_MISSING, Source, CellField,
};
use crate::celltx::types::{CellOut, ScriptRef, OutPoint};
use crate::vm::cost_model::transferred_byte_cycles;
use ckb_vm::{
    Error as VMError, Register, SupportMachine, Syscalls,
    registers::{A0, A3, A4, A5, A7},
};

/// Cell metadata for VM access
#[derive(Debug, Clone)]
pub struct CellMeta {
    pub cell_output: CellOut,
    pub out_point: OutPoint,
    pub data: Option<Vec<u8>>,
}

/// Cell data provider trait (SPORA-specific)
pub trait CellDataProvider: Send + Sync {
    /// Get Cell by OutPoint
    fn get_cell(&self, out_point: &OutPoint) -> Option<CellMeta>;
}

/// Load cell syscall
#[derive(Debug)]
pub struct LoadCell<DL> {
    inputs: Vec<CellMeta>,
    outputs: Vec<CellOut>,
    deps: Vec<CellMeta>,
    data_loader: DL,
}

impl<DL: CellDataProvider> LoadCell<DL> {
    /// Create a new LoadCell syscall
    pub fn new(
        inputs: Vec<CellMeta>,
        outputs: Vec<CellOut>,
        deps: Vec<CellMeta>,
        data_loader: DL,
    ) -> Self {
        Self { inputs, outputs, deps, data_loader }
    }

    /// Fetch cell from source
    fn fetch_cell(&self, source: Source, index: usize) -> Result<&CellOut, u8> {
        match source {
            Source::Input => {
                self.inputs.get(index)
                    .map(|meta| &meta.cell_output)
                    .ok_or(INDEX_OUT_OF_BOUND)
            }
            Source::Output => {
                self.outputs.get(index)
                    .ok_or(INDEX_OUT_OF_BOUND)
            }
            Source::CellDep => {
                self.deps.get(index)
                    .map(|meta| &meta.cell_output)
                    .ok_or(INDEX_OUT_OF_BOUND)
            }
            _ => Err(INDEX_OUT_OF_BOUND),
        }
    }

    /// Serialize CellOutput
    fn serialize_cell_output(&self, cell: &CellOut) -> Vec<u8> {
        // Simple serialization: capacity || lock || type
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&cell.capacity.to_le_bytes());
        
        // Lock script
        bytes.extend_from_slice(&cell.lock.code_hash);
        bytes.push(cell.lock.hash_type);
        bytes.extend_from_slice(&(cell.lock.args.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&cell.lock.args);
        
        // Type script (optional)
        if let Some(ref type_script) = cell.type_ {
            bytes.push(1); // Present flag
            bytes.extend_from_slice(&type_script.code_hash);
            bytes.push(type_script.hash_type);
            bytes.extend_from_slice(&(type_script.args.len() as u32).to_le_bytes());
            bytes.extend_from_slice(&type_script.args);
        } else {
            bytes.push(0); // Not present
        }
        
        bytes
    }

    /// Load cell by field
    fn load_cell_by_field<Mac: SupportMachine>(
        &self,
        machine: &mut Mac,
        source: Source,
        index: usize,
        field: CellField,
    ) -> Result<usize, VMError> {
        let cell = match self.fetch_cell(source, index) {
            Ok(c) => c,
            Err(code) => {
                machine.set_register(A0, Mac::REG::from_u8(code));
                return Err(VMError::Unexpected("Cell not found".to_string()));
            }
        };

        let data = match field {
            CellField::Capacity => {
                cell.capacity.to_le_bytes().to_vec()
            }
            CellField::Lock => {
                self.serialize_script(&cell.lock)
            }
            CellField::LockHash => {
                cell.lock.hash().to_vec()
            }
            CellField::Type => {
                if let Some(ref type_script) = cell.type_ {
                    self.serialize_script(type_script)
                } else {
                    machine.set_register(A0, Mac::REG::from_u8(ITEM_MISSING));
                    return Err(VMError::Unexpected("Type script missing".to_string()));
                }
            }
            CellField::TypeHash => {
                if let Some(ref type_script) = cell.type_ {
                    type_script.hash().to_vec()
                } else {
                    machine.set_register(A0, Mac::REG::from_u8(ITEM_MISSING));
                    return Err(VMError::Unexpected("Type script missing".to_string()));
                }
            }
            CellField::OccupiedCapacity => {
                let occupied = cell.occupied_capacity(0); // TODO: get actual data length
                occupied.to_le_bytes().to_vec()
            }
            CellField::DataHash => {
                // TODO: Implement data hash
                machine.set_register(A0, Mac::REG::from_u8(ITEM_MISSING));
                return Err(VMError::Unexpected("Data hash not implemented".to_string()));
            }
        };

        store_data(machine, &data)
    }

    fn serialize_script(&self, script: &ScriptRef) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&script.code_hash);
        bytes.push(script.hash_type);
        bytes.extend_from_slice(&(script.args.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&script.args);
        bytes
    }
}

impl<Mac: SupportMachine, DL: CellDataProvider> Syscalls<Mac> for LoadCell<DL> {
    fn initialize(&mut self, _machine: &mut Mac) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut Mac) -> Result<bool, VMError> {
        let syscall_number = machine.registers()[A7].to_u64();
        
        match syscall_number {
            LOAD_CELL_SYSCALL_NUMBER => {
                let index = machine.registers()[A3].to_u64() as usize;
                let source_value = machine.registers()[A4].to_u64();
                
                let source = Source::parse(source_value)
                    .ok_or_else(|| VMError::Unexpected("Invalid source".to_string()))?;

                let cell = match self.fetch_cell(source, index) {
                    Ok(c) => c,
                    Err(code) => {
                        machine.set_register(A0, Mac::REG::from_u8(code));
                        return Err(VMError::Unexpected("Cell not found".to_string()));
                    }
                };

                let serialized = self.serialize_cell_output(cell);
                let wrote_size = store_data(machine, &serialized)?;

                machine.add_cycles_no_checking(transferred_byte_cycles(wrote_size))?;
                machine.set_register(A0, Mac::REG::from_u8(SUCCESS));
                
                Ok(true)
            }
            LOAD_CELL_BY_FIELD_SYSCALL_NUMBER => {
                let index = machine.registers()[A3].to_u64() as usize;
                let source_value = machine.registers()[A4].to_u64();
                let field_value = machine.registers()[A5].to_u64();
                
                let source = Source::parse(source_value)
                    .ok_or_else(|| VMError::Unexpected("Invalid source".to_string()))?;
                let field = CellField::parse(field_value)
                    .ok_or_else(|| VMError::Unexpected("Invalid field".to_string()))?;

                let wrote_size = self.load_cell_by_field(machine, source, index, field)?;

                machine.add_cycles_no_checking(transferred_byte_cycles(wrote_size))?;
                machine.set_register(A0, Mac::REG::from_u8(SUCCESS));
                
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockDataProvider;

    impl CellDataProvider for MockDataProvider {
        fn get_cell(&self, _out_point: &OutPoint) -> Option<CellMeta> {
            None
        }
    }

    #[test]
    fn test_load_cell_creation() {
        let provider = MockDataProvider;
        let syscall = LoadCell::new(vec![], vec![], vec![], provider);
        assert_eq!(syscall.inputs.len(), 0);
    }
}

