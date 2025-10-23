use crate::{processor::Processor, IDENT};
use spora_cellindex::api::CellIndexProxy;
use spora_consensus_notify::{
    connection::ConsensusChannelConnection, notification::Notification as ConsensusNotification, notifier::ConsensusNotifier,
};
use spora_core::{
    task::service::{AsyncService, AsyncServiceError, AsyncServiceFuture},
    trace, warn,
};
use spora_index_core::notifier::IndexNotifier;
use spora_notify::{
    connection::ChannelType,
    events::{EventSwitches, EventType},
    listener::ListenerLifespan,
    scope::{CellsChangedScope, PruningPointUtxoSetOverrideScope, UtxosChangedScope},
    subscription::{context::SubscriptionContext, MutationPolicies, UtxosChangedMutationPolicy},
};
use spora_utils::{channel::Channel, triggers::SingleTrigger};
use std::sync::Arc;

const INDEX_SERVICE: &str = IDENT;

pub struct IndexService {
    cellindex: Option<CellIndexProxy>, // Replaced utxoindex with cellindex
    notifier: Arc<IndexNotifier>,
    shutdown: SingleTrigger,
}

impl IndexService {
    pub fn new(
        consensus_notifier: &Arc<ConsensusNotifier>,
        subscription_context: SubscriptionContext,
        cellindex: Option<CellIndexProxy>, // Changed from utxoindex to cellindex
    ) -> Self {
        // TODO(spora): Update to Cells subscription granularity
        let policies = MutationPolicies::new(UtxosChangedMutationPolicy::Wildcard);

        // Prepare consensus-notify objects
        let consensus_notify_channel = Channel::<ConsensusNotification>::default();
        let consensus_notify_listener_id = consensus_notifier.register_new_listener(
            ConsensusChannelConnection::new(INDEX_SERVICE, consensus_notify_channel.sender(), ChannelType::Closable),
            ListenerLifespan::Static(policies),
        );

        // Prepare the index-processor notifier
        // Subscribe to both UtxosChanged (legacy) and CellsChanged (new)
        let events: EventSwitches =
            [EventType::UtxosChanged, EventType::CellsChanged, EventType::PruningPointUtxoSetOverride].as_ref().into();
        let collector = Arc::new(Processor::new(cellindex.clone(), consensus_notify_channel.receiver()));
        let notifier = Arc::new(IndexNotifier::new(INDEX_SERVICE, events, vec![collector], vec![], subscription_context, 1, policies));

        // Subscribe to both legacy UTXO and new Cell notifications
        consensus_notifier
            .try_start_notify(consensus_notify_listener_id, UtxosChangedScope::default().into())
            .expect("the subscription always succeeds");
        consensus_notifier
            .try_start_notify(consensus_notify_listener_id, CellsChangedScope::default().into())
            .expect("the subscription always succeeds");
        consensus_notifier
            .try_start_notify(consensus_notify_listener_id, PruningPointUtxoSetOverrideScope::default().into())
            .expect("the subscription always succeeds");

        Self { cellindex, notifier, shutdown: SingleTrigger::default() }
    }

    pub fn notifier(&self) -> Arc<IndexNotifier> {
        self.notifier.clone()
    }

    pub fn cellindex(&self) -> Option<CellIndexProxy> {
        self.cellindex.clone()
    }
}

impl AsyncService for IndexService {
    fn ident(self: Arc<Self>) -> &'static str {
        INDEX_SERVICE
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        trace!("{} starting", INDEX_SERVICE);

        // Prepare a shutdown signal receiver
        let shutdown_signal = self.shutdown.listener.clone();

        // Launch the service and wait for a shutdown signal
        Box::pin(async move {
            self.notifier.clone().start();

            // Keep the notifier running until a service shutdown signal is received
            shutdown_signal.await;
            match self.notifier.join().await {
                Ok(_) => Ok(()),
                Err(err) => {
                    warn!("Error while stopping {}: {}", INDEX_SERVICE, err);
                    Err(AsyncServiceError::Service(err.to_string()))
                }
            }
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", INDEX_SERVICE);
        self.shutdown.trigger.trigger();
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            trace!("{} stopped", INDEX_SERVICE);
            Ok(())
        })
    }
}
