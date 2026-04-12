use std::fmt::Debug;
extern crate derive_more;
use spora_core::debug;

use crate::{
    error::Result,
    subscription::{
        context::SubscriptionContext, CellsChangedMutationPolicy, DynSubscription, MutateSingle, MutationOutcome, MutationPolicies,
    },
};

use super::{
    connection::Connection,
    events::EventArray,
    subscription::{array::ArrayBuilder, Mutation},
};

pub type ListenerId = u64;

#[derive(Copy, Clone, Debug)]
pub enum ListenerLifespan {
    Static(MutationPolicies),
    Dynamic,
}

/// A listener of [`super::notifier::Notifier`] notifications.
#[derive(Debug)]
pub(crate) struct Listener<C>
where
    C: Connection,
{
    connection: C,
    pub(crate) subscriptions: EventArray<DynSubscription>,
    pub(crate) _lifespan: ListenerLifespan,
}

impl<C> Listener<C>
where
    C: Connection,
{
    pub fn new(id: ListenerId, connection: C) -> Self {
        Self { connection, subscriptions: ArrayBuilder::single(id, None), _lifespan: ListenerLifespan::Dynamic }
    }

    pub fn new_static(id: ListenerId, connection: C, context: &SubscriptionContext, policies: MutationPolicies) -> Self {
        let capacity = match policies.cells_changed {
            CellsChangedMutationPolicy::AddressSet => {
                debug!(
                    "Creating a static listener {} with CellsChanged capacity of {}",
                    connection,
                    context.address_tracker.addresses_preallocation().unwrap_or_default()
                );
                context.address_tracker.addresses_preallocation()
            }
            CellsChangedMutationPolicy::Wildcard => None,
        };
        let subscriptions = ArrayBuilder::single(id, capacity);
        Self { connection, subscriptions, _lifespan: ListenerLifespan::Static(policies) }
    }

    pub fn connection(&self) -> C {
        self.connection.clone()
    }

    /// Apply a mutation to the subscriptions
    pub fn mutate(
        &mut self,
        mutation: Mutation,
        policies: MutationPolicies,
        context: &SubscriptionContext,
    ) -> Result<MutationOutcome> {
        let event_type = mutation.event_type();
        self.subscriptions[event_type].mutate(mutation, policies, context)
    }

    pub fn close(&self) {
        if !self.is_closed() {
            self.connection.close();
        }
    }

    pub fn is_closed(&self) -> bool {
        self.connection.is_closed()
    }
}
