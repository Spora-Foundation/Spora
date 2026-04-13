use rand::Rng;
use spora_hashes::Hash;

/// Compute data hash for cell output
///
/// Hashes the cell data using blake3 with domain separation
pub fn compute_data_hash(data: &[u8]) -> [u8; 32] {
    if data.is_empty() {
        // Empty data has zero hash
        [0u8; 32]
    } else {
        use blake3::Hasher;

        let mut hasher = Hasher::new();
        hasher.update(b"spora-cell/data"); // Domain separation
        hasher.update(data);

        *hasher.finalize().as_bytes()
    }
}

/// Convert TransactionOutpoint to Hash for tree indexing
pub fn outpoint_to_hash(outpoint: &spora_consensus_core::tx::TransactionOutpoint) -> Hash {
    use blake3::Hasher;

    let mut hasher = Hasher::new();
    hasher.update(b"spora-cell/outpoint"); // Domain separation
    hasher.update(&outpoint.tx_hash);
    hasher.update(&outpoint.index.to_le_bytes());

    Hash::from_bytes(*hasher.finalize().as_bytes())
}

pub(crate) struct CoinFlip {
    p: f64,
}

impl Default for CoinFlip {
    fn default() -> Self {
        Self { p: 1.0 / 200.0 }
    }
}

impl CoinFlip {
    pub(crate) fn new(p: f64) -> Self {
        Self { p }
    }

    pub fn flip(self) -> bool {
        rand::thread_rng().gen_bool(self.p)
    }
}
