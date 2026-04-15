use arc_swap::ArcSwapOption;
use spora_exec::celltx::sighash::{
    calc_standard_ecdsa_signature_hash, calc_standard_signature_hash, hash_cell_output as exec_hash_cell_output,
    hash_outpoint as exec_hash_outpoint, standard_outputs_hash, standard_payload_hash, standard_previous_outputs_hash,
    standard_sequences_hash, standard_sig_op_counts_hash, StandardSigHashType, StandardSigningInput,
};
use spora_hashes::{Hash, Hasher};
use std::cell::Cell;
use std::sync::Arc;

use crate::tx::{CellOutput, CellTx, TransactionOutpoint, VerifiableTransaction};

use super::sighash_type::SigHashType;

pub use spora_exec::celltx::sighash::StandardSigHashReusedValues as SigHashReusedValues;

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

impl StandardSigHashType for SigHashType {
    fn is_sighash_none(self) -> bool {
        self.is_sighash_none()
    }

    fn is_sighash_single(self) -> bool {
        self.is_sighash_single()
    }

    fn is_sighash_anyone_can_pay(self) -> bool {
        self.is_sighash_anyone_can_pay()
    }

    fn to_u8(self) -> u8 {
        self.to_u8()
    }
}

pub fn previous_outputs_hash(tx: &CellTx, hash_type: SigHashType, reused_values: &impl SigHashReusedValues) -> Hash {
    standard_previous_outputs_hash(tx, hash_type, reused_values)
}

pub fn sequences_hash(tx: &CellTx, hash_type: SigHashType, reused_values: &impl SigHashReusedValues) -> Hash {
    standard_sequences_hash(tx, hash_type, reused_values)
}

pub fn sig_op_counts_hash(tx: &CellTx, hash_type: SigHashType, reused_values: &impl SigHashReusedValues) -> Hash {
    standard_sig_op_counts_hash(tx, hash_type, reused_values)
}

pub fn payload_hash(tx: &CellTx, reused_values: &impl SigHashReusedValues) -> Hash {
    standard_payload_hash(tx, reused_values)
}

pub fn hash_outpoint(hasher: &mut impl Hasher, outpoint: TransactionOutpoint) {
    exec_hash_outpoint(hasher, outpoint);
}

pub fn hash_cell_output(hasher: &mut impl Hasher, output: &CellOutput, data: &[u8]) {
    exec_hash_cell_output(hasher, output, data);
}

pub fn outputs_hash(tx: &CellTx, hash_type: SigHashType, reused_values: &impl SigHashReusedValues, input_index: usize) -> Hash {
    standard_outputs_hash(tx, hash_type, reused_values, input_index)
}

fn signing_input_material(verifiable_tx: &impl VerifiableTransaction, input_index: usize) -> StandardSigningInput {
    if let Some(cell_metadata) = verifiable_tx.cell_metadata(input_index) {
        return StandardSigningInput {
            lock_hash: cell_metadata.lock_hash,
            type_hash: cell_metadata.type_hash,
            data_hash: cell_metadata.data_hash,
            data_bytes: cell_metadata.data_bytes,
            capacity: cell_metadata.capacity,
        };
    }

    let entry = verifiable_tx
        .cell_entry(input_index)
        .expect("calc_schnorr_signature_hash requires either canonical cell metadata or a populated cell entry");
    StandardSigningInput {
        lock_hash: entry.lock_hash,
        type_hash: entry.type_hash,
        data_hash: entry.data_hash,
        data_bytes: entry.data_bytes,
        capacity: entry.capacity,
    }
}

pub fn calc_schnorr_signature_hash(
    verifiable_tx: &impl VerifiableTransaction,
    input_index: usize,
    hash_type: SigHashType,
    reused_values: &impl SigHashReusedValues,
) -> Hash {
    let tx = verifiable_tx.tx();
    let signing_input = signing_input_material(verifiable_tx, input_index);
    calc_standard_signature_hash(tx, input_index, hash_type, &signing_input, reused_values)
}

pub fn calc_ecdsa_signature_hash(
    tx: &impl VerifiableTransaction,
    input_index: usize,
    hash_type: SigHashType,
    reused_values: &impl SigHashReusedValues,
) -> Hash {
    let signing_input = signing_input_material(tx, input_index);
    calc_standard_ecdsa_signature_hash(tx.tx(), input_index, hash_type, &signing_input, reused_values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cell_diff::CellMeta,
        cell_metadata::CellMetadata,
        hashing::sighash_type::SIG_HASH_ALL,
        tx::{CellInput, CellOutput, CellTx, MutableTransaction, OutPoint, Script},
    };
    use spora_hashes::Hash;

    fn test_cell_tx() -> CellTx {
        let tx = CellTx::new(
            vec![CellInput::new(OutPoint::new([0x11; 32], 0), 42)],
            vec![],
            vec![CellOutput { lock: Script::new([0x55; 32], 1, vec![0xAB; 20]), type_: None, capacity: 1_000 }],
            vec![vec![0xCC; 8]],
            vec![vec![1, 2, 3]],
        );
        tx.expect("test tx should be valid")
    }

    fn test_entry() -> CellMeta {
        CellMeta {
            out_point: OutPoint::new([0x22; 32], 7),
            capacity: 2_500,
            data_bytes: 16,
            lock_hash: [0x33; 32],
            type_hash: Some([0x44; 32]),
            data_hash: [0x55; 32],
            block_daa_score: 99,
            is_cellbase: false,
        }
    }

    fn test_metadata() -> CellMetadata {
        CellMetadata {
            out_point: OutPoint::new([0x22; 32], 7),
            capacity: 2_500,
            data_bytes: 16,
            lock_hash: [0x33; 32],
            type_hash: Some([0x44; 32]),
            data_hash: [0x55; 32],
            block_daa_score: 99,
            is_cellbase: false,
            block_hash: Hash::from_bytes([0x66; 32]),
            lock_code_hash: Some([0x77; 32]),
            type_code_hash: Some([0x88; 32]),
            lock_script: Some(Script::new([0x99; 32], 1, vec![0xAA; 20])),
            type_script: None,
            data: Some(vec![0xBB; 16]),
        }
    }

    #[test]
    fn test_signature_hash_matches_between_entry_and_metadata_views() {
        let tx = test_cell_tx();
        let entry_tx = MutableTransaction {
            tx: tx.clone(),
            entries: vec![Some(test_entry())],
            resolved_cell_metadata: vec![None],
            calculated_fee: None,
            calculated_non_contextual_masses: None,
            calculated_contextual_masses: None,
            verified_cycles: None,
        };
        let metadata_tx = MutableTransaction::with_resolved_metadata(tx, vec![test_metadata()]);
        let reused_entry = SigHashReusedValuesUnsync::new();
        let reused_metadata = SigHashReusedValuesUnsync::new();

        let entry_hash = calc_schnorr_signature_hash(&entry_tx.as_verifiable(), 0, SIG_HASH_ALL, &reused_entry);
        let metadata_hash = calc_schnorr_signature_hash(&metadata_tx.as_verifiable(), 0, SIG_HASH_ALL, &reused_metadata);

        assert_eq!(entry_hash, metadata_hash);
    }

    #[test]
    fn test_signature_hash_prefers_resolved_metadata_when_both_exist() {
        let tx = test_cell_tx();
        let entry =
            CellMeta { lock_hash: [0x10; 32], type_hash: None, data_hash: [0x20; 32], data_bytes: 0, capacity: 100, ..test_entry() };
        let metadata = CellMetadata {
            lock_hash: [0x33; 32],
            type_hash: Some([0x44; 32]),
            data_hash: [0x55; 32],
            data_bytes: 16,
            capacity: 2_500,
            ..test_metadata()
        };
        let both_tx = MutableTransaction {
            tx,
            entries: vec![Some(entry)],
            resolved_cell_metadata: vec![Some(metadata.clone())],
            calculated_fee: None,
            calculated_non_contextual_masses: None,
            calculated_contextual_masses: None,
            verified_cycles: None,
        };
        let metadata_only_tx = MutableTransaction::with_resolved_metadata(both_tx.tx.clone(), vec![metadata]);
        let reused_both = SigHashReusedValuesUnsync::new();
        let reused_metadata = SigHashReusedValuesUnsync::new();

        let hash_with_both = calc_schnorr_signature_hash(&both_tx.as_verifiable(), 0, SIG_HASH_ALL, &reused_both);
        let hash_with_metadata_only =
            calc_schnorr_signature_hash(&metadata_only_tx.as_verifiable(), 0, SIG_HASH_ALL, &reused_metadata);

        assert_eq!(hash_with_both, hash_with_metadata_only);
    }
}
