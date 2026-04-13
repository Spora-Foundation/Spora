use rand::Rng;
use spora_core::{time::Stopwatch, trace};
use std::collections::HashMap;

use crate::model::candidate_tx::CandidateTransaction;

use super::{
    model::tx::{CandidateList, SelectableTransaction, SelectableTransactions, TransactionIndex},
    policy::Policy,
};
use spora_consensus_core::{
    block::TemplateTransactionSelector,
    tx::{CellTx, TransactionId},
};

/// ALPHA is a coefficient that defines how uniform the distribution of
/// candidate transactions should be. A smaller alpha makes the distribution
/// more uniform. ALPHA is used when determining a candidate transaction's
/// initial p value.
pub(crate) const ALPHA: i32 = 3;

/// REBALANCE_THRESHOLD is the percentage of candidate transactions under which
/// we don't rebalance. Rebalancing is a heavy operation so we prefer to avoid
/// rebalancing very often. On the other hand, if we don't rebalance often enough
/// we risk having too many collisions.
/// The value is derived from the max probability of collision. That is to say,
/// if REBALANCE_THRESHOLD is 0.95, there's a 1-in-20 chance of collision.
const REBALANCE_THRESHOLD: f64 = 0.95;

pub struct RebalancingWeightedTransactionSelector {
    policy: Policy,
    /// Transaction store
    transactions: Vec<CandidateTransaction>,
    /// Selectable transactions store
    selectable_txs: SelectableTransactions,

    /// Indexes of selected transactions in stores
    selected_txs: Vec<TransactionIndex>,

    /// Optional state for handling selection rejections. Maps from a selected tx id
    /// to the index of the tx in the `transactions` vec
    selected_txs_map: Option<HashMap<TransactionId, TransactionIndex>>,

    // Inner state of the selection process
    candidate_list: CandidateList,
    overall_rejections: usize,
    used_count: usize,
    used_p: f64,
    total_mass: u64,
    total_fees: u64,
}

impl RebalancingWeightedTransactionSelector {
    pub fn new(policy: Policy, mut transactions: Vec<CandidateTransaction>) -> Self {
        let _sw = Stopwatch::<100>::with_threshold("TransactionsSelector::new op");
        // Keep selection deterministic across nodes.
        transactions.sort_by_key(|tx| tx.tx.id());

        // Create the object without selectable transactions
        let mut selector = Self {
            policy,
            transactions,
            selectable_txs: Default::default(),
            selected_txs: Default::default(),
            selected_txs_map: None,
            candidate_list: Default::default(),
            overall_rejections: 0,
            used_count: 0,
            used_p: 0.0,
            total_mass: 0,
            total_fees: 0,
        };

        // Create the selectable transactions
        selector.selectable_txs =
            selector.transactions.iter().map(|x| SelectableTransaction::new(selector.calc_tx_value(x), 0, ALPHA)).collect();
        // Prepare the initial candidate list
        selector.candidate_list = CandidateList::new(&selector.selectable_txs);

        selector
    }

    /// select_transactions implements a probabilistic transaction selection algorithm.
    /// The algorithm, roughly, is as follows:
    /// 1. We assign a probability to each transaction equal to:
    ///    (candidateTx.Value^alpha) / Σ(tx.Value^alpha)
    ///    Where the sum of the probabilities of all txs is 1.
    /// 2. We draw a random number in [0,1) and select a transaction accordingly.
    /// 3. If it's valid, add it to the selectedTxs and remove it from the candidates.
    /// 4. Continue iterating the above until we have either selected all
    ///    available transactions or ran out of gas/block space.
    ///
    /// Note that we make two optimizations here:
    /// * Draw a number in [0,Σ(tx.Value^alpha)) to avoid normalization
    /// * Instead of removing a candidate after each iteration, mark it for deletion.
    ///   Once the sum of probabilities of marked transactions is greater than
    ///   REBALANCE_THRESHOLD percent of the sum of probabilities of all transactions,
    ///   rebalance.
    ///
    /// select_transactions loops over the candidate transactions
    /// and appends the ones that will be included in the next block into
    /// selected_txs.
    pub fn select_transactions(&mut self) -> Vec<CellTx> {
        let _sw = Stopwatch::<15>::with_threshold("select_transaction op");
        let mut rng = rand::thread_rng();

        self.reset_selection();

        while self.candidate_list.candidates.len() - self.used_count > 0 {
            // Rebalance the candidates if it's required
            if self.used_p >= REBALANCE_THRESHOLD * self.candidate_list.total_p {
                self.candidate_list = self.candidate_list.rebalanced(&self.selectable_txs);
                self.used_count = 0;
                self.used_p = 0.0;

                // Break if we now ran out of transactions
                if self.candidate_list.is_empty() {
                    break;
                }
            }

            // Select a candidate tx at random
            let r = rng.gen::<f64>() * self.candidate_list.total_p;
            let selected_candidate_idx = self.candidate_list.find(r);
            let selected_candidate = self.candidate_list.candidates.get_mut(selected_candidate_idx).unwrap();

            // If is_marked_for_deletion is set, it means we got a collision.
            // Ignore and select another Tx.
            if selected_candidate.is_marked_for_deletion {
                continue;
            }
            let selected_tx = &self.transactions[selected_candidate.index];

            // Enforce maximum transaction mass per block.
            // Also check for overflow.
            let next_total_mass = self.total_mass.checked_add(selected_tx.calculated_mass);
            if next_total_mass.is_none() || next_total_mass.unwrap() > self.policy.max_block_mass {
                trace!("Tx {:?} would exceed the max block mass. As such, stopping.", selected_tx.tx.id());
                break;
            }

            // Add the transaction to the result, increment counters, and
            // save the masses, fees, and signature operation counts to the
            // result.
            self.selected_txs.push(selected_candidate.index);
            self.total_mass += selected_tx.calculated_mass;
            self.total_fees += selected_tx.calculated_fee;

            trace!(
                "Adding tx {:?} (fee per gram: {1})",
                selected_tx.tx.id(),
                selected_tx.calculated_fee / selected_tx.calculated_mass
            );

            // Mark for deletion
            selected_candidate.is_marked_for_deletion = true;
            self.used_count += 1;
            self.used_p += self.selectable_txs[selected_candidate.index].p;
        }

        self.selected_txs.sort();

        self.get_transactions()
    }

    fn get_transactions(&self) -> Vec<CellTx> {
        // These transactions leave the selector so we clone
        self.selected_txs.iter().map(|x| self.transactions[*x].tx.as_ref().clone()).collect()
    }

    fn reset_selection(&mut self) {
        assert_eq!(self.transactions.len(), self.selectable_txs.len());
        self.selected_txs.clear();
        // TODO: consider to min with the approximated amount of txs which fit into max block mass
        self.selected_txs.reserve_exact(self.transactions.len());
        self.selected_txs_map = None;
    }

    /// calc_tx_value calculates a value to be used in transaction selection.
    /// The higher the number the more likely it is that the transaction will be
    /// included in the block.
    fn calc_tx_value(&self, transaction: &CandidateTransaction) -> f64 {
        if let Some(cell_score_total) = transaction.cell_score_total {
            return cell_score_total.max(f64::EPSILON);
        }

        let mass_limit = self.policy.max_block_mass as f64;
        let mass = transaction.calculated_mass as f64;
        let fee = transaction.calculated_fee as f64;
        fee / mass / mass_limit
    }
}

impl TemplateTransactionSelector for RebalancingWeightedTransactionSelector {
    fn select_transactions(&mut self) -> Vec<CellTx> {
        let selected_cell_txs = RebalancingWeightedTransactionSelector::select_transactions(self);
        let mut selected_txs_map = HashMap::with_capacity(selected_cell_txs.len());
        for (tx_index, cell_tx) in self.selected_txs.iter().copied().zip(selected_cell_txs.iter()) {
            selected_txs_map.insert(cell_tx.id().into(), tx_index);
        }
        self.selected_txs_map = Some(selected_txs_map);
        selected_cell_txs
    }

    fn reject_selection(&mut self, tx_id: TransactionId) {
        let selected_txs_map = self
            .selected_txs_map
            // We lazy-create the map only when there are actual rejections
            .get_or_insert_with(|| self.selected_txs.iter().map(|&x| (self.transactions[x].tx.id().into(), x)).collect());
        let tx_index = selected_txs_map.remove(&tx_id).expect("only previously selected txs can be rejected (and only once)");
        let tx = &self.transactions[tx_index];
        self.total_mass -= tx.calculated_mass;
        self.total_fees -= tx.calculated_fee;
        self.overall_rejections += 1;
    }

    fn is_successful(&self) -> bool {
        const SUFFICIENT_MASS_THRESHOLD: f64 = 0.8;
        const LOW_REJECTION_FRACTION: f64 = 0.2;

        // We consider the operation successful if either mass occupation is above 80% or rejection rate is below 20%
        self.overall_rejections == 0
            || (self.total_mass as f64) > self.policy.max_block_mass as f64 * SUFFICIENT_MASS_THRESHOLD
            || (self.overall_rejections as f64) < self.transactions.len() as f64 * LOW_REJECTION_FRACTION
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutils::script::op_true_script;
    use itertools::Itertools;
    use spora_consensus_core::{
        constants::{MAX_TX_IN_SEQUENCE_NUM, SAU_PER_SPORA},
        mass::cell_tx_estimated_serialized_size,
        tx::{pay_to_script_hash_witness_script, CellOut, CellRef, CellTx, TransactionId, TransactionOutpoint},
    };
    use std::{collections::HashSet, sync::Arc};

    use crate::{
        mempool::{
            config::DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE,
            model::frontier::selectors::{SequenceSelector, SequenceSelectorInput, SequenceSelectorTransaction},
        },
        model::candidate_tx::CandidateTransaction,
    };

    #[test]
    fn test_reject_transaction() {
        const TX_INITIAL_COUNT: usize = 1_000;

        // Create a vector of transactions differing by output value so they have unique ids
        let transactions = (0..TX_INITIAL_COUNT).map(|i| create_transaction(SAU_PER_SPORA * (i + 1) as u64)).collect_vec();
        let masses: HashMap<_, _> = transactions.iter().map(|tx| (tx.tx.id(), tx.calculated_mass)).collect();
        let sequence: SequenceSelectorInput = transactions
            .iter()
            .map(|tx| SequenceSelectorTransaction::new(tx.tx.clone(), tx.cell_tx.clone(), tx.calculated_mass))
            .collect();

        let policy = Policy::new(100_000);
        let selectors: [Box<dyn TemplateTransactionSelector>; 2] = [
            Box::new(RebalancingWeightedTransactionSelector::new(policy.clone(), transactions)),
            Box::new(SequenceSelector::new(sequence, policy.clone())),
        ];

        for mut selector in selectors {
            let (mut kept, mut rejected) = (HashSet::new(), HashSet::new());
            let mut reject_count = 32;
            let mut total_mass = 0;
            for i in 0..10 {
                let selected_txs = selector.select_transactions();
                if i > 0 {
                    assert_eq!(
                        selected_txs.len(),
                        reject_count,
                        "subsequent select calls are expected to only refill the previous rejections"
                    );
                    reject_count /= 2;
                }
                for tx in selected_txs.iter() {
                    total_mass += masses[&tx.id()];
                    kept.insert(tx.id()).then_some(()).expect("selected txs should never repeat themselves");
                    assert!(!rejected.contains(&tx.id()), "selected txs should never repeat themselves");
                }
                assert!(total_mass <= policy.max_block_mass);
                selected_txs.iter().take(reject_count).for_each(|x| {
                    total_mass -= masses[&x.id()];
                    selector.reject_selection(x.id().into());
                    kept.remove(&x.id()).then_some(()).expect("was just inserted");
                    rejected.insert(x.id()).then_some(()).expect("was just verified");
                });
            }
        }
    }

    fn create_transaction(value: u64) -> CandidateTransaction {
        let previous_outpoint = TransactionOutpoint::new(TransactionId::default().as_bytes(), 0);
        let (lock_script, redeem_script) = op_true_script();
        let witness_script = pay_to_script_hash_witness_script(&redeem_script, vec![]).expect("the redeem script is canonical");

        let tx = Arc::new(
            CellTx::new(
                vec![CellRef::new(previous_outpoint, MAX_TX_IN_SEQUENCE_NUM)],
                vec![],
                vec![CellOut { lock: lock_script, type_: None, capacity: value - DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE }],
                vec![vec![]],
                vec![witness_script],
            )
            .expect("test helper must construct a valid CellTx"),
        );
        let calculated_mass = cell_tx_estimated_serialized_size(tx.as_ref());
        let calculated_fee = DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE;
        CandidateTransaction {
            tx: tx.clone(),
            cell_tx: tx,
            calculated_fee,
            calculated_mass,
            cell_score_total: None,
            cell_fee_density: None,
            cell_deps_width: None,
        }
    }
}
