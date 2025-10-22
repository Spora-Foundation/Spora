use tondi_hashes::{Hash, Hasher, HasherBase, ZERO_HASH};

/// Cell state tree for computing cell_root
/// This is a Merkle tree of all live cells in the state
#[derive(Clone, Debug)]
pub struct CellStateTree {
    /// Live cells indexed by their ID (OutPoint)
    cells: Vec<(Hash, CellStateEntry)>,
}

/// Entry in the cell state tree
#[derive(Clone, Debug)]
pub struct CellStateEntry {
    /// Cell output data hash
    pub data_hash: Hash,
    /// Cell capacity
    pub capacity: u64,
    /// Lock script hash
    pub lock_hash: Hash,
    /// Type script hash (optional)
    pub type_hash: Option<Hash>,
}

impl CellStateTree {
    /// Create a new empty cell state tree
    pub fn new() -> Self {
        Self { cells: Vec::new() }
    }

    /// Add a cell to the state tree
    pub fn add_cell(&mut self, cell_id: Hash, entry: CellStateEntry) {
        self.cells.push((cell_id, entry));
    }

    /// Remove a cell from the state tree
    pub fn remove_cell(&mut self, cell_id: &Hash) {
        self.cells.retain(|(id, _)| id != cell_id);
    }

    /// Compute the Merkle root of all cells
    /// Returns ZERO_HASH if there are no cells
    pub fn compute_root(&self) -> Hash {
        if self.cells.is_empty() {
            return ZERO_HASH;
        }

        // Sort cells by ID for deterministic ordering
        let mut sorted_cells = self.cells.clone();
        sorted_cells.sort_by(|(a, _), (b, _)| a.cmp(b));

        // Compute leaf hashes
        let mut hashes: Vec<Hash> = sorted_cells
            .iter()
            .map(|(cell_id, entry)| hash_cell_entry(cell_id, entry))
            .collect();

        // Build Merkle tree bottom-up
        while hashes.len() > 1 {
            let mut next_level = Vec::new();
            for chunk in hashes.chunks(2) {
                if chunk.len() == 2 {
                    // Hash pair of nodes
                    next_level.push(hash_pair(&chunk[0], &chunk[1]));
                } else {
                    // Odd node, hash with itself
                    next_level.push(hash_pair(&chunk[0], &chunk[0]));
                }
            }
            hashes = next_level;
        }

        hashes[0]
    }

    /// Get the number of cells in the tree
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// Check if the tree is empty
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }
}

impl Default for CellStateTree {
    fn default() -> Self {
        Self::new()
    }
}

/// Hash a cell entry for inclusion in the Merkle tree
fn hash_cell_entry(cell_id: &Hash, entry: &CellStateEntry) -> Hash {
    let mut hasher = CellStateHash::new();
    hasher.update(b"entry");
    hasher.update(cell_id);
    hasher.update(entry.data_hash);
    hasher.update(entry.capacity.to_le_bytes());
    hasher.update(entry.lock_hash);
    if let Some(type_hash) = &entry.type_hash {
        hasher.update([1u8]); // Has type script
        hasher.update(type_hash);
    } else {
        hasher.update([0u8]); // No type script
    }
    hasher.finalize()
}

/// Hash a pair of nodes in the Merkle tree
fn hash_pair(left: &Hash, right: &Hash) -> Hash {
    let mut hasher = CellStateHash::new();
    hasher.update(b"node");
    hasher.update(left);
    hasher.update(right);
    hasher.finalize()
}

/// Domain-separated hasher for cell state tree
#[derive(Clone)]
struct CellStateHash(Vec<u8>);

impl CellStateHash {
    #[inline(always)]
    fn new() -> Self {
        Self(Vec::from(&b"CellStateTree"[..]))
    }

    fn write<A: AsRef<[u8]>>(&mut self, data: A) {
        self.0.extend_from_slice(data.as_ref());
    }

    #[inline(always)]
    fn finalize(self) -> Hash {
        // Use blake3 directly since tondi_hashes::blake3 is private
        (*blake3::hash(&self.0).as_bytes()).into()
    }
}

impl HasherBase for CellStateHash {
    #[inline(always)]
    fn update<A: AsRef<[u8]>>(&mut self, data: A) -> &mut Self {
        self.write(data);
        self
    }
}

impl Hasher for CellStateHash {
    #[inline(always)]
    fn finalize(self) -> Hash {
        CellStateHash::finalize(self)
    }
    
    #[inline(always)]
    fn reset(&mut self) {
        *self = Self::new();
    }
}

impl Default for CellStateHash {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_tree() {
        let tree = CellStateTree::new();
        assert_eq!(tree.compute_root(), ZERO_HASH);
        assert!(tree.is_empty());
        assert_eq!(tree.len(), 0);
    }

    #[test]
    fn test_single_cell() {
        let mut tree = CellStateTree::new();
        let cell_id = Hash::from_u64_word(1);
        let entry = CellStateEntry {
            data_hash: Hash::from_u64_word(2),
            capacity: 100,
            lock_hash: Hash::from_u64_word(3),
            type_hash: None,
        };
        tree.add_cell(cell_id, entry);
        
        let root = tree.compute_root();
        assert_ne!(root, ZERO_HASH);
        assert_eq!(tree.len(), 1);
    }

    #[test]
    fn test_multiple_cells() {
        let mut tree = CellStateTree::new();
        
        for i in 0..10 {
            let cell_id = Hash::from_u64_word(i);
            let entry = CellStateEntry {
                data_hash: Hash::from_u64_word(i * 2),
                capacity: 100 + i,
                lock_hash: Hash::from_u64_word(i * 3),
                type_hash: None,
            };
            tree.add_cell(cell_id, entry);
        }

        let root = tree.compute_root();
        assert_ne!(root, ZERO_HASH);
        assert_eq!(tree.len(), 10);
    }

    #[test]
    fn test_deterministic_ordering() {
        let mut tree1 = CellStateTree::new();
        let mut tree2 = CellStateTree::new();

        // Add cells in different order
        for i in 0..5 {
            let cell_id = Hash::from_u64_word(i);
            let entry = CellStateEntry {
                data_hash: Hash::from_u64_word(i * 2),
                capacity: 100,
                lock_hash: Hash::from_u64_word(i * 3),
                type_hash: None,
            };
            tree1.add_cell(cell_id, entry.clone());
        }

        for i in (0..5).rev() {
            let cell_id = Hash::from_u64_word(i);
            let entry = CellStateEntry {
                data_hash: Hash::from_u64_word(i * 2),
                capacity: 100,
                lock_hash: Hash::from_u64_word(i * 3),
                type_hash: None,
            };
            tree2.add_cell(cell_id, entry);
        }

        // Should produce same root regardless of insertion order
        assert_eq!(tree1.compute_root(), tree2.compute_root());
    }

    #[test]
    fn test_remove_cell() {
        let mut tree = CellStateTree::new();
        let cell_id = Hash::from_u64_word(1);
        let entry = CellStateEntry {
            data_hash: Hash::from_u64_word(2),
            capacity: 100,
            lock_hash: Hash::from_u64_word(3),
            type_hash: None,
        };
        tree.add_cell(cell_id, entry);
        
        assert_eq!(tree.len(), 1);
        tree.remove_cell(&cell_id);
        assert_eq!(tree.len(), 0);
        assert_eq!(tree.compute_root(), ZERO_HASH);
    }

    #[test]
    fn test_with_type_script() {
        let mut tree = CellStateTree::new();
        let cell_id = Hash::from_u64_word(1);
        let entry = CellStateEntry {
            data_hash: Hash::from_u64_word(2),
            capacity: 100,
            lock_hash: Hash::from_u64_word(3),
            type_hash: Some(Hash::from_u64_word(4)),
        };
        tree.add_cell(cell_id, entry);
        
        let root_with_type = tree.compute_root();

        let mut tree2 = CellStateTree::new();
        let entry2 = CellStateEntry {
            data_hash: Hash::from_u64_word(2),
            capacity: 100,
            lock_hash: Hash::from_u64_word(3),
            type_hash: None,
        };
        tree2.add_cell(cell_id, entry2);
        
        let root_without_type = tree2.compute_root();

        // Roots should be different
        assert_ne!(root_with_type, root_without_type);
    }
}

