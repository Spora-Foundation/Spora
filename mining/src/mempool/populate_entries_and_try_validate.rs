use crate::{
    cell_conversion::cell_output_to_metadata,
    mempool::{errors::RuleResult, model::{pool::Pool, tx::MempoolTransaction}, Mempool},
};
use spora_consensus_core::{
    api::{
        args::{TransactionValidationArgs, TransactionValidationBatchArgs},
        ConsensusApi,
    },
    constants::UNACCEPTED_DAA_SCORE,
    tx::{MutableTransaction, OutPointCompat},
};
use spora_mining_errors::mempool::RuleError;

impl Mempool {
    pub(crate) fn populate_mempool_entries(&self, transaction: &mut MutableTransaction) {
        for (i, input) in transaction.tx.inputs.iter().enumerate() {
            if let Some(parent) = self.transaction_pool.get(&input.out_point.transaction_id()) {
                let output_index = input.out_point.index as usize;
                if let Some(cell_tx) = parent.cell_tx() {
                    if let Some(output) = cell_tx.outputs.get(output_index) {
                        let output_data = cell_tx.outputs_data.get(output_index).map(Vec::as_slice).unwrap_or(&[]);
                        transaction.resolved_cell_metadata[i] =
                            Some(cell_output_to_metadata(input.out_point, output, output_data, UNACCEPTED_DAA_SCORE, false));
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

pub(crate) fn validate_mempool_cell_transaction(
    consensus: &dyn ConsensusApi,
    transaction: &mut MutableTransaction,
    cell_tx: &spora_consensus_core::tx::CellTx,
    args: &TransactionValidationArgs,
) -> RuleResult<()> {
    Ok(consensus.validate_mempool_cell_transaction(transaction, cell_tx, args)?)
}

pub(crate) fn validate_mempool_transactions_in_parallel(
    consensus: &dyn ConsensusApi,
    transactions: &mut [MutableTransaction],
    args: &TransactionValidationBatchArgs,
) -> Vec<RuleResult<()>> {
    consensus.validate_mempool_transactions_in_parallel(transactions, args).into_iter().map(|x| x.map_err(RuleError::from)).collect()
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
                let cell_tx = transaction
                    .cell_tx()
                    .expect("canonical mempool transactions must retain their canonical CellTx");
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
