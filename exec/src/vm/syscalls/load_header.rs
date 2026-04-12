// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Load header syscall (DAG-aware)

use super::utils::{store_data, INDEX_OUT_OF_BOUND, ITEM_MISSING, SUCCESS};
use crate::celltx::CellTx;
use crate::vm::{CellDataProvider, ResolvedHeader};
use ckb_vm::{
    registers::{A0, A2, A3, A4, A5, A7},
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
        match source {
            0x04 => {
                let hash = self.tx.header_deps.get(index)?;
                self.provider.load_header(hash)
            }
            _ => None,
        }
    }

    fn serialize_header_field(&self, header: &ResolvedHeader, field: u64) -> Option<Vec<u8>> {
        match field {
            0 => Some(header.daa_score.to_le_bytes().to_vec()),
            1 => Some(header.timestamp.to_le_bytes().to_vec()),
            2 => Some(header.hash.to_vec()),
            3 => Some(header.parents.iter().flatten().copied().collect()),
            _ => None,
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
        if syscall_number != 2072 && syscall_number != 2082 {
            return Ok(false);
        }

        let offset = machine.registers()[A2].to_u64();
        let index = machine.registers()[A3].to_u64() as usize;
        let source = machine.registers()[A4].to_u64();

        let header = match self.get_header(source, index) {
            Some(header) => header,
            None => {
                machine.set_register(A0, M::REG::from_u8(INDEX_OUT_OF_BOUND));
                return Ok(true);
            }
        };

        let data = if syscall_number == 2082 {
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

        if offset != 0 && (offset as usize) > data.len() {
            machine.set_register(A0, M::REG::from_u8(INDEX_OUT_OF_BOUND));
            return Ok(true);
        }

        store_data(machine, &data)?;
        machine.set_register(A0, M::REG::from_u8(SUCCESS));

        Ok(true)
    }
}
