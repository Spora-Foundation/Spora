// SPDX-License-Identifier: ISC
// Copyright (C) 2025Tondi developers
//
// Load transaction syscall
// Adapted from CKB script/src/syscalls/load_tx.rs
// ⚠️ Modified: Blake2b → Blake3

use super::utils::store_data;
use super::{LOAD_TX_HASH_SYSCALL_NUMBER, SUCCESS};
use crate::celltx::sighash::compute_wtxid;
use crate::celltx::types::CellTx;
use crate::vm::cost_model::transferred_byte_cycles;
use ckb_vm::{
    Error as VMError, Register, SupportMachine, Syscalls,
    registers::{A0, A7},
};
use std::sync::Arc;

/// Load transaction hash syscall
#[derive(Debug)]
pub struct LoadTx {
    tx: Arc<CellTx>,
}

impl LoadTx {
    /// Create a new LoadTx syscall
    pub fn new(tx: Arc<CellTx>) -> Self {
        Self { tx }
    }
}

impl<Mac: SupportMachine> Syscalls<Mac> for LoadTx {
    fn initialize(&mut self, _machine: &mut Mac) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut Mac) -> Result<bool, VMError> {
        let syscall_number = machine.registers()[A7].to_u64();
        
        if syscall_number != LOAD_TX_HASH_SYSCALL_NUMBER {
            return Ok(false);
        }

        // Compute wtxid using Blake3 (not Blake2b like CKB)
        let wtxid = compute_wtxid(&self.tx);
        
        // Store wtxid to VM memory
        let wrote_size = store_data(machine, &wtxid)?;

        // Add cycles cost
        machine.add_cycles_no_checking(transferred_byte_cycles(wrote_size))?;
        machine.set_register(A0, Mac::REG::from_u8(SUCCESS));
        
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::celltx::types::{CellRef, CellOut, ScriptRef, OutPoint};

    fn create_test_tx() -> CellTx {
        let lock = ScriptRef::new([0x00; 32], 0, vec![0; 20]);
        CellTx::new(
            vec![CellRef::new(OutPoint::new([0; 32], 0), 0)],
            vec![],
            vec![CellOut { lock, type_: None, capacity: 10000 }],
            vec![vec![]],
            vec![],
        ).unwrap()
    }

    #[test]
    fn test_load_tx_creation() {
        let tx = Arc::new(create_test_tx());
        let syscall = LoadTx::new(tx);
        assert!(syscall.tx.ver == 0xC001);
    }
}

