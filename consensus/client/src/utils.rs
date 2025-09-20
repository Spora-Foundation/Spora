//!
//! Client-side utility functions and their WASM bindings.
//!

#![allow(non_snake_case)]

use crate::imports::*;
use crate::result::Result;
use tondi_addresses::*;
use tondi_consensus_core::{
    network::{NetworkType, NetworkTypeT},
    tx::ScriptPublicKeyT,
};
use tondi_txscript::{script_class::ScriptClass, standard};
use tondi_utils::hex::ToHex;
use tondi_wasm_core::types::{BinaryT, HexString};

/// Creates a new script to pay a transaction output to the specified address.
/// @category Wallet SDK
#[wasm_bindgen(js_name = payToAddressScript)]
pub fn pay_to_address_script(address: &AddressT) -> Result<ScriptPublicKey> {
    let address = Address::try_cast_from(address)?;
    Ok(standard::pay_to_address_script(address.as_ref()))
}

/// Creates a new script to pay a transaction output to the specified address with lock time.
/// @category Wallet SDK
#[wasm_bindgen(js_name = payToAddressWithLockTimeScript)]
pub fn pay_to_address_with_lock_time_script(address: &AddressT, lock_time: u64) -> Result<ScriptPublicKey> {
    let address = Address::try_cast_from(address)?;
    Ok(standard::pay_to_address_with_lock_time_script(address.as_ref(), lock_time)?)
}

/// Takes a script and returns an equivalent pay-to-script-hash script.
/// @param redeem_script - The redeem script ({@link HexString} or Uint8Array).
/// @category Wallet SDK
#[wasm_bindgen(js_name = payToScriptHashScript)]
pub fn pay_to_script_hash_script(redeem_script: BinaryT) -> Result<ScriptPublicKey> {
    let redeem_script = redeem_script.try_as_vec_u8()?;
    Ok(standard::pay_to_script_hash_script(redeem_script.as_slice()))
}

/// Generates a signature script that fits a pay-to-script-hash script.
/// @param redeem_script - The redeem script ({@link HexString} or Uint8Array).
/// @param signature - The signature ({@link HexString} or Uint8Array).
/// @category Wallet SDK
#[wasm_bindgen(js_name = payToScriptHashSignatureScript)]
pub fn pay_to_script_hash_signature_script(redeem_script: BinaryT, signature: BinaryT) -> Result<HexString> {
    let redeem_script = redeem_script.try_as_vec_u8()?;
    let signature = signature.try_as_vec_u8()?;
    let script = standard::pay_to_script_hash_signature_script(&redeem_script, signature)?;
    Ok(script.to_hex().into())
}

/// Returns the address encoded in a script public key.
/// @param script_public_key - The script public key ({@link ScriptPublicKey}).
/// @param network - The network type.
/// @category Wallet SDK
#[wasm_bindgen(js_name = addressFromScriptPublicKey)]
pub fn address_from_script_public_key(script_public_key: &ScriptPublicKeyT, network: &NetworkTypeT) -> Result<AddressOrUndefinedT> {
    let script_public_key = ScriptPublicKey::try_cast_from(script_public_key)?;
    let network_type = NetworkType::try_from(network)?;

    match standard::extract_script_pub_key_address(script_public_key.as_ref(), network_type.into()) {
        Ok(address) => Ok(AddressOrUndefinedT::from(JsValue::from(address))),
        Err(_) => Ok(AddressOrUndefinedT::from(JsValue::UNDEFINED)),
    }
}

/// Returns true if the script passed is a pay-to-pubkey.
/// @param script - The script ({@link HexString} or Uint8Array).
/// @category Wallet SDK
#[wasm_bindgen(js_name = isScriptPayToPubkey)]
pub fn is_script_pay_to_pubkey(script: BinaryT) -> Result<bool> {
    let script = script.try_as_vec_u8()?;
    Ok(ScriptClass::is_pay_to_pubkey(script.as_slice()))
}

/// Returns returns true if the script passed is an ECDSA pay-to-pubkey.
/// @param script - The script ({@link HexString} or Uint8Array).
/// @category Wallet SDK
#[wasm_bindgen(js_name = isScriptPayToPubkeyECDSA)]
pub fn is_script_pay_to_pubkey_ecdsa(script: BinaryT) -> Result<bool> {
    let script = script.try_as_vec_u8()?;
    Ok(ScriptClass::is_pay_to_pubkey_ecdsa(script.as_slice()))
}

/// Returns true if the script passed is a pay-to-script-hash (P2SH) format, false otherwise.
/// @param script - The script ({@link HexString} or Uint8Array).
/// @category Wallet SDK
#[wasm_bindgen(js_name = isScriptPayToScriptHash)]
pub fn is_script_pay_to_script_hash(script: BinaryT) -> Result<bool> {
    let script = script.try_as_vec_u8()?;
    Ok(ScriptClass::is_pay_to_script_hash(script.as_slice()))
}

/// Creates a Hash Time Locked Contract (HTLC) script.
///
/// This function creates an HTLC script that allows spending in two ways:
/// 1. With the correct preimage (secret) and a valid signature from the recipient
/// 2. With a valid signature from the sender after the lock time expires
///
/// @param secret_hash - The hash160 of the secret (20 bytes)
/// @param recipient_pubkey - The recipient's public key (32 bytes for Schnorr)
/// @param sender_pubkey - The sender's public key (32 bytes for Schnorr)
/// @param lock_time - The minimum lock time required for sender to spend
/// @category Wallet SDK
#[wasm_bindgen(js_name = htlcScript)]
pub fn htlc_script(
    secret_hash: BinaryT,
    recipient_pubkey: BinaryT,
    sender_pubkey: BinaryT,
    lock_time: u64,
) -> Result<ScriptPublicKey> {
    let secret_hash = secret_hash.try_as_vec_u8()?;
    let recipient_pubkey = recipient_pubkey.try_as_vec_u8()?;
    let sender_pubkey = sender_pubkey.try_as_vec_u8()?;

    Ok(standard::htlc_script(secret_hash.as_slice(), recipient_pubkey.as_slice(), sender_pubkey.as_slice(), lock_time)?)
}

/// Creates a Hash Time Locked Contract (HTLC) script with ECDSA signatures.
///
/// Similar to htlcScript but uses ECDSA signature verification instead of Schnorr.
///
/// @param secret_hash - The hash160 of the secret (20 bytes)
/// @param recipient_pubkey - The recipient's ECDSA public key (33 bytes)
/// @param sender_pubkey - The sender's ECDSA public key (33 bytes)
/// @param lock_time - The minimum lock time required for sender to spend
/// @category Wallet SDK
#[wasm_bindgen(js_name = htlcScriptECDSA)]
pub fn htlc_script_ecdsa(
    secret_hash: BinaryT,
    recipient_pubkey: BinaryT,
    sender_pubkey: BinaryT,
    lock_time: u64,
) -> Result<ScriptPublicKey> {
    let secret_hash = secret_hash.try_as_vec_u8()?;
    let recipient_pubkey = recipient_pubkey.try_as_vec_u8()?;
    let sender_pubkey = sender_pubkey.try_as_vec_u8()?;

    Ok(standard::htlc_script_ecdsa(secret_hash.as_slice(), recipient_pubkey.as_slice(), sender_pubkey.as_slice(), lock_time)?)
}

/// Generates a signature script for spending an HTLC with the secret (recipient path).
///
/// @param redeem_script - The HTLC redeem script
/// @param secret - The secret preimage
/// @param signature - The recipient's signature
/// @category Wallet SDK
#[wasm_bindgen(js_name = htlcSignatureScriptWithSecret)]
pub fn htlc_signature_script_with_secret(redeem_script: BinaryT, secret: BinaryT, signature: BinaryT) -> Result<HexString> {
    let redeem_script = redeem_script.try_as_vec_u8()?;
    let secret = secret.try_as_vec_u8()?;
    let signature = signature.try_as_vec_u8()?;

    let script = standard::htlc_signature_script_with_secret(redeem_script, secret, signature)?;
    Ok(script.to_hex().into())
}

/// Generates a signature script for spending an HTLC after lock time expires (sender path).
///
/// @param redeem_script - The HTLC redeem script
/// @param signature - The sender's signature
/// @category Wallet SDK
#[wasm_bindgen(js_name = htlcSignatureScriptWithTimeout)]
pub fn htlc_signature_script_with_timeout(redeem_script: BinaryT, signature: BinaryT) -> Result<HexString> {
    let redeem_script = redeem_script.try_as_vec_u8()?;
    let signature = signature.try_as_vec_u8()?;

    let script = standard::htlc_signature_script_with_timeout(redeem_script, signature)?;
    Ok(script.to_hex().into())
}
