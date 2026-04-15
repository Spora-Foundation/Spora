use crate::{
    cell_conversion::cell_output_to_metadata,
    mempool::{
        errors::RuleResult,
        model::{pool::Pool, tx::MempoolTransaction},
        Mempool,
    },
};
use spora_consensus_core::{
    api::{
        args::{TransactionValidationArgs, TransactionValidationBatchArgs},
        ConsensusApi,
    },
    constants::UNACCEPTED_DAA_SCORE,
    tx::{MutableTransaction, TransactionId},
};
use spora_mining_errors::mempool::RuleError;

impl Mempool {
    pub(crate) fn populate_mempool_entries(&self, transaction: &mut MutableTransaction) {
        for (i, input) in transaction.tx.inputs.iter().enumerate() {
            let previous_tx_id = self
                .transaction_pool
                .get_mempool_cell_owner_id(&input.previous_output)
                .copied()
                .unwrap_or_else(|| TransactionId::from_bytes(input.previous_output.tx_hash));
            if let Some(parent) = self.transaction_pool.get(&previous_tx_id) {
                let output_index = input.previous_output.index as usize;
                if let Some(cell_tx) = parent.cell_tx() {
                    if let Some(output) = cell_tx.outputs.get(output_index) {
                        let output_data = cell_tx.outputs_data.get(output_index).map(Vec::as_slice).unwrap_or(&[]);
                        transaction.resolved_cell_metadata[i] =
                            Some(cell_output_to_metadata(input.previous_output, output, output_data, UNACCEPTED_DAA_SCORE, false));
                        continue;
                    }
                }

                transaction.entries[i] = parent.mtx.entries.get(output_index).and_then(Clone::clone);
            }
        }
    }
}

pub(crate) fn validate_mempool_transaction(
    consensus: &dyn ConsensusApi,
    transaction: &mut MutableTransaction,
    args: &TransactionValidationArgs,
) -> RuleResult<()> {
    Ok(consensus.validate_mempool_transaction(transaction, args)?)
}

/// Validate a canonical [`CellTx`](spora_consensus_core::tx::CellTx) against the
/// current virtual-state snapshot.
///
/// This is the Cell-model counterpart to [`validate_mempool_transaction`].
/// The validation flow is intentionally split in two stages:
///
/// 1. `populate_mempool_entries()` resolves any parents already living in the
///    local mempool so the mutable transaction has the same entry metadata view
///    as the canonical CellTx.
/// 2. `consensus.validate_mempool_cell_transaction()` replays the full
///    consensus-side Cell validation path against virtual state, including
///    isolation, context, DAG/time-lock, and script verification.
///
/// Keeping the canonical `CellTx` attached to [`MempoolTransaction`] avoids any
/// lossy fallback to legacy transaction-only validation when a Cell-native
/// transaction enters the mempool.
pub(crate) fn validate_mempool_cell_transaction(
    consensus: &dyn ConsensusApi,
    transaction: &mut MutableTransaction,
    cell_tx: &spora_consensus_core::tx::CellTx,
    args: &TransactionValidationArgs,
) -> RuleResult<()> {
    Ok(consensus.validate_mempool_cell_transaction(transaction, cell_tx, args)?)
}

pub(crate) fn validate_mempool_mempool_transactions_in_parallel(
    consensus: &dyn ConsensusApi,
    transactions: &mut [MempoolTransaction],
    args: &TransactionValidationBatchArgs,
) -> Vec<RuleResult<()>> {
    transactions
        .iter_mut()
        .map(|transaction: &mut MempoolTransaction| {
            let validation_args = args.get(&transaction.id());
            if transaction.has_canonical_cell_tx() {
                // Cell-native transactions keep their canonical representation all
                // the way into batch revalidation so they continue to use the
                // POV-aware Cell validation path.
                let cell_tx = transaction.cell_tx().expect("canonical mempool transactions must retain their canonical CellTx");
                validate_mempool_cell_transaction(consensus, &mut transaction.mtx, cell_tx.as_ref(), validation_args)
            } else {
                validate_mempool_transaction(consensus, &mut transaction.mtx, validation_args)
            }
        })
        .collect()
}

#[allow(dead_code)]
pub(crate) fn populate_mempool_transactions_in_parallel(
    consensus: &dyn ConsensusApi,
    transactions: &mut [MutableTransaction],
) -> Vec<RuleResult<()>> {
    consensus.populate_mempool_transactions_in_parallel(transactions).into_iter().map(|x| x.map_err(RuleError::from)).collect()
}
