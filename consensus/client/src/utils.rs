//!
//! Client-side utility functions and their WASM bindings.
//!

#![allow(non_snake_case)]

use crate::imports::*;
use crate::result::Result;
use crate::standard_script;
use js_sys::Reflect;
use spora_addresses::*;
use spora_consensus_core::{
    network::{NetworkType, NetworkTypeT},
    tx::ScriptPublicKeyT,
};
use spora_exec::{scripts::timelock as exec_timelock, ScriptRef as CellScriptRef};
use spora_utils::hex::ToHex;
use spora_wasm_core::types::{BinaryT, HexString};

#[wasm_bindgen(typescript_custom_section)]
const TS_TIMELOCK_TYPES: &'static str = r#"
/**
 * Serialized CKB-VM ScriptRef returned by timelock helpers.
 *
 * This is only the timelock verifier script. It does not enforce ownership by itself.
 */
interface ITimeLockScriptRef {
    codeHash: string;
    hashType: number;
    args: string;
    scriptHash: string;
}

/**
 * Decoded `since` value metadata.
 */
interface IDecodedSince {
    isRelative: boolean;
    isTimestamp: boolean;
    value: bigint;
}
"#;

fn script_ref_to_js_value(script: &CellScriptRef) -> JsValue {
    let obj = Object::new();
    Reflect::set(&obj, &"codeHash".into(), &JsValue::from_str(&script.code_hash.as_ref().to_hex())).unwrap();
    Reflect::set(&obj, &"hashType".into(), &JsValue::from_f64(script.hash_type as f64)).unwrap();
    Reflect::set(&obj, &"args".into(), &JsValue::from_str(&script.args.to_hex())).unwrap();
    Reflect::set(&obj, &"scriptHash".into(), &JsValue::from_str(&script.hash().as_ref().to_hex())).unwrap();
    obj.into()
}

fn decoded_since_to_js_value(is_relative: bool, is_timestamp: bool, value: u64) -> JsValue {
    let obj = Object::new();
    Reflect::set(&obj, &"isRelative".into(), &JsValue::from_bool(is_relative)).unwrap();
    Reflect::set(&obj, &"isTimestamp".into(), &JsValue::from_bool(is_timestamp)).unwrap();
    Reflect::set(&obj, &"value".into(), &JsValue::from(value)).unwrap();
    obj.into()
}

/// Creates a new script to pay a transaction output to the specified address.
/// @category Wallet SDK
#[wasm_bindgen(js_name = payToAddressScript)]
pub fn pay_to_address_script(address: &AddressT) -> Result<ScriptPublicKey> {
    let address = Address::try_cast_from(address)?;
    Ok(standard_script::pay_to_address_script(address.as_ref()))
}

/// Encodes an absolute timestamp `since` value for Cell inputs.
///
/// @param timestamp - Unix timestamp in seconds.
/// @category Wallet SDK
#[wasm_bindgen(js_name = encodeAbsoluteTimestampSince)]
pub fn encode_absolute_timestamp_since(timestamp: u64) -> u64 {
    exec_timelock::encode_absolute_timestamp_since(timestamp)
}

/// Encodes a relative DAA-score `since` value for Cell inputs.
///
/// @param delta - Number of DAA-score steps to wait.
/// @category Wallet SDK
#[wasm_bindgen(js_name = encodeRelativeDaaSince)]
pub fn encode_relative_daa_since(delta: u64) -> u64 {
    exec_timelock::encode_relative_daa_since(delta)
}

/// Encodes an absolute DAA-score `since` value for Cell inputs.
///
/// @param daaScore - Absolute DAA score target.
/// @category Wallet SDK
#[wasm_bindgen(js_name = encodeAbsoluteDaaSince)]
pub fn encode_absolute_daa_since(daa_score: u64) -> u64 {
    exec_timelock::encode_absolute_daa_since(daa_score)
}

/// Encodes a relative timestamp `since` value for Cell inputs.
///
/// @param deltaSeconds - Number of seconds to wait relative to confirmation.
/// @category Wallet SDK
#[wasm_bindgen(js_name = encodeRelativeTimestampSince)]
pub fn encode_relative_timestamp_since(delta_seconds: u64) -> u64 {
    exec_timelock::encode_relative_timestamp_since(delta_seconds)
}

/// Decodes a Cell-model `since` value into its flags and value.
///
/// @param since - Encoded `since` value.
/// @returns `IDecodedSince`
/// @category Wallet SDK
#[wasm_bindgen(js_name = decodeSince)]
pub fn decode_since(since: u64) -> JsValue {
    let (is_relative, is_timestamp, value) = exec_timelock::decode_since(since);
    decoded_since_to_js_value(is_relative, is_timestamp, value)
}

/// Creates an absolute timestamp timelock `ScriptRef`.
///
/// This is only the timelock verifier script. It does not enforce ownership by itself.
///
/// @param targetTimestamp - Unix timestamp in seconds.
/// @returns `ITimeLockScriptRef`
/// @category Wallet SDK
#[wasm_bindgen(js_name = absoluteTimestampLockScript)]
pub fn absolute_timestamp_lock_script(target_timestamp: u64) -> JsValue {
    script_ref_to_js_value(&exec_timelock::absolute_timestamp_lock(target_timestamp))
}

/// Creates a relative DAA-score timelock `ScriptRef`.
///
/// This is only the timelock verifier script. It does not enforce ownership by itself.
///
/// @param delta - Number of DAA-score steps to wait.
/// @returns `ITimeLockScriptRef`
/// @category Wallet SDK
#[wasm_bindgen(js_name = relativeDaaLockScript)]
pub fn relative_daa_lock_script(delta: u64) -> JsValue {
    script_ref_to_js_value(&exec_timelock::relative_daa_lock(delta))
}

/// Creates an absolute DAA-score timelock `ScriptRef`.
///
/// This is only the timelock verifier script. It does not enforce ownership by itself.
///
/// @param daaScore - Absolute DAA score target.
/// @returns `ITimeLockScriptRef`
/// @category Wallet SDK
#[wasm_bindgen(js_name = absoluteDaaLockScript)]
pub fn absolute_daa_lock_script(daa_score: u64) -> JsValue {
    script_ref_to_js_value(&exec_timelock::absolute_daa_lock(daa_score))
}

/// Creates a relative timestamp timelock `ScriptRef`.
///
/// This is only the timelock verifier script. It does not enforce ownership by itself.
///
/// @param deltaSeconds - Number of seconds to wait relative to confirmation.
/// @returns `ITimeLockScriptRef`
/// @category Wallet SDK
#[wasm_bindgen(js_name = relativeTimestampLockScript)]
pub fn relative_timestamp_lock_script(delta_seconds: u64) -> JsValue {
    script_ref_to_js_value(&exec_timelock::relative_timestamp_lock(delta_seconds))
}

/// Takes a script and returns an equivalent pay-to-script-hash script.
/// @param redeem_script - The redeem script ({@link HexString} or Uint8Array).
/// @category Wallet SDK
#[wasm_bindgen(js_name = payToScriptHashScript)]
pub fn pay_to_script_hash_script(redeem_script: BinaryT) -> Result<ScriptPublicKey> {
    let redeem_script = redeem_script.try_as_vec_u8()?;
    Ok(standard_script::pay_to_script_hash_script(redeem_script.as_slice()))
}

/// Generates a signature script that fits a pay-to-script-hash script.
/// @param redeem_script - The redeem script ({@link HexString} or Uint8Array).
/// @param signature - The signature ({@link HexString} or Uint8Array).
/// @category Wallet SDK
#[wasm_bindgen(js_name = payToScriptHashSignatureScript)]
pub fn pay_to_script_hash_signature_script(redeem_script: BinaryT, signature: BinaryT) -> Result<HexString> {
    let redeem_script = redeem_script.try_as_vec_u8()?;
    let signature = signature.try_as_vec_u8()?;
    let script = standard_script::pay_to_script_hash_signature_script(&redeem_script, signature)?;
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

    match standard_script::extract_script_pub_key_address(script_public_key.as_ref(), network_type.into()) {
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
    Ok(standard_script::is_pay_to_pubkey(script.as_slice()))
}

/// Returns returns true if the script passed is an ECDSA pay-to-pubkey.
/// @param script - The script ({@link HexString} or Uint8Array).
/// @category Wallet SDK
#[wasm_bindgen(js_name = isScriptPayToPubkeyECDSA)]
pub fn is_script_pay_to_pubkey_ecdsa(script: BinaryT) -> Result<bool> {
    let script = script.try_as_vec_u8()?;
    Ok(standard_script::is_pay_to_pubkey_ecdsa(script.as_slice()))
}

/// Returns true if the script passed is a pay-to-script-hash (P2SH) format, false otherwise.
/// @param script - The script ({@link HexString} or Uint8Array).
/// @category Wallet SDK
#[wasm_bindgen(js_name = isScriptPayToScriptHash)]
pub fn is_script_pay_to_script_hash(script: BinaryT) -> Result<bool> {
    let script = script.try_as_vec_u8()?;
    Ok(standard_script::is_pay_to_script_hash(script.as_slice()))
}
