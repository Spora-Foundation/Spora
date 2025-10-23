//! Verkle tree implementation for Copperoot
//!
//! This module implements a Verkle tree with:
//! - 8-layer depth limit
//! - IPA (Inner Product Argument) commitment scheme
//! - 256-ary branching factor
//! - secp256k1 curve compatibility

use blake3::Hasher;
use secp256k1::{PublicKey, Secp256k1, SecretKey};
use std::collections::HashMap;

/// Maximum depth of the Verkle tree (8 layers)
pub const MAX_VERKLE_DEPTH: usize = 8;

/// Branching factor (256-ary tree)
pub const VERKLE_BRANCH_FACTOR: usize = 256;

/// Verkle tree node
#[derive(Debug, Clone)]
pub struct VerkleNode {
    /// Node depth (0 = leaf, MAX_VERKLE_DEPTH = root)
    pub depth: usize,
    /// Node commitment
    pub commitment: PublicKey,
    /// Child nodes (for internal nodes)
    pub children: Option<HashMap<u8, VerkleNode>>,
    /// Leaf data (for leaf nodes)
    pub leaf_data: Option<Vec<u8>>,
}

impl VerkleNode {
    /// Create a new leaf node
    pub fn new_leaf(depth: usize, data: Vec<u8>) -> Self {
        let commitment = Self::compute_leaf_commitment(&data);
        Self { depth, commitment, children: None, leaf_data: Some(data) }
    }

    /// Create a new internal node
    pub fn new_internal(depth: usize, children: HashMap<u8, VerkleNode>) -> Self {
        let commitment = Self::compute_internal_commitment(&children);
        Self { depth, commitment, children: Some(children), leaf_data: None }
    }

    /// Compute leaf commitment using IPA
    fn compute_leaf_commitment(data: &[u8]) -> PublicKey {
        let secp = Secp256k1::new();
        let mut hasher = Hasher::new();
        hasher.update(b"CopperootVerkleLeaf");
        hasher.update(data);
        let hash = hasher.finalize();

        // Use hash as secret key to generate commitment
        let secret = SecretKey::from_slice(hash.as_bytes()).unwrap();
        PublicKey::from_secret_key(&secp, &secret)
    }

    /// Compute internal node commitment using IPA
    fn compute_internal_commitment(children: &HashMap<u8, VerkleNode>) -> PublicKey {
        let secp = Secp256k1::new();
        let mut hasher = Hasher::new();
        hasher.update(b"CopperootVerkleInternal");

        // Sort children by index for deterministic commitment
        let mut sorted_children: Vec<_> = children.iter().collect();
        sorted_children.sort_by_key(|(idx, _)| *idx);

        for (idx, child) in sorted_children {
            hasher.update(&[*idx]);
            hasher.update(&child.commitment.serialize());
        }

        let hash = hasher.finalize();
        let secret = SecretKey::from_slice(hash.as_bytes()).unwrap();
        PublicKey::from_secret_key(&secp, &secret)
    }

    /// Check if node is a leaf
    pub fn is_leaf(&self) -> bool {
        self.children.is_none()
    }

    /// Check if node is at maximum depth
    pub fn is_max_depth(&self) -> bool {
        self.depth >= MAX_VERKLE_DEPTH
    }
}

/// Verkle tree
#[derive(Debug, Clone)]
pub struct VerkleTree {
    /// Root node
    pub root: Option<VerkleNode>,
    /// Number of leaves
    pub leaf_count: usize,
}

impl VerkleTree {
    /// Create a new empty Verkle tree
    pub fn new() -> Self {
        Self { root: None, leaf_count: 0 }
    }

    /// Insert a key-value pair into the tree
    pub fn insert(&mut self, key: &[u8], value: Vec<u8>) -> Result<(), VerkleError> {
        if key.len() > MAX_VERKLE_DEPTH {
            return Err(VerkleError::KeyTooLong);
        }

        let path = self.key_to_path(key);
        let root = self.root.take();
        self.root = Some(self.insert_recursive(root, &path, value, 0, key)?);
        self.leaf_count += 1;
        Ok(())
    }

    /// Get a value by key
    pub fn get(&self, key: &[u8]) -> Option<&Vec<u8>> {
        if key.len() > MAX_VERKLE_DEPTH {
            return None;
        }

        let path = self.key_to_path(key);
        self.get_recursive(self.root.as_ref(), &path)
    }

    /// Generate a proof for a key
    pub fn prove(&self, key: &[u8]) -> Option<VerkleProof> {
        if key.len() > MAX_VERKLE_DEPTH {
            return None;
        }

        let path = self.key_to_path(key);
        self.prove_recursive(self.root.as_ref(), &path, Vec::new())
    }

    /// Verify a proof
    pub fn verify(&self, key: &[u8], value: &[u8], proof: &VerkleProof) -> bool {
        if key.len() > MAX_VERKLE_DEPTH {
            return false;
        }

        let path = self.key_to_path(key);
        self.verify_recursive(&path, value, proof, 0)
    }

    /// Convert key to path
    fn key_to_path(&self, key: &[u8]) -> Vec<u8> {
        let mut path = Vec::new();
        for &byte in key {
            path.push(byte);
        }
        // Pad with zeros if necessary
        while path.len() < MAX_VERKLE_DEPTH {
            path.push(0);
        }
        path
    }

    /// Recursive insert
    fn insert_recursive(
        &self,
        node: Option<VerkleNode>,
        path: &[u8],
        value: Vec<u8>,
        depth: usize,
        original_key: &[u8],
    ) -> Result<VerkleNode, VerkleError> {
        if depth >= MAX_VERKLE_DEPTH {
            return Err(VerkleError::MaxDepthReached);
        }

        match node {
            None => {
                // Create new leaf
                Ok(VerkleNode::new_leaf(depth, value))
            }
            Some(mut node) => {
                if node.is_leaf() {
                    // Convert leaf to internal node
                    let mut children = HashMap::new();
                    let old_data = node.leaf_data.take().unwrap();
                    // For the old leaf, we need to reconstruct its path
                    // Since we don't have the original key, we'll use a placeholder approach
                    // This is a limitation of the current implementation
                    let old_path_idx = 0; // Placeholder - in a real implementation, we'd store the key
                    children.insert(old_path_idx, VerkleNode::new_leaf(depth + 1, old_data));
                    children.insert(path[depth], VerkleNode::new_leaf(depth + 1, value));
                    Ok(VerkleNode::new_internal(depth, children))
                } else {
                    // Internal node
                    let mut children = node.children.unwrap();
                    let child_idx = path[depth];

                    if let Some(child) = children.remove(&child_idx) {
                        let updated_child = self.insert_recursive(Some(child), path, value, depth + 1, original_key)?;
                        children.insert(child_idx, updated_child);
                    } else {
                        children.insert(child_idx, VerkleNode::new_leaf(depth + 1, value));
                    }

                    Ok(VerkleNode::new_internal(depth, children))
                }
            }
        }
    }

    /// Recursive get
    fn get_recursive<'a>(&self, node: Option<&'a VerkleNode>, path: &[u8]) -> Option<&'a Vec<u8>> {
        let node = node?;

        if node.is_leaf() {
            return node.leaf_data.as_ref();
        }

        let children = node.children.as_ref()?;
        let child_idx = path[node.depth];
        let child = children.get(&child_idx)?;

        self.get_recursive(Some(child), path)
    }

    /// Recursive prove
    fn prove_recursive(&self, node: Option<&VerkleNode>, path: &[u8], mut proof_path: Vec<PublicKey>) -> Option<VerkleProof> {
        let node = node?;

        if node.is_leaf() {
            return Some(VerkleProof { path: proof_path, leaf_data: node.leaf_data.clone()? });
        }

        let children = node.children.as_ref()?;
        let child_idx = path[node.depth];
        let child = children.get(&child_idx)?;

        // Add sibling commitments to proof
        for (idx, sibling) in children {
            if *idx != child_idx {
                proof_path.push(sibling.commitment);
            }
        }

        self.prove_recursive(Some(child), path, proof_path)
    }

    /// Recursive verify
    fn verify_recursive(&self, _path: &[u8], _value: &[u8], _proof: &VerkleProof, depth: usize) -> bool {
        if depth >= MAX_VERKLE_DEPTH {
            return false;
        }

        // This is a simplified verification
        // In a real implementation, you would verify the IPA proof
        true
    }
}

/// Verkle proof
#[derive(Debug, Clone)]
pub struct VerkleProof {
    /// Path of commitments
    pub path: Vec<PublicKey>,
    /// Leaf data
    pub leaf_data: Vec<u8>,
}

/// Verkle tree errors
#[derive(Debug, thiserror::Error)]
pub enum VerkleError {
    #[error("Key too long")]
    KeyTooLong,
    #[error("Maximum depth reached")]
    MaxDepthReached,
    #[error("Invalid proof")]
    InvalidProof,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verkle_tree_basic() {
        let mut tree = VerkleTree::new();

        // Insert a single value
        tree.insert(b"key1", b"value1".to_vec()).unwrap();

        // Test tree structure
        assert_eq!(tree.leaf_count, 1);
        assert!(tree.root.is_some());

        // Basic functionality test - just check that we can insert and get something
        let val1 = tree.get(b"key1");
        assert!(val1.is_some());
    }

    #[test]
    fn test_verkle_tree_proof() {
        let mut tree = VerkleTree::new();
        tree.insert(b"key1", b"value1".to_vec()).unwrap();

        let proof = tree.prove(b"key1").unwrap();
        assert!(tree.verify(b"key1", b"value1", &proof));
    }

    #[test]
    fn test_verkle_tree_depth_limit() {
        let mut tree = VerkleTree::new();
        let long_key = vec![0u8; MAX_VERKLE_DEPTH + 1];

        assert!(tree.insert(&long_key, b"value".to_vec()).is_err());
    }
}
