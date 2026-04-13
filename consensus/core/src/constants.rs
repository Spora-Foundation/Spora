/// BLOCK_VERSION represents the current block version
pub const BLOCK_VERSION: u16 = 1;

/// TX_VERSION is the current latest supported transaction version.
pub const TX_VERSION: u16 = 0;

/// CELL_TX_VERSION is the canonical Cell transaction version used on active paths.
pub const CELL_TX_VERSION: u16 = spora_exec::CELL_TX_VERSION;

pub const LOCK_TIME_THRESHOLD: u64 = 500_000_000_000;

/// MAX_SCRIPT_PUBLIC_KEY_VERSION is the current latest supported public key script version.
pub const MAX_SCRIPT_PUBLIC_KEY_VERSION: u16 = 0;

// Script version constants for different script types
pub const SCRIPT_VER_CLASSIC: u16 = 0; // Classic script types (PubKey, ScriptHash, etc.)

/// SauPerSpora is the number of sau in one spora (1 SPORA).
pub const SAU_PER_SPORA: u64 = 100_000_000;

/// The parameter for scaling inverse SPORA value to mass units (KIP-0009)
pub const STORAGE_MASS_PARAMETER: u64 = SAU_PER_SPORA * 10_000;

/// The parameter defining how much mass per byte to charge for when calculating
/// transient storage mass. Since normally the block mass limit is 500_000, this limits
/// block body byte size to 125_000 (KIP-0013).
pub const TRANSIENT_BYTE_TO_MASS_FACTOR: u64 = 4;

/// MaxSau is the maximum transaction amount allowed in sau.
pub const MAX_SAU: u64 = 29_000_000_000 * SAU_PER_SPORA;

// MAX_TX_IN_SEQUENCE_NUM is the maximum sequence number the sequence field
// of a transaction input can be.
pub const MAX_TX_IN_SEQUENCE_NUM: u64 = u64::MAX;

/// UNACCEPTED_DAA_SCORE is used for Cell entries that were created by
/// transactions in the mempool, or otherwise not-yet-accepted transactions.
pub const UNACCEPTED_DAA_SCORE: u64 = u64::MAX;
