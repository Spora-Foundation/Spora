# Spora Standard Scripts

This directory contains standard lock and type scripts for Spora.

## Lock Scripts

### 1. Always Success (Testing Only)

**Code**: 8 bytes RISC-V
```riscv
addi a0, zero, 0  # Set return value to 0
ret               # Return
```

**Usage**:
```rust
use spora_exec::scripts::{ALWAYS_SUCCESS_SCRIPT, always_success_code_hash};

let lock = ScriptRef {
    code_hash: always_success_code_hash(),
    hash_type: 0,  // Data hash type
    args: vec![],
};
```

### 2. Secp256k1 + Blake3 Lock

**File**: `secp256k1_blake3_lock.c`

**Functionality**:
- Verifies secp256k1 signatures
- Uses **blake3** for hashing (Spora-specific!)
- Args: pubkey hash (20 bytes, blake3 of pubkey)
- Witness: signature (65 bytes, r + s + v)

**Build**:
```bash
# Install RISC-V toolchain
# https://github.com/riscv-collab/riscv-gnu-toolchain

# Compile
riscv64-unknown-elf-gcc -O3 -nostdlib -nostartfiles \
    -fno-builtin-printf -fno-builtin-memcmp \
    -Wl,-Ttext=0x0 \
    -o secp256k1_blake3_lock.elf \
    secp256k1_blake3_lock.c

# Extract binary
riscv64-unknown-elf-objcopy -O binary \
    secp256k1_blake3_lock.elf \
    secp256k1_blake3_lock.bin

# Get code hash (for use in transactions)
blake3sum secp256k1_blake3_lock.bin
```

**Usage**:
```rust
// In transaction
let pubkey = /* secp256k1 public key (33 bytes compressed) */;
let pubkey_hash = &blake3::hash(&pubkey).as_bytes()[0..20];

let lock = ScriptRef {
    code_hash: blake3::hash(&secp256k1_lock_binary).into(),
    hash_type: 0,
    args: pubkey_hash.to_vec(),
};

let output = CellOut {
    capacity: 10000,
    lock,
    type_: None,
};
```

## Key Differences from CKB

| Feature | CKB | Spora |
|---------|-----|-------|
| Sighash | blake2b | **blake3** |
| VM syscalls | 9 standard | 9 standard + **blake3_hash** |
| Binary format | Same RISC-V | Same RISC-V ✅ |

**Important**: CKB scripts need to be **recompiled** for Spora because:
1. Sighash uses blake3 (not blake2b)
2. Tx hash uses blake3
3. Script hash uses blake3

But the **logic** can be reused!

## Type Scripts

### 1. Capacity Type (Future)

Ensures capacity conservation:
```
sum(inputs.capacity) == sum(outputs.capacity)
```

### 2. UDT (User Defined Token)

Standard token contract (CKB-compatible logic).

---

## Development Guide

### Testing Scripts

Use the always-success script for initial testing:

```rust
#[test]
fn test_always_success() {
    use spora_exec::vm::{TransactionScriptVerifier, SimpleDataProvider};
    
    // Create provider with always-success script
    let mut provider = SimpleDataProvider::new();
    provider.add_script(
        always_success_code_hash(),
        ALWAYS_SUCCESS_SCRIPT.to_vec(),
    );
    
    // Create transaction with always-success lock
    let tx = CellTx {
        outputs: vec![
            CellOut {
                capacity: 1000,
                lock: ScriptRef {
                    code_hash: always_success_code_hash(),
                    hash_type: 0,
                    args: vec![],
                },
                type_: None,
            }
        ],
        // ...
    };
    
    // Verify
    let verifier = TransactionScriptVerifier::new(
        Arc::new(tx),
        Arc::new(provider),
    );
    
    assert!(verifier.verify().is_ok());
}
```

### Building Custom Scripts

1. Write script in C (using Spora syscalls)
2. Compile to RISC-V binary
3. Compute blake3 code hash
4. Deploy as cell data in genesis or via transaction
5. Reference in lock/type scripts

---

**Last Updated**: 2025-10-22  
**See Also**: `../vm/syscalls/` for syscall implementations

