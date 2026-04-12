use spora_consensus_core::tx::CellTx;
use std::sync::Arc;

#[derive(Debug)]
pub struct TransactionInsertion {
    pub removed: Option<Arc<CellTx>>,
    pub accepted: Vec<Arc<CellTx>>,
}

impl TransactionInsertion {
    pub fn new(removed: Option<Arc<CellTx>>, accepted: Vec<Arc<CellTx>>) -> Self {
        Self { removed, accepted }
    }
}
