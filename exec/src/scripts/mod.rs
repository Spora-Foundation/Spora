// SPDX-License-Identifier: ISC
// Copyright (C) 2025 Spora developers
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
/// Returns: exit code 0 (always succeeds)
pub const ALWAYS_SUCCESS_SCRIPT: &[u8] = &[
    // RISC-V: addi a0, zero, 0; ret
    0x13, 0x05, 0x00, 0x00,  // addi a0, zero, 0
    0x67, 0x80, 0x00, 0x00,  // ret
];

/// Always-success lock script code hash
pub fn always_success_code_hash() -> [u8; 32] {
    blake3::hash(ALWAYS_SUCCESS_SCRIPT).into()
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
        assert_eq!(ALWAYS_SUCCESS_SCRIPT.len(), 8);
    }
}
