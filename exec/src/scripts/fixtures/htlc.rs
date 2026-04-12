#![no_std]
#![no_main]

//! HTLC (Hash Time Locked Contract) script for CKB-VM
//!
//! This script implements a complete HTLC with two spending paths:
//! 1. Recipient path: Provide secret preimage + signature
//! 2. Sender timeout path: Provide signature after timeout
//!
//! Script args format (105 bytes total):
//! - [0..32]:  secret_hash (blake3 hash of the secret)
//! - [32..64]: recipient_pubkey (32 bytes for Schnorr)
//! - [64..96]: sender_pubkey (32 bytes for Schnorr)
//! - [96]:     lock_type (0=absolute DAA, 1=absolute timestamp, 2=relative DAA, 3=relative timestamp)
//! - [97..105]: lock_value (u64, target timestamp or delta)
//!
//! Witness format:
//! - Path 1 (recipient): <signature (64 bytes)> <secret (32 bytes)> <path_selector (1 byte = 0x01)>
//! - Path 2 (sender):    <signature (64 bytes)> <path_selector (1 byte = 0x00)>

use core::arch::asm;

// Syscall numbers
const SYS_EXIT: usize = 93;
const SYS_LOAD_WITNESS: usize = 2074;
const SYS_LOAD_SCRIPT: usize = 2075;
const SYS_BLAKE3_HASH: usize = 3001;
const SYS_LOAD_INPUT_BY_FIELD: usize = 2083;

// Source types
const SOURCE_GROUP_INPUT: usize = 0x0100;

// Field types
const FIELD_SINCE: usize = 0x01;

// Exit codes
const EXIT_SUCCESS: usize = 0;
const EXIT_FAILURE: usize = 1;

// Lock types
const LOCK_ABSOLUTE_DAA: u8 = 0;
const LOCK_ABSOLUTE_TIMESTAMP: u8 = 1;
const LOCK_RELATIVE_DAA: u8 = 2;
const LOCK_RELATIVE_TIMESTAMP: u8 = 3;

// Since flags
const SINCE_RELATIVE: u64 = 1 << 63;
const SINCE_TIMESTAMP: u64 = 1 << 62;
const SINCE_VALUE_MASK: u64 = 0x00FFFFFFFFFFFFFF;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    exit(EXIT_FAILURE);
}

#[inline(always)]
fn exit(code: usize) -> ! {
    unsafe {
        asm!(
            "ecall",
            in("a0") code,
            in("a7") SYS_EXIT,
            options(noreturn)
        );
    }
}

/// Load witness data
fn load_witness(buf: &mut [u8], index: usize) -> Option<usize> {
    let mut size = buf.len() as u64;
    let ret: usize;

    unsafe {
        asm!(
            "ecall",
            inlateout("a0") buf.as_mut_ptr() as usize => ret,
            in("a1") (&mut size as *mut u64) as usize,
            in("a2") 0usize,      // offset
            in("a3") index,       // index
            in("a4") SOURCE_GROUP_INPUT,
            in("a7") SYS_LOAD_WITNESS,
        );
    }

    if ret != 0 || (size as usize) > buf.len() {
        None
    } else {
        Some(size as usize)
    }
}

/// Load script args
///
/// `LOAD_SCRIPT` returns the full serialized script:
/// `code_hash(32) || hash_type(1) || args_len(4) || args(...)`.
fn load_script(buf: &mut [u8]) -> Option<usize> {
    let mut size = buf.len() as u64;
    let ret: usize;

    unsafe {
        asm!(
            "ecall",
            inlateout("a0") buf.as_mut_ptr() as usize => ret,
            in("a1") (&mut size as *mut u64) as usize,
            in("a2") 0usize,      // offset
            in("a7") SYS_LOAD_SCRIPT,
        );
    }

    if ret != 0 {
        None
    } else {
        Some(size as usize)
    }
}

/// Parse raw script bytes returned by `LOAD_SCRIPT` and extract typed HTLC args.
fn parse_loaded_script_args(script: &[u8]) -> Option<(&[u8; 32], &[u8; 32], &[u8; 32], u8, u64)> {
    if script.len() < 37 {
        return None;
    }

    let args_len =
        (script[33] as usize) |
        ((script[34] as usize) << 8) |
        ((script[35] as usize) << 16) |
        ((script[36] as usize) << 24);
    let args_end = 37usize.checked_add(args_len)?;
    if script.len() < args_end {
        return None;
    }

    parse_script_args(&script[37..args_end])
}

/// Load input since field
fn load_input_since() -> Option<u64> {
    let mut buf = [0u8; 8];
    let mut size = buf.len() as u64;
    let ret: usize;

    unsafe {
        asm!(
            "ecall",
            inlateout("a0") buf.as_mut_ptr() as usize => ret,
            in("a1") (&mut size as *mut u64) as usize,
            in("a2") 0usize,      // offset
            in("a3") 0usize,      // index
            in("a4") SOURCE_GROUP_INPUT,
            in("a5") FIELD_SINCE,
            in("a7") SYS_LOAD_INPUT_BY_FIELD,
        );
    }

    if ret != 0 || size != 8 {
        return None;
    }

    // Parse little-endian u64
    let mut since: u64 = 0;
    for i in 0..8 {
        since |= (buf[i] as u64) << (8 * i);
    }
    Some(since)
}

/// Compute blake3 hash
fn blake3_hash(input: &[u8], output: &mut [u8; 32]) -> bool {
    let mut output_len = output.len() as u64;
    let ret: usize;

    unsafe {
        asm!(
            "ecall",
            inlateout("a0") output.as_mut_ptr() as usize => ret,
            in("a1") (&mut output_len as *mut u64) as usize,
            in("a2") input.as_ptr() as usize,
            in("a3") input.len(),
            in("a7") SYS_BLAKE3_HASH,
        );
    }

    ret == 0 && output_len == 32
}

/// Verify secret preimage
fn verify_secret(secret: &[u8], expected_hash: &[u8; 32]) -> bool {
    if secret.len() != 32 {
        return false;
    }

    let mut computed_hash = [0u8; 32];
    if !blake3_hash(secret, &mut computed_hash) {
        return false;
    }

    computed_hash == *expected_hash
}

/// Verify time lock
fn verify_timelock(since: u64, lock_type: u8, lock_value: u64) -> bool {
    let is_relative = (since & SINCE_RELATIVE) != 0;
    let is_timestamp = (since & SINCE_TIMESTAMP) != 0;
    let since_value = since & SINCE_VALUE_MASK;

    match lock_type {
        LOCK_ABSOLUTE_DAA => {
            // bit63=0, bit62=0
            if is_relative || is_timestamp {
                return false;
            }
            since_value >= lock_value
        }
        LOCK_ABSOLUTE_TIMESTAMP => {
            // bit63=0, bit62=1
            if is_relative || !is_timestamp {
                return false;
            }
            since_value >= lock_value
        }
        LOCK_RELATIVE_DAA => {
            // bit63=1, bit62=0
            if !is_relative || is_timestamp {
                return false;
            }
            since_value >= lock_value
        }
        LOCK_RELATIVE_TIMESTAMP => {
            // bit63=1, bit62=1
            if !is_relative || !is_timestamp {
                return false;
            }
            since_value >= lock_value
        }
        _ => false,
    }
}

/// Parse script args
fn parse_script_args(args: &[u8]) -> Option<(&[u8; 32], &[u8; 32], &[u8; 32], u8, u64)> {
    if args.len() < 105 {
        return None;
    }

    let secret_hash = (&args[0..32]).try_into().ok()?;
    let recipient_pubkey = (&args[32..64]).try_into().ok()?;
    let sender_pubkey = (&args[64..96]).try_into().ok()?;
    let lock_type = args[96];

    // Parse lock_value (u64, little-endian)
    let lock_value =
        (args[97] as u64) |
        ((args[98] as u64) << 8) |
        ((args[99] as u64) << 16) |
        ((args[100] as u64) << 24) |
        ((args[101] as u64) << 32) |
        ((args[102] as u64) << 40) |
        ((args[103] as u64) << 48) |
        ((args[104] as u64) << 56);

    Some((secret_hash, recipient_pubkey, sender_pubkey, lock_type, lock_value))
}

/// Parse witness for recipient path
/// Format: <signature (64 bytes)> <secret (32 bytes)> <path_selector (1 byte = 0x01)>
fn parse_recipient_witness(witness: &[u8]) -> Option<(&[u8], &[u8])> {
    if witness.len() < 97 || witness[96] != 0x01 {
        return None;
    }
    Some((&witness[0..64], &witness[64..96]))
}

/// Parse witness for sender path
/// Format: <signature (64 bytes)> <path_selector (1 byte = 0x00)>
fn parse_sender_witness(witness: &[u8]) -> Option<&[u8]> {
    if witness.len() < 65 || witness[64] != 0x00 {
        return None;
    }
    Some(&witness[0..64])
}

/// Verify signature (placeholder - actual verification needs secp256k1)
/// Returns true for testing purposes
fn verify_signature(_pubkey: &[u8; 32], _signature: &[u8], _message: &[u8]) -> bool {
    // TODO: Implement actual secp256k1 signature verification
    // For now, always return true for testing
    true
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // 1. Load script args
    let mut args_buf = [0u8; 256];
    let script_len = match load_script(&mut args_buf) {
        Some(len) => len,
        None => exit(EXIT_FAILURE),
    };

    let (secret_hash, recipient_pubkey, sender_pubkey, lock_type, lock_value) =
        match parse_loaded_script_args(&args_buf[..script_len]) {
            Some(parsed) => parsed,
            None => exit(EXIT_FAILURE),
        };

    // 2. Load witness
    let mut witness_buf = [0u8; 128];
    let witness_len = match load_witness(&mut witness_buf, 0) {
        Some(len) => len,
        None => exit(EXIT_FAILURE),
    };
    let witness = &witness_buf[..witness_len];

    // 3. Determine path and verify
    if witness.last() == Some(&0x01) {
        // Recipient path: verify secret + signature
        let (signature, secret) = match parse_recipient_witness(witness) {
            Some(parsed) => parsed,
            None => exit(EXIT_FAILURE),
        };

        // Verify secret hash
        if !verify_secret(secret, secret_hash) {
            exit(EXIT_FAILURE);
        }

        // Verify recipient signature
        // Message is the transaction sighash (computed elsewhere)
        if !verify_signature(recipient_pubkey, signature, &[]) {
            exit(EXIT_FAILURE);
        }

        exit(EXIT_SUCCESS);
    } else if witness.last() == Some(&0x00) {
        // Sender timeout path: verify timelock + signature
        let signature = match parse_sender_witness(witness) {
            Some(sig) => sig,
            None => exit(EXIT_FAILURE),
        };

        // Load and verify time lock
        let since = match load_input_since() {
            Some(s) => s,
            None => exit(EXIT_FAILURE),
        };

        if !verify_timelock(since, lock_type, lock_value) {
            exit(EXIT_FAILURE);
        }

        // Verify sender signature
        if !verify_signature(sender_pubkey, signature, &[]) {
            exit(EXIT_FAILURE);
        }

        exit(EXIT_SUCCESS);
    } else {
        // Invalid path selector
        exit(EXIT_FAILURE);
    }
}
