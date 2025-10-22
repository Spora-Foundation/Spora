// SPDX-License-Identifier: ISC
// Copyright (C) 2024 Tondi developers
//
// Load input syscall
// Adapted from CKB script/src/syscalls/load_input.rs
// ⚠️ Modified: Blake2b → Blake3, adapted for SPORA

use super::utils::store_data;
use super::{
    LOAD_INPUT_SYSCALL_NUMBER, LOAD_INPUT_BY_FIELD_SYSCALL_NUMBER,
    SUCCESS, INDEX_OUT_OF_BOUND, Source, InputField,
};
use crate::celltx::types::{CellRef, OutPoint};
use crate::vm::cost_model::transferred_byte_cycles;
use ckb_vm::{
    Error as VMError, Register, SupportMachine, Syscalls,
    registers::{A0, A3, A4, A5, A7},
};

/// Load input syscall
///
/// Loads information about transaction inputs (CellRefs).
/// An input references a previous Cell that is being consumed.
///
/// # Arguments (via registers)
/// - A0: Output - return code
/// - A1: Output address - where to write data
/// - A2: Output length - max bytes to write
/// - A3: Input index - which input to load
/// - A4: Input source - must be Source::Input
/// - A5: Input field (for LOAD_INPUT_BY_FIELD) - which field to load
/// - A7: Syscall number (2073 or 2083)
///
/// # Fields
/// - OutPoint: The reference to the Cell being consumed
/// - Since: Time-lock value (absolute/relative lock time)
///
/// # Returns
/// - SUCCESS (0): Input loaded successfully
/// - INDEX_OUT_OF_BOUND (1): Invalid index
#[derive(Debug)]
pub struct LoadInput {
    inputs: Vec<CellRef>,
}

impl LoadInput {
    /// Create a new LoadInput syscall
    pub fn new(inputs: Vec<CellRef>) -> Self {
        Self { inputs }
    }

    /// Serialize CellRef (input)
    fn serialize_input(&self, input: &CellRef) -> Vec<u8> {
        let mut bytes = Vec::new();
        
        // OutPoint: tx_hash (32 bytes) + index (4 bytes)
        bytes.extend_from_slice(&input.out_point.tx_hash);
        bytes.extend_from_slice(&input.out_point.index.to_le_bytes());
        
        // Since field (8 bytes) - time-lock value
        bytes.extend_from_slice(&input.since.to_le_bytes());
        
        bytes
    }

    /// Serialize OutPoint
    fn serialize_out_point(&self, out_point: &OutPoint) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&out_point.tx_hash);
        bytes.extend_from_slice(&out_point.index.to_le_bytes());
        bytes
    }

    /// Load input by field
    fn load_input_by_field<Mac: SupportMachine>(
        &self,
        machine: &mut Mac,
        index: usize,
        field: InputField,
    ) -> Result<usize, VMError> {
        let input = match self.inputs.get(index) {
            Some(i) => i,
            None => {
                machine.set_register(A0, Mac::REG::from_u8(INDEX_OUT_OF_BOUND));
                return Err(VMError::Unexpected("Input not found".to_string()));
            }
        };

        let data = match field {
            InputField::OutPoint => {
                self.serialize_out_point(&input.out_point)
            }
            InputField::Since => {
                input.since.to_le_bytes().to_vec()
            }
        };

        store_data(machine, &data)
    }
}

impl<Mac: SupportMachine> Syscalls<Mac> for LoadInput {
    fn initialize(&mut self, _machine: &mut Mac) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut Mac) -> Result<bool, VMError> {
        let syscall_number = machine.registers()[A7].to_u64();
        
        match syscall_number {
            LOAD_INPUT_SYSCALL_NUMBER => {
                let index = machine.registers()[A3].to_u64() as usize;
                let source_value = machine.registers()[A4].to_u64();
                
                // Inputs can only come from Source::Input
                let source = Source::parse(source_value)
                    .ok_or_else(|| VMError::Unexpected("Invalid source".to_string()))?;
                
                if !matches!(source, Source::Input) {
                    machine.set_register(A0, Mac::REG::from_u8(INDEX_OUT_OF_BOUND));
                    return Ok(true);
                }

                let input = match self.inputs.get(index) {
                    Some(i) => i,
                    None => {
                        machine.set_register(A0, Mac::REG::from_u8(INDEX_OUT_OF_BOUND));
                        return Ok(true);
                    }
                };

                let serialized = self.serialize_input(input);
                let wrote_size = store_data(machine, &serialized)?;

                machine.add_cycles_no_checking(transferred_byte_cycles(wrote_size))?;
                machine.set_register(A0, Mac::REG::from_u8(SUCCESS));
                
                Ok(true)
            }
            LOAD_INPUT_BY_FIELD_SYSCALL_NUMBER => {
                let index = machine.registers()[A3].to_u64() as usize;
                let source_value = machine.registers()[A4].to_u64();
                let field_value = machine.registers()[A5].to_u64();
                
                let source = Source::parse(source_value)
                    .ok_or_else(|| VMError::Unexpected("Invalid source".to_string()))?;
                
                if !matches!(source, Source::Input) {
                    machine.set_register(A0, Mac::REG::from_u8(INDEX_OUT_OF_BOUND));
                    return Ok(true);
                }

                let field = InputField::parse(field_value)
                    .ok_or_else(|| VMError::Unexpected("Invalid field".to_string()))?;

                let wrote_size = self.load_input_by_field(machine, index, field)?;

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

    fn create_test_input(tx_hash: [u8; 32], index: u32, since: u64) -> CellRef {
        CellRef {
            out_point: OutPoint { tx_hash, index },
            since,
        }
    }

    #[test]
    fn test_load_input_creation() {
        let inputs = vec![create_test_input([1u8; 32], 0, 0)];
        let syscall = LoadInput::new(inputs);
        assert_eq!(syscall.inputs.len(), 1);
    }

    #[test]
    fn test_serialize_input() {
        let input = create_test_input([0x42u8; 32], 5, 12345);
        let syscall = LoadInput::new(vec![input.clone()]);
        
        let serialized = syscall.serialize_input(&input);
        
        // Should be: 32 bytes (tx_hash) + 4 bytes (index) + 8 bytes (since) = 44 bytes
        assert_eq!(serialized.len(), 44);
        
        // Verify tx_hash
        assert_eq!(&serialized[0..32], &[0x42u8; 32]);
        
        // Verify index (5 in little-endian)
        assert_eq!(&serialized[32..36], &[5, 0, 0, 0]);
        
        // Verify since (12345 in little-endian)
        let since_bytes = 12345u64.to_le_bytes();
        assert_eq!(&serialized[36..44], &since_bytes);
    }

    #[test]
    fn test_serialize_out_point() {
        let out_point = OutPoint {
            tx_hash: [0xAAu8; 32],
            index: 7,
        };
        let syscall = LoadInput::new(vec![]);
        
        let serialized = syscall.serialize_out_point(&out_point);
        
        // Should be: 32 bytes (tx_hash) + 4 bytes (index) = 36 bytes
        assert_eq!(serialized.len(), 36);
        assert_eq!(&serialized[0..32], &[0xAAu8; 32]);
        assert_eq!(&serialized[32..36], &[7, 0, 0, 0]);
    }
}

