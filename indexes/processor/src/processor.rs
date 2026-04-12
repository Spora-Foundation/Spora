use crate::{
    errors::{IndexError, IndexResult},
    IDENT,
};
use async_trait::async_trait;
use spora_cellindex::api::CellIndexProxy;
use spora_consensus_notify::{notification as consensus_notification, notification::Notification as ConsensusNotification};
use spora_core::{debug, trace};
use spora_index_core::notification::{CellsChangedNotification, Notification, PruningPointCellSetOverrideNotification};
use spora_notify::{
    collector::{Collector, CollectorNotificationReceiver},
    error::Result,
    notification::Notification as NotificationTrait,
    notifier::DynNotify,
};
use spora_utils::triggers::SingleTrigger;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// Processor processes incoming consensus CellsChanged notifications
/// submitting them to a CellIndex.
///
/// It also acts as a [`Collector`], converting the incoming consensus notifications
/// into their pending local versions and relaying them to a local notifier.
///
#[derive(Debug)]
pub struct Processor {
    /// An optional Cell indexer
    cellindex: Option<CellIndexProxy>,

    recv_channel: CollectorNotificationReceiver<ConsensusNotification>,

    /// Has this collector been started?
    is_started: Arc<AtomicBool>,

    collect_shutdown: Arc<SingleTrigger>,
}

impl Processor {
    pub fn new(cellindex: Option<CellIndexProxy>, recv_channel: CollectorNotificationReceiver<ConsensusNotification>) -> Self {
        Self {
            cellindex,
            recv_channel,
            collect_shutdown: Arc::new(SingleTrigger::new()),
            is_started: Arc::new(AtomicBool::new(false)),
        }
    }

    fn spawn_collecting_task(self: Arc<Self>, notifier: DynNotify<Notification>) {
        // The task can only be spawned once
        if self.is_started.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return;
        }
        tokio::spawn(async move {
            trace!("[Index processor] collecting task starting");

            while let Ok(notification) = self.recv_channel.recv().await {
                match self.process_notification(notification).await {
                    Ok(notification) => match notifier.notify(notification) {
                        Ok(_) => (),
                        Err(err) => {
                            trace!("[Index processor] notification sender error: {err:?}");
                        }
                    },
                    Err(err) => {
                        trace!("[Index processor] error while processing a consensus notification: {err:?}");
                    }
                }
            }

            debug!("[Index processor] notification stream ended");
            self.collect_shutdown.trigger.trigger();
            trace!("[Index processor] collecting task ended");
        });
    }

    async fn process_notification(self: &Arc<Self>, notification: ConsensusNotification) -> IndexResult<Notification> {
        match notification {
            ConsensusNotification::CellsChanged(cells_changed) => {
                Ok(Notification::CellsChanged(self.process_cells_changed(cells_changed).await?))
            }
            ConsensusNotification::PruningPointCellSetOverride(_) => {
                Ok(Notification::PruningPointCellSetOverride(PruningPointCellSetOverrideNotification {}))
            }
            _ => Err(IndexError::NotSupported(notification.event_type())),
        }
    }

    /// Process CellsChanged notification from consensus
    ///
    /// GHOSTDAG-aware: processes accumulated Cell diff and updates CellIndex
    async fn process_cells_changed(
        self: &Arc<Self>,
        notification: consensus_notification::CellsChangedNotification,
    ) -> IndexResult<CellsChangedNotification> {
        trace!(
            "[{IDENT}]: processing CellsChanged notification with {} added, {} removed cells",
            notification.accumulated_cell_diff.num_added(),
            notification.accumulated_cell_diff.num_removed()
        );

        // Update cellindex if present
        if let Some(cellindex) = self.cellindex.clone() {
            if notification.block_cell_diffs.is_empty() {
                cellindex
                    .update_with_diff(notification.accumulated_cell_diff.as_ref())
                    .await
                    .map_err(|e| IndexError::CellIndexError(e))?;
            } else {
                cellindex
                    .update_with_block_diffs(notification.block_cell_diffs.as_ref())
                    .await
                    .map_err(|e| IndexError::CellIndexError(e))?;
            }
        }

        // Convert to index notification format
        let converted = CellsChangedNotification {
            accumulated_cell_diff: notification.accumulated_cell_diff.clone(),
            virtual_parents: notification.virtual_parents.clone(),
            block_cell_diffs: notification.block_cell_diffs.clone(),
        };

        Ok(converted)
    }

    async fn join_collecting_task(&self) -> Result<()> {
        trace!("[Index processor] joining");
        self.collect_shutdown.listener.clone().await;
        debug!("[Index processor] terminated");
        Ok(())
    }
}

#[async_trait]
impl Collector<Notification> for Processor {
    fn start(self: Arc<Self>, notifier: DynNotify<Notification>) {
        self.spawn_collecting_task(notifier);
    }

    async fn join(self: Arc<Self>) -> Result<()> {
        self.join_collecting_task().await
    }
}
