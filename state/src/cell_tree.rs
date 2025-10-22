// SPDX-License-Identifier: ISC
// Copyright (C) 2024 Tondi developers
//
// Cell State Tree - Merkle tree for live cells
// Provides state root for lightweight client verification

use tondi_hashes::{Blake3Hasher, Hash, Hasher};
use std::collections::BTreeMap;

/// Cell state entry in the tree
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellEntry {
    /// Cell capacity
    pub capacity: u64,
    /// Lock script hash
    pub lock_hash: Hash,
    /// Type script hash (if present)
    pub type_hash: Option<Hash>,
    /// Data hash
    pub data_hash: Hash,
}

impl CellEntry {
    /// Create a new cell entry
    pub fn new(capacity: u64, lock_hash: Hash, type_hash: Option<Hash>, data_hash: Hash) -> Self {
        Self {
            capacity,
            lock_hash,
            type_hash,
            data_hash,
        }
    }

    /// Serialize cell entry for hashing
    pub fn serialize(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        
        // Capacity (8 bytes)
        bytes.extend_from_slice(&self.capacity.to_le_bytes());
        
        // Lock hash (32 bytes)
        bytes.extend_from_slice(self.lock_hash.as_bytes());
        
        // Type hash (1 byte flag + 32 bytes if present)
        if let Some(ref type_hash) = self.type_hash {
            bytes.push(1);
            bytes.extend_from_slice(type_hash.as_bytes());
        } else {
            bytes.push(0);
        }
        
        // Data hash (32 bytes)
        bytes.extend_from_slice(self.data_hash.as_bytes());
        
        bytes
    }

    /// Hash the cell entry
    pub fn hash(&self) -> Hash {
        let serialized = self.serialize();
        let mut hasher = Blake3Hasher::new();
        hasher.update(b"tondi-cell/entry");  // Domain separation
        hasher.update(&serialized);
        hasher.finalize()
    }
}

/// Cell State Tree - Sparse Merkle Tree for live cells
/// 
/// This tree maintains the state of all live (unspent) cells.
/// The tree is keyed by OutPoint hash and stores cell metadata.
/// 
/// Features:
/// - Sparse Merkle Tree structure (256-bit keys)
/// - Incremental updates (add/remove cells)
/// - Efficient state root calculation
/// - Support for Merkle proofs
pub struct CellStateTree {
    /// Cells indexed by outpoint hash
    cells: BTreeMap<Hash, CellEntry>,
    
    /// Cached root (invalidated on updates)
    cached_root: Option<Hash>,
}

impl CellStateTree {
    /// Create a new empty cell state tree
    pub fn new() -> Self {
        Self {
            cells: BTreeMap::new(),
            cached_root: None,
        }
    }

    /// Insert a cell into the tree
    pub fn insert(&mut self, outpoint_hash: Hash, entry: CellEntry) {
        self.cells.insert(outpoint_hash, entry);
        self.cached_root = None; // Invalidate cache
    }

    /// Remove a cell from the tree
    pub fn remove(&mut self, outpoint_hash: &Hash) -> Option<CellEntry> {
        let result = self.cells.remove(outpoint_hash);
        if result.is_some() {
            self.cached_root = None; // Invalidate cache
        }
        result
    }

    /// Get a cell from the tree
    pub fn get(&self, outpoint_hash: &Hash) -> Option<&CellEntry> {
        self.cells.get(outpoint_hash)
    }

    /// Get the number of cells in the tree
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// Check if the tree is empty
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Calculate the Merkle root of the cell tree
    /// 
    /// For now, we use a simple approach:
    /// 1. Sort all (outpoint_hash, cell_hash) pairs by outpoint_hash
    /// 2. Build a binary Merkle tree
    /// 
    /// Future optimization: Use incremental Merkle tree (e.g., Jellyfish Merkle Tree)
    pub fn root(&mut self) -> Hash {
        // Return cached root if available
        if let Some(ref root) = self.cached_root {
            return *root;
        }

        // Empty tree has zero hash
        if self.cells.is_empty() {
            return Hash::from_bytes([0u8; 32]);
        }

        // Collect (outpoint_hash, cell_hash) pairs, sorted by outpoint_hash
        let mut entries: Vec<(Hash, Hash)> = self.cells
            .iter()
            .map(|(outpoint_hash, cell)| (*outpoint_hash, cell.hash()))
            .collect();
        
        entries.sort_by(|a, b| a.0.cmp(&b.0));

        // Build binary Merkle tree bottom-up
        let mut current_level: Vec<Hash> = entries
            .into_iter()
            .map(|(outpoint, cell_hash)| {
                // Leaf = H("tondi-cell/leaf" || outpoint || cell_hash)
                let mut hasher = Blake3Hasher::new();
                hasher.update(b"tondi-cell/leaf");
                hasher.update(outpoint.as_bytes());
                hasher.update(cell_hash.as_bytes());
                hasher.finalize()
            })
            .collect();

        // Build tree upwards
        while current_level.len() > 1 {
            let mut next_level = Vec::new();
            
            for chunk in current_level.chunks(2) {
                let hash = if chunk.len() == 2 {
                    // Internal node = H("tondi-cell/node" || left || right)
                    let mut hasher = Blake3Hasher::new();
                    hasher.update(b"tondi-cell/node");
                    hasher.update(chunk[0].as_bytes());
                    hasher.update(chunk[1].as_bytes());
                    hasher.finalize()
                } else {
                    // Odd number, promote the single node
                    chunk[0]
                };
                
                next_level.push(hash);
            }
            
            current_level = next_level;
        }

        let root = current_level[0];
        self.cached_root = Some(root);
        root
    }

    /// Clear the tree
    pub fn clear(&mut self) {
        self.cells.clear();
        self.cached_root = None;
    }

    /// Get all cell entries (for iteration)
    pub fn iter(&self) -> impl Iterator<Item = (&Hash, &CellEntry)> {
        self.cells.iter()
    }
}

impl Default for CellStateTree {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_entry(capacity: u64) -> CellEntry {
        CellEntry::new(
            capacity,
            Hash::from_bytes([1u8; 32]),
            None,
            Hash::from_bytes([2u8; 32]),
        )
    }

    #[test]
    fn test_cell_entry_serialization() {
        let entry = create_test_entry(100000);
        let serialized = entry.serialize();
        
        // 8 (capacity) + 32 (lock) + 1 (type flag) + 32 (data) = 73 bytes
        assert_eq!(serialized.len(), 73);
        
        // Verify capacity
        assert_eq!(&serialized[0..8], &100000u64.to_le_bytes());
        
        // Verify no type script
        assert_eq!(serialized[40], 0);
    }

    #[test]
    fn test_cell_entry_with_type_script() {
        let entry = CellEntry::new(
            100000,
            Hash::from_bytes([1u8; 32]),
            Some(Hash::from_bytes([3u8; 32])),
            Hash::from_bytes([2u8; 32]),
        );
        
        let serialized = entry.serialize();
        
        // 8 + 32 + 1 + 32 (type) + 32 (data) = 105 bytes
        assert_eq!(serialized.len(), 105);
        
        // Verify type script present
        assert_eq!(serialized[40], 1);
        assert_eq!(&serialized[41..73], &[3u8; 32]);
    }

    #[test]
    fn test_cell_entry_hash() {
        let entry1 = create_test_entry(100000);
        let entry2 = create_test_entry(100000);
        let entry3 = create_test_entry(200000);
        
        // Same entries should have same hash
        assert_eq!(entry1.hash(), entry2.hash());
        
        // Different entries should have different hash
        assert_ne!(entry1.hash(), entry3.hash());
    }

    #[test]
    fn test_empty_tree() {
        let mut tree = CellStateTree::new();
        assert_eq!(tree.len(), 0);
        assert!(tree.is_empty());
        
        // Empty tree root is zero hash
        assert_eq!(tree.root(), Hash::from_bytes([0u8; 32]));
    }

    #[test]
    fn test_insert_and_get() {
        let mut tree = CellStateTree::new();
        let outpoint = Hash::from_bytes([10u8; 32]);
        let entry = create_test_entry(100000);
        
        tree.insert(outpoint, entry.clone());
        
        assert_eq!(tree.len(), 1);
        assert_eq!(tree.get(&outpoint), Some(&entry));
    }

    #[test]
    fn test_remove() {
        let mut tree = CellStateTree::new();
        let outpoint = Hash::from_bytes([10u8; 32]);
        let entry = create_test_entry(100000);
        
        tree.insert(outpoint, entry.clone());
        assert_eq!(tree.len(), 1);
        
        let removed = tree.remove(&outpoint);
        assert_eq!(removed, Some(entry));
        assert_eq!(tree.len(), 0);
        assert!(tree.is_empty());
    }

    #[test]
    fn test_single_cell_root() {
        let mut tree = CellStateTree::new();
        let outpoint = Hash::from_bytes([10u8; 32]);
        let entry = create_test_entry(100000);
        
        tree.insert(outpoint, entry);
        
        let root = tree.root();
        assert_ne!(root, Hash::from_bytes([0u8; 32]));
        
        // Root should be cached
        let root2 = tree.root();
        assert_eq!(root, root2);
    }

    #[test]
    fn test_multiple_cells_root() {
        let mut tree = CellStateTree::new();
        
        // Insert 3 cells
        for i in 0..3 {
            let outpoint = Hash::from_bytes([i as u8; 32]);
            let entry = create_test_entry(100000 + i as u64);
            tree.insert(outpoint, entry);
        }
        
        assert_eq!(tree.len(), 3);
        
        let root = tree.root();
        assert_ne!(root, Hash::from_bytes([0u8; 32]));
    }

    #[test]
    fn test_root_changes_on_modification() {
        let mut tree = CellStateTree::new();
        let outpoint1 = Hash::from_bytes([1u8; 32]);
        let outpoint2 = Hash::from_bytes([2u8; 32]);
        
        tree.insert(outpoint1, create_test_entry(100000));
        let root1 = tree.root();
        
        tree.insert(outpoint2, create_test_entry(200000));
        let root2 = tree.root();
        
        // Root should change after insertion
        assert_ne!(root1, root2);
        
        tree.remove(&outpoint2);
        let root3 = tree.root();
        
        // Root should return to original after removal
        assert_eq!(root1, root3);
    }

    #[test]
    fn test_deterministic_root() {
        // Same cells in different order should produce same root
        let mut tree1 = CellStateTree::new();
        let mut tree2 = CellStateTree::new();
        
        let cells = vec![
            (Hash::from_bytes([1u8; 32]), create_test_entry(100)),
            (Hash::from_bytes([2u8; 32]), create_test_entry(200)),
            (Hash::from_bytes([3u8; 32]), create_test_entry(300)),
        ];
        
        // Insert in forward order
        for (outpoint, entry) in &cells {
            tree1.insert(*outpoint, entry.clone());
        }
        
        // Insert in reverse order
        for (outpoint, entry) in cells.iter().rev() {
            tree2.insert(*outpoint, entry.clone());
        }
        
        assert_eq!(tree1.root(), tree2.root());
    }
}

