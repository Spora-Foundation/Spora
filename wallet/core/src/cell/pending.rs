//!
//! Implements pending tracked-cell references used to monitor maturity progress.
//!

use crate::cell::{CellContext, CellEntryId, CellEntryReference, CellEntryReferenceExtension, Maturity};
use crate::imports::*;

pub struct PendingCellEntryReferenceInner {
    pub entry: CellEntryReference,
    pub cell_context: CellContext,
}

#[derive(Clone)]
pub struct PendingCellEntryReference {
    pub inner: Arc<PendingCellEntryReferenceInner>,
}

impl PendingCellEntryReference {
    pub fn new(entry: CellEntryReference, cell_context: CellContext) -> Self {
        Self { inner: Arc::new(PendingCellEntryReferenceInner { entry, cell_context }) }
    }

    #[inline(always)]
    pub fn inner(&self) -> &PendingCellEntryReferenceInner {
        &self.inner
    }

    #[inline(always)]
    pub fn entry(&self) -> &CellEntryReference {
        &self.inner().entry
    }

    #[inline(always)]
    pub fn cell_context(&self) -> &CellContext {
        &self.inner().cell_context
    }

    #[inline(always)]
    pub fn id(&self) -> CellEntryId {
        self.inner().entry.id()
    }

    #[inline(always)]
    pub fn transaction_id(&self) -> TransactionId {
        self.inner().entry.transaction_id()
    }

    #[inline(always)]
    pub fn maturity(&self, params: &NetworkParams, current_daa_score: u64) -> Maturity {
        self.inner().entry.maturity(params, current_daa_score)
    }
}

impl From<(&Arc<dyn Account>, CellEntryReference)> for PendingCellEntryReference {
    fn from((account, entry): (&Arc<dyn Account>, CellEntryReference)) -> Self {
        Self::new(entry, (*account.cell_context()).clone())
    }
}

impl From<PendingCellEntryReference> for CellEntryReference {
    fn from(pending: PendingCellEntryReference) -> Self {
        pending.inner().entry.clone()
    }
}
