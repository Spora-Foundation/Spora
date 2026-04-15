//!
//! Client-side utility functions and their WASM bindings.
//!

#![allow(non_snake_case)]

use crate::imports::*;
use crate::result::Result;
use crate::standard_script;
use js_sys::Reflect;
use spora_addresses::*;
use spora_consensus_core::network::{NetworkType, NetworkTypeT};
use spora_exec::{scripts::timelock as exec_timelock, Script as CellScript};
use spora_utils::hex::ToHex;

#[wasm_bindgen(typescript_custom_section)]
const TS_TIMELOCK_TYPES: &'static str = r#"
/**
 * Serialized CKB-VM Script returned by timelock helpers.
 *
 * This is only the timelock verifier script. It does not enforce ownership by itself.
 */
interface ITimeLockScript {
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

fn script_ref_to_js_value(script: &CellScript) -> JsValue {
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

/// Creates a new canonical lock script for the specified address.
/// @category Wallet SDK
#[wasm_bindgen(js_name = payToAddressLockScript)]
pub fn pay_to_address_lock_script(address: &AddressT) -> Result<JsValue> {
    let address = Address::try_cast_from(address)?;
    Ok(workflow_wasm::serde::to_value(&standard_script::pay_to_address_lock_script(address.as_ref()))?)
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

/// Creates an absolute timestamp timelock `Script`.
///
/// This is only the timelock verifier script. It does not enforce ownership by itself.
///
/// @param targetTimestamp - Unix timestamp in seconds.
/// @returns `ITimeLockScript`
/// @category Wallet SDK
#[wasm_bindgen(js_name = absoluteTimestampLockScript)]
pub fn absolute_timestamp_lock_script(target_timestamp: u64) -> JsValue {
    script_ref_to_js_value(&exec_timelock::absolute_timestamp_lock(target_timestamp))
}

/// Creates a relative DAA-score timelock `Script`.
///
/// This is only the timelock verifier script. It does not enforce ownership by itself.
///
/// @param delta - Number of DAA-score steps to wait.
/// @returns `ITimeLockScript`
/// @category Wallet SDK
#[wasm_bindgen(js_name = relativeDaaLockScript)]
pub fn relative_daa_lock_script(delta: u64) -> JsValue {
    script_ref_to_js_value(&exec_timelock::relative_daa_lock(delta))
}

/// Creates an absolute DAA-score timelock `Script`.
///
/// This is only the timelock verifier script. It does not enforce ownership by itself.
///
/// @param daaScore - Absolute DAA score target.
/// @returns `ITimeLockScript`
/// @category Wallet SDK
#[wasm_bindgen(js_name = absoluteDaaLockScript)]
pub fn absolute_daa_lock_script(daa_score: u64) -> JsValue {
    script_ref_to_js_value(&exec_timelock::absolute_daa_lock(daa_score))
}

/// Creates a relative timestamp timelock `Script`.
///
/// This is only the timelock verifier script. It does not enforce ownership by itself.
///
/// @param deltaSeconds - Number of seconds to wait relative to confirmation.
/// @returns `ITimeLockScript`
/// @category Wallet SDK
#[wasm_bindgen(js_name = relativeTimestampLockScript)]
pub fn relative_timestamp_lock_script(delta_seconds: u64) -> JsValue {
    script_ref_to_js_value(&exec_timelock::relative_timestamp_lock(delta_seconds))
}

/// Returns the address encoded in a canonical Cell lock script.
/// @param lock_script - The lock script ({@link Script}, {@link HexString} or Uint8Array).
/// @param network - The network type.
/// @category Wallet SDK
#[wasm_bindgen(js_name = addressFromLockScript)]
pub fn address_from_lock_script(lock_script: JsValue, network: &NetworkTypeT) -> Result<AddressOrUndefinedT> {
    let network_type = NetworkType::try_from(network)?;
    let script = workflow_wasm::serde::from_value::<CellScript>(lock_script)
        .map_err(|_| Error::custom("addressFromLockScript now only accepts canonical Script objects".to_string()))?;
    let address = standard_script::extract_address_from_script(&script, network_type.into());

    match address {
        Ok(address) => Ok(AddressOrUndefinedT::from(JsValue::from(address))),
        Err(_) => Ok(AddressOrUndefinedT::from(JsValue::UNDEFINED)),
    }
}
