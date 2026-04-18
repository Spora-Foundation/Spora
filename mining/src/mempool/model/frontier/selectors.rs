use crate::Policy;
use spora_consensus_core::{
    block::{CellScriptSchedulerAccessList, CellScriptSchedulerAccessSets, TemplateTransactionSelector},
    tx::{CellTx, TransactionId},
};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

pub struct SequenceSelectorTransaction {
    pub tx: Arc<CellTx>,
    pub cell_tx: Arc<CellTx>,
    pub mass: u64,
    pub cellscript_scheduler_accesses: Option<CellScriptSchedulerAccessList>,
}

impl SequenceSelectorTransaction {
    pub fn new(tx: Arc<CellTx>, cell_tx: Arc<CellTx>, mass: u64) -> Self {
        Self { tx, cell_tx, mass, cellscript_scheduler_accesses: None }
    }

    pub fn new_with_cellscript_scheduler_accesses(
        tx: Arc<CellTx>,
        cell_tx: Arc<CellTx>,
        mass: u64,
        cellscript_scheduler_accesses: Option<CellScriptSchedulerAccessList>,
    ) -> Self {
        Self { tx, cell_tx, mass, cellscript_scheduler_accesses }
    }
}

type SequencePriorityIndex = u32;

/// The input sequence for the [`SequenceSelector`] transaction selector
#[derive(Default)]
pub struct SequenceSelectorInput {
    /// We use the btree map ordered by insertion order in order to follow
    /// the initial sequence order while allowing for efficient removal of previous selections
    inner: BTreeMap<SequencePriorityIndex, SequenceSelectorTransaction>,
}

impl FromIterator<SequenceSelectorTransaction> for SequenceSelectorInput {
    fn from_iter<T: IntoIterator<Item = SequenceSelectorTransaction>>(iter: T) -> Self {
        Self { inner: BTreeMap::from_iter(iter.into_iter().enumerate().map(|(i, v)| (i as SequencePriorityIndex, v))) }
    }
}

impl SequenceSelectorInput {
    pub fn push(
        &mut self,
        tx: Arc<CellTx>,
        cell_tx: Arc<CellTx>,
        mass: u64,
        cellscript_scheduler_accesses: Option<CellScriptSchedulerAccessList>,
    ) {
        let idx = self.inner.len() as SequencePriorityIndex;
        self.inner.insert(
            idx,
            SequenceSelectorTransaction::new_with_cellscript_scheduler_accesses(tx, cell_tx, mass, cellscript_scheduler_accesses),
        );
    }

    pub fn iter(&self) -> impl Iterator<Item = &SequenceSelectorTransaction> {
        self.inner.values()
    }
}

/// Helper struct for storing data related to previous selections
struct SequenceSelectorSelection {
    tx_id: TransactionId,
    mass: u64,
}

/// A selector which selects transactions in the order they are provided. The selector assumes
/// that the transactions were already selected via weighted sampling and simply tries them one
/// after the other until the block mass limit is reached.  
pub struct SequenceSelector {
    input_sequence: SequenceSelectorInput,
    selected_vec: Vec<SequenceSelectorSelection>,
    /// Maps from selected tx ids to tx mass so that the total used mass can be subtracted on tx reject
    selected_map: Option<HashMap<TransactionId, u64>>,
    selected_cellscript_scheduler_accesses: CellScriptSchedulerAccessSets,
    total_selected_mass: u64,
    overall_candidates: usize,
    overall_rejections: usize,
    next_candidate_index: SequencePriorityIndex,
    policy: Policy,
}

impl SequenceSelector {
    pub fn new(input_sequence: SequenceSelectorInput, policy: Policy) -> Self {
        Self {
            overall_candidates: input_sequence.inner.len(),
            selected_vec: Vec::with_capacity(input_sequence.inner.len()),
            input_sequence,
            selected_map: Default::default(),
            selected_cellscript_scheduler_accesses: Default::default(),
            total_selected_mass: Default::default(),
            overall_rejections: Default::default(),
            next_candidate_index: 0,
            policy,
        }
    }

    #[inline]
    fn reset_selection(&mut self) {
        self.selected_vec.clear();
        self.selected_map = None;
        self.selected_cellscript_scheduler_accesses.clear();
    }
}

impl TemplateTransactionSelector for SequenceSelector {
    fn select_transactions(&mut self) -> Vec<CellTx> {
        self.reset_selection();

        let mut selected_candidates = Vec::new();
        for (&priority_index, candidate) in self.input_sequence.inner.range(self.next_candidate_index..) {
            let Some(next_total_mass) = self.total_selected_mass.checked_add(candidate.mass) else {
                break;
            };
            if next_total_mass > self.policy.max_block_mass {
                break;
            }

            self.total_selected_mass = next_total_mass;
            self.next_candidate_index = priority_index + 1;
            selected_candidates.push((
                priority_index,
                candidate.mass,
                candidate.cell_tx.clone(),
                candidate.cellscript_scheduler_accesses.clone(),
            ));
        }

        let selected = selected_candidates.iter().map(|(_, _, cell_tx, _)| cell_tx.as_ref().clone()).collect::<Vec<_>>();
        self.selected_vec = selected_candidates
            .iter()
            .zip(selected.iter())
            .map(|((_, mass, _, _), cell_tx)| SequenceSelectorSelection { tx_id: cell_tx.id().into(), mass: *mass })
            .collect();
        self.selected_map = Some(self.selected_vec.iter().map(|tx| (tx.tx_id, tx.mass)).collect());
        self.selected_cellscript_scheduler_accesses = selected_candidates
            .into_iter()
            .filter_map(|(_, _, cell_tx, accesses)| accesses.map(|accesses| (cell_tx.id().into(), accesses)))
            .collect();
        selected
    }

    fn selected_cellscript_scheduler_accesses(&self) -> CellScriptSchedulerAccessSets {
        self.selected_cellscript_scheduler_accesses.clone()
    }

    fn reject_selection(&mut self, tx_id: TransactionId) {
        // Lazy-create the map only when there are actual rejections
        let selected_map = self.selected_map.get_or_insert_with(|| self.selected_vec.iter().map(|tx| (tx.tx_id, tx.mass)).collect());
        let mass = selected_map.remove(&tx_id).expect("only previously selected txs can be rejected (and only once)");
        self.selected_cellscript_scheduler_accesses.remove(&tx_id);
        // Selections must be counted in total selected mass, so this subtraction cannot underflow
        self.total_selected_mass -= mass;
        self.overall_rejections += 1;
    }

    fn is_successful(&self) -> bool {
        const SUFFICIENT_MASS_THRESHOLD: f64 = 0.8;
        const LOW_REJECTION_FRACTION: f64 = 0.2;

        // We consider the operation successful if either mass occupation is above 80% or rejection rate is below 20%
        self.overall_rejections == 0
            || (self.total_selected_mass as f64) > self.policy.max_block_mass as f64 * SUFFICIENT_MASS_THRESHOLD
            || (self.overall_rejections as f64) < self.overall_candidates as f64 * LOW_REJECTION_FRACTION
    }
}

/// A selector that selects all the transactions it holds and is always considered successful.
/// If all mempool transactions have combined mass which is <= block mass limit, this selector
/// should be called and provided with all the transactions.
pub struct TakeAllSelector {
    txs: Vec<Arc<CellTx>>,
    cellscript_scheduler_accesses: CellScriptSchedulerAccessSets,
    selected_cellscript_scheduler_accesses: CellScriptSchedulerAccessSets,
}

impl TakeAllSelector {
    pub fn new(txs: Vec<Arc<CellTx>>) -> Self {
        Self {
            txs,
            cellscript_scheduler_accesses: CellScriptSchedulerAccessSets::new(),
            selected_cellscript_scheduler_accesses: CellScriptSchedulerAccessSets::new(),
        }
    }

    pub fn from_cell_data(cells: Vec<crate::model::candidate_tx::CandidateCellData>) -> Self {
        let mut cellscript_scheduler_accesses = CellScriptSchedulerAccessSets::new();
        let txs = cells
            .into_iter()
            .map(|cell| {
                if let Some(accesses) = cell.cellscript_scheduler_accesses {
                    cellscript_scheduler_accesses.insert(cell.cell_tx.id().into(), accesses);
                }
                cell.cell_tx
            })
            .collect();
        Self { txs, cellscript_scheduler_accesses, selected_cellscript_scheduler_accesses: CellScriptSchedulerAccessSets::new() }
    }

    #[cfg(test)]
    pub fn from_cell_txs(txs: Vec<CellTx>) -> Self {
        Self::new(txs.into_iter().map(Arc::new).collect())
    }
}

impl TemplateTransactionSelector for TakeAllSelector {
    fn select_transactions(&mut self) -> Vec<CellTx> {
        self.selected_cellscript_scheduler_accesses = std::mem::take(&mut self.cellscript_scheduler_accesses);
        std::mem::take(&mut self.txs).into_iter().map(|tx| tx.as_ref().clone()).collect()
    }

    fn selected_cellscript_scheduler_accesses(&self) -> CellScriptSchedulerAccessSets {
        self.selected_cellscript_scheduler_accesses.clone()
    }

    fn reject_selection(&mut self, tx_id: TransactionId) {
        self.selected_cellscript_scheduler_accesses.remove(&tx_id);
        // No need to track rejections (for reduced mass), since there's nothing else to select
    }

    fn is_successful(&self) -> bool {
        // Considered successful because we provided all mempool transactions to this
        // selector, so there's no point in retries
        true
    }
}
