// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// WASM bindings for HTLC utilities

use crate::tx::htlc::{HtlcConfig as NativeHtlcConfig, HtlcLockType as NativeHtlcLockType, HtlcWitness as NativeHtlcWitness};
use wasm_bindgen::prelude::*;

/// HTLC lock type for WASM
#[wasm_bindgen]
#[derive(Debug, Clone, Copy)]
pub struct HtlcLockType {
    inner: NativeHtlcLockType,
}

#[wasm_bindgen]
impl HtlcLockType {
    /// Create an absolute DAA score lock
    #[wasm_bindgen(js_name = absoluteDaa)]
    pub fn absolute_daa(target: u64) -> Self {
        Self { inner: NativeHtlcLockType::AbsoluteDaa { target } }
    }

    /// Create an absolute timestamp lock
    #[wasm_bindgen(js_name = absoluteTimestamp)]
    pub fn absolute_timestamp(target: u64) -> Self {
        Self { inner: NativeHtlcLockType::AbsoluteTimestamp { target } }
    }

    /// Create a relative DAA score lock
    #[wasm_bindgen(js_name = relativeDaa)]
    pub fn relative_daa(delta: u64) -> Self {
        Self { inner: NativeHtlcLockType::RelativeDaa { delta } }
    }

    /// Create a relative timestamp lock
    #[wasm_bindgen(js_name = relativeTimestamp)]
    pub fn relative_timestamp(delta_seconds: u64) -> Self {
        Self { inner: NativeHtlcLockType::RelativeTimestamp { delta_seconds } }
    }

    /// Get the lock type as u8 (0-3)
    #[wasm_bindgen(js_name = typeValue)]
    pub fn type_value(&self) -> u8 {
        self.inner.to_u8()
    }

    /// Get the lock value (target or delta)
    #[wasm_bindgen(js_name = lockValue)]
    pub fn lock_value(&self) -> u64 {
        self.inner.value()
    }
}

impl From<HtlcLockType> for NativeHtlcLockType {
    fn from(lock_type: HtlcLockType) -> Self {
        lock_type.inner
    }
}

/// HTLC configuration for WASM
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct HtlcConfig {
    inner: NativeHtlcConfig,
}

#[wasm_bindgen]
impl HtlcConfig {
    /// Create a new HTLC configuration
    ///
    /// @param secretHash - 32-byte array (Uint8Array) - blake3 hash of the secret
    /// @param recipientPubkey - 32-byte array (Uint8Array) - recipient's public key
    /// @param senderPubkey - 32-byte array (Uint8Array) - sender's public key
    /// @param lockType - HtlcLockType instance
    #[wasm_bindgen(constructor)]
    pub fn new(
        secret_hash: &[u8],
        recipient_pubkey: &[u8],
        sender_pubkey: &[u8],
        lock_type: &HtlcLockType,
    ) -> Result<HtlcConfig, JsValue> {
        let secret_hash: [u8; 32] = secret_hash.try_into().map_err(|_| JsValue::from_str("secret_hash must be 32 bytes"))?;
        let recipient_pubkey: [u8; 32] =
            recipient_pubkey.try_into().map_err(|_| JsValue::from_str("recipient_pubkey must be 32 bytes"))?;
        let sender_pubkey: [u8; 32] = sender_pubkey.try_into().map_err(|_| JsValue::from_str("sender_pubkey must be 32 bytes"))?;

        Ok(Self { inner: NativeHtlcConfig::new(secret_hash, recipient_pubkey, sender_pubkey, lock_type.inner) })
    }

    /// Build script args for the HTLC script
    ///
    /// @returns Uint8Array - 105 bytes of script args
    #[wasm_bindgen(js_name = buildScriptArgs)]
    pub fn build_script_args(&self) -> Vec<u8> {
        self.inner.build_script_args()
    }

    /// Encode the since value for spending this HTLC
    ///
    /// @returns u64 - encoded since value
    #[wasm_bindgen(js_name = encodeSince)]
    pub fn encode_since(&self) -> u64 {
        self.inner.encode_since()
    }

    /// Get the secret hash
    #[wasm_bindgen(getter)]
    pub fn secret_hash(&self) -> Vec<u8> {
        self.inner.secret_hash.to_vec()
    }

    /// Get the recipient public key
    #[wasm_bindgen(getter, js_name = recipientPubkey)]
    pub fn recipient_pubkey(&self) -> Vec<u8> {
        self.inner.recipient_pubkey.to_vec()
    }

    /// Get the sender public key
    #[wasm_bindgen(getter, js_name = senderPubkey)]
    pub fn sender_pubkey(&self) -> Vec<u8> {
        self.inner.sender_pubkey.to_vec()
    }
}

/// HTLC witness builder for WASM
#[wasm_bindgen]
pub struct HtlcWitness;

#[wasm_bindgen]
impl HtlcWitness {
    /// Build witness for recipient path (secret + signature)
    ///
    /// @param signature - 64-byte array (Uint8Array)
    /// @param secret - 32-byte array (Uint8Array)
    /// @returns Uint8Array - 97 bytes
    #[wasm_bindgen(js_name = recipient)]
    pub fn recipient(signature: &[u8], secret: &[u8]) -> Result<Vec<u8>, JsValue> {
        let signature: [u8; 64] = signature.try_into().map_err(|_| JsValue::from_str("signature must be 64 bytes"))?;
        let secret: [u8; 32] = secret.try_into().map_err(|_| JsValue::from_str("secret must be 32 bytes"))?;
        Ok(NativeHtlcWitness::recipient(signature, secret))
    }

    /// Build witness for sender timeout path (signature only)
    ///
    /// @param signature - 64-byte array (Uint8Array)
    /// @returns Uint8Array - 65 bytes
    #[wasm_bindgen(js_name = senderTimeout)]
    pub fn sender_timeout(signature: &[u8]) -> Result<Vec<u8>, JsValue> {
        let signature: [u8; 64] = signature.try_into().map_err(|_| JsValue::from_str("signature must be 64 bytes"))?;
        Ok(NativeHtlcWitness::sender_timeout(signature))
    }
}

/// Compute blake3 hash of a secret
///
/// @param secret - Uint8Array - the secret data
/// @returns Uint8Array - 32-byte hash
#[wasm_bindgen(js_name = computeSecretHash)]
pub fn compute_secret_hash(secret: &[u8]) -> Vec<u8> {
    crate::tx::htlc::compute_secret_hash(secret).to_vec()
}

/// Verify that a secret matches the given hash
///
/// @param secret - Uint8Array - the secret data
/// @param expectedHash - Uint8Array - 32-byte expected hash
/// @returns boolean
#[wasm_bindgen(js_name = verifySecret)]
pub fn verify_secret(secret: &[u8], expected_hash: &[u8]) -> bool {
    if expected_hash.len() != 32 {
        return false;
    }
    let expected_hash: [u8; 32] = expected_hash.try_into().unwrap();
    crate::tx::htlc::verify_secret(secret, &expected_hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_htlc_lock_type() {
        let lock = HtlcLockType::absolute_timestamp(1735689600);
        assert_eq!(lock.type_value(), 1);
        assert_eq!(lock.lock_value(), 1735689600);
    }

    #[test]
    fn test_wasm_htlc_config() {
        let secret_hash = [0xABu8; 32];
        let recipient_pubkey = [0x11u8; 32];
        let sender_pubkey = [0x22u8; 32];
        let lock_type = HtlcLockType::absolute_timestamp(1735689600);

        let config = HtlcConfig::new(&secret_hash, &recipient_pubkey, &sender_pubkey, &lock_type).unwrap();

        let args = config.build_script_args();
        assert_eq!(args.len(), 105);

        let since = config.encode_since();
        assert!(since > 0);
    }

    #[test]
    fn test_wasm_htlc_witness() {
        let signature = [0x33u8; 64];
        let secret = [0x44u8; 32];

        let witness = HtlcWitness::recipient(&signature, &secret).unwrap();
        assert_eq!(witness.len(), 97);

        let witness = HtlcWitness::sender_timeout(&signature).unwrap();
        assert_eq!(witness.len(), 65);
    }

    #[test]
    fn test_wasm_compute_secret_hash() {
        let secret = b"my secret";
        let hash = compute_secret_hash(secret);
        assert_eq!(hash.len(), 32);

        // Verify
        assert!(verify_secret(secret, &hash));
        assert!(!verify_secret(b"wrong", &hash));
    }
}
