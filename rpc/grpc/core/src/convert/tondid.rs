use crate::protowire::{sporad_request, SporadRequest, SporadResponse};

impl From<sporad_request::Payload> for SporadRequest {
    fn from(item: sporad_request::Payload) -> Self {
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

pub mod sporad_request_convert {
    use crate::protowire::*;
    use spora_rpc_core::{RpcError, RpcResult};

    impl_into_sporad_request!(Shutdown);
    impl_into_sporad_request!(SubmitBlock);
    impl_into_sporad_request!(GetBlockTemplate);
    impl_into_sporad_request!(GetBlock);
    impl_into_sporad_request!(GetBlockStatus);
    impl_into_sporad_request!(GetTransaction);
    impl_into_sporad_request!(GetInfo);

    impl_into_sporad_request!(GetCurrentNetwork);
    impl_into_sporad_request!(GetPeerAddresses);
    impl_into_sporad_request!(GetSink);
    impl_into_sporad_request!(GetMempoolEntry);
    impl_into_sporad_request!(GetMempoolEntries);
    impl_into_sporad_request!(GetConnectedPeerInfo);
    impl_into_sporad_request!(AddPeer);
    impl_into_sporad_request!(SubmitTransaction);
    impl_into_sporad_request!(SubmitTransactionReplacement);
    impl_into_sporad_request!(GetSubnetwork);
    impl_into_sporad_request!(GetVirtualChainFromBlock);
    impl_into_sporad_request!(GetBlocks);
    impl_into_sporad_request!(GetBlockCount);
    impl_into_sporad_request!(GetBlockDagInfo);
    impl_into_sporad_request!(ResolveFinalityConflict);
    impl_into_sporad_request!(GetHeader);
    impl_into_sporad_request!(GetHeaders);
    impl_into_sporad_request!(GetCellsByAddress);
    impl_into_sporad_request!(GetCellsByAddresses);
    impl_into_sporad_request!(GetBalanceByAddress);
    impl_into_sporad_request!(GetBalancesByAddresses);
    impl_into_sporad_request!(GetSinkBlueScore);
    impl_into_sporad_request!(Ban);
    impl_into_sporad_request!(Unban);
    impl_into_sporad_request!(EstimateNetworkHashesPerSecond);
    impl_into_sporad_request!(GetMempoolEntriesByAddresses);
    impl_into_sporad_request!(GetCoinSupply);
    impl_into_sporad_request!(Ping);
    impl_into_sporad_request!(GetMetrics);
    impl_into_sporad_request!(GetConnections);
    impl_into_sporad_request!(GetSystemInfo);
    impl_into_sporad_request!(GetServerInfo);
    impl_into_sporad_request!(GetSyncStatus);
    impl_into_sporad_request!(GetDaaScoreTimestampEstimate);
    impl_into_sporad_request!(GetFeeEstimate);
    impl_into_sporad_request!(GetFeeEstimateExperimental);
    impl_into_sporad_request!(GetCurrentBlockColor);
    impl_into_sporad_request!(GetCellReturnAddress);

    impl_into_sporad_request!(NotifyBlockAdded);
    impl_into_sporad_request!(NotifyNewBlockTemplate);
    impl_into_sporad_request_ex!(
        spora_rpc_core::NotifyCellsChangedRequest,
        NotifyCellsChangedRequestMessage,
        NotifyCellsChangedRequest
    );
    impl_into_sporad_request!(NotifyPruningPointCellSetOverride);
    impl_into_sporad_request!(NotifyFinalityConflict);
    impl_into_sporad_request!(NotifyVirtualDaaScoreChanged);
    impl_into_sporad_request!(NotifyVirtualChainChanged);
    impl_into_sporad_request!(NotifySinkBlueScoreChanged);

    macro_rules! impl_into_sporad_request {
        ($name:tt) => {
            paste::paste! {
                impl_into_sporad_request_ex!(spora_rpc_core::[<$name Request>],[<$name RequestMessage>],[<$name Request>]);
            }
        };
    }

    use impl_into_sporad_request;

    macro_rules! impl_into_sporad_request_ex {
        // ($($core_struct:ident)::+, $($protowire_struct:ident)::+, $($variant:ident)::+) => {
        ($core_struct:path, $protowire_struct:ident, $variant:ident) => {
            // ----------------------------------------------------------------------------
            // rpc_core to protowire
            // ----------------------------------------------------------------------------

            impl From<&$core_struct> for sporad_request::Payload {
                fn from(item: &$core_struct) -> Self {
                    Self::$variant(item.into())
                }
            }

            impl From<&$core_struct> for SporadRequest {
                fn from(item: &$core_struct) -> Self {
                    Self { id: 0, payload: Some(item.into()) }
                }
            }

            impl From<$core_struct> for sporad_request::Payload {
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

            impl TryFrom<&sporad_request::Payload> for $core_struct {
                type Error = RpcError;
                fn try_from(item: &sporad_request::Payload) -> RpcResult<Self> {
                    if let sporad_request::Payload::$variant(request) = item {
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
                    Self { id: 0, payload: Some(sporad_request::Payload::$variant(item)) }
                }
            }

            impl From<$protowire_struct> for sporad_request::Payload {
                fn from(item: $protowire_struct) -> Self {
                    sporad_request::Payload::$variant(item)
                }
            }
        };
    }
    use impl_into_sporad_request_ex;
}

pub mod sporad_response_convert {
    use crate::protowire::*;
    use spora_rpc_core::{RpcError, RpcResult};

    impl_into_sporad_response!(Shutdown);
    impl_into_sporad_response!(SubmitBlock);
    impl_into_sporad_response!(GetBlockTemplate);
    impl_into_sporad_response!(GetBlock);
    impl_into_sporad_response!(GetBlockStatus);
    impl_into_sporad_response!(GetTransaction);
    impl_into_sporad_response!(GetInfo);
    impl_into_sporad_response!(GetCurrentNetwork);

    impl_into_sporad_response!(GetPeerAddresses);
    impl_into_sporad_response!(GetSink);
    impl_into_sporad_response!(GetMempoolEntry);
    impl_into_sporad_response!(GetMempoolEntries);
    impl_into_sporad_response!(GetConnectedPeerInfo);
    impl_into_sporad_response!(AddPeer);
    impl_into_sporad_response!(SubmitTransaction);
    impl_into_sporad_response!(SubmitTransactionReplacement);
    impl_into_sporad_response!(GetSubnetwork);
    impl_into_sporad_response!(GetVirtualChainFromBlock);
    impl_into_sporad_response!(GetBlocks);
    impl_into_sporad_response!(GetBlockCount);
    impl_into_sporad_response!(GetBlockDagInfo);
    impl_into_sporad_response!(ResolveFinalityConflict);
    impl_into_sporad_response!(GetHeader);
    impl_into_sporad_response!(GetHeaders);
    impl_into_sporad_response!(GetCellsByAddress);
    impl_into_sporad_response!(GetCellsByAddresses);
    impl_into_sporad_response!(GetBalanceByAddress);
    impl_into_sporad_response!(GetBalancesByAddresses);
    impl_into_sporad_response!(GetSinkBlueScore);
    impl_into_sporad_response!(Ban);
    impl_into_sporad_response!(Unban);
    impl_into_sporad_response!(EstimateNetworkHashesPerSecond);
    impl_into_sporad_response!(GetMempoolEntriesByAddresses);
    impl_into_sporad_response!(GetCoinSupply);
    impl_into_sporad_response!(Ping);
    impl_into_sporad_response!(GetMetrics);
    impl_into_sporad_response!(GetConnections);
    impl_into_sporad_response!(GetSystemInfo);
    impl_into_sporad_response!(GetServerInfo);
    impl_into_sporad_response!(GetSyncStatus);
    impl_into_sporad_response!(GetDaaScoreTimestampEstimate);
    impl_into_sporad_response!(GetFeeEstimate);
    impl_into_sporad_response!(GetFeeEstimateExperimental);
    impl_into_sporad_response!(GetCurrentBlockColor);
    impl_into_sporad_response!(GetCellReturnAddress);

    impl_into_sporad_notify_response!(NotifyBlockAdded);
    impl_into_sporad_notify_response!(NotifyNewBlockTemplate);
    impl_into_sporad_notify_response!(NotifyCellsChanged);
    impl_into_sporad_notify_response!(NotifyPruningPointCellSetOverride);
    impl_into_sporad_notify_response!(NotifyFinalityConflict);
    impl_into_sporad_notify_response!(NotifyVirtualDaaScoreChanged);
    impl_into_sporad_notify_response!(NotifyVirtualChainChanged);
    impl_into_sporad_notify_response!(NotifySinkBlueScoreChanged);

    impl_into_sporad_notify_response!(NotifyCellsChanged, StopNotifyingCellsChanged);
    impl_into_sporad_notify_response!(NotifyPruningPointCellSetOverride, StopNotifyingPruningPointCellSetOverride);

    macro_rules! impl_into_sporad_response {
        ($name:tt) => {
            paste::paste! {
                impl_into_sporad_response_ex!(spora_rpc_core::[<$name Response>],[<$name ResponseMessage>],[<$name Response>]);
            }
        };
        ($core_name:tt, $protowire_name:tt) => {
            paste::paste! {
                impl_into_sporad_response_base!(spora_rpc_core::[<$core_name Response>],[<$protowire_name ResponseMessage>],[<$protowire_name Response>]);
            }
        };
    }
    use impl_into_sporad_response;

    macro_rules! impl_into_sporad_response_base {
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

            impl From<$protowire_struct> for sporad_response::Payload {
                fn from(item: $protowire_struct) -> Self {
                    sporad_response::Payload::$variant(item)
                }
            }

            impl From<$protowire_struct> for SporadResponse {
                fn from(item: $protowire_struct) -> Self {
                    Self { id: 0, payload: Some(sporad_response::Payload::$variant(item)) }
                }
            }
        };
    }
    use impl_into_sporad_response_base;

    macro_rules! impl_into_sporad_response_ex {
        ($core_struct:path, $protowire_struct:ident, $variant:ident) => {
            // ----------------------------------------------------------------------------
            // rpc_core to protowire
            // ----------------------------------------------------------------------------

            impl From<RpcResult<&$core_struct>> for sporad_response::Payload {
                fn from(item: RpcResult<&$core_struct>) -> Self {
                    sporad_response::Payload::$variant(item.into())
                }
            }

            impl From<RpcResult<&$core_struct>> for SporadResponse {
                fn from(item: RpcResult<&$core_struct>) -> Self {
                    Self { id: 0, payload: Some(item.into()) }
                }
            }

            impl From<RpcResult<$core_struct>> for sporad_response::Payload {
                fn from(item: RpcResult<$core_struct>) -> Self {
                    sporad_response::Payload::$variant(item.into())
                }
            }

            impl From<RpcResult<$core_struct>> for SporadResponse {
                fn from(item: RpcResult<$core_struct>) -> Self {
                    Self { id: 0, payload: Some(item.into()) }
                }
            }

            impl_into_sporad_response_base!($core_struct, $protowire_struct, $variant);

            // ----------------------------------------------------------------------------
            // protowire to rpc_core
            // ----------------------------------------------------------------------------

            impl TryFrom<&sporad_response::Payload> for $core_struct {
                type Error = RpcError;
                fn try_from(item: &sporad_response::Payload) -> RpcResult<Self> {
                    if let sporad_response::Payload::$variant(response) = item {
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
    use impl_into_sporad_response_ex;

    macro_rules! impl_into_sporad_notify_response {
        ($name:tt) => {
            impl_into_sporad_response!($name);

            paste::paste! {
                impl_into_sporad_notify_response_ex!(spora_rpc_core::[<$name Response>],[<$name ResponseMessage>]);
            }
        };
        ($core_name:tt, $protowire_name:tt) => {
            impl_into_sporad_response!($core_name, $protowire_name);

            paste::paste! {
                impl_into_sporad_notify_response_ex!(spora_rpc_core::[<$core_name Response>],[<$protowire_name ResponseMessage>]);
            }
        };
    }
    use impl_into_sporad_notify_response;

    macro_rules! impl_into_sporad_notify_response_ex {
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
    use impl_into_sporad_notify_response_ex;
}
