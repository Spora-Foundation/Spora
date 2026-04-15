//! Core server implementation for ClientAPI

use super::collector::{CollectorFromConsensus, CollectorFromIndex};
use crate::converter::feerate_estimate::{FeeEstimateConverter, FeeEstimateVerboseConverter};
use crate::converter::{consensus::ConsensusConverter, index::IndexConverter, protocol::ProtocolConverter};
use async_trait::async_trait;
use spora_cellindex::api::{CellIndexProxy, CellQuery};
use spora_consensus_client::pay_to_address_lock_script;
use spora_consensus_core::api::counters::ProcessingCounters;
use spora_consensus_core::daa_score_timestamp::DaaScoreTimestamp;
use spora_consensus_core::errors::block::RuleError;
use spora_consensus_core::tx::TransactionOutpoint;
use spora_consensus_core::{
    block::Block,
    coinbase::MinerData,
    config::Config,
    constants::MAX_SAU,
    network::NetworkType,
    tx::{CellTx, COINBASE_TRANSACTION_INDEX},
};
use spora_consensus_notify::{
    notifier::ConsensusNotifier,
    {connection::ConsensusChannelConnection, notification::Notification as ConsensusNotification},
};
use spora_consensusmanager::ConsensusManager;
use spora_core::time::unix_now;
use spora_core::{
    core::Core,
    debug,
    signals::Shutdown,
    sporad_env::version,
    task::service::{AsyncService, AsyncServiceError, AsyncServiceFuture},
    task::tick::TickService,
    trace, warn,
};
use spora_index_core::indexed_cells::{BalanceByAddress, CellSetByAddress, CompactCellCollection, CompactCellEntry};
use spora_index_core::{connection::IndexChannelConnection, notification::Notification as IndexNotification, notifier::IndexNotifier};
use spora_mining::feerate::FeeEstimateVerbose;
use spora_mining::model::tx_query::TransactionQuery;
use spora_mining::{manager::MiningManagerProxy, mempool::tx::Orphan};
use spora_notify::listener::ListenerLifespan;
use spora_notify::subscription::context::SubscriptionContext;
use spora_notify::subscription::{CellsChangedMutationPolicy, MutationPolicies};
use spora_notify::{
    collector::DynCollector,
    connection::ChannelType,
    events::{EventSwitches, EventType, EVENT_TYPE_ARRAY},
    listener::ListenerId,
    notifier::Notifier,
    scope::Scope,
    subscriber::{Subscriber, SubscriptionManager},
};
use spora_p2p_flows::flow_context::FlowContext;
use spora_p2p_lib::common::ProtocolError;
use spora_p2p_mining::rule_engine::MiningRuleEngine;
use spora_perf_monitor::{counters::CountersSnapshot, Monitor as PerfMonitor};
use spora_rpc_core::cell_collection_into_rpc;
use spora_rpc_core::{
    api::{
        connection::DynRpcConnection,
        ops::{RPC_API_REVISION, RPC_API_VERSION},
        rpc::{RpcApi, MAX_SAFE_WINDOW_SIZE},
    },
    model::*,
    notify::connection::ChannelConnection,
    Notification, RpcError, RpcResult,
};
use spora_utils::expiring_cache::ExpiringCache;
use spora_utils::sysinfo::SystemInfo;
use spora_utils::{channel::Channel, triggers::SingleTrigger};
use spora_utils_tower::counters::TowerConnectionCounters;
use std::time::Duration;
use std::{
    collections::HashMap,
    iter::once,
    sync::{atomic::Ordering, Arc},
    vec,
};
use tokio::join;
use workflow_rpc::server::WebSocketCounters as WrpcServerCounters;

fn resolve_cell_return_address(
    tx: &spora_consensus_core::tx::ResolvedCellTransaction,
    prefix: spora_addresses::Prefix,
) -> RpcResult<RpcAddress> {
    if tx.tx.inputs.is_empty() {
        return Err(RpcError::General("GetCellReturnAddress is not available for coinbase transactions".to_string()));
    }

    let resolved_input =
        tx.resolved_input(0).ok_or_else(|| RpcError::General("GetCellReturnAddress could not resolve the first input".to_string()))?;
    let lock_script = resolved_input.lock_script.as_ref().ok_or_else(|| {
        RpcError::General("GetCellReturnAddress is missing the first input lock script in resolved metadata".to_string())
    })?;

    spora_consensus_core::tx::extract_address_from_script(lock_script, prefix)
        .map_err(|error| RpcError::General(format!("GetCellReturnAddress failed to decode the first input lock script: {error}")))
}

/// A service implementing the Rpc API at spora_rpc_core level.
///
/// Collects notifications from the consensus and forwards them to
/// actual protocol-featured services. Thanks to the subscription pattern,
/// notifications are sent to the registered services only if the actually
/// need them.
///
/// ### Implementation notes
///
/// This was designed to have a unique instance in the whole application,
/// though multiple instances could coexist safely.
///
/// Any lower-level service providing an actual protocol, like gPRC should
/// register into this instance in order to get notifications. The data flow
/// from this instance to registered services and backwards should occur
/// by adding respectively to the registered service a Collector and a
/// Subscriber.
pub struct RpcCoreService {
    consensus_manager: Arc<ConsensusManager>,
    notifier: Arc<Notifier<Notification, ChannelConnection>>,
    mining_manager: MiningManagerProxy,
    flow_context: Arc<FlowContext>,
    cellindex: Option<CellIndexProxy>,
    config: Arc<Config>,
    consensus_converter: Arc<ConsensusConverter>,
    index_converter: Arc<IndexConverter>,
    protocol_converter: Arc<ProtocolConverter>,
    core: Arc<Core>,
    processing_counters: Arc<ProcessingCounters>,
    wrpc_borsh_counters: Arc<WrpcServerCounters>,
    wrpc_json_counters: Arc<WrpcServerCounters>,
    shutdown: SingleTrigger,
    core_shutdown_request: SingleTrigger,
    perf_monitor: Arc<PerfMonitor<Arc<TickService>>>,
    p2p_tower_counters: Arc<TowerConnectionCounters>,
    grpc_tower_counters: Arc<TowerConnectionCounters>,
    system_info: SystemInfo,
    fee_estimate_cache: ExpiringCache<RpcFeeEstimate>,
    fee_estimate_verbose_cache: ExpiringCache<spora_mining::errors::MiningManagerResult<GetFeeEstimateExperimentalResponse>>,
    mining_rule_engine: Arc<MiningRuleEngine>,
}

const RPC_CORE: &str = "rpc-core";

impl RpcCoreService {
    pub const IDENT: &'static str = "rpc-core-service";

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        consensus_manager: Arc<ConsensusManager>,
        consensus_notifier: Arc<ConsensusNotifier>,
        index_notifier: Option<Arc<IndexNotifier>>,
        mining_manager: MiningManagerProxy,
        flow_context: Arc<FlowContext>,
        subscription_context: SubscriptionContext,
        cellindex: Option<CellIndexProxy>,
        config: Arc<Config>,
        core: Arc<Core>,
        processing_counters: Arc<ProcessingCounters>,
        wrpc_borsh_counters: Arc<WrpcServerCounters>,
        wrpc_json_counters: Arc<WrpcServerCounters>,
        perf_monitor: Arc<PerfMonitor<Arc<TickService>>>,
        p2p_tower_counters: Arc<TowerConnectionCounters>,
        grpc_tower_counters: Arc<TowerConnectionCounters>,
        system_info: SystemInfo,
        mining_rule_engine: Arc<MiningRuleEngine>,
    ) -> Self {
        // This notifier uses the address-scoped mutation policy from the current
        // notify framework while the backing index feed is Cell-model based.
        let policies = match index_notifier {
            Some(_) => MutationPolicies::new(CellsChangedMutationPolicy::AddressSet),
            None => MutationPolicies::new(CellsChangedMutationPolicy::Wildcard),
        };

        // Prepare consensus-notify objects
        let consensus_notify_channel = Channel::<ConsensusNotification>::default();
        let consensus_notify_listener_id = consensus_notifier.register_new_listener(
            ConsensusChannelConnection::new(RPC_CORE, consensus_notify_channel.sender(), ChannelType::Closable),
            ListenerLifespan::Static(Default::default()),
        );

        // Prepare the rpc-core notifier objects
        let mut consensus_events: EventSwitches = EVENT_TYPE_ARRAY[..].into();
        consensus_events[EventType::CellsChanged] = false;
        consensus_events[EventType::PruningPointCellSetOverride] = index_notifier.is_none();
        let consensus_converter = Arc::new(ConsensusConverter::new(consensus_manager.clone(), config.clone()));
        let consensus_collector = Arc::new(CollectorFromConsensus::new(
            "rpc-core <= consensus",
            consensus_notify_channel.receiver(),
            consensus_converter.clone(),
        ));
        let consensus_subscriber =
            Arc::new(Subscriber::new("rpc-core => consensus", consensus_events, consensus_notifier, consensus_notify_listener_id));

        let mut collectors: Vec<DynCollector<Notification>> = vec![consensus_collector];
        let mut subscribers = vec![consensus_subscriber];

        // Prepare index-processor objects if an IndexService is provided
        let index_converter = Arc::new(IndexConverter::new(config.clone()));
        if let Some(ref index_notifier) = index_notifier {
            let index_notify_channel = Channel::<IndexNotification>::default();
            let index_notify_listener_id = index_notifier.clone().register_new_listener(
                IndexChannelConnection::new(RPC_CORE, index_notify_channel.sender(), ChannelType::Closable),
                ListenerLifespan::Static(policies),
            );

            let index_events: EventSwitches = [EventType::CellsChanged, EventType::PruningPointCellSetOverride].as_ref().into();
            let index_collector =
                Arc::new(CollectorFromIndex::new("rpc-core <= index", index_notify_channel.receiver(), index_converter.clone()));
            let index_subscriber =
                Arc::new(Subscriber::new("rpc-core => index", index_events, index_notifier.clone(), index_notify_listener_id));

            collectors.push(index_collector);
            subscribers.push(index_subscriber);
        }

        // Protocol converter
        let protocol_converter = Arc::new(ProtocolConverter::new(flow_context.clone()));

        // Create the rcp-core notifier
        let notifier =
            Arc::new(Notifier::new(RPC_CORE, EVENT_TYPE_ARRAY[..].into(), collectors, subscribers, subscription_context, 1, policies));

        Self {
            consensus_manager,
            notifier,
            mining_manager,
            flow_context,
            cellindex,
            config,
            consensus_converter,
            index_converter,
            protocol_converter,
            core,
            processing_counters,
            wrpc_borsh_counters,
            wrpc_json_counters,
            shutdown: SingleTrigger::default(),
            core_shutdown_request: SingleTrigger::default(),
            perf_monitor,
            p2p_tower_counters,
            grpc_tower_counters,
            system_info,
            fee_estimate_cache: ExpiringCache::new(Duration::from_millis(500), Duration::from_millis(1000)),
            fee_estimate_verbose_cache: ExpiringCache::new(Duration::from_millis(500), Duration::from_millis(1000)),
            mining_rule_engine,
        }
    }

    pub fn start_impl(&self) {
        self.notifier().start();
    }

    pub async fn join(&self) -> RpcResult<()> {
        trace!("{} joining notifier", Self::IDENT);
        self.notifier().join().await?;
        Ok(())
    }

    #[inline(always)]
    pub fn notifier(&self) -> Arc<Notifier<Notification, ChannelConnection>> {
        self.notifier.clone()
    }

    #[inline(always)]
    pub fn subscription_context(&self) -> SubscriptionContext {
        self.notifier.subscription_context().clone()
    }

    pub fn core_shutdown_request_listener(&self) -> triggered::Listener {
        self.core_shutdown_request.listener.clone()
    }

    fn cellindex(&self) -> RpcResult<&CellIndexProxy> {
        self.cellindex.as_ref().ok_or_else(|| RpcError::General("Cell index is not initialized".to_string()))
    }

    async fn get_cell_set_by_address(&self, address: &RpcAddress, start: u64, limit: u32) -> RpcResult<(CompactCellCollection, u64)> {
        if limit == 0 {
            return Ok((CompactCellCollection::default(), 0));
        }

        let query_limit = usize::try_from(start).unwrap_or(usize::MAX).saturating_add(limit as usize);
        let lock_script = pay_to_address_lock_script(address);
        let result = self
            .cellindex()?
            .query(&CellQuery::by_lock(lock_script.hash(), query_limit))
            .map_err(|err| RpcError::General(format!("Cell index query failed: {err}")))?;

        let start = usize::try_from(start).unwrap_or(usize::MAX);
        let collection = result
            .cells
            .into_iter()
            .skip(start)
            .map(|(out_point, meta)| (Self::transaction_outpoint_from_exec(&out_point), Self::compact_cell_entry_from_meta(&meta)))
            .collect();

        Ok((collection, result.total_count as u64))
    }

    async fn get_balance_by_addresses<'a>(&self, addresses: impl Iterator<Item = &'a RpcAddress>) -> RpcResult<BalanceByAddress> {
        let mut balances = BalanceByAddress::default();

        for address in addresses {
            let (cells, _) = self.get_cell_set_by_address(address, 0, u32::MAX).await?;
            let balance = cells.values().map(|entry| entry.amount).sum();
            balances.insert(address.clone(), balance);
        }

        Ok(balances)
    }

    async fn get_cell_set_by_addresses<'a>(&self, addresses: impl Iterator<Item = &'a RpcAddress>) -> RpcResult<CellSetByAddress> {
        let mut entry_map = CellSetByAddress::default();

        for address in addresses {
            let (cells, _) = self.get_cell_set_by_address(address, 0, u32::MAX).await?;
            if !cells.is_empty() {
                entry_map.insert(address.clone(), cells);
            }
        }

        Ok(entry_map)
    }

    fn transaction_outpoint_from_exec(out_point: &spora_exec::OutPoint) -> TransactionOutpoint {
        TransactionOutpoint::new(out_point.tx_hash.into(), out_point.index)
    }

    fn compact_cell_entry_from_meta(meta: &spora_state::index::CellMeta) -> CompactCellEntry {
        CompactCellEntry::new(
            meta.cell_output.capacity,
            meta.cell_data.len() as u64,
            meta.cell_output.lock.hash(),
            meta.cell_output.type_.as_ref().map(|script| script.hash()),
            *blake3::hash(&meta.cell_data).as_bytes(),
            meta.daa_score,
            meta.is_cellbase,
        )
    }

    fn extract_tx_query(&self, filter_transaction_pool: bool, include_orphan_pool: bool) -> RpcResult<TransactionQuery> {
        match (filter_transaction_pool, include_orphan_pool) {
            (true, true) => Ok(TransactionQuery::OrphansOnly),
            // Note that the first `true` indicates *filtering* transactions and the second `false` indicates not including
            // orphan txs -- hence the query would be empty by definition and is thus useless
            (true, false) => Err(RpcError::InconsistentMempoolTxQuery),
            (false, true) => Ok(TransactionQuery::All),
            (false, false) => Ok(TransactionQuery::TransactionsOnly),
        }
    }
}

#[async_trait]
impl RpcApi for RpcCoreService {
    async fn submit_block_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: SubmitBlockRequest,
    ) -> RpcResult<SubmitBlockResponse> {
        let session = self.consensus_manager.consensus().unguarded_session();
        let sink_daa_score_timestamp = session.async_get_sink_daa_score_timestamp().await;

        // TODO: consider adding an error field to SubmitBlockReport to document both the report and error fields
        let is_synced: bool = self.mining_rule_engine.should_mine(sink_daa_score_timestamp);

        if !self.config.enable_unsynced_mining && !is_synced {
            // error = "Block not submitted - node is not synced"
            return Ok(SubmitBlockResponse { report: SubmitBlockReport::Reject(SubmitBlockRejectReason::IsInIBD) });
        }

        let try_block: RpcResult<Block> = request.block.try_into();
        if let Err(err) = &try_block {
            trace!("incoming SubmitBlockRequest with block conversion error: {}", err);
            // error = format!("Could not parse block: {0}", err)
            return Ok(SubmitBlockResponse { report: SubmitBlockReport::Reject(SubmitBlockRejectReason::BlockInvalid) });
        }
        let block = try_block?;
        let hash = block.hash();

        if !request.allow_non_daa_blocks {
            let virtual_daa_score = session.get_virtual_daa_score();

            // A simple heuristic check which signals that the mined block is out of date
            // and should not be accepted unless user explicitly requests
            //
            let difficulty_window_duration = self.config.difficulty_window_duration_in_block_units();
            if virtual_daa_score > difficulty_window_duration
                && block.header.daa_score < virtual_daa_score - difficulty_window_duration
            {
                // error = format!("Block rejected. Reason: block DAA score {0} is too far behind virtual's DAA score {1}", block.header.daa_score, virtual_daa_score)
                return Ok(SubmitBlockResponse { report: SubmitBlockReport::Reject(SubmitBlockRejectReason::BlockInvalid) });
            }
        }

        trace!("incoming SubmitBlockRequest for block {}", hash);
        match self.flow_context.submit_rpc_block(&session, block.clone()).await {
            Ok(_) => Ok(SubmitBlockResponse { report: SubmitBlockReport::Success }),
            Err(ProtocolError::RuleError(RuleError::BadMerkleRoot(h1, h2))) => {
                warn!(
                    "The RPC submitted block {} triggered a {} error: {}.
NOTE: This error usually indicates an RPC conversion error between the node and the miner. This is likely to reflect using a NON-SUPPORTED miner.",
                    hash,
                    stringify!(RuleError::BadMerkleRoot),
                    RuleError::BadMerkleRoot(h1, h2)
                );
                if matches!(self.config.net.network_type, NetworkType::Mainnet | NetworkType::Testnet) {
                    warn!("Printing the full block for debug purposes:\n{:?}", block);
                }
                Ok(SubmitBlockResponse { report: SubmitBlockReport::Reject(SubmitBlockRejectReason::BlockInvalid) })
            }
            Err(err) => {
                warn!("The RPC submitted block triggered an error: {}\nPrinting the full block for debug purposes:\n{:?}", err, block);
                Ok(SubmitBlockResponse { report: SubmitBlockReport::Reject(SubmitBlockRejectReason::BlockInvalid) })
            }
        }
    }

    async fn get_block_template_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetBlockTemplateRequest,
    ) -> RpcResult<GetBlockTemplateResponse> {
        trace!("incoming GetBlockTemplate request");

        // Make sure the pay address prefix matches the config network type
        if request.pay_address.prefix != self.config.prefix() {
            return Err(spora_addresses::AddressError::InvalidPrefix(request.pay_address.prefix.to_string()))?;
        }

        // Build block template
        let extra_data = version().as_bytes().iter().chain(once(&(b'/'))).chain(&request.extra_data).cloned().collect::<Vec<_>>();
        let miner_data: MinerData = MinerData::new(pay_to_address_lock_script(&request.pay_address), extra_data);
        let session = self.consensus_manager.consensus().unguarded_session();
        let block_template = self.mining_manager.clone().get_block_template(&session, miner_data).await?;

        // Check coinbase transaction payload length.
        if block_template.block.transactions[COINBASE_TRANSACTION_INDEX].payload().map_or(0, |p| p.len())
            > self.config.max_coinbase_payload_len
        {
            return Err(RpcError::CoinbasePayloadLengthAboveMax(self.config.max_coinbase_payload_len));
        }

        Ok(GetBlockTemplateResponse {
            block: block_template.block.into(),
            is_synced: self.mining_rule_engine.should_mine(DaaScoreTimestamp {
                timestamp: block_template.selected_parent_timestamp,
                daa_score: block_template.selected_parent_daa_score,
            }),
        })
    }

    async fn get_current_block_color_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetCurrentBlockColorRequest,
    ) -> RpcResult<GetCurrentBlockColorResponse> {
        let session = self.consensus_manager.consensus().unguarded_session();

        match session.async_get_current_block_color(request.hash).await {
            Some(blue) => Ok(GetCurrentBlockColorResponse { blue }),
            None => Err(RpcError::MergerNotFound(request.hash)),
        }
    }

    async fn get_block_call(&self, _connection: Option<&DynRpcConnection>, request: GetBlockRequest) -> RpcResult<GetBlockResponse> {
        // TODO: test
        let session = self.consensus_manager.consensus().session().await;
        let block = session.async_get_block_even_if_header_only(request.hash).await?;
        Ok(GetBlockResponse {
            block: self
                .consensus_converter
                .get_block(&session, &block, request.include_transactions, request.include_transactions)
                .await?,
        })
    }

    async fn get_block_status_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetBlockStatusRequest,
    ) -> RpcResult<GetBlockStatusResponse> {
        let session = self.consensus_manager.consensus().session().await;
        if let Some(status) = session.async_get_block_status(request.hash).await {
            Ok(GetBlockStatusResponse { status: self.consensus_converter.get_block_status(&status) })
        } else {
            Err(RpcError::InvalidBlock(request.hash))
        }
    }

    async fn get_transaction_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetTransactionRequest,
    ) -> RpcResult<GetTransactionResponse> {
        let session = self.consensus_manager.consensus().session().await;
        let tx = session.async_get_cell_transaction(request.hash).await?;
        let (block_hash, _) = session.async_get_transaction_location(request.hash).await.map_err(RpcError::General)?;
        let header = session.async_get_header(block_hash).await?;
        let transaction = if tx.is_coinbase() {
            self.consensus_converter.get_cell_transaction(&session, &tx, Some(&header), false)
        } else {
            let resolved = session
                .async_get_resolved_cell_transaction_in_accepting_block(request.hash, block_hash)
                .await
                .map_err(RpcError::General)?;
            self.consensus_converter.get_resolved_cell_transaction(&resolved, Some(&header), false)
        };
        Ok(GetTransactionResponse { transaction })
    }

    async fn get_blocks_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetBlocksRequest,
    ) -> RpcResult<GetBlocksResponse> {
        // Validate that user didn't set include_transactions without setting include_blocks
        if !request.include_blocks && request.include_transactions {
            return Err(RpcError::InvalidGetBlocksRequest);
        }

        let session = self.consensus_manager.consensus().session().await;

        // If low_hash is empty - use genesis instead.
        let low_hash = match request.low_hash {
            Some(low_hash) => {
                // Make sure low_hash points to an existing and valid block
                session.async_get_ghostdag_data(low_hash).await?;
                low_hash
            }
            None => self.config.genesis.hash,
        };

        // Get hashes between low_hash and sink
        let sink_hash = session.async_get_sink().await;

        // We use +1 because low_hash is also returned
        // max_blocks MUST be >= mergeset_size_limit + 1
        let max_blocks = self.config.mergeset_size_limit() as usize + 1;
        let (block_hashes, high_hash) = session.async_get_hashes_between(low_hash, sink_hash, max_blocks).await?;

        // If the high hash is equal to sink it means get_hashes_between didn't skip any hashes, and
        // there's space to add the sink anticone, otherwise we cannot add the anticone because
        // there's no guarantee that all of the anticone root ancestors will be present.
        let sink_anticone = if high_hash == sink_hash { session.async_get_anticone(sink_hash).await? } else { vec![] };
        // Prepend low hash to make it inclusive and append the sink anticone
        let block_hashes = once(low_hash).chain(block_hashes).chain(sink_anticone).collect::<Vec<_>>();
        let blocks = if request.include_blocks {
            let mut blocks = Vec::with_capacity(block_hashes.len());
            for hash in block_hashes.iter().copied() {
                let block = session.async_get_block_even_if_header_only(hash).await?;
                let rpc_block = self
                    .consensus_converter
                    .get_block(&session, &block, request.include_transactions, request.include_transactions)
                    .await?;
                blocks.push(rpc_block)
            }
            blocks
        } else {
            Vec::new()
        };
        Ok(GetBlocksResponse { block_hashes, blocks })
    }

    async fn get_info_call(&self, _connection: Option<&DynRpcConnection>, _request: GetInfoRequest) -> RpcResult<GetInfoResponse> {
        let sink_daa_score_timestamp =
            self.consensus_manager.consensus().unguarded_session().async_get_sink_daa_score_timestamp().await;
        Ok(GetInfoResponse {
            p2p_id: self.flow_context.node_id.to_string(),
            mempool_size: self.mining_manager.transaction_count_sample(TransactionQuery::TransactionsOnly),
            server_version: version().to_string(),
            is_cell_indexed: self.config.cellindex,
            is_synced: self.mining_rule_engine.is_sink_recent_and_connected(sink_daa_score_timestamp),
            has_notify_command: true,
            has_message_id: true,
        })
    }

    async fn get_mempool_entry_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetMempoolEntryRequest,
    ) -> RpcResult<GetMempoolEntryResponse> {
        let query = self.extract_tx_query(request.filter_transaction_pool, request.include_orphan_pool)?;
        let Some(transaction) = self.mining_manager.clone().get_transaction(request.transaction_id, query).await else {
            return Err(RpcError::TransactionNotFound(request.transaction_id));
        };
        let session = self.consensus_manager.consensus().unguarded_session();
        Ok(GetMempoolEntryResponse::new(self.consensus_converter.get_mempool_entry(&session, &transaction)))
    }

    async fn get_mempool_entries_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetMempoolEntriesRequest,
    ) -> RpcResult<GetMempoolEntriesResponse> {
        let query = self.extract_tx_query(request.filter_transaction_pool, request.include_orphan_pool)?;
        let session = self.consensus_manager.consensus().unguarded_session();
        let (transactions, orphans) = self.mining_manager.clone().get_all_transactions(query).await;
        let mempool_entries = transactions
            .iter()
            .chain(orphans.iter())
            .map(|transaction| self.consensus_converter.get_mempool_entry(&session, transaction))
            .collect();
        Ok(GetMempoolEntriesResponse::new(mempool_entries))
    }

    async fn get_mempool_entries_by_addresses_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetMempoolEntriesByAddressesRequest,
    ) -> RpcResult<GetMempoolEntriesByAddressesResponse> {
        let query = self.extract_tx_query(request.filter_transaction_pool, request.include_orphan_pool)?;
        let session = self.consensus_manager.consensus().unguarded_session();
        let addresses = request.addresses.iter().cloned().collect();
        let grouped_txs = self.mining_manager.clone().get_transactions_by_addresses(addresses, query).await;
        let mempool_entries = grouped_txs
            .owners
            .iter()
            .map(|(address, owner_transactions)| {
                self.consensus_converter.get_mempool_entries_by_address(
                    &session,
                    address.clone(),
                    owner_transactions,
                    &grouped_txs.transactions,
                )
            })
            .collect();
        Ok(GetMempoolEntriesByAddressesResponse::new(mempool_entries))
    }

    async fn submit_transaction_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: SubmitTransactionRequest,
    ) -> RpcResult<SubmitTransactionResponse> {
        let allow_orphan = self.config.unsafe_rpc && request.allow_orphan;
        if !self.config.unsafe_rpc && request.allow_orphan {
            debug!("SubmitTransaction RPC command called with AllowOrphan enabled while node in safe RPC mode -- switching to ForbidOrphan.");
        }

        let transaction: CellTx = request.transaction.try_into()?;
        let transaction_id = transaction.id().into();
        let session = self.consensus_manager.consensus().unguarded_session();
        let orphan = match allow_orphan {
            true => Orphan::Allowed,
            false => Orphan::Forbidden,
        };
        self.flow_context.submit_rpc_transaction(&session, transaction, orphan).await.map_err(|err| {
            let err = RpcError::RejectedTransaction(transaction_id, err.to_string());
            debug!("{err}");
            err
        })?;
        Ok(SubmitTransactionResponse::new(transaction_id))
    }

    async fn submit_transaction_replacement_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: SubmitTransactionReplacementRequest,
    ) -> RpcResult<SubmitTransactionReplacementResponse> {
        let transaction: CellTx = request.transaction.try_into()?;
        let transaction_id = transaction.id().into();
        let session = self.consensus_manager.consensus().unguarded_session();
        let replaced_transaction =
            self.flow_context.submit_rpc_transaction_replacement(&session, transaction).await.map_err(|err| {
                let err = RpcError::RejectedTransaction(transaction_id, err.to_string());
                debug!("{err}");
                err
            })?;
        Ok(SubmitTransactionReplacementResponse::new(transaction_id, (&*replaced_transaction).into()))
    }

    async fn get_current_network_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _: GetCurrentNetworkRequest,
    ) -> RpcResult<GetCurrentNetworkResponse> {
        Ok(GetCurrentNetworkResponse::new(*self.config.net))
    }
    async fn get_sink_call(&self, _connection: Option<&DynRpcConnection>, _: GetSinkRequest) -> RpcResult<GetSinkResponse> {
        Ok(GetSinkResponse::new(self.consensus_manager.consensus().unguarded_session().async_get_sink().await))
    }

    async fn get_sink_blue_score_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _: GetSinkBlueScoreRequest,
    ) -> RpcResult<GetSinkBlueScoreResponse> {
        let session = self.consensus_manager.consensus().unguarded_session();
        Ok(GetSinkBlueScoreResponse::new(session.async_get_ghostdag_data(session.async_get_sink().await).await?.blue_score))
    }

    async fn get_virtual_chain_from_block_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetVirtualChainFromBlockRequest,
    ) -> RpcResult<GetVirtualChainFromBlockResponse> {
        let session = self.consensus_manager.consensus().session().await;

        // batch_size is set to 10 times the mergeset_size_limit.
        // this means batch_size is 2480 on 10 bps, and 1800 on mainnet.
        // this bounds by number of merged blocks, if include_accepted_transactions = true
        // else it returns the batch_size amount on pure chain blocks.
        // Note: batch_size does not bound removed chain blocks, only added chain blocks.
        let batch_size = (self.config.mergeset_size_limit() * 10) as usize;
        let mut virtual_chain_batch = session.async_get_virtual_chain_from_block(request.start_hash, Some(batch_size)).await?;

        // Apply min confirmation count filter if specified
        if let Some(min_confirmation_count) = request.min_confirmation_count {
            if min_confirmation_count > 0 {
                let sink_blue_score = session.async_get_sink_blue_score().await;

                // Collect blue scores for all blocks first
                let mut filtered_blocks = Vec::new();
                for &block_hash in &virtual_chain_batch.added {
                    if let Ok(block_blue_score) = session.async_get_block_blue_score(block_hash).await {
                        let confirmations = sink_blue_score.saturating_sub(block_blue_score);
                        if confirmations >= min_confirmation_count {
                            filtered_blocks.push(block_hash);
                        }
                    }
                }
                virtual_chain_batch.added = filtered_blocks;
            }
        }

        let accepted_transaction_ids = if request.include_accepted_transaction_ids {
            let accepted_transaction_ids = self
                .consensus_converter
                .get_virtual_chain_accepted_transaction_ids(&session, &virtual_chain_batch, Some(batch_size))
                .await?;
            // bound added to the length of the accepted transaction ids, which is bounded by merged blocks
            virtual_chain_batch.added.truncate(accepted_transaction_ids.len());
            accepted_transaction_ids
        } else {
            vec![]
        };
        Ok(GetVirtualChainFromBlockResponse::new(virtual_chain_batch.removed, virtual_chain_batch.added, accepted_transaction_ids))
    }

    async fn get_block_count_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _: GetBlockCountRequest,
    ) -> RpcResult<GetBlockCountResponse> {
        Ok(self.consensus_manager.consensus().unguarded_session().async_estimate_block_count().await)
    }

    async fn get_cells_by_address_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetCellsByAddressRequest,
    ) -> RpcResult<GetCellsByAddressResponse> {
        if !self.config.cellindex {
            return Err(RpcError::NoCellIndex);
        }
        let GetCellsByAddressRequest { address, start, limit } = request;
        let (cell_collection, total) = self.get_cell_set_by_address(&address, start, limit).await?;
        let mut entries = cell_collection_into_rpc(&cell_collection);
        entries.iter_mut().for_each(|entry| entry.address = Some(address.clone()));
        Ok(GetCellsByAddressResponse::new(entries, total))
    }

    async fn get_cells_by_addresses_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetCellsByAddressesRequest,
    ) -> RpcResult<GetCellsByAddressesResponse> {
        if !self.config.cellindex {
            return Err(RpcError::NoCellIndex);
        }
        // TODO: discuss if the entry order is part of the method requirements
        //       (the current impl does not retain an entry order matching the request addresses order)
        let entry_map = self.get_cell_set_by_addresses(request.addresses.iter()).await?;
        Ok(GetCellsByAddressesResponse::new(self.index_converter.get_cells_by_addresses_entries(&entry_map)))
    }

    async fn get_balance_by_address_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetBalanceByAddressRequest,
    ) -> RpcResult<GetBalanceByAddressResponse> {
        if !self.config.cellindex {
            return Err(RpcError::NoCellIndex);
        }
        let entry_map = self.get_balance_by_addresses(once(&request.address)).await?;
        let balance = entry_map.values().sum();
        Ok(GetBalanceByAddressResponse::new(balance))
    }

    async fn get_balances_by_addresses_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetBalancesByAddressesRequest,
    ) -> RpcResult<GetBalancesByAddressesResponse> {
        if !self.config.cellindex {
            return Err(RpcError::NoCellIndex);
        }
        let entry_map = self.get_balance_by_addresses(request.addresses.iter()).await?;
        let entries = request
            .addresses
            .iter()
            .map(|address| {
                let balance = entry_map.get(address).copied();
                RpcBalancesByAddressesEntry { address: address.to_owned(), balance }
            })
            .collect();
        Ok(GetBalancesByAddressesResponse::new(entries))
    }

    async fn get_coin_supply_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _: GetCoinSupplyRequest,
    ) -> RpcResult<GetCoinSupplyResponse> {
        if !self.config.cellindex {
            return Err(RpcError::NoCellIndex);
        }
        let circulating_sau = self
            .cellindex()?
            .total_live_capacity()
            .map_err(|err| RpcError::General(format!("Cell index supply query failed: {err}")))?;
        Ok(GetCoinSupplyResponse::new(MAX_SAU, circulating_sau))
    }

    async fn get_daa_score_timestamp_estimate_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetDaaScoreTimestampEstimateRequest,
    ) -> RpcResult<GetDaaScoreTimestampEstimateResponse> {
        let session = self.consensus_manager.consensus().session().await;
        // TODO: cache samples based on sufficient recency of the data and append sink data
        let mut headers = session.async_get_chain_block_samples().await;
        let mut requested_daa_scores = request.daa_scores.clone();
        let mut daa_score_timestamp_map = HashMap::<u64, u64>::new();

        headers.reverse();
        requested_daa_scores.sort_by(|a, b| b.cmp(a));

        let mut header_idx = 0;
        let mut req_idx = 0;

        // TODO (relaxed; post-HF): the below interpolation should remain valid also after the hardfork as long
        // as the two pruning points used are both either from before activation or after. The only exception are
        // the two pruning points before and after activation. However this inaccuracy can be considered negligible.
        // Alternatively, we can remedy this post the HF by manually adding a (DAA score, timestamp) point from the
        // moment of activation.

        // Loop runs at O(n + m) where n = # pp headers, m = # requested daa_scores
        // Loop will always end because in the worst case the last header with daa_score = 0 (the genesis)
        // will cause every remaining requested daa_score to be "found in range"
        //
        // TODO: optimize using binary search over the samples to obtain O(m log n) complexity (which is an improvement assuming m << n)
        while header_idx < headers.len() && req_idx < request.daa_scores.len() {
            let header = headers.get(header_idx).unwrap();
            let curr_daa_score = requested_daa_scores[req_idx];

            // Found daa_score in range
            if header.daa_score <= curr_daa_score {
                // For daa_score later than the last header, we estimate in milliseconds based on the difference
                let time_adjustment = if header_idx == 0 {
                    // estimate milliseconds = (daa_score * target_time_per_block)
                    (curr_daa_score - header.daa_score).saturating_mul(self.config.target_time_per_block())
                } else {
                    // "next" header is the one that we processed last iteration
                    let next_header = &headers[header_idx - 1];
                    // Unlike DAA scores which are monotonic (over the selected chain), timestamps are not strictly monotonic, so we avoid assuming so
                    let time_between_headers = next_header.timestamp.checked_sub(header.timestamp).unwrap_or_default();
                    let score_between_query_and_header = (curr_daa_score - header.daa_score) as f64;
                    let score_between_headers = (next_header.daa_score - header.daa_score) as f64;
                    // Interpolate the timestamp delta using the estimated fraction based on DAA scores
                    ((time_between_headers as f64) * (score_between_query_and_header / score_between_headers)) as u64
                };

                let daa_score_timestamp = header.timestamp.saturating_add(time_adjustment);
                daa_score_timestamp_map.insert(curr_daa_score, daa_score_timestamp);

                // Process the next daa score that's <= than current one (at earlier idx)
                req_idx += 1;
            } else {
                header_idx += 1;
            }
        }

        // Note: it is safe to assume all entries exist in the map since the first sampled header is expected to have daa_score=0
        let timestamps = request.daa_scores.iter().map(|curr_daa_score| daa_score_timestamp_map[curr_daa_score]).collect();

        Ok(GetDaaScoreTimestampEstimateResponse::new(timestamps))
    }

    async fn get_fee_estimate_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetFeeEstimateRequest,
    ) -> RpcResult<GetFeeEstimateResponse> {
        let mining_manager = self.mining_manager.clone();
        let consensus_manager = self.consensus_manager.clone();
        let estimate = self
            .fee_estimate_cache
            .get(async move {
                mining_manager
                    .get_realtime_feerate_estimations(consensus_manager.consensus().unguarded_session().get_virtual_daa_score())
                    .await
                    .into_rpc()
            })
            .await;
        Ok(GetFeeEstimateResponse { estimate })
    }

    async fn get_fee_estimate_experimental_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetFeeEstimateExperimentalRequest,
    ) -> RpcResult<GetFeeEstimateExperimentalResponse> {
        if request.verbose {
            let mining_manager = self.mining_manager.clone();
            let consensus_manager = self.consensus_manager.clone();
            let prefix = self.config.prefix();

            let response = self
                .fee_estimate_verbose_cache
                .get(async move {
                    let session = consensus_manager.consensus().unguarded_session();
                    mining_manager.get_realtime_feerate_estimations_verbose(&session, prefix).await.map(FeeEstimateVerbose::into_rpc)
                })
                .await?;
            Ok(response)
        } else {
            let estimate = self.get_fee_estimate_call(connection, GetFeeEstimateRequest {}).await?.estimate;
            Ok(GetFeeEstimateExperimentalResponse { estimate, verbose: None })
        }
    }

    async fn get_cell_return_address_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetCellReturnAddressRequest,
    ) -> RpcResult<GetCellReturnAddressResponse> {
        let session = self.consensus_manager.consensus().session().await;
        let tx = session
            .async_get_resolved_cell_transaction(request.txid, request.accepting_block_daa_score)
            .await
            .map_err(|error| RpcError::General(format!("GetCellReturnAddress failed: {error}")))?;

        Ok(GetCellReturnAddressResponse::new(resolve_cell_return_address(&tx, self.config.prefix())?))
    }

    async fn ping_call(&self, _connection: Option<&DynRpcConnection>, _: PingRequest) -> RpcResult<PingResponse> {
        Ok(PingResponse {})
    }

    async fn get_header_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetHeaderRequest,
    ) -> RpcResult<GetHeaderResponse> {
        let session: spora_consensusmanager::ConsensusSessionOwned = self.consensus_manager.consensus().session().await;
        let header = session.async_get_header(request.hash).await?;
        Ok(GetHeaderResponse { header: From::from(&*header) })
    }

    async fn get_headers_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetHeadersRequest,
    ) -> RpcResult<GetHeadersResponse> {
        if request.limit == 0 {
            return Ok(GetHeadersResponse::new(Vec::new()));
        }

        let session = self.consensus_manager.consensus().session().await;
        let selected_tip = session.async_get_headers_selected_tip().await;
        let mut header_hashes = vec![request.start_hash];

        if request.start_hash != selected_tip {
            let max_headers = request.limit.saturating_sub(1).try_into().unwrap_or(usize::MAX);
            let (chain_hashes, _) = session.async_get_hashes_between(request.start_hash, selected_tip, max_headers).await?;
            header_hashes.extend(chain_hashes);
        }

        if !request.is_ascending {
            header_hashes.reverse();
        }

        let mut headers = Vec::with_capacity(header_hashes.len());
        for hash in header_hashes {
            let header = session.async_get_header(hash).await?;
            headers.push((&*header).into());
        }

        Ok(GetHeadersResponse::new(headers))
    }

    async fn get_block_dag_info_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _: GetBlockDagInfoRequest,
    ) -> RpcResult<GetBlockDagInfoResponse> {
        let session = self.consensus_manager.consensus().unguarded_session();
        let (consensus_stats, tips, pruning_point, sink) =
            join!(session.async_get_stats(), session.async_get_tips(), session.async_pruning_point(), session.async_get_sink());
        Ok(GetBlockDagInfoResponse::new(
            self.config.net,
            consensus_stats.block_counts.block_count,
            consensus_stats.block_counts.header_count,
            tips,
            self.consensus_converter.get_difficulty_ratio(consensus_stats.virtual_stats.bits),
            consensus_stats.virtual_stats.past_median_time,
            session.get_virtual_parents().into_iter().collect::<Vec<_>>(),
            pruning_point,
            consensus_stats.virtual_stats.daa_score,
            sink,
        ))
    }

    async fn estimate_network_hashes_per_second_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: EstimateNetworkHashesPerSecondRequest,
    ) -> RpcResult<EstimateNetworkHashesPerSecondResponse> {
        if !self.config.unsafe_rpc && request.window_size > MAX_SAFE_WINDOW_SIZE {
            return Err(RpcError::WindowSizeExceedingMaximum(request.window_size, MAX_SAFE_WINDOW_SIZE));
        }
        if request.window_size as u64 > self.config.pruning_depth() {
            return Err(RpcError::WindowSizeExceedingPruningDepth(request.window_size, self.config.pruning_depth()));
        }

        let start_hash = request.start_hash;

        Ok(EstimateNetworkHashesPerSecondResponse::new(
            self.consensus_manager
                .consensus()
                .session()
                .await
                .async_estimate_network_hashes_per_second(start_hash, request.window_size as usize)
                .await?,
        ))
    }

    async fn add_peer_call(&self, _connection: Option<&DynRpcConnection>, request: AddPeerRequest) -> RpcResult<AddPeerResponse> {
        if !self.config.unsafe_rpc {
            warn!("AddPeer RPC command called while node in safe RPC mode -- ignoring.");
            return Err(RpcError::UnavailableInSafeMode);
        }
        let peer_address = request.peer_address.normalize(self.config.net.default_p2p_port());
        if let Some(connection_manager) = self.flow_context.connection_manager() {
            connection_manager.add_connection_request(peer_address.into(), request.is_permanent).await;
        } else {
            return Err(RpcError::NoConnectionManager);
        }
        Ok(AddPeerResponse {})
    }

    async fn get_peer_addresses_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _: GetPeerAddressesRequest,
    ) -> RpcResult<GetPeerAddressesResponse> {
        let address_manager = self.flow_context.address_manager.lock();
        Ok(GetPeerAddressesResponse::new(address_manager.get_all_addresses(), address_manager.get_all_banned_addresses()))
    }

    async fn ban_call(&self, _connection: Option<&DynRpcConnection>, request: BanRequest) -> RpcResult<BanResponse> {
        if !self.config.unsafe_rpc {
            warn!("Ban RPC command called while node in safe RPC mode -- ignoring.");
            return Err(RpcError::UnavailableInSafeMode);
        }
        if let Some(connection_manager) = self.flow_context.connection_manager() {
            let ip = request.ip.into();
            if connection_manager.ip_has_permanent_connection(ip).await {
                return Err(RpcError::IpHasPermanentConnection(request.ip));
            }
            connection_manager.ban(ip).await;
        } else {
            return Err(RpcError::NoConnectionManager);
        }
        Ok(BanResponse {})
    }

    async fn unban_call(&self, _connection: Option<&DynRpcConnection>, request: UnbanRequest) -> RpcResult<UnbanResponse> {
        if !self.config.unsafe_rpc {
            warn!("Unban RPC command called while node in safe RPC mode -- ignoring.");
            return Err(RpcError::UnavailableInSafeMode);
        }
        let mut address_manager = self.flow_context.address_manager.lock();
        if address_manager.is_banned(request.ip) {
            address_manager.unban(request.ip)
        } else {
            return Err(RpcError::IpIsNotBanned(request.ip));
        }
        Ok(UnbanResponse {})
    }

    async fn get_connected_peer_info_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _: GetConnectedPeerInfoRequest,
    ) -> RpcResult<GetConnectedPeerInfoResponse> {
        let peers = self.flow_context.hub().active_peers();
        let peer_info = self.protocol_converter.get_peers_info(&peers);
        Ok(GetConnectedPeerInfoResponse::new(peer_info))
    }

    async fn shutdown_call(&self, _connection: Option<&DynRpcConnection>, _: ShutdownRequest) -> RpcResult<ShutdownResponse> {
        if !self.config.unsafe_rpc {
            warn!("Shutdown RPC command called while node in safe RPC mode -- ignoring.");
            return Err(RpcError::UnavailableInSafeMode);
        }
        warn!("Shutdown RPC command was called, shutting down in 1 second...");

        // Signal the shutdown request
        self.core_shutdown_request.trigger.trigger();

        // Wait for a second before shutting down,
        // giving time for the response to be sent to the caller.
        let core = self.core.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            core.shutdown();
        });

        Ok(ShutdownResponse {})
    }

    async fn resolve_finality_conflict_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: ResolveFinalityConflictRequest,
    ) -> RpcResult<ResolveFinalityConflictResponse> {
        if !self.config.unsafe_rpc {
            warn!("ResolveFinalityConflict RPC command called while node in safe RPC mode -- ignoring.");
            return Err(RpcError::UnavailableInSafeMode);
        }

        let session = self.consensus_manager.consensus().session().await;
        let current_finality_point = session.async_finality_point().await;

        if request.finality_block_hash != current_finality_point {
            return Err(RpcError::General(format!(
                "requested finality block {} does not match the active finality point {}",
                request.finality_block_hash, current_finality_point
            )));
        }

        session.async_get_header(request.finality_block_hash).await?;

        if !self.flow_context.resolve_finality_conflict(request.finality_block_hash) {
            return Err(RpcError::General(format!(
                "a different finality conflict is currently pending; expected finality block {}",
                request.finality_block_hash
            )));
        }

        Ok(ResolveFinalityConflictResponse {})
    }

    async fn get_connections_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        req: GetConnectionsRequest,
    ) -> RpcResult<GetConnectionsResponse> {
        let clients = (self.wrpc_borsh_counters.active_connections.load(Ordering::Relaxed)
            + self.wrpc_json_counters.active_connections.load(Ordering::Relaxed)) as u32;
        let peers = self.flow_context.hub().active_peers_len() as u16;

        let profile_data = req.include_profile_data.then(|| {
            let CountersSnapshot { resident_set_size: memory_usage, cpu_usage, .. } = self.perf_monitor.snapshot();

            ConnectionsProfileData { cpu_usage: cpu_usage as f32, memory_usage }
        });

        Ok(GetConnectionsResponse { clients, peers, profile_data })
    }

    async fn get_metrics_call(&self, _connection: Option<&DynRpcConnection>, req: GetMetricsRequest) -> RpcResult<GetMetricsResponse> {
        let CountersSnapshot {
            resident_set_size,
            virtual_memory_size,
            core_num,
            cpu_usage,
            fd_num,
            disk_io_read_bytes,
            disk_io_write_bytes,
            disk_io_read_per_sec,
            disk_io_write_per_sec,
        } = self.perf_monitor.snapshot();

        let process_metrics = req.process_metrics.then_some(ProcessMetrics {
            resident_set_size,
            virtual_memory_size,
            core_num: core_num as u32,
            cpu_usage: cpu_usage as f32,
            fd_num: fd_num as u32,
            disk_io_read_bytes,
            disk_io_write_bytes,
            disk_io_read_per_sec: disk_io_read_per_sec as f32,
            disk_io_write_per_sec: disk_io_write_per_sec as f32,
        });

        let connection_metrics = req.connection_metrics.then(|| ConnectionMetrics {
            borsh_live_connections: self.wrpc_borsh_counters.active_connections.load(Ordering::Relaxed) as u32,
            borsh_connection_attempts: self.wrpc_borsh_counters.total_connections.load(Ordering::Relaxed) as u64,
            borsh_handshake_failures: self.wrpc_borsh_counters.handshake_failures.load(Ordering::Relaxed) as u64,
            json_live_connections: self.wrpc_json_counters.active_connections.load(Ordering::Relaxed) as u32,
            json_connection_attempts: self.wrpc_json_counters.total_connections.load(Ordering::Relaxed) as u64,
            json_handshake_failures: self.wrpc_json_counters.handshake_failures.load(Ordering::Relaxed) as u64,

            active_peers: self.flow_context.hub().active_peers_len() as u32,
        });

        let bandwidth_metrics = req.bandwidth_metrics.then(|| BandwidthMetrics {
            borsh_bytes_tx: self.wrpc_borsh_counters.tx_bytes.load(Ordering::Relaxed) as u64,
            borsh_bytes_rx: self.wrpc_borsh_counters.rx_bytes.load(Ordering::Relaxed) as u64,
            json_bytes_tx: self.wrpc_json_counters.tx_bytes.load(Ordering::Relaxed) as u64,
            json_bytes_rx: self.wrpc_json_counters.rx_bytes.load(Ordering::Relaxed) as u64,
            p2p_bytes_tx: self.p2p_tower_counters.bytes_tx.load(Ordering::Relaxed) as u64,
            p2p_bytes_rx: self.p2p_tower_counters.bytes_rx.load(Ordering::Relaxed) as u64,
            grpc_bytes_tx: self.grpc_tower_counters.bytes_tx.load(Ordering::Relaxed) as u64,
            grpc_bytes_rx: self.grpc_tower_counters.bytes_rx.load(Ordering::Relaxed) as u64,
        });

        let consensus_metrics = if req.consensus_metrics {
            let consensus_stats = self.consensus_manager.consensus().unguarded_session().async_get_stats().await;
            let processing_counters = self.processing_counters.snapshot();

            Some(ConsensusMetrics {
                node_blocks_submitted_count: processing_counters.blocks_submitted,
                node_headers_processed_count: processing_counters.header_counts,
                node_dependencies_processed_count: processing_counters.dep_counts,
                node_bodies_processed_count: processing_counters.body_counts,
                node_transactions_processed_count: processing_counters.txs_counts,
                node_chain_blocks_processed_count: processing_counters.chain_block_counts,
                node_mass_processed_count: processing_counters.mass_counts,
                // ---
                node_database_blocks_count: consensus_stats.block_counts.block_count,
                node_database_headers_count: consensus_stats.block_counts.header_count,
                // ---
                network_mempool_size: self.mining_manager.transaction_count_sample(TransactionQuery::TransactionsOnly),
                network_tip_hashes_count: consensus_stats.num_tips.try_into().unwrap_or(u32::MAX),
                network_difficulty: self.consensus_converter.get_difficulty_ratio(consensus_stats.virtual_stats.bits),
                network_past_median_time: consensus_stats.virtual_stats.past_median_time,
                network_virtual_parent_hashes_count: consensus_stats.virtual_stats.num_parents,
                network_virtual_daa_score: consensus_stats.virtual_stats.daa_score,
            })
        } else {
            None
        };

        let storage_metrics = req.storage_metrics.then_some(StorageMetrics { storage_size_bytes: 0 });

        let custom_metrics: Option<HashMap<String, CustomMetricValue>> = None;

        let server_time = unix_now();

        let response = GetMetricsResponse {
            server_time,
            process_metrics,
            connection_metrics,
            bandwidth_metrics,
            consensus_metrics,
            storage_metrics,
            custom_metrics,
        };

        Ok(response)
    }

    async fn get_system_info_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetSystemInfoRequest,
    ) -> RpcResult<GetSystemInfoResponse> {
        let response = GetSystemInfoResponse {
            version: self.system_info.version.clone(),
            system_id: self.system_info.system_id.clone(),
            git_hash: self.system_info.git_short_hash.clone(),
            cpu_physical_cores: self.system_info.cpu_physical_cores,
            total_memory: self.system_info.total_memory,
            fd_limit: self.system_info.fd_limit,
            proxy_socket_limit_per_cpu_core: self.system_info.proxy_socket_limit_per_cpu_core,
        };

        Ok(response)
    }

    async fn get_server_info_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetServerInfoRequest,
    ) -> RpcResult<GetServerInfoResponse> {
        let session = self.consensus_manager.consensus().unguarded_session();
        let sink_daa_score_timestamp = session.async_get_sink_daa_score_timestamp().await;
        let is_synced: bool = self.mining_rule_engine.is_sink_recent_and_connected(sink_daa_score_timestamp);
        let virtual_daa_score = session.get_virtual_daa_score();

        Ok(GetServerInfoResponse {
            rpc_api_version: RPC_API_VERSION,
            rpc_api_revision: RPC_API_REVISION,
            server_version: version().to_string(),
            network_id: self.config.net,
            has_cell_index: self.config.cellindex,
            is_synced,
            virtual_daa_score,
        })
    }

    async fn get_sync_status_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        _request: GetSyncStatusRequest,
    ) -> RpcResult<GetSyncStatusResponse> {
        let sink_daa_score_timestamp =
            self.consensus_manager.consensus().unguarded_session().async_get_sink_daa_score_timestamp().await;
        let is_synced: bool = self.mining_rule_engine.is_sink_recent_and_connected(sink_daa_score_timestamp);
        Ok(GetSyncStatusResponse { is_synced })
    }

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Notification API

    /// Register a new listener and returns an id identifying it.
    #[allow(deprecated)]
    fn register_new_listener(&self, connection: ChannelConnection) -> ListenerId {
        self.notifier.register_new_listener(connection, ListenerLifespan::Dynamic)
    }

    /// Unregister an existing listener.
    ///
    /// Stop all notifications for this listener, unregister the id and its associated connection.
    async fn unregister_listener(&self, id: ListenerId) -> RpcResult<()> {
        self.notifier.unregister_listener(id)?;
        Ok(())
    }

    /// Start sending notifications of some type to a listener.
    async fn start_notify(&self, id: ListenerId, scope: Scope) -> RpcResult<()> {
        match scope {
            Scope::CellsChanged(ref cells_changed_scope) if !self.config.unsafe_rpc && cells_changed_scope.addresses.is_empty() => {
                // The subscription to blanket CellsChanged notifications is restricted to unsafe mode only
                // since the notifications yielded are highly resource intensive.
                //
                // Please note that unsubscribing to blanket CellsChanged is always allowed and cancels
                // the whole subscription no matter if blanket or targeting specified addresses.

                warn!("RPC subscription to blanket CellsChanged called while node in safe RPC mode -- ignoring.");
                Err(RpcError::UnavailableInSafeMode)
            }
            _ => {
                self.notifier.clone().start_notify(id, scope).await?;
                Ok(())
            }
        }
    }

    /// Stop sending notifications of some type to a listener.
    async fn stop_notify(&self, id: ListenerId, scope: Scope) -> RpcResult<()> {
        self.notifier.clone().stop_notify(id, scope).await?;
        Ok(())
    }

    async fn subscribe_notifications(
        &self,
        request: SubscribeNotificationsRequest,
    ) -> RpcResult<SubscribeNotificationsResponse> {
        // Create an internal channel and register it as a new dynamic listener.
        let channel = Channel::default();
        let connection = ChannelConnection::new(
            "rpc-subscribe",
            channel.sender(),
            spora_notify::connection::ChannelType::Closable,
        );
        #[allow(deprecated)]
        let listener_id = self.register_new_listener(connection);

        // Activate the requested scope on the newly created listener.
        self.notifier.clone().start_notify(listener_id, request.scope).await?;

        Ok(SubscribeNotificationsResponse::new(listener_id))
    }

    async fn unsubscribe_notifications(
        &self,
        request: UnsubscribeNotificationsRequest,
    ) -> RpcResult<UnsubscribeNotificationsResponse> {
        self.notifier.unregister_listener(request.subscription_id)?;
        Ok(UnsubscribeNotificationsResponse {})
    }
}

// It might be necessary to opt this out in the context of wasm32

impl AsyncService for RpcCoreService {
    fn ident(self: Arc<Self>) -> &'static str {
        Self::IDENT
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        trace!("{} starting", Self::IDENT);
        let service = self.clone();

        // Prepare a shutdown signal receiver
        let shutdown_signal = self.shutdown.listener.clone();

        // Launch the service and wait for a shutdown signal
        Box::pin(async move {
            service.clone().start_impl();
            shutdown_signal.await;
            match service.join().await {
                Ok(_) => Ok(()),
                Err(err) => {
                    warn!("Error while stopping {}: {}", Self::IDENT, err);
                    Err(AsyncServiceError::Service(err.to_string()))
                }
            }
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", Self::IDENT);
        self.shutdown.trigger.trigger();
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            trace!("{} stopped", Self::IDENT);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_cell_return_address;
    use spora_addresses::{Address, Prefix};
    use spora_consensus_core::cell_metadata::CellMetadata;
    use spora_consensus_core::tx::{pay_to_address_lock_script, CellInput, CellTx, OutPoint, ResolvedCellTransaction};
    use spora_hashes::Hash;

    #[test]
    fn resolve_cell_return_address_uses_first_resolved_input_lock_script() {
        let address = Address::new_std_single(Prefix::Simnet, &[0x11; 32]).expect("test address");
        let lock_script = pay_to_address_lock_script(&address);
        let tx = CellTx {
            version: 0xC001,
            inputs: vec![CellInput::new(OutPoint::new([0x22; 32], 0), 0)],
            cell_deps: vec![],
            header_deps: vec![],
            outputs: vec![],
            outputs_data: vec![],
            witnesses: vec![],
        };
        let resolved = ResolvedCellTransaction::new(
            tx,
            vec![CellMetadata {
                out_point: OutPoint::new([0x22; 32], 0),
                capacity: 42,
                data_bytes: 0,
                lock_hash: lock_script.hash(),
                type_hash: None,
                data_hash: [0; 32],
                block_daa_score: 1,
                is_cellbase: false,
                block_hash: Hash::from_bytes([0x33; 32]),
                lock_code_hash: Some(lock_script.code_hash),
                type_code_hash: None,
                lock_script: Some(lock_script),
                type_script: None,
                data: Some(vec![]),
            }],
        );

        let return_address = resolve_cell_return_address(&resolved, Prefix::Simnet).expect("return address");

        assert_eq!(return_address, address);
    }
}
