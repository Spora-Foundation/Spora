// SPDX-License-Identifier: ISC
// Copyright (C) 2025 Spora developers
//
// Load transaction hash syscall
// Reference: ckb/script/src/syscalls/load_tx.rs

use super::utils::{store_data, SUCCESS, INDEX_OUT_OF_BOUND};
use ckb_vm::{
    Register, Syscalls, SupportMachine,
    Error as VMError,
    registers::{A0, A7},
};

/// Syscall: Load Transaction Hash
///
/// Syscall number: 2061
///
/// Returns the transaction hash (32 bytes)
pub struct LoadTx {
    tx_hash: [u8; 32],
}

impl LoadTx {
    pub fn new(tx_hash: [u8; 32]) -> Self {
        Self { tx_hash }
    }
}

impl<M: SupportMachine> Syscalls<M> for LoadTx {
    fn initialize(&mut self, _machine: &mut M) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut M) -> Result<bool, VMError> {
        let syscall_number = machine.registers()[A7].to_u64();
        
        // LOAD_TX_HASH = 2061
        if syscall_number != 2061 {
            return Ok(false);
        }

        // Store tx hash using CKB-style store_data
        // It reads A0, A1, A2 from registers internally
        store_data(machine, &self.tx_hash)?;
        machine.set_register(A0, M::REG::from_u8(SUCCESS));
        
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::celltx::{CellTx, CellRef, CellOut, ScriptRef, OutPoint};
    
    #[test]
    fn test_load_tx_creation() {
        let tx_hash = [0x42u8; 32];
        let syscall = LoadTx::new(tx_hash);
        assert_eq!(syscall.tx_hash.len(), 32);
    }
}
