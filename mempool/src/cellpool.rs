// SPDX-License-Identifier: ISC
// Copyright (C) 2025Tondi developers
//
// CellPool: Cell transaction memory pool

use crate::{Result, MempoolError, TransactionScore, TransactionScorer};
use indexmap::IndexMap;
use parking_lot::RwLock;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use tondi_exec::{CellTx, OutPoint};

/// Pool entry metadata
#[derive(Clone, Debug)]
pub struct PoolEntry {
    /// Transaction
    pub tx: CellTx,
    /// Transaction hash (wtxid)
    pub wtxid: [u8; 32],
    /// Transaction score
    pub score: TransactionScore,
    /// Entry timestamp
    pub timestamp: u64,
    /// Fee (capacity)
    pub fee: u64,
    /// Estimated cycles
    pub cycles: u64,
    /// Blue preference score (for tie-breaking)
    pub blue_score: Option<u64>,
    /// Dependencies (parent wtxids)
    pub dependencies: Vec<[u8; 32]>,
    /// Dependents (child wtxids)
    pub dependents: Vec<[u8; 32]>,
}

/// Deterministic conflict resolution key
///
/// Priority order:
/// 1. fee_density (higher better) - descending
/// 2. blue_score (higher better) - descending  
/// 3. wtxid (lexicographic) - ascending (tie-breaker)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConflictKey {
    /// Negative fee density (for descending order)
    /// Uses fixed-point: multiply by 10^9 for precision
    pub neg_fee_density: u64,
    /// Negative blue score (for descending order)
    pub neg_blue_score: u64,
    /// WTxID (ascending order for determinism)
    pub wtxid: [u8; 32],
}

impl ConflictKey {
    /// Create conflict key from pool entry
    pub fn from_entry(entry: &PoolEntry) -> Self {
        // Convert fee_density to fixed-point u64 (multiply by 10^9)
        let fee_density_fp = (entry.score.fee_density * 1_000_000_000.0) as u64;
        
        Self {
            neg_fee_density: u64::MAX - fee_density_fp, // Negate for descending order
            neg_blue_score: u64::MAX - entry.blue_score.unwrap_or(0),
            wtxid: entry.wtxid,
        }
    }
    
    /// Check if this key is better than another (higher priority)
    pub fn is_better_than(&self, other: &Self) -> bool {
        self < other  // Lower ConflictKey = higher priority (due to negation)
    }
}

/// Cell transaction memory pool
///
/// Features:
/// - Priority queue by score
/// - Dependency tracking
/// - RBF (Replace-By-Fee) support
/// - CPFP (Child-Pays-For-Parent) support
pub struct CellPool {
    /// Transactions: wtxid → PoolEntry
    txs: Arc<RwLock<IndexMap<[u8; 32], PoolEntry>>>,
    
    /// OutPoint → wtxid (for conflict detection)
    spent_outputs: Arc<RwLock<BTreeMap<OutPoint, [u8; 32]>>>,
    
    /// Transaction scorer
    scorer: Arc<TransactionScorer>,
    
    /// Maximum pool size
    max_size: usize,
    
    /// Statistics
    stats: Arc<RwLock<PoolStats>>,
}

/// Pool statistics
#[derive(Clone, Debug, Default)]
pub struct PoolStats {
    /// Total transactions
    pub total_txs: usize,
    /// Total size (bytes)
    pub total_size: usize,
    /// Total fee
    pub total_fee: u64,
    /// Transactions added
    pub txs_added: u64,
    /// Transactions removed
    pub txs_removed: u64,
    /// RBF replacements
    pub rbf_count: u64,
}

impl CellPool {
    /// Create a new CellPool
    pub fn new(max_size: usize) -> Self {
        Self {
            txs: Arc::new(RwLock::new(IndexMap::new())),
            spent_outputs: Arc::new(RwLock::new(BTreeMap::new())),
            scorer: Arc::new(TransactionScorer::default()),
            max_size,
            stats: Arc::new(RwLock::new(PoolStats::default())),
        }
    }
    
    /// Add a transaction to the pool
    ///
    /// Optional blue_score for GhostDAG tie-breaking (higher = more confirmed)
    pub fn add_with_blue_score(&self, tx: CellTx, fee: u64, cycles: u64, blue_score: Option<u64>) -> Result<[u8; 32]> {
        let wtxid = tondi_exec::celltx::sighash::compute_wtxid(&tx);
        
        // Check if already exists
        if self.txs.read().contains_key(&wtxid) {
            return Err(MempoolError::TxExists(wtxid));
        }
        
        // Check pool size
        if self.txs.read().len() >= self.max_size {
            return Err(MempoolError::MempoolFull(self.max_size));
        }
        
        // Check for conflicts (double-spend)
        let conflicts = self.check_conflicts(&tx)?;
        if !conflicts.is_empty() {
            // Try RBF with deterministic conflict resolution
            return self.try_replace_by_fee(&tx, wtxid, fee, cycles, blue_score, &conflicts);
        }
        
        // Compute score
        let score = self.scorer.compute_score(&tx, fee, cycles, blue_score);
        
        // Build dependencies
        let dependencies = self.find_dependencies(&tx);
        
        // Create entry
        let entry = PoolEntry {
            tx: tx.clone(),
            wtxid,
            score,
            timestamp: Self::current_timestamp(),
            fee,
            cycles,
            blue_score,
            dependencies: dependencies.clone(),
            dependents: Vec::new(),
        };
        
        // Add to pool
        let mut txs = self.txs.write();
        let mut spent = self.spent_outputs.write();
        let mut stats = self.stats.write();
        
        // Update spent outputs
        for input in &tx.inputs {
            spent.insert(input.out_point.clone(), wtxid);
        }
        
        // Update dependents in parent transactions
        for dep_wtxid in &dependencies {
            if let Some(parent) = txs.get_mut(dep_wtxid) {
                parent.dependents.push(wtxid);
            }
        }
        
        txs.insert(wtxid, entry);
        
        // Update stats
        stats.total_txs += 1;
        stats.total_size += tx.serialized_size();
        stats.total_fee += fee;
        stats.txs_added += 1;
        
        Ok(wtxid)
    }
    
    /// Remove a transaction from the pool
    pub fn remove(&self, wtxid: &[u8; 32]) -> Result<CellTx> {
        let mut txs = self.txs.write();
        let mut spent = self.spent_outputs.write();
        let mut stats = self.stats.write();
        
        let entry = txs.shift_remove(wtxid)
            .ok_or(MempoolError::TxNotFound(*wtxid))?;
        
        // Remove from spent outputs
        for input in &entry.tx.inputs {
            spent.remove(&input.out_point);
        }
        
        // Update dependents in parent transactions
        for dep_wtxid in &entry.dependencies {
            if let Some(parent) = txs.get_mut(dep_wtxid) {
                parent.dependents.retain(|id| id != wtxid);
            }
        }
        
        // Update stats
        stats.total_txs -= 1;
        stats.total_size -= entry.tx.serialized_size();
        stats.total_fee -= entry.fee;
        stats.txs_removed += 1;
        
        Ok(entry.tx)
    }
    
    /// Get a transaction by wtxid
    pub fn get(&self, wtxid: &[u8; 32]) -> Option<PoolEntry> {
        self.txs.read().get(wtxid).cloned()
    }
    
    /// Get transactions sorted by score (descending)
    pub fn get_sorted(&self, limit: usize) -> Vec<PoolEntry> {
        let txs = self.txs.read();
        let mut entries: Vec<_> = txs.values().cloned().collect();
        
        // Sort by score (descending)
        entries.sort_by(|a, b| b.score.total.partial_cmp(&a.score.total).unwrap());
        
        entries.into_iter().take(limit).collect()
    }
    
    /// Get pool statistics
    pub fn stats(&self) -> PoolStats {
        self.stats.read().clone()
    }
    
    /// Check for conflicting transactions
    fn check_conflicts(&self, tx: &CellTx) -> Result<Vec<[u8; 32]>> {
        let spent = self.spent_outputs.read();
        let mut conflicts = Vec::new();
        
        for input in &tx.inputs {
            if let Some(existing_wtxid) = spent.get(&input.out_point) {
                conflicts.push(*existing_wtxid);
            }
        }
        
        Ok(conflicts)
    }
    
    /// Try to replace by fee (RBF)
    /// Try to replace conflicting transactions with RBF
    ///
    /// Uses deterministic conflict resolution:
    /// Priority: fee_density (desc) → blue_score (desc) → wtxid (asc)
    fn try_replace_by_fee(
        &self,
        tx: &CellTx,
        wtxid: [u8; 32],
        fee: u64,
        cycles: u64,
        blue_score: Option<u64>,
        conflicts: &[[u8; 32]],
    ) -> Result<[u8; 32]> {
        let txs = self.txs.read();
        
        // Compute score for new transaction
        let new_score = self.scorer.compute_score(tx, fee, cycles, blue_score);
        let new_fee_density = new_score.fee_density; // Store for error message
        
        // Create conflict key for new transaction
        let new_entry_temp = PoolEntry {
            tx: tx.clone(),
            wtxid,
            score: new_score,
            timestamp: Self::current_timestamp(),
            fee,
            cycles,
            blue_score,
            dependencies: Vec::new(),
            dependents: Vec::new(),
        };
        let new_key = ConflictKey::from_entry(&new_entry_temp);
        
        // Check if new transaction beats ALL conflicts
        for conflict_id in conflicts {
            let conflict = txs.get(conflict_id)
                .ok_or(MempoolError::TxNotFound(*conflict_id))?;
            
            let conflict_key = ConflictKey::from_entry(conflict);
            
            // New transaction must be better than conflict
            if !new_key.is_better_than(&conflict_key) {
                return Err(MempoolError::RBFFailed(format!(
                    "New transaction (fee_density={:.2}, blue={:?}) does not beat conflict (fee_density={:.2}, blue={:?})",
                    new_fee_density,
                    blue_score,
                    conflict.score.fee_density,
                    conflict.blue_score,
                )));
            }
        }
        
        drop(txs);
        
        // Remove all conflicts
        for conflict_id in conflicts {
            self.remove(conflict_id)?;
        }
        
        // Add new transaction
        self.stats.write().rbf_count += 1;
        self.add_with_blue_score(tx.clone(), fee, cycles, blue_score)
    }
    
    /// Find dependencies (parent transactions in pool)
    fn find_dependencies(&self, tx: &CellTx) -> Vec<[u8; 32]> {
        let txs = self.txs.read();
        let mut deps = BTreeSet::new();
        
        for input in &tx.inputs {
            // Check if input is produced by a transaction in pool
            for (parent_wtxid, parent_entry) in txs.iter() {
                let parent_hash = tondi_exec::celltx::sighash::compute_wtxid(&parent_entry.tx);
                if input.out_point.tx_hash == parent_hash {
                    deps.insert(*parent_wtxid);
                }
            }
        }
        
        deps.into_iter().collect()
    }
    
    /// Get current Unix timestamp
    fn current_timestamp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tondi_exec::{CellRef, CellOut, ScriptRef};

    fn create_test_tx(inputs: Vec<OutPoint>, capacity: u64) -> CellTx {
        let lock = ScriptRef::new([0x00; 32], 0, vec![0; 20]);
        let inputs = inputs.into_iter().map(|op| CellRef::new(op, 0)).collect();
        CellTx::new(
            inputs,
            vec![],
            vec![CellOut { lock, type_: None, capacity }],
            vec![vec![]],
            vec![vec![0; 65]],
        ).unwrap()
    }

    #[test]
    fn test_cellpool_add() {
        let pool = CellPool::new(100);
        
        let tx = create_test_tx(vec![], 1000);
        let wtxid = pool.add(tx, 100, 1000).unwrap();
        
        assert!(pool.get(&wtxid).is_some());
        
        let stats = pool.stats();
        assert_eq!(stats.total_txs, 1);
        assert_eq!(stats.txs_added, 1);
    }

    #[test]
    fn test_cellpool_remove() {
        let pool = CellPool::new(100);
        
        let tx = create_test_tx(vec![], 1000);
        let wtxid = pool.add(tx.clone(), 100, 1000).unwrap();
        
        let removed = pool.remove(&wtxid).unwrap();
        assert_eq!(removed.ver, tx.ver);
        
        assert!(pool.get(&wtxid).is_none());
    }

    #[test]
    fn test_cellpool_conflict_detection() {
        let pool = CellPool::new(100);
        
        let out_point = OutPoint::new([0x42; 32], 0);
        
        let tx1 = create_test_tx(vec![out_point.clone()], 1000);
        let tx2 = create_test_tx(vec![out_point], 2000);
        
        pool.add(tx1, 100, 1000).unwrap();
        
        // Should fail due to conflict (same input)
        let result = pool.add(tx2.clone(), 50, 1000);
        assert!(result.is_err());
    }

    #[test]
    fn test_cellpool_rbf() {
        let pool = CellPool::new(100);
        
        let out_point = OutPoint::new([0x42; 32], 0);
        
        let tx1 = create_test_tx(vec![out_point.clone()], 1000);
        let tx2 = create_test_tx(vec![out_point], 2000);
        
        pool.add(tx1, 100, 1000).unwrap();
        
        // RBF with higher fee should succeed
        let wtxid2 = pool.add(tx2, 200, 1000).unwrap();
        
        assert!(pool.get(&wtxid2).is_some());
        
        let stats = pool.stats();
        assert_eq!(stats.rbf_count, 1);
    }

    #[test]
    fn test_cellpool_sorted() {
        let pool = CellPool::new(100);
        
        let tx1 = create_test_tx(vec![], 1000);
        let tx2 = create_test_tx(vec![], 2000);
        let tx3 = create_test_tx(vec![], 1500);
        
        pool.add(tx1, 50, 1000).unwrap();   // Low fee
        pool.add(tx2, 200, 1000).unwrap();  // High fee
        pool.add(tx3, 100, 1000).unwrap();  // Medium fee
        
        let sorted = pool.get_sorted(10);
        
        // Should be sorted by score (fee density)
        assert!(sorted[0].fee >= sorted[1].fee);
        assert!(sorted[1].fee >= sorted[2].fee);
    }
}

