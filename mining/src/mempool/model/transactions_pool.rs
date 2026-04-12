use crate::{
    feerate::{FeerateEstimator, FeerateEstimatorArgs},
    mempool::{
        config::Config,
        errors::{RuleError, RuleResult},
        model::{
            cell_set::MempoolCellSet,
            map::{CellTransactionIndex, MempoolTransactionCollection},
            pool::{Pool, TransactionsEdges},
            tx::{DoubleSpend, MempoolTransaction},
        },
        tx::Priority,
    },
    model::{candidate_tx::CandidateCellData, topological_index::TopologicalIndex, TransactionIdSet},
    Policy,
};
use spora_consensus_core::{
    block::TemplateTransactionSelector,
    tx::{MutableTransaction, TransactionId, TransactionOutpoint},
};
use spora_core::{debug, time::unix_now, trace};
use spora_mempool::CellPool;
use std::{
    collections::{hash_map::Keys, hash_set::Iter, HashMap, HashSet, VecDeque},
    iter::once,
    sync::Arc,
};

use super::frontier::Frontier;

/// Pool of transactions to be included in a block template
///
/// ### Rust rewrite notes
///
/// The main design decision is to have [MempoolTransaction]s owned by [all_transactions]
/// without any other external reference so no smart pointer is needed.
///
/// This has following consequences:
///
/// - highPriorityTransactions is dropped in favour of an in-place filtered iterator.
/// - MempoolTransaction.parentTransactionsInPool is moved here and replaced by a map from
///   an id to a set of parent transaction ids introducing an indirection stage when
///   a matching object is required.
/// - chainedTransactionsByParentID maps an id instead of a transaction reference
///   introducing a indirection stage when the matching object is required.
/// - Hash sets are used by parent_transaction_ids_in_pool and chained_transaction_ids_by_parent_id
///   instead of vectors to prevent duplicates.
/// - transactionsOrderedByFeeRate is dropped and replaced by an in-place vector
///   of low-priority transactions sorted by fee rates. This design might eventually
///   prove to be sub-optimal, in which case an index should be implemented, probably
///   requiring smart pointers eventually or an indirection stage too.
pub(crate) struct TransactionsPool {
    /// Mempool config
    config: Arc<Config>,

    /// Store of transactions.
    /// Any mutable access to this map should be carefully reviewed for consistency with all other collections
    /// and fields of this struct. In particular, `estimated_size` must reflect the exact sum of estimated size
    /// for all current transactions in this collection.
    all_transactions: MempoolTransactionCollection,

    /// Index from Cell transaction ids back to legacy transaction ids stored by the mempool.
    cell_transaction_ids: CellTransactionIndex,

    /// Index from Cell transaction witness ids back to legacy transaction ids stored by the mempool.
    cell_wtxids: HashMap<[u8; 32], TransactionId>,

    /// Transactions dependencies formed by inputs present in pool - ancestor relations.
    parent_transactions: TransactionsEdges,

    /// Transactions dependencies formed by outputs present in pool - successor relations.
    chained_transactions: TransactionsEdges,

    /// Transactions with no parents in the mempool -- ready to be inserted into a block template
    ready_transactions: Frontier,

    last_expire_scan_daa_score: u64,

    /// last expire scan time in milliseconds
    last_expire_scan_time: u64,

    /// Sum of estimated size for all transactions currently held in `all_transactions`
    estimated_size: usize,

    /// Transitional store of mempool-owned Cell outpoints.
    cell_set: MempoolCellSet,

    /// Cell-native mirror of the legacy transaction pool.
    cell_pool: CellPool,
}

impl TransactionsPool {
    pub(crate) fn new(config: Arc<Config>) -> Self {
        Self {
            config: config.clone(),
            all_transactions: MempoolTransactionCollection::default(),
            cell_transaction_ids: CellTransactionIndex::default(),
            cell_wtxids: HashMap::default(),
            parent_transactions: TransactionsEdges::default(),
            chained_transactions: TransactionsEdges::default(),
            ready_transactions: Default::default(),
            last_expire_scan_daa_score: 0,
            last_expire_scan_time: unix_now(),
            cell_set: MempoolCellSet::new(),
            cell_pool: CellPool::new(config.maximum_transaction_count),
            estimated_size: 0,
        }
    }

    fn build_cell_selector_snapshot(&self) -> HashMap<TransactionId, CandidateCellData> {
        self.all_transactions
            .values()
            .filter_map(|transaction| {
                transaction
                    .cell_wtxid()
                    .and_then(|wtxid| self.cell_pool.get(&wtxid))
                    .map(|entry| {
                        (
                            transaction.id(),
                            CandidateCellData {
                                cell_tx: Arc::new(entry.tx),
                                score_total: Some(entry.score.total),
                                fee_density: Some(entry.score.fee_density),
                                deps_width: Some(entry.score.deps_width),
                            },
                        )
                    })
                    .or_else(|| {
                        transaction.cell_tx().map(|cell_tx| {
                            (transaction.id(), CandidateCellData { cell_tx, score_total: None, fee_density: None, deps_width: None })
                        })
                    })
            })
            .collect()
    }

    fn build_parent_cell_id_context(&self, parent_ids: &TransactionIdSet) -> RuleResult<HashMap<TransactionId, TransactionId>> {
        let mut parent_cell_ids = HashMap::with_capacity(parent_ids.len());
        for parent_id in parent_ids {
            let parent = self.all_transactions.get(parent_id).ok_or_else(|| {
                RuleError::RejectCellMirror(*parent_id, "missing parent transaction while building Cell mirror".to_string())
            })?;
            let parent_cell_id = parent
                .cell_tx_id()
                .ok_or_else(|| RuleError::RejectCellMirror(*parent_id, "parent transaction is missing Cell mirror".to_string()))?;
            parent_cell_ids.insert(*parent_id, parent_cell_id);
        }
        Ok(parent_cell_ids)
    }

    fn get_effective_parent_transaction_ids_in_pool(&self, transaction: &MempoolTransaction) -> TransactionIdSet {
        let mut parents = self.get_parent_transaction_ids_in_pool(&transaction.mtx);

        if let Some(cell_tx) = transaction.cell_tx() {
            for input in &cell_tx.inputs {
                let cell_parent_id: TransactionId = input.out_point.tx_hash.into();
                if let Some(parent_id) = self.cell_transaction_ids.get(&cell_parent_id).copied() {
                    parents.insert(parent_id);
                }
            }
        }

        parents.remove(&transaction.id());
        parents
    }

    pub(crate) fn get_transaction_ids_with_parent_transaction(&self, parent_transaction_id: &TransactionId) -> Vec<TransactionId> {
        self.all_transactions
            .values()
            .filter(|tx| self.get_effective_parent_transaction_ids_in_pool(tx).contains(parent_transaction_id))
            .map(MempoolTransaction::id)
            .collect()
    }

    fn has_remaining_cell_dependencies(&self, transaction: &MempoolTransaction) -> bool {
        transaction
            .cell_wtxid()
            .and_then(|wtxid| self.cell_pool.get(&wtxid))
            .map(|entry| !entry.dependencies.is_empty())
            .unwrap_or(false)
    }

    fn refresh_ready_status(&mut self, transaction_id: TransactionId) {
        let is_ready = self
            .all_transactions
            .get(&transaction_id)
            .map(|tx| self.get_effective_parent_transaction_ids_in_pool(tx).is_empty() && !self.has_remaining_cell_dependencies(tx))
            .unwrap_or(false);

        if let Some(tx) = self.all_transactions.get(&transaction_id) {
            if is_ready {
                self.ready_transactions.insert(tx.into());
            } else {
                self.ready_transactions.remove(&(tx.into()));
            }
        }
    }

    fn get_cell_redeemer_ids_in_pool(&self, transaction_id: &TransactionId) -> Vec<TransactionId> {
        let Some(root_wtxid) = self.all_transactions.get(transaction_id).and_then(|transaction| transaction.cell_wtxid()) else {
            return vec![];
        };

        let mut visited_wtxids = HashSet::from([root_wtxid]);
        let mut visited_ids = TransactionIdSet::new();
        let mut descendants = vec![];
        let mut queue = VecDeque::from([root_wtxid]);

        while let Some(current_wtxid) = queue.pop_front() {
            let Some(entry) = self.cell_pool.get(&current_wtxid) else {
                continue;
            };

            for child_wtxid in entry.dependents {
                if !visited_wtxids.insert(child_wtxid) {
                    continue;
                }

                if let Some(child_id) = self.cell_wtxids.get(&child_wtxid).copied() {
                    if visited_ids.insert(child_id) {
                        descendants.push(child_id);
                    }
                    queue.push_back(child_wtxid);
                }
            }
        }

        descendants
    }

    pub(crate) fn get_redeemer_ids_in_pool(&self, transaction_id: &TransactionId) -> Vec<TransactionId> {
        let mut visited = TransactionIdSet::new();
        let mut descendants = vec![];

        for descendant in <Self as Pool>::get_redeemer_ids_in_pool(self, transaction_id)
            .into_iter()
            .chain(self.get_cell_redeemer_ids_in_pool(transaction_id))
        {
            if visited.insert(descendant) {
                descendants.push(descendant);
            }
        }

        descendants
    }

    fn add_cell_mirror(&self, transaction: &mut MempoolTransaction) -> RuleResult<()> {
        let cell_tx = transaction
            .cell_tx()
            .ok_or_else(|| RuleError::RejectCellMirror(transaction.id(), "legacy transaction conversion failed".to_string()))?;
        let fee = transaction
            .mtx
            .calculated_fee
            .ok_or_else(|| RuleError::RejectCellMirror(transaction.id(), "transaction fee was not populated".to_string()))?;
        let cycles = cell_tx.compute_mass();
        let wtxid = self
            .cell_pool
            .add(cell_tx.as_ref().clone(), fee, cycles)
            .map_err(|err| RuleError::RejectCellMirror(transaction.id(), err.to_string()))?;
        transaction.cell_wtxid = Some(wtxid);
        Ok(())
    }

    #[allow(dead_code)]
    /// Add a mutable transaction to the pool
    pub(crate) fn add_transaction(
        &mut self,
        transaction: MutableTransaction,
        virtual_daa_score: u64,
        priority: Priority,
        transaction_size: usize,
    ) -> RuleResult<&MempoolTransaction> {
        let transaction = MempoolTransaction::new(transaction, priority, virtual_daa_score);
        let id = transaction.id();
        self.add_mempool_transaction(transaction, transaction_size)?;
        Ok(self.get(&id).unwrap())
    }

    /// Add a mempool transaction to the pool
    pub(crate) fn add_mempool_transaction(
        &mut self,
        transaction: MempoolTransaction,
        transaction_size: usize,
    ) -> RuleResult<&MempoolTransaction> {
        let mut transaction = transaction;
        let id = transaction.id();

        assert!(!self.all_transactions.contains_key(&id), "transaction {id} to be added already exists in the transactions pool");
        assert!(transaction.mtx.is_fully_populated(), "transaction {id} to be added in the transactions pool is not fully populated");

        // Create the bijective parent/chained relations.
        // This concerns only the parents of the added transaction.
        // The transactions chained to the added transaction cannot be stored
        // here yet since, by definition, they would have been orphans.
        let legacy_parents = self.get_parent_transaction_ids_in_pool(&transaction.mtx);
        let parent_cell_ids = self.build_parent_cell_id_context(&legacy_parents)?;
        transaction.refresh_cell_mirror_with_context(&parent_cell_ids);
        self.add_cell_mirror(&mut transaction)?;
        let parents = self.get_effective_parent_transaction_ids_in_pool(&transaction);
        self.parent_transactions.insert(id, parents.clone());
        for parent_id in parents {
            let entry = self.chained_transactions.entry(parent_id).or_default();
            entry.insert(id);
        }
        if let Some(cell_tx_id) = transaction.cell_tx_id() {
            self.cell_transaction_ids.insert(cell_tx_id, id);
        }
        if let Some(cell_wtxid) = transaction.cell_wtxid() {
            self.cell_wtxids.insert(cell_wtxid, id);
        }

        self.cell_set.add_transaction(&transaction.mtx);
        self.estimated_size += transaction_size;
        self.all_transactions.insert(id, transaction);
        self.refresh_ready_status(id);
        trace!("Added transaction {}", id);
        Ok(self.get(&id).unwrap())
    }

    /// Fully removes the transaction from all relational sets, as well as from the transitional Cell set
    pub(crate) fn remove_transaction(&mut self, transaction_id: &TransactionId) -> RuleResult<MempoolTransaction> {
        let mut cell_dependents = Vec::new();
        let legacy_chains = self.chained_transactions.get(transaction_id).cloned().unwrap_or_default();
        if let Some(cell_wtxid) = self.all_transactions.get(transaction_id).and_then(|transaction| transaction.cell_wtxid()) {
            cell_dependents = self.cell_pool.get(&cell_wtxid).map(|entry| entry.dependents).unwrap_or_default();
            self.cell_pool.remove(&cell_wtxid).map_err(|err| RuleError::RejectCellMirror(*transaction_id, err.to_string()))?;
        }

        // Remove all bijective parent/chained relations
        if let Some(parents) = self.parent_transactions.get(transaction_id) {
            for parent in parents.iter() {
                if let Some(chains) = self.chained_transactions.get_mut(parent) {
                    chains.remove(transaction_id);
                }
            }
        }
        if let Some(chains) = self.chained_transactions.get(transaction_id) {
            for chain in chains.iter() {
                if let Some(parents) = self.parent_transactions.get_mut(chain) {
                    parents.remove(transaction_id);
                }
            }
        }
        self.parent_transactions.remove(transaction_id);
        self.chained_transactions.remove(transaction_id);

        // Remove the transaction itself
        let removed_tx = self.all_transactions.remove(transaction_id).ok_or(RuleError::RejectMissingTransaction(*transaction_id))?;
        if let Some(cell_tx_id) = removed_tx.cell_tx_id() {
            self.cell_transaction_ids.remove(&cell_tx_id);
        }
        if let Some(cell_wtxid) = removed_tx.cell_wtxid() {
            self.cell_wtxids.remove(&cell_wtxid);
        }

        self.ready_transactions.remove(&(&removed_tx).into());

        // TODO: consider using `self.parent_transactions.get(transaction_id)`
        // The tradeoff to consider is whether it might be possible that a parent tx exists in the pool
        // however its relation as parent is not registered. This can supposedly happen in rare cases where
        // the parent was removed w/o redeemers and then re-added
        let parent_ids = self.get_parent_transaction_ids_in_pool(&removed_tx.mtx);

        // Remove the transaction from the transitional mempool Cell set
        self.cell_set.remove_transaction(&removed_tx.mtx, &parent_ids);
        self.estimated_size -= removed_tx.mtx.mempool_estimated_bytes();

        let mut maybe_ready = TransactionIdSet::new();
        maybe_ready.extend(legacy_chains.iter().copied());
        for child_wtxid in cell_dependents {
            if let Some(child_id) = self.cell_wtxids.get(&child_wtxid).copied() {
                maybe_ready.insert(child_id);
            }
        }
        for child_id in maybe_ready {
            self.refresh_ready_status(child_id);
        }

        if self.all_transactions.is_empty() {
            assert_eq!(0, self.estimated_size, "Sanity test -- if tx pool is empty, estimated byte size should be zero");
        }

        Ok(removed_tx)
    }

    pub(crate) fn update_revalidated_transaction(&mut self, transaction: MempoolTransaction) -> bool {
        let transaction_id = transaction.id();
        let previous_cell_tx_id = self.all_transactions.get(&transaction_id).and_then(|tx| tx.cell_tx_id());
        let previous_cell_wtxid = self.all_transactions.get(&transaction_id).and_then(|tx| tx.cell_wtxid());
        let previous_parents = self.parent_transactions.get(&transaction_id).cloned().unwrap_or_default();

        if let Some(previous_cell_wtxid) = previous_cell_wtxid {
            if self.cell_pool.remove(&previous_cell_wtxid).is_err() {
                return false;
            }
            self.cell_wtxids.remove(&previous_cell_wtxid);
        }

        let legacy_parent_ids = if let Some(existing) = self.all_transactions.get(&transaction_id) {
            self.get_parent_transaction_ids_in_pool(&existing.mtx)
        } else {
            return false;
        };
        let parent_cell_ids = match self.build_parent_cell_id_context(&legacy_parent_ids) {
            Ok(parent_cell_ids) => parent_cell_ids,
            Err(_) => return false,
        };

        let mut transaction = transaction;
        let (cell_tx, fee) = if let Some(tx) = self.all_transactions.get_mut(&transaction_id) {
            // Make sure to update the overall estimated size since the updated transaction might have a different size
            self.estimated_size -= tx.mtx.mempool_estimated_bytes();
            transaction.priority = tx.priority;
            transaction.added_at_daa_score = tx.added_at_daa_score;
            transaction.refresh_cell_mirror_with_context(&parent_cell_ids);
            self.estimated_size += transaction.mtx.mempool_estimated_bytes();
            if let Some(previous_cell_tx_id) = previous_cell_tx_id {
                self.cell_transaction_ids.remove(&previous_cell_tx_id);
            }
            if let Some(cell_tx_id) = transaction.cell_tx_id() {
                self.cell_transaction_ids.insert(cell_tx_id, transaction.id());
            }
            let Some(cell_tx) = transaction.cell_tx() else {
                return false;
            };
            let Some(fee) = transaction.mtx.calculated_fee else {
                return false;
            };
            *tx = transaction;
            (cell_tx, fee)
        } else {
            return false;
        };

        let wtxid = match self.cell_pool.add(cell_tx.as_ref().clone(), fee, cell_tx.compute_mass()) {
            Ok(wtxid) => wtxid,
            Err(_) => return false,
        };
        if let Some(tx) = self.all_transactions.get_mut(&transaction_id) {
            tx.cell_wtxid = Some(wtxid);
            self.cell_wtxids.insert(wtxid, transaction_id);
        } else {
            return false;
        }
        for parent_id in previous_parents {
            if let Some(chains) = self.chained_transactions.get_mut(&parent_id) {
                chains.remove(&transaction_id);
            }
        }
        let effective_parents = if let Some(tx) = self.all_transactions.get(&transaction_id) {
            self.get_effective_parent_transaction_ids_in_pool(tx)
        } else {
            return false;
        };
        self.parent_transactions.insert(transaction_id, effective_parents.clone());
        for parent_id in effective_parents {
            self.chained_transactions.entry(parent_id).or_default().insert(transaction_id);
        }
        self.refresh_ready_status(transaction_id);
        true
    }

    pub(crate) fn ready_transaction_count(&self) -> usize {
        self.ready_transactions.len()
    }

    pub(crate) fn ready_transaction_total_mass(&self) -> u64 {
        self.ready_transactions.total_mass()
    }

    /// Dynamically builds a transaction selector based on the specific state of the ready transactions frontier
    pub(crate) fn build_selector(&self) -> Box<dyn TemplateTransactionSelector> {
        let cell_txs = self.build_cell_selector_snapshot();
        self.ready_transactions.build_selector_with_cells(&Policy::new(self.config.maximum_mass_per_block), Some(&cell_txs))
    }

    /// Builds a feerate estimator based on internal state of the ready transactions frontier
    pub(crate) fn build_feerate_estimator(&self, args: FeerateEstimatorArgs) -> FeerateEstimator {
        self.ready_transactions.build_feerate_estimator(args)
    }

    /// Returns the exceeding low-priority transactions having the lowest fee rates in order
    /// to make room for `transaction`. The returned transactions
    /// are guaranteed to be unchained (no successor in mempool) and to not be parent of
    /// `transaction`.
    ///
    /// An error is returned if the mempool is filled with high priority transactions, or
    /// there are not enough lower feerate transactions that can be removed to accommodate `transaction`
    pub(crate) fn limit_transaction_count(
        &self,
        transaction: &MutableTransaction,
        transaction_size: usize,
    ) -> RuleResult<Vec<TransactionId>> {
        // No eviction needed -- return
        if self.len() < self.config.maximum_transaction_count
            && self.estimated_size + transaction_size <= self.config.mempool_size_limit
        {
            return Ok(Default::default());
        }

        // Returns a vector of transactions to be removed (the caller has to actually remove)
        let feerate_threshold = transaction.calculated_feerate().unwrap();
        let mut txs_to_remove = Vec::with_capacity(1); // Normally we expect a single removal
        let mut selection_overall_size = 0;
        for tx in self
            .ready_transactions
            .ascending_iter()
            .map(|tx| self.all_transactions.get(&tx.id().into()).unwrap())
            .filter(|mtx| mtx.priority == Priority::Low)
        {
            // TODO (optimization): inline the `has_parent_in_set` check within the redeemer traversal and exit early if possible
            let redeemers = self.get_redeemer_ids_in_pool(&tx.id()).into_iter().chain(once(tx.id())).collect::<TransactionIdSet>();
            if transaction.has_parent_in_set(&redeemers) {
                continue;
            }

            // We are iterating ready txs by ascending feerate so the pending tx has lower feerate than all remaining txs
            if tx.feerate() > feerate_threshold {
                let err = RuleError::RejectMempoolIsFull;
                debug!("Transaction {} with feerate {} has been rejected: {}", transaction.id(), feerate_threshold, err);
                return Err(err);
            }

            txs_to_remove.push(tx.id());
            selection_overall_size += tx.mtx.mempool_estimated_bytes();

            if self.len() + 1 - txs_to_remove.len() <= self.config.maximum_transaction_count
                && self.estimated_size + transaction_size - selection_overall_size <= self.config.mempool_size_limit
            {
                return Ok(txs_to_remove);
            }
        }

        // We could not find sufficient space for the pending transaction
        debug!(
            "Mempool is filled with high-priority/ancestor txs (count: {}, bytes: {}). Transaction {} with feerate {} and size {} has been rejected: {}",
            self.len(),
            self.estimated_size,
            transaction.id(),
            feerate_threshold,
            transaction_size,
            RuleError::RejectMempoolIsFull
        );
        Err(RuleError::RejectMempoolIsFull)
    }

    pub(crate) fn get_estimated_size(&self) -> usize {
        self.estimated_size
    }

    pub(crate) fn all_transaction_ids_with_priority(&self, priority: Priority) -> Vec<TransactionId> {
        self.all().values().filter_map(|x| if x.priority == priority { Some(x.id()) } else { None }).collect()
    }

    pub(crate) fn get_outpoint_owner_id(&self, outpoint: &TransactionOutpoint) -> Option<&TransactionId> {
        self.cell_set.get_outpoint_owner_id(outpoint)
    }

    pub(crate) fn resolve_transaction_id_by_cell_transaction_id(&self, cell_transaction_id: &TransactionId) -> Option<TransactionId> {
        self.cell_transaction_ids.get(cell_transaction_id).copied()
    }

    /// Make sure no other transaction in the mempool is already spending an output which one of this transaction inputs spends
    pub(crate) fn check_double_spends(&self, transaction: &MutableTransaction) -> RuleResult<()> {
        self.cell_set.check_double_spends(transaction)
    }

    /// Returns the first double spend of every transaction in the mempool double spending on `transaction`
    pub(crate) fn get_double_spend_transaction_ids(&self, transaction: &MutableTransaction) -> Vec<DoubleSpend> {
        self.cell_set.get_double_spend_transaction_ids(transaction)
    }

    pub(crate) fn get_double_spend_owner<'a>(&'a self, double_spend: &DoubleSpend) -> RuleResult<&'a MempoolTransaction> {
        match self.get(&double_spend.owner_id) {
            Some(transaction) => Ok(transaction),
            None => {
                // This case should never arise in the first place.
                // Anyway, in case it does, if a double spent transaction id is found but the matching
                // transaction cannot be located in the mempool a replacement is no longer possible
                // so a double spend error is returned.
                Err(double_spend.into())
            }
        }
    }

    pub(crate) fn collect_expired_low_priority_transactions(&mut self, virtual_daa_score: u64) -> Vec<TransactionId> {
        let now = unix_now();
        if virtual_daa_score
            < self.last_expire_scan_daa_score + self.config.transaction_expire_scan_interval_daa_score
            || now < self.last_expire_scan_time + self.config.transaction_expire_scan_interval_milliseconds
        {
            return vec![];
        }

        self.last_expire_scan_daa_score = virtual_daa_score;
        self.last_expire_scan_time = now;

        // Never expire high priority transactions
        // Remove all transactions whose added_at_daa_score is older then transaction_expire_interval_daa_score
        self.all_transactions
            .values()
            .filter_map(|x| {
                if (x.priority == Priority::Low)
                    && virtual_daa_score
                        > x.added_at_daa_score + self.config.transaction_expire_interval_daa_score
                {
                    Some(x.id())
                } else {
                    None
                }
            })
            .collect()
    }
}

type IterTxId<'a> = Iter<'a, TransactionId>;
type KeysTxId<'a> = Keys<'a, TransactionId, MempoolTransaction>;

impl<'a> TopologicalIndex<'a, KeysTxId<'a>, IterTxId<'a>, TransactionId> for TransactionsPool {
    fn topology_nodes(&'a self) -> KeysTxId<'a> {
        self.all_transactions.keys()
    }

    fn topology_node_edges(&'a self, key: &TransactionId) -> Option<IterTxId<'a>> {
        self.chained_transactions.get(key).map(|x| x.iter())
    }
}

impl Pool for TransactionsPool {
    #[inline]
    fn all(&self) -> &MempoolTransactionCollection {
        &self.all_transactions
    }

    #[inline]
    fn chained(&self) -> &TransactionsEdges {
        &self.chained_transactions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell_conversion::cell_output_to_placeholder_entry;
    use smallvec::smallvec;
    use spora_consensus_core::{
        mass::NonContextualMasses,
        tx::{CellOut, CellRef, CellTx, MutableTransaction, ScriptPublicKey, ScriptRef, TransactionId, TransactionOutpoint},
    };

    fn build_test_mtx() -> MutableTransaction {
        let script_public_key = ScriptPublicKey::new(0, smallvec![0x51]);
        let input = CellRef::new(TransactionOutpoint::new(*TransactionId::default().as_bytes(), 0), 0);
        let output = CellOut {
            lock: ScriptRef::new(crate::cell_conversion::compute_lock_hash(&script_public_key), 0, vec![]),
            type_: None,
            capacity: 9_000,
        };
        let tx = Arc::new(CellTx::new(vec![input], vec![], vec![output], vec![vec![]], vec![vec![1, 2, 3]]).unwrap());
        let entry = cell_output_to_placeholder_entry(&CellOut {
            lock: ScriptRef::new(crate::cell_conversion::compute_lock_hash(&script_public_key), 0, vec![]),
            type_: None,
            capacity: 10_000,
        }, &[], 0, false);
        let mut mtx = MutableTransaction::with_entries(tx, vec![entry]);
        mtx.calculated_fee = Some(1_000);
        mtx.calculated_non_contextual_masses = Some(NonContextualMasses::new(100, 50));
        mtx
    }

    fn build_child_mtx(parent_id: TransactionId) -> MutableTransaction {
        let script_public_key = ScriptPublicKey::new(0, smallvec![0x51]);
        let input = CellRef::new(TransactionOutpoint::new(*parent_id.as_bytes(), 0), 0);
        let output = CellOut {
            lock: ScriptRef::new(crate::cell_conversion::compute_lock_hash(&script_public_key), 0, vec![]),
            type_: None,
            capacity: 8_000,
        };
        let tx = Arc::new(CellTx::new(vec![input], vec![], vec![output], vec![vec![]], vec![vec![4, 5, 6]]).unwrap());
        let entry = cell_output_to_placeholder_entry(&CellOut {
            lock: ScriptRef::new(crate::cell_conversion::compute_lock_hash(&script_public_key), 0, vec![]),
            type_: None,
            capacity: 9_000,
        }, &[], 0, false);
        let mut mtx = MutableTransaction::with_entries(tx, vec![entry]);
        mtx.calculated_fee = Some(1_000);
        mtx.calculated_non_contextual_masses = Some(NonContextualMasses::new(100, 50));
        mtx
    }

    #[test]
    fn build_selector_reads_cell_txs_from_cell_pool() {
        let config = Arc::new(Config::build_default(1000, false, 1_000_000));
        let mut pool = TransactionsPool::new(config);
        let mtx = build_test_mtx();
        let tx_id = mtx.id();
        let expected_cell_tx_id = mtx.tx.id();

        pool.add_transaction(mtx, 0, Priority::Low, 256).unwrap();
        let stored = pool.all_transactions.get_mut(&tx_id).unwrap();
        stored.cell_tx = None;

        let selected = pool.build_selector().select_transactions();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id(), expected_cell_tx_id);
    }

    #[test]
    fn redeemers_fall_back_to_cell_pool_dependencies() {
        let config = Arc::new(Config::build_default(1000, false, 1_000_000));
        let mut pool = TransactionsPool::new(config);

        let parent = build_test_mtx();
        let parent_id = parent.id();
        let child = build_child_mtx(parent_id);
        let child_id = child.id();

        pool.add_transaction(parent, 0, Priority::Low, 256).unwrap();
        pool.add_transaction(child, 0, Priority::Low, 256).unwrap();

        pool.parent_transactions.remove(&child_id);
        pool.chained_transactions.remove(&parent_id);

        let redeemers = pool.get_redeemer_ids_in_pool(&parent_id);
        assert_eq!(redeemers, vec![child_id]);
    }

    #[test]
    fn effective_parent_relations_can_be_rebuilt_from_cell_ids() {
        let config = Arc::new(Config::build_default(1000, false, 1_000_000));
        let mut pool = TransactionsPool::new(config);

        let parent = build_test_mtx();
        let parent_id = parent.id();
        let child = build_child_mtx(parent_id);
        let child_id = child.id();

        pool.add_transaction(parent, 0, Priority::Low, 256).unwrap();
        pool.add_transaction(child, 0, Priority::Low, 256).unwrap();

        let parent_cell_id = pool.all_transactions.get(&parent_id).unwrap().cell_tx_id().unwrap();
        let child_tx = pool.all_transactions.get_mut(&child_id).unwrap();
        child_tx.cell_tx.as_mut().unwrap().inputs.iter_mut().for_each(|input| input.out_point.tx_hash = *parent_cell_id.as_bytes());

        let effective = {
            let child = pool.all_transactions.get(&child_id).unwrap();
            pool.get_effective_parent_transaction_ids_in_pool(child)
        };
        assert_eq!(effective, TransactionIdSet::from([parent_id]));
    }

    #[test]
    fn removing_parent_reactivates_child_via_cell_pool_dependencies() {
        let config = Arc::new(Config::build_default(1000, false, 1_000_000));
        let mut pool = TransactionsPool::new(config);

        let parent = build_test_mtx();
        let parent_id = parent.id();
        let child = build_child_mtx(parent_id);
        let child_id = child.id();
        let expected_child_cell_tx_id = child.tx.id();

        pool.add_transaction(parent, 0, Priority::Low, 256).unwrap();
        pool.add_transaction(child, 0, Priority::Low, 256).unwrap();

        pool.parent_transactions.remove(&child_id);
        pool.chained_transactions.remove(&parent_id);

        pool.remove_transaction(&parent_id).unwrap();

        let selected = pool.build_selector().select_transactions();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id(), expected_child_cell_tx_id);
    }
}
