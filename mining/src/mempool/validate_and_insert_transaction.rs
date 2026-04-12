use std::sync::{atomic::Ordering, Arc};

use crate::cell_conversion::cell_output_to_metadata;
use crate::mempool::{
    errors::{RuleError, RuleResult},
    model::{
        pool::Pool,
        tx::{MempoolTransaction, TransactionPostValidation, TransactionPreValidation, TxRemovalReason},
    },
    tx::{Orphan, Priority, RbfPolicy},
    Mempool,
};
use spora_consensus_core::{
    api::ConsensusApi,
    tx::{CellTx, MutableTransaction, TransactionId, TransactionOutpoint},
};
use spora_core::{debug, info};

impl Mempool {
    pub(crate) fn pre_validate_and_populate_cell_transaction(
        &self,
        consensus: &dyn ConsensusApi,
        cell_tx: CellTx,
        rbf_policy: RbfPolicy,
    ) -> RuleResult<TransactionPreValidation> {
        let mut transaction = MutableTransaction::from_cell_tx(cell_tx.clone());
        self.validate_transaction_unacceptance(&transaction)?;
        transaction.calculated_non_contextual_masses =
            Some(consensus.calculate_transaction_non_contextual_masses(transaction.tx.as_ref()));
        self.validate_transaction_in_isolation(&transaction)?;
        let feerate_threshold = self.get_replace_by_fee_constraint(&transaction, rbf_policy)?;
        self.populate_mempool_entries(&mut transaction);
        Ok(TransactionPreValidation { transaction, cell_tx: Some(Arc::new(cell_tx)), feerate_threshold })
    }

    pub(crate) fn post_validate_and_insert_transaction(
        &mut self,
        consensus: &dyn ConsensusApi,
        validation_result: RuleResult<()>,
        transaction: MutableTransaction,
        cell_tx: Option<Arc<CellTx>>,
        priority: Priority,
        orphan: Orphan,
        rbf_policy: RbfPolicy,
    ) -> RuleResult<TransactionPostValidation> {
        let transaction_id = transaction.id();

        // First check if the transaction was not already added to the mempool.
        // The case may arise since the execution of the manager public functions is no
        // longer atomic and different code paths may lead to inserting the same transaction
        // concurrently.
        if self.transaction_pool.has(&transaction_id) {
            debug!("Transaction {0} is not post validated since already in the mempool", transaction_id);
            return Err(RuleError::RejectDuplicate(transaction_id));
        }

        self.validate_transaction_unacceptance(&transaction)?;

        match validation_result {
            Ok(_) => {}
            Err(RuleError::RejectMissingOutpoint) => {
                if orphan == Orphan::Forbidden {
                    return Err(RuleError::RejectDisallowedOrphan(transaction_id));
                }
                let _ = self.get_replace_by_fee_constraint(&transaction, rbf_policy)?;
                let mempool_tx = match cell_tx.clone() {
                    Some(cell_tx) => {
                        MempoolTransaction::new_with_cell_tx(transaction, cell_tx, priority, consensus.get_virtual_daa_score())
                    }
                    None => MempoolTransaction::new(transaction, priority, consensus.get_virtual_daa_score()),
                };
                self.orphan_pool.try_add_mempool_transaction_orphan(mempool_tx)?;
                return Ok(TransactionPostValidation::default());
            }
            Err(err) => {
                return Err(err);
            }
        }

        // Perform mempool in-context validations prior to possible RBF replacements
        self.validate_transaction_in_context(&transaction)?;

        // Check double spends and try to remove them if the RBF policy requires it
        let removed_transaction = self.execute_replace_by_fee(&transaction, rbf_policy)?;

        //
        // Note: there exists a case below where `limit_transaction_count` returns an error signaling that
        //       this tx should be rejected due to mempool size limits (rather than evicting others). However,
        //       if this tx happened to be an RBF tx, it might have already caused an eviction in the line
        //       above. We choose to ignore this rare case for now, as it essentially means that even the increased
        //       feerate of the replacement tx is very low relative to the mempool overall.
        //

        // Before adding the transaction, check if there is room in the pool
        let transaction_size = transaction.mempool_estimated_bytes();
        let txs_to_remove = self.transaction_pool.limit_transaction_count(&transaction, transaction_size)?;
        if !txs_to_remove.is_empty() {
            let transaction_pool_len_before = self.transaction_pool.len();
            for x in txs_to_remove.iter() {
                self.remove_transaction(x, true, TxRemovalReason::MakingRoom, format!(" for {}", transaction_id).as_str())?;
                // self.transaction_pool.limit_transaction_count(&transaction) returns the
                // smallest prefix of `ready_transactions` (sorted by ascending fee-rate)
                // that makes enough room for `transaction`, but since each call to `self.remove_transaction`
                // also removes all transactions dependant on `x` we might already have sufficient space, so
                // we constantly check the break condition.
                //
                // Note that self.transaction_pool.len() < self.config.maximum_transaction_count means we have
                // at least one available slot in terms of the count limit
                if self.transaction_pool.len() < self.config.maximum_transaction_count
                    && self.transaction_pool.get_estimated_size() + transaction_size <= self.config.mempool_size_limit
                {
                    break;
                }
            }
            self.counters
                .tx_evicted_counts
                .fetch_add(transaction_pool_len_before.saturating_sub(self.transaction_pool.len()) as u64, Ordering::Relaxed);
        }

        assert!(
            self.transaction_pool.len() < self.config.maximum_transaction_count
                && self.transaction_pool.get_estimated_size() + transaction_size <= self.config.mempool_size_limit,
            "Transactions in mempool: {}, max: {}, mempool bytes size: {}, max: {}",
            self.transaction_pool.len() + 1,
            self.config.maximum_transaction_count,
            self.transaction_pool.get_estimated_size() + transaction_size,
            self.config.mempool_size_limit,
        );

        // Add the transaction to the mempool as a MempoolTransaction and return clones of the stored compatibility/canonical views.
        let mempool_tx = match cell_tx {
            Some(cell_tx) => MempoolTransaction::new_with_cell_tx(transaction, cell_tx, priority, consensus.get_virtual_daa_score()),
            None => MempoolTransaction::new(transaction, priority, consensus.get_virtual_daa_score()),
        };
        let accepted = self.transaction_pool.add_mempool_transaction(mempool_tx, transaction_size)?;
        Ok(TransactionPostValidation {
            removed: removed_transaction,
            accepted: Some(accepted.mtx.tx.clone()),
            accepted_cell_tx: accepted.cell_tx(),
        })
    }

    /// Validates that the transaction wasn't already accepted into the DAG
    fn validate_transaction_unacceptance(&self, transaction: &MutableTransaction) -> RuleResult<()> {
        // Reject if the transaction is registered as an accepted transaction
        let transaction_id = transaction.id();
        match self.accepted_transactions.has(&transaction_id) {
            true => Err(RuleError::RejectAlreadyAccepted(transaction_id)),
            false => Ok(()),
        }
    }

    fn validate_transaction_in_isolation(&self, transaction: &MutableTransaction) -> RuleResult<()> {
        let transaction_id = transaction.id();
        if self.transaction_pool.has(&transaction_id) {
            return Err(RuleError::RejectDuplicate(transaction_id));
        }

        if !self.config.accept_non_standard {
            self.check_transaction_standard_in_isolation(transaction)?;
        }
        Ok(())
    }

    fn validate_transaction_in_context(&self, transaction: &MutableTransaction) -> RuleResult<()> {
        if !self.config.accept_non_standard {
            self.check_transaction_standard_in_context(transaction)?;
        }
        Ok(())
    }

    pub(crate) fn get_unorphaned_transactions_after_accepted_cell_transaction(
        &mut self,
        transaction: &CellTx,
        accepted_legacy_transaction_id: Option<TransactionId>,
        block_daa_score: u64,
    ) -> Vec<MempoolTransaction> {
        let mut unorphaned_transactions = Vec::new();
        let transaction_id: TransactionId = transaction.id().into();
        let mut accepted_parent_ids = vec![transaction_id];
        if let Some(legacy_id) = accepted_legacy_transaction_id.filter(|legacy_id| *legacy_id != transaction_id) {
            accepted_parent_ids.push(legacy_id);
        }

        for parent_id in accepted_parent_ids {
            let mut outpoint = TransactionOutpoint::new(parent_id.as_bytes(), 0);
            for (i, output) in transaction.outputs.iter().enumerate() {
                outpoint.index = i as u32;
                let mut orphan_id = None;
                if let Some(orphan) = self.orphan_pool.outpoint_orphan_mut(&outpoint) {
                    for (input_index, input) in orphan.mtx.tx.inputs.iter().enumerate() {
                        if input.out_point == outpoint {
                            if orphan.mtx.entries[input_index].is_none() && orphan.mtx.resolved_cell_metadata[input_index].is_none() {
                                let output_data = transaction.outputs_data.get(i).cloned().unwrap_or_default();
                                orphan.mtx.resolved_cell_metadata[input_index] =
                                    Some(cell_output_to_metadata(outpoint, output, &output_data, block_daa_score, false));
                                if orphan.mtx.is_verifiable() {
                                    orphan_id = Some(orphan.id());
                                }
                            }
                            break;
                        }
                    }
                } else {
                    continue;
                }
                if let Some(orphan_id) = orphan_id {
                    match self.unorphan_transaction(&orphan_id) {
                        Ok(unorphaned_tx) => {
                            unorphaned_transactions.push(unorphaned_tx);
                            debug!("Transaction {0} unorphaned", transaction_id);
                        }
                        Err(RuleError::RejectAlreadyAccepted(transaction_id)) => {
                            debug!("Ignoring already accepted transaction {}", transaction_id);
                        }
                        Err(err) => {
                            // In case of validation error, we log the problem and drop the
                            // erroneous transaction.
                            info!("Failed to unorphan transaction {0} due to rule error: {1}", orphan_id, err.to_string());
                        }
                    }
                }
            }
        }

        unorphaned_transactions
    }

    fn unorphan_transaction(&mut self, transaction_id: &TransactionId) -> RuleResult<MempoolTransaction> {
        // Rust rewrite:
        // - Instead of adding the validated transaction to mempool transaction pool,
        //   we return it.
        // - The function is relocated from OrphanPool into Mempool.
        // - The function no longer validates the transaction in mempool (signatures) nor in context.
        //   This job is delegated to a fn called later in the process (Manager::validate_and_insert_unorphaned_transactions).

        // Remove the transaction identified by transaction_id from the orphan pool.
        let mut transactions = self.orphan_pool.remove_orphan(transaction_id, false, TxRemovalReason::Unorphaned, "")?;

        // At this point, `transactions` contains exactly one transaction.
        // The one we just removed from the orphan pool.
        assert_eq!(transactions.len(), 1, "the list returned by remove_orphan is expected to contain exactly one transaction");
        let transaction = transactions.pop().unwrap();
        let rbf_policy = Self::get_orphan_transaction_rbf_policy(transaction.priority);

        self.validate_transaction_unacceptance(&transaction.mtx)?;
        let _ = self.get_replace_by_fee_constraint(&transaction.mtx, rbf_policy)?;
        Ok(transaction)
    }

    /// Returns the RBF policy to apply to an orphan/unorphaned transaction by inferring it from the transaction priority.
    pub(crate) fn get_orphan_transaction_rbf_policy(priority: Priority) -> RbfPolicy {
        // The RBF policy applied to an orphaned transaction is not recorded in the orphan pool
        // but we can infer it from the priority:
        //
        //  - high means a submitted tx via RPC which forbids RBF
        //  - low means a tx arrived via P2P which allows RBF
        //
        // Note that the RPC submit transaction replacement case, implying a mandatory RBF, forbids orphans
        // so is excluded here.
        match priority {
            Priority::High => RbfPolicy::Forbidden,
            Priority::Low => RbfPolicy::Allowed,
        }
    }
}
