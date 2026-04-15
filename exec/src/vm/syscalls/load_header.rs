// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Load header syscall (DAG-aware)

use super::utils::{store_data, INDEX_OUT_OF_BOUND, ITEM_MISSING};
use super::{HeaderField, Source, LOAD_HEADER_BY_FIELD_SYSCALL_NUMBER, LOAD_HEADER_SYSCALL_NUMBER};
use crate::celltx::CellTx;
use crate::vm::transferred_byte_cycles;
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
    group_input_indices: Vec<usize>,
    group_output_indices: Vec<usize>,
}

enum HeaderLookupResult {
    Header(ResolvedHeader),
    IndexOutOfBound,
    ItemMissing,
}

impl<D: CellDataProvider> LoadHeader<D> {
    pub fn new(tx: Arc<CellTx>, provider: Arc<D>, group_input_indices: Vec<usize>, group_output_indices: Vec<usize>) -> Self {
        Self { tx, provider, group_input_indices, group_output_indices }
    }

    fn get_header(&self, source: Source, index: usize) -> HeaderLookupResult {
        match source {
            Source::Input => match self.tx.inputs.get(index) {
                Some(input) => self
                    .provider
                    .load_header_by_outpoint(&input.previous_output.tx_hash, input.previous_output.index)
                    .map(HeaderLookupResult::Header)
                    .unwrap_or(HeaderLookupResult::ItemMissing),
                None => HeaderLookupResult::IndexOutOfBound,
            },
            Source::CellDep => match self.tx.cell_deps.get(index) {
                Some(dep) => self
                    .provider
                    .load_header_by_outpoint(&dep.out_point.tx_hash, dep.out_point.index)
                    .map(HeaderLookupResult::Header)
                    .unwrap_or(HeaderLookupResult::ItemMissing),
                None => HeaderLookupResult::IndexOutOfBound,
            },
            Source::HeaderDep => match self.tx.header_deps.get(index) {
                Some(hash) => {
                    self.provider.load_header(hash).map(HeaderLookupResult::Header).unwrap_or(HeaderLookupResult::ItemMissing)
                }
                None => HeaderLookupResult::IndexOutOfBound,
            },
            Source::GroupInput => match self.group_input_indices.get(index).and_then(|&idx| self.tx.inputs.get(idx)) {
                Some(input) => self
                    .provider
                    .load_header_by_outpoint(&input.previous_output.tx_hash, input.previous_output.index)
                    .map(HeaderLookupResult::Header)
                    .unwrap_or(HeaderLookupResult::ItemMissing),
                None => HeaderLookupResult::IndexOutOfBound,
            },
            Source::Output => {
                if self.tx.outputs.get(index).is_some() {
                    HeaderLookupResult::ItemMissing
                } else {
                    HeaderLookupResult::IndexOutOfBound
                }
            }
            Source::GroupOutput => match self.group_output_indices.get(index).and_then(|&idx| self.tx.outputs.get(idx)) {
                Some(_) => HeaderLookupResult::ItemMissing,
                None => HeaderLookupResult::IndexOutOfBound,
            },
            Source::GroupCellDep | Source::GroupHeaderDep => HeaderLookupResult::IndexOutOfBound,
        }
    }

    fn serialize_header_field(&self, header: &ResolvedHeader, field: u64) -> Result<Vec<u8>, VMError> {
        match HeaderField::parse_from_u64(field)? {
            HeaderField::DaaScore => Ok(header.daa_score.to_le_bytes().to_vec()),
            HeaderField::Timestamp => Ok(header.timestamp.to_le_bytes().to_vec()),
            HeaderField::Hash => Ok(header.hash.to_vec()),
            HeaderField::Parents => Ok(header.direct_parents().iter().flatten().copied().collect()),
            HeaderField::Version => Ok(header.version.to_le_bytes().to_vec()),
            HeaderField::Bits => Ok(header.bits.to_le_bytes().to_vec()),
            HeaderField::Nonce => Ok(header.nonce.to_le_bytes().to_vec()),
            HeaderField::HashMerkleRoot => Ok(header.hash_merkle_root.to_vec()),
            HeaderField::AcceptedIdMerkleRoot => Ok(header.accepted_id_merkle_root.to_vec()),
            HeaderField::CellCommitment => Ok(header.cell_commitment.to_vec()),
            HeaderField::CellRoot => Ok(header.cell_root.to_vec()),
            HeaderField::SegmentRoot => Ok(header.segment_root.to_vec()),
            HeaderField::BlueScore => Ok(header.blue_score.to_le_bytes().to_vec()),
            HeaderField::BlueWork => Ok(header.blue_work.to_vec()),
            HeaderField::PruningPoint => Ok(header.pruning_point.to_vec()),
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
        let source = Source::parse_from_u64(machine.registers()[A4].to_u64())?;

        let header = match self.get_header(source, index) {
            HeaderLookupResult::Header(header) => header,
            HeaderLookupResult::IndexOutOfBound => {
                machine.set_register(A0, M::REG::from_u8(INDEX_OUT_OF_BOUND));
                return Ok(true);
            }
            HeaderLookupResult::ItemMissing => {
                machine.set_register(A0, M::REG::from_u8(ITEM_MISSING));
                return Ok(true);
            }
        };

        let data = if syscall_number == LOAD_HEADER_BY_FIELD_SYSCALL_NUMBER {
            let field = machine.registers()[A5].to_u64();
            self.serialize_header_field(&header, field)?
        } else {
            self.serialize_header(&header)?
        };
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
        let input_out_point = crate::celltx::OutPoint::new([0x11; 32], 0);
        let dep_out_point = crate::celltx::OutPoint::new([0x22; 32], 1);
        let input_header_hash = [0x55; 32];
        let dep_header_hash = [0x66; 32];
        let header_hash = [0x77; 32];
        let tx = Arc::new(CellTx {
            version: 0xC001,
            inputs: vec![crate::celltx::CellInput::new(input_out_point, 0)],
            cell_deps: vec![crate::celltx::CellDep { out_point: dep_out_point, dep_type: crate::celltx::DepType::Code }],
            header_deps: vec![header_hash],
            outputs: vec![],
            outputs_data: vec![],
            witnesses: vec![],
        });
        let mut provider = SimpleDataProvider::new();
        provider.add_cell_with_header(
            [0x11; 32],
            0,
            crate::vm::ResolvedCell {
                cell_output: crate::celltx::CellOutput {
                    capacity: 1_000,
                    lock: crate::celltx::Script::new([0x01; 32], 0, vec![]),
                    type_: None,
                },
                data: Some(vec![]),
            },
            input_header_hash,
        );
        provider.add_cell_with_header(
            [0x22; 32],
            1,
            crate::vm::ResolvedCell {
                cell_output: crate::celltx::CellOutput {
                    capacity: 2_000,
                    lock: crate::celltx::Script::new([0x02; 32], 0, vec![]),
                    type_: None,
                },
                data: Some(vec![]),
            },
            dep_header_hash,
        );
        provider.add_header(input_header_hash, resolved_header(input_header_hash));
        provider.add_header(dep_header_hash, resolved_header(dep_header_hash));
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

        let mut syscall = LoadHeader::new(tx, provider, vec![0], vec![]);
        let handled = syscall.ecall(&mut machine).expect("load header syscall should succeed");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), SUCCESS as u64);
        assert_eq!(machine.memory_mut().load64(&SIZE_ADDR).unwrap().to_u64(), 6);
        assert_eq!(machine.memory_mut().load_bytes(BUFFER_ADDR, 4).unwrap().as_ref(), &[0x06, 0x05, 0x04, 0x03]);
    }

    #[test]
    fn test_load_header_output_source_reports_item_missing_when_output_exists() {
        let (tx, provider) = build_tx_and_provider();
        let tx = Arc::new(CellTx {
            outputs: vec![crate::celltx::CellOutput {
                capacity: 42,
                lock: crate::celltx::Script::new([0x33; 32], 0, vec![]),
                type_: None,
            }],
            outputs_data: vec![vec![]],
            ..(*tx).clone()
        });
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.memory_mut().store64(&SIZE_ADDR, &8u64).unwrap();
        machine.set_register(A0, BUFFER_ADDR);
        machine.set_register(A1, SIZE_ADDR);
        machine.set_register(A2, 0);
        machine.set_register(A3, 0);
        machine.set_register(A4, Source::Output as u64);
        machine.set_register(A7, LOAD_HEADER_SYSCALL_NUMBER);

        let mut syscall = LoadHeader::new(tx, provider, vec![0], vec![]);
        let handled = syscall.ecall(&mut machine).expect("load header syscall should be handled");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), ITEM_MISSING as u64);
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

        let mut syscall = LoadHeader::new(tx, provider, vec![0], vec![]);
        let err = syscall.ecall(&mut machine).expect_err("unknown field should trap");

        assert_eq!(err, VMError::External("HeaderField parse_from_u64 99".to_string()));
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

        let mut syscall = LoadHeader::new(tx, provider, vec![0], vec![]);
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

        let mut syscall = LoadHeader::new(tx, provider, vec![0], vec![]);
        let handled = syscall.ecall(&mut machine).expect("load header by field should succeed");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), SUCCESS as u64);
        assert_eq!(machine.memory_mut().load64(&SIZE_ADDR).unwrap().to_u64(), 24);
        assert_eq!(machine.memory_mut().load_bytes(BUFFER_ADDR, 24).unwrap().as_ref(), &[0x60; 24]);
    }

    #[test]
    fn test_load_header_supports_input_source() {
        let (tx, provider) = build_tx_and_provider();
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.memory_mut().store64(&SIZE_ADDR, &512u64).unwrap();
        machine.set_register(A0, BUFFER_ADDR);
        machine.set_register(A1, SIZE_ADDR);
        machine.set_register(A2, 0);
        machine.set_register(A3, 0);
        machine.set_register(A4, Source::Input as u64);
        machine.set_register(A7, LOAD_HEADER_SYSCALL_NUMBER);

        let mut syscall = LoadHeader::new(tx, provider, vec![0], vec![]);
        let handled = syscall.ecall(&mut machine).expect("load header syscall should succeed");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), SUCCESS as u64);
        let size = machine.memory_mut().load64(&SIZE_ADDR).unwrap().to_u64();
        let bytes = machine.memory_mut().load_bytes(BUFFER_ADDR, size).unwrap();
        let header = ResolvedHeader::try_from_slice(bytes.as_ref()).expect("header should deserialize");
        assert_eq!(header.hash, [0x55; 32]);
    }

    #[test]
    fn test_load_header_supports_cell_dep_source() {
        let (tx, provider) = build_tx_and_provider();
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.memory_mut().store64(&SIZE_ADDR, &32u64).unwrap();
        machine.set_register(A0, BUFFER_ADDR);
        machine.set_register(A1, SIZE_ADDR);
        machine.set_register(A2, 0);
        machine.set_register(A3, 0);
        machine.set_register(A4, Source::CellDep as u64);
        machine.set_register(A5, HeaderField::Hash as u64);
        machine.set_register(A7, LOAD_HEADER_BY_FIELD_SYSCALL_NUMBER);

        let mut syscall = LoadHeader::new(tx, provider, vec![0], vec![]);
        let handled = syscall.ecall(&mut machine).expect("load header by field should succeed");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), SUCCESS as u64);
        assert_eq!(machine.memory_mut().load_bytes(BUFFER_ADDR, 32).unwrap().as_ref(), &[0x66; 32]);
    }

    #[test]
    fn test_load_header_supports_group_input_source() {
        let (tx, provider) = build_tx_and_provider();
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.memory_mut().store64(&SIZE_ADDR, &32u64).unwrap();
        machine.set_register(A0, BUFFER_ADDR);
        machine.set_register(A1, SIZE_ADDR);
        machine.set_register(A2, 0);
        machine.set_register(A3, 0);
        machine.set_register(A4, Source::GroupInput as u64);
        machine.set_register(A5, HeaderField::Hash as u64);
        machine.set_register(A7, LOAD_HEADER_BY_FIELD_SYSCALL_NUMBER);

        let mut syscall = LoadHeader::new(tx, provider, vec![0], vec![]);
        let handled = syscall.ecall(&mut machine).expect("load header by field should succeed");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), SUCCESS as u64);
        assert_eq!(machine.memory_mut().load_bytes(BUFFER_ADDR, 32).unwrap().as_ref(), &[0x55; 32]);
    }

    #[test]
    fn test_load_header_returns_item_missing_when_input_header_not_found() {
        let input_out_point = crate::celltx::OutPoint::new([0x11; 32], 0);
        let tx = Arc::new(CellTx {
            version: 0xC001,
            inputs: vec![crate::celltx::CellInput::new(input_out_point, 0)],
            cell_deps: vec![],
            header_deps: vec![],
            outputs: vec![],
            outputs_data: vec![],
            witnesses: vec![],
        });
        let mut provider = SimpleDataProvider::new();
        provider.add_cell_with_header(
            [0x11; 32],
            0,
            crate::vm::ResolvedCell {
                cell_output: crate::celltx::CellOutput {
                    capacity: 1_000,
                    lock: crate::celltx::Script::new([0x01; 32], 0, vec![]),
                    type_: None,
                },
                data: Some(vec![]),
            },
            [0xAA; 32],
        );

        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.memory_mut().store64(&SIZE_ADDR, &8u64).unwrap();
        machine.set_register(A0, BUFFER_ADDR);
        machine.set_register(A1, SIZE_ADDR);
        machine.set_register(A2, 0);
        machine.set_register(A3, 0);
        machine.set_register(A4, Source::Input as u64);
        machine.set_register(A7, LOAD_HEADER_SYSCALL_NUMBER);

        let mut syscall = LoadHeader::new(tx, Arc::new(provider), vec![0], vec![]);
        let handled = syscall.ecall(&mut machine).expect("load header syscall should be handled");
        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), ITEM_MISSING as u64);
    }

    #[test]
    fn test_load_header_group_output_reports_item_missing_when_output_exists() {
        let (tx, provider) = build_tx_and_provider();
        let tx = Arc::new(CellTx {
            outputs: vec![crate::celltx::CellOutput {
                capacity: 1_000,
                lock: crate::celltx::Script::new([0x44; 32], 0, vec![]),
                type_: None,
            }],
            outputs_data: vec![vec![]],
            ..(*tx).clone()
        });
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.memory_mut().store64(&SIZE_ADDR, &8u64).unwrap();
        machine.set_register(A0, BUFFER_ADDR);
        machine.set_register(A1, SIZE_ADDR);
        machine.set_register(A2, 0);
        machine.set_register(A3, 0);
        machine.set_register(A4, Source::GroupOutput as u64);
        machine.set_register(A7, LOAD_HEADER_SYSCALL_NUMBER);

        let mut syscall = LoadHeader::new(tx, provider, vec![0], vec![0]);
        let handled = syscall.ecall(&mut machine).expect("load header syscall should be handled");
        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), ITEM_MISSING as u64);
    }

    #[test]
    fn test_load_header_by_field_covers_all_supported_fields() {
        let (tx, provider) = build_tx_and_provider();
        let syscall = LoadHeader::new(tx, provider, vec![0], vec![]);
        let header = resolved_header([0x77; 32]);

        let expected = vec![
            (HeaderField::DaaScore as u64, header.daa_score.to_le_bytes().to_vec()),
            (HeaderField::Timestamp as u64, header.timestamp.to_le_bytes().to_vec()),
            (HeaderField::Hash as u64, header.hash.to_vec()),
            (HeaderField::Parents as u64, vec![[0xAA; 32], [0xBB; 32]].into_iter().flatten().collect::<Vec<_>>()),
            (HeaderField::Version as u64, header.version.to_le_bytes().to_vec()),
            (HeaderField::Bits as u64, header.bits.to_le_bytes().to_vec()),
            (HeaderField::Nonce as u64, header.nonce.to_le_bytes().to_vec()),
            (HeaderField::HashMerkleRoot as u64, header.hash_merkle_root.to_vec()),
            (HeaderField::AcceptedIdMerkleRoot as u64, header.accepted_id_merkle_root.to_vec()),
            (HeaderField::CellCommitment as u64, header.cell_commitment.to_vec()),
            (HeaderField::CellRoot as u64, header.cell_root.to_vec()),
            (HeaderField::SegmentRoot as u64, header.segment_root.to_vec()),
            (HeaderField::BlueScore as u64, header.blue_score.to_le_bytes().to_vec()),
            (HeaderField::BlueWork as u64, header.blue_work.to_vec()),
            (HeaderField::PruningPoint as u64, header.pruning_point.to_vec()),
        ];

        for (field, bytes) in expected {
            let actual = syscall.serialize_header_field(&header, field).expect("known field should serialize");
            assert_eq!(actual, bytes, "field {field} serialization mismatch");
        }
    }

    #[test]
    fn test_load_header_returns_item_missing_when_header_dep_not_found() {
        let missing_header_hash = [0x99; 32];
        let tx = Arc::new(CellTx {
            version: 0xC001,
            inputs: vec![],
            cell_deps: vec![],
            header_deps: vec![missing_header_hash],
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
        machine.set_register(A4, Source::HeaderDep as u64);
        machine.set_register(A7, LOAD_HEADER_SYSCALL_NUMBER);

        let mut syscall = LoadHeader::new(tx, Arc::new(SimpleDataProvider::new()), vec![], vec![]);
        let handled = syscall.ecall(&mut machine).expect("load header syscall should be handled");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), ITEM_MISSING as u64);
    }
}
