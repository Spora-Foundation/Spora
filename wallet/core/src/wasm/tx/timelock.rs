// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// WASM bindings for time lock utilities

use crate::tx::timelock::TimelockConfig;
use js_sys::{Object, Reflect};
use spora_utils::hex::ToHex;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const TS_TIMELOCK_SCRIPT_REF: &'static str = r#"
/**
 * Serialized ScriptRef returned by timelock helpers.
 */
interface ITimeLockScriptRef {
    codeHash: string;
    hashType: number;
    args: string;
    scriptHash: string;
}
"#;

fn script_ref_to_js_value(script: &spora_exec::ScriptRef) -> JsValue {
    let obj = Object::new();
    Reflect::set(&obj, &"codeHash".into(), &JsValue::from_str(&script.code_hash.as_ref().to_hex())).unwrap();
    Reflect::set(&obj, &"hashType".into(), &JsValue::from_f64(script.hash_type as f64)).unwrap();
    Reflect::set(&obj, &"args".into(), &JsValue::from_str(&script.args.to_hex())).unwrap();
    Reflect::set(&obj, &"scriptHash".into(), &JsValue::from_str(&script.hash().as_ref().to_hex())).unwrap();
    obj.into()
}

/// Time lock configuration for WASM
///
/// This type provides a JavaScript-friendly interface for configuring
/// time locks in the Cell model.
#[wasm_bindgen]
#[derive(Debug, Clone, Copy)]
pub struct TimeLockConfig {
    inner: TimelockConfig,
}

#[wasm_bindgen]
impl TimeLockConfig {
    /// Create an absolute timestamp lock
    ///
    /// @param target - Unix timestamp (seconds since epoch)
    /// @returns TimeLockConfig instance
    #[wasm_bindgen(js_name = absoluteTimestamp)]
    pub fn absolute_timestamp(target: u64) -> Self {
        Self { inner: TimelockConfig::absolute_timestamp(target) }
    }

    /// Create a relative DAA score lock
    ///
    /// @param delta - Number of blocks to wait
    /// @returns TimeLockConfig instance
    #[wasm_bindgen(js_name = relativeDaa)]
    pub fn relative_daa(delta: u64) -> Self {
        Self { inner: TimelockConfig::relative_daa(delta) }
    }

    /// Create an absolute DAA score lock
    ///
    /// @param target - Target DAA score
    /// @returns TimeLockConfig instance
    #[wasm_bindgen(js_name = absoluteDaa)]
    pub fn absolute_daa(target: u64) -> Self {
        Self { inner: TimelockConfig::absolute_daa(target) }
    }

    /// Create a relative timestamp lock
    ///
    /// @param deltaSeconds - Number of seconds to wait
    /// @returns TimeLockConfig instance
    #[wasm_bindgen(js_name = relativeTimestamp)]
    pub fn relative_timestamp(delta_seconds: u64) -> Self {
        Self { inner: TimelockConfig::relative_timestamp(delta_seconds) }
    }

    /// Create a no-lock configuration
    ///
    /// @returns TimeLockConfig instance with no time lock
    #[wasm_bindgen(js_name = none)]
    pub fn none() -> Self {
        Self { inner: TimelockConfig::None }
    }

    /// Encode the since value for this configuration
    ///
    /// @returns The encoded `since` value to use in transaction inputs
    #[wasm_bindgen(js_name = encodeSince)]
    pub fn encode_since(&self) -> u64 {
        self.inner.encode_since()
    }

    /// Check if this configuration has a time lock
    ///
    /// @returns true if locked, false otherwise
    #[wasm_bindgen(js_name = isLocked)]
    pub fn is_locked(&self) -> bool {
        self.inner.is_locked()
    }

    /// Check if this configuration uses relative lock semantics.
    #[wasm_bindgen(js_name = isRelative)]
    pub fn is_relative(&self) -> bool {
        self.inner.is_relative()
    }

    /// Check if this configuration uses timestamp semantics.
    #[wasm_bindgen(js_name = isTimestamp)]
    pub fn is_timestamp(&self) -> bool {
        self.inner.is_timestamp()
    }

    /// Get the lock value (target or delta)
    ///
    /// @returns The lock value, or undefined if no lock
    #[wasm_bindgen(js_name = value)]
    pub fn value(&self) -> Option<u64> {
        self.inner.value()
    }

    /// Create the standalone timelock ScriptRef for this configuration.
    ///
    /// Returns an object with `codeHash`, `hashType`, `args`, and `scriptHash`
    /// fields, or `undefined` for `TimeLockConfig.none()`.
    ///
    /// This is only the timelock verifier script. It does not enforce ownership
    /// by itself, so it should not be used as the sole production lock for user funds.
    #[wasm_bindgen(js_name = createLockScript)]
    pub fn create_lock_script(&self) -> JsValue {
        self.inner.create_lock_script().as_ref().map(script_ref_to_js_value).unwrap_or(JsValue::UNDEFINED)
    }
}

impl From<TimeLockConfig> for TimelockConfig {
    fn from(config: TimeLockConfig) -> Self {
        config.inner
    }
}

impl From<TimelockConfig> for TimeLockConfig {
    fn from(inner: TimelockConfig) -> Self {
        Self { inner }
    }
}

/// Since encoding utilities for WASM
#[wasm_bindgen]
pub struct SinceEncoding;

#[wasm_bindgen]
impl SinceEncoding {
    /// Encode an absolute timestamp since value
    ///
    /// @param timestamp - Unix timestamp
    /// @returns Encoded since value
    #[wasm_bindgen(js_name = absoluteTimestamp)]
    pub fn absolute_timestamp(timestamp: u64) -> u64 {
        crate::tx::timelock::since_encoding::encode_absolute_timestamp_since(timestamp)
    }

    /// Encode a relative DAA score since value
    ///
    /// @param delta - Number of blocks
    /// @returns Encoded since value
    #[wasm_bindgen(js_name = relativeDaa)]
    pub fn relative_daa(delta: u64) -> u64 {
        crate::tx::timelock::since_encoding::encode_relative_daa_since(delta)
    }

    /// Encode an absolute DAA score since value
    ///
    /// @param daaScore - Target DAA score
    /// @returns Encoded since value
    #[wasm_bindgen(js_name = absoluteDaa)]
    pub fn absolute_daa(daa_score: u64) -> u64 {
        crate::tx::timelock::since_encoding::encode_absolute_daa_since(daa_score)
    }

    /// Encode a relative timestamp since value
    ///
    /// @param deltaSeconds - Number of seconds
    /// @returns Encoded since value
    #[wasm_bindgen(js_name = relativeTimestamp)]
    pub fn relative_timestamp(delta_seconds: u64) -> u64 {
        crate::tx::timelock::since_encoding::encode_relative_timestamp_since(delta_seconds)
    }

    /// Decode a since value
    ///
    /// @param since - The since value to decode
    /// @returns Object with isRelative, isTimestamp, and value properties
    #[wasm_bindgen(js_name = decode)]
    pub fn decode(since: u64) -> JsValue {
        let (is_relative, is_timestamp, value) = crate::tx::timelock::since_encoding::decode_since(since);

        let obj = js_sys::Object::new();
        js_sys::Reflect::set(&obj, &"isRelative".into(), &is_relative.into()).unwrap();
        js_sys::Reflect::set(&obj, &"isTimestamp".into(), &is_timestamp.into()).unwrap();
        js_sys::Reflect::set(&obj, &"value".into(), &value.into()).unwrap();

        obj.into()
    }
}

/// Since flags constants
#[wasm_bindgen]
pub struct SinceFlags;

#[wasm_bindgen]
impl SinceFlags {
    /// Relative lock flag (bit 63)
    #[wasm_bindgen(getter)]
    pub fn relative() -> u64 {
        crate::tx::timelock::since_encoding::since_flags::RELATIVE
    }

    /// Timestamp flag (bit 62)
    #[wasm_bindgen(getter)]
    pub fn timestamp() -> u64 {
        crate::tx::timelock::since_encoding::since_flags::TIMESTAMP
    }

    /// Value mask (bits 0-55)
    #[wasm_bindgen(getter)]
    pub fn value_mask() -> u64 {
        crate::tx::timelock::since_encoding::since_flags::VALUE_MASK
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_timelock_config() {
        let config = TimeLockConfig::absolute_timestamp(1735689600);
        assert!(config.is_locked());
        assert!(!config.is_relative());
        assert!(config.is_timestamp());
        assert_eq!(config.value(), Some(1735689600));

        let since = config.encode_since();
        assert!(since > 0);
    }

    #[test]
    fn test_wasm_since_encoding() {
        let timestamp = 1735689600u64;
        let since = SinceEncoding::absolute_timestamp(timestamp);

        // Check flags
        assert_eq!(since & 0x4000000000000000, 0x4000000000000000); // Timestamp flag
        assert_eq!(since & 0x8000000000000000, 0); // Not relative
    }
}
