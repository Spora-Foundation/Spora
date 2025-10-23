# Spora RPC Client API Reference

The Spora RPC client provides a complete interface for communicating with Spora nodes. This document details all available RPC endpoints, parameters, and return values.

## Overview

The Spora RPC API supports two main communication protocols:
- **gRPC**: High-performance binary protocol
- **wRPC**: WebSocket and HTTP protocols

## Connection Management

### Connection Control
- `Connect` - Establish connection
- `Disconnect` - Disconnect

### Subscription Management
- `Subscribe` - Subscribe to notifications
- `Unsubscribe` - Unsubscribe from notifications

## Node Information Endpoints

### Basic Information
- **`ping`** - Check node connection status
  - Parameters: None
  - Returns: Empty response or error message

- **`get_system_info`** - Get system information
  - Parameters: None
  - Returns: System information (available memory, CPU cores, file descriptors, etc.)

- **`get_connections`** - Get current active TCP connections count
  - Parameters: `include_profile_data` (bool) - Whether to include connection profile data
  - Returns: Connection information

- **`get_server_info`** - Get server information
  - Parameters: None
  - Returns: Version, network ID, sync status, virtual DAA score, etc.

- **`get_sync_status`** - Get node sync status
  - Parameters: None
  - Returns: Whether synced (bool)

- **`get_current_network`** - Get current network type
  - Parameters: None
  - Returns: Network type (mainnet, testnet, etc.)

- **`get_info`** - Get general node information
  - Parameters: None
  - Returns: Detailed node information

### Performance Monitoring
- **`get_metrics`** - Get performance metrics
  - Parameters:
    - `process_metrics` (bool) - Process metrics
    - `connection_metrics` (bool) - Connection metrics
    - `bandwidth_metrics` (bool) - Bandwidth metrics
    - `consensus_metrics` (bool) - Consensus metrics
    - `storage_metrics` (bool) - Storage metrics
    - `custom_metrics` (bool) - Custom metrics
  - Returns: Performance metrics data

## Blockchain Operation Endpoints

### Block Operations
- **`submit_block`** - Submit new block to DAG
  - Parameters:
    - `block` (RpcRawBlock) - Block data
    - `allow_non_daa_blocks` (bool) - Whether to allow non-DAA blocks
  - Returns: Submission result

- **`get_block_template`** - Get mining block template
  - Parameters:
    - `pay_address` (RpcAddress) - Payment address
    - `extra_data` (RpcExtraData) - Extra data
  - Returns: Block template

- **`get_block`** - Get specified block information
  - Parameters:
    - `hash` (RpcHash) - Block hash
    - `include_transactions` (bool) - Whether to include transaction information
  - Returns: Block information

- **`get_block_status`** - Get block status
  - Parameters: `hash` (RpcHash) - Block hash
  - Returns: Block status

- **`get_header`** - Get block header information
  - Parameters: `hash` (RpcHash) - Block hash
  - Returns: Block header information

- **`get_blocks`** - Get block range
  - Parameters:
    - `low_hash` (Option<RpcHash>) - Starting block hash
    - `include_blocks` (bool) - Whether to include block data
    - `include_transactions` (bool) - Whether to include transaction data
  - Returns: Block list

- **`get_block_count`** - Get total number of blocks in DAG
  - Parameters: None
  - Returns: Block count

- **`get_block_dag_info`** - Get DAG state information
  - Parameters: None
  - Returns: DAG information

- **`get_headers`** - Get block header list
  - Parameters:
    - `start_hash` (RpcHash) - Starting hash
    - `limit` (u64) - Limit count
    - `is_ascending` (bool) - Whether to sort in ascending order
  - Returns: Block header list

### Transaction Operations
- **`submit_transaction`** - Submit transaction to mempool
  - Parameters:
    - `transaction` (RpcTransaction) - Transaction data
    - `allow_orphan` (bool) - Whether to allow orphan transactions
  - Returns: Transaction ID

- **`submit_transaction_replacement`** - Submit transaction replacement (Replace by Fee policy)
  - Parameters: `transaction` (RpcTransaction) - New transaction
  - Returns: Replacement result

- **`get_transaction`** - Get transaction information
  - Parameters: `hash` (RpcHash) - Transaction hash
  - Returns: Transaction information

- **`get_mempool_entry`** - Get transaction information in mempool
  - Parameters:
    - `transaction_id` (RpcTransactionId) - Transaction ID
    - `include_orphan_pool` (bool) - Whether to include orphan pool
    - `filter_transaction_pool` (bool) - Whether to filter transaction pool
  - Returns: Mempool entry

- **`get_mempool_entries`** - Get mempool snapshot
  - Parameters:
    - `include_orphan_pool` (bool) - Whether to include orphan pool
    - `filter_transaction_pool` (bool) - Whether to filter transaction pool
  - Returns: Mempool entries list

- **`get_mempool_entries_by_addresses`** - Get mempool entries for specified addresses
  - Parameters:
    - `addresses` (Vec<RpcAddress>) - Address list
    - `include_orphan_pool` (bool) - Whether to include orphan pool
    - `filter_transaction_pool` (bool) - Whether to filter transaction pool
  - Returns: Mempool entries grouped by address

### Network and Consensus
- **`get_sink`** - Get hash of current virtual selected parent block
  - Parameters: None
  - Returns: Current sink hash

- **`get_sink_blue_score`** - Get blue score of current virtual selected parent block
  - Parameters: None
  - Returns: Blue score

- **`get_virtual_chain_from_block`** - Get chain from specified block to current virtual
  - Parameters:
    - `start_hash` (RpcHash) - Starting block hash
    - `include_accepted_transaction_ids` (bool) - Whether to include accepted transaction IDs
  - Returns: Virtual chain information

- **`get_subnetwork`** - Get subnetwork information
  - Parameters: `subnetwork_id` (RpcSubnetworkId) - Subnetwork ID
  - Returns: Subnetwork information

- **`resolve_finality_conflict`** - Resolve finality conflict
  - Parameters: `finality_block_hash` (RpcHash) - Finality block hash
  - Returns: Empty response or error message

- **`get_current_block_color`** - Determine block color by iterating DAG
  - Parameters: `hash` (RpcHash) - Block hash
  - Returns: Block color information

## Address and UTXO Endpoints

### Address Operations
- **`get_balance_by_address`** - Get balance for specified address
  - Parameters: `address` (RpcAddress) - Address
  - Returns: Balance (sau)
  - Note: Requires `--utxoindex` option to be enabled

- **`get_balances_by_addresses`** - Get balances for multiple addresses
  - Parameters: `addresses` (Vec<RpcAddress>) - Address list
  - Returns: Address balances list
  - Note: Requires `--utxoindex` option to be enabled

- **`get_utxos_by_addresses`** - Get UTXO list for specified addresses
  - Parameters: `addresses` (Vec<RpcAddress>) - Address list
  - Returns: Address UTXO entries list
  - Note: Requires `--utxoindex` option to be enabled

- **`get_utxo_return_address`** - Get UTXO return address
  - Parameters:
    - `txid` (RpcHash) - Transaction ID
    - `accepting_block_daa_score` (u64) - DAA score of accepting block
  - Returns: Return address

## Network Management Endpoints

### Peer Management
- **`get_peer_addresses`** - Get list of known Spora addresses
  - Parameters: None
  - Returns: Known addresses and banned addresses list

- **`get_connected_peer_info`** - Get connected peer information
  - Parameters: None
  - Returns: Connected peer information and statistics

- **`add_peer`** - Add peer
  - Parameters:
    - `peer_address` (RpcContextualPeerAddress) - Peer address
    - `is_permanent` (bool) - Whether it's a permanent connection
  - Returns: Empty response or error message

- **`ban`** - Ban specified IP address
  - Parameters: `ip` (RpcIpAddress) - IP address
  - Returns: Empty response or error message

- **`unban`** - Unban IP address
  - Parameters: `ip` (RpcIpAddress) - IP address
  - Returns: Empty response or error message

## Mining and Fee Endpoints

### Mining Operations
- **`estimate_network_hashes_per_second`** - Estimate network hash rate
  - Parameters:
    - `window_size` (u32) - Window size
    - `start_hash` (Option<RpcHash>) - Starting block hash
  - Returns: Network hash rate

### Fee Estimation
- **`get_fee_estimate`** - Get fee estimate
  - Parameters: None
  - Returns: Fee estimate information

- **`get_fee_estimate_experimental`** - Get experimental fee estimate
  - Parameters: `verbose` (bool) - Whether to output verbosely
  - Returns: Experimental fee estimate information

## Supply and Statistics Endpoints

- **`get_coin_supply`** - Get current issuance supply
  - Parameters: None
  - Returns: Coin supply information

- **`get_daa_score_timestamp_estimate`** - Get DAA score timestamp estimate
  - Parameters: `daa_scores` (Vec<u64>) - DAA scores list
  - Returns: Timestamps list

## Notification Endpoints

### Block Notifications
- **`NotifyBlockAdded`** - New block added notification
- **`NotifyNewBlockTemplate`** - New block template notification

### Chain State Notifications
- **`NotifyUtxosChanged`** - UTXO change notification
- **`NotifyPruningPointUtxoSetOverride`** - Pruning point UTXO set override notification
- **`NotifyFinalityConflict`** - Finality conflict notification
- **`NotifyFinalityConflictResolved`** - Finality conflict resolved notification
- **`NotifyVirtualDaaScoreChanged`** - Virtual DAA score change notification
- **`NotifyVirtualChainChanged`** - Virtual chain change notification
- **`NotifySinkBlueScoreChanged`** - Sink blue score change notification

### Notification Management
- **`register_new_listener`** - Register new listener
- **`unregister_listener`** - Unregister listener
- **`start_notify`** - Start sending notifications
- **`stop_notify`** - Stop sending notifications
- **`execute_subscribe_command`** - Execute subscription command

## System Management Endpoints

- **`shutdown`** - Shutdown node
  - Parameters: None
  - Returns: Empty response or error message

## Usage Examples

### Basic Connection
```rust
use spora_rpc_core::api::rpc::RpcApi;
use spora_grpc_client::GrpcClient;

let client = GrpcClient::new("localhost:17110").await?;

// Check connection
client.ping().await?;

// Get node information
let info = client.get_info().await?;
println!("Node version: {}", info.version);
```

### Subscribe to Notifications
```rust
// Register listener
let listener_id = client.register_new_listener(connection).await?;

// Start receiving block added notifications
client.start_notify(listener_id, Scope::BlockAdded).await?;

// Stop notifications
client.stop_notify(listener_id, Scope::BlockAdded).await?;
```

### Query Blockchain Data
```rust
// Get latest block
let sink = client.get_sink().await?;
let block = client.get_block(sink.sink_hash, true).await?;

// Get address balance
let balance = client.get_balance_by_address(address).await?;
```

### Submit Transaction
```rust
// Submit transaction to mempool
let tx_id = client.submit_transaction(transaction, false).await?;
println!("Transaction submitted: {}", tx_id);
```

## Error Handling

All RPC calls return `RpcResult<T>` type, requiring appropriate error handling:

```rust
match client.get_block(hash, true).await {
    Ok(block) => println!("Block: {:?}", block),
    Err(e) => eprintln!("Error: {}", e),
}
```

## Important Notes

1. **UTXO Index**: Some endpoints (such as balance queries) require the `--utxoindex` option to be enabled
2. **Network Type**: Ensure connection to the correct network (mainnet, testnet, etc.)
3. **Permissions**: Some operations may require appropriate permission settings
4. **Connection Management**: Reasonably manage connection count to avoid resource exhaustion
5. **Error Retry**: Implement appropriate retry mechanisms for network errors

## Version Information

- **API Version**: 1
- **API Revision**: 0
- **Max Safe Window Size**: 10,000

## Support

For more help or to report issues, please refer to the project documentation or submit an issue.
