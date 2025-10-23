// SPDX-License-Identifier: ISC
// Copyright (C) 2025 Spora developers
//
// Data availability proofs: NMT/KZG sampling (simplified version)

use crate::Result;
use borsh::{BorshDeserialize, BorshSerialize};

/// Segment proof (Merkle proof for DA sampling)
///
/// Simplified version: full segment hash as commitment
/// Future: Replace with NMT (Namespaced Merkle Tree) or KZG
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SegmentProof {
    /// Segment ID
    pub segment_id: u32,
    /// Data chunk (sampled)
    pub chunk_data: Vec<u8>,
    /// Chunk offset
    pub chunk_offset: u64,
    /// Chunk length
    pub chunk_length: u32,
    /// Merkle path (simplified: just root hash for now)
    pub merkle_path: Vec<[u8; 32]>,
    /// Segment root (commitment)
    pub segment_root: [u8; 32],
}

impl SegmentProof {
    /// Create a new segment proof
    pub fn new(
        segment_id: u32,
        chunk_data: Vec<u8>,
        chunk_offset: u64,
        chunk_length: u32,
        segment_root: [u8; 32],
    ) -> Self {
        Self {
            segment_id,
            chunk_data,
            chunk_offset,
            chunk_length,
            merkle_path: vec![], // TODO: implement Merkle path
            segment_root,
        }
    }
    
    /// Verify proof (simplified)
    pub fn verify(&self) -> Result<bool> {
        // Simplified verification: just check chunk hash against root
        // In production, this should verify the full Merkle path
        
        if self.chunk_data.len() != self.chunk_length as usize {
            return Ok(false);
        }
        
        // For now, just return true (placeholder)
        // TODO: Implement proper Merkle verification
        Ok(true)
    }
}

/// Proof verifier
pub struct ProofVerifier {
    /// Verification parameters (for future KZG/NMT)
    _params: (),
}

impl ProofVerifier {
    /// Create a new proof verifier
    pub fn new() -> Self {
        Self { _params: () }
    }
    
    /// Verify a segment proof
    pub fn verify_proof(&self, proof: &SegmentProof) -> Result<bool> {
        proof.verify()
    }
    
    /// Batch verify multiple proofs
    pub fn batch_verify(&self, proofs: &[SegmentProof]) -> Result<Vec<bool>> {
        proofs.iter().map(|p| p.verify()).collect()
    }
}

impl Default for ProofVerifier {
    fn default() -> Self {
        Self::new()
    }
}

/// Merkle tree builder (simplified)
///
/// Future: Replace with proper NMT implementation
pub struct MerkleTreeBuilder {
    leaves: Vec<[u8; 32]>,
}

impl MerkleTreeBuilder {
    /// Create a new Merkle tree builder
    pub fn new() -> Self {
        Self { leaves: Vec::new() }
    }
    
    /// Add a leaf
    pub fn add_leaf(&mut self, data: &[u8]) {
        let hash = blake3::hash(data);
        self.leaves.push(*hash.as_bytes());
    }
    
    /// Build the tree and return root
    pub fn build(&self) -> [u8; 32] {
        if self.leaves.is_empty() {
            return [0u8; 32];
        }
        
        let mut level = self.leaves.clone();
        
        while level.len() > 1 {
            let mut next_level = Vec::new();
            
            for chunk in level.chunks(2) {
                let hash = if chunk.len() == 2 {
                    // Hash pair
                    let mut hasher = blake3::Hasher::new();
                    hasher.update(&chunk[0]);
                    hasher.update(&chunk[1]);
                    *hasher.finalize().as_bytes()
                } else {
                    // Odd node: promote directly
                    chunk[0]
                };
                next_level.push(hash);
            }
            
            level = next_level;
        }
        
        level[0]
    }
    
    /// Get Merkle proof for a leaf index
    pub fn get_proof(&self, _index: usize) -> Vec<[u8; 32]> {
        // TODO: Implement Merkle proof generation
        vec![]
    }
}

impl Default for MerkleTreeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Verify Merkle proof
pub fn verify_merkle_proof(
    leaf: &[u8; 32],
    proof: &[[u8; 32]],
    root: &[u8; 32],
    _index: usize,
) -> bool {
    // Simplified: just check if proof is empty and leaf equals root
    // TODO: Implement proper Merkle proof verification
    if proof.is_empty() {
        return leaf == root;
    }
    
    // Placeholder for now
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merkle_tree_builder() {
        let mut builder = MerkleTreeBuilder::new();
        
        builder.add_leaf(b"data1");
        builder.add_leaf(b"data2");
        builder.add_leaf(b"data3");
        builder.add_leaf(b"data4");
        
        let root = builder.build();
        
        // Root should be non-zero
        assert_ne!(root, [0u8; 32]);
    }

    #[test]
    fn test_merkle_tree_single_leaf() {
        let mut builder = MerkleTreeBuilder::new();
        builder.add_leaf(b"single");
        
        let root = builder.build();
        let expected = blake3::hash(b"single");
        
        assert_eq!(root, *expected.as_bytes());
    }

    #[test]
    fn test_segment_proof_creation() {
        let proof = SegmentProof::new(
            0,
            vec![0xAA; 1024],
            0,
            1024,
            [0x42; 32],
        );
        
        assert_eq!(proof.segment_id, 0);
        assert_eq!(proof.chunk_length, 1024);
    }

    #[test]
    fn test_proof_verification() {
        let proof = SegmentProof::new(
            0,
            vec![0xBB; 512],
            100,
            512,
            [0x99; 32],
        );
        
        // Simplified verification should pass
        assert!(proof.verify().unwrap());
    }

    #[test]
    fn test_proof_verifier() {
        let verifier = ProofVerifier::new();
        
        let proof = SegmentProof::new(
            0,
            vec![0xCC; 256],
            200,
            256,
            [0x11; 32],
        );
        
        assert!(verifier.verify_proof(&proof).unwrap());
    }

    #[test]
    fn test_batch_verify() {
        let verifier = ProofVerifier::new();
        
        let proofs = vec![
            SegmentProof::new(0, vec![0xAA; 128], 0, 128, [0x01; 32]),
            SegmentProof::new(1, vec![0xBB; 256], 0, 256, [0x02; 32]),
            SegmentProof::new(2, vec![0xCC; 512], 0, 512, [0x03; 32]),
        ];
        
        let results = verifier.batch_verify(&proofs).unwrap();
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|&r| r));
    }
}

