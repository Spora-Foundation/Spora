// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Standard scripts for Spora

//! Standard lock and type scripts
//!
//! This module contains:
//! - secp256k1 lock script (RISC-V binary)
//! - Always-success lock (for testing)
//! - Capacity type script

/// Always-success lock script (for testing)
///
/// This is a real RISC-V ELF fixture that exits with code 0.
pub const ALWAYS_SUCCESS_SCRIPT: &[u8] = include_bytes!("fixtures/always_success.elf");

/// Always-success lock script code hash
pub fn always_success_code_hash() -> [u8; 32] {
    blake3::hash(ALWAYS_SUCCESS_SCRIPT).into()
}

/// Load-input-since lock script (for testing)
///
/// This ELF fixture exercises `LOAD_INPUT_BY_FIELD` and exits with code 0 only
/// when the first input's `since` matches the baked-in constant.
pub const LOAD_INPUT_SINCE_SCRIPT: &[u8] = include_bytes!("fixtures/load_input_since.elf");

/// Load-input-since lock script code hash
pub fn load_input_since_code_hash() -> [u8; 32] {
    blake3::hash(LOAD_INPUT_SINCE_SCRIPT).into()
}

/// Load-header-timestamp lock script (for testing)
///
/// This ELF fixture exercises `LOAD_HEADER_BY_FIELD` over the first header dep
/// and exits with code 0 only when the timestamp matches the baked-in constant.
pub const LOAD_HEADER_TIMESTAMP_SCRIPT: &[u8] = include_bytes!("fixtures/load_header_timestamp.elf");

/// Load-header-timestamp lock script code hash
pub fn load_header_timestamp_code_hash() -> [u8; 32] {
    blake3::hash(LOAD_HEADER_TIMESTAMP_SCRIPT).into()
}

/// Load-dep-cell-data lock script (for testing)
///
/// This ELF fixture exercises `LOAD_CELL_DATA` against the first cell dep and
/// exits with code 0 only when the returned bytes match the baked-in constant.
pub const LOAD_DEP_CELL_DATA_SCRIPT: &[u8] = include_bytes!("fixtures/load_dep_cell_data.elf");

/// Load-dep-cell-data lock script code hash
pub fn load_dep_cell_data_code_hash() -> [u8; 32] {
    blake3::hash(LOAD_DEP_CELL_DATA_SCRIPT).into()
}

/// Secp256k1 + Blake3 lock script (placeholder)
///
/// Note: This should be compiled from secp256k1_blake3_lock.c
/// For now, we provide the source code and compilation instructions
pub const SECP256K1_BLAKE3_LOCK_SOURCE: &str = include_str!("secp256k1_blake3_lock.c");

/// Build instructions for secp256k1 lock
pub const BUILD_INSTRUCTIONS: &str = r#"
# Build secp256k1_blake3_lock.c to RISC-V binary

## Prerequisites
- RISC-V GNU toolchain (riscv64-unknown-elf-gcc)
- Install: https://github.com/riscv-collab/riscv-gnu-toolchain

## Build Command
riscv64-unknown-elf-gcc -O3 -nostdlib -nostartfiles \
    -fno-builtin-printf -fno-builtin-memcmp \
    -Wl,-Ttext=0x0 \
    -o secp256k1_blake3_lock.elf \
    secp256k1_blake3_lock.c

riscv64-unknown-elf-objcopy -O binary \
    secp256k1_blake3_lock.elf \
    secp256k1_blake3_lock.bin

## Verify
hexdump -C secp256k1_blake3_lock.bin

## Get Code Hash
blake3sum secp256k1_blake3_lock.bin
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_always_success_code_hash() {
        let hash = always_success_code_hash();
        assert_eq!(hash.len(), 32);

        // Verify it's deterministic
        let hash2 = always_success_code_hash();
        assert_eq!(hash, hash2);
    }

    #[test]
    fn test_always_success_script_size() {
        assert!(ALWAYS_SUCCESS_SCRIPT.len() > 64);
        assert_eq!(&ALWAYS_SUCCESS_SCRIPT[..4], b"\x7fELF");
    }

    #[test]
    fn test_load_input_since_script_size() {
        assert!(LOAD_INPUT_SINCE_SCRIPT.len() > 64);
        assert_eq!(&LOAD_INPUT_SINCE_SCRIPT[..4], b"\x7fELF");
    }

    #[test]
    fn test_load_header_timestamp_script_size() {
        assert!(LOAD_HEADER_TIMESTAMP_SCRIPT.len() > 64);
        assert_eq!(&LOAD_HEADER_TIMESTAMP_SCRIPT[..4], b"\x7fELF");
    }

    #[test]
    fn test_load_dep_cell_data_script_size() {
        assert!(LOAD_DEP_CELL_DATA_SCRIPT.len() > 64);
        assert_eq!(&LOAD_DEP_CELL_DATA_SCRIPT[..4], b"\x7fELF");
    }
}

#[cfg(all(test, feature = "vm"))]
mod always_success_test;

#[cfg(all(test, feature = "vm"))]
mod load_input_since_test;

#[cfg(all(test, feature = "vm"))]
mod load_header_timestamp_test;

#[cfg(all(test, feature = "vm"))]
mod load_dep_cell_data_test;
