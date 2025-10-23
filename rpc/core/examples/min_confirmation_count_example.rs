//! Example: Demonstrating the min confirmation count feature in GetVirtualChainFromBlockRequest
//!
//! This example shows how to use the new min_confirmation_count parameter to filter
//! blocks by their confirmation count (distance from virtual chain tip).

use spora_rpc_core::model::{GetVirtualChainFromBlockRequest, RpcHash};
use workflow_serializer::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Min Confirmation Count Feature Example ===\n");

    // Example 1: Request without min confirmation count (default behavior)
    println!("1. Request without min confirmation count:");
    let _request_no_filter = GetVirtualChainFromBlockRequest {
        start_hash: RpcHash::from([1u8; 32]),
        include_accepted_transaction_ids: false,
        min_confirmation_count: None,
    };

    println!("   ✓ Created request with no confirmation filter");
    println!("   ✓ Will return all blocks regardless of confirmation count");

    // Example 2: Request with min confirmation count of 0 (same as no filter)
    println!("\n2. Request with min confirmation count of 0:");
    let _request_zero_filter = GetVirtualChainFromBlockRequest {
        start_hash: RpcHash::from([2u8; 32]),
        include_accepted_transaction_ids: true,
        min_confirmation_count: Some(0),
    };

    println!("   ✓ Created request with min_confirmation_count = 0");
    println!("   ✓ Will return all blocks (same as no filter)");

    // Example 3: Request with min confirmation count of 5
    println!("\n3. Request with min confirmation count of 5:");
    let request_filtered = GetVirtualChainFromBlockRequest {
        start_hash: RpcHash::from([3u8; 32]),
        include_accepted_transaction_ids: true,
        min_confirmation_count: Some(5),
    };

    println!("   ✓ Created request with min_confirmation_count = 5");
    println!("   ✓ Will only return blocks with at least 5 confirmations");
    println!("   ✓ Confirmation count = sink_blue_score - block_blue_score");

    // Example 4: Test serialization/deserialization
    println!("\n4. Testing serialization/deserialization:");

    // Test version 2 serialization (with min_confirmation_count)
    let mut buffer = Vec::new();
    request_filtered.serialize(&mut buffer)?;
    println!("   ✓ Serialized request with min_confirmation_count");

    // Test deserialization
    let deserialized: GetVirtualChainFromBlockRequest = Deserializer::deserialize(&mut std::io::Cursor::new(&buffer))?;

    println!("   ✓ Deserialized request successfully");
    println!("   ✓ min_confirmation_count preserved: {:?}", deserialized.min_confirmation_count);

    // Example 5: CLI usage example
    println!("\n5. CLI Usage Example:");
    println!(
        "   Command: spora-cli rpc get-virtual-chain-from-block <startHash> <includeAcceptedTransactionIds> <minConfirmationCount>"
    );
    println!("   Example: spora-cli rpc get-virtual-chain-from-block 0x1234... false 10");
    println!("   ✓ This will only return blocks with at least 10 confirmations");

    // Example 6: WASM/TypeScript usage
    println!("\n6. WASM/TypeScript Usage:");
    println!("   ```typescript");
    println!("   const request: IGetVirtualChainFromBlockRequest = {{");
    println!("     startHash: '0x1234...',");
    println!("     includeAcceptedTransactionIds: true,");
    println!("     minConfirmationCount: 5  // Optional parameter");
    println!("   }};");
    println!("   ```");
    println!("   ✓ minConfirmationCount is optional and defaults to 0");

    println!("\n=== Example Complete ===");
    println!("\nKey Benefits:");
    println!("• Filter blocks by confirmation count for better security");
    println!("• Backward compatible with existing clients");
    println!("• Version-aware serialization (v1 vs v2)");
    println!("• Available in CLI, gRPC, and WASM interfaces");

    Ok(())
}
