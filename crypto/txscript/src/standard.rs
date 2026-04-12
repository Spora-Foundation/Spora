use crate::{
    opcodes::codes::{
        OpBlake3, OpCheckLockTimeVerify, OpCheckSig, OpCheckSigECDSA, OpData32, OpData33, OpDrop, OpElse, OpEndIf, OpEqual,
        OpEqualVerify, OpFalse, OpIf, OpTrue,
    },
    script_builder::{ScriptBuilder, ScriptBuilderError, ScriptBuilderResult},
    script_class::ScriptClass,
    SCRIPT_VER_CLASSIC,
};
use blake3::hash;
use smallvec::SmallVec;
use spora_addresses::{Address, Prefix, Version};
use spora_consensus_core::tx::{ScriptPublicKey, ScriptVec};
use spora_txscript_errors::TxScriptError;
use std::iter::once;

mod multisig;

pub use multisig::{multisig_redeem_script, multisig_redeem_script_ecdsa, Error as MultisigCreateError};

/// Creates a new script to pay a transaction output to a 32-byte pubkey.
fn pay_to_pub_key(address_payload: &[u8]) -> ScriptVec {
    // TODO: use ScriptBuilder when add_op and add_data fns or equivalents are available
    assert_eq!(address_payload.len(), 32);
    SmallVec::from_iter(once(OpData32).chain(address_payload.iter().copied()).chain(once(OpCheckSig)))
}

pub fn pay_to_pub_key_with_lock_time(address_payload: &[u8], lock_time: u64) -> ScriptBuilderResult<Vec<u8>> {
    assert_eq!(address_payload.len(), 32);
    let script = ScriptBuilder::new()
        .add_lock_time(lock_time)?
        .add_op(OpCheckLockTimeVerify)?
        .add_data(address_payload)?
        .add_op(OpCheckSig)?
        .drain();
    Ok(script)
}

/// Creates a new script to pay a transaction output to a 33-byte ECDSA pubkey.
fn pay_to_pub_key_ecdsa(address_payload: &[u8]) -> ScriptVec {
    // TODO: use ScriptBuilder when add_op and add_data fns or equivalents are available
    assert_eq!(address_payload.len(), 33);
    SmallVec::from_iter(once(OpData33).chain(address_payload.iter().copied()).chain(once(OpCheckSigECDSA)))
}

/// Creates a new script to pay a transaction output to a script hash.
/// It is expected that the input is a valid hash.
fn pay_to_script_hash(script_hash: &[u8]) -> ScriptVec {
    // TODO: use ScriptBuilder when add_op and add_data fns or equivalents are available
    assert_eq!(script_hash.len(), 32);
    SmallVec::from_iter([OpBlake3, OpData32].iter().copied().chain(script_hash.iter().copied()).chain(once(OpEqual)))
}

/// Creates a new script to pay a transaction output to the specified address.
pub fn pay_to_address_script(address: &Address) -> ScriptPublicKey {
    let (script, version) = match address.version {
        Version::PubKey => (pay_to_pub_key(address.payload.as_slice()), SCRIPT_VER_CLASSIC),
        Version::PubKeyECDSA => (pay_to_pub_key_ecdsa(address.payload.as_slice()), SCRIPT_VER_CLASSIC),
        Version::ScriptHash => (pay_to_script_hash(address.payload.as_slice()), SCRIPT_VER_CLASSIC),
    };
    ScriptPublicKey::new(version, script)
}

/// Creates a new script to pay a transaction output to the specified address with lock time.
///
/// This function creates a Time Locked Contract (TLC) script that requires:
/// 1. The transaction's lock time to be greater than or equal to the specified lock_time
/// 2. A valid signature from the address owner
///
/// The lock_time can be either:
/// - A block height (if < LOCK_TIME_THRESHOLD)
/// - A Unix timestamp (if >= LOCK_TIME_THRESHOLD)
///
/// # Arguments
/// * `address` - The address to pay to (must be PubKey version)
/// * `lock_time` - The minimum lock time required to spend this output
///
/// # Returns
/// * `Ok(ScriptPublicKey)` - The constructed script public key
/// * `Err(ScriptBuilderError::InvalidAddressVersion)` - If address is not PubKey version
///
/// # Example
/// ```
/// use spora_txscript::pay_to_address_with_lock_time_script;
/// use spora_addresses::Address;
///
/// let addr = Address::constructor("spora0:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvp55hu9");
/// let lock_time = 1756684800; // Unix timestamp
/// let script = pay_to_address_with_lock_time_script(&addr, lock_time).unwrap();
/// ```
pub fn pay_to_address_with_lock_time_script(address: &Address, lock_time: u64) -> ScriptBuilderResult<ScriptPublicKey> {
    if address.version != Version::PubKey {
        return Err(ScriptBuilderError::InvalidAddressVersion(address.version as u8));
    }

    let xpub = address.payload.as_slice();
    let redeem_script = pay_to_pub_key_with_lock_time(xpub, lock_time)?;
    Ok(pay_to_script_hash_script(&redeem_script))
}

/// Creates a Hash Time Locked Contract (HTLC) script.
///
/// This function creates an HTLC script that allows spending in two ways:
/// 1. With the correct preimage (secret) and a valid signature from the recipient
/// 2. With a valid signature from the sender after the lock time expires
///
/// The HTLC script structure:
/// ```text
/// OP_IF
///   OP_BLAKE3 <hash(secret)> OP_EQUALVERIFY
///   <recipient_pubkey> OP_CHECKSIG
/// OP_ELSE
///   <lock_time> OP_CHECKLOCKTIMEVERIFY OP_DROP
///   <sender_pubkey> OP_CHECKSIG
/// OP_ENDIF
/// ```
///
/// # Arguments
/// * `secret_hash` - The Blake3 hash of the secret (32 bytes)
/// * `recipient_pubkey` - The recipient's public key (32 bytes for Schnorr)
/// * `sender_pubkey` - The sender's public key (32 bytes for Schnorr)
/// * `lock_time` - The minimum lock time required for sender to spend
///
/// # Returns
/// * `Ok(ScriptPublicKey)` - The constructed HTLC script public key
/// * `Err(ScriptBuilderError)` - If any parameter is invalid
///
/// # Example
/// ```
/// use spora_txscript::htlc_script;
/// use blake3::hash;
///
/// let secret = b"my_secret_key";
/// let secret_hash = hash(secret);
/// let recipient_pubkey = [0u8; 32]; // Replace with actual pubkey
/// let sender_pubkey = [0u8; 32]; // Replace with actual pubkey
/// let lock_time = 1756684800; // Unix timestamp
///
/// let script = htlc_script(secret_hash.as_bytes(), &recipient_pubkey, &sender_pubkey, lock_time).unwrap();
/// ```
pub fn htlc_script(
    secret_hash: &[u8],
    recipient_pubkey: &[u8],
    sender_pubkey: &[u8],
    lock_time: u64,
) -> ScriptBuilderResult<ScriptPublicKey> {
    // Validate input lengths
    if secret_hash.len() != 32 {
        return Err(ScriptBuilderError::ElementExceedsMaxSize(secret_hash.len()));
    }
    if recipient_pubkey.len() != 32 {
        return Err(ScriptBuilderError::ElementExceedsMaxSize(recipient_pubkey.len()));
    }
    if sender_pubkey.len() != 32 {
        return Err(ScriptBuilderError::ElementExceedsMaxSize(sender_pubkey.len()));
    }

    let script = ScriptBuilder::new()
        // HTLC script structure:
        // OP_IF
        //   OP_BLAKE3 <hash(secret)> OP_EQUALVERIFY
        //   <recipient_pubkey> OP_CHECKSIG
        // OP_ELSE
        //   <lock_time> OP_CHECKLOCKTIMEVERIFY OP_DROP
        //   <sender_pubkey> OP_CHECKSIG
        // OP_ENDIF
        .add_op(OpIf)?
        // Recipient path (with secret)
        .add_op(OpBlake3)?
        .add_data(secret_hash)?
        .add_op(OpEqualVerify)?
        .add_data(recipient_pubkey)?
        .add_op(OpCheckSig)?
        // Sender path (timeout)
        .add_op(OpElse)?
        .add_lock_time(lock_time)?
        .add_op(OpCheckLockTimeVerify)?
        .add_op(OpDrop)?
        .add_data(sender_pubkey)?
        .add_op(OpCheckSig)?
        .add_op(OpEndIf)?
        .drain();

    // Use Legacy version for HTLC scripts
    let version = SCRIPT_VER_CLASSIC;
    Ok(ScriptPublicKey::from_vec(version, script))
}

/// Creates a Hash Time Locked Contract (HTLC) script with ECDSA signatures.
///
/// Similar to `htlc_script` but uses ECDSA signature verification instead of Schnorr.
///
/// # Arguments
/// * `secret_hash` - The BLAKE3-256 hash of the secret (32 bytes)
/// * `recipient_pubkey` - The recipient's ECDSA public key (33 bytes)
/// * `sender_pubkey` - The sender's ECDSA public key (33 bytes)
/// * `lock_time` - The minimum lock time required for sender to spend
///
/// # Returns
/// * `Ok(ScriptPublicKey)` - The constructed HTLC script public key
/// * `Err(ScriptBuilderError)` - If any parameter is invalid
pub fn htlc_script_ecdsa(
    secret_hash: &[u8],
    recipient_pubkey: &[u8],
    sender_pubkey: &[u8],
    lock_time: u64,
) -> ScriptBuilderResult<ScriptPublicKey> {
    // Validate input lengths
    if secret_hash.len() != 32 {
        return Err(ScriptBuilderError::ElementExceedsMaxSize(secret_hash.len()));
    }
    if recipient_pubkey.len() != 33 {
        return Err(ScriptBuilderError::ElementExceedsMaxSize(recipient_pubkey.len()));
    }
    if sender_pubkey.len() != 33 {
        return Err(ScriptBuilderError::ElementExceedsMaxSize(sender_pubkey.len()));
    }

    let script = ScriptBuilder::new()
        // HTLC script structure with ECDSA:
        // OP_IF
        //   OP_BLAKE3 <hash(secret)> OP_EQUALVERIFY
        //   <recipient_pubkey> OP_CHECKSIGECDSA
        // OP_ELSE
        //   <lock_time> OP_CHECKLOCKTIMEVERIFY OP_DROP
        //   <sender_pubkey> OP_CHECKSIGECDSA
        // OP_ENDIF
        .add_op(OpIf)?
        // Recipient path (with secret)
        .add_op(OpBlake3)?
        .add_data(secret_hash)?
        .add_op(OpEqualVerify)?
        .add_data(recipient_pubkey)?
        .add_op(OpCheckSigECDSA)?
        // Sender path (timeout)
        .add_op(OpElse)?
        .add_lock_time(lock_time)?
        .add_op(OpCheckLockTimeVerify)?
        .add_op(OpDrop)?
        .add_data(sender_pubkey)?
        .add_op(OpCheckSigECDSA)?
        .add_op(OpEndIf)?
        .drain();

    // Use Legacy version for HTLC scripts
    let version = SCRIPT_VER_CLASSIC;
    Ok(ScriptPublicKey::from_vec(version, script))
}

/// Generates a signature script for spending an HTLC with the secret (recipient path).
///
/// # Arguments
/// * `redeem_script` - The HTLC redeem script
/// * `secret` - The secret preimage
/// * `signature` - The recipient's signature
///
/// # Returns
/// * `Ok(Vec<u8>)` - The signature script
/// * `Err(ScriptBuilderError)` - If any parameter is invalid
pub fn htlc_signature_script_with_secret(redeem_script: Vec<u8>, secret: Vec<u8>, signature: Vec<u8>) -> ScriptBuilderResult<Vec<u8>> {
    // Signature script structure: <signature> <secret> OP_TRUE <redeem_script>
    // OP_TRUE triggers the IF branch (recipient path)
    let script = ScriptBuilder::new().add_data(&signature)?.add_data(&secret)?.add_op(OpTrue)?.add_data(&redeem_script)?.drain();

    Ok(script)
}

/// Generates a signature script for spending an HTLC after lock time expires (sender path).
///
/// # Arguments
/// * `redeem_script` - The HTLC redeem script
/// * `signature` - The sender's signature
///
/// # Returns
/// * `Ok(Vec<u8>)` - The signature script
/// * `Err(ScriptBuilderError)` - If any parameter is invalid
pub fn htlc_signature_script_with_timeout(redeem_script: Vec<u8>, signature: Vec<u8>) -> ScriptBuilderResult<Vec<u8>> {
    // Signature script structure: <signature> OP_FALSE <redeem_script>
    // OP_FALSE triggers the ELSE branch (sender timeout path)
    let script = ScriptBuilder::new().add_data(&signature)?.add_op(OpFalse)?.add_data(&redeem_script)?.drain();

    Ok(script)
}

/// Takes a script and returns an equivalent pay-to-script-hash script
pub fn pay_to_script_hash_script(redeem_script: &[u8]) -> ScriptPublicKey {
    // Use Blake3 instead of Blake2b
    let redeem_script_hash = hash(redeem_script); // Blake3 default output is 32 bytes
    let script = pay_to_script_hash(redeem_script_hash.as_bytes());
    ScriptPublicKey::new(SCRIPT_VER_CLASSIC, script)
}

/// Generates a signature script that fits a pay-to-script-hash script
pub fn pay_to_script_hash_signature_script(redeem_script: &[u8], signature: Vec<u8>) -> ScriptBuilderResult<Vec<u8>> {
    let redeem_script_as_data = ScriptBuilder::new().add_data(redeem_script)?.drain();
    Ok(Vec::from_iter(signature.into_iter().chain(redeem_script_as_data.into_iter())))
}

/// Returns the address encoded in a script public key.
///
/// Notes:
///  - This function only works for 'standard' transaction script types.
///    Any data such as public keys which are invalid will return the
///    `TxScriptError::PubKeyFormat` error.
///
///  - In case a ScriptClass is needed by the caller, call `ScriptClass::from(address.version)`
///    or use `address.version` directly instead, where address is the successfully
///    returned address.
pub fn extract_script_pub_key_address(script_public_key: &ScriptPublicKey, prefix: Prefix) -> Result<Address, TxScriptError> {
    let class = ScriptClass::from(script_public_key);
    let script = script_public_key.script();

    // Version consistency check: script version must match expected version for the script class
    if script_public_key.version() != class.version() {
        return Err(TxScriptError::PubKeyFormat);
    }

    match class {
        ScriptClass::NonStandard => Err(TxScriptError::PubKeyFormat),
        ScriptClass::PubKey => Address::new(prefix, Version::PubKey, &script[1..33]).map_err(|_| TxScriptError::PubKeyFormat),
        ScriptClass::PubKeyECDSA => {
            Address::new(prefix, Version::PubKeyECDSA, &script[1..34]).map_err(|_| TxScriptError::PubKeyFormat)
        }
        ScriptClass::ScriptHash => Address::new(prefix, Version::ScriptHash, &script[2..34]).map_err(|_| TxScriptError::PubKeyFormat),
    }
}

pub mod test_helpers {
    use super::*;
    use crate::{opcodes::codes::OpTrue, MAX_TX_IN_SEQUENCE_NUM};
    use spora_consensus_core::{
        constants::TX_VERSION,
        subnets::SUBNETWORK_ID_NATIVE,
        tx::{Transaction, TransactionInput, TransactionOutpoint, TransactionOutput},
    };

    /// Returns a P2SH script paying to an anyone-can-spend address,
    /// The second return value is a redeemScript to be used with txscript.pay_to_script_hash_signature_script
    pub fn op_true_script() -> (ScriptPublicKey, Vec<u8>) {
        let redeem_script = vec![OpTrue];
        let script_public_key = pay_to_script_hash_script(&redeem_script);
        (script_public_key, redeem_script)
    }

    /// Creates a transaction that spends the first output of provided transaction.
    /// Assumes that the output being spent has opTrueScript as its scriptPublicKey.
    /// Creates the value of the spent output minus provided `fee` (in sau).
    pub fn create_transaction(tx_to_spend: &Transaction, fee: u64) -> Transaction {
        let (script_public_key, redeem_script) = op_true_script();
        let signature_script = pay_to_script_hash_signature_script(&redeem_script, vec![]).expect("the script is canonical");
        let previous_outpoint = TransactionOutpoint::new(tx_to_spend.id().as_bytes(), 0);
        let input = TransactionInput::new(previous_outpoint, signature_script, MAX_TX_IN_SEQUENCE_NUM, 1);
        let output = TransactionOutput::new(tx_to_spend.outputs[0].value - fee, script_public_key);
        Transaction::new(TX_VERSION, vec![input], vec![output], 0, SUBNETWORK_ID_NATIVE, 0, vec![])
    }

    /// Creates a transaction that spends the outputs of specified indexes (if they exist) of every provided transaction and returns an optional change.
    /// Assumes that the outputs being spent have opTrueScript as their scriptPublicKey.
    ///
    /// If some change is provided, creates two outputs, first one with the value of the spent outputs minus `change`
    /// and `fee` (in sau) and second one of `change` amount.
    ///
    /// If no change is provided, creates only one output with the value of the spent outputs minus and `fee` (in sau)
    pub fn create_transaction_with_change<'a>(
        txs_to_spend: impl Iterator<Item = &'a Transaction>,
        output_indexes: Vec<usize>,
        change: Option<u64>,
        fee: u64,
    ) -> Transaction {
        let (script_public_key, redeem_script) = op_true_script();
        let signature_script = pay_to_script_hash_signature_script(&redeem_script, vec![]).expect("the script is canonical");
        let mut inputs_value: u64 = 0;
        let mut inputs = vec![];
        for tx_to_spend in txs_to_spend {
            for i in output_indexes.iter().copied() {
                if i < tx_to_spend.outputs.len() {
                    let previous_outpoint = TransactionOutpoint::new(tx_to_spend.id().as_bytes(), i as u32);
                    inputs.push(TransactionInput::new(previous_outpoint, signature_script.clone(), MAX_TX_IN_SEQUENCE_NUM, 1));
                    inputs_value += tx_to_spend.outputs[i].value;
                }
            }
        }
        let outputs = match change {
            Some(change) => vec![
                TransactionOutput::new(inputs_value - fee - change, script_public_key.clone()),
                TransactionOutput::new(change, script_public_key),
            ],
            None => vec![TransactionOutput::new(inputs_value - fee, script_public_key.clone())],
        };
        Transaction::new(TX_VERSION, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_address_and_encode_script() {
        struct Test {
            name: &'static str,
            script_pub_key: ScriptPublicKey,
            prefix: Prefix,
            expect_success: bool,
        }

        // cspell:disable
        let tests = vec![
            Test {
                name: "Mainnet PubKey script",
                script_pub_key: ScriptPublicKey::new(
                    SCRIPT_VER_CLASSIC,
                    ScriptVec::from_slice(
                        &hex::decode("207bc04196f1125e4f2676cd09ed14afb77223b1f62177da5488346323eaa91a69ac").unwrap(),
                    ),
                ),
                prefix: Prefix::Mainnet,
                expect_success: true,
            },
            Test {
                name: "Testnet PubKeyECDSA script",
                script_pub_key: ScriptPublicKey::new(
                    SCRIPT_VER_CLASSIC,
                    ScriptVec::from_slice(
                        &hex::decode("21ba01fc5f4e9d9879599c69a3dafdb835a7255e5f2e934e9322ecd3af190ab0f60eab").unwrap(),
                    ),
                ),
                prefix: Prefix::Testnet,
                expect_success: true,
            },
            Test {
                name: "Testnet non standard script",
                script_pub_key: ScriptPublicKey::new(
                    SCRIPT_VER_CLASSIC,
                    ScriptVec::from_slice(
                        &hex::decode("2001fc5f4e9d9879599c69a3dafdb835a7255e5f2e934e9322ecd3af190ab0f60eab").unwrap(),
                    ),
                ),
                prefix: Prefix::Testnet,
                expect_success: false,
            },
            Test {
                name: "Mainnet script with unknown version",
                script_pub_key: ScriptPublicKey::new(
                    SCRIPT_VER_CLASSIC + 1,
                    ScriptVec::from_slice(
                        &hex::decode("207bc04196f1125e4f2676cd09ed14afb77223b1f62177da5488346323eaa91a69ac").unwrap(),
                    ),
                ),
                prefix: Prefix::Mainnet,
                expect_success: false,
            },
        ];
        // cspell:enable

        for test in tests {
            let extracted = extract_script_pub_key_address(&test.script_pub_key, test.prefix);
            match (test.expect_success, extracted) {
                (true, Ok(address)) => {
                    let encoded = pay_to_address_script(&address);
                    assert_eq!(encoded, test.script_pub_key, "re-encoded script mismatch for '{}'", test.name);
                }
                (false, Err(_)) => {
                    // Expected failure
                }
                (true, Err(e)) => panic!("Test '{}' failed unexpectedly: {:?}", test.name, e),
                (false, Ok(a)) => panic!("Test '{}' unexpectedly succeeded, got address: {}", test.name, a),
            }
        }
    }

    #[test]
    fn test_htlc_script_creation() {
        use blake3::hash;

        // Test data
        let secret = b"my_secret_key_12345";
        let secret_hash = hash(secret);
        let secret_hash_bytes = secret_hash.as_bytes(); // Use full 32 bytes for Blake3

        let recipient_pubkey = [0x01; 32]; // 32-byte Schnorr pubkey
        let sender_pubkey = [0x02; 32]; // 32-byte Schnorr pubkey
        let lock_time = 1756684800; // Unix timestamp

        // Test HTLC script creation
        let htlc_script = htlc_script(secret_hash_bytes, &recipient_pubkey, &sender_pubkey, lock_time);
        assert!(htlc_script.is_ok(), "HTLC script creation should succeed");

        let script_pubkey = htlc_script.unwrap();
        assert_eq!(script_pubkey.version(), SCRIPT_VER_CLASSIC, "HTLC script should use Legacy version");

        // Test ECDSA HTLC script creation
        let recipient_pubkey_ecdsa = [0x01; 33]; // 33-byte ECDSA pubkey
        let sender_pubkey_ecdsa = [0x02; 33]; // 33-byte ECDSA pubkey

        let htlc_script_ecdsa = htlc_script_ecdsa(secret_hash_bytes, &recipient_pubkey_ecdsa, &sender_pubkey_ecdsa, lock_time);
        assert!(htlc_script_ecdsa.is_ok(), "ECDSA HTLC script creation should succeed");
    }

    #[test]
    fn test_htlc_script_validation() {
        use blake3::hash;

        let secret = b"test_secret";
        let secret_hash = hash(secret);
        let secret_hash_bytes = secret_hash.as_bytes();

        let recipient_pubkey = [0x01; 32];
        let sender_pubkey = [0x02; 32];
        let lock_time = 1756684800;

        // Test with invalid secret hash length
        let invalid_secret_hash = &secret_hash.as_bytes()[..10]; // Too short
        let result = htlc_script(invalid_secret_hash, &recipient_pubkey, &sender_pubkey, lock_time);
        assert!(result.is_err(), "Should fail with invalid secret hash length");

        // Test with invalid recipient pubkey length
        let invalid_recipient_pubkey = [0x01; 20]; // Too short
        let result = htlc_script(secret_hash_bytes, &invalid_recipient_pubkey, &sender_pubkey, lock_time);
        assert!(result.is_err(), "Should fail with invalid recipient pubkey length");

        // Test with invalid sender pubkey length
        let invalid_sender_pubkey = [0x02; 20]; // Too short
        let result = htlc_script(secret_hash_bytes, &recipient_pubkey, &invalid_sender_pubkey, lock_time);
        assert!(result.is_err(), "Should fail with invalid sender pubkey length");
    }

    #[test]
    fn test_htlc_signature_scripts() {
        use blake3::hash;

        let secret = b"test_secret_for_signing";
        let secret_hash = hash(secret);
        let secret_hash_bytes = secret_hash.as_bytes();

        let recipient_pubkey = [0x01; 32];
        let sender_pubkey = [0x02; 32];
        let lock_time = 1756684800;

        // Create HTLC script
        let htlc_script_pubkey = htlc_script(secret_hash_bytes, &recipient_pubkey, &sender_pubkey, lock_time).unwrap();
        let redeem_script = htlc_script_pubkey.script().to_vec();

        // Test signature script with secret (recipient path)
        let signature = vec![0x01; 64]; // 64-byte Schnorr signature
        let secret_bytes = secret.to_vec();

        let sig_script_with_secret = htlc_signature_script_with_secret(redeem_script.clone(), secret_bytes, signature.clone());
        assert!(sig_script_with_secret.is_ok(), "Signature script with secret should succeed");

        let script_with_secret = sig_script_with_secret.unwrap();
        // The signature script structure is: <signature> <secret> OP_TRUE <redeem_script>
        // OP_TRUE should be at position: signature_len + secret_len
        let signature_len = 65; // 64 bytes + 1 byte length prefix
        let secret_len = secret.len() + 1; // secret bytes + 1 byte length prefix
        let op_true_position = signature_len + secret_len;
        assert_eq!(script_with_secret[op_true_position], OpTrue, "OP_TRUE should be at correct position to trigger IF branch");

        // Test signature script with timeout (sender path)
        let sig_script_with_timeout = htlc_signature_script_with_timeout(redeem_script, signature);
        assert!(sig_script_with_timeout.is_ok(), "Signature script with timeout should succeed");

        let script_with_timeout = sig_script_with_timeout.unwrap();
        // The signature script structure is: <signature> OP_FALSE <redeem_script>
        // OP_FALSE should be at position: signature_len
        let op_false_position = signature_len;
        assert_eq!(script_with_timeout[op_false_position], OpFalse, "OP_FALSE should be at correct position to trigger ELSE branch");
    }

    #[test]
    fn test_htlc_ecdsa_signature_scripts() {
        use blake3::hash;

        let secret = b"test_secret_for_ecdsa_signing";
        let secret_hash = hash(secret);
        let secret_hash_bytes = secret_hash.as_bytes();

        let recipient_pubkey = [0x01; 33]; // 33-byte ECDSA pubkey
        let sender_pubkey = [0x02; 33]; // 33-byte ECDSA pubkey
        let lock_time = 1756684800;

        // Create ECDSA HTLC script
        let htlc_script_pubkey = htlc_script_ecdsa(secret_hash_bytes, &recipient_pubkey, &sender_pubkey, lock_time).unwrap();
        let redeem_script = htlc_script_pubkey.script().to_vec();

        // Test signature script with secret (recipient path)
        let signature = vec![0x01; 72]; // ECDSA signature (DER format)
        let secret_bytes = secret.to_vec();

        let sig_script_with_secret = htlc_signature_script_with_secret(redeem_script.clone(), secret_bytes, signature.clone());
        assert!(sig_script_with_secret.is_ok(), "ECDSA signature script with secret should succeed");

        // Test signature script with timeout (sender path)
        let sig_script_with_timeout = htlc_signature_script_with_timeout(redeem_script, signature);
        assert!(sig_script_with_timeout.is_ok(), "ECDSA signature script with timeout should succeed");
    }

    #[test]
    fn test_htlc_script_structure() {
        use blake3::hash;

        let secret = b"structure_test_secret";
        let secret_hash = hash(secret);
        let secret_hash_bytes = secret_hash.as_bytes();

        let recipient_pubkey = [0x01; 32];
        let sender_pubkey = [0x02; 32];
        let lock_time = 1756684800;

        // Create HTLC script
        let htlc_script_pubkey = htlc_script(secret_hash_bytes, &recipient_pubkey, &sender_pubkey, lock_time).unwrap();
        let script = htlc_script_pubkey.script();

        // Verify script structure contains expected opcodes
        assert!(script.contains(&OpIf), "Script should contain OP_IF");
        assert!(script.contains(&OpBlake3), "Script should contain OP_BLAKE3");
        assert!(script.contains(&OpEqualVerify), "Script should contain OP_EQUALVERIFY");
        assert!(script.contains(&OpCheckSig), "Script should contain OP_CHECKSIG");
        assert!(script.contains(&OpElse), "Script should contain OP_ELSE");
        assert!(script.contains(&OpCheckLockTimeVerify), "Script should contain OP_CHECKLOCKTIMEVERIFY");
        assert!(script.contains(&OpDrop), "Script should contain OP_DROP");
        assert!(script.contains(&OpEndIf), "Script should contain OP_ENDIF");
    }

    #[test]
    fn test_htlc_security_validation() {
        use blake3::hash;

        let secret = b"security_test_secret";
        let secret_hash = hash(secret);
        let secret_hash_bytes = secret_hash.as_bytes();

        let recipient_pubkey = [0x01; 32];
        let sender_pubkey = [0x02; 32];
        let lock_time = 1756684800;

        // Test 1: Verify script structure prevents unauthorized access
        let htlc_script_pubkey = htlc_script(secret_hash_bytes, &recipient_pubkey, &sender_pubkey, lock_time).unwrap();
        let script = htlc_script_pubkey.script();

        // Verify that both paths require signatures
        // The script should have exactly 2 OP_CHECKSIG operations (one for each path)
        let checksig_count = script.iter().filter(|&&x| x == OpCheckSig).count();
        assert_eq!(checksig_count, 2, "HTLC script should have exactly 2 OP_CHECKSIG operations");

        // Test 2: Verify lock time is properly embedded
        // The script should contain the lock time value (trimmed little-endian)
        let lock_time_bytes = lock_time.to_le_bytes();
        let trimmed_size = 8 - lock_time_bytes.iter().rev().position(|x| *x != 0u8).unwrap_or(8);
        let trimmed_lock_time = &lock_time_bytes[0..trimmed_size];

        let mut found_lock_time = false;
        for window in script.windows(trimmed_lock_time.len()) {
            if window == trimmed_lock_time {
                found_lock_time = true;
                break;
            }
        }
        assert!(found_lock_time, "HTLC script should contain the lock time value");

        // Test 3: Verify secret hash is properly embedded
        let mut found_secret_hash = false;
        for window in script.windows(secret_hash_bytes.len()) {
            if window == secret_hash_bytes {
                found_secret_hash = true;
                break;
            }
        }
        assert!(found_secret_hash, "HTLC script should contain the secret hash");

        // Test 4: Verify public keys are properly embedded
        let mut found_recipient_pubkey = false;
        for window in script.windows(recipient_pubkey.len()) {
            if window == recipient_pubkey {
                found_recipient_pubkey = true;
                break;
            }
        }
        assert!(found_recipient_pubkey, "HTLC script should contain the recipient public key");

        let mut found_sender_pubkey = false;
        for window in script.windows(sender_pubkey.len()) {
            if window == sender_pubkey {
                found_sender_pubkey = true;
                break;
            }
        }
        assert!(found_sender_pubkey, "HTLC script should contain the sender public key");
    }

    #[test]
    fn test_htlc_edge_cases() {
        use blake3::hash;

        // Test with minimum lock time
        let secret = b"edge_case_secret";
        let secret_hash = hash(secret);
        let secret_hash_bytes = secret_hash.as_bytes();

        let recipient_pubkey = [0x01; 32];
        let sender_pubkey = [0x02; 32];

        // Test with lock time = 0
        let htlc_script_zero = htlc_script(secret_hash_bytes, &recipient_pubkey, &sender_pubkey, 0);
        assert!(htlc_script_zero.is_ok(), "HTLC should work with lock time = 0");

        // Test with maximum lock time
        let htlc_script_max = htlc_script(secret_hash_bytes, &recipient_pubkey, &sender_pubkey, u64::MAX);
        assert!(htlc_script_max.is_ok(), "HTLC should work with maximum lock time");

        // Test with identical recipient and sender pubkeys (edge case)
        let htlc_script_same = htlc_script(secret_hash_bytes, &recipient_pubkey, &recipient_pubkey, 1756684800);
        assert!(htlc_script_same.is_ok(), "HTLC should work with identical recipient and sender pubkeys");
    }

    #[test]
    fn test_htlc_signature_script_security() {
        use blake3::hash;

        let secret = b"signature_security_test";
        let secret_hash = hash(secret);
        let secret_hash_bytes = secret_hash.as_bytes();

        let recipient_pubkey = [0x01; 32];
        let sender_pubkey = [0x02; 32];
        let lock_time = 1756684800;

        // Create HTLC script
        let htlc_script_pubkey = htlc_script(secret_hash_bytes, &recipient_pubkey, &sender_pubkey, lock_time).unwrap();
        let redeem_script = htlc_script_pubkey.script().to_vec();

        // Test recipient path signature script
        let signature = vec![0x01; 64];
        let secret_bytes = secret.to_vec();

        let sig_script_recipient = htlc_signature_script_with_secret(redeem_script.clone(), secret_bytes, signature.clone()).unwrap();

        // Verify signature script structure
        // Should contain: <signature> <secret> OP_TRUE <redeem_script>
        assert!(sig_script_recipient.contains(&OpTrue), "Recipient signature script should contain OP_TRUE");

        // Test sender path signature script
        let sig_script_sender = htlc_signature_script_with_timeout(redeem_script, signature).unwrap();

        // Verify signature script structure
        // Should contain: <signature> OP_FALSE <redeem_script>
        assert!(sig_script_sender.contains(&OpFalse), "Sender signature script should contain OP_FALSE");

        // Verify that recipient and sender scripts are different
        assert_ne!(sig_script_recipient, sig_script_sender, "Recipient and sender signature scripts should be different");
    }

    #[test]
    fn test_htlc_security_edge_cases() {
        // Test with zero-length inputs (should fail)
        let empty_hash = [0u8; 0];
        let empty_pubkey = [0u8; 0];
        let result = htlc_script(&empty_hash, &empty_pubkey, &empty_pubkey, 0);
        assert!(result.is_err(), "HTLC should reject zero-length inputs");

        // Test with all-zero inputs (edge case but valid)
        let zero_hash = [0u8; 32];
        let zero_pubkey = [0u8; 32];
        let result = htlc_script(&zero_hash, &zero_pubkey, &zero_pubkey, 0);
        assert!(result.is_ok(), "HTLC should accept all-zero inputs");

        // Test with maximum values
        let max_hash = [0xFFu8; 32];
        let max_pubkey = [0xFFu8; 32];
        let result = htlc_script(&max_hash, &max_pubkey, &max_pubkey, u64::MAX);
        assert!(result.is_ok(), "HTLC should accept maximum values");

        // Test signature script with empty inputs
        let empty_redeem_script = vec![];
        let empty_signature = vec![];
        let empty_secret = vec![];

        let result = htlc_signature_script_with_secret(empty_redeem_script.clone(), empty_secret, empty_signature.clone());
        assert!(result.is_ok(), "Signature script should handle empty inputs gracefully");

        let result = htlc_signature_script_with_timeout(empty_redeem_script, empty_signature);
        assert!(result.is_ok(), "Timeout signature script should handle empty inputs gracefully");
    }
}
