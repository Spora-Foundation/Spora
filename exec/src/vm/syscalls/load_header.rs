// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Load header syscall (DAG-aware)

use super::utils::{store_data, INDEX_OUT_OF_BOUND, ITEM_MISSING, SUCCESS};
use super::{HeaderField, Source, LOAD_HEADER_BY_FIELD_SYSCALL_NUMBER, LOAD_HEADER_SYSCALL_NUMBER};
use crate::celltx::CellTx;
use crate::vm::{CellDataProvider, ResolvedHeader};
use ckb_vm::{
    registers::{A0, A3, A4, A5, A7},
    Error as VMError, Register, SupportMachine, Syscalls,
};
use std::sync::Arc;

/// Syscall: Load Header
///
/// Syscall number: 2072
///
/// Note: In DAG, headers are more complex (multi-parent)
/// This is a simplified implementation
pub struct LoadHeader<D: CellDataProvider> {
    tx: Arc<CellTx>,
    provider: Arc<D>,
}

impl<D: CellDataProvider> LoadHeader<D> {
    pub fn new(tx: Arc<CellTx>, provider: Arc<D>) -> Self {
        Self { tx, provider }
    }

    fn get_header(&self, source: u64, index: usize) -> Option<ResolvedHeader> {
        match Source::parse(source)? {
            Source::HeaderDep => {
                let hash = self.tx.header_deps.get(index)?;
                self.provider.load_header(hash)
            }
            _ => None,
        }
    }

    fn serialize_header_field(&self, header: &ResolvedHeader, field: u64) -> Option<Vec<u8>> {
        match HeaderField::parse(field)? {
            HeaderField::DaaScore => Some(header.daa_score.to_le_bytes().to_vec()),
            HeaderField::Timestamp => Some(header.timestamp.to_le_bytes().to_vec()),
            HeaderField::Hash => Some(header.hash.to_vec()),
            HeaderField::Parents => Some(header.parents.iter().flatten().copied().collect()),
        }
    }

    fn serialize_header(&self, header: &ResolvedHeader) -> Result<Vec<u8>, VMError> {
        borsh::to_vec(&(header.hash, header.timestamp, header.daa_score, header.parents.clone()))
            .map_err(|e| VMError::Unexpected(format!("Failed to serialize header: {e}")))
    }
}

impl<D: CellDataProvider, M: SupportMachine> Syscalls<M> for LoadHeader<D> {
    fn initialize(&mut self, _machine: &mut M) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut M) -> Result<bool, VMError> {
        let syscall_number = machine.registers()[A7].to_u64();

        // LOAD_HEADER = 2072 or LOAD_HEADER_BY_FIELD = 2082
        if syscall_number != LOAD_HEADER_SYSCALL_NUMBER && syscall_number != LOAD_HEADER_BY_FIELD_SYSCALL_NUMBER {
            return Ok(false);
        }

        let index = machine.registers()[A3].to_u64() as usize;
        let source = machine.registers()[A4].to_u64();

        let header = match self.get_header(source, index) {
            Some(header) => header,
            None => {
                machine.set_register(A0, M::REG::from_u8(INDEX_OUT_OF_BOUND));
                return Ok(true);
            }
        };

        let data = if syscall_number == LOAD_HEADER_BY_FIELD_SYSCALL_NUMBER {
            let field = machine.registers()[A5].to_u64();
            match self.serialize_header_field(&header, field) {
                Some(data) => data,
                None => {
                    machine.set_register(A0, M::REG::from_u8(ITEM_MISSING));
                    return Ok(true);
                }
            }
        } else {
            self.serialize_header(&header)?
        };
        store_data(machine, &data)?;
        machine.set_register(A0, M::REG::from_u8(SUCCESS));

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::{ScriptVersion, SimpleDataProvider};
    use ckb_vm::{
        registers::{A1, A2},
        CoreMachine, Memory, Register,
    };

    const BUFFER_ADDR: u64 = 0x1000;
    const SIZE_ADDR: u64 = 0x2000;

    fn build_tx_and_provider() -> (Arc<CellTx>, Arc<SimpleDataProvider>) {
        let header_hash = [0x77; 32];
        let tx = Arc::new(CellTx {
            ver: 0xC001,
            inputs: vec![],
            deps: vec![],
            header_deps: vec![header_hash],
            outputs: vec![],
            outputs_data: vec![],
            witnesses: vec![],
        });
        let mut provider = SimpleDataProvider::new();
        provider.add_header(
            header_hash,
            ResolvedHeader {
                hash: header_hash,
                timestamp: 0x0102_0304_0506_0708,
                daa_score: 0x1122_3344_5566_7788,
                parents: vec![[0xAA; 32], [0xBB; 32]],
            },
        );
        (tx, Arc::new(provider))
    }

    #[test]
    fn test_load_header_by_field_supports_partial_reads() {
        let (tx, provider) = build_tx_and_provider();
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.memory_mut().store64(&SIZE_ADDR, &4u64).unwrap();
        machine.set_register(A0, BUFFER_ADDR);
        machine.set_register(A1, SIZE_ADDR);
        machine.set_register(A2, 2);
        machine.set_register(A3, 0);
        machine.set_register(A4, Source::HeaderDep as u64);
        machine.set_register(A5, HeaderField::Timestamp as u64);
        machine.set_register(A7, LOAD_HEADER_BY_FIELD_SYSCALL_NUMBER);

        let mut syscall = LoadHeader::new(tx, provider);
        let handled = syscall.ecall(&mut machine).expect("load header syscall should succeed");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), SUCCESS as u64);
        assert_eq!(machine.memory_mut().load64(&SIZE_ADDR).unwrap().to_u64(), 6);
        assert_eq!(machine.memory_mut().load_bytes(BUFFER_ADDR, 4).unwrap().as_ref(), &[0x06, 0x05, 0x04, 0x03]);
    }

    #[test]
    fn test_load_header_rejects_invalid_source() {
        let (tx, provider) = build_tx_and_provider();
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.memory_mut().store64(&SIZE_ADDR, &8u64).unwrap();
        machine.set_register(A0, BUFFER_ADDR);
        machine.set_register(A1, SIZE_ADDR);
        machine.set_register(A2, 0);
        machine.set_register(A3, 0);
        machine.set_register(A4, Source::Input as u64);
        machine.set_register(A7, LOAD_HEADER_SYSCALL_NUMBER);

        let mut syscall = LoadHeader::new(tx, provider);
        let handled = syscall.ecall(&mut machine).expect("load header syscall should be handled");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), INDEX_OUT_OF_BOUND as u64);
    }

    #[test]
    fn test_load_header_by_field_rejects_unknown_field() {
        let (tx, provider) = build_tx_and_provider();
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.memory_mut().store64(&SIZE_ADDR, &8u64).unwrap();
        machine.set_register(A0, BUFFER_ADDR);
        machine.set_register(A1, SIZE_ADDR);
        machine.set_register(A2, 0);
        machine.set_register(A3, 0);
        machine.set_register(A4, Source::HeaderDep as u64);
        machine.set_register(A5, 99);
        machine.set_register(A7, LOAD_HEADER_BY_FIELD_SYSCALL_NUMBER);

        let mut syscall = LoadHeader::new(tx, provider);
        let handled = syscall.ecall(&mut machine).expect("load header syscall should be handled");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), ITEM_MISSING as u64);
    }
}
