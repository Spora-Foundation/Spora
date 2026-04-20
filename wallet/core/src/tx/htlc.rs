// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Spora developers
//
// HTLC (Hash Time Locked Contract) support for wallet transactions
//
// This module provides utilities for creating and spending HTLC outputs
// using the Cell model and CKB-VM scripts.

//! HTLC utilities for wallet transactions
//!
//! This module provides types and functions for creating HTLC contracts
//! in the Cell model. HTLC supports two spending paths:
//! 1. Recipient path: secret preimage + signature
//! 2. Sender timeout path: signature after timelock expires
//!
//! # Example
//!
//! ```rust,ignore
//! use spora_wallet_core::tx::htlc::{HtlcConfig, HtlcLockType};
//! use spora_exec::scripts::timelock;
//!
//! // Configure an HTLC with absolute timestamp lock
//! let config = HtlcConfig {
//!     secret_hash: [0xAB; 32],
//!     recipient_pubkey: [0x11; 32],
//!     sender_pubkey: [0x22; 32],
//!     lock: HtlcLockType::AbsoluteTimestamp { target: 1735689600 },
//! };
//!
//! // Create the HTLC script args
//! let args = config.build_script_args();
//! ```

use crate::tx::timelock::TimelockConfig;
use spora_exec::celltx::Script;
use spora_exec::scripts::htlc_code_hash;

/// HTLC lock type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HtlcLockType {
    /// Absolute DAA score lock
    AbsoluteDaa { target: u64 },
    /// Absolute timestamp lock
    AbsoluteTimestamp { target: u64 },
    /// Relative DAA score lock
    RelativeDaa { delta: u64 },
    /// Relative timestamp lock
    RelativeTimestamp { delta_seconds: u64 },
}

impl HtlcLockType {
    /// Convert to lock type byte
    pub fn to_u8(self) -> u8 {
        match self {
            Self::AbsoluteDaa { .. } => 0,
            Self::AbsoluteTimestamp { .. } => 1,
            Self::RelativeDaa { .. } => 2,
            Self::RelativeTimestamp { .. } => 3,
        }
    }

    /// Get the lock value
    pub fn value(&self) -> u64 {
        match *self {
            Self::AbsoluteDaa { target } => target,
            Self::AbsoluteTimestamp { target } => target,
            Self::RelativeDaa { delta } => delta,
            Self::RelativeTimestamp { delta_seconds } => delta_seconds,
        }
    }

    /// Convert to TimelockConfig for since encoding
    pub fn to_timelock_config(self) -> TimelockConfig {
        match self {
            Self::AbsoluteDaa { target } => TimelockConfig::absolute_daa(target),
            Self::AbsoluteTimestamp { target } => TimelockConfig::absolute_timestamp(target),
            Self::RelativeDaa { delta } => TimelockConfig::relative_daa(delta),
            Self::RelativeTimestamp { delta_seconds } => TimelockConfig::relative_timestamp(delta_seconds),
        }
    }
}

/// HTLC configuration
#[derive(Debug, Clone)]
pub struct HtlcConfig {
    /// Blake3 hash of the secret (32 bytes)
    pub secret_hash: [u8; 32],
    /// Recipient's public key (32 bytes for Schnorr)
    pub recipient_pubkey: [u8; 32],
    /// Sender's public key (32 bytes for Schnorr)
    pub sender_pubkey: [u8; 32],
    /// Lock type and value
    pub lock: HtlcLockType,
}

impl HtlcConfig {
    /// Create a new HTLC configuration
    pub fn new(secret_hash: [u8; 32], recipient_pubkey: [u8; 32], sender_pubkey: [u8; 32], lock: HtlcLockType) -> Self {
        Self { secret_hash, recipient_pubkey, sender_pubkey, lock }
    }

    /// Build script args for the HTLC script
    ///
    /// Format (105 bytes):
    /// - [0..32]: secret_hash
    /// - [32..64]: recipient_pubkey
    /// - [64..96]: sender_pubkey
    /// - [96]: lock_type (0-3)
    /// - [97..105]: lock_value (u64, little-endian)
    pub fn build_script_args(&self) -> Vec<u8> {
        let mut args = Vec::with_capacity(105);
        args.extend_from_slice(&self.secret_hash);
        args.extend_from_slice(&self.recipient_pubkey);
        args.extend_from_slice(&self.sender_pubkey);
        args.push(self.lock.to_u8());
        args.extend_from_slice(&self.lock.value().to_le_bytes());
        args
    }

    /// Create a Script for this HTLC configuration
    pub fn create_script_ref(&self) -> Script {
        let args = self.build_script_args();
        Script::new(htlc_code_hash(), 0, args)
    }

    /// Encode the since value for spending this HTLC
    pub fn encode_since(&self) -> u64 {
        self.lock.to_timelock_config().encode_since()
    }
}

/// HTLC witness builder
pub struct HtlcWitness;

impl HtlcWitness {
    /// Build witness for recipient path (secret + signature)
    ///
    /// Format: <signature (64 bytes)> <secret (32 bytes)> <path_selector (0x01)>
    pub fn recipient(signature: [u8; 64], secret: [u8; 32]) -> Vec<u8> {
        let mut witness = Vec::with_capacity(97);
        witness.extend_from_slice(&signature);
        witness.extend_from_slice(&secret);
        witness.push(0x01); // Recipient path selector
        witness
    }

    /// Build witness for sender timeout path (signature only)
    ///
    /// Format: <signature (64 bytes)> <path_selector (0x00)>
    pub fn sender_timeout(signature: [u8; 64]) -> Vec<u8> {
        let mut witness = Vec::with_capacity(65);
        witness.extend_from_slice(&signature);
        witness.push(0x00); // Sender path selector
        witness
    }
}

/// Compute blake3 hash of a secret
pub fn compute_secret_hash(secret: &[u8]) -> [u8; 32] {
    blake3::hash(secret).into()
}

/// Generate a random secret (32 bytes)
#[cfg(feature = "rand")]
pub fn generate_secret() -> [u8; 32] {
    let mut secret = [0u8; 32];
    rand::fill(&mut secret);
    secret
}

/// Verify that a secret matches the given hash
pub fn verify_secret(secret: &[u8], expected_hash: &[u8; 32]) -> bool {
    compute_secret_hash(secret) == *expected_hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_htlc_lock_type_to_u8() {
        assert_eq!(HtlcLockType::AbsoluteDaa { target: 100 }.to_u8(), 0);
        assert_eq!(HtlcLockType::AbsoluteTimestamp { target: 100 }.to_u8(), 1);
        assert_eq!(HtlcLockType::RelativeDaa { delta: 100 }.to_u8(), 2);
        assert_eq!(HtlcLockType::RelativeTimestamp { delta_seconds: 100 }.to_u8(), 3);
    }

    #[test]
    fn test_htlc_lock_type_value() {
        assert_eq!(HtlcLockType::AbsoluteDaa { target: 1000 }.value(), 1000);
        assert_eq!(HtlcLockType::RelativeDaa { delta: 500 }.value(), 500);
    }

    #[test]
    fn test_htlc_config_build_script_args() {
        let config = HtlcConfig {
            secret_hash: [0xAB; 32],
            recipient_pubkey: [0x11; 32],
            sender_pubkey: [0x22; 32],
            lock: HtlcLockType::AbsoluteTimestamp { target: 1735689600 },
        };

        let args = config.build_script_args();
        assert_eq!(args.len(), 105);

        // Check secret_hash
        assert_eq!(&args[0..32], &[0xAB; 32]);
        // Check recipient_pubkey
        assert_eq!(&args[32..64], &[0x11; 32]);
        // Check sender_pubkey
        assert_eq!(&args[64..96], &[0x22; 32]);
        // Check lock_type
        assert_eq!(args[96], 1);
        // Check lock_value (little-endian)
        assert_eq!(&args[97..105], &1735689600u64.to_le_bytes());
    }

    #[test]
    fn test_htlc_witness_recipient() {
        let signature = [0x33; 64];
        let secret = [0x44; 32];
        let witness = HtlcWitness::recipient(signature, secret);

        assert_eq!(witness.len(), 97);
        assert_eq!(&witness[0..64], &signature);
        assert_eq!(&witness[64..96], &secret);
        assert_eq!(witness[96], 0x01);
    }

    #[test]
    fn test_htlc_witness_sender_timeout() {
        let signature = [0x33; 64];
        let witness = HtlcWitness::sender_timeout(signature);

        assert_eq!(witness.len(), 65);
        assert_eq!(&witness[0..64], &signature);
        assert_eq!(witness[64], 0x00);
    }

    #[test]
    fn test_compute_secret_hash() {
        let secret = b"my secret";
        let hash = compute_secret_hash(secret);
        assert_eq!(hash.len(), 32);

        // Verify deterministic
        let hash2 = compute_secret_hash(secret);
        assert_eq!(hash, hash2);
    }

    #[test]
    fn test_verify_secret() {
        let secret = b"my secret";
        let hash = compute_secret_hash(secret);

        assert!(verify_secret(secret, &hash));
        assert!(!verify_secret(b"wrong secret", &hash));
    }

    #[test]
    fn test_htlc_config_encode_since() {
        let config = HtlcConfig {
            secret_hash: [0xAB; 32],
            recipient_pubkey: [0x11; 32],
            sender_pubkey: [0x22; 32],
            lock: HtlcLockType::AbsoluteTimestamp { target: 1735689600 },
        };

        let since = config.encode_since();
        // Check flags: bit63=0 (absolute), bit62=1 (timestamp)
        assert_eq!(since & 0x8000000000000000, 0);
        assert_eq!(since & 0x4000000000000000, 0x4000000000000000);
        // Check value
        assert_eq!(since & 0x00FFFFFFFFFFFFFF, 1735689600);
    }
}
