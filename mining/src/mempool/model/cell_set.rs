use std::collections::HashSet;

use crate::{
    mempool::{
        errors::RuleResult,
        model::{map::OutpointIndex, tx::DoubleSpend},
    },
    model::TransactionIdSet,
};
use spora_consensus_core::tx::{MutableTransaction, OutPointCompat, TransactionId, TransactionOutpoint};

// TODO: extend MempoolCellSet from spent-input tracking to full mempool-owned Cell tracking.
pub(crate) struct MempoolCellSet {
    outpoint_owner_id: OutpointIndex,
}

impl MempoolCellSet {
    pub(crate) fn new() -> Self {
        Self { outpoint_owner_id: OutpointIndex::default() }
    }

    pub(crate) fn add_transaction(&mut self, transaction: &MutableTransaction) {
        let transaction_id = transaction.id();

        for input in transaction.tx.inputs.iter() {
            // Track spent inputs immediately for double-spend detection.
            self.outpoint_owner_id.insert(input.out_point, transaction_id);
        }

        // TODO: Track newly created mempool-owned Cells once the pool
        // stops depending on transaction entries for child discovery.
    }

    pub(crate) fn remove_transaction(&mut self, transaction: &MutableTransaction, parent_ids_in_pool: &TransactionIdSet) {
        // We cannot assume here that the transaction is fully populated.
        // Notably, this is not the case when revalidate_transaction fails and leads the execution path here.
        for (i, input) in transaction.tx.inputs.iter().enumerate() {
            if let Some(ref entry) = transaction.entries[i] {
                // If the transaction creating the output spent by this input is in the
                // mempool, restore the parent-owned Cell once the full Cell set exists.
                if parent_ids_in_pool.contains(&input.out_point.transaction_id()) {
                    let _ = entry;
                }
            }
            self.outpoint_owner_id.remove(&input.out_point);
        }

        // TODO: Remove newly created mempool-owned Cells once output
        // tracking is implemented in this structure.
    }

    pub(crate) fn get_outpoint_owner_id(&self, outpoint: &TransactionOutpoint) -> Option<&TransactionId> {
        self.outpoint_owner_id.get(outpoint)
    }

    /// Make sure no other transaction in the mempool is already spending an output which one of this transaction inputs spends
    pub(crate) fn check_double_spends(&self, transaction: &MutableTransaction) -> RuleResult<()> {
        match self.get_first_double_spend(transaction) {
            Some(double_spend) => Err(double_spend.into()),
            None => Ok(()),
        }
    }

    pub(crate) fn get_first_double_spend(&self, transaction: &MutableTransaction) -> Option<DoubleSpend> {
        let transaction_id = transaction.id();
        for input in transaction.tx.inputs.iter() {
            if let Some(existing_transaction_id) = self.get_outpoint_owner_id(&input.out_point) {
                if *existing_transaction_id != transaction_id {
                    return Some(DoubleSpend::new(input.out_point, *existing_transaction_id));
                }
            }
        }
        None
    }

    /// Returns the first double spend of every transaction in the mempool double spending on `transaction`
    pub(crate) fn get_double_spend_transaction_ids(&self, transaction: &MutableTransaction) -> Vec<DoubleSpend> {
        let transaction_id = transaction.id();
        let mut double_spends = vec![];
        let mut visited = HashSet::new();
        for input in transaction.tx.inputs.iter() {
            if let Some(existing_transaction_id) = self.get_outpoint_owner_id(&input.out_point) {
                if *existing_transaction_id != transaction_id && visited.insert(*existing_transaction_id) {
                    double_spends.push(DoubleSpend::new(input.out_point, *existing_transaction_id));
                }
            }
        }
        double_spends
    }
}
