// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Load transaction hash syscall
// Reference: ckb/script/src/syscalls/load_tx.rs

use super::LOAD_TX_HASH_SYSCALL_NUMBER;
use super::utils::{store_data, SUCCESS};
use ckb_vm::{
    registers::{A0, A7},
    Error as VMError, Register, SupportMachine, Syscalls,
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
        if syscall_number != LOAD_TX_HASH_SYSCALL_NUMBER {
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
    use crate::vm::ScriptVersion;
    use ckb_vm::{
        registers::{A1, A2},
        CoreMachine, Memory, Register,
    };

    const BUFFER_ADDR: u64 = 0x1000;
    const SIZE_ADDR: u64 = 0x2000;

    #[test]
    fn test_load_tx_creation() {
        let tx_hash = [0x42u8; 32];
        let syscall = LoadTx::new(tx_hash);
        assert_eq!(syscall.tx_hash.len(), 32);
    }

    #[test]
    fn test_load_tx_supports_partial_reads() {
        let tx_hash = [0x42u8; 32];
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.memory_mut().store64(&SIZE_ADDR, &8u64).unwrap();
        machine.set_register(A0, BUFFER_ADDR);
        machine.set_register(A1, SIZE_ADDR);
        machine.set_register(A2, 24);
        machine.set_register(A7, LOAD_TX_HASH_SYSCALL_NUMBER);

        let mut syscall = LoadTx::new(tx_hash);
        let handled = syscall.ecall(&mut machine).expect("load tx syscall should succeed");

        assert!(handled);
        assert_eq!(machine.registers()[A0].to_u64(), SUCCESS as u64);
        assert_eq!(machine.memory_mut().load64(&SIZE_ADDR).unwrap().to_u64(), 8);
        assert_eq!(machine.memory_mut().load_bytes(BUFFER_ADDR, 8).unwrap().as_ref(), &[0x42; 8]);
    }

    #[test]
    fn test_load_tx_ignores_other_syscalls() {
        let mut machine = ScriptVersion::V2.init_core_machine(10_000);
        machine.set_register(A7, 9999);

        let mut syscall = LoadTx::new([0x42u8; 32]);
        let handled = syscall.ecall(&mut machine).expect("non-load-tx syscall should not fail");

        assert!(!handled);
    }
}
