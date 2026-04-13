use super::error::Result;
use core::fmt::Debug;
use spora_grpc_core::{
    ops::RpcPayloadOps,
    protowire::{RpcRequest, RpcResponse},
};
use std::{sync::Arc, time::Duration};
use tokio::sync::oneshot;

pub(crate) mod id;
pub(crate) mod matcher;
pub(crate) mod queue;

pub(crate) trait Resolver: Send + Sync + Debug {
    fn register_request(&self, op: RpcPayloadOps, request: &RpcRequest) -> P2pMessageReceiver;
    fn handle_response(&self, response: RpcResponse);
    fn remove_expired_requests(&self, timeout: Duration);
}

pub(crate) type DynResolver = Arc<dyn Resolver>;

pub(crate) type P2pMessageSender = oneshot::Sender<Result<RpcResponse>>;
pub(crate) type P2pMessageReceiver = oneshot::Receiver<Result<RpcResponse>>;
