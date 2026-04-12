use crate::mempool::{
    errors::RuleResult,
    model::{
        pool::Pool,
        tx::{MempoolTransaction, TxRemovalReason},
    },
    Mempool,
};
use spora_consensus_core::{
    api::ConsensusApi,
    tx::{CellTx, TransactionId},
};
use spora_core::time::Stopwatch;
use spora_hashes::Hash;
use std::{collections::HashSet, sync::atomic::Ordering};

impl Mempool {
    fn get_local_transaction_ids_with_parent_transaction(&self, parent_transaction_id: &TransactionId) -> Vec<TransactionId> {
        let mut transaction_ids = self.transaction_pool.get_transaction_ids_with_parent_transaction(parent_transaction_id);
        transaction_ids.extend(self.orphan_pool.get_transaction_ids_with_parent_transaction(parent_transaction_id));
        transaction_ids.sort_unstable();
        transaction_ids.dedup();
        transaction_ids
    }

    fn find_local_transaction_id_for_accepted_cell_tx(&self, accepted_tx: &CellTx) -> Option<TransactionId> {
        let mut candidate_ids = HashSet::new();
        for input in &accepted_tx.inputs {
            let parent_cell_id = Hash::from_bytes(input.out_point.tx_hash);
            candidate_ids.extend(self.get_local_transaction_ids_with_parent_transaction(&parent_cell_id));
        }

        for candidate_id in candidate_ids {
            let Some(candidate_tx) = self.transaction_pool.get(&candidate_id).or_else(|| self.orphan_pool.get(&candidate_id)) else {
                continue;
            };
            if candidate_tx.mtx.tx.as_ref() == accepted_tx {
                return Some(candidate_id);
            }
        }

        None
    }

    pub(crate) fn handle_new_block_transactions(
        &mut self,
        consensus: &dyn ConsensusApi,
        block_daa_score: u64,
        block_transactions: &[CellTx],
    ) -> RuleResult<Vec<MempoolTransaction>> {
        #[cfg(not(test))]
        let _ = consensus;
        let _sw = Stopwatch::<400>::with_threshold("handle_new_block_transactions op");
        let mut unorphaned_transactions = vec![];
        let mut tx_accepted_counts = 0;
        let mut input_counts = 0;
        let mut output_counts = 0;
        for transaction in block_transactions.iter().skip(1) {
            let transaction_id: TransactionId = transaction.id().into();
            let resolved_transaction_id = self
                .resolve_transaction_id_by_cell_transaction_id(&transaction_id)
                .or_else(|| self.find_local_transaction_id_for_accepted_cell_tx(transaction))
                .unwrap_or(transaction_id);

            let accepted_compat_transaction_id: Option<TransactionId> = None;

            // Rust rewrite: This behavior does differ from golang implementation.
            // If the transaction got accepted via a peer but is still an orphan here, do not remove
            // its redeemers in the orphan pool. We give those a chance to be unorphaned and included
            // in the next block template.
            if !self.orphan_pool.has(&resolved_transaction_id) {
                self.remove_transaction(&resolved_transaction_id, false, TxRemovalReason::Accepted, "")?;
            }
            self.remove_double_spends_cell(transaction)?;
            self.orphan_pool.remove_orphan(&resolved_transaction_id, false, TxRemovalReason::Accepted, "")?;

            let mut was_newly_accepted = self.accepted_transactions.add(transaction_id, block_daa_score);
            if resolved_transaction_id != transaction_id {
                was_newly_accepted |= self.accepted_transactions.add(resolved_transaction_id, block_daa_score);
            }
            if was_newly_accepted {
                tx_accepted_counts += 1;
                input_counts += transaction.inputs.len();
                output_counts += transaction.outputs.len();
            }

            let newly_unorphaned = self.get_unorphaned_transactions_after_accepted_cell_transaction(
                transaction,
                accepted_compat_transaction_id,
                block_daa_score,
            );
            if !newly_unorphaned.is_empty() {
                unorphaned_transactions.extend(newly_unorphaned);
            } else {
                let mut candidate_orphan_ids = self.orphan_pool.get_orphan_ids_with_parent_transaction(&transaction_id);
                if resolved_transaction_id != transaction_id {
                    candidate_orphan_ids.extend(self.orphan_pool.get_orphan_ids_with_parent_transaction(&resolved_transaction_id));
                    candidate_orphan_ids.sort_unstable();
                    candidate_orphan_ids.dedup();
                }
                for orphan_id in candidate_orphan_ids {
                    let removed =
                        self.orphan_pool.remove_orphan(&orphan_id, false, TxRemovalReason::Unorphaned, " for accepted block retry")?;
                    unorphaned_transactions.extend(removed);
                }
            }
        }
        self.counters.block_tx_counts.fetch_add(block_transactions.len().saturating_sub(1) as u64, Ordering::Relaxed);
        self.counters.tx_accepted_counts.fetch_add(tx_accepted_counts, Ordering::Relaxed);
        self.counters.input_counts.fetch_add(input_counts as u64, Ordering::Relaxed);
        self.counters.output_counts.fetch_add(output_counts as u64, Ordering::Relaxed);
        self.counters.ready_txs_sample.store(self.transaction_pool.ready_transaction_count() as u64, Ordering::Relaxed);
        self.counters.txs_sample.store(self.transaction_pool.len() as u64, Ordering::Relaxed);
        self.counters.orphans_sample.store(self.orphan_pool.len() as u64, Ordering::Relaxed);
        self.counters.accepted_sample.store(self.accepted_transactions.len() as u64, Ordering::Relaxed);

        Ok(unorphaned_transactions)
    }

    pub(crate) fn expire_orphan_low_priority_transactions(&mut self, consensus: &dyn ConsensusApi) -> RuleResult<()> {
        self.orphan_pool.expire_low_priority_transactions(consensus.get_virtual_daa_score())
    }

    pub(crate) fn expire_accepted_transactions(&mut self, consensus: &dyn ConsensusApi) {
        self.accepted_transactions.expire(consensus.get_virtual_daa_score());
    }

    pub(crate) fn collect_expired_low_priority_transactions(&mut self, consensus: &dyn ConsensusApi) -> Vec<TransactionId> {
        self.transaction_pool.collect_expired_low_priority_transactions(consensus.get_virtual_daa_score())
    }

    fn remove_double_spends_cell(&mut self, transaction: &CellTx) -> RuleResult<()> {
        let mut transactions_to_remove = HashSet::new();
        for input in transaction.inputs.iter() {
            let previous_outpoint = input.out_point;
            if let Some(redeemer_id) = self.transaction_pool.get_outpoint_owner_id(&previous_outpoint) {
                transactions_to_remove.insert(*redeemer_id);
            }
        }

        let transaction_id: TransactionId = transaction.id().into();
        transactions_to_remove.iter().try_for_each(|x| {
            self.remove_transaction(x, true, TxRemovalReason::DoubleSpend, format!(" favouring {}", transaction_id).as_str())
        })
    }
}
