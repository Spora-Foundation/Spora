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
/// Note: In DAG, headers are multi-parent and the serialized runtime view preserves
/// the richer header fields exposed by `ResolvedHeader`.
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
            HeaderField::Parents => Some(header.direct_parents().iter().flatten().copied().collect()),
            HeaderField::Version => Some(header.version.to_le_bytes().to_vec()),
            HeaderField::Bits => Some(header.bits.to_le_bytes().to_vec()),
            HeaderField::Nonce => Some(header.nonce.to_le_bytes().to_vec()),
            HeaderField::HashMerkleRoot => Some(header.hash_merkle_root.to_vec()),
            HeaderField::AcceptedIdMerkleRoot => Some(header.accepted_id_merkle_root.to_vec()),
            HeaderField::CellCommitment => Some(header.cell_commitment.to_vec()),
            HeaderField::CellRoot => Some(header.cell_root.to_vec()),
            HeaderField::SegmentRoot => Some(header.segment_root.to_vec()),
            HeaderField::BlueScore => Some(header.blue_score.to_le_bytes().to_vec()),
            HeaderField::BlueWork => Some(header.blue_work.to_vec()),
            HeaderField::PruningPoint => Some(header.pruning_point.to_vec()),
        }
    }

    fn serialize_header(&self, header: &ResolvedHeader) -> Result<Vec<u8>, VMError> {
        borsh::to_vec(header).map_err(|e| VMError::Unexpected(format!("Failed to serialize header: {e}")))
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
    use borsh::BorshDeserialize;
    use ckb_vm::{
        registers::{A1, A2},
        CoreMachine, Memory, Register,
    };

    const BUFFER_ADDR: u64 = 0x1000;
    const SIZE_ADDR: u64 = 0x2000;

    fn resolved_header(header_hash: [u8; 32]) -> ResolvedHeader {
        ResolvedHeader {
            hash: header_hash,
            version: 7,
            parents_by_level: vec![vec![[0xAA; 32], [0xBB; 32]], vec![[0xCC; 32]]],
            hash_merkle_root: [0x10; 32],
            accepted_id_merkle_root: [0x20; 32],
            cell_commitment: [0x30; 32],
            cell_root: [0x40; 32],
            segment_root: [0x50; 32],
            timestamp: 0x0102_0304_0506_0708,
            bits: 0x1d00_ffff,
            nonce: 0x8877_6655_4433_2211,
            daa_score: 0x1122_3344_5566_7788,
            blue_work: [0x60; 24],
            blue_score: 0x99AA_BBCC_DDEE_FF00,
            pruning_point: [0x70; 32],
        }
    }

    fn build_tx_and_provider() -> (Arc<CellTx>, Arc<SimpleDataProvider>) {
        let header_hash = [0x77; 32];
        let tx = Arc::new(CellTx {
            version: 0xC001,
            inputs: vec![],
            cell_deps: vec![],
            header_deps: vec![header_hash],
            outputs: vec![],
            outputs_data: vec![],
            witnesses: vec![],
        });
        let mut provider = SimpleDataProvider::new();
        provider.add_header(header_hash, resolved_header(header_hash));
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

    #[test]
    fn test_load_header_returns_richer_header_view() {
        let (tx, provider) = build_tx_and_provider();
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.memory_mut().store64(&SIZE_ADDR, &512u64).unwrap();
        machine.set_register(A0, BUFFER_ADDR);
        machine.set_register(A1, SIZE_ADDR);
        machine.set_register(A2, 0);
        machine.set_register(A3, 0);
        machine.set_register(A4, Source::HeaderDep as u64);
        machine.set_register(A7, LOAD_HEADER_SYSCALL_NUMBER);

        let mut syscall = LoadHeader::new(tx, provider);
        let handled = syscall.ecall(&mut machine).expect("load header syscall should succeed");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), SUCCESS as u64);
        let size = machine.memory_mut().load64(&SIZE_ADDR).unwrap().to_u64();
        let bytes = machine.memory_mut().load_bytes(BUFFER_ADDR, size).unwrap();
        let header = ResolvedHeader::try_from_slice(bytes.as_ref()).expect("header should deserialize");
        assert_eq!(header, resolved_header([0x77; 32]));
    }

    #[test]
    fn test_load_header_by_field_supports_blue_work() {
        let (tx, provider) = build_tx_and_provider();
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.memory_mut().store64(&SIZE_ADDR, &24u64).unwrap();
        machine.set_register(A0, BUFFER_ADDR);
        machine.set_register(A1, SIZE_ADDR);
        machine.set_register(A2, 0);
        machine.set_register(A3, 0);
        machine.set_register(A4, Source::HeaderDep as u64);
        machine.set_register(A5, HeaderField::BlueWork as u64);
        machine.set_register(A7, LOAD_HEADER_BY_FIELD_SYSCALL_NUMBER);

        let mut syscall = LoadHeader::new(tx, provider);
        let handled = syscall.ecall(&mut machine).expect("load header by field should succeed");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), SUCCESS as u64);
        assert_eq!(machine.memory_mut().load64(&SIZE_ADDR).unwrap().to_u64(), 24);
        assert_eq!(machine.memory_mut().load_bytes(BUFFER_ADDR, 24).unwrap().as_ref(), &[0x60; 24]);
    }
}
