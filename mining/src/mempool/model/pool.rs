use crate::{
    mempool::{
        model::{map::MempoolTransactionCollection, tx::MempoolTransaction},
        tx::Priority,
    },
    model::{
        owner_txs::{AddressSet, GroupedOwnerTransactions},
        topological_index::TopologicalIndex,
        TransactionIdSet,
    },
};
use spora_consensus_client::pay_to_address_lock_script;
use spora_consensus_core::tx::{MutableTransaction, TransactionId};
use std::collections::{hash_set::Iter, HashMap, HashSet, VecDeque};

pub(crate) type TransactionsEdges = HashMap<TransactionId, TransactionIdSet>;

pub(crate) trait Pool {
    fn all(&self) -> &MempoolTransactionCollection;

    fn chained(&self) -> &TransactionsEdges;

    fn has(&self, transaction_id: &TransactionId) -> bool {
        self.all().contains_key(transaction_id)
    }

    fn get(&self, transaction_id: &TransactionId) -> Option<&MempoolTransaction> {
        self.all().get(transaction_id)
    }

    /// Returns the number of transactions in the pool
    fn len(&self) -> usize {
        self.all().len()
    }

    /// Returns an index over either high or low priority transaction ids which can
    /// in turn be topologically ordered.
    #[allow(dead_code)]
    fn index(&self, priority: Priority) -> PoolIndex {
        let transactions: TransactionIdSet =
            self.all().iter().filter_map(|(id, tx)| if tx.priority == priority { Some(*id) } else { None }).collect();
        let chained_transactions = transactions
            .iter()
            .filter_map(|id| {
                self.chained()
                    .get(id)
                    .map(|chains| (*id, chains.iter().filter_map(|chain| transactions.get(chain).copied()).collect()))
            })
            .collect();
        PoolIndex::new(transactions, chained_transactions)
    }

    /// Returns the ids of all transactions being parents of `transaction` and existing in the pool.
    fn get_parent_transaction_ids_in_pool(&self, transaction: &MutableTransaction) -> TransactionIdSet {
        let mut parents = HashSet::with_capacity(transaction.tx.inputs.len());
        for input in transaction.tx.inputs.iter() {
            let previous_tx_id = TransactionId::from_bytes(input.previous_output.tx_hash);
            if self.has(&previous_tx_id) {
                parents.insert(previous_tx_id);
            }
        }
        parents
    }

    /// Returns the ids of all transactions being directly and indirectly chained to `transaction_id`
    /// and existing in the pool.
    ///
    /// The transactions are traversed in BFS mode. The returned order is not guaranteed to be
    /// topological.
    ///
    /// NOTE: this operation's complexity might become linear in the size of the mempool if the mempool
    /// contains deeply chained transactions
    fn get_redeemer_ids_in_pool(&self, transaction_id: &TransactionId) -> Vec<TransactionId> {
        // DESIGN(P2): The BFS traversal returns redeemer ids in discovery order, which is NOT
        // guaranteed to be reverse-topological. When a caller removes these redeemers sequentially,
        // removing a parent before its child may momentarily create a "missing outpoint" state
        // visible to concurrent readers (e.g. block-template builder or revalidation). In practice
        // this is benign today because removals happen under the mempool write lock, but if the
        // lock is ever split (see MiningManager locking note), removals should be sorted in
        // reverse-topological order (children first, then parents) so that no intermediate state
        // exposes a transaction whose parent has already been evicted.
        let mut visited = TransactionIdSet::new();
        let mut descendants = vec![];
        if let Some(transaction) = self.get(transaction_id) {
            let mut queue = VecDeque::new();
            queue.push_back(transaction);
            while let Some(transaction) = queue.pop_front() {
                if let Some(chains) = self.chained().get(&transaction.id()) {
                    chains.iter().for_each(|redeemer_id| {
                        if let Some(redeemer) = self.get(redeemer_id) {
                            if visited.insert(*redeemer_id) {
                                descendants.push(*redeemer_id);
                                queue.push_back(redeemer);
                            }
                        }
                    })
                }
            }
        }
        descendants
    }

    /// Returns a vector with clones of all the transactions in the pool.
    fn get_all_transactions(&self) -> Vec<MutableTransaction> {
        self.all().values().map(|x| x.mtx.clone()).collect()
    }

    /// Returns a vector with ids of all the transactions in the pool.
    fn get_all_transaction_ids(&self) -> Vec<TransactionId> {
        self.all().keys().cloned().collect()
    }

    /// Fills owner transactions for a set of addresses.
    fn fill_owner_set_transactions(&self, addresses: &AddressSet, owner_set: &mut GroupedOwnerTransactions) {
        addresses.iter().for_each(|address| {
            let owner = owner_set.owners.entry(address.clone()).or_default();
            let lock_hash = pay_to_address_lock_script(address).hash();

            self.all().iter().for_each(|(id, transaction)| {
                // Sending transactions
                if transaction.mtx.entries.iter().flatten().any(|entry| entry.lock_hash == lock_hash)
                    || transaction.mtx.resolved_cell_metadata.iter().flatten().any(|metadata| metadata.lock_hash == lock_hash)
                {
                    // Insert the mutable transaction in the owners object if not already present.
                    // Clone since the transaction leaves the mempool.
                    owner_set.transactions.entry(*id).or_insert_with(|| transaction.mtx.clone());
                    owner.sending_txs.insert(*id);
                }

                if transaction.mtx.tx.outputs.iter().any(|x| x.lock.hash() == lock_hash) {
                    // Insert the mutable transaction in the owners object if not already present.
                    // Clone since the transaction leaves the mempool.
                    owner_set.transactions.entry(*id).or_insert_with(|| transaction.mtx.clone());
                    owner.receiving_txs.insert(*id);
                }
            });
        });
    }
}

pub(crate) struct PoolIndex {
    transactions: TransactionIdSet,
    chained_transactions: TransactionsEdges,
}

impl PoolIndex {
    #[allow(dead_code)]
    pub(crate) fn new(transactions: TransactionIdSet, chained_transactions: TransactionsEdges) -> Self {
        Self { transactions, chained_transactions }
    }
}

type IterTxId<'a> = Iter<'a, TransactionId>;

impl<'a> TopologicalIndex<'a, IterTxId<'a>, IterTxId<'a>, TransactionId> for PoolIndex {
    fn topology_nodes(&'a self) -> IterTxId<'a> {
        self.transactions.iter()
    }

    fn topology_node_edges(&'a self, key: &TransactionId) -> Option<IterTxId<'a>> {
        self.chained_transactions.get(key).map(|x| x.iter())
    }
}
