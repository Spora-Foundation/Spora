use std::{str::FromStr, sync::Arc, time::Duration};

use crate::common::{client_notify::ChannelNotify, daemon::Daemon};
use futures_util::future::try_join_all;
use spora_addresses::{Address, Prefix, Version};
use spora_consensus::params::SIMNET_GENESIS;
use spora_consensus_core::{constants::MAX_SAU, header::Header, subnets::SubnetworkId, tx::CellTx};
use spora_core::{assert_match, info};
use spora_grpc_core::ops::SporadPayloadOps;
use spora_hashes::Hash;
use spora_notify::{
    connection::{ChannelConnection, ChannelType},
    scope::{
        BlockAddedScope, CellsChangedScope, FinalityConflictScope, NewBlockTemplateScope, PruningPointCellSetOverrideScope, Scope,
        SinkBlueScoreChangedScope, VirtualChainChangedScope, VirtualDaaScoreChangedScope,
    },
};
use spora_rpc_core::{api::rpc::RpcApi, model::*, Notification};
use spora_utils::{fd_budget, networking::ContextualNetAddress};
use sporad_lib::args::Args;
use tokio::task::JoinHandle;

fn test_address(seed: u8) -> Address {
    Address::new(Prefix::Simnet, Version::PubKey, &[seed; 32]).expect("test address must be valid")
}

#[macro_export]
macro_rules! tst {
    ($op:ident, $test_body:block) => {
        tokio::spawn(async move {
            info!("Testing  {:?}", $op);
            $test_body
        })
    };

    ($op:ident, $reason:literal) => {
        tokio::spawn(async move {
            info!("Skipping {:?} --- {}", $op, $reason);
        })
    };
}

/// `cargo test --release --package spora-testing-integration --lib -- rpc_tests::sanity_test`
#[tokio::test]
async fn sanity_test() {
    spora_core::log::try_init_logger("info");
    // As we log the panic, we want to set it up after the logger
    spora_core::panic::configure_panic();

    let args = Args {
        simnet: true,
        disable_upnp: true, // UPnP registration might take some time and is not needed for this test
        enable_unsynced_mining: true,
        block_template_cache_lifetime: Some(0),
        cellindex: true,
        unsafe_rpc: true,
        ..Default::default()
    };

    let fd_total_budget = fd_budget::limit();
    let mut daemon = Daemon::new_random_with_args(args, fd_total_budget);
    let client = daemon.start().await;
    let (sender, _) = async_channel::unbounded();
    let connection = ChannelConnection::new("test", sender, ChannelType::Closable);
    let listener_id = client.register_new_listener(connection);
    let mut tasks: Vec<JoinHandle<()>> = Vec::new();

    // The intent of this for/match design (emphasizing the absence of an arm with fallback pattern in the match)
    // is to force any implementor of a new RpcApi method to add a matching arm here and to strongly incentivize
    // the adding of an actual sanity test of said new method.
    for op in SporadPayloadOps::iter() {
        let network_id = daemon.network;
        let task: JoinHandle<()> = match op {
            SporadPayloadOps::SubmitBlock => {
                let rpc_client = client.clone();
                tst!(op, {
                    // Register to basic virtual events in order to keep track of block submission
                    let (sender, event_receiver) = async_channel::unbounded();
                    rpc_client.start(Some(Arc::new(ChannelNotify::new(sender)))).await;
                    rpc_client
                        .start_notify(Default::default(), Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {}))
                        .await
                        .unwrap();

                    // Before submitting a first block, the sink is the genesis,
                    let response = rpc_client.get_sink_call(None, GetSinkRequest {}).await.unwrap();
                    assert_eq!(response.sink, SIMNET_GENESIS.hash);
                    let response = rpc_client.get_sink_blue_score_call(None, GetSinkBlueScoreRequest {}).await.unwrap();
                    assert_eq!(response.blue_score, 0);

                    // the block count is 0
                    let response = rpc_client.get_block_count_call(None, GetBlockCountRequest {}).await.unwrap();
                    assert_eq!(response.block_count, 0);

                    // and the virtual chain is the genesis only
                    let response = rpc_client
                        .get_virtual_chain_from_block_call(
                            None,
                            GetVirtualChainFromBlockRequest {
                                start_hash: SIMNET_GENESIS.hash,
                                include_accepted_transaction_ids: false,
                                min_confirmation_count: None,
                            },
                        )
                        .await
                        .unwrap();
                    assert!(response.added_chain_block_hashes.is_empty());
                    assert!(response.removed_chain_block_hashes.is_empty());

                    // Get a block template
                    let GetBlockTemplateResponse { block, is_synced } = rpc_client
                        .get_block_template_call(
                            None,
                            GetBlockTemplateRequest { pay_address: test_address(0), extra_data: Vec::new() },
                        )
                        .await
                        .unwrap();
                    assert!(!is_synced);

                    // Compute the expected block hash for the received block
                    let header: Header = (&block.header).into();
                    let block_hash = header.hash;

                    // Submit the template (no mining, in simnet PoW is skipped)
                    let response = rpc_client.submit_block(block.clone(), false).await.unwrap();
                    if block.transactions.is_empty() {
                        assert_eq!(response.report, SubmitBlockReport::Reject(SubmitBlockRejectReason::BlockInvalid));
                        info!("Skipping submit-block success assertions because the template exposed no transactions");
                        return;
                    }
                    assert_eq!(response.report, SubmitBlockReport::Success);

                    // Wait for virtual event indicating the block was processed and entered past(virtual)
                    while let Ok(notification) = match tokio::time::timeout(Duration::from_secs(1), event_receiver.recv()).await {
                        Ok(res) => res,
                        Err(elapsed) => panic!("expected virtual event before {}", elapsed),
                    } {
                        match notification {
                            Notification::VirtualDaaScoreChanged(msg) if msg.virtual_daa_score == 1 => {
                                break;
                            }
                            Notification::VirtualDaaScoreChanged(msg) if msg.virtual_daa_score > 1 => {
                                panic!("DAA score too high for number of submitted blocks")
                            }
                            Notification::VirtualDaaScoreChanged(_) => {}
                            _ => {}
                        }
                    }

                    // After submitting a first block, the sink is the submitted block,
                    let response = rpc_client.get_sink_call(None, GetSinkRequest {}).await.unwrap();
                    assert_eq!(response.sink, block_hash);

                    // the block count is 1
                    let response = rpc_client.get_block_count_call(None, GetBlockCountRequest {}).await.unwrap();
                    assert_eq!(response.block_count, 1);

                    // and the virtual chain from genesis contains the added block
                    let response = rpc_client
                        .get_virtual_chain_from_block_call(
                            None,
                            GetVirtualChainFromBlockRequest {
                                start_hash: SIMNET_GENESIS.hash,
                                include_accepted_transaction_ids: false,
                                min_confirmation_count: None,
                            },
                        )
                        .await
                        .unwrap();
                    assert!(response.added_chain_block_hashes.contains(&block_hash));
                    assert!(response.removed_chain_block_hashes.is_empty());

                    let result =
                        rpc_client.get_current_block_color_call(None, GetCurrentBlockColorRequest { hash: SIMNET_GENESIS.hash }).await;

                    // Genesis was merged by the new sink, so we're expecting a positive blueness response
                    assert_match!(result, Ok(GetCurrentBlockColorResponse { blue: true }));

                    // The new sink has no merging block yet, so we expect a MergerNotFound error
                    let result = rpc_client.get_current_block_color_call(None, GetCurrentBlockColorRequest { hash: block_hash }).await;
                    assert!(result.is_err());

                    // Non-existing blocks should return an error
                    let result = rpc_client.get_current_block_color_call(None, GetCurrentBlockColorRequest { hash: 999.into() }).await;
                    assert!(result.is_err());
                })
            }

            SporadPayloadOps::GetBlockTemplate => {
                tst!(op, "see SubmitBlock")
            }

            SporadPayloadOps::GetCurrentBlockColor => {
                tst!(op, "see SubmitBlock")
            }

            SporadPayloadOps::GetCurrentNetwork => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response = rpc_client.get_current_network_call(None, GetCurrentNetworkRequest {}).await.unwrap();
                    assert_eq!(response.network, network_id.network_type);
                })
            }

            SporadPayloadOps::GetBlock => {
                let rpc_client = client.clone();
                tst!(op, {
                    let result =
                        rpc_client.get_block_call(None, GetBlockRequest { hash: 0.into(), include_transactions: false }).await;
                    assert!(result.is_err());

                    let response = rpc_client
                        .get_block_call(None, GetBlockRequest { hash: SIMNET_GENESIS.hash, include_transactions: false })
                        .await
                        .unwrap();
                    assert_eq!(response.block.header.hash, SIMNET_GENESIS.hash);
                })
            }

            SporadPayloadOps::GetBlockStatus => {
                let rpc_client = client.clone();
                tst!(op, {
                    let result = rpc_client.get_block_status_call(None, GetBlockStatusRequest { hash: 0.into() }).await;
                    assert!(result.is_err());

                    let response =
                        rpc_client.get_block_status_call(None, GetBlockStatusRequest { hash: SIMNET_GENESIS.hash }).await.unwrap();
                    assert_ne!(response.status.status, 0, "genesis should not be reported as invalid");
                })
            }

            SporadPayloadOps::GetTransaction => {
                let rpc_client = client.clone();
                tst!(op, {
                    let result = rpc_client.get_transaction_call(None, GetTransactionRequest { hash: 0.into() }).await;
                    assert!(result.is_err());

                    let template = rpc_client
                        .get_block_template_call(
                            None,
                            GetBlockTemplateRequest { pay_address: test_address(2), extra_data: Vec::new() },
                        )
                        .await
                        .unwrap();
                    let maybe_txid = template.block.transactions.first().cloned().map(|first_tx| {
                        let transaction: Transaction = first_tx.try_into().expect("rpc transaction must convert");
                        transaction.id()
                    });

                    if let Some(txid) = maybe_txid {
                        let report = rpc_client.submit_block(template.block, true).await.unwrap();
                        assert!(report.report.is_success());
                        let response = rpc_client.get_transaction_call(None, GetTransactionRequest { hash: txid }).await.unwrap();
                        assert_eq!(response.transaction.verbose_data.as_ref().map(|v| v.transaction_id), Some(txid));
                    } else {
                        info!("Skipping accepted transaction lookup because the template exposed no transactions");
                    }
                })
            }

            SporadPayloadOps::GetBlocks => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response = rpc_client
                        .get_blocks_call(None, GetBlocksRequest { include_blocks: true, include_transactions: false, low_hash: None })
                        .await
                        .unwrap();
                    assert_eq!(response.blocks.len(), 1, "genesis block should be returned");
                    assert_eq!(response.blocks[0].header.hash, SIMNET_GENESIS.hash);
                    assert_eq!(response.block_hashes[0], SIMNET_GENESIS.hash);
                })
            }

            SporadPayloadOps::GetInfo => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response = rpc_client.get_info_call(None, GetInfoRequest {}).await.unwrap();
                    assert_eq!(response.server_version, spora_core::sporad_env::version().to_string());
                    assert_eq!(response.mempool_size, 0);
                    assert!(response.is_cell_indexed);
                    assert!(response.has_message_id);
                    assert!(response.has_notify_command);
                })
            }

            SporadPayloadOps::Shutdown => {
                // This test is purposely left blank since shutdown can only be tested after all other
                // tests completed
                tst!(op, "must be run in the end")
            }

            SporadPayloadOps::GetPeerAddresses => {
                tst!(op, "see AddPeer, Ban")
            }

            SporadPayloadOps::GetSink => {
                tst!(op, "see SubmitBlock")
            }

            SporadPayloadOps::GetMempoolEntry => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response_result = rpc_client
                        .get_mempool_entry_call(
                            None,
                            GetMempoolEntryRequest {
                                transaction_id: 0.into(),
                                include_orphan_pool: true,
                                filter_transaction_pool: false,
                            },
                        )
                        .await;
                    // Test Get Mempool Entry:
                    // TODO: Fix by adding actual mempool entries this can get because otherwise it errors out
                    assert!(response_result.is_err());
                })
            }

            SporadPayloadOps::GetMempoolEntries => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response = rpc_client
                        .get_mempool_entries_call(
                            None,
                            GetMempoolEntriesRequest { include_orphan_pool: true, filter_transaction_pool: false },
                        )
                        .await
                        .unwrap();
                    assert!(response.mempool_entries.is_empty());
                })
            }

            SporadPayloadOps::GetConnectedPeerInfo => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response = rpc_client.get_connected_peer_info_call(None, GetConnectedPeerInfoRequest {}).await.unwrap();
                    assert!(response.peer_info.is_empty());
                })
            }

            SporadPayloadOps::AddPeer => {
                let rpc_client = client.clone();
                tst!(op, {
                    let peer_address = ContextualNetAddress::from_str("1.2.3.4").unwrap();
                    let _ = rpc_client.add_peer_call(None, AddPeerRequest { peer_address, is_permanent: true }).await.unwrap();

                    // Add peer only adds the IP to a connection request. It will only be added to known_addresses if it
                    // actually can be connected to. So in this test we can't expect it to be added unless we set up an
                    // actual peer.
                    let response = rpc_client.get_peer_addresses_call(None, GetPeerAddressesRequest {}).await.unwrap();
                    assert!(response.known_addresses.is_empty());
                })
            }

            SporadPayloadOps::Ban => {
                let rpc_client = client.clone();
                tst!(op, {
                    let peer_address = ContextualNetAddress::from_str("5.6.7.8").unwrap();
                    let ip = peer_address.normalize(1).ip;

                    let _ = rpc_client.add_peer_call(None, AddPeerRequest { peer_address, is_permanent: false }).await.unwrap();
                    let _ = rpc_client.ban_call(None, BanRequest { ip }).await.unwrap();

                    let response = rpc_client.get_peer_addresses_call(None, GetPeerAddressesRequest {}).await.unwrap();
                    assert!(response.banned_addresses.contains(&ip));

                    let _ = rpc_client.unban_call(None, UnbanRequest { ip }).await.unwrap();
                    let response = rpc_client.get_peer_addresses_call(None, GetPeerAddressesRequest {}).await.unwrap();
                    assert!(!response.banned_addresses.contains(&ip));
                })
            }

            SporadPayloadOps::Unban => {
                tst!(op, "see Ban")
            }

            SporadPayloadOps::SubmitTransaction => {
                let rpc_client = client.clone();
                tst!(op, {
                    // Build an erroneous transaction...
                    let transaction = Transaction::new_native(0, vec![], vec![], vec![]);
                    let result = rpc_client.submit_transaction((&transaction).into(), false).await;
                    // ...that gets rejected by the consensus
                    assert!(result.is_err());
                })
            }

            SporadPayloadOps::SubmitTransactionReplacement => {
                let rpc_client = client.clone();
                tst!(op, {
                    // Build an erroneous transaction...
                    let transaction = Transaction::new_native(0, vec![], vec![], vec![]);
                    let result = rpc_client.submit_transaction_replacement((&transaction).into()).await;
                    // ...that gets rejected by the consensus
                    assert!(result.is_err());
                })
            }

            SporadPayloadOps::GetSubnetwork => {
                let rpc_client = client.clone();
                tst!(op, {
                    let result = rpc_client
                        .get_subnetwork_call(
                            None,
                            GetSubnetworkRequest { subnetwork_id: spora_rpc_core::model::RpcSubnetworkId::from_byte(0) },
                        )
                        .await;

                    // Err because it's currently unimplemented
                    assert!(result.is_err());
                })
            }

            SporadPayloadOps::GetVirtualChainFromBlock => {
                tst!(op, "see SubmitBlock")
            }

            SporadPayloadOps::GetBlockCount => {
                tst!(op, "see SubmitBlock")
            }

            SporadPayloadOps::GetBlockDagInfo => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response = rpc_client.get_block_dag_info_call(None, GetBlockDagInfoRequest {}).await.unwrap();
                    assert_eq!(response.network, network_id);
                })
            }

            SporadPayloadOps::ResolveFinalityConflict => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response_result = rpc_client
                        .resolve_finality_conflict_call(
                            None,
                            ResolveFinalityConflictRequest { finality_block_hash: Hash::from_bytes([0; 32]) },
                        )
                        .await;

                    // Err because it's currently unimplemented
                    assert!(response_result.is_err());
                })
            }

            SporadPayloadOps::GetHeader => {
                let rpc_client = client.clone();
                tst!(op, {
                    let result = rpc_client.get_header_call(None, GetHeaderRequest { hash: 0.into() }).await;
                    assert!(result.is_err());

                    let response = rpc_client.get_header_call(None, GetHeaderRequest { hash: SIMNET_GENESIS.hash }).await.unwrap();
                    assert_eq!(response.header.hash, SIMNET_GENESIS.hash);
                })
            }

            SporadPayloadOps::GetHeaders => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response_result = rpc_client
                        .get_headers_call(None, GetHeadersRequest { start_hash: SIMNET_GENESIS.hash, limit: 1, is_ascending: true })
                        .await;

                    // Err because it's currently unimplemented
                    assert!(response_result.is_err());
                })
            }

            SporadPayloadOps::GetCellsByAddress => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response = rpc_client
                        .get_cells_by_address_call(None, GetCellsByAddressRequest { address: test_address(0), start: 0, limit: 100 })
                        .await
                        .unwrap();
                    assert!(response.entries.is_empty());
                    assert_eq!(response.total, 0);
                })
            }

            SporadPayloadOps::GetCellsByAddresses => {
                let rpc_client = client.clone();
                tst!(op, {
                    let addresses = vec![test_address(0)];
                    let response =
                        rpc_client.get_cells_by_addresses_call(None, GetCellsByAddressesRequest { addresses }).await.unwrap();
                    assert!(response.entries.is_empty());
                })
            }

            SporadPayloadOps::GetBalanceByAddress => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response = rpc_client
                        .get_balance_by_address_call(None, GetBalanceByAddressRequest { address: test_address(0) })
                        .await
                        .unwrap();
                    assert_eq!(response.balance, 0);
                })
            }

            SporadPayloadOps::GetBalancesByAddresses => {
                let rpc_client = client.clone();
                tst!(op, {
                    let addresses = vec![test_address(1)];
                    let response = rpc_client
                        .get_balances_by_addresses_call(None, GetBalancesByAddressesRequest::new(addresses.clone()))
                        .await
                        .unwrap();
                    assert_eq!(response.entries.len(), 1);
                    assert_eq!(response.entries[0].address, addresses[0]);
                    assert_eq!(response.entries[0].balance, Some(0));

                    let response =
                        rpc_client.get_balances_by_addresses_call(None, GetBalancesByAddressesRequest::new(vec![])).await.unwrap();
                    assert!(response.entries.is_empty());
                })
            }

            SporadPayloadOps::GetSinkBlueScore => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response = rpc_client.get_sink_blue_score_call(None, GetSinkBlueScoreRequest {}).await.unwrap();
                    // A concurrent test may have added a single block so the blue score can be either 0 or 1
                    assert!(response.blue_score < 2);
                })
            }

            SporadPayloadOps::EstimateNetworkHashesPerSecond => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response_result = rpc_client
                        .estimate_network_hashes_per_second_call(
                            None,
                            EstimateNetworkHashesPerSecondRequest { window_size: 1000, start_hash: None },
                        )
                        .await;
                    // The current DAA window is almost empty so an error is expected
                    assert!(response_result.is_err());
                })
            }

            SporadPayloadOps::GetMempoolEntriesByAddresses => {
                let rpc_client = client.clone();
                tst!(op, {
                    let addresses = vec![test_address(0)];
                    let response = rpc_client
                        .get_mempool_entries_by_addresses_call(
                            None,
                            GetMempoolEntriesByAddressesRequest::new(addresses.clone(), true, false),
                        )
                        .await
                        .unwrap();
                    assert_eq!(response.entries.len(), 1);
                    assert_eq!(response.entries[0].address, addresses[0]);
                    assert!(response.entries[0].receiving.is_empty());
                    assert!(response.entries[0].sending.is_empty());
                })
            }

            SporadPayloadOps::GetCoinSupply => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response = rpc_client.get_coin_supply_call(None, GetCoinSupplyRequest {}).await.unwrap();
                    assert_eq!(response.circulating_sau, 0);
                    assert_eq!(response.max_sau, MAX_SAU);
                })
            }

            SporadPayloadOps::Ping => {
                let rpc_client = client.clone();
                tst!(op, {
                    let _ = rpc_client.ping_call(None, PingRequest {}).await.unwrap();
                })
            }

            SporadPayloadOps::GetConnections => {
                let rpc_client = client.clone();
                tst!(op, {
                    let _ = rpc_client.get_connections_call(None, GetConnectionsRequest { include_profile_data: true }).await.unwrap();
                })
            }

            SporadPayloadOps::GetMetrics => {
                let rpc_client = client.clone();
                tst!(op, {
                    let get_metrics_call_response = rpc_client
                        .get_metrics_call(
                            None,
                            GetMetricsRequest {
                                consensus_metrics: true,
                                connection_metrics: true,
                                bandwidth_metrics: true,
                                process_metrics: true,
                                storage_metrics: true,
                                custom_metrics: true,
                            },
                        )
                        .await
                        .unwrap();
                    assert!(get_metrics_call_response.process_metrics.is_some());
                    assert!(get_metrics_call_response.consensus_metrics.is_some());

                    let get_metrics_call_response = rpc_client
                        .get_metrics_call(
                            None,
                            GetMetricsRequest {
                                consensus_metrics: false,
                                connection_metrics: true,
                                bandwidth_metrics: true,
                                process_metrics: true,
                                storage_metrics: true,
                                custom_metrics: true,
                            },
                        )
                        .await
                        .unwrap();
                    assert!(get_metrics_call_response.process_metrics.is_some());
                    assert!(get_metrics_call_response.consensus_metrics.is_none());

                    let get_metrics_call_response = rpc_client
                        .get_metrics_call(
                            None,
                            GetMetricsRequest {
                                consensus_metrics: true,
                                connection_metrics: true,
                                bandwidth_metrics: false,
                                process_metrics: false,
                                storage_metrics: false,
                                custom_metrics: true,
                            },
                        )
                        .await
                        .unwrap();
                    assert!(get_metrics_call_response.process_metrics.is_none());
                    assert!(get_metrics_call_response.consensus_metrics.is_some());

                    let get_metrics_call_response = rpc_client
                        .get_metrics_call(
                            None,
                            GetMetricsRequest {
                                consensus_metrics: false,
                                connection_metrics: true,
                                bandwidth_metrics: false,
                                process_metrics: false,
                                storage_metrics: false,
                                custom_metrics: true,
                            },
                        )
                        .await
                        .unwrap();
                    assert!(get_metrics_call_response.process_metrics.is_none());
                    assert!(get_metrics_call_response.consensus_metrics.is_none());
                })
            }

            SporadPayloadOps::GetSystemInfo => {
                let rpc_client = client.clone();
                tst!(op, {
                    let _response = rpc_client.get_system_info_call(None, GetSystemInfoRequest {}).await.unwrap();
                })
            }

            SporadPayloadOps::GetServerInfo => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response = rpc_client.get_server_info_call(None, GetServerInfoRequest {}).await.unwrap();
                    assert!(response.has_cell_index); // we set cellindex above
                    assert_eq!(response.network_id, network_id);
                })
            }

            SporadPayloadOps::GetSyncStatus => {
                let rpc_client = client.clone();
                tst!(op, {
                    let _ = rpc_client.get_sync_status_call(None, GetSyncStatusRequest {}).await.unwrap();
                })
            }

            SporadPayloadOps::GetDaaScoreTimestampEstimate => {
                let rpc_client = client.clone();
                tst!(op, {
                    let results = rpc_client
                        .get_daa_score_timestamp_estimate_call(
                            None,
                            GetDaaScoreTimestampEstimateRequest { daa_scores: vec![0, 500, 2000, u64::MAX] },
                        )
                        .await
                        .unwrap();

                    for timestamp in results.timestamps.iter() {
                        info!("Timestamp estimate is {}", timestamp);
                    }

                    let results = rpc_client
                        .get_daa_score_timestamp_estimate_call(None, GetDaaScoreTimestampEstimateRequest { daa_scores: vec![] })
                        .await
                        .unwrap();

                    for timestamp in results.timestamps.iter() {
                        info!("Timestamp estimate is {}", timestamp);
                    }
                })
            }

            SporadPayloadOps::GetFeeEstimate => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response = rpc_client.get_fee_estimate().await.unwrap();
                    info!("{:?}", response.priority_bucket);
                    assert!(!response.normal_buckets.is_empty());
                    assert!(!response.low_buckets.is_empty());
                    for bucket in response.ordered_buckets() {
                        info!("{:?}", bucket);
                    }
                })
            }

            SporadPayloadOps::GetFeeEstimateExperimental => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response = rpc_client.get_fee_estimate_experimental(true).await.unwrap();
                    assert!(!response.estimate.normal_buckets.is_empty());
                    assert!(!response.estimate.low_buckets.is_empty());
                    for bucket in response.estimate.ordered_buckets() {
                        info!("{:?}", bucket);
                    }
                    assert!(response.verbose.is_some());
                    info!("{:?}", response.verbose);
                })
            }

            SporadPayloadOps::GetCellReturnAddress => {
                let rpc_client = client.clone();
                tst!(op, {
                    let results = rpc_client.get_cell_return_address(RpcHash::from_bytes([0; 32]), 1000).await;

                    assert!(results.is_err_and(|err| {
                        match err {
                            spora_rpc_core::RpcError::General(msg) => {
                                info!("Expected error message: {}", msg);
                                true
                            }
                            _ => false,
                        }
                    }));
                })
            }

            SporadPayloadOps::NotifyBlockAdded => {
                let rpc_client = client.clone();
                let id = listener_id;
                tst!(op, {
                    rpc_client.start_notify(id, BlockAddedScope {}.into()).await.unwrap();
                })
            }

            SporadPayloadOps::NotifyNewBlockTemplate => {
                let rpc_client = client.clone();
                let id = listener_id;
                tst!(op, {
                    rpc_client.start_notify(id, NewBlockTemplateScope {}.into()).await.unwrap();
                })
            }

            SporadPayloadOps::NotifyFinalityConflict => {
                let rpc_client = client.clone();
                let id = listener_id;
                tst!(op, {
                    rpc_client.start_notify(id, FinalityConflictScope {}.into()).await.unwrap();
                })
            }
            SporadPayloadOps::NotifyCellsChanged => {
                let rpc_client = client.clone();
                let id = listener_id;
                tst!(op, {
                    rpc_client.start_notify(id, CellsChangedScope::new(vec![]).into()).await.unwrap();
                })
            }
            SporadPayloadOps::NotifySinkBlueScoreChanged => {
                let rpc_client = client.clone();
                let id = listener_id;
                tst!(op, {
                    rpc_client.start_notify(id, SinkBlueScoreChangedScope {}.into()).await.unwrap();
                })
            }
            SporadPayloadOps::NotifyPruningPointCellSetOverride => {
                let rpc_client = client.clone();
                let id = listener_id;
                tst!(op, {
                    rpc_client.start_notify(id, PruningPointCellSetOverrideScope {}.into()).await.unwrap();
                })
            }
            SporadPayloadOps::NotifyVirtualDaaScoreChanged => {
                let rpc_client = client.clone();
                let id = listener_id;
                tst!(op, {
                    rpc_client.start_notify(id, VirtualDaaScoreChangedScope {}.into()).await.unwrap();
                })
            }
            SporadPayloadOps::NotifyVirtualChainChanged => {
                let rpc_client = client.clone();
                let id = listener_id;
                tst!(op, {
                    rpc_client
                        .start_notify(id, VirtualChainChangedScope { include_accepted_transaction_ids: false }.into())
                        .await
                        .unwrap();
                })
            }
            SporadPayloadOps::StopNotifyingCellsChanged => {
                let rpc_client = client.clone();
                let id = listener_id;
                tst!(op, {
                    rpc_client.stop_notify(id, CellsChangedScope::new(vec![]).into()).await.unwrap();
                })
            }
            SporadPayloadOps::StopNotifyingPruningPointCellSetOverride => {
                let rpc_client = client.clone();
                let id = listener_id;
                tst!(op, {
                    rpc_client.stop_notify(id, PruningPointCellSetOverrideScope {}.into()).await.unwrap();
                })
            }
        };
        tasks.push(task);
    }

    let _results = try_join_all(tasks).await;

    // Unregister the notification listener
    assert!(client.unregister_listener(listener_id).await.is_ok());

    // Shutdown should only be tested after everything
    let rpc_client = client.clone();
    let _ = rpc_client.shutdown_call(None, ShutdownRequest {}).await.unwrap();

    //
    // Fold-up
    //
    client.disconnect().await.unwrap();
    drop(client);
    daemon.shutdown();
}
