#![cfg(test)]

use spora_consensus_core::tx::{pay_to_script_hash_lock_script, ScriptRef};

const OP_TRUE: u8 = 0x51;

pub(crate) fn op_true_script() -> (ScriptRef, Vec<u8>) {
    let redeem_script = vec![OP_TRUE];
    let lock_script = pay_to_script_hash_lock_script(&redeem_script);
    (lock_script, redeem_script)
}
