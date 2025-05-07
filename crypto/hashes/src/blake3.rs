use blake3::Hasher;

/// Computes a one-shot BLAKE3-256 hash of the input.
/// Returns a 32-byte array.
///
/// 📌 Used for:
/// - Transaction ID (TXID) generation
/// - Block header hashing
/// - Public key hashing (e.g., address encoding)
/// - Schnorr signing challenge hash (H(R‖m))
pub fn blake3_256(data: &[u8]) -> [u8; 32] {
    *blake3::hash(data).as_bytes()
}

/// Computes a double BLAKE3 hash (i.e., BLAKE3d),
/// suitable as a drop-in replacement for SHA256d (SHA256 twice).
/// Returns a 32-byte array.
///
/// 📌 Used for:
/// - Block hash (chain PoW identifier)
/// - Commitment ID hashing
/// - UTXO ID or contract state anchors
pub fn blake3d(data: &[u8]) -> [u8; 32] {
    blake3_256(&blake3_256(data))
}

/// Computes a streaming BLAKE3 hash from multiple input slices,
/// useful when serializing structured data piecewise.
/// Returns a 32-byte array.
///
/// 📌 Used for:
/// - Merkle tree hash construction
/// - Incremental digest (e.g., block serialization)
/// - Multi-part message signing
pub fn blake3_stream(data: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Hasher::new();
    for chunk in data {
        hasher.update(chunk);
    }
    *hasher.finalize().as_bytes()
}

/// Unit test for `blake3_256()`. Verifies hash length is 32 bytes.
/// Replace "tondi" with a test vector if needed.
#[test]
fn test_blake3_256() {
    use crate::blake3::blake3_256;
    let hash = blake3_256(b"tondi");
    assert_eq!(hash.len(), 32);
}
