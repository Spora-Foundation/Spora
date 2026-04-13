// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Secp256k1 + Blake3 Lock Script
// Compile to RISC-V binary for use in CKB-VM
//
// Build:
//   riscv64-unknown-elf-gcc -O3 -nostdlib -nostartfiles \
//     -fno-builtin-printf -fno-builtin-memcmp \
//     -o secp256k1_blake3_lock secp256k1_blake3_lock.c

#include <stdint.h>
#include <stddef.h>

// ============================================================================
// Spora Syscall Definitions
// ============================================================================

#define LOAD_TX_HASH_SYSCALL     2061
#define LOAD_SCRIPT_HASH_SYSCALL 2062
#define LOAD_CELL_SYSCALL        2071
#define LOAD_INPUT_SYSCALL       2073
#define LOAD_WITNESS_SYSCALL     2074
#define LOAD_SCRIPT_SYSCALL      2075
#define BLAKE3_HASH_SYSCALL      3001  // ← Spora extension!

#define SUCCESS              0
#define INDEX_OUT_OF_BOUND   1
#define ITEM_MISSING         2
#define LENGTH_NOT_ENOUGH    3

// Source types
#define SOURCE_INPUT         0x01
#define SOURCE_OUTPUT        0x02
#define SOURCE_GROUP_INPUT   0x0100
#define SOURCE_GROUP_OUTPUT  0x0200

// ============================================================================
// Syscall Wrappers
// ============================================================================

static inline int syscall(
    uint64_t n,
    uint64_t a0,
    uint64_t a1,
    uint64_t a2,
    uint64_t a3,
    uint64_t a4,
    uint64_t a5
) {
    register uint64_t _a0 asm("a0") = a0;
    register uint64_t _a1 asm("a1") = a1;
    register uint64_t _a2 asm("a2") = a2;
    register uint64_t _a3 asm("a3") = a3;
    register uint64_t _a4 asm("a4") = a4;
    register uint64_t _a5 asm("a5") = a5;
    register uint64_t _a7 asm("a7") = n;
    
    asm volatile (
        "ecall"
        : "+r"(_a0)
        : "r"(_a1), "r"(_a2), "r"(_a3), "r"(_a4), "r"(_a5), "r"(_a7)
        : "memory"
    );
    
    return _a0;
}

// Load witness data
static inline int load_witness(
    uint8_t* buf,
    uint64_t* len,
    size_t offset,
    size_t index,
    size_t source
) {
    return syscall(LOAD_WITNESS_SYSCALL, (uint64_t)buf, (uint64_t)len, offset, index, source, 0);
}

// Load script args
static inline int load_script(
    uint8_t* buf,
    uint64_t* len,
    size_t offset
) {
    return syscall(LOAD_SCRIPT_SYSCALL, (uint64_t)buf, (uint64_t)len, offset, 0, 0, 0);
}

// Blake3 hash (Spora-specific!)
static inline int blake3_hash(
    uint8_t* output,
    const uint8_t* input,
    size_t input_len
) {
    uint64_t output_len = 32;
    return syscall(BLAKE3_HASH_SYSCALL, (uint64_t)output, (uint64_t)&output_len, (uint64_t)input, input_len, 0, 0);
}

// ============================================================================
// Secp256k1 Signature Verification (simplified)
// ============================================================================

// Note: Full secp256k1 implementation is ~3000 lines.
// Fail closed until a real implementation is wired in.
int verify_secp256k1_signature(
    const uint8_t* pubkey_hash,   // 20 bytes (blake3(pubkey)[0..20])
    const uint8_t* signature,     // 65 bytes (r + s + v)
    const uint8_t* message_hash   // 32 bytes
) {
    (void)pubkey_hash;
    (void)signature;
    (void)message_hash;

    // TODO: Implement full secp256k1 recovery and verification.
    // Until then, reject instead of silently accepting any witness.
    return 1;
}

// ============================================================================
// Lock Script Main Logic
// ============================================================================

int main() {
    int ret;
    
    // 1. Load script args (should contain pubkey hash, 20 bytes)
    uint8_t script_args[256];
    uint64_t script_args_len = 256;
    ret = load_script(script_args, &script_args_len, 0);
    if (ret != SUCCESS) {
        return 1;  // Failed to load script
    }
    
    // Args format: [pubkey_hash(20 bytes)]
    if (script_args_len < 20) {
        return 1;  // Invalid args
    }
    
    // Skip code_hash (32 bytes) + hash_type (1 byte) + args_len (4 bytes)
    uint8_t* pubkey_hash = script_args + 37;
    
    // 2. Load witness (should contain signature, 65 bytes)
    uint8_t witness[256];
    uint64_t witness_len = 256;
    ret = load_witness(witness, &witness_len, 0, 0, SOURCE_GROUP_INPUT);
    if (ret != SUCCESS) {
        return 1;  // No witness
    }
    
    if (witness_len < 65) {
        return 1;  // Invalid signature length
    }
    
    uint8_t* signature = witness;
    
    // 3. Compute message hash (sighash)
    // In real implementation, need to:
    // - Load tx hash
    // - Load inputs/outputs
    // - Compute sighash (blake3-based, not blake2b!)
    uint8_t sighash[32];
    // blake3_hash(sighash, sighash_preimage, preimage_len);
    
    // For demo, just use zero hash
    for (int i = 0; i < 32; i++) {
        sighash[i] = 0;
    }
    
    // 4. Verify signature
    ret = verify_secp256k1_signature(pubkey_hash, signature, sighash);
    if (ret != 0) {
        return 1;  // Signature verification failed
    }
    
    // Success!
    return 0;
}
