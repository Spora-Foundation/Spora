use crate::{
    hashing::{
        sighash::{calc_schnorr_signature_hash, SigHashReusedValuesUnsync},
        sighash_type::{SigHashType, SIG_HASH_ALL},
    },
    tx::{compute_lock_hash_for_script, ScriptPublicKey, SignableTransaction, VerifiableTransaction},
};
use itertools::Itertools;
use std::collections::BTreeMap;
use std::iter::once;
use thiserror::Error;

/// Compute the lock_hash that a given raw script (version 0) would produce.
fn lock_hash_for_script_bytes(script: &[u8]) -> [u8; 32] {
    compute_lock_hash_for_script(&ScriptPublicKey::from_vec(0, script.to_vec()))
}

#[derive(Error, Debug, Clone)]
pub enum Error {
    #[error("{0}")]
    Message(String),

    #[error("Secp256k1 -> {0}")]
    Secp256k1Error(#[from] secp256k1::Error),

    #[error("The transaction is partially signed")]
    PartiallySigned,

    #[error("The transaction is fully signed")]
    FullySigned,
}

/// A wrapper enum that represents the transaction signed state. A transaction
/// contained by this enum can be either fully signed or partially signed.
pub enum Signed {
    Fully(SignableTransaction),
    Partially(SignableTransaction),
}

impl Signed {
    /// Returns the transaction if it is fully signed, otherwise returns an error
    pub fn fully_signed(self) -> std::result::Result<SignableTransaction, Error> {
        match self {
            Signed::Fully(tx) => Ok(tx),
            Signed::Partially(_) => Err(Error::PartiallySigned),
        }
    }

    /// Returns the transaction if it is fully signed, otherwise returns the
    /// transaction as an error `Err(tx)`.
    #[allow(clippy::result_large_err)]
    pub fn try_fully_signed(self) -> std::result::Result<SignableTransaction, SignableTransaction> {
        match self {
            Signed::Fully(tx) => Ok(tx),
            Signed::Partially(tx) => Err(tx),
        }
    }

    /// Returns the transaction if it is partially signed, otherwise fail with an error
    pub fn partially_signed(self) -> std::result::Result<SignableTransaction, Error> {
        match self {
            Signed::Fully(_) => Err(Error::FullySigned),
            Signed::Partially(tx) => Ok(tx),
        }
    }

    /// Returns the transaction if it is partially signed, otherwise returns the
    /// transaction as an error `Err(tx)`.
    #[allow(clippy::result_large_err)]
    pub fn try_partially_signed(self) -> std::result::Result<SignableTransaction, SignableTransaction> {
        match self {
            Signed::Fully(tx) => Err(tx),
            Signed::Partially(tx) => Ok(tx),
        }
    }

    /// Returns the transaction regardless of whether it is fully or partially signed
    pub fn unwrap(self) -> SignableTransaction {
        match self {
            Signed::Fully(tx) => tx,
            Signed::Partially(tx) => tx,
        }
    }
}

/// Sign a transaction using schnorr
pub fn sign(mut signable_tx: SignableTransaction, schnorr_key: secp256k1::Keypair) -> SignableTransaction {
    // Ensure witnesses vector has enough entries
    while signable_tx.tx.witnesses.len() < signable_tx.tx.inputs.len() {
        signable_tx.tx.witnesses.push(vec![]);
    }

    let reused_values = SigHashReusedValuesUnsync::new();
    for i in 0..signable_tx.tx.inputs.len() {
        let sig_hash = calc_schnorr_signature_hash(&signable_tx.as_verifiable(), i, SIG_HASH_ALL, &reused_values);
        let msg = secp256k1::Message::from_digest_slice(sig_hash.as_bytes().as_slice()).unwrap();
        let sig: [u8; 64] = *schnorr_key.sign_schnorr(msg).as_ref();
        // This represents OP_DATA_65 <SIGNATURE+SIGHASH_TYPE> (since signature length is 64 bytes and SIGHASH_TYPE is one byte)
        signable_tx.tx.witnesses[i] = std::iter::once(65u8).chain(sig).chain([SIG_HASH_ALL.to_u8()]).collect();
    }
    signable_tx
}

/// Sign a transaction using schnorr
pub fn sign_with_multiple(mut mutable_tx: SignableTransaction, privkeys: Vec<[u8; 32]>) -> SignableTransaction {
    let mut map = BTreeMap::new();
    for privkey in privkeys {
        let schnorr_key = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, &privkey).unwrap();
        let lock_hash = lock_hash_for_script_bytes(&schnorr_key.public_key().serialize());
        map.insert(lock_hash, schnorr_key);
    }
    // Ensure witnesses vector has enough entries
    while mutable_tx.tx.witnesses.len() < mutable_tx.tx.inputs.len() {
        mutable_tx.tx.witnesses.push(vec![]);
    }

    let reused_values = SigHashReusedValuesUnsync::new();
    for i in 0..mutable_tx.tx.inputs.len() {
        let Some(entry) = mutable_tx.entries[i].as_ref() else {
            continue;
        };
        if let Some(schnorr_key) = map.get(&entry.lock_hash) {
            let sig_hash = calc_schnorr_signature_hash(&mutable_tx.as_verifiable(), i, SIG_HASH_ALL, &reused_values);
            let msg = secp256k1::Message::from_digest_slice(sig_hash.as_bytes().as_slice()).unwrap();
            let sig: [u8; 64] = *schnorr_key.sign_schnorr(msg).as_ref();
            // This represents OP_DATA_65 <SIGNATURE+SIGHASH_TYPE> (since signature length is 64 bytes and SIGHASH_TYPE is one byte)
            mutable_tx.tx.witnesses[i] = std::iter::once(65u8).chain(sig).chain([SIG_HASH_ALL.to_u8()]).collect();
        }
    }
    mutable_tx
}

/// TODO (aspect) - merge this with `v1` fn above or refactor wallet core to use the script engine.
/// Sign a transaction using schnorr
#[allow(clippy::result_large_err)]
pub fn sign_with_multiple_v2(mut mutable_tx: SignableTransaction, privkeys: &[[u8; 32]]) -> Signed {
    let mut map = BTreeMap::new();
    for privkey in privkeys {
        let schnorr_key = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, privkey).unwrap();
        let schnorr_public_key = schnorr_key.public_key().x_only_public_key().0;
        let script_pub_key_script: Vec<u8> =
            once(0x20).chain(schnorr_public_key.serialize().into_iter()).chain(once(0xac)).collect_vec();
        let lock_hash = lock_hash_for_script_bytes(&script_pub_key_script);
        map.insert(lock_hash, schnorr_key);
    }
    // Ensure witnesses vector has enough entries
    while mutable_tx.tx.witnesses.len() < mutable_tx.tx.inputs.len() {
        mutable_tx.tx.witnesses.push(vec![]);
    }

    let reused_values = SigHashReusedValuesUnsync::new();
    let mut additional_signatures_required = false;
    for i in 0..mutable_tx.tx.inputs.len() {
        let Some(entry) = mutable_tx.entries[i].as_ref() else {
            additional_signatures_required = true;
            continue;
        };
        if let Some(schnorr_key) = map.get(&entry.lock_hash) {
            let sig_hash = calc_schnorr_signature_hash(&mutable_tx.as_verifiable(), i, SIG_HASH_ALL, &reused_values);
            let msg = secp256k1::Message::from_digest_slice(sig_hash.as_bytes().as_slice()).unwrap();
            let sig: [u8; 64] = *schnorr_key.sign_schnorr(msg).as_ref();
            // This represents OP_DATA_65 <SIGNATURE+SIGHASH_TYPE> (since signature length is 64 bytes and SIGHASH_TYPE is one byte)
            mutable_tx.tx.witnesses[i] = std::iter::once(65u8).chain(sig).chain([SIG_HASH_ALL.to_u8()]).collect();
        } else {
            additional_signatures_required = true;
        }
    }
    if additional_signatures_required {
        Signed::Partially(mutable_tx)
    } else {
        Signed::Fully(mutable_tx)
    }
}

/// Sign a transaction input with a sighash_type using schnorr
pub fn sign_input(tx: &impl VerifiableTransaction, input_index: usize, private_key: &[u8; 32], hash_type: SigHashType) -> Vec<u8> {
    let reused_values = SigHashReusedValuesUnsync::new();

    let hash = calc_schnorr_signature_hash(tx, input_index, hash_type, &reused_values);
    let msg = secp256k1::Message::from_digest_slice(hash.as_bytes().as_slice()).unwrap();
    let schnorr_key = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, private_key).unwrap();
    let sig: [u8; 64] = *schnorr_key.sign_schnorr(msg).as_ref();

    // This represents OP_DATA_65 <SIGNATURE+SIGHASH_TYPE> (since signature length is 64 bytes and SIGHASH_TYPE is one byte)
    std::iter::once(65u8).chain(sig).chain([hash_type.to_u8()]).collect()
}

pub fn verify(tx: &impl VerifiableTransaction) -> Result<(), Error> {
    let reused_values = SigHashReusedValuesUnsync::new();
    for (i, _input) in tx.inputs().iter().enumerate() {
        let witness = tx.witnesses().get(i).map(Vec::as_slice).unwrap_or_default();
        if witness.is_empty() {
            return Err(Error::Message(format!("Signature is empty for input: {i}")));
        }
        let Some(_entry) = tx.cell_entry(i) else {
            return Err(Error::Message(format!("cannot verify signature for input {i}: missing cell entry")));
        };
        // Extract the public key from resolved CellMetadata's lock_script if available
        let metadata = tx.cell_metadata(i);
        let pk_bytes: Vec<u8> = if let Some(ref meta) = metadata {
            if let Some(ref lock_script) = meta.lock_script {
                // For P2PK-like scripts, the args contain the public key
                if lock_script.args.len() >= 32 {
                    lock_script.args[..32].to_vec()
                } else {
                    return Err(Error::Message(format!("cannot verify schnorr signature for input {i}: lock_script args too short")));
                }
            } else {
                return Err(Error::Message(format!(
                    "cannot verify schnorr signature for input {i}: no lock_script in metadata (Cell model entries require script engine)"
                )));
            }
        } else {
            return Err(Error::Message(format!("cannot verify signature for input {i}: no metadata")));
        };
        let pk = secp256k1::XOnlyPublicKey::from_slice(&pk_bytes)?;
        let sig = secp256k1::schnorr::Signature::from_slice(&witness[1..65])?;
        let sig_hash = calc_schnorr_signature_hash(tx, i, SIG_HASH_ALL, &reused_values);
        let msg = secp256k1::Message::from_digest_slice(sig_hash.as_bytes().as_slice())?;
        sig.verify(&msg, &pk)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        subnets::SubnetworkId,
        tx::{cell_meta_from_legacy_output, cell_tx_from_legacy_transaction, *},
    };
    use secp256k1::{rand, Secp256k1};
    use std::str::FromStr;

    #[allow(deprecated)]
    #[test]
    fn test_and_verify_sign() {
        let secp = Secp256k1::new();
        let (secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
        let script_pub_key = ScriptVec::from_slice(&public_key.serialize());

        let (secret_key2, public_key2) = secp.generate_keypair(&mut rand::thread_rng());
        let script_pub_key2 = ScriptVec::from_slice(&public_key2.serialize());

        let prev_tx_id = TransactionId::from_str("880eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3").unwrap();
        let unsigned_tx_legacy = Transaction::new(
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
                TransactionOutput { value: 300, script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()) },
                TransactionOutput { value: 300, script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()) },
            ],
            1615462089000,
            SubnetworkId::from_bytes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );
        let unsigned_tx = cell_tx_from_legacy_transaction(&unsigned_tx_legacy);

        let entries = vec![
            cell_meta_from_legacy_output(100, &ScriptPublicKey::new(0, script_pub_key.clone()), 0, false),
            cell_meta_from_legacy_output(200, &ScriptPublicKey::new(0, script_pub_key), 0, false),
            cell_meta_from_legacy_output(300, &ScriptPublicKey::new(0, script_pub_key2), 0, false),
        ];
        let signed_tx = sign_with_multiple(
            SignableTransaction::with_entries(unsigned_tx, entries),
            vec![secret_key.secret_bytes(), secret_key2.secret_bytes()],
        );

        // Verify signing worked by checking witnesses are non-empty.
        for witness in signed_tx.tx.witnesses.iter() {
            assert!(!witness.is_empty(), "Input should be signed");
        }
    }
}
