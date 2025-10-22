// SPDX-License-Identifier: ISC
// Copyright (C) 2025Tondi developers
//
// System call utility functions
// Adapted from CKB script/src/syscalls/utils.rs

use ckb_vm::{Error as VMError, Memory, Register, SupportMachine, registers::{A0, A1, A2}};
use super::{SUCCESS, LENGTH_NOT_ENOUGH, SLICE_OUT_OF_BOUND};

/// Store data to VM memory
///
/// Returns the number of bytes written
pub fn store_data<Mac: SupportMachine>(
    machine: &mut Mac,
    data: &[u8],
) -> Result<usize, VMError> {
    let addr = machine.registers()[A0].to_u64();
    let size_addr = machine.registers()[A1].to_u64();
    let offset = machine.registers()[A2].to_u64() as usize;

    // Read the buffer size from memory
    let size = machine.memory_mut().load64(&Mac::REG::from_u64(size_addr))?.to_u64() as usize;

    // Calculate slice bounds
    let data_len = data.len();
    let offset = offset.min(data_len);
    let full_size = data_len - offset;
    let real_size = size.min(full_size);

    // Write data to memory
    machine
        .memory_mut()
        .store_bytes(addr, &data[offset..offset + real_size])?;

    // Write actual size back
    machine
        .memory_mut()
        .store64(&Mac::REG::from_u64(size_addr), &Mac::REG::from_u64(full_size as u64))?;

    Ok(real_size)
}

/// Store a u64 value to VM memory
pub fn store_u64<Mac: SupportMachine>(
    machine: &mut Mac,
    value: u64,
) -> Result<(), VMError> {
    let addr = machine.registers()[A0].to_u64();
    machine
        .memory_mut()
        .store64(&Mac::REG::from_u64(addr), &Mac::REG::from_u64(value))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require a VM machine instance
    // Full tests will be in integration tests
}

