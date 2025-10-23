use crate::protowire::{tondid_request, SporadRequest, SporadResponse};

impl From<tondid_request::Payload> for SporadRequest {
    fn from(item: tondid_request::Payload) -> Self {
        SporadRequest { id: 0, payload: Some(item) }
    }
}

impl AsRef<SporadRequest> for SporadRequest {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl AsRef<SporadResponse> for SporadResponse {
    fn as_ref(&self) -> &Self {
        self
    }
}

pub mod tondid_request_convert {
    use crate::protowire::*;
    use spora_rpc_core::{RpcError, RpcResult};

    impl_into_tondid_request!(Shutdown);
    impl_into_tondid_request!(SubmitBlock);
    impl_into_tondid_request!(GetBlockTemplate);
    impl_into_tondid_request!(GetBlock);
    impl_into_tondid_request!(GetInfo);

    impl_into_tondid_request!(GetCurrentNetwork);
    impl_into_tondid_request!(GetPeerAddresses);
    impl_into_tondid_request!(GetSink);
    impl_into_tondid_request!(GetMempoolEntry);
    impl_into_tondid_request!(GetMempoolEntries);
    impl_into_tondid_request!(GetConnectedPeerInfo);
    impl_into_tondid_request!(AddPeer);
    impl_into_tondid_request!(SubmitTransaction);
    impl_into_tondid_request!(SubmitTransactionReplacement);
    impl_into_tondid_request!(GetSubnetwork);
    impl_into_tondid_request!(GetVirtualChainFromBlock);
    impl_into_tondid_request!(GetBlocks);
    impl_into_tondid_request!(GetBlockCount);
    impl_into_tondid_request!(GetBlockDagInfo);
    impl_into_tondid_request!(ResolveFinalityConflict);
    impl_into_tondid_request!(GetHeaders);
    impl_into_tondid_request!(GetUtxosByAddresses);
    impl_into_tondid_request!(GetBalanceByAddress);
    impl_into_tondid_request!(GetBalancesByAddresses);
    impl_into_tondid_request!(GetSinkBlueScore);
    impl_into_tondid_request!(Ban);
    impl_into_tondid_request!(Unban);
    impl_into_tondid_request!(EstimateNetworkHashesPerSecond);
    impl_into_tondid_request!(GetMempoolEntriesByAddresses);
    impl_into_tondid_request!(GetCoinSupply);
    impl_into_tondid_request!(Ping);
    impl_into_tondid_request!(GetMetrics);
    impl_into_tondid_request!(GetConnections);
    impl_into_tondid_request!(GetSystemInfo);
    impl_into_tondid_request!(GetServerInfo);
    impl_into_tondid_request!(GetSyncStatus);
    impl_into_tondid_request!(GetDaaScoreTimestampEstimate);
    impl_into_tondid_request!(GetFeeEstimate);
    impl_into_tondid_request!(GetFeeEstimateExperimental);
    impl_into_tondid_request!(GetCurrentBlockColor);
    impl_into_tondid_request!(GetUtxoReturnAddress);

    impl_into_tondid_request!(NotifyBlockAdded);
    impl_into_tondid_request!(NotifyNewBlockTemplate);
    impl_into_tondid_request!(NotifyUtxosChanged);
    impl_into_tondid_request!(NotifyPruningPointUtxoSetOverride);
    impl_into_tondid_request!(NotifyFinalityConflict);
    impl_into_tondid_request!(NotifyVirtualDaaScoreChanged);
    impl_into_tondid_request!(NotifyVirtualChainChanged);
    impl_into_tondid_request!(NotifySinkBlueScoreChanged);

    macro_rules! impl_into_tondid_request {
        ($name:tt) => {
            paste::paste! {
                impl_into_tondid_request_ex!(spora_rpc_core::[<$name Request>],[<$name RequestMessage>],[<$name Request>]);
            }
        };
    }

    use impl_into_tondid_request;

    macro_rules! impl_into_tondid_request_ex {
        // ($($core_struct:ident)::+, $($protowire_struct:ident)::+, $($variant:ident)::+) => {
        ($core_struct:path, $protowire_struct:ident, $variant:ident) => {
            // ----------------------------------------------------------------------------
            // rpc_core to protowire
            // ----------------------------------------------------------------------------

            impl From<&$core_struct> for tondid_request::Payload {
                fn from(item: &$core_struct) -> Self {
                    Self::$variant(item.into())
                }
            }

            impl From<&$core_struct> for SporadRequest {
                fn from(item: &$core_struct) -> Self {
                    Self { id: 0, payload: Some(item.into()) }
                }
            }

            impl From<$core_struct> for tondid_request::Payload {
                fn from(item: $core_struct) -> Self {
                    Self::$variant((&item).into())
                }
            }

            impl From<$core_struct> for SporadRequest {
                fn from(item: $core_struct) -> Self {
                    Self { id: 0, payload: Some((&item).into()) }
                }
            }

            // ----------------------------------------------------------------------------
            // protowire to rpc_core
            // ----------------------------------------------------------------------------

            impl TryFrom<&tondid_request::Payload> for $core_struct {
                type Error = RpcError;
                fn try_from(item: &tondid_request::Payload) -> RpcResult<Self> {
                    if let tondid_request::Payload::$variant(request) = item {
                        request.try_into()
                    } else {
                        Err(RpcError::MissingRpcFieldError("Payload".to_string(), stringify!($variant).to_string()))
                    }
                }
            }

            impl TryFrom<&SporadRequest> for $core_struct {
                type Error = RpcError;
                fn try_from(item: &SporadRequest) -> RpcResult<Self> {
                    item.payload
                        .as_ref()
                        .ok_or(RpcError::MissingRpcFieldError("SporaRequest".to_string(), "Payload".to_string()))?
                        .try_into()
                }
            }

            impl From<$protowire_struct> for SporadRequest {
                fn from(item: $protowire_struct) -> Self {
                    Self { id: 0, payload: Some(tondid_request::Payload::$variant(item)) }
                }
            }

            impl From<$protowire_struct> for tondid_request::Payload {
                fn from(item: $protowire_struct) -> Self {
                    tondid_request::Payload::$variant(item)
                }
            }
        };
    }
    use impl_into_tondid_request_ex;
}

pub mod tondid_response_convert {
    use crate::protowire::*;
    use spora_rpc_core::{RpcError, RpcResult};

    impl_into_tondid_response!(Shutdown);
    impl_into_tondid_response!(SubmitBlock);
    impl_into_tondid_response!(GetBlockTemplate);
    impl_into_tondid_response!(GetBlock);
    impl_into_tondid_response!(GetInfo);
    impl_into_tondid_response!(GetCurrentNetwork);

    impl_into_tondid_response!(GetPeerAddresses);
    impl_into_tondid_response!(GetSink);
    impl_into_tondid_response!(GetMempoolEntry);
    impl_into_tondid_response!(GetMempoolEntries);
    impl_into_tondid_response!(GetConnectedPeerInfo);
    impl_into_tondid_response!(AddPeer);
    impl_into_tondid_response!(SubmitTransaction);
    impl_into_tondid_response!(SubmitTransactionReplacement);
    impl_into_tondid_response!(GetSubnetwork);
    impl_into_tondid_response!(GetVirtualChainFromBlock);
    impl_into_tondid_response!(GetBlocks);
    impl_into_tondid_response!(GetBlockCount);
    impl_into_tondid_response!(GetBlockDagInfo);
    impl_into_tondid_response!(ResolveFinalityConflict);
    impl_into_tondid_response!(GetHeaders);
    impl_into_tondid_response!(GetUtxosByAddresses);
    impl_into_tondid_response!(GetBalanceByAddress);
    impl_into_tondid_response!(GetBalancesByAddresses);
    impl_into_tondid_response!(GetSinkBlueScore);
    impl_into_tondid_response!(Ban);
    impl_into_tondid_response!(Unban);
    impl_into_tondid_response!(EstimateNetworkHashesPerSecond);
    impl_into_tondid_response!(GetMempoolEntriesByAddresses);
    impl_into_tondid_response!(GetCoinSupply);
    impl_into_tondid_response!(Ping);
    impl_into_tondid_response!(GetMetrics);
    impl_into_tondid_response!(GetConnections);
    impl_into_tondid_response!(GetSystemInfo);
    impl_into_tondid_response!(GetServerInfo);
    impl_into_tondid_response!(GetSyncStatus);
    impl_into_tondid_response!(GetDaaScoreTimestampEstimate);
    impl_into_tondid_response!(GetFeeEstimate);
    impl_into_tondid_response!(GetFeeEstimateExperimental);
    impl_into_tondid_response!(GetCurrentBlockColor);
    impl_into_tondid_response!(GetUtxoReturnAddress);

    impl_into_tondid_notify_response!(NotifyBlockAdded);
    impl_into_tondid_notify_response!(NotifyNewBlockTemplate);
    impl_into_tondid_notify_response!(NotifyUtxosChanged);
    impl_into_tondid_notify_response!(NotifyPruningPointUtxoSetOverride);
    impl_into_tondid_notify_response!(NotifyFinalityConflict);
    impl_into_tondid_notify_response!(NotifyVirtualDaaScoreChanged);
    impl_into_tondid_notify_response!(NotifyVirtualChainChanged);
    impl_into_tondid_notify_response!(NotifySinkBlueScoreChanged);

    impl_into_tondid_notify_response!(NotifyUtxosChanged, StopNotifyingUtxosChanged);
    impl_into_tondid_notify_response!(NotifyPruningPointUtxoSetOverride, StopNotifyingPruningPointUtxoSetOverride);

    macro_rules! impl_into_tondid_response {
        ($name:tt) => {
            paste::paste! {
                impl_into_tondid_response_ex!(spora_rpc_core::[<$name Response>],[<$name ResponseMessage>],[<$name Response>]);
            }
        };
        ($core_name:tt, $protowire_name:tt) => {
            paste::paste! {
                impl_into_tondid_response_base!(spora_rpc_core::[<$core_name Response>],[<$protowire_name ResponseMessage>],[<$protowire_name Response>]);
            }
        };
    }
    use impl_into_tondid_response;

    macro_rules! impl_into_tondid_response_base {
        ($core_struct:path, $protowire_struct:ident, $variant:ident) => {
            // ----------------------------------------------------------------------------
            // rpc_core to protowire
            // ----------------------------------------------------------------------------

            impl From<RpcResult<$core_struct>> for $protowire_struct {
                fn from(item: RpcResult<$core_struct>) -> Self {
                    item.as_ref().map_err(|x| (*x).clone()).into()
                }
            }

            impl From<RpcError> for $protowire_struct {
                fn from(item: RpcError) -> Self {
                    let x: RpcResult<&$core_struct> = Err(item);
                    x.into()
                }
            }

            impl From<$protowire_struct> for tondid_response::Payload {
                fn from(item: $protowire_struct) -> Self {
                    tondid_response::Payload::$variant(item)
                }
            }

            impl From<$protowire_struct> for SporadResponse {
                fn from(item: $protowire_struct) -> Self {
                    Self { id: 0, payload: Some(tondid_response::Payload::$variant(item)) }
                }
            }
        };
    }
    use impl_into_tondid_response_base;

    macro_rules! impl_into_tondid_response_ex {
        ($core_struct:path, $protowire_struct:ident, $variant:ident) => {
            // ----------------------------------------------------------------------------
            // rpc_core to protowire
            // ----------------------------------------------------------------------------

            impl From<RpcResult<&$core_struct>> for tondid_response::Payload {
                fn from(item: RpcResult<&$core_struct>) -> Self {
                    tondid_response::Payload::$variant(item.into())
                }
            }

            impl From<RpcResult<&$core_struct>> for SporadResponse {
                fn from(item: RpcResult<&$core_struct>) -> Self {
                    Self { id: 0, payload: Some(item.into()) }
                }
            }

            impl From<RpcResult<$core_struct>> for tondid_response::Payload {
                fn from(item: RpcResult<$core_struct>) -> Self {
                    tondid_response::Payload::$variant(item.into())
                }
            }

            impl From<RpcResult<$core_struct>> for SporadResponse {
                fn from(item: RpcResult<$core_struct>) -> Self {
                    Self { id: 0, payload: Some(item.into()) }
                }
            }

            impl_into_tondid_response_base!($core_struct, $protowire_struct, $variant);

            // ----------------------------------------------------------------------------
            // protowire to rpc_core
            // ----------------------------------------------------------------------------

            impl TryFrom<&tondid_response::Payload> for $core_struct {
                type Error = RpcError;
                fn try_from(item: &tondid_response::Payload) -> RpcResult<Self> {
                    if let tondid_response::Payload::$variant(response) = item {
                        response.try_into()
                    } else {
                        Err(RpcError::MissingRpcFieldError("Payload".to_string(), stringify!($variant).to_string()))
                    }
                }
            }

            impl TryFrom<&SporadResponse> for $core_struct {
                type Error = RpcError;
                fn try_from(item: &SporadResponse) -> RpcResult<Self> {
                    item.payload
                        .as_ref()
                        .ok_or(RpcError::MissingRpcFieldError("SporaResponse".to_string(), "Payload".to_string()))?
                        .try_into()
                }
            }
        };
    }
    use impl_into_tondid_response_ex;

    macro_rules! impl_into_tondid_notify_response {
        ($name:tt) => {
            impl_into_tondid_response!($name);

            paste::paste! {
                impl_into_tondid_notify_response_ex!(spora_rpc_core::[<$name Response>],[<$name ResponseMessage>]);
            }
        };
        ($core_name:tt, $protowire_name:tt) => {
            impl_into_tondid_response!($core_name, $protowire_name);

            paste::paste! {
                impl_into_tondid_notify_response_ex!(spora_rpc_core::[<$core_name Response>],[<$protowire_name ResponseMessage>]);
            }
        };
    }
    use impl_into_tondid_notify_response;

    macro_rules! impl_into_tondid_notify_response_ex {
        ($($core_struct:ident)::+, $protowire_struct:ident) => {
            // ----------------------------------------------------------------------------
            // rpc_core to protowire
            // ----------------------------------------------------------------------------

            impl<T> From<Result<(), T>> for $protowire_struct
            where
                T: Into<RpcError>,
            {
                fn from(item: Result<(), T>) -> Self {
                    item
                        .map(|_| $($core_struct)::+{})
                        .map_err(|err| err.into()).into()
                }
            }

        };
    }
    use impl_into_tondid_notify_response_ex;
}
