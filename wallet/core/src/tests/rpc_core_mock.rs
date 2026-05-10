use crate::imports::*;

use async_channel::{unbounded, Receiver};
use async_trait::async_trait;
use spora_notify::events::EVENT_TYPE_ARRAY;
use spora_notify::listener::{ListenerId, ListenerLifespan};
use spora_notify::notifier::{Notifier, Notify};
use spora_notify::scope::Scope;
use spora_notify::subscription::context::SubscriptionContext;
use spora_notify::subscription::{CellsChangedMutationPolicy, MutationPolicies};
use spora_rpc_core::api::ctl::RpcCtl;
use spora_rpc_core::{api::connection::DynRpcConnection, api::rpc::RpcApi, *};
use spora_rpc_core::{
    notify::connection::{ChannelConnection, ChannelType},
    RpcResult,
};
use spora_utils::channel::Channel;
use std::sync::Arc;

pub type RpcCoreNotifier = Notifier<Notification, ChannelConnection>;

impl From<Arc<RpcCoreMock>> for Rpc {
    fn from(rpc_mock: Arc<RpcCoreMock>) -> Self {
        Self::new(rpc_mock.clone(), rpc_mock.ctl.clone())
    }
}

pub struct RpcCoreMock {
    ctl: RpcCtl,
    core_notifier: Arc<RpcCoreNotifier>,
    transactions: Arc<Mutex<HashMap<TransactionId, RpcTransaction>>>,
    _sync_receiver: Receiver<()>,
}

impl RpcCoreMock {
    pub fn new() -> Self {
        let (sync_sender, sync_receiver) = unbounded();
        let policies = MutationPolicies::new(CellsChangedMutationPolicy::AddressSet);
        let core_notifier: Arc<RpcCoreNotifier> = Arc::new(Notifier::with_sync(
            "rpc-core",
            EVENT_TYPE_ARRAY[..].into(),
            vec![],
            vec![],
            SubscriptionContext::new(),
            10,
            policies,
            Some(sync_sender),
        ));
        Self { core_notifier, transactions: Arc::new(Mutex::new(HashMap::new())), _sync_receiver: sync_receiver, ctl: RpcCtl::new() }
    }

    pub fn core_notifier(&self) -> Arc<RpcCoreNotifier> {
        self.core_notifier.clone()
    }

    #[allow(dead_code)]
    pub fn notify_new_block_template(&self) -> spora_notify::error::Result<()> {
        let notification = Notification::NewBlockTemplate(NewBlockTemplateNotification {});
        self.core_notifier.notify(notification)
    }

    #[allow(dead_code)]
    pub async fn notify_complete(&self) {
        assert!(self._sync_receiver.recv().await.is_ok(), "the notifier sync channel is unexpectedly empty and closed");
    }

    pub fn start(&self) {
        self.core_notifier.clone().start();
    }

    pub fn insert_transaction(&self, hash: TransactionId, transaction: RpcTransaction) {
        self.transactions.lock().unwrap().insert(hash, transaction);
    }

    pub async fn join(&self) {
        self.core_notifier.join().await.expect("core notifier shutdown")
    }

    // ---

    pub fn ctl(&self) -> RpcCtl {
        self.ctl.clone()
    }
}

impl Default for RpcCoreMock {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RpcApi for RpcCoreMock {
    // This fn needs to succeed while the client connects
    async fn get_info_call(&self, _connection: Option<&DynRpcConnection>, _request: GetInfoRequest) -> RpcResult<GetInfoResponse> {
        Ok(GetInfoResponse {
            p2p_id: "wallet-mock".to_string(),
            mempool_size: 1234,
            server_version: "mock".to_string(),
            is_cell_indexed: false,
            is_synced: false,
            has_notify_command: false,
            has_message_id: false,
        })
    }

    async fn ping_call(&self, _connection: Option<&DynRpcConnection>, _request: PingRequest) -> RpcResult<PingResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_metrics_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetMetricsRequest,
    ) -> RpcResult<GetMetricsResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_connections_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetConnectionsRequest,
    ) -> RpcResult<GetConnectionsResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_server_info_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetServerInfoRequest,
    ) -> RpcResult<GetServerInfoResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_system_info_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetSystemInfoRequest,
    ) -> RpcResult<GetSystemInfoResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_sync_status_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetSyncStatusRequest,
    ) -> RpcResult<GetSyncStatusResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_current_network_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetCurrentNetworkRequest,
    ) -> RpcResult<GetCurrentNetworkResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn submit_block_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: SubmitBlockRequest,
    ) -> RpcResult<SubmitBlockResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_block_template_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetBlockTemplateRequest,
    ) -> RpcResult<GetBlockTemplateResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_peer_addresses_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetPeerAddressesRequest,
    ) -> RpcResult<GetPeerAddressesResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_sink_call(&self, _connection: Option<&DynRpcConnection>, _request: GetSinkRequest) -> RpcResult<GetSinkResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_mempool_entry_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetMempoolEntryRequest,
    ) -> RpcResult<GetMempoolEntryResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_mempool_entries_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetMempoolEntriesRequest,
    ) -> RpcResult<GetMempoolEntriesResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_connected_peer_info_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetConnectedPeerInfoRequest,
    ) -> RpcResult<GetConnectedPeerInfoResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn submit_transaction_replacement_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: SubmitTransactionReplacementRequest,
    ) -> RpcResult<SubmitTransactionReplacementResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn add_peer_call(&self, _connection: Option<&DynRpcConnection>, _request: AddPeerRequest) -> RpcResult<AddPeerResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn submit_transaction_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: SubmitTransactionRequest,
    ) -> RpcResult<SubmitTransactionResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_block_call(&self, _connection: Option<&DynRpcConnection>, _request: GetBlockRequest) -> RpcResult<GetBlockResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_block_status_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetBlockStatusRequest,
    ) -> RpcResult<GetBlockStatusResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_transaction_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetTransactionRequest,
    ) -> RpcResult<GetTransactionResponse> {
        let transaction = self
            .transactions
            .lock()
            .unwrap()
            .get(&request.hash)
            .cloned()
            .ok_or_else(|| RpcError::TransactionNotFound(request.hash))?;
        Ok(GetTransactionResponse { transaction })
    }

    async fn get_virtual_chain_from_block_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetVirtualChainFromBlockRequest,
    ) -> RpcResult<GetVirtualChainFromBlockResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_blocks_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetBlocksRequest,
    ) -> RpcResult<GetBlocksResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_block_count_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetBlockCountRequest,
    ) -> RpcResult<GetBlockCountResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_block_dag_info_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetBlockDagInfoRequest,
    ) -> RpcResult<GetBlockDagInfoResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn resolve_finality_conflict_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: ResolveFinalityConflictRequest,
    ) -> RpcResult<ResolveFinalityConflictResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn shutdown_call(&self, _connection: Option<&DynRpcConnection>, _request: ShutdownRequest) -> RpcResult<ShutdownResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_headers_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetHeadersRequest,
    ) -> RpcResult<GetHeadersResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_header_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetHeaderRequest,
    ) -> RpcResult<GetHeaderResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_balance_by_address_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetBalanceByAddressRequest,
    ) -> RpcResult<GetBalanceByAddressResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_balances_by_addresses_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetBalancesByAddressesRequest,
    ) -> RpcResult<GetBalancesByAddressesResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_cells_by_address_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetCellsByAddressRequest,
    ) -> RpcResult<GetCellsByAddressResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_cells_by_addresses_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetCellsByAddressesRequest,
    ) -> RpcResult<GetCellsByAddressesResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_sink_blue_score_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetSinkBlueScoreRequest,
    ) -> RpcResult<GetSinkBlueScoreResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn ban_call(&self, _connection: Option<&DynRpcConnection>, _request: BanRequest) -> RpcResult<BanResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn unban_call(&self, _connection: Option<&DynRpcConnection>, _request: UnbanRequest) -> RpcResult<UnbanResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn estimate_network_hashes_per_second_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: EstimateNetworkHashesPerSecondRequest,
    ) -> RpcResult<EstimateNetworkHashesPerSecondResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_mempool_entries_by_addresses_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetMempoolEntriesByAddressesRequest,
    ) -> RpcResult<GetMempoolEntriesByAddressesResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_coin_supply_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetCoinSupplyRequest,
    ) -> RpcResult<GetCoinSupplyResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_daa_score_timestamp_estimate_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetDaaScoreTimestampEstimateRequest,
    ) -> RpcResult<GetDaaScoreTimestampEstimateResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_fee_estimate_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetFeeEstimateRequest,
    ) -> RpcResult<GetFeeEstimateResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_fee_estimate_experimental_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetFeeEstimateExperimentalRequest,
    ) -> RpcResult<GetFeeEstimateExperimentalResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_current_block_color_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetCurrentBlockColorRequest,
    ) -> RpcResult<GetCurrentBlockColorResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_cell_return_address_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetCellReturnAddressRequest,
    ) -> RpcResult<GetCellReturnAddressResponse> {
        Err(RpcError::NotImplemented)
    }

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Notification API

    fn register_new_listener(&self, connection: ChannelConnection) -> ListenerId {
        self.core_notifier.register_new_listener(connection, ListenerLifespan::Dynamic)
    }

    async fn unregister_listener(&self, id: ListenerId) -> RpcResult<()> {
        self.core_notifier.unregister_listener(id)?;
        Ok(())
    }

    async fn start_notify(&self, id: ListenerId, scope: Scope) -> RpcResult<()> {
        self.core_notifier.try_start_notify(id, scope)?;
        Ok(())
    }

    async fn stop_notify(&self, id: ListenerId, scope: Scope) -> RpcResult<()> {
        self.core_notifier.try_stop_notify(id, scope)?;
        Ok(())
    }

    async fn subscribe_notifications(&self, request: SubscribeNotificationsRequest) -> RpcResult<SubscribeNotificationsResponse> {
        let channel = Channel::default();
        let connection = ChannelConnection::new("rpc-core-mock", channel.sender(), ChannelType::Closable);
        let listener_id = self.core_notifier.register_new_listener(connection, ListenerLifespan::Dynamic);

        self.start_notify(listener_id, request.scope).await?;
        Ok(SubscribeNotificationsResponse::new(listener_id))
    }

    async fn unsubscribe_notifications(
        &self,
        request: UnsubscribeNotificationsRequest,
    ) -> RpcResult<UnsubscribeNotificationsResponse> {
        self.unregister_listener(request.subscription_id).await?;
        Ok(UnsubscribeNotificationsResponse {})
    }
}
