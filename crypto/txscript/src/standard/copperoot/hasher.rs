//! Copperoot hasher implementation using BLAKE3-256 with domain separation
//!
//! This module provides tagged hash functions for Copperoot operations,
//! all using BLAKE3-256 as the underlying hash function.

use blake3::Hasher;
use tondi_hashes::Hash;

/// Copperoot tagged hash function using BLAKE3-256
pub fn tagged_hash(tag: &[u8], data: &[u8]) -> Hash {
    let mut hasher = Hasher::new();
    hasher.update(tag);
    hasher.update(data);
    Hash::from_slice(hasher.finalize().as_bytes())
}

/// Copperoot tap tweak hash
pub fn copperoot_tap_tweak(internal_key: &[u8], merkle_root: Option<&[u8]>) -> Hash {
    let mut hasher = Hasher::new();
    hasher.update(b"CopperootTapTweak");
    hasher.update(internal_key);
    if let Some(root) = merkle_root {
        hasher.update(root);
    }
    Hash::from_slice(hasher.finalize().as_bytes())
}

/// Copperoot leaf hash
pub fn copperoot_leaf(leaf_version: u8, script: &[u8]) -> Hash {
    let mut hasher = Hasher::new();
    hasher.update(b"CopperootLeaf");
    hasher.update(&[leaf_version]);
    hasher.update(script);
    Hash::from_slice(hasher.finalize().as_bytes())
}

/// Copperoot node hash
pub fn copperoot_node(left: &[u8], right: &[u8]) -> Hash {
    let mut hasher = Hasher::new();
    hasher.update(b"CopperootNode");
    hasher.update(left);
    hasher.update(right);
    Hash::from_slice(hasher.finalize().as_bytes())
}

/// Copperoot script path hash
pub fn copperoot_script_path(leaf_hash: &[u8], path: &[&[u8]]) -> Hash {
    let mut hasher = Hasher::new();
    hasher.update(b"CopperootScriptPath");
    hasher.update(leaf_hash);
    for node in path {
        hasher.update(node);
    }
    Hash::from_slice(hasher.finalize().as_bytes())
}

/// Copperoot key path hash
pub fn copperoot_key_path(internal_key: &[u8], merkle_root: Option<&[u8]>) -> Hash {
    let mut hasher = Hasher::new();
    hasher.update(b"CopperootKeyPath");
    hasher.update(internal_key);
    if let Some(root) = merkle_root {
        hasher.update(root);
    }
    Hash::from_slice(hasher.finalize().as_bytes())
}

/// Copperoot sighash
pub fn copperoot_sighash(data: &[u8]) -> Hash {
    let mut hasher = Hasher::new();
    hasher.update(b"CopperootSighash");
    hasher.update(data);
    Hash::from_slice(hasher.finalize().as_bytes())
}

/// Copperoot merkle root hash
pub fn copperoot_merkle_root(leaves: &[&[u8]]) -> Hash {
    let mut hasher = Hasher::new();
    hasher.update(b"CopperootMerkleRoot");
    for leaf in leaves {
        hasher.update(leaf);
    }
    Hash::from_slice(hasher.finalize().as_bytes())
}

/// Copperoot verkle root hash
pub fn copperoot_verkle_root(commitment: &[u8], proof: &[u8]) -> Hash {
    let mut hasher = Hasher::new();
    hasher.update(b"CopperootVerkleRoot");
    hasher.update(commitment);
    hasher.update(proof);
    Hash::from_slice(hasher.finalize().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tagged_hash() {
        let tag = b"CopperootTapTweak";
        let data = b"test data";
        let hash = tagged_hash(tag, data);
        assert_eq!(hash.as_bytes().len(), 32);
    }

    #[test]
    fn test_copperoot_tap_tweak() {
        let internal_key = b"internal_key_32_bytes_long_test_data";
        let merkle_root = b"merkle_root_32_bytes_long_test_data";
        let hash = copperoot_tap_tweak(internal_key, Some(merkle_root));
        assert_eq!(hash.as_bytes().len(), 32);
    }

    #[test]
    fn test_copperoot_leaf() {
        let script = b"test script";
        let hash = copperoot_leaf(0xc0, script);
        assert_eq!(hash.as_bytes().len(), 32);
    }

    #[test]
    fn test_copperoot_node() {
        let left = b"left_node_32_bytes_long_test_data";
        let right = b"right_node_32_bytes_long_test_data";
        let hash = copperoot_node(left, right);
        assert_eq!(hash.as_bytes().len(), 32);
    }
}
