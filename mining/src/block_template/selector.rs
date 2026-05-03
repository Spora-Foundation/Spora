use rand::Rng;
use spora_core::{time::Stopwatch, trace};
use std::collections::HashMap;

use crate::model::candidate_tx::CandidateTransaction;

use super::{
    model::tx::{CandidateList, SelectableTransaction, SelectableTransactions, TransactionIndex},
    policy::Policy,
};
use spora_consensus_core::{
    block::{CellScriptSchedulerAccessSets, TemplateTransactionSelector},
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
    selected_cellscript_scheduler_accesses: CellScriptSchedulerAccessSets,

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
            selected_cellscript_scheduler_accesses: Default::default(),
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
        self.selected_cellscript_scheduler_accesses = self
            .selected_txs
            .iter()
            .filter_map(|&tx_index| {
                let tx = &self.transactions[tx_index];
                tx.cellscript_scheduler_accesses.clone().map(|accesses| (tx.tx.id().into(), accesses))
            })
            .collect();

        self.get_transactions()
    }

    fn get_transactions(&self) -> Vec<CellTx> {
        // These transactions leave the selector so we clone
        self.selected_txs.iter().map(|x| self.transactions[*x].tx.as_ref().clone()).collect()
    }

    fn reset_selection(&mut self) {
        assert_eq!(self.transactions.len(), self.selectable_txs.len());
        self.selected_txs.clear();
        self.selected_txs.reserve_exact(self.estimated_selection_capacity());
        self.selected_txs_map = None;
        self.selected_cellscript_scheduler_accesses.clear();
    }

    fn estimated_selection_capacity(&self) -> usize {
        let Some(min_mass) = self
            .transactions
            .iter()
            .filter_map(|transaction| (transaction.calculated_mass > 0).then_some(transaction.calculated_mass))
            .min()
        else {
            return self.transactions.len();
        };

        let approx_fit = self.policy.max_block_mass.saturating_add(min_mass.saturating_sub(1)).saturating_div(min_mass);
        usize::try_from(approx_fit).unwrap_or(usize::MAX).min(self.transactions.len())
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

    fn selected_cellscript_scheduler_accesses(&self) -> CellScriptSchedulerAccessSets {
        self.selected_cellscript_scheduler_accesses.clone()
    }

    fn reject_selection(&mut self, tx_id: TransactionId) {
        let selected_txs_map = self
            .selected_txs_map
            // We lazy-create the map only when there are actual rejections
            .get_or_insert_with(|| self.selected_txs.iter().map(|&x| (self.transactions[x].tx.id().into(), x)).collect());
        let tx_index = selected_txs_map.remove(&tx_id).expect("only previously selected txs can be rejected (and only once)");
        self.selected_cellscript_scheduler_accesses.remove(&tx_id);
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
        block::CellScriptSchedulerAccessList,
        constants::{MAX_TX_IN_SEQUENCE_NUM, SAU_PER_SPORA},
        mass::cell_tx_estimated_serialized_size,
        tx::{CellInput, CellOutput, CellTx, TransactionId, TransactionOutpoint},
    };
    use spora_exec::celltx::{
        CellScriptSchedulerAccessWitness, CellScriptSchedulerWitness, CELLSCRIPT_SCHEDULER_EFFECT_CREATING,
        CELLSCRIPT_SCHEDULER_OP_CREATE, CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT, CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
    };
    use std::{collections::HashSet, sync::Arc};

    use crate::{
        mempool::{
            config::DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE,
            model::frontier::selectors::{SequenceSelector, SequenceSelectorInput, SequenceSelectorTransaction, TakeAllSelector},
        },
        model::candidate_tx::{CandidateCellData, CandidateTransaction},
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
        let (lock_script, _witness) = op_true_script();

        let tx = Arc::new(
            CellTx::new(
                vec![CellInput::new(previous_outpoint, MAX_TX_IN_SEQUENCE_NUM)],
                vec![],
                vec![CellOutput { lock: lock_script, type_: None, capacity: value - DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE }],
                vec![vec![]],
                vec![vec![]],
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
            cellscript_scheduler_accesses: None,
        }
    }

    fn scheduler_accesses(marker: u8) -> CellScriptSchedulerAccessList {
        scheduler_summary(vec![CellScriptSchedulerAccessWitness {
            operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
            source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
            index: 0,
            conflict_hash: [marker; 32],
            typed_data_hash: [0x00; 32],
        }])
    }

    fn scheduler_summary(accesses: Vec<CellScriptSchedulerAccessWitness>) -> CellScriptSchedulerAccessList {
        CellScriptSchedulerWitness {
            magic: 0xCE11,
            version: CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
            effect_class: CELLSCRIPT_SCHEDULER_EFFECT_CREATING,
            parallelizable: false,
            estimated_cycles: 64,
            access_count: accesses.len() as u32,
            accesses,
        }
    }

    #[test]
    fn test_selectors_expose_cellscript_scheduler_sidecars() {
        let mut transactions = vec![create_transaction(SAU_PER_SPORA), create_transaction(SAU_PER_SPORA * 2)];
        let tx_id = transactions[0].tx.id().into();
        let empty_tx_id = transactions[1].tx.id().into();
        let accesses = scheduler_accesses(0x42);
        let empty_accesses = scheduler_summary(vec![]);
        transactions[0].cellscript_scheduler_accesses = Some(accesses.clone());
        transactions[1].cellscript_scheduler_accesses = Some(empty_accesses.clone());

        let sequence: SequenceSelectorInput = transactions
            .iter()
            .map(|tx| {
                SequenceSelectorTransaction::new_with_cellscript_scheduler_accesses(
                    tx.tx.clone(),
                    tx.cell_tx.clone(),
                    tx.calculated_mass,
                    tx.cellscript_scheduler_accesses.clone(),
                )
            })
            .collect();
        let mut sequence_selector = SequenceSelector::new(sequence, Policy::new(100_000));
        sequence_selector.select_transactions();
        assert_eq!(sequence_selector.selected_cellscript_scheduler_accesses().get(&tx_id), Some(&accesses));
        assert_eq!(sequence_selector.selected_cellscript_scheduler_accesses().get(&empty_tx_id), Some(&empty_accesses));

        let mut rebalancing_selector = RebalancingWeightedTransactionSelector::new(Policy::new(100_000), transactions.clone());
        rebalancing_selector.select_transactions();
        assert_eq!(rebalancing_selector.selected_cellscript_scheduler_accesses().get(&tx_id), Some(&accesses));
        assert_eq!(rebalancing_selector.selected_cellscript_scheduler_accesses().get(&empty_tx_id), Some(&empty_accesses));

        let cells = transactions
            .into_iter()
            .map(|tx| CandidateCellData {
                cell_tx: tx.cell_tx,
                score_total: None,
                fee_density: None,
                deps_width: None,
                cellscript_scheduler_accesses: tx.cellscript_scheduler_accesses,
            })
            .collect();
        let mut take_all_selector = TakeAllSelector::from_cell_data(cells);
        take_all_selector.select_transactions();
        assert_eq!(take_all_selector.selected_cellscript_scheduler_accesses().get(&tx_id), Some(&accesses));
        assert_eq!(take_all_selector.selected_cellscript_scheduler_accesses().get(&empty_tx_id), Some(&empty_accesses));
    }

    #[test]
    fn test_selectors_remove_cellscript_scheduler_sidecars_on_reject() {
        let mut transactions =
            vec![create_transaction(SAU_PER_SPORA), create_transaction(SAU_PER_SPORA * 2), create_transaction(SAU_PER_SPORA * 3)];
        let rejected_tx_id: TransactionId = transactions[0].tx.id().into();
        let kept_tx_id: TransactionId = transactions[1].tx.id().into();
        let plain_tx_id: TransactionId = transactions[2].tx.id().into();
        let rejected_accesses = scheduler_accesses(0x52);
        let kept_accesses = scheduler_accesses(0x53);
        transactions[0].cellscript_scheduler_accesses = Some(rejected_accesses.clone());
        transactions[1].cellscript_scheduler_accesses = Some(kept_accesses.clone());
        transactions[2].cellscript_scheduler_accesses = None;

        let sequence: SequenceSelectorInput = transactions
            .iter()
            .map(|tx| {
                SequenceSelectorTransaction::new_with_cellscript_scheduler_accesses(
                    tx.tx.clone(),
                    tx.cell_tx.clone(),
                    tx.calculated_mass,
                    tx.cellscript_scheduler_accesses.clone(),
                )
            })
            .collect();
        let mut sequence_selector = SequenceSelector::new(sequence, Policy::new(100_000));
        let selected = sequence_selector.select_transactions();
        assert_eq!(selected.len(), 3);
        assert_eq!(sequence_selector.selected_cellscript_scheduler_accesses().get(&rejected_tx_id), Some(&rejected_accesses));
        assert_eq!(sequence_selector.selected_cellscript_scheduler_accesses().get(&kept_tx_id), Some(&kept_accesses));
        assert!(!sequence_selector.selected_cellscript_scheduler_accesses().contains_key(&plain_tx_id));
        sequence_selector.reject_selection(rejected_tx_id);
        assert!(!sequence_selector.selected_cellscript_scheduler_accesses().contains_key(&rejected_tx_id));
        assert_eq!(sequence_selector.selected_cellscript_scheduler_accesses().get(&kept_tx_id), Some(&kept_accesses));

        let mut rebalancing_selector = RebalancingWeightedTransactionSelector::new(Policy::new(100_000), transactions.clone());
        let selected = rebalancing_selector.select_transactions();
        assert_eq!(selected.len(), 3);
        assert_eq!(rebalancing_selector.selected_cellscript_scheduler_accesses().get(&rejected_tx_id), Some(&rejected_accesses));
        assert_eq!(rebalancing_selector.selected_cellscript_scheduler_accesses().get(&kept_tx_id), Some(&kept_accesses));
        assert!(!rebalancing_selector.selected_cellscript_scheduler_accesses().contains_key(&plain_tx_id));
        rebalancing_selector.reject_selection(rejected_tx_id);
        assert!(!rebalancing_selector.selected_cellscript_scheduler_accesses().contains_key(&rejected_tx_id));
        assert_eq!(rebalancing_selector.selected_cellscript_scheduler_accesses().get(&kept_tx_id), Some(&kept_accesses));

        let cells = transactions
            .into_iter()
            .map(|tx| CandidateCellData {
                cell_tx: tx.cell_tx,
                score_total: None,
                fee_density: None,
                deps_width: None,
                cellscript_scheduler_accesses: tx.cellscript_scheduler_accesses,
            })
            .collect();
        let mut take_all_selector = TakeAllSelector::from_cell_data(cells);
        let selected = take_all_selector.select_transactions();
        assert_eq!(selected.len(), 3);
        assert_eq!(take_all_selector.selected_cellscript_scheduler_accesses().get(&rejected_tx_id), Some(&rejected_accesses));
        assert_eq!(take_all_selector.selected_cellscript_scheduler_accesses().get(&kept_tx_id), Some(&kept_accesses));
        assert!(!take_all_selector.selected_cellscript_scheduler_accesses().contains_key(&plain_tx_id));
        take_all_selector.reject_selection(rejected_tx_id);
        assert!(!take_all_selector.selected_cellscript_scheduler_accesses().contains_key(&rejected_tx_id));
        assert_eq!(take_all_selector.selected_cellscript_scheduler_accesses().get(&kept_tx_id), Some(&kept_accesses));
    }

    #[test]
    fn test_reset_selection_reserves_only_estimated_fit_count() {
        let mut transactions = (0..64).map(|i| create_transaction(SAU_PER_SPORA * (i + 1) as u64)).collect_vec();
        for tx in &mut transactions {
            tx.calculated_mass = 10_000;
        }

        let mut selector = RebalancingWeightedTransactionSelector::new(Policy::new(25_000), transactions);
        selector.reset_selection();

        assert_eq!(selector.selected_txs.capacity(), 3);
    }
}
