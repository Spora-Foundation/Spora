// SPDX-License-Identifier: ISC
// Copyright (C) 2025Tondi developers
//
// Deterministic conflict resolution

use crate::celltx::types::CellTx;
use crate::celltx::sighash::compute_wtxid;
use std::cmp::Ordering;

/// Conflict resolution key (deterministic ordering)
///
/// Priority order:
/// 1. fee_density ↓ (higher is better)
/// 2. blue_preference ↑ (more blue blocks preferred)
/// 3. wtxid ↑ (lexicographic tiebreaker)
#[derive(Debug, Clone, PartialEq)]
pub struct ConflictKey {
    /// Fee density: fee / effective_size (considering cycles)
    pub fee_density: OrderedFloat,
    /// Blue preference: sum of blue scores of dependencies
    pub blue_preference: u64,
    /// Witness transaction ID (tiebreaker)
    pub wtxid: [u8; 32],
}

impl Eq for ConflictKey {}

impl Ord for ConflictKey {
    fn cmp(&self, other: &Self) -> Ordering {
        // 1. Fee density (descending: higher is better)
        other.fee_density.cmp(&self.fee_density)
            // 2. Blue preference (descending: higher is better)
            .then(other.blue_preference.cmp(&self.blue_preference))
            // 3. wtxid (ascending: for determinism)
            .then(self.wtxid.cmp(&other.wtxid))
    }
}

impl PartialOrd for ConflictKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Ordered float wrapper for deterministic comparison
#[derive(Debug, Clone, Copy)]
pub struct OrderedFloat(pub f64);

impl PartialEq for OrderedFloat {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for OrderedFloat {}

impl PartialOrd for OrderedFloat {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrderedFloat {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.partial_cmp(&other.0).unwrap_or(Ordering::Equal)
    }
}

/// Conflict resolver
pub struct ConflictResolver {
    /// Cycles per byte (for effective size calculation)
    cycles_per_byte: f64,
}

impl Default for ConflictResolver {
    fn default() -> Self {
        Self {
            cycles_per_byte: 100.0, // Standard conversion factor
        }
    }
}

impl ConflictResolver {
    /// Create a new conflict resolver
    pub fn new(cycles_per_byte: f64) -> Self {
        Self { cycles_per_byte }
    }
    
    /// Compute conflict key for a transaction
    ///
    /// # Parameters
    /// - `tx`: The transaction
    /// - `fee`: Transaction fee (input_capacity - output_capacity)
    /// - `cycles`: Estimated execution cycles
    /// - `blue_score`: Optional blue score (for DAG preference)
    pub fn compute_key(
        &self,
        tx: &CellTx,
        fee: u64,
        cycles: u64,
        blue_score: Option<u64>,
    ) -> ConflictKey {
        let size = tx.serialized_size() as f64;
        let cycles_size = cycles as f64 / self.cycles_per_byte;
        let effective_size = size.max(cycles_size);
        
        let fee_density = if effective_size > 0.0 {
            fee as f64 / effective_size
        } else {
            0.0
        };
        
        ConflictKey {
            fee_density: OrderedFloat(fee_density),
            blue_preference: blue_score.unwrap_or(0),
            wtxid: compute_wtxid(tx),
        }
    }
    
    /// Resolve conflict between two transactions
    ///
    /// Returns the winner (transaction with higher priority)
    pub fn resolve(
        &self,
        tx1: &CellTx,
        fee1: u64,
        cycles1: u64,
        blue1: Option<u64>,
        tx2: &CellTx,
        fee2: u64,
        cycles2: u64,
        blue2: Option<u64>,
    ) -> ConflictResolution {
        let key1 = self.compute_key(tx1, fee1, cycles1, blue1);
        let key2 = self.compute_key(tx2, fee2, cycles2, blue2);
        
        match key1.cmp(&key2) {
            Ordering::Less => ConflictResolution::KeepFirst, // key1 wins (higher priority)
            Ordering::Greater => ConflictResolution::KeepSecond, // key2 wins (higher priority)
            Ordering::Equal => ConflictResolution::KeepFirst, // Should never happen
        }
    }
    
    /// Select winners from a set of conflicting transactions
    ///
    /// Returns indices of transactions to keep (sorted by priority)
    pub fn select_winners(
        &self,
        txs: &[(CellTx, u64, u64, Option<u64>)], // (tx, fee, cycles, blue_score)
    ) -> Vec<usize> {
        let mut entries: Vec<(usize, ConflictKey)> = txs
            .iter()
            .enumerate()
            .map(|(i, (tx, fee, cycles, blue))| {
                (i, self.compute_key(tx, *fee, *cycles, *blue))
            })
            .collect();
        
        // Sort by conflict key (highest priority first)
        entries.sort_by(|(_, k1), (_, k2)| k1.cmp(k2));
        
        // Return indices in priority order
        entries.into_iter().map(|(i, _)| i).collect()
    }
}

/// Conflict resolution result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictResolution {
    /// Keep the first transaction
    KeepFirst,
    /// Keep the second transaction
    KeepSecond,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::celltx::types::{CellRef, CellOut, ScriptRef, OutPoint};
    
    fn create_test_tx(capacity: u64) -> CellTx {
        let lock = ScriptRef::new([0x00; 32], 0, vec![]);
        CellTx::new(
            vec![],
            vec![],
            vec![CellOut { lock, type_: None, capacity }],
            vec![vec![]],
            vec![],
        ).unwrap()
    }
    
    #[test]
    fn test_conflict_key_ordering() {
        let resolver = ConflictResolver::default();
        
        // tx1: higher fee density
        let tx1 = create_test_tx(1000);
        let key1 = resolver.compute_key(&tx1, 1000, 1000, None);
        
        // tx2: lower fee density
        let tx2 = create_test_tx(1000);
        let key2 = resolver.compute_key(&tx2, 500, 1000, None);
        
        // key1 should win (higher fee density)
        assert!(key1 < key2); // Note: < because we want descending order
    }
    
    #[test]
    fn test_blue_preference_tiebreak() {
        let resolver = ConflictResolver::default();
        
        let tx = create_test_tx(1000);
        
        // Same fee density, different blue scores
        let key1 = resolver.compute_key(&tx, 1000, 1000, Some(100));
        let key2 = resolver.compute_key(&tx, 1000, 1000, Some(50));
        
        // key1 should win (higher blue score)
        assert!(key1 < key2);
    }
    
    #[test]
    fn test_wtxid_tiebreak() {
        let resolver = ConflictResolver::default();
        
        let tx1 = create_test_tx(1000);
        let tx2 = create_test_tx(1001); // Different capacity → different wtxid
        
        // Same fee and cycles, no blue score
        let key1 = resolver.compute_key(&tx1, 1000, 1000, None);
        let key2 = resolver.compute_key(&tx2, 1000, 1000, None);
        
        // Should be deterministically ordered by wtxid
        assert_ne!(key1, key2);
        assert!(key1 < key2 || key2 < key1);
    }
    
    #[test]
    fn test_resolve_conflict() {
        let resolver = ConflictResolver::default();
        
        let tx1 = create_test_tx(1000);
        let tx2 = create_test_tx(1001); // Different capacity to get different wtxid
        
        // tx1 has MUCH higher fee (to dominate wtxid tiebreaker)
        let result = resolver.resolve(
            &tx1, 10000, 1000, None,  // Very high fee
            &tx2, 1000, 1000, None,   // Low fee
        );
        
        // tx1 should win due to much higher fee density
        assert_eq!(result, ConflictResolution::KeepFirst);
    }
    
    #[test]
    fn test_select_winners() {
        let resolver = ConflictResolver::default();
        
        let tx1 = create_test_tx(1000);
        let tx2 = create_test_tx(1001);
        let tx3 = create_test_tx(1002);
        
        let txs = vec![
            (tx1, 500, 1000, None),  // Lowest fee
            (tx2, 2000, 1000, None), // Highest fee
            (tx3, 1000, 1000, None), // Middle fee
        ];
        
        let winners = resolver.select_winners(&txs);
        
        // Should be sorted by priority: tx2 (idx 1), tx3 (idx 2), tx1 (idx 0)
        assert_eq!(winners[0], 1); // tx2 wins
    }
    
    #[test]
    fn test_effective_size_with_cycles() {
        let resolver = ConflictResolver::new(100.0);
        
        let tx = create_test_tx(1000);
        let size = tx.serialized_size() as f64;
        
        // High cycles → effective size dominated by cycles
        let key_high_cycles = resolver.compute_key(&tx, 1000, 100000, None);
        
        // Low cycles → effective size dominated by serialized size
        let key_low_cycles = resolver.compute_key(&tx, 1000, 100, None);
        
        // High cycles tx should have lower fee density (larger effective size)
        assert!(key_high_cycles.fee_density.0 < key_low_cycles.fee_density.0);
    }
}


