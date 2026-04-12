// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Syscall utility functions
// Reference: ckb/script/src/syscalls/utils.rs

use ckb_vm::{
    registers::{A0, A1, A2},
    Error as VMError, Memory, Register, SupportMachine,
};
use std::cmp;

/// Success return code
pub const SUCCESS: u8 = 0;
/// Index out of bound
pub const INDEX_OUT_OF_BOUND: u8 = 1;
/// Item missing
pub const ITEM_MISSING: u8 = 2;
/// Length not enough (buffer too small)
pub const LENGTH_NOT_ENOUGH: u8 = 3;

/// Store data to VM memory (CKB-compatible implementation)
///
/// This follows CKB's exact pattern for maximum compatibility
pub fn store_data<Mac: SupportMachine>(machine: &mut Mac, data: &[u8]) -> Result<u64, VMError> {
    let addr = machine.registers()[A0].to_u64();
    let size_addr = machine.registers()[A1].clone();
    let data_len = data.len() as u64;
    let offset = cmp::min(data_len, machine.registers()[A2].to_u64());

    let size = machine.memory_mut().load64(&size_addr)?.to_u64();
    let full_size = data_len - offset;
    let real_size = cmp::min(size, full_size);
    machine.memory_mut().store64(&size_addr, &Mac::REG::from_u64(full_size))?;
    machine.memory_mut().store_bytes(addr, &data[offset as usize..(offset + real_size) as usize])?;
    Ok(real_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_return_codes() {
        assert_eq!(SUCCESS, 0);
        assert_eq!(INDEX_OUT_OF_BOUND, 1);
        assert_eq!(ITEM_MISSING, 2);
        assert_eq!(LENGTH_NOT_ENOUGH, 3);
    }
}
