use super::error::Result;
use core::fmt::Debug;
use std::{sync::Arc, time::Duration};
use tokio::sync::oneshot;
use spora_grpc_core::{
    ops::SporadPayloadOps,
    protowire::{SporadRequest, SporadResponse},
};

pub(crate) mod id;
pub(crate) mod matcher;
pub(crate) mod queue;

pub(crate) trait Resolver: Send + Sync + Debug {
    fn register_request(&self, op: SporadPayloadOps, request: &SporadRequest) -> SporadMessageReceiver;
    fn handle_response(&self, response: SporadResponse);
    fn remove_expired_requests(&self, timeout: Duration);
}

pub(crate) type DynResolver = Arc<dyn Resolver>;

pub(crate) type SporadMessageSender = oneshot::Sender<Result<SporadResponse>>;
pub(crate) type SporadMessageReceiver = oneshot::Receiver<Result<SporadResponse>>;
