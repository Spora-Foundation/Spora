// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Time lock support for wallet transactions (Cell model)
//
// This module provides wallet-level helpers for creating time-locked transactions
// using the Cell model's `since` field and CKB-VM scripts.
//
// ## Migration from the previous lock_time flow
//
// Previous wallet code used `lock_time` in `GeneratorSettings` to create time-locked
// outputs using `pay_to_address_with_lock_time_script`. This has been deprecated
// because:
//
// 1. `OP_CHECKLOCKTIMEVERIFY` is disabled in Cell model
// 2. Cell model uses per-input `since` field, not tx-level `lock_time`
// 3. Time locks should be enforced by CKB-VM scripts, not txscript opcodes
//
// ## New Approach
//
// 1. Set `since` field on inputs using `SinceEncoding`
// 2. Use time lock Script from `spora_exec::scripts::timelock`
// 3. The lock script verifies `since` via CKB-VM syscall

//! Time lock utilities for wallet transactions
//!
//! This module provides types and functions for creating time-locked transactions
//! in the Cell model. Unlike the previous approach that used tx-level `lock_time`,
//! the Cell model uses per-input `since` fields with CKB-VM verification.
//!
//! # Example
//!
//! ```rust,ignore
//! use spora_wallet_core::tx::timelock::{SinceEncoding, TimelockConfig};
//! use spora_exec::scripts::timelock;
//!
//! // Configure an absolute timestamp lock
//! let config = TimelockConfig::AbsoluteTimestamp {
//!     target: 1735689600, // 2025-01-01 00:00:00 UTC
//! };
//!
//! // Encode the since value for the input
//! let since = config.encode_since();
//!
//! // Create the standalone timelock script reference
//! let lock_script = config.create_lock_script().expect("timelock config must be locked");
//! ```

use spora_exec::{scripts::timelock as exec_timelock, Script};

/// Time lock configuration for wallet transactions
///
/// This enum represents different types of time locks that can be applied
/// to transaction inputs in the Cell model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelockConfig {
    /// Absolute timestamp lock (Unix timestamp)
    ///
    /// The input can only be spent after the specified Unix timestamp.
    /// Uses bit62=1, bit63=0 in the since encoding.
    AbsoluteTimestamp {
        /// Target Unix timestamp (seconds since epoch)
        target: u64,
    },

    /// Relative DAA score lock (block count)
    ///
    /// The input can only be spent after `delta` blocks have passed
    /// since the input was confirmed.
    /// Uses bit62=0, bit63=1 in the since encoding.
    RelativeDaa {
        /// Number of blocks to wait
        delta: u64,
    },

    /// Absolute DAA score lock
    ///
    /// The input can only be spent after the specified DAA score.
    /// Uses bit62=0, bit63=0 in the since encoding.
    AbsoluteDaa {
        /// Target DAA score
        target: u64,
    },

    /// Relative timestamp lock
    ///
    /// The input can only be spent after `delta_seconds` have passed
    /// since the input was confirmed.
    /// Uses bit62=1, bit63=1 in the since encoding.
    RelativeTimestamp {
        /// Number of seconds to wait
        delta_seconds: u64,
    },

    /// No time lock
    None,
}

impl TimelockConfig {
    /// Create an absolute timestamp lock
    pub fn absolute_timestamp(target: u64) -> Self {
        Self::AbsoluteTimestamp { target }
    }

    /// Create a relative DAA score lock
    pub fn relative_daa(delta: u64) -> Self {
        Self::RelativeDaa { delta }
    }

    /// Create an absolute DAA score lock
    pub fn absolute_daa(target: u64) -> Self {
        Self::AbsoluteDaa { target }
    }

    /// Create a relative timestamp lock
    pub fn relative_timestamp(delta_seconds: u64) -> Self {
        Self::RelativeTimestamp { delta_seconds }
    }

    /// Encode the since value for this time lock configuration
    ///
    /// Returns the `since` value to use when creating a `CellInput`.
    pub fn encode_since(&self) -> u64 {
        match self {
            Self::AbsoluteTimestamp { target } => exec_timelock::encode_absolute_timestamp_since(*target),
            Self::RelativeDaa { delta } => exec_timelock::encode_relative_daa_since(*delta),
            Self::AbsoluteDaa { target } => exec_timelock::encode_absolute_daa_since(*target),
            Self::RelativeTimestamp { delta_seconds } => exec_timelock::encode_relative_timestamp_since(*delta_seconds),
            Self::None => 0,
        }
    }

    /// Returns `true` when the lock uses relative semantics.
    pub fn is_relative(&self) -> bool {
        matches!(self, Self::RelativeDaa { .. } | Self::RelativeTimestamp { .. })
    }

    /// Returns `true` when the lock uses timestamp semantics.
    pub fn is_timestamp(&self) -> bool {
        matches!(self, Self::AbsoluteTimestamp { .. } | Self::RelativeTimestamp { .. })
    }

    /// Check if this configuration has a time lock
    pub fn is_locked(&self) -> bool {
        !matches!(self, Self::None)
    }

    /// Get the lock value (target or delta)
    pub fn value(&self) -> Option<u64> {
        match self {
            Self::AbsoluteTimestamp { target } => Some(*target),
            Self::RelativeDaa { delta } => Some(*delta),
            Self::AbsoluteDaa { target } => Some(*target),
            Self::RelativeTimestamp { delta_seconds } => Some(*delta_seconds),
            Self::None => None,
        }
    }

    /// Create the standalone timelock [`Script`] for this configuration.
    ///
    /// This returns only the timelock verifier script. It does not enforce ownership
    /// by itself, so it must not be used as the sole production lock for user funds
    /// until the combined secp256k1 + timelock VM path is fully wired in.
    ///
    /// Returns `None` for [`TimelockConfig::None`].
    pub fn create_lock_script(&self) -> Option<Script> {
        match self {
            Self::AbsoluteTimestamp { target } => Some(exec_timelock::absolute_timestamp_lock(*target)),
            Self::RelativeDaa { delta } => Some(exec_timelock::relative_daa_lock(*delta)),
            Self::AbsoluteDaa { target } => Some(exec_timelock::absolute_daa_lock(*target)),
            Self::RelativeTimestamp { delta_seconds } => Some(exec_timelock::relative_timestamp_lock(*delta_seconds)),
            Self::None => None,
        }
    }
}

impl Default for TimelockConfig {
    fn default() -> Self {
        Self::None
    }
}

/// Since encoding utilities
///
/// These functions provide low-level access to since encoding for advanced use cases.
/// Most users should use [`TimelockConfig`] instead.
pub mod since_encoding {
    pub use spora_exec::scripts::timelock::{
        decode_since, encode_absolute_daa_since, encode_absolute_timestamp_since, encode_relative_daa_since,
        encode_relative_timestamp_since, since_flags,
    };
}

/// Re-export of time lock script helpers from spora_exec
///
/// These functions create [`Script`](spora_exec::celltx::Script) instances
/// for use in Cell outputs.
pub mod script_helpers {
    pub use spora_exec::scripts::timelock::{absolute_daa_lock, absolute_timestamp_lock, relative_daa_lock, relative_timestamp_lock};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timelock_config_absolute_timestamp() {
        let target = 1735689600u64;
        let config = TimelockConfig::absolute_timestamp(target);

        assert!(config.is_locked());
        assert_eq!(config.value(), Some(target));

        let since = config.encode_since();
        assert_eq!(since & 0x8000000000000000, 0); // Not relative
        assert_eq!(since & 0x4000000000000000, 0x4000000000000000); // Timestamp
        assert_eq!(since & 0x00FFFFFFFFFFFFFF, target);
    }

    #[test]
    fn test_timelock_config_relative_daa() {
        let delta = 100u64;
        let config = TimelockConfig::relative_daa(delta);

        assert!(config.is_locked());
        assert_eq!(config.value(), Some(delta));

        let since = config.encode_since();
        assert_eq!(since & 0x8000000000000000, 0x8000000000000000); // Relative
        assert_eq!(since & 0x4000000000000000, 0); // Not timestamp
        assert_eq!(since & 0x00FFFFFFFFFFFFFF, delta);
    }

    #[test]
    fn test_timelock_config_none() {
        let config = TimelockConfig::None;

        assert!(!config.is_locked());
        assert_eq!(config.value(), None);
        assert_eq!(config.encode_since(), 0);
        assert_eq!(config.create_lock_script(), None);
    }

    #[test]
    fn test_timelock_config_create_lock_script() {
        let config = TimelockConfig::relative_timestamp(3600);
        let script = config.create_lock_script().expect("relative timestamp config must create a script");

        assert_eq!(script.code_hash, exec_timelock::RELATIVE_TIMESTAMP_LOCK_CODE_HASH);
        assert_eq!(script.hash_type, exec_timelock::TIME_LOCK_HASH_TYPE);
        assert_eq!(script.args, 3600u64.to_le_bytes().to_vec());
    }

    #[test]
    fn test_timelock_config_flags() {
        let absolute_timestamp = TimelockConfig::absolute_timestamp(1735689600);
        assert!(!absolute_timestamp.is_relative());
        assert!(absolute_timestamp.is_timestamp());

        let relative_daa = TimelockConfig::relative_daa(144);
        assert!(relative_daa.is_relative());
        assert!(!relative_daa.is_timestamp());
    }
}
