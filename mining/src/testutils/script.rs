#![cfg(test)]

use spora_consensus_core::tx::{builtin_schnorr_blake3_160_code_hash, Script, HASH_TYPE_TYPE};

/// Creates a standard test lock script for mempool and template tests.
/// This intentionally uses the canonical StdSingle lock shape:
/// `hash_type=type`, builtin schnorr code hash, and 20-byte key-id args.
/// Returns the lock script and an empty witness.
pub(crate) fn op_true_script() -> (Script, Vec<u8>) {
    let lock_script = Script::new(builtin_schnorr_blake3_160_code_hash(), HASH_TYPE_TYPE, vec![0x11; 20]);
    // Witness is intentionally empty for this test helper.
    let witness = vec![];
    (lock_script, witness)
}
