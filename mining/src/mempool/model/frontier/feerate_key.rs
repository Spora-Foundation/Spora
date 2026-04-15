use crate::{block_template::selector::ALPHA, mempool::model::tx::MempoolTransaction};
use spora_consensus_core::tx::CellTx;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct FeerateTransactionKey {
    pub fee: u64,
    pub mass: u64,
    weight: f64,
    pub tx: Arc<CellTx>,
}

impl Eq for FeerateTransactionKey {}

impl PartialEq for FeerateTransactionKey {
    fn eq(&self, other: &Self) -> bool {
        self.tx.id() == other.tx.id()
    }
}

impl FeerateTransactionKey {
    pub fn new(fee: u64, mass: u64, tx: Arc<CellTx>) -> Self {
        // NOTE: any change to the way this weight is calculated (such as scaling by some factor)
        // requires a reversed update to total_weight in `Frontier::build_feerate_estimator`. This
        // is because the math methods in FeeEstimator assume this specific weight function.
        Self { fee, mass, weight: (fee as f64 / mass as f64).powi(ALPHA), tx }
    }

    pub fn feerate(&self) -> f64 {
        self.fee as f64 / self.mass as f64
    }

    pub fn weight(&self) -> f64 {
        self.weight
    }
}

impl std::hash::Hash for FeerateTransactionKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Transaction id is a sufficient identifier for this key
        self.tx.id().hash(state);
    }
}

impl PartialOrd for FeerateTransactionKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FeerateTransactionKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Our first priority is the feerate.
        // The weight function is monotonic in feerate so we prefer using it
        // since it is cached
        match self.weight().total_cmp(&other.weight()) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }

        // If feerates (and thus weights) are equal, prefer the higher fee in absolute value
        match self.fee.cmp(&other.fee) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }

        //
        // At this point we don't compare the mass fields since if both feerate
        // and fee are equal, mass must be equal as well
        //

        // Finally, we compare transaction ids in order to allow multiple transactions with
        // the same fee and mass to exist within the same sorted container
        self.tx.id().cmp(&other.tx.id())
    }
}

impl From<&MempoolTransaction> for FeerateTransactionKey {
    fn from(tx: &MempoolTransaction) -> Self {
        // NOTE: The code below is a mempool simplification reducing the various block mass units to a
        //       single one-dimension value (making it easier to select transactions for block templates).
        // Future mempool improvements are expected to refine this behavior and use the multi-dimension values
        // in order to optimize and increase block space usage.
        let mass = tx.mtx.selection_mass().expect("masses are expected to be calculated");
        let fee = tx.mtx.calculated_fee.expect("fee is expected to be populated");
        Self::new(fee, mass, tx.mtx.tx.clone())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::mempool::tx::Priority;
    use spora_consensus_core::{
        mass::{ContextualMasses, NonContextualMasses},
        tx::{CellInput, MutableTransaction, TransactionOutpoint},
    };
    use spora_hashes::{CellTxId, HasherBase};
    use std::sync::Arc;

    fn generate_unique_tx(i: u64) -> Arc<CellTx> {
        let mut hasher = CellTxId::new();
        let prev = hasher.update(i.to_le_bytes()).clone().finalize();
        let input = CellInput::new(TransactionOutpoint::new(prev.as_bytes(), 0), 0);
        Arc::new(CellTx::new(vec![input], vec![], vec![], vec![], vec![vec![]]).expect("test tx must be a valid CellTx"))
    }

    /// Test helper for generating a feerate key with a unique tx (per u64 id)
    pub(crate) fn build_feerate_key(fee: u64, mass: u64, id: u64) -> FeerateTransactionKey {
        FeerateTransactionKey::new(fee, mass, generate_unique_tx(id))
    }

    #[test]
    fn feerate_key_uses_selection_mass_from_verified_cycles() {
        let tx = generate_unique_tx(7);
        let mut mtx = MutableTransaction::new(tx.clone());
        mtx.calculated_fee = Some(1_000);
        mtx.calculated_non_contextual_masses = Some(NonContextualMasses::new(100, 50));
        mtx.calculated_contextual_masses = Some(ContextualMasses::new(3_000));
        mtx.verified_cycles = Some(1_000_000);
        let expected_mass = mtx.selection_mass().expect("selection mass should be available");

        let mempool_tx = MempoolTransaction::new(mtx, Priority::Low, 0);
        let key = FeerateTransactionKey::from(&mempool_tx);

        assert_eq!(key.mass, expected_mass);
    }
}
