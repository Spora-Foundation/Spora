use std::collections::HashSet;

use crate::mempool::{
    errors::RuleResult,
    model::{
        map::OutpointIndex,
        tx::{DoubleSpend, MempoolTransaction},
    },
};
use spora_consensus_core::tx::{MutableTransaction, TransactionId, TransactionOutpoint};

pub(crate) struct MempoolCellSet {
    spent_outpoint_owner_id: OutpointIndex,
    created_outpoint_owner_id: OutpointIndex,
}

impl MempoolCellSet {
    pub(crate) fn new() -> Self {
        Self { spent_outpoint_owner_id: OutpointIndex::default(), created_outpoint_owner_id: OutpointIndex::default() }
    }

    fn track_created_outpoints(&mut self, outpoint_tx_id: TransactionId, owner_id: TransactionId, output_count: usize) {
        for index in 0..output_count {
            self.created_outpoint_owner_id.insert(TransactionOutpoint::new(outpoint_tx_id.as_bytes(), index as u32), owner_id);
        }
    }

    fn remove_created_outpoints(&mut self, outpoint_tx_id: TransactionId, output_count: usize) {
        for index in 0..output_count {
            self.created_outpoint_owner_id.remove(&TransactionOutpoint::new(outpoint_tx_id.as_bytes(), index as u32));
        }
    }

    pub(crate) fn add_transaction(&mut self, transaction: &MempoolTransaction) {
        let transaction_id = transaction.id();

        for input in transaction.mtx.tx.inputs.iter() {
            // Track spent inputs immediately for double-spend detection.
            self.spent_outpoint_owner_id.insert(input.previous_output, transaction_id);
        }

        let output_count = transaction.mtx.tx.outputs.len();
        self.track_created_outpoints(transaction_id, transaction_id, output_count);
        if let Some(cell_tx_id) = transaction.cell_tx_id() {
            if cell_tx_id != transaction_id {
                self.track_created_outpoints(cell_tx_id, transaction_id, output_count);
            }
        }
    }

    pub(crate) fn remove_transaction(&mut self, transaction: &MempoolTransaction) {
        for input in transaction.mtx.tx.inputs.iter() {
            self.spent_outpoint_owner_id.remove(&input.previous_output);
        }

        let transaction_id = transaction.id();
        let output_count = transaction.mtx.tx.outputs.len();
        self.remove_created_outpoints(transaction_id, output_count);
        if let Some(cell_tx_id) = transaction.cell_tx_id() {
            if cell_tx_id != transaction_id {
                self.remove_created_outpoints(cell_tx_id, output_count);
            }
        }
    }

    pub(crate) fn get_outpoint_owner_id(&self, outpoint: &TransactionOutpoint) -> Option<&TransactionId> {
        self.spent_outpoint_owner_id.get(outpoint)
    }

    pub(crate) fn get_mempool_cell_owner_id(&self, outpoint: &TransactionOutpoint) -> Option<&TransactionId> {
        self.created_outpoint_owner_id.get(outpoint)
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
            if let Some(existing_transaction_id) = self.get_outpoint_owner_id(&input.previous_output) {
                if *existing_transaction_id != transaction_id {
                    return Some(DoubleSpend::new(input.previous_output, *existing_transaction_id));
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
            if let Some(existing_transaction_id) = self.get_outpoint_owner_id(&input.previous_output) {
                if *existing_transaction_id != transaction_id && visited.insert(*existing_transaction_id) {
                    double_spends.push(DoubleSpend::new(input.previous_output, *existing_transaction_id));
                }
            }
        }
        double_spends
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mempool::model::tx::MempoolTransaction;
    use crate::mempool::tx::Priority;
    use spora_consensus_core::{
        mass::{ContextualMasses, NonContextualMasses},
        tx::{CellInput, CellOutput, CellTx, Script},
    };

    fn build_transaction(input: TransactionOutpoint, output_capacity: u64) -> MutableTransaction {
        let lock_script = Script::new([0; 32], 0, vec![0x51]);
        let output = CellOutput { lock: lock_script, type_: None, capacity: output_capacity };
        let tx = CellTx::new(vec![CellInput::new(input, 0)], vec![], vec![output], vec![vec![]], vec![vec![1]]).unwrap();
        let mut mtx = MutableTransaction::from_cell_tx(tx);
        mtx.calculated_fee = Some(1_000);
        mtx.calculated_non_contextual_masses = Some(NonContextualMasses::new(100, 50));
        mtx.calculated_contextual_masses = Some(ContextualMasses::new(75));
        mtx.verified_cycles = Some(321);
        mtx
    }

    #[test]
    fn tracks_created_mempool_outputs_alongside_spent_inputs() {
        let mut cell_set = MempoolCellSet::new();

        let parent = MempoolTransaction::new(
            build_transaction(TransactionOutpoint::new(TransactionId::default().as_bytes(), 0), 9_000),
            Priority::Low,
            0,
        );
        let parent_id = parent.id();
        let parent_output = TransactionOutpoint::new(parent_id.as_bytes(), 0);

        cell_set.add_transaction(&parent);
        assert_eq!(cell_set.get_mempool_cell_owner_id(&parent_output), Some(&parent_id));
        assert!(cell_set.get_outpoint_owner_id(&parent_output).is_none());

        let child = MempoolTransaction::new(build_transaction(parent_output, 8_000), Priority::Low, 0);
        let child_id = child.id();
        cell_set.add_transaction(&child);

        assert_eq!(cell_set.get_outpoint_owner_id(&parent_output), Some(&child_id));
        assert_eq!(cell_set.get_mempool_cell_owner_id(&parent_output), Some(&parent_id));

        cell_set.remove_transaction(&child);
        assert!(cell_set.get_outpoint_owner_id(&parent_output).is_none());
        assert_eq!(cell_set.get_mempool_cell_owner_id(&parent_output), Some(&parent_id));

        cell_set.remove_transaction(&parent);
        assert!(cell_set.get_mempool_cell_owner_id(&parent_output).is_none());
    }
}
