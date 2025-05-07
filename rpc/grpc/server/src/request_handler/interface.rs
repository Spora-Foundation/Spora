use super::method::{DropFn, Method, MethodTrait, RoutingPolicy};
use crate::{
    connection::Connection,
    connection_handler::ServerContext,
    error::{GrpcServerError, GrpcServerResult},
};
use tondi_grpc_core::{
    ops::TondidPayloadOps,
    protowire::{TondidRequest, TondidResponse},
};
use std::fmt::Debug;
use std::{collections::HashMap, sync::Arc};

pub type TondidMethod = Method<ServerContext, Connection, TondidRequest, TondidResponse>;
pub type DynTondidMethod = Arc<dyn MethodTrait<ServerContext, Connection, TondidRequest, TondidResponse>>;
pub type TondidDropFn = DropFn<TondidRequest, TondidResponse>;
pub type TondidRoutingPolicy = RoutingPolicy<TondidRequest, TondidResponse>;

/// An interface providing methods implementations and a fallback "not implemented" method
/// actually returning a message with a "not implemented" error.
///
/// The interface can provide a method clone for every [`TondidPayloadOps`] variant for later
/// processing of related requests.
///
/// It is also possible to directly let the interface itself process a request by invoking
/// the `call()` method.
pub struct Interface {
    server_ctx: ServerContext,
    methods: HashMap<TondidPayloadOps, DynTondidMethod>,
    method_not_implemented: DynTondidMethod,
}

impl Interface {
    pub fn new(server_ctx: ServerContext) -> Self {
        let method_not_implemented = Arc::new(Method::new(|_, _, tondid_request: TondidRequest| {
            Box::pin(async move {
                match tondid_request.payload {
                    Some(ref request) => Ok(TondidResponse {
                        id: tondid_request.id,
                        payload: Some(TondidPayloadOps::from(request).to_error_response(GrpcServerError::MethodNotImplemented.into())),
                    }),
                    None => Err(GrpcServerError::InvalidRequestPayload),
                }
            })
        }));
        Self { server_ctx, methods: Default::default(), method_not_implemented }
    }

    pub fn method(&mut self, op: TondidPayloadOps, method: TondidMethod) {
        let method: DynTondidMethod = Arc::new(method);
        if self.methods.insert(op, method).is_some() {
            panic!("RPC method {op:?} is declared multiple times")
        }
    }

    pub fn replace_method(&mut self, op: TondidPayloadOps, method: TondidMethod) {
        let method: DynTondidMethod = Arc::new(method);
        let _ = self.methods.insert(op, method);
    }

    pub fn set_method_properties(
        &mut self,
        op: TondidPayloadOps,
        tasks: usize,
        queue_size: usize,
        routing_policy: TondidRoutingPolicy,
    ) {
        self.methods.entry(op).and_modify(|x| {
            let method: Method<ServerContext, Connection, TondidRequest, TondidResponse> =
                Method::with_properties(x.method_fn(), tasks, queue_size, routing_policy);
            let method: Arc<dyn MethodTrait<ServerContext, Connection, TondidRequest, TondidResponse>> = Arc::new(method);
            *x = method;
        });
    }

    pub async fn call(
        &self,
        op: &TondidPayloadOps,
        connection: Connection,
        request: TondidRequest,
    ) -> GrpcServerResult<TondidResponse> {
        self.methods.get(op).unwrap_or(&self.method_not_implemented).call(self.server_ctx.clone(), connection, request).await
    }

    pub fn get_method(&self, op: &TondidPayloadOps) -> DynTondidMethod {
        self.methods.get(op).unwrap_or(&self.method_not_implemented).clone()
    }
}

impl Debug for Interface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Interface").finish()
    }
}
