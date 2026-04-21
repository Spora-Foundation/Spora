use crate::{
    hashing::{
        sighash::{calc_ecdsa_signature_hash, calc_schnorr_signature_hash, SigHashReusedValuesUnsync},
        sighash_type::{SigHashType, SIG_HASH_ALL},
    },
    tx::{
        builtin_ecdsa_blake3_160_code_hash, builtin_schnorr_blake3_160_code_hash, classify_script, Script, ScriptClass,
        SignableTransaction, VerifiableTransaction, HASH_TYPE_TYPE,
    },
};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Clone, Copy)]
enum NativeLockKind {
    StdSingle,
    StdSingleEcdsa,
}

/// Domain tag used to derive StdSingle key-id payloads.
pub const KEY_ID_DOMAIN_SCHNORR: &[u8] = b"spora/key-id/schnorr/v1";
/// Domain tag used to derive StdSingleECDSA key-id payloads.
pub const KEY_ID_DOMAIN_ECDSA: &[u8] = b"spora/key-id/ecdsa/v1";

/// Derive a 20-byte key-id as `blake3(domain || data)[0..20]`.
pub fn key_id20(domain: &[u8], data: &[u8]) -> [u8; 20] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(data);
    let digest = hasher.finalize();
    let mut out = [0u8; 20];
    out.copy_from_slice(&digest.as_bytes()[..20]);
    out
}

fn script_hash_for_standard(code_hash: [u8; 32], args: Vec<u8>) -> [u8; 32] {
    Script::new(code_hash, HASH_TYPE_TYPE, args).hash()
}

fn native_lock_hashes(privkey: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    let keypair = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, privkey).unwrap();
    let xonly = keypair.public_key().x_only_public_key().0.serialize();
    let ecdsa = keypair.public_key().serialize();
    let schnorr_key_id = key_id20(KEY_ID_DOMAIN_SCHNORR, &xonly);
    let ecdsa_key_id = key_id20(KEY_ID_DOMAIN_ECDSA, &ecdsa);
    (
        script_hash_for_standard(builtin_schnorr_blake3_160_code_hash(), schnorr_key_id.to_vec()),
        script_hash_for_standard(builtin_ecdsa_blake3_160_code_hash(), ecdsa_key_id.to_vec()),
    )
}

fn witness_from_standard_envelope(pubkey: &[u8], signature: [u8; 64], hash_type: SigHashType) -> Vec<u8> {
    let mut witness = Vec::with_capacity(4 + pubkey.len() + signature.len());
    witness.push(1); // version
    witness.push(hash_type.to_u8());
    witness.push(pubkey.len() as u8);
    witness.extend_from_slice(pubkey);
    witness.push(signature.len() as u8);
    witness.extend_from_slice(&signature);
    witness
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedStandardWitnessEnvelope {
    pub signature: [u8; 64],
    pub hash_type: SigHashType,
    pub pubkey: Vec<u8>,
}

pub fn parse_standard_witness_envelope(witness: &[u8], input_index: usize) -> Result<ParsedStandardWitnessEnvelope, Error> {
    if witness.first().copied() != Some(1) {
        return Err(Error::Message(format!("invalid standard witness envelope for input {input_index}: expected versioned envelope")));
    }
    if witness.len() < 5 {
        return Err(Error::Message(format!("invalid standard witness envelope for input {input_index}: too short")));
    }

    let hash_type = SigHashType::from_u8(witness[1])
        .map_err(|err| Error::Message(format!("invalid sighash type for input {input_index}: {err}")))?;
    let pubkey_len = usize::from(witness[2]);
    if pubkey_len != 32 && pubkey_len != 33 {
        return Err(Error::Message(format!("invalid standard witness envelope for input {input_index}: bad pubkey length marker")));
    }

    let pubkey_start = 3usize;
    let pubkey_end = pubkey_start + pubkey_len;
    if pubkey_end >= witness.len() {
        return Err(Error::Message(format!("invalid standard witness envelope for input {input_index}: bad pubkey length")));
    }

    let sig_len = usize::from(witness[pubkey_end]);
    let sig_start = pubkey_end + 1;
    let sig_end = sig_start + sig_len;
    if sig_end > witness.len() || sig_len != 64 {
        return Err(Error::Message(format!("invalid standard witness envelope for input {input_index}: bad signature length")));
    }
    if sig_end != witness.len() {
        return Err(Error::Message(format!(
            "invalid standard witness envelope for input {input_index}: trailing bytes are not allowed"
        )));
    }

    let signature = witness[sig_start..sig_end]
        .try_into()
        .map_err(|_| Error::Message(format!("invalid signature length for input {input_index}: expected 64 bytes")))?;
    let pubkey = witness[pubkey_start..pubkey_end].to_vec();

    Ok(ParsedStandardWitnessEnvelope { signature, hash_type, pubkey })
}

fn sign_for_kind(
    tx: &impl VerifiableTransaction,
    input_index: usize,
    private_key: &[u8; 32],
    hash_type: SigHashType,
    reused_values: &impl crate::hashing::sighash::SigHashReusedValues,
    kind: NativeLockKind,
) -> Vec<u8> {
    let msg = match kind {
        NativeLockKind::StdSingle => {
            let hash = calc_schnorr_signature_hash(tx, input_index, hash_type, reused_values);
            secp256k1::Message::from_digest_slice(hash.as_bytes().as_slice()).unwrap()
        }
        NativeLockKind::StdSingleEcdsa => {
            let hash = calc_ecdsa_signature_hash(tx, input_index, hash_type, reused_values);
            secp256k1::Message::from_digest_slice(hash.as_bytes().as_slice()).unwrap()
        }
    };

    let signature = match kind {
        NativeLockKind::StdSingle => {
            let schnorr_key = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, private_key).unwrap();
            *schnorr_key.sign_schnorr(msg).as_ref()
        }
        NativeLockKind::StdSingleEcdsa => {
            let secret_key = secp256k1::SecretKey::from_slice(private_key).unwrap();
            secp256k1::SECP256K1.sign_ecdsa(&msg, &secret_key).serialize_compact()
        }
    };

    match kind {
        NativeLockKind::StdSingle => {
            let keypair = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, private_key).unwrap();
            witness_from_standard_envelope(&keypair.public_key().x_only_public_key().0.serialize(), signature, hash_type)
        }
        NativeLockKind::StdSingleEcdsa => {
            let keypair = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, private_key).unwrap();
            witness_from_standard_envelope(&keypair.public_key().serialize(), signature, hash_type)
        }
    }
}

fn infer_lock_kind_for_private_key(
    tx: &impl VerifiableTransaction,
    input_index: usize,
    private_key: &[u8; 32],
) -> Option<NativeLockKind> {
    if let Some(metadata) = tx.cell_metadata(input_index) {
        if let Some(lock_script) = metadata.lock_script {
            let keypair = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, private_key).ok()?;
            let xonly = keypair.public_key().x_only_public_key().0.serialize();
            let ecdsa = keypair.public_key().serialize();
            match classify_script(&lock_script) {
                ScriptClass::StdSingle if lock_script.args.len() == 20 => {
                    if lock_script.args == key_id20(KEY_ID_DOMAIN_SCHNORR, &xonly) {
                        return Some(NativeLockKind::StdSingle);
                    }
                }
                ScriptClass::StdSingleECDSA if lock_script.args.len() == 20 => {
                    if lock_script.args == key_id20(KEY_ID_DOMAIN_ECDSA, &ecdsa) {
                        return Some(NativeLockKind::StdSingleEcdsa);
                    }
                }
                _ => {}
            }
        }
    }

    let entry = tx.cell_entry(input_index)?;
    let (std_single_hash, std_single_ecdsa_hash) = native_lock_hashes(private_key);
    if entry.lock_hash == std_single_hash {
        Some(NativeLockKind::StdSingle)
    } else if entry.lock_hash == std_single_ecdsa_hash {
        Some(NativeLockKind::StdSingleEcdsa)
    } else {
        None
    }
}

#[derive(Debug)]
struct ParsedWitness {
    signature: [u8; 64],
    hash_type: SigHashType,
    pubkey: Option<Vec<u8>>,
}

fn extract_signature_and_hash_type(witness: &[u8], input_index: usize) -> Result<ParsedWitness, Error> {
    let parsed = parse_standard_witness_envelope(witness, input_index)?;
    Ok(ParsedWitness { signature: parsed.signature, hash_type: parsed.hash_type, pubkey: Some(parsed.pubkey) })
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

    let private_key = schnorr_key.secret_bytes();
    let reused_values = SigHashReusedValuesUnsync::new();
    for i in 0..signable_tx.tx.inputs.len() {
        let kind = {
            let verifiable = signable_tx.as_verifiable();
            infer_lock_kind_for_private_key(&verifiable, i, &private_key).unwrap_or(NativeLockKind::StdSingle)
        };
        let witness = {
            let verifiable = signable_tx.as_verifiable();
            sign_for_kind(&verifiable, i, &private_key, SIG_HASH_ALL, &reused_values, kind)
        };
        signable_tx.tx.witnesses[i] = witness;
    }
    signable_tx
}

/// Sign a transaction with multiple private keys.
///
/// The returned transaction may remain partially signed when not all required keys are provided.
pub fn sign_with_multiple(mutable_tx: SignableTransaction, privkeys: Vec<[u8; 32]>) -> SignableTransaction {
    sign_with_multiple_v2(mutable_tx, &privkeys).unwrap()
}

/// Sign a transaction with multiple private keys and return explicit full/partial status.
#[allow(clippy::result_large_err)]
pub fn sign_with_multiple_v2(mut mutable_tx: SignableTransaction, privkeys: &[[u8; 32]]) -> Signed {
    let mut map = BTreeMap::new();
    for privkey in privkeys {
        let (std_single_hash, std_single_ecdsa_hash) = native_lock_hashes(privkey);
        map.insert(std_single_hash, (*privkey, NativeLockKind::StdSingle));
        map.insert(std_single_ecdsa_hash, (*privkey, NativeLockKind::StdSingleEcdsa));
    }
    // Ensure witnesses vector has enough entries
    while mutable_tx.tx.witnesses.len() < mutable_tx.tx.inputs.len() {
        mutable_tx.tx.witnesses.push(vec![]);
    }

    let reused_values = SigHashReusedValuesUnsync::new();
    let mut additional_signatures_required = false;
    for i in 0..mutable_tx.tx.inputs.len() {
        let Some(lock_kind) =
            mutable_tx.entries.get(i).and_then(Option::as_ref).and_then(|entry| map.get(&entry.lock_hash).copied()).or_else(|| {
                privkeys.iter().find_map(|private_key| {
                    infer_lock_kind_for_private_key(&mutable_tx.as_verifiable(), i, private_key).map(|kind| (*private_key, kind))
                })
            })
        else {
            additional_signatures_required = true;
            continue;
        };
        let witness = {
            let verifiable = mutable_tx.as_verifiable();
            sign_for_kind(&verifiable, i, &lock_kind.0, SIG_HASH_ALL, &reused_values, lock_kind.1)
        };
        mutable_tx.tx.witnesses[i] = witness;
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
    let kind = infer_lock_kind_for_private_key(tx, input_index, private_key).unwrap_or(NativeLockKind::StdSingle);
    sign_for_kind(tx, input_index, private_key, hash_type, &reused_values, kind)
}

pub fn verify(tx: &impl VerifiableTransaction) -> Result<(), Error> {
    let reused_values = SigHashReusedValuesUnsync::new();
    let secp = secp256k1::Secp256k1::new();
    for (i, _input) in tx.inputs().iter().enumerate() {
        let witness = tx.witnesses().get(i).map(Vec::as_slice).unwrap_or_default();
        if witness.is_empty() {
            return Err(Error::Message(format!("Signature is empty for input: {i}")));
        }
        let Some(_entry) = tx.cell_entry(i) else {
            return Err(Error::Message(format!("cannot verify signature for input {i}: missing cell entry")));
        };
        let parsed_witness = extract_signature_and_hash_type(witness, i)?;
        let metadata =
            tx.cell_metadata(i).ok_or_else(|| Error::Message(format!("cannot verify signature for input {i}: no metadata")))?;
        let lock_script = metadata.lock_script.ok_or_else(|| {
            Error::Message(format!(
                "cannot verify signature for input {i}: no lock_script in metadata (Cell model entries require script engine)"
            ))
        })?;

        match classify_script(&lock_script) {
            ScriptClass::StdSingle if lock_script.args.len() == 20 => {
                let pubkey = parsed_witness
                    .pubkey
                    .as_ref()
                    .ok_or_else(|| Error::Message(format!("missing pubkey in standard witness envelope for input {i}")))?;
                if pubkey.len() != 32 {
                    return Err(Error::Message(format!("invalid schnorr pubkey length for input {i}: expected 32")));
                }
                if lock_script.args != key_id20(KEY_ID_DOMAIN_SCHNORR, pubkey) {
                    return Err(Error::Message(format!("standard schnorr key-id mismatch for input {i}")));
                }
                let pk = secp256k1::XOnlyPublicKey::from_slice(pubkey)?;
                let sig = secp256k1::schnorr::Signature::from_slice(&parsed_witness.signature)?;
                let sig_hash = calc_schnorr_signature_hash(tx, i, parsed_witness.hash_type, &reused_values);
                let msg = secp256k1::Message::from_digest_slice(sig_hash.as_bytes().as_slice())?;
                sig.verify(&msg, &pk)?;
            }
            ScriptClass::StdSingleECDSA if lock_script.args.len() == 20 => {
                let pubkey = parsed_witness
                    .pubkey
                    .as_ref()
                    .ok_or_else(|| Error::Message(format!("missing pubkey in standard witness envelope for input {i}")))?;
                if pubkey.len() != 33 {
                    return Err(Error::Message(format!("invalid ecdsa pubkey length for input {i}: expected 33")));
                }
                if lock_script.args != key_id20(KEY_ID_DOMAIN_ECDSA, pubkey) {
                    return Err(Error::Message(format!("standard ecdsa key-id mismatch for input {i}")));
                }
                let pk = secp256k1::PublicKey::from_slice(pubkey)?;
                let mut sig = secp256k1::ecdsa::Signature::from_compact(&parsed_witness.signature)?;
                let original = sig.serialize_compact();
                sig.normalize_s();
                if sig.serialize_compact() != original {
                    return Err(Error::Message(format!("non-canonical ecdsa signature (high-S) for input {i}")));
                }
                let sig_hash = calc_ecdsa_signature_hash(tx, i, parsed_witness.hash_type, &reused_values);
                let msg = secp256k1::Message::from_digest_slice(sig_hash.as_bytes().as_slice())?;
                secp.verify_ecdsa(&msg, &sig, &pk)?;
            }
            _ => {
                return Err(Error::Message(format!("cannot verify standard signature for input {i}: unsupported lock script shape")));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell_diff::CellMeta;
    use crate::tx::*;
    use secp256k1::{rand, Keypair, Secp256k1};
    use spora_addresses::{Address, Prefix};
    use std::str::FromStr;

    fn lock_script_from_pubkey(pubkey: &[u8; 32]) -> Script {
        pay_to_address_lock_script(&Address::new_std_single(Prefix::Testnet, pubkey).unwrap())
    }

    fn lock_script_from_ecdsa_pubkey(pubkey: &[u8; 33]) -> Script {
        pay_to_address_lock_script(&Address::new_std_single_ecdsa(Prefix::Testnet, pubkey).unwrap())
    }

    fn cell_entry_from_lock_script(value: u64, lock_script: &Script, block_daa_score: u64, is_cellbase: bool) -> CellMeta {
        CellMeta {
            out_point: TransactionOutpoint::default(),
            capacity: value,
            data_bytes: 0,
            lock_hash: lock_script.hash(),
            type_hash: None,
            data_hash: [0; 32],
            block_daa_score,
            is_cellbase,
            lock_script: Some(lock_script.clone()),
            type_script: None,
            data: Some(Vec::new()),
        }
    }

    #[test]
    fn test_and_verify_sign() {
        let secp = Secp256k1::new();
        let (secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
        let script_pub_key = public_key.x_only_public_key().0.serialize();

        let (secret_key2, public_key2) = secp.generate_keypair(&mut rand::thread_rng());
        let script_pub_key2 = public_key2.x_only_public_key().0.serialize();

        let lock_script = lock_script_from_pubkey(&script_pub_key);
        let lock_script2 = lock_script_from_pubkey(&script_pub_key2);

        let prev_tx_id = TransactionId::from_str("880eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3").unwrap();
        let unsigned_tx = CellTx::new(
            vec![
                CellInput::new(outpoint_from_id(prev_tx_id, 0), 0),
                CellInput::new(outpoint_from_id(prev_tx_id, 1), 1),
                CellInput::new(outpoint_from_id(prev_tx_id, 2), 2),
            ],
            vec![],
            vec![
                CellOutput { capacity: 300, lock: lock_script.clone(), type_: None },
                CellOutput { capacity: 300, lock: lock_script.clone(), type_: None },
            ],
            vec![vec![], vec![]],
            vec![vec![], vec![], vec![]],
        )
        .unwrap();

        let entries = vec![
            cell_entry_from_lock_script(100, &lock_script, 0, false),
            cell_entry_from_lock_script(200, &lock_script, 0, false),
            cell_entry_from_lock_script(300, &lock_script2, 0, false),
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

    #[test]
    fn sign_with_multiple_keeps_partial_signing_behavior() {
        let secp = Secp256k1::new();
        let (secret_key_a, public_key_a) = secp.generate_keypair(&mut rand::thread_rng());
        let (secret_key_b, public_key_b) = secp.generate_keypair(&mut rand::thread_rng());
        let lock_script_a = lock_script_from_pubkey(&public_key_a.x_only_public_key().0.serialize());
        let lock_script_b = lock_script_from_pubkey(&public_key_b.x_only_public_key().0.serialize());

        let unsigned_tx = CellTx::new(
            vec![
                CellInput::new(TransactionOutpoint::new([0x21; 32], 0), 0),
                CellInput::new(TransactionOutpoint::new([0x22; 32], 0), 0),
            ],
            vec![],
            vec![CellOutput { capacity: 100, lock: lock_script_a.clone(), type_: None }],
            vec![vec![]],
            vec![vec![], vec![]],
        )
        .unwrap();

        let entries = vec![
            cell_entry_from_lock_script(100, &lock_script_a, 0, false),
            cell_entry_from_lock_script(100, &lock_script_b, 0, false),
        ];

        let signed_tx = sign_with_multiple(SignableTransaction::with_entries(unsigned_tx, entries), vec![secret_key_a.secret_bytes()]);

        assert_eq!(signed_tx.tx.witnesses.len(), 2);
        assert!(!signed_tx.tx.witnesses[0].is_empty(), "input signed by available key must contain witness");
        assert!(signed_tx.tx.witnesses[1].is_empty(), "input without matching key must remain unsigned");

        // Keep `secret_key_b` live to avoid accidental "unused variable" edits changing test intent.
        let _ = secret_key_b;
    }

    #[test]
    fn verify_accepts_signed_transaction_with_resolved_pubkey_lock_metadata() {
        let secp = Secp256k1::new();
        let (secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
        let script_pub_key = public_key.x_only_public_key().0.serialize();
        let lock_script = lock_script_from_pubkey(&script_pub_key);

        let unsigned_tx = CellTx::new(
            vec![CellInput::new(TransactionOutpoint::new([0x77; 32], 0), 0)],
            vec![],
            vec![CellOutput { capacity: 100, lock: lock_script.clone(), type_: None }],
            vec![vec![]],
            vec![vec![]],
        )
        .unwrap();

        let metadata = crate::cell_metadata::CellMetadata {
            out_point: TransactionOutpoint::new([0x77; 32], 0),
            capacity: 200,
            data_bytes: 0,
            lock_hash: lock_script.hash(),
            type_hash: None,
            data_hash: [0; 32],
            block_daa_score: 0,
            is_cellbase: false,
            block_hash: TransactionId::from([0x99; 32]),
            lock_code_hash: None,
            type_code_hash: None,
            lock_script: Some(lock_script.clone()),
            type_script: None,
            data: Some(vec![]),
        };

        let signed_tx = sign(
            MutableTransaction::with_resolved_metadata(unsigned_tx, vec![metadata]),
            Keypair::from_secret_key(&secp, &secret_key),
        );
        let mut signable = signed_tx.clone();
        signable.entries[0] = Some(cell_entry_from_lock_script(200, &lock_script, 0, false));

        assert!(verify(&signable.as_verifiable()).is_ok());
    }

    #[test]
    fn verify_accepts_signed_transaction_with_resolved_pubkey_ecdsa_lock_metadata() {
        let secp = Secp256k1::new();
        let (secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
        let script_pub_key = public_key.serialize();
        let lock_script = lock_script_from_ecdsa_pubkey(&script_pub_key);

        let unsigned_tx = CellTx::new(
            vec![CellInput::new(TransactionOutpoint::new([0x88; 32], 0), 0)],
            vec![],
            vec![CellOutput { capacity: 100, lock: lock_script.clone(), type_: None }],
            vec![vec![]],
            vec![vec![]],
        )
        .unwrap();

        let metadata = crate::cell_metadata::CellMetadata {
            out_point: TransactionOutpoint::new([0x88; 32], 0),
            capacity: 200,
            data_bytes: 0,
            lock_hash: lock_script.hash(),
            type_hash: None,
            data_hash: [0; 32],
            block_daa_score: 0,
            is_cellbase: false,
            block_hash: TransactionId::from([0xAA; 32]),
            lock_code_hash: None,
            type_code_hash: None,
            lock_script: Some(lock_script.clone()),
            type_script: None,
            data: Some(vec![]),
        };

        let signed_tx = sign_with_multiple_v2(
            MutableTransaction::with_resolved_metadata(unsigned_tx, vec![metadata]),
            &[secret_key.secret_bytes()],
        )
        .fully_signed()
        .expect("ecdsa lock should be fully signed");
        let mut signable = signed_tx.clone();
        signable.entries[0] = Some(cell_entry_from_lock_script(200, &lock_script, 0, false));

        assert!(verify(&signable.as_verifiable()).is_ok());
    }

    #[test]
    fn sign_with_multiple_v2_signs_ecdsa_entries() {
        let secp = Secp256k1::new();
        let (secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
        let lock_script = lock_script_from_ecdsa_pubkey(&public_key.serialize());
        let unsigned_tx = CellTx::new(
            vec![CellInput::new(TransactionOutpoint::new([0x99; 32], 0), 0)],
            vec![],
            vec![CellOutput { capacity: 150, lock: lock_script.clone(), type_: None }],
            vec![vec![]],
            vec![vec![]],
        )
        .unwrap();
        let signed = sign_with_multiple_v2(
            SignableTransaction::with_entries(unsigned_tx, vec![cell_entry_from_lock_script(200, &lock_script, 0, false)]),
            &[secret_key.secret_bytes()],
        )
        .fully_signed()
        .expect("ecdsa entry should be signable");

        assert_eq!(signed.tx.witnesses.len(), 1);
        assert_eq!(signed.tx.witnesses[0].first().copied(), Some(1));
        assert_eq!(signed.tx.witnesses[0].len(), 101);
    }

    #[test]
    fn standard_witness_envelope_rejects_trailing_bytes() {
        let mut witness = vec![1u8, SIG_HASH_ALL.to_u8(), 32u8];
        witness.extend_from_slice(&[0x11; 32]);
        witness.push(64);
        witness.extend_from_slice(&[0x22; 64]);
        witness.push(0x99);

        let err = extract_signature_and_hash_type(&witness, 0).expect_err("trailing bytes must be rejected");
        assert!(err.to_string().contains("trailing bytes"));
    }

    #[test]
    fn standard_witness_envelope_rejects_invalid_pubkey_len_marker() {
        let mut witness = vec![1u8, SIG_HASH_ALL.to_u8(), 31u8];
        witness.extend_from_slice(&[0x11; 31]);
        witness.push(64);
        witness.extend_from_slice(&[0x22; 64]);

        let err = extract_signature_and_hash_type(&witness, 0).expect_err("invalid pubkey marker must be rejected");
        assert!(err.to_string().contains("bad pubkey length marker"));
    }

    #[test]
    fn standard_witness_envelope_rejects_unversioned_witness() {
        let witness = vec![0u8; 65];
        let err = parse_standard_witness_envelope(&witness, 0).expect_err("missing version marker must be rejected");
        assert!(err.to_string().contains("expected versioned envelope"));
    }

    #[test]
    fn standard_witness_envelope_rejects_invalid_sighash_type() {
        let mut witness = vec![1u8, 0xff, 32u8];
        witness.extend_from_slice(&[0x11; 32]);
        witness.push(64);
        witness.extend_from_slice(&[0x22; 64]);

        let err = parse_standard_witness_envelope(&witness, 0).expect_err("invalid sighash type must be rejected");
        assert!(err.to_string().contains("invalid sighash type"));
    }

    #[test]
    fn standard_witness_envelope_rejects_invalid_signature_len_marker() {
        let mut witness = vec![1u8, SIG_HASH_ALL.to_u8(), 32u8];
        witness.extend_from_slice(&[0x11; 32]);
        witness.push(63);
        witness.extend_from_slice(&[0x22; 63]);

        let err = parse_standard_witness_envelope(&witness, 0).expect_err("invalid signature length must be rejected");
        assert!(err.to_string().contains("bad signature length"));
    }
}
