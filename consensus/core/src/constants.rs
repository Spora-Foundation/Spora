/// BLOCK_VERSION represents the current block version
pub const BLOCK_VERSION: u16 = 1;

/// TX_VERSION is the current latest supported transaction version.
pub const TX_VERSION: u16 = 0;

pub const LOCK_TIME_THRESHOLD: u64 = 500_000_000_000;

/// MAX_SCRIPT_PUBLIC_KEY_VERSION is the current latest supported public key script version.
pub const MAX_SCRIPT_PUBLIC_KEY_VERSION: u16 = 3;

// Script version constants for different script types
pub const SCRIPT_VER_CLASSIC: u16 = 0;       // Legacy script types (PubKey, ScriptHash, etc.)
pub const SCRIPT_VER_TAPROOT: u16 = 1;      // Taproot (BIP341/SHA256)
pub const SCRIPT_VER_COPPEROOT_MERKLE: u16 = 2;         // Pay-to-Copperoot-Merkle (BLAKE3)
pub const SCRIPT_VER_COPPEROOT_VERKLE: u16 = 3;        // Pay-to-Copperoot-Verkle (BLAKE3) - Reserved

// 保持向后兼容的别名
pub const SCRIPT_VER_P2CR: u16 = SCRIPT_VER_COPPEROOT_MERKLE;
pub const SCRIPT_VER_P2CRV: u16 = SCRIPT_VER_COPPEROOT_VERKLE;

/// SauPerTondi is the number of sau in one tondi (1 TONDI).
pub const SAU_PER_TONDI: u64 = 100_000_000;

/// The parameter for scaling inverse TONDI value to mass units (KIP-0009)
pub const STORAGE_MASS_PARAMETER: u64 = SAU_PER_TONDI * 10_000;

/// The parameter defining how much mass per byte to charge for when calculating
/// transient storage mass. Since normally the block mass limit is 500_000, this limits
/// block body byte size to 125_000 (KIP-0013).
pub const TRANSIENT_BYTE_TO_MASS_FACTOR: u64 = 4;

/// MaxSau is the maximum transaction amount allowed in sau.
pub const MAX_SAU: u64 = 29_000_000_000 * SAU_PER_TONDI;

// MAX_TX_IN_SEQUENCE_NUM is the maximum sequence number the sequence field
// of a transaction input can be.
pub const MAX_TX_IN_SEQUENCE_NUM: u64 = u64::MAX;

// SEQUENCE_LOCK_TIME_MASK is a mask that extracts the relative lock time
// when masked against the transaction input sequence number.
pub const SEQUENCE_LOCK_TIME_MASK: u64 = 0x00000000ffffffff;

// SEQUENCE_LOCK_TIME_DISABLED is a flag that if set on a transaction
// input's sequence number, the sequence number will not be interpreted
// as a relative lock time.
pub const SEQUENCE_LOCK_TIME_DISABLED: u64 = 1 << 63;

/// UNACCEPTED_DAA_SCORE is used to for UtxoEntries that were created by
/// transactions in the mempool, or otherwise not-yet-accepted transactions.
pub const UNACCEPTED_DAA_SCORE: u64 = u64::MAX;
