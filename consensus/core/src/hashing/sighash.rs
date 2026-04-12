use arc_swap::ArcSwapOption;
use spora_hashes::{Hash, Hasher, HasherBase, SchnorrSigningHash, TransactionSigningHash, TransactionSigningHashECDSA, ZERO_HASH};
use std::cell::Cell;
use std::sync::Arc;

use crate::cell_metadata::{parse_cell_metadata_placeholder_script_public_key, PlaceholderCellMetadata};
use crate::tx::{CellOut, CellTx, ScriptPublicKey, TransactionOutpoint, VerifiableTransaction};

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

pub fn previous_outputs_hash(tx: &CellTx, hash_type: SigHashType, reused_values: &impl SigHashReusedValues) -> Hash {
    if hash_type.is_sighash_anyone_can_pay() {
        return ZERO_HASH;
    }
    let hash = || {
        let mut hasher = TransactionSigningHash::new();
        for input in tx.inputs.iter() {
            hasher.update(input.out_point.tx_hash);
            hasher.write_u32(input.out_point.index);
        }
        hasher.finalize()
    };
    reused_values.previous_outputs_hash(hash)
}

pub fn sequences_hash(tx: &CellTx, hash_type: SigHashType, reused_values: &impl SigHashReusedValues) -> Hash {
    if hash_type.is_sighash_single() || hash_type.is_sighash_anyone_can_pay() || hash_type.is_sighash_none() {
        return ZERO_HASH;
    }
    let hash = || {
        let mut hasher = TransactionSigningHash::new();
        for input in tx.inputs.iter() {
            hasher.write_u64(input.since);
        }
        hasher.finalize()
    };
    reused_values.sequences_hash(hash)
}

pub fn sig_op_counts_hash(tx: &CellTx, hash_type: SigHashType, reused_values: &impl SigHashReusedValues) -> Hash {
    if hash_type.is_sighash_anyone_can_pay() {
        return ZERO_HASH;
    }

    let hash = || {
        let mut hasher = TransactionSigningHash::new();
        // In Cell model, sig_op_count is always 1 per input (implicit)
        for _input in tx.inputs.iter() {
            hasher.write_u8(1);
        }
        hasher.finalize()
    };
    reused_values.sig_op_counts_hash(hash)
}

pub fn payload_hash(tx: &CellTx, reused_values: &impl SigHashReusedValues) -> Hash {
    let payload = tx.payload().unwrap_or_default();
    if !tx.is_coinbase() && payload.is_empty() {
        return ZERO_HASH;
    }

    let hash = || {
        let mut hasher = TransactionSigningHash::new();
        hasher.write_var_bytes(payload);
        hasher.finalize()
    };
    reused_values.payload_hash(hash)
}

pub fn outputs_hash(tx: &CellTx, hash_type: SigHashType, reused_values: &impl SigHashReusedValues, input_index: usize) -> Hash {
    if hash_type.is_sighash_none() {
        return ZERO_HASH;
    }

    if hash_type.is_sighash_single() {
        // If the relevant output exists - return its hash, otherwise return zero-hash
        if input_index >= tx.outputs.len() {
            return ZERO_HASH;
        }

        let mut hasher = TransactionSigningHash::new();
        hash_cell_output(
            &mut hasher,
            &tx.outputs[input_index],
            tx.outputs_data.get(input_index).map(Vec::as_slice).unwrap_or_default(),
        );
        return hasher.finalize();
    }
    let hash = || {
        let mut hasher = TransactionSigningHash::new();
        for (i, output) in tx.outputs.iter().enumerate() {
            let data = tx.outputs_data.get(i).map(Vec::as_slice).unwrap_or_default();
            hash_cell_output(&mut hasher, output, data);
        }
        hasher.finalize()
    };
    // Otherwise, return hash of all outputs. Re-use hash if available.
    reused_values.outputs_hash(hash)
}

pub fn hash_outpoint(hasher: &mut impl Hasher, outpoint: TransactionOutpoint) {
    hasher.update(outpoint.tx_hash);
    hasher.write_u32(outpoint.index);
}

pub fn hash_cell_output(hasher: &mut impl Hasher, output: &CellOut, data: &[u8]) {
    hasher.write_u64(output.capacity);
    // Hash lock script components
    hasher.update(output.lock.code_hash);
    hasher.write_u8(output.lock.hash_type);
    hasher.write_var_bytes(&output.lock.args);
    // Hash type script presence and components
    hasher.write_bool(output.type_.is_some());
    if let Some(ref type_script) = output.type_ {
        hasher.update(type_script.code_hash);
        hasher.write_u8(type_script.hash_type);
        hasher.write_var_bytes(&type_script.args);
    }
    // Hash output data
    hasher.write_var_bytes(data);
}

pub fn hash_script_public_key(hasher: &mut impl Hasher, script_public_key: &ScriptPublicKey) {
    hasher.write_u16(script_public_key.version());
    hasher.write_var_bytes(script_public_key.script());
}

fn hash_placeholder_cell_metadata(hasher: &mut impl Hasher, metadata: &PlaceholderCellMetadata) {
    hasher.update(metadata.lock_hash).write_bool(metadata.type_hash.is_some());
    if let Some(type_hash) = metadata.type_hash {
        hasher.update(type_hash);
    }
    hasher.update(metadata.data_hash).write_u64(metadata.data_bytes);
}

fn hash_script_public_key_or_metadata(hasher: &mut impl Hasher, script_public_key: &ScriptPublicKey) {
    if let Some(metadata) = parse_cell_metadata_placeholder_script_public_key(script_public_key) {
        hash_placeholder_cell_metadata(hasher, &metadata);
    } else {
        hash_script_public_key(hasher, script_public_key);
    }
}

fn real_signing_entry<'a>(_verifiable_tx: &'a impl VerifiableTransaction, _input_index: usize) -> Option<&'a crate::tx::CellEntry> {
    // CellMeta (aka CellEntry) always carries metadata, so there are no "real script" entries.
    // All signing now goes through the cell_metadata path.
    None
}

pub fn calc_schnorr_signature_hash(
    verifiable_tx: &impl VerifiableTransaction,
    input_index: usize,
    hash_type: SigHashType,
    reused_values: &impl SigHashReusedValues,
) -> Hash {
    let tx = verifiable_tx.tx();
    let input = &verifiable_tx.inputs()[input_index];
    let mut hasher = SchnorrSigningHash::new();
    hasher
        .write_u16(tx.ver)
        .update(previous_outputs_hash(tx, hash_type, reused_values))
        .update(sequences_hash(tx, hash_type, reused_values))
        .update(sig_op_counts_hash(tx, hash_type, reused_values));
    hash_outpoint(&mut hasher, input.out_point);
    if let Some(entry) = real_signing_entry(verifiable_tx, input_index) {
        // This branch is now unreachable since CellMeta always has metadata
        let metadata = entry.embedded_cell_metadata().expect("CellMeta always has metadata");
        hash_placeholder_cell_metadata(&mut hasher, &metadata);
        hasher.write_u64(entry.capacity);
    } else if let Some(metadata) = verifiable_tx.cell_metadata(input_index) {
        let placeholder = PlaceholderCellMetadata {
            lock_hash: metadata.lock_hash,
            type_hash: metadata.type_hash,
            data_hash: metadata.data_hash,
            data_bytes: metadata.data_bytes,
        };
        hash_placeholder_cell_metadata(&mut hasher, &placeholder);
        hasher.write_u64(metadata.capacity);
    } else {
        let entry = verifiable_tx
            .cell_entry(input_index)
            .expect("calc_schnorr_signature_hash requires either canonical cell metadata or a populated cell entry");
        let metadata = entry.embedded_cell_metadata().expect("CellMeta always has metadata");
        hash_placeholder_cell_metadata(&mut hasher, &metadata);
        hasher.write_u64(entry.capacity);
    }
    hasher
        .write_u64(input.since)
        .write_u8(1) // sig_op_count is implicit 1 in Cell model
        .update(outputs_hash(tx, hash_type, reused_values, input_index))
        .write_u64(0) // lock_time: no equivalent in CellTx
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
        cell_metadata::CellMetadata,
        hashing::sighash_type::{SIG_HASH_ALL, SIG_HASH_ANY_ONE_CAN_PAY, SIG_HASH_NONE, SIG_HASH_SINGLE},
        subnets::{SubnetworkId, SUBNETWORK_ID_NATIVE},
        tx::{
            cell_meta_from_legacy_output, cell_tx_from_legacy_transaction, outpoint_from_id, MutableTransaction, PopulatedTransaction,
            Transaction, TransactionId, TransactionInput,
        },
    };

    use super::*;

    #[allow(deprecated)]
    #[test]
    fn test_signature_hash() {
        // TODO: Copy all sighash tests from go sporad.
        let prev_tx_id = TransactionId::from_str("880eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3").unwrap();
        let mut bytes = [0u8; 34];
        faster_hex::hex_decode("208325613d2eeaf7176ac6c670b13c0043156c427438ed72d74b7800862ad884e8ac".as_bytes(), &mut bytes).unwrap();
        let script_pub_key_1 = SmallVec::from(bytes.to_vec());

        let mut bytes = [0u8; 34];
        faster_hex::hex_decode("20fcef4c106cf11135bbd70f02a726a92162d2fb8b22f0469126f800862ad884e8ac".as_bytes(), &mut bytes).unwrap();
        let script_pub_key_2 = SmallVec::from_vec(bytes.to_vec());

        let native_tx_legacy = Transaction::new(
            0,
            vec![
                TransactionInput {
                    previous_outpoint: outpoint_from_id(prev_tx_id, 0),
                    signature_script: vec![],
                    sequence: 0,
                    sig_op_count: 0,
                },
                TransactionInput {
                    previous_outpoint: outpoint_from_id(prev_tx_id, 1),
                    signature_script: vec![],
                    sequence: 1,
                    sig_op_count: 0,
                },
                TransactionInput {
                    previous_outpoint: outpoint_from_id(prev_tx_id, 2),
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
        let native_tx = cell_tx_from_legacy_transaction(&native_tx_legacy);

        let native_populated_tx = PopulatedTransaction::new(
            &native_tx,
            vec![
                cell_meta_from_legacy_output(100, &ScriptPublicKey::new(0, script_pub_key_1.clone()), 0, false),
                cell_meta_from_legacy_output(200, &ScriptPublicKey::new(0, script_pub_key_2.clone()), 0, false),
                cell_meta_from_legacy_output(300, &ScriptPublicKey::new(0, script_pub_key_2.clone()), 0, false),
            ],
        );

        let mut subnetwork_tx_legacy = native_tx_legacy.clone();
        subnetwork_tx_legacy.subnetwork_id = SubnetworkId::from_bytes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        subnetwork_tx_legacy.gas = 250;
        subnetwork_tx_legacy.payload = vec![10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20];
        let subnetwork_tx = cell_tx_from_legacy_transaction(&subnetwork_tx_legacy);
        let subnetwork_populated_tx = PopulatedTransaction::new(
            &subnetwork_tx,
            vec![
                cell_meta_from_legacy_output(100, &ScriptPublicKey::new(0, script_pub_key_1), 0, false),
                cell_meta_from_legacy_output(200, &ScriptPublicKey::new(0, script_pub_key_2.clone()), 0, false),
                cell_meta_from_legacy_output(300, &ScriptPublicKey::new(0, script_pub_key_2), 0, false),
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
            },
            TestVector {
                name: "native-all-0-modify-input-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::Input(1),
            },
            TestVector {
                name: "native-all-0-modify-output-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::Output(1),
            },
            TestVector {
                name: "native-all-0-modify-sequence-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::Sequence(1),
            },
            TestVector {
                name: "native-all-anyonecanpay-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::NoAction,
            },
            TestVector {
                name: "native-all-anyonecanpay-0-modify-input-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::Input(0),
            },
            TestVector {
                name: "native-all-anyonecanpay-0-modify-input-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::Input(1),
            },
            TestVector {
                name: "native-all-anyonecanpay-0-modify-sequence",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::Sequence(1),
            },
            // SIG_HASH_NONE
            TestVector {
                name: "native-none-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE,
                input_index: 0,
                action: ModifyAction::NoAction,
            },
            TestVector {
                name: "native-none-0-modify-output-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE,
                input_index: 0,
                action: ModifyAction::Output(1),
            },
            TestVector {
                name: "native-none-0-modify-sequence-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE,
                input_index: 0,
                action: ModifyAction::Sequence(0),
            },
            TestVector {
                name: "native-none-0-modify-sequence-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE,
                input_index: 0,
                action: ModifyAction::Sequence(1),
            },
            TestVector {
                name: "native-none-anyonecanpay-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::NoAction,
            },
            TestVector {
                name: "native-none-anyonecanpay-0-modify-amount-spent",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::AmountSpent(0),
            },
            TestVector {
                name: "native-none-anyonecanpay-0-modify-script-public-key",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_NONE_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::PrevScriptPublicKey(0),
            },
            // SIG_HASH_SINGLE
            TestVector {
                name: "native-single-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE,
                input_index: 0,
                action: ModifyAction::NoAction,
            },
            TestVector {
                name: "native-single-0-modify-output-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE,
                input_index: 0,
                action: ModifyAction::Output(1),
            },
            TestVector {
                name: "native-single-0-modify-sequence-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE,
                input_index: 0,
                action: ModifyAction::Sequence(0),
            },
            TestVector {
                name: "native-single-0-modify-sequence-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE,
                input_index: 0,
                action: ModifyAction::Sequence(1),
            },
            TestVector {
                name: "native-single-2-no-corresponding-output",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE,
                input_index: 2,
                action: ModifyAction::NoAction,
            },
            TestVector {
                name: "native-single-2-no-corresponding-output-modify-output-1",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE,
                input_index: 2,
                action: ModifyAction::Output(1),
            },
            TestVector {
                name: "native-single-anyonecanpay-0",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE_ANYONE_CAN_PAY,
                input_index: 0,
                action: ModifyAction::NoAction,
            },
            TestVector {
                name: "native-single-anyonecanpay-2-no-corresponding-output",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_SINGLE_ANYONE_CAN_PAY,
                input_index: 2,
                action: ModifyAction::NoAction,
            },
            TestVector {
                name: "native-all-0-modify-payload",
                populated_tx: &native_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::Payload,
            },
            // subnetwork transaction
            TestVector {
                name: "subnetwork-all-0",
                populated_tx: &subnetwork_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::NoAction,
            },
            TestVector {
                name: "subnetwork-all-modify-payload",
                populated_tx: &subnetwork_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::Payload,
            },
            TestVector {
                name: "subnetwork-all-modify-gas",
                populated_tx: &subnetwork_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::Gas,
            },
            TestVector {
                name: "subnetwork-all-subnetwork-id",
                populated_tx: &subnetwork_populated_tx,
                hash_type: SIG_HASH_ALL,
                input_index: 0,
                action: ModifyAction::SubnetworkId,
            },
        ];

        let mut expected_hashs = Vec::new();
        for test in tests {
            let mut tx = test.populated_tx.tx.clone();
            let mut entries = test.populated_tx.entries.clone();
            match test.action {
                ModifyAction::NoAction => {}
                ModifyAction::Output(i) => {
                    tx.outputs[i].capacity = 100;
                }
                ModifyAction::Input(i) => {
                    tx.inputs[i].out_point.index = 2;
                }
                ModifyAction::AmountSpent(i) => {
                    entries[i].capacity = 666;
                }
                ModifyAction::PrevScriptPublicKey(i) => {
                    // Simulate changing the script by modifying the lock_hash directly
                    entries[i].lock_hash = [0xFF; 32];
                }
                ModifyAction::Sequence(i) => {
                    tx.inputs[i].since = 12345;
                }
                ModifyAction::Payload => {
                    // In Cell model, modify outputs_data instead of payload
                    if !tx.outputs_data.is_empty() {
                        tx.outputs_data[0] = vec![6, 6, 6, 4, 2, 0, 1, 3, 3, 7];
                    }
                }
                ModifyAction::Gas => {}          // No equivalent in CellTx
                ModifyAction::SubnetworkId => {} // No equivalent in CellTx
            }
            let populated_tx = PopulatedTransaction::new(&tx, entries);
            let reused_values = SigHashReusedValuesUnsync::new();
            let actual_hash = calc_schnorr_signature_hash(&populated_tx, test.input_index, test.hash_type, &reused_values);
            expected_hashs.push((test.name, actual_hash));
        }
        insta::assert_debug_snapshot!(expected_hashs)
    }

    #[allow(deprecated)]
    #[test]
    fn test_signature_hash_resolved_metadata_overrides_embedded() {
        // When resolved_cell_metadata is explicitly set on MutableTransaction,
        // it overrides the auto-derived metadata from CellMeta.
        let prev_tx_id = TransactionId::from_str("880eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3").unwrap();
        let tx_legacy = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: outpoint_from_id(prev_tx_id, 0),
                signature_script: vec![],
                sequence: 0,
                sig_op_count: 0,
            }],
            vec![TransactionOutput { value: 100, script_public_key: ScriptPublicKey::from_vec(0, vec![0x20; 34]) }],
            0,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        let tx = cell_tx_from_legacy_transaction(&tx_legacy);

        let entry = cell_meta_from_legacy_output(42, &ScriptPublicKey::from_vec(0, vec![0x21; 34]), 7, false);

        let baseline = PopulatedTransaction::new(&tx, vec![entry.clone()]);
        let baseline_hash = calc_schnorr_signature_hash(&baseline, 0, SIG_HASH_ALL, &SigHashReusedValuesUnsync::new());

        // Override resolved metadata with different lock_hash/type_hash/data_hash/data_bytes
        let mut mutable = MutableTransaction::with_entries(tx.clone(), vec![entry]);
        mutable.resolved_cell_metadata[0] = Some(CellMetadata {
            out_point: tx.inputs[0].out_point,
            capacity: 42,
            data_bytes: 128,
            lock_hash: [0x11; 32],
            type_hash: Some([0x22; 32]),
            data_hash: [0x33; 32],
            block_daa_score: 7,
            is_cellbase: false,
            block_hash: Hash::default(),
            lock_code_hash: None,
            type_code_hash: None,
            lock_script: None,
            type_script: None,
            data: None,
        });
        let metadata_hash = calc_schnorr_signature_hash(&mutable.as_verifiable(), 0, SIG_HASH_ALL, &SigHashReusedValuesUnsync::new());

        // Resolved metadata has different fields, so the hash must differ
        assert_ne!(baseline_hash, metadata_hash);
    }

    #[allow(deprecated)]
    #[test]
    fn test_signature_hash_uses_metadata_for_placeholder_entries() {
        let prev_tx_id = TransactionId::from_str("880eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3").unwrap();
        let tx_legacy = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: outpoint_from_id(prev_tx_id, 0),
                signature_script: vec![],
                sequence: 0,
                sig_op_count: 0,
            }],
            vec![TransactionOutput { value: 100, script_public_key: ScriptPublicKey::from_vec(0, vec![0x20; 34]) }],
            0,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        let tx = cell_tx_from_legacy_transaction(&tx_legacy);

        let metadata = CellMetadata {
            out_point: tx.inputs[0].out_point,
            capacity: 500,
            data_bytes: 64,
            lock_hash: [0x44; 32],
            type_hash: Some([0x55; 32]),
            data_hash: [0x66; 32],
            block_daa_score: 11,
            is_cellbase: false,
            block_hash: Hash::default(),
            lock_code_hash: None,
            type_code_hash: None,
            lock_script: None,
            type_script: None,
            data: None,
        };

        let placeholder_entry = metadata.to_placeholder_cell_entry();
        let placeholder_tx = PopulatedTransaction::new(&tx, vec![placeholder_entry.clone()]);
        let placeholder_hash = calc_schnorr_signature_hash(&placeholder_tx, 0, SIG_HASH_ALL, &SigHashReusedValuesUnsync::new());

        let mut resolved = MutableTransaction::with_entries(tx, vec![placeholder_entry]);
        resolved.resolved_cell_metadata[0] = Some(metadata);
        let resolved_hash = calc_schnorr_signature_hash(&resolved.as_verifiable(), 0, SIG_HASH_ALL, &SigHashReusedValuesUnsync::new());

        assert_eq!(placeholder_hash, resolved_hash);
    }
}
