#![cfg(test)]
#![allow(deprecated)]

use spora_consensus_core::tx::{pay_to_script_hash_script, ScriptPublicKey};

const OP_TRUE: u8 = 0x51;

pub(crate) fn op_true_script() -> (ScriptPublicKey, Vec<u8>) {
    let redeem_script = vec![OP_TRUE];
    let script_public_key = pay_to_script_hash_script(&redeem_script);
    (script_public_key, redeem_script)
}
