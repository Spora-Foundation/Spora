use arc_swap::ArcSwapOption;
use std::cell::Cell;
use std::sync::Arc;
use tondi_hashes::{Hash, Hasher, HasherBase, TransactionSigningHash, TransactionSigningHashECDSA, SchnorrSigningHash, ZERO_HASH};

use crate::tx::{ScriptPublicKey, Transaction, TransactionOutpoint, TransactionOutput, VerifiableTransaction};

use super::{sighash_type::SigHashType, HasherExtensions};

/// Holds all fields used in the calculation of a transaction's sig_hash which are
/// the same for all transaction inputs.
/// Reuse of such values prevents the quadratic hashing problem.
#[derive(Default)]
pub struct SigHashReusedValuesUnsync {
    previous_outputs_hash: Cell<Option<Hash>>,
    sequences_hash: Cell<Option<Hash>>,
    sig_op_counts_hash: Cell<Option<Hash>>,
    outputs_hash: Cell<Option<Hash>>,
    payload_hash: Cell<Option<Hash>>,
}

impl SigHashReusedValuesUnsync {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Default)]
pub struct SigHashReusedValuesSync {
    previous_outputs_hash: ArcSwapOption<Hash>,
    sequences_hash: ArcSwapOption<Hash>,
    sig_op_counts_hash: ArcSwapOption<Hash>,
    outputs_hash: ArcSwapOption<Hash>,
    payload_hash: ArcSwapOption<Hash>,
}

impl SigHashReusedValuesSync {
    pub fn new() -> Self {
        Self::default()
    }
}

pub trait SigHashReusedValues {
    fn previous_outputs_hash(&self, set: impl Fn() -> Hash) -> Hash;
    fn sequences_hash(&self, set: impl Fn() -> Hash) -> Hash;
    fn sig_op_counts_hash(&self, set: impl Fn() -> Hash) -> Hash;
    fn outputs_hash(&self, set: impl Fn() -> Hash) -> Hash;
    fn payload_hash(&self, set: impl Fn() -> Hash) -> Hash;
}

impl SigHashReusedValues for SigHashReusedValuesUnsync {
    fn previous_outputs_hash(&self, set: impl Fn() -> Hash) -> Hash {
        self.previous_outputs_hash.get().unwrap_or_else(|| {
            let hash = set();
            self.previous_outputs_hash.set(Some(hash));
            hash
        })
    }

    fn sequences_hash(&self, set: impl Fn() -> Hash) -> Hash {
        self.sequences_hash.get().unwrap_or_else(|| {
            let hash = set();
            self.sequences_hash.set(Some(hash));
            hash
        })
    }

    fn sig_op_counts_hash(&self, set: impl Fn() -> Hash) -> Hash {
        self.sig_op_counts_hash.get().unwrap_or_else(|| {
            let hash = set();
            self.sig_op_counts_hash.set(Some(hash));
            hash
        })
    }

    fn outputs_hash(&self, set: impl Fn() -> Hash) -> Hash {
        self.outputs_hash.get().unwrap_or_else(|| {
            let hash = set();
            self.outputs_hash.set(Some(hash));
            hash
        })
    }

    fn payload_hash(&self, set: impl Fn() -> Hash) -> Hash {
        self.payload_hash.get().unwrap_or_else(|| {
            let hash = set();
            self.payload_hash.set(Some(hash));
            hash
        })
    }
}

impl SigHashReusedValues for SigHashReusedValuesSync {
    fn previous_outputs_hash(&self, set: impl Fn() -> Hash) -> Hash {
        if let Some(value) = self.previous_outputs_hash.load().as_ref() {
            return **value;
        }
        let hash = set();
        self.previous_outputs_hash.rcu(|_| Arc::new(hash));
        hash
    }

    fn sequences_hash(&self, set: impl Fn() -> Hash) -> Hash {
        if let Some(value) = self.sequences_hash.load().as_ref() {
            return **value;
        }
        let hash = set();
        self.sequences_hash.rcu(|_| Arc::new(hash));
        hash
    }

    fn sig_op_counts_hash(&self, set: impl Fn() -> Hash) -> Hash {
        if let Some(value) = self.sig_op_counts_hash.load().as_ref() {
            return **value;
        }
        let hash = set();
        self.sig_op_counts_hash.rcu(|_| Arc::new(hash));
        hash
    }

    fn outputs_hash(&self, set: impl Fn() -> Hash) -> Hash {
        if let Some(value) = self.outputs_hash.load().as_ref() {
            return **value;
        }
        let hash = set();
        self.outputs_hash.rcu(|_| Arc::new(hash));
        hash
    }

    fn payload_hash(&self, set: impl Fn() -> Hash) -> Hash {
        if let Some(value) = self.payload_hash.load().as_ref() {
            return **value;
        }
        let hash = set();
        self.payload_hash.rcu(|_| Arc::new(hash));
        hash
    }
}

pub fn previous_outputs_hash(tx: &Transaction, hash_type: SigHashType, reused_values: &impl SigHashReusedValues) -> Hash {
    if hash_type.is_sighash_anyone_can_pay() {
        return ZERO_HASH;
    }
    let hash = || {
        let mut hasher = TransactionSigningHash::new();
        for input in tx.inputs.iter() {
            hasher.update(input.previous_outpoint.transaction_id.as_bytes());
            hasher.write_u32(input.previous_outpoint.index);
        }
        hasher.finalize()
    };
    reused_values.previous_outputs_hash(hash)
}

pub fn sequences_hash(tx: &Transaction, hash_type: SigHashType, reused_values: &impl SigHashReusedValues) -> Hash {
    if hash_type.is_sighash_single() || hash_type.is_sighash_anyone_can_pay() || hash_type.is_sighash_none() {
        return ZERO_HASH;
    }
    let hash = || {
        let mut hasher = TransactionSigningHash::new();
        for input in tx.inputs.iter() {
            hasher.write_u64(input.sequence);
        }
        hasher.finalize()
    };
    reused_values.sequences_hash(hash)
}

pub fn sig_op_counts_hash(tx: &Transaction, hash_type: SigHashType, reused_values: &impl SigHashReusedValues) -> Hash {
    if hash_type.is_sighash_anyone_can_pay() {
        return ZERO_HASH;
    }

    let hash = || {
        let mut hasher = TransactionSigningHash::new();
        for input in tx.inputs.iter() {
            hasher.write_u8(input.sig_op_count);
        }
        hasher.finalize()
    };
    reused_values.sig_op_counts_hash(hash)
}

pub fn payload_hash(tx: &Transaction, reused_values: &impl SigHashReusedValues) -> Hash {
    if tx.subnetwork_id.is_native() && tx.payload.is_empty() {
        return ZERO_HASH;
    }

    let hash = || {
        let mut hasher = TransactionSigningHash::new();
        hasher.write_var_bytes(&tx.payload);
        hasher.finalize()
    };
    reused_values.payload_hash(hash)
}

pub fn outputs_hash(tx: &Transaction, hash_type: SigHashType, reused_values: &impl SigHashReusedValues, input_index: usize) -> Hash {
    if hash_type.is_sighash_none() {
        return ZERO_HASH;
    }

    if hash_type.is_sighash_single() {
        // If the relevant output exists - return its hash, otherwise return zero-hash
        if input_index >= tx.outputs.len() {
            return ZERO_HASH;
        }

        let mut hasher = TransactionSigningHash::new();
        hash_output(&mut hasher, &tx.outputs[input_index]);
        return hasher.finalize();
    }
    let hash = || {
        let mut hasher = TransactionSigningHash::new();
        for output in tx.outputs.iter() {
            hash_output(&mut hasher, output);
        }
        hasher.finalize()
    };
    // Otherwise, return hash of all outputs. Re-use hash if available.
    reused_values.outputs_hash(hash)
}

pub fn hash_outpoint(hasher: &mut impl Hasher, outpoint: TransactionOutpoint) {
    hasher.update(outpoint.transaction_id);
    hasher.write_u32(outpoint.index);
}

pub fn hash_output(hasher: &mut impl Hasher, output: &TransactionOutput) {
    hasher.write_u64(output.value);
    hash_script_public_key(hasher, &output.script_public_key);
}

pub fn hash_script_public_key(hasher: &mut impl Hasher, script_public_key: &ScriptPublicKey) {
    hasher.write_u16(script_public_key.version());
    hasher.write_var_bytes(script_public_key.script());
}

pub fn calc_schnorr_signature_hash(
    verifiable_tx: &impl VerifiableTransaction,
    input_index: usize,
    hash_type: SigHashType,
    reused_values: &impl SigHashReusedValues,
) -> Hash {
    let input = verifiable_tx.populated_input(input_index);
    let tx = verifiable_tx.tx();
    let mut hasher = SchnorrSigningHash::new();
    hasher
        .write_u16(tx.version)
        .update(previous_outputs_hash(tx, hash_type, reused_values))
        .update(sequences_hash(tx, hash_type, reused_values))
        .update(sig_op_counts_hash(tx, hash_type, reused_values));
    hash_outpoint(&mut hasher, input.0.previous_outpoint);
    hash_script_public_key(&mut hasher, &input.1.script_public_key);
    hasher
        .write_u64(input.1.amount)
        .write_u64(input.0.sequence)
        .write_u8(input.0.sig_op_count)
        .update(outputs_hash(tx, hash_type, reused_values, input_index))
        .write_u64(tx.lock_time)
        .update(&tx.subnetwork_id)
        .write_u64(tx.gas)
        .update(payload_hash(tx, reused_values))
        .write_u8(hash_type.to_u8());
    hasher.finalize()
}

pub fn calc_ecdsa_signature_hash(
    tx: &impl VerifiableTransaction,
    input_index: usize,
    hash_type: SigHashType,
    reused_values: &impl SigHashReusedValues,
) -> Hash {
    let hash = calc_schnorr_signature_hash(tx, input_index, hash_type, reused_values);
    let mut hasher = TransactionSigningHashECDSA::new();
    hasher.update(hash);
    hasher.finalize()
}

#[cfg(test)]
mod tests {
    use std::{str::FromStr, vec};

    use smallvec::SmallVec;

    use crate::{
        hashing::sighash_type::{SIG_HASH_ALL, SIG_HASH_ANY_ONE_CAN_PAY, SIG_HASH_NONE, SIG_HASH_SINGLE},
        subnets::{SubnetworkId, SUBNETWORK_ID_NATIVE},
        tx::{PopulatedTransaction, Transaction, TransactionId, TransactionInput, UtxoEntry},
    };

    use super::*;

    #[test]
    fn test_signature_hash() {
        // TODO: Copy all sighash tests from go tondid.
        let prev_tx_id = TransactionId::from_str("880eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3").unwrap();
        let mut bytes = [0u8; 34];
        faster_hex::hex_decode("208325613d2eeaf7176ac6c670b13c0043156c427438ed72d74b7800862ad884e8ac".as_bytes(), &mut bytes).unwrap();
        let script_pub_key_1 = SmallVec::from(bytes.to_vec());

        let mut bytes = [0u8; 34];
        faster_hex::hex_decode("20fcef4c106cf11135bbd70f02a726a92162d2fb8b22f0469126f800862ad884e8ac".as_bytes(), &mut bytes).unwrap();
        let script_pub_key_2 = SmallVec::from_vec(bytes.to_vec());

        let native_tx = Transaction::new(
            0,
            vec![
                TransactionInput {
                    previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                    signature_script: vec![],
                    sequence: 0,
                    sig_op_count: 0,
                },
                TransactionInput {
                    previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 1 },
                    signature_script: vec![],
                    sequence: 1,
                    sig_op_count: 0,
                },
                TransactionInput {
                    previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 2 },
                    signature_script: vec![],
                    sequence: 2,
                    sig_op_count: 0,
                },
            ],
            vec![
                TransactionOutput { value: 300, script_public_key: ScriptPublicKey::new(0, script_pub_key_2.clone()) },
                TransactionOutput { value: 300, script_public_key: ScriptPublicKey::new(0, script_pub_key_1.clone()) },
            ],
            1615462089000,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );

        let native_populated_tx = PopulatedTransaction::new(
            &native_tx,
            vec![
                UtxoEntry {
                    amount: 100,
                    script_public_key: ScriptPublicKey::new(0, script_pub_key_1.clone()),
                    block_daa_score: 0,
                    is_coinbase: false,
                },
                UtxoEntry {
                    amount: 200,
                    script_public_key: ScriptPublicKey::new(0, script_pub_key_2.clone()),
                    block_daa_score: 0,
                    is_coinbase: false,
                },
                UtxoEntry {
                    amount: 300,
                    script_public_key: ScriptPublicKey::new(0, script_pub_key_2.clone()),
                    block_daa_score: 0,
                    is_coinbase: false,
                },
            ],
        );

        let mut subnetwork_tx = native_tx.clone();
        subnetwork_tx.subnetwork_id = SubnetworkId::from_bytes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        subnetwork_tx.gas = 250;
        subnetwork_tx.payload = vec![10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20];
        let subnetwork_populated_tx = PopulatedTransaction::new(
            &subnetwork_tx,
            vec![
                UtxoEntry {
                    amount: 100,
                    script_public_key: ScriptPublicKey::new(0, script_pub_key_1),
                    block_daa_score: 0,
                    is_coinbase: false,
                },
                UtxoEntry {
                    amount: 200,
                    script_public_key: ScriptPublicKey::new(0, script_pub_key_2.clone()),
                    block_daa_score: 0,
                    is_coinbase: false,
                },
                UtxoEntry {
                    amount: 300,
                    script_public_key: ScriptPublicKey::new(0, script_pub_key_2),
                    block_daa_score: 0,
                    is_coinbase: false,
                },
            ],
        );

        enum ModifyAction {
            NoAction,
            Output(usize),
            Input(usize),
            AmountSpent(usize),
            PrevScriptPublicKey(usize),
            Sequence(usize),
            Payload,
            Gas,
            SubnetworkId,
        }

        struct TestVector<'a> {
            name: &'static str,
            populated_tx: &'a PopulatedTransaction<'a>,
            hash_type: SigHashType,
            input_index: usize,
            action: ModifyAction,
            expected_hash: &'static str,
        }

        const SIG_HASH_ALL_ANYONE_CAN_PAY: SigHashType = SigHashType(SIG_HASH_ALL.0 | SIG_HASH_ANY_ONE_CAN_PAY.0);
        const SIG_HASH_NONE_ANYONE_CAN_PAY: SigHashType = SigHashType(SIG_HASH_NONE.0 | SIG_HASH_ANY_ONE_CAN_PAY.0);
        const SIG_HASH_SINGLE_ANYONE_CAN_PAY: SigHashType = SigHashType(SIG_HASH_SINGLE.0 | SIG_HASH_ANY_ONE_CAN_PAY.0);

        let tests = [
            // SIG_HASH_ALL
            TestVector {
                name: "native-all-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::NoAction,
                expected_hash: "2496bde1dabf3af3d2057cc77260d4e499935dcb80628b130ee536db6c1f65d9",
            },
            TestVector {
                name: "native-all-0-modify-input-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::Input(1),
                expected_hash: "8511a615713650b6fc026d77be22e5dac230660ad6b722609ef54565306717c8",
            },
            TestVector {
                name: "native-all-0-modify-output-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::Output(1),
                expected_hash: "a5e29a32fd70c8a5d50a48e3ef7542f6eb5eab974e63afba8972f200078e3c83",
            },
            TestVector {
                name: "native-all-0-modify-sequence-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::Sequence(1),
                expected_hash: "26ab27c86c04c72299b12a2882fbd1de8f460518afec77046297cf72de6fddd5",
            },
            TestVector {
                name: "native-all-anyonecanpay-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::NoAction,
                expected_hash: "b4cf020d9d353f9e251176243628028136e03042acde8b97c095f449959a3b9d",
            },
            TestVector {
                name: "native-all-anyonecanpay-0-modify-input-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::Input(0),
                expected_hash: "e06fe198aa43741d985e48cad5b3ccdc775c7fbde552867553f0610c87d6bc4a",
            },
            TestVector {
                name: "native-all-anyonecanpay-0-modify-input-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::Input(1),
                expected_hash: "b4cf020d9d353f9e251176243628028136e03042acde8b97c095f449959a3b9d",
            },
            TestVector {
                name: "native-all-anyonecanpay-0-modify-sequence",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::Sequence(1),
                expected_hash: "b4cf020d9d353f9e251176243628028136e03042acde8b97c095f449959a3b9d",
            },
            // SIG_HASH_NONE
            TestVector {
                name: "native-none-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE,
                input_index: 0,
                action: ModifyAction::NoAction,
                expected_hash: "322aa93591edffb7a5e407bad8ebf7d7d151e28a83a62177dd7b8f776cc7ed22",
            },
            TestVector {
                name: "native-none-0-modify-output-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE,
                input_index: 0,
                action: ModifyAction::Output(1),
                expected_hash: "322aa93591edffb7a5e407bad8ebf7d7d151e28a83a62177dd7b8f776cc7ed22",
            },
            TestVector {
                name: "native-none-0-modify-sequence-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE,
                input_index: 0,
                action: ModifyAction::Sequence(0),
                expected_hash: "6e619e4771af34a2a13332b26e2ccfe79b90f215dbe37a8d5a9f52d45530aeb7",
            },
            TestVector {
                name: "native-none-0-modify-sequence-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE,
                input_index: 0,
                action: ModifyAction::Sequence(1),
                expected_hash: "322aa93591edffb7a5e407bad8ebf7d7d151e28a83a62177dd7b8f776cc7ed22",
            },
            TestVector {
                name: "native-none-anyonecanpay-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::NoAction,
                expected_hash: "8c33dc5e5e1837ac150b7d761be4389afd706529fcc245366e5e8d160000ff87",
            },
            TestVector {
                name: "native-none-anyonecanpay-0-modify-amount-spent",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::AmountSpent(0),
                expected_hash: "f999c2a7c9c4750e1fc625a921ae4163da3d50a96d0b5db53862448d2905124a",
            },
            TestVector {
                name: "native-none-anyonecanpay-0-modify-script-public-key",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::PrevScriptPublicKey(0),
                expected_hash: "7cf976bb4175a61a94d01ae3005fc644d1c29370c0ae3110b1397bb722ad15c8",
            },
            // SIG_HASH_SINGLE
            TestVector {
                name: "native-single-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE,
                input_index: 0,
                action: ModifyAction::NoAction,
                expected_hash: "edd5f0bb4011936e246e959e5537afbbca024ef81e00bdb880232090d77f8b74",
            },
            TestVector {
                name: "native-single-0-modify-output-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE,
                input_index: 0,
                action: ModifyAction::Output(1),
                expected_hash: "edd5f0bb4011936e246e959e5537afbbca024ef81e00bdb880232090d77f8b74",
            },
            TestVector {
                name: "native-single-0-modify-sequence-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE,
                input_index: 0,
                action: ModifyAction::Sequence(0),
                expected_hash: "ded7fe3f54cd163e1c8f9f695ebc89d89fa22935148824d68b5fa4682fcca98d",
            },
            TestVector {
                name: "native-single-0-modify-sequence-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE,
                input_index: 0,
                action: ModifyAction::Sequence(1),
                expected_hash: "edd5f0bb4011936e246e959e5537afbbca024ef81e00bdb880232090d77f8b74",
            },
            TestVector {
                name: "native-single-2-no-corresponding-output",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE,
                input_index: 2,
                action: ModifyAction::NoAction,
                expected_hash: "92fe49f75292d358d1ece29976d9b7bd3077bb91f85e76cafe3537707e812d31",
            },
            TestVector {
                name: "native-single-2-no-corresponding-output-modify-output-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE,
                input_index: 2,
                action: ModifyAction::Output(1),
                expected_hash: "92fe49f75292d358d1ece29976d9b7bd3077bb91f85e76cafe3537707e812d31",
            },
            TestVector {
                name: "native-single-anyonecanpay-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::NoAction,
                expected_hash: "b56d9aabf3f41d68d0d66cd64ed3210d3d64a9b84d36bfd72d2dd31b9195d8a2",
            },
            TestVector {
                name: "native-single-anyonecanpay-2-no-corresponding-output",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE_ANYONE_CAN_PAY,
                input_index: 2,
                action: ModifyAction::NoAction,
                expected_hash: "5e503c096052bda3318b9f424e9354a23493dc1a2d413590b68fc4402082372f",
            },
            TestVector {
                name: "native-all-0-modify-payload",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::Payload,
                expected_hash: "14a60aa00cd2bd03e304ab9bf1f86fade2a47e25cb8d4833b682136e15e47d93",
            },
            // subnetwork transaction
            TestVector {
                name: "subnetwork-all-0",
                populated_tx: &subnetwork_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::NoAction,
                expected_hash: "7190a8e319375cb6a6af4f4532b69846452eeb7db344836c282d92ad747e4898",
            },
            TestVector {
                name: "subnetwork-all-modify-payload",
                populated_tx: &subnetwork_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::Payload,
                expected_hash: "235cde3d553c6eea0e88c45010409fdbae9378b95b7f5366a3c0df0189031e74",
            },
            TestVector {
                name: "subnetwork-all-modify-gas",
                populated_tx: &subnetwork_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::Gas,
                expected_hash: "5cad256a1e01e63347a72eb41b60ed340281a4a596ba364691a48d4b3dd30212",
            },
            TestVector {
                name: "subnetwork-all-subnetwork-id",
                populated_tx: &subnetwork_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::SubnetworkId,
                expected_hash: "10dcfa634e5f1515d89e29592d18d0c8a2d4d358f418fe01a2d6148280c3425c",
            },
        ];

        for test in tests {
            let mut tx = test.populated_tx.tx.clone();
            let mut entries = test.populated_tx.entries.clone();
            match test.action {
                ModifyAction::NoAction => {}
                ModifyAction::Output(i) => {
                    tx.outputs[i].value = 100;
                }
                ModifyAction::Input(i) => {
                    tx.inputs[i].previous_outpoint.index = 2;
                }
                ModifyAction::AmountSpent(i) => {
                    entries[i].amount = 666;
                }
                ModifyAction::PrevScriptPublicKey(i) => {
                    let mut script_vec = entries[i].script_public_key.script().to_vec();
                    script_vec.append(&mut vec![1, 2, 3]);
                    entries[i].script_public_key = ScriptPublicKey::new(entries[i].script_public_key.version(), script_vec.into());
                }
                ModifyAction::Sequence(i) => {
                    tx.inputs[i].sequence = 12345;
                }
                ModifyAction::Payload => tx.payload = vec![6, 6, 6, 4, 2, 0, 1, 3, 3, 7],
                ModifyAction::Gas => tx.gas = 1234,
                ModifyAction::SubnetworkId => {
                    tx.subnetwork_id = SubnetworkId::from_bytes([6, 6, 6, 4, 2, 0, 1, 3, 3, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
                }
            }
            let populated_tx = PopulatedTransaction::new(&tx, entries);
            let reused_values = SigHashReusedValuesUnsync::new();
            let actual_hash = calc_schnorr_signature_hash(&populated_tx, test.input_index, test.hash_type, &reused_values);
            assert_eq!(actual_hash.to_string(), test.expected_hash, "Test case {} failed", test.name);
        }
    }
}
