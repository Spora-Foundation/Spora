use crate::mempool::tx::{Priority, RbfPolicy};
use spora_consensus_core::tx::{CellTx, MutableTransaction, TransactionId, TransactionOutpoint};
use spora_mining_errors::mempool::RuleError;
use std::{
    fmt::{Display, Formatter},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CellMirrorKind {
    DerivedView,
    CanonicalProvided,
}

#[derive(Clone)]
pub(crate) struct MempoolTransaction {
    pub(crate) mtx: MutableTransaction,
    pub(crate) cell_tx: Option<Arc<CellTx>>,
    pub(crate) cell_tx_id: Option<TransactionId>,
    pub(crate) cell_wtxid: Option<[u8; 32]>,
    pub(crate) cell_mirror_kind: CellMirrorKind,
    pub(crate) priority: Priority,
    pub(crate) added_at_daa_score: u64,
}

impl MempoolTransaction {
    pub(crate) fn new(mtx: MutableTransaction, priority: Priority, added_at_daa_score: u64) -> Self {
        assert_eq!(mtx.tx.inputs.len(), mtx.entries.len());
        // mtx.tx is already Arc<CellTx> after MutableTransaction migration
        let cell_tx = Some(mtx.tx.clone());
        Self::new_with_cell_mirror(mtx, cell_tx, CellMirrorKind::DerivedView, priority, added_at_daa_score)
    }

    pub(crate) fn new_with_cell_tx(
        mtx: MutableTransaction,
        cell_tx: Arc<CellTx>,
        priority: Priority,
        added_at_daa_score: u64,
    ) -> Self {
        Self::new_with_cell_mirror(mtx, Some(cell_tx), CellMirrorKind::CanonicalProvided, priority, added_at_daa_score)
    }

    fn new_with_cell_mirror(
        mtx: MutableTransaction,
        cell_tx: Option<Arc<CellTx>>,
        cell_mirror_kind: CellMirrorKind,
        priority: Priority,
        added_at_daa_score: u64,
    ) -> Self {
        assert_eq!(mtx.tx.inputs.len(), mtx.entries.len());
        let cell_tx_id = cell_tx.as_ref().map(|tx| TransactionId::from_bytes(tx.id()));
        Self { mtx, cell_tx, cell_tx_id, cell_wtxid: None, cell_mirror_kind, priority, added_at_daa_score }
    }

    pub(crate) fn id(&self) -> TransactionId {
        TransactionId::from_bytes(self.mtx.tx.id())
    }

    pub(crate) fn cell_tx(&self) -> Option<Arc<CellTx>> {
        self.cell_tx.clone()
    }

    pub(crate) fn cell_tx_id(&self) -> Option<TransactionId> {
        self.cell_tx_id
    }

    pub(crate) fn cell_wtxid(&self) -> Option<[u8; 32]> {
        self.cell_wtxid
    }

    pub(crate) fn has_canonical_cell_tx(&self) -> bool {
        matches!(self.cell_mirror_kind, CellMirrorKind::CanonicalProvided) && self.cell_tx.is_some()
    }

    pub(crate) fn refresh_cell_mirror_with_context(
        &mut self,
        _parent_cell_ids: &std::collections::HashMap<TransactionId, TransactionId>,
    ) {
        // mtx.tx is already a CellTx; no conversion is needed.
        // For DerivedView, the cell_tx is just a clone of mtx.tx.
        if matches!(self.cell_mirror_kind, CellMirrorKind::DerivedView) {
            self.cell_tx = Some(self.mtx.tx.clone());
        }
        self.cell_tx_id = self.cell_tx.as_ref().map(|tx| TransactionId::from_bytes(tx.id()));
        self.cell_wtxid = None;
    }

    pub(crate) fn feerate(&self) -> f64 {
        self.mtx.calculated_feerate().unwrap()
    }
}

impl AsRef<CellTx> for MempoolTransaction {
    fn as_ref(&self) -> &CellTx {
        self.mtx.tx.as_ref()
    }
}

impl RbfPolicy {
    #[cfg(test)]
    /// Returns an alternate policy accepting a transaction insertion in case the policy requires a replacement
    pub(crate) fn for_insert(&self) -> RbfPolicy {
        match self {
            RbfPolicy::Forbidden | RbfPolicy::Allowed => *self,
            RbfPolicy::Mandatory => RbfPolicy::Allowed,
        }
    }
}

pub(crate) struct DoubleSpend {
    pub outpoint: TransactionOutpoint,
    pub owner_id: TransactionId,
}

impl DoubleSpend {
    pub fn new(outpoint: TransactionOutpoint, owner_id: TransactionId) -> Self {
        Self { outpoint, owner_id }
    }
}

impl From<DoubleSpend> for RuleError {
    fn from(value: DoubleSpend) -> Self {
        RuleError::RejectDoubleSpendInMempool(value.outpoint, value.owner_id)
    }
}

impl From<&DoubleSpend> for RuleError {
    fn from(value: &DoubleSpend) -> Self {
        RuleError::RejectDoubleSpendInMempool(value.outpoint, value.owner_id)
    }
}

pub(crate) struct TransactionPreValidation {
    pub transaction: MutableTransaction,
    pub cell_tx: Option<Arc<CellTx>>,
    pub feerate_threshold: Option<f64>,
}

#[derive(Default)]
pub(crate) struct TransactionPostValidation {
    pub removed: Option<Arc<CellTx>>,
    pub accepted: Option<Arc<CellTx>>,
    #[allow(dead_code)]
    pub accepted_cell_tx: Option<Arc<CellTx>>,
}

#[derive(PartialEq, Eq)]
pub(crate) enum TxRemovalReason {
    Muted,
    Accepted,
    MakingRoom,
    Unorphaned,
    Expired,
    DoubleSpend,
    InvalidInBlockTemplate,
    RevalidationWithMissingOutpoints,
    ReplacedByFee,
}

impl TxRemovalReason {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            TxRemovalReason::Muted => "",
            TxRemovalReason::Accepted => "accepted",
            TxRemovalReason::MakingRoom => "making room",
            TxRemovalReason::Unorphaned => "unorphaned",
            TxRemovalReason::Expired => "expired",
            TxRemovalReason::DoubleSpend => "double spend",
            TxRemovalReason::InvalidInBlockTemplate => "invalid in block template",
            TxRemovalReason::RevalidationWithMissingOutpoints => "revalidation with missing outpoints",
            TxRemovalReason::ReplacedByFee => "replaced by fee",
        }
    }

    pub(crate) fn verbose(&self) -> bool {
        !matches!(self, TxRemovalReason::Muted)
    }
}

impl Display for TxRemovalReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
