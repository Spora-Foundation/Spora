use crate::FeerateTransactionKey;
use spora_consensus_core::{block::CellScriptSchedulerAccessList, tx::CellTx};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct CandidateCellData {
    pub cell_tx: Arc<CellTx>,
    pub score_total: Option<f64>,
    pub fee_density: Option<f64>,
    pub deps_width: Option<f64>,
    pub cellscript_scheduler_accesses: Option<CellScriptSchedulerAccessList>,
}

/// Transaction with additional metadata needed in order to be a candidate
/// in the transaction selection algorithm
#[derive(Clone, Debug)]
pub struct CandidateTransaction {
    /// The actual transaction
    pub tx: Arc<CellTx>,
    /// Mirrored Cell transaction
    pub cell_tx: Arc<CellTx>,
    /// Populated fee
    pub calculated_fee: u64,
    /// Populated mass
    pub calculated_mass: u64,
    /// Optional CellPool-native total score
    pub cell_score_total: Option<f64>,
    /// Optional CellPool-native fee density
    pub cell_fee_density: Option<f64>,
    /// Optional CellPool-native dependency width
    pub cell_deps_width: Option<f64>,
    /// Optional trusted CellScript scheduler summary.
    pub cellscript_scheduler_accesses: Option<CellScriptSchedulerAccessList>,
}

impl CandidateTransaction {
    pub fn from_key_and_cell(key: FeerateTransactionKey, cell_tx: Arc<CellTx>) -> Self {
        Self::from_key_and_cell_data(
            key,
            CandidateCellData { cell_tx, score_total: None, fee_density: None, deps_width: None, cellscript_scheduler_accesses: None },
        )
    }

    pub fn from_key_and_cell_data(key: FeerateTransactionKey, cell: CandidateCellData) -> Self {
        Self {
            tx: key.tx,
            cell_tx: cell.cell_tx,
            calculated_fee: key.fee,
            calculated_mass: key.mass,
            cell_score_total: cell.score_total,
            cell_fee_density: cell.fee_density,
            cell_deps_width: cell.deps_width,
            cellscript_scheduler_accesses: cell.cellscript_scheduler_accesses,
        }
    }
}
