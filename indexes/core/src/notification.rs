use derive_more::Display;
use spora_addresses::Prefix;
use spora_consensus_core::{
    cell_diff::{BlockCellDiff, CellCollection, CellDiff},
    tx::pay_to_address_lock_script,
};
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
use std::{collections::HashSet, sync::Arc};

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
        subscription: &CellsChangedSubscription,
        context: &SubscriptionContext,
    ) -> Option<Self> {
        let Notification::CellsChanged(payload) = self else {
            return None;
        };

        if !subscription.active() {
            return None;
        }

        if subscription.to_all() {
            return Some(self.clone());
        }

        let subscribed_lock_hashes = subscription
            .data()
            .to_addresses(Prefix::Mainnet, context)
            .into_iter()
            .map(|address| pay_to_address_lock_script(&address).hash())
            .collect::<HashSet<_>>();
        if subscribed_lock_hashes.is_empty() {
            return None;
        }

        let filter_cells = |cells: &CellCollection| -> CellCollection {
            cells
                .iter()
                .filter(|(_, meta)| subscribed_lock_hashes.contains(&meta.lock_hash))
                .map(|(outpoint, meta)| (outpoint.clone(), meta.clone()))
                .collect()
        };
        let filter_diff = |diff: &CellDiff| CellDiff { add: filter_cells(&diff.add), remove: filter_cells(&diff.remove) };

        let accumulated_cell_diff = filter_diff(&payload.accumulated_cell_diff);
        if accumulated_cell_diff.is_empty() {
            return None;
        }

        let block_cell_diffs = payload
            .block_cell_diffs
            .iter()
            .filter_map(|block_diff| {
                let cell_diff = filter_diff(&block_diff.cell_diff);
                (!cell_diff.is_empty()).then(|| BlockCellDiff::new(block_diff.block_hash, block_diff.block_daa_score, cell_diff))
            })
            .collect();

        Some(Notification::CellsChanged(CellsChangedNotification {
            accumulated_cell_diff: Arc::new(accumulated_cell_diff),
            virtual_parents: payload.virtual_parents.clone(),
            block_cell_diffs: Arc::new(block_cell_diffs),
        }))
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
