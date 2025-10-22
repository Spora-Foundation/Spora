// SPDX-License-Identifier: ISC
// Copyright (C) 2024 Tondi developers
//
// Load header syscall
// Adapted from CKB script/src/syscalls/load_header.rs
// ⚠️ Modified: Blake2b → Blake3, adapted for SPORA DAG (multiple parents)

use super::utils::store_data;
use super::{
    LOAD_HEADER_SYSCALL_NUMBER, LOAD_HEADER_BY_FIELD_SYSCALL_NUMBER,
    SUCCESS, INDEX_OUT_OF_BOUND, ITEM_MISSING, Source, HeaderField,
};
use crate::vm::cost_model::transferred_byte_cycles;
use ckb_vm::{
    Error as VMError, Register, SupportMachine, Syscalls,
    registers::{A0, A3, A4, A5, A7},
};

/// Block header (SPORA DAG-aware)
#[derive(Debug, Clone)]
pub struct Header {
    /// Block hash
    pub hash: [u8; 32],
    /// Parent hashes (DAG - multiple parents)
    pub parents: Vec<[u8; 32]>,
    /// DAA score (replaces CKB's block_number)
    pub daa_score: u64,
    /// Timestamp (milliseconds)
    pub timestamp: u64,
    /// Difficulty bits
    pub bits: u32,
    /// Nonce
    pub nonce: u64,
    /// Blue score (GHOSTDAG)
    pub blue_score: u64,
}

/// Header provider trait (DAG-aware)
pub trait HeaderProvider: Send + Sync {
    /// Get header by hash
    fn get_header(&self, hash: &[u8; 32]) -> Option<Header>;
    
    /// Get header at DAA score (for relative lookups)
    fn get_header_by_daa_score(&self, daa_score: u64) -> Option<Header>;
}

/// Load header syscall
///
/// Loads block header information. SPORA adaptation includes:
/// - DAA score instead of block number
/// - Multiple parent hashes (DAG structure)
/// - Blue score for GHOSTDAG ordering
///
/// # Arguments (via registers)
/// - A0: Output - return code
/// - A1: Output address - where to write data
/// - A2: Output length - max bytes to write
/// - A3: Input index - which header to load
/// - A4: Input source - HeaderDep only
/// - A5: Input field (for LOAD_HEADER_BY_FIELD) - which field to load
/// - A7: Syscall number (2072 or 2082)
///
/// # Returns
/// - SUCCESS (0): Header loaded successfully
/// - INDEX_OUT_OF_BOUND (1): Invalid index
/// - ITEM_MISSING (2): Header not found
#[derive(Debug)]
pub struct LoadHeader<HP> {
    headers: Vec<Header>,
    header_provider: HP,
}

impl<HP: HeaderProvider> LoadHeader<HP> {
    /// Create a new LoadHeader syscall
    pub fn new(headers: Vec<Header>, header_provider: HP) -> Self {
        Self {
            headers,
            header_provider,
        }
    }

    /// Serialize header (SPORA format with DAG parents)
    fn serialize_header(&self, header: &Header) -> Vec<u8> {
        let mut bytes = Vec::new();
        
        // Hash (32 bytes)
        bytes.extend_from_slice(&header.hash);
        
        // Number of parents (4 bytes) - DAG specific
        bytes.extend_from_slice(&(header.parents.len() as u32).to_le_bytes());
        
        // Parent hashes (32 bytes each)
        for parent in &header.parents {
            bytes.extend_from_slice(parent);
        }
        
        // DAA score (8 bytes) - replaces block_number
        bytes.extend_from_slice(&header.daa_score.to_le_bytes());
        
        // Timestamp (8 bytes)
        bytes.extend_from_slice(&header.timestamp.to_le_bytes());
        
        // Bits (4 bytes)
        bytes.extend_from_slice(&header.bits.to_le_bytes());
        
        // Nonce (8 bytes)
        bytes.extend_from_slice(&header.nonce.to_le_bytes());
        
        // Blue score (8 bytes) - GHOSTDAG specific
        bytes.extend_from_slice(&header.blue_score.to_le_bytes());
        
        bytes
    }

    /// Serialize parent hashes
    fn serialize_parents(&self, parents: &[[u8; 32]]) -> Vec<u8> {
        let mut bytes = Vec::new();
        
        // Number of parents
        bytes.extend_from_slice(&(parents.len() as u32).to_le_bytes());
        
        // Parent hashes
        for parent in parents {
            bytes.extend_from_slice(parent);
        }
        
        bytes
    }

    /// Load header by field
    fn load_header_by_field<Mac: SupportMachine>(
        &self,
        machine: &mut Mac,
        index: usize,
        field: HeaderField,
    ) -> Result<usize, VMError> {
        let header = match self.headers.get(index) {
            Some(h) => h,
            None => {
                machine.set_register(A0, Mac::REG::from_u8(INDEX_OUT_OF_BOUND));
                return Err(VMError::Unexpected("Header not found".to_string()));
            }
        };

        let data = match field {
            HeaderField::DaaScore => {
                header.daa_score.to_le_bytes().to_vec()
            }
            HeaderField::Timestamp => {
                header.timestamp.to_le_bytes().to_vec()
            }
            HeaderField::Hash => {
                header.hash.to_vec()
            }
            HeaderField::Parents => {
                self.serialize_parents(&header.parents)
            }
        };

        store_data(machine, &data)
    }
}

impl<Mac: SupportMachine, HP: HeaderProvider> Syscalls<Mac> for LoadHeader<HP> {
    fn initialize(&mut self, _machine: &mut Mac) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut Mac) -> Result<bool, VMError> {
        let syscall_number = machine.registers()[A7].to_u64();
        
        match syscall_number {
            LOAD_HEADER_SYSCALL_NUMBER => {
                let index = machine.registers()[A3].to_u64() as usize;
                let source_value = machine.registers()[A4].to_u64();
                
                // Headers can only come from Source::HeaderDep
                let source = Source::parse(source_value)
                    .ok_or_else(|| VMError::Unexpected("Invalid source".to_string()))?;
                
                if !matches!(source, Source::HeaderDep) {
                    machine.set_register(A0, Mac::REG::from_u8(INDEX_OUT_OF_BOUND));
                    return Ok(true);
                }

                let header = match self.headers.get(index) {
                    Some(h) => h,
                    None => {
                        machine.set_register(A0, Mac::REG::from_u8(INDEX_OUT_OF_BOUND));
                        return Ok(true);
                    }
                };

                let serialized = self.serialize_header(header);
                let wrote_size = store_data(machine, &serialized)?;

                machine.add_cycles_no_checking(transferred_byte_cycles(wrote_size))?;
                machine.set_register(A0, Mac::REG::from_u8(SUCCESS));
                
                Ok(true)
            }
            LOAD_HEADER_BY_FIELD_SYSCALL_NUMBER => {
                let index = machine.registers()[A3].to_u64() as usize;
                let source_value = machine.registers()[A4].to_u64();
                let field_value = machine.registers()[A5].to_u64();
                
                let source = Source::parse(source_value)
                    .ok_or_else(|| VMError::Unexpected("Invalid source".to_string()))?;
                
                if !matches!(source, Source::HeaderDep) {
                    machine.set_register(A0, Mac::REG::from_u8(INDEX_OUT_OF_BOUND));
                    return Ok(true);
                }

                let field = HeaderField::parse(field_value)
                    .ok_or_else(|| VMError::Unexpected("Invalid field".to_string()))?;

                let wrote_size = self.load_header_by_field(machine, index, field)?;

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

    struct MockHeaderProvider;

    impl HeaderProvider for MockHeaderProvider {
        fn get_header(&self, _hash: &[u8; 32]) -> Option<Header> {
            None
        }
        
        fn get_header_by_daa_score(&self, _daa_score: u64) -> Option<Header> {
            None
        }
    }

    fn create_test_header() -> Header {
        Header {
            hash: [0x42u8; 32],
            parents: vec![[0x01u8; 32], [0x02u8; 32]],  // Two parents (DAG)
            daa_score: 12345,
            timestamp: 1234567890,
            bits: 0x1d00ffff,
            nonce: 999,
            blue_score: 12340,
        }
    }

    #[test]
    fn test_load_header_creation() {
        let provider = MockHeaderProvider;
        let headers = vec![create_test_header()];
        let syscall = LoadHeader::new(headers, provider);
        assert_eq!(syscall.headers.len(), 1);
    }

    #[test]
    fn test_serialize_header() {
        let provider = MockHeaderProvider;
        let header = create_test_header();
        let syscall = LoadHeader::new(vec![header.clone()], provider);
        
        let serialized = syscall.serialize_header(&header);
        
        // Verify structure:
        // 32 (hash) + 4 (num_parents) + 64 (2 parents * 32) + 8 (daa_score) 
        // + 8 (timestamp) + 4 (bits) + 8 (nonce) + 8 (blue_score) = 136 bytes
        assert_eq!(serialized.len(), 136);
        
        // Verify hash
        assert_eq!(&serialized[0..32], &[0x42u8; 32]);
        
        // Verify number of parents
        assert_eq!(&serialized[32..36], &[2, 0, 0, 0]);
        
        // Verify first parent
        assert_eq!(&serialized[36..68], &[0x01u8; 32]);
        
        // Verify second parent
        assert_eq!(&serialized[68..100], &[0x02u8; 32]);
    }

    #[test]
    fn test_serialize_parents() {
        let provider = MockHeaderProvider;
        let syscall = LoadHeader::new(vec![], provider);
        
        let parents = vec![[0xAAu8; 32], [0xBBu8; 32], [0xCCu8; 32]];
        let serialized = syscall.serialize_parents(&parents);
        
        // 4 bytes (count) + 3 * 32 bytes (parents) = 100 bytes
        assert_eq!(serialized.len(), 100);
        
        // Verify count
        assert_eq!(&serialized[0..4], &[3, 0, 0, 0]);
        
        // Verify parents
        assert_eq!(&serialized[4..36], &[0xAAu8; 32]);
        assert_eq!(&serialized[36..68], &[0xBBu8; 32]);
        assert_eq!(&serialized[68..100], &[0xCCu8; 32]);
    }

    #[test]
    fn test_dag_multiple_parents() {
        // Test that DAG structure with multiple parents works correctly
        let provider = MockHeaderProvider;
        let header = Header {
            hash: [0x99u8; 32],
            parents: vec![
                [0x10u8; 32],
                [0x20u8; 32],
                [0x30u8; 32],
                [0x40u8; 32],
            ],  // Four parents - typical DAG case
            daa_score: 100,
            timestamp: 999,
            bits: 0x1d00ffff,
            nonce: 1,
            blue_score: 95,
        };
        
        let syscall = LoadHeader::new(vec![header.clone()], provider);
        let serialized = syscall.serialize_header(&header);
        
        // Verify correct structure with 4 parents
        // 32 + 4 + 128 (4*32) + 8 + 8 + 4 + 8 + 8 = 200 bytes
        assert_eq!(serialized.len(), 200);
    }
}

