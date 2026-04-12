// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Relative time lock script for CKB-VM
//
// This script verifies that the input's `since` field indicates a relative
// lock of at least `target_delta` blocks from the input's confirmation.
//
// Script args: [target_delta: u64 (8 bytes, little-endian)]
//
// The `since` field encoding:
// - Bit 63: 1 (relative lock)
// - Bit 62: 0 (DAA score, not timestamp)
// - Bits 0-55: delta value
//
// Build:
//   riscv64-unknown-elf-gcc -O3 -nostdlib -nostartfiles \
//     -fno-builtin-printf -fno-builtin-memcmp \
//     -o timelock_relative.elf \
//     timelock_relative.c

#include <stdint.h>
#include <stddef.h>

// ============================================================================
// Spora Syscall Definitions
// ============================================================================

#define LOAD_INPUT_BY_FIELD_SYSCALL 2074

#define SUCCESS              0

// Source types
#define SOURCE_GROUP_INPUT   0x0100

// Field types for LOAD_INPUT_BY_FIELD
#define FIELD_SINCE          0x00

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

// Load input field by index
static inline int load_input_by_field(
    uint8_t* buf,
    uint64_t* len,
    size_t offset,
    size_t index,
    size_t source,
    size_t field
) {
    return syscall(LOAD_INPUT_BY_FIELD_SYSCALL, (uint64_t)buf, (uint64_t)len, offset, index, source, field);
}

// ============================================================================
// Time Lock Script Main Logic
// ============================================================================

// Placeholder: In real implementation, read from script args using LOAD_SCRIPT
static uint64_t get_target_delta() {
    // TODO: Use LOAD_SCRIPT syscall to read script args
    // The args would be: [target_delta: u64 (8 bytes)]
    return 0;
}

int main() {
    int ret;
    
    // 1. Load the `since` field from the first input in the group
    uint8_t since_buf[8];
    uint64_t since_len = 8;
    
    ret = load_input_by_field(
        since_buf,
        &since_len,
        0,              // offset
        0,              // index
        SOURCE_GROUP_INPUT,  // source: current input group
        FIELD_SINCE     // field: since
    );
    
    if (ret != SUCCESS) {
        return 1;  // Failed to load since field
    }
    
    if (since_len != 8) {
        return 1;  // Invalid since length
    }
    
    // 2. Parse the since value (little-endian)
    uint64_t since = 0;
    for (int i = 0; i < 8; i++) {
        since |= ((uint64_t)since_buf[i]) << (8 * i);
    }
    
    // 3. Check that this is a relative DAA score lock
    // Bit 63 must be 1 (relative)
    // Bit 62 must be 0 (DAA score, not timestamp)
    if ((since & 0x8000000000000000) == 0) {
        return 1;  // Not a relative lock
    }
    if ((since & 0x4000000000000000) != 0) {
        return 1;  // Timestamp lock not allowed for DAA-based relative lock
    }
    
    // 4. Extract the delta value (bits 0-55)
    uint64_t delta = since & 0x00FFFFFFFFFFFFFF;
    
    // 5. Get target delta from script args
    uint64_t target_delta = get_target_delta();
    
    // 6. Verify: delta >= target_delta
    if (delta < target_delta) {
        return 1;  // Relative lock not satisfied
    }
    
    // Success!
    return 0;
}
