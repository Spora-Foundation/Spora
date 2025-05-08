use super::error::Result;
use core::fmt::Debug;
use std::{sync::Arc, time::Duration};
use tokio::sync::oneshot;
use tondi_grpc_core::{
    ops::TondidPayloadOps,
    protowire::{TondidRequest, TondidResponse},
};

pub(crate) mod id;
pub(crate) mod matcher;
pub(crate) mod queue;

pub(crate) trait Resolver: Send + Sync + Debug {
    fn register_request(&self, op: TondidPayloadOps, request: &TondidRequest) -> TondidMessageReceiver;
    fn handle_response(&self, response: TondidResponse);
    fn remove_expired_requests(&self, timeout: Duration);
}

pub(crate) type DynResolver = Arc<dyn Resolver>;

pub(crate) type TondidMessageSender = oneshot::Sender<Result<TondidResponse>>;
pub(crate) type TondidMessageReceiver = oneshot::Receiver<Result<TondidResponse>>;
