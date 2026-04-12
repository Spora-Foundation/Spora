use derive_more::Display;
use spora_consensus_core::cell_diff::{BlockCellDiff, CellDiff};
use spora_hashes::Hash;
use spora_notify::{
    events::EventType,
    full_featured,
    notification::Notification as NotificationTrait,
    subscription::{
        context::SubscriptionContext,
        single::{CellsChangedSubscription, OverallSubscription, VirtualChainChangedSubscription},
        Subscription,
    },
};
use std::sync::Arc;

full_featured! {
#[derive(Clone, Debug, Display)]
pub enum Notification {
    #[display(fmt = "CellsChanged notification")]
    CellsChanged(CellsChangedNotification),

    #[display(fmt = "PruningPointCellSetOverride notification")]
    PruningPointCellSetOverride(PruningPointCellSetOverrideNotification),
}
}

impl NotificationTrait for Notification {
    fn apply_overall_subscription(&self, subscription: &OverallSubscription, _context: &SubscriptionContext) -> Option<Self> {
        match subscription.active() {
            true => Some(self.clone()),
            false => None,
        }
    }

    fn apply_virtual_chain_changed_subscription(
        &self,
        _subscription: &VirtualChainChangedSubscription,
        _context: &SubscriptionContext,
    ) -> Option<Self> {
        Some(self.clone())
    }

    fn apply_cells_changed_subscription(
        &self,
        _subscription: &CellsChangedSubscription,
        _context: &SubscriptionContext,
    ) -> Option<Self> {
        None
    }

    fn event_type(&self) -> EventType {
        self.into()
    }
}

#[derive(Debug, Clone, Default)]
pub struct PruningPointCellSetOverrideNotification {}

/// CellsChanged notification for index layer
///
/// This is the index-layer version of consensus CellsChangedNotification.
/// It contains the accumulated Cell diff from virtual state updates.
#[derive(Debug, Clone)]
pub struct CellsChangedNotification {
    /// Accumulated Cell diff
    pub accumulated_cell_diff: Arc<CellDiff>,
    /// Virtual parents
    pub virtual_parents: Arc<Vec<Hash>>,
    /// Block-level provenance for the accumulated diff.
    pub block_cell_diffs: Arc<Vec<BlockCellDiff>>,
}

impl CellsChangedNotification {
    pub fn new(
        accumulated_cell_diff: Arc<CellDiff>,
        virtual_parents: Arc<Vec<Hash>>,
        block_cell_diffs: Arc<Vec<BlockCellDiff>>,
    ) -> Self {
        Self { accumulated_cell_diff, virtual_parents, block_cell_diffs }
    }
}
