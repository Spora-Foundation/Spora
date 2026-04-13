use super::method::{DropFn, Method, MethodTrait, RoutingPolicy};
use crate::{
    connection::Connection,
    connection_handler::ServerContext,
    error::{GrpcServerError, GrpcServerResult},
};
use spora_grpc_core::{
    ops::RpcPayloadOps,
    protowire::{RpcRequest, RpcResponse},
};
use std::fmt::Debug;
use std::{collections::HashMap, sync::Arc};

pub type RpcMethod = Method<ServerContext, Connection, RpcRequest, RpcResponse>;
pub type DynRpcMethod = Arc<dyn MethodTrait<ServerContext, Connection, RpcRequest, RpcResponse>>;
pub type RpcDropFn = DropFn<RpcRequest, RpcResponse>;
pub type RpcRoutingPolicy = RoutingPolicy<RpcRequest, RpcResponse>;

/// An interface providing methods implementations and a fallback "not implemented" method
/// actually returning a message with a "not implemented" error.
///
/// The interface can provide a method clone for every [`RpcPayloadOps`] variant for later
/// processing of related requests.
///
/// It is also possible to directly let the interface itself process a request by invoking
/// the `call()` method.
pub struct Interface {
    server_ctx: ServerContext,
    methods: HashMap<RpcPayloadOps, DynRpcMethod>,
    method_not_implemented: DynRpcMethod,
}

impl Interface {
    pub fn new(server_ctx: ServerContext) -> Self {
        let method_not_implemented = Arc::new(Method::new(|_, _, rpc_request: RpcRequest| {
            Box::pin(async move {
                match rpc_request.payload {
                    Some(ref request) => Ok(RpcResponse {
                        id: rpc_request.id,
                        payload: Some(RpcPayloadOps::from(request).to_error_response(GrpcServerError::MethodNotImplemented.into())),
                    }),
                    None => Err(GrpcServerError::InvalidRequestPayload),
                }
            })
        }));
        Self { server_ctx, methods: Default::default(), method_not_implemented }
    }

    pub fn method(&mut self, op: RpcPayloadOps, method: RpcMethod) {
        let method: DynRpcMethod = Arc::new(method);
        if self.methods.insert(op, method).is_some() {
            panic!("RPC method {op:?} is declared multiple times")
        }
    }

    pub fn replace_method(&mut self, op: RpcPayloadOps, method: RpcMethod) {
        let method: DynRpcMethod = Arc::new(method);
        let _ = self.methods.insert(op, method);
    }

    pub fn set_method_properties(&mut self, op: RpcPayloadOps, tasks: usize, queue_size: usize, routing_policy: RpcRoutingPolicy) {
        self.methods.entry(op).and_modify(|x| {
            let method: Method<ServerContext, Connection, RpcRequest, RpcResponse> =
                Method::with_properties(x.method_fn(), tasks, queue_size, routing_policy);
            let method: Arc<dyn MethodTrait<ServerContext, Connection, RpcRequest, RpcResponse>> = Arc::new(method);
            *x = method;
        });
    }

    pub async fn call(&self, op: &RpcPayloadOps, connection: Connection, request: RpcRequest) -> GrpcServerResult<RpcResponse> {
        self.methods.get(op).unwrap_or(&self.method_not_implemented).call(self.server_ctx.clone(), connection, request).await
    }

    pub fn get_method(&self, op: &RpcPayloadOps) -> DynRpcMethod {
        self.methods.get(op).unwrap_or(&self.method_not_implemented).clone()
    }
}

impl Debug for Interface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Interface").finish()
    }
}
