//! Example: Show how to use payload functionality in PSST
//!
//! This example shows:
//! 1. How to set the PSST version to One to support payload
//! 2. How to add payload data
//! 3. How to combine PSSTs with payload
//! 4. Error handling

use spora_wallet_psst::global::CombineError as GlobalCombineError;
use spora_wallet_psst::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== PSST Payload functionality example ===\n");

    // Example 1: Create a PSST with payload
    println!("1. Create a PSST with payload:");
    let payload_data = b"Hello, Spora!".to_vec();

    let psst_with_payload = PSST::<Creator>::default()
        .set_version(Version::One)  // Set version to One to support payload
        .constructor()
        .payload(Some(payload_data.clone()))?; // Add payload data

    println!("   ✓ Successfully created a PSST with payload");
    println!("   ✓ Payload data: {:?}", psst_with_payload.global.payload);
    println!("   ✓ PSST version: {:?}", psst_with_payload.global.version);

    // Example 2: Try to set payload on Version::Zero (should fail)
    println!("\n2. Try to set payload on Version::Zero:");
    let result = PSST::<Creator>::default().set_version(Version::Zero).constructor().payload(Some(vec![1, 2, 3]));

    match result {
        Err(Error::PayloadRequiresVersion1(version)) => {
            println!("   ✓ Correctly captured error: Payload requires version 1 or higher, but current version is {:?}", version);
        }
        _ => {
            println!("   ✗ Unexpected result");
        }
    }

    // Example 3: Combine two PSSTs with the same payload
    println!("\n3. Combine two PSSTs with the same payload:");
    let psst1 = PSST::<Creator>::default().set_version(Version::One).constructor().payload(Some(payload_data.clone()))?;

    let psst2 = PSST::<Creator>::default().set_version(Version::One).constructor().payload(Some(payload_data.clone()))?;

    let combiner = psst1.combiner();
    let combined_result = combiner + psst2;

    match combined_result {
        Ok(combined) => {
            println!("   ✓ Successfully combined PSST");
            println!("   ✓ Combined payload: {:?}", combined.global.payload);
        }
        Err(e) => {
            println!("   ✗ Failed to combine: {:?}", e);
        }
    }

    // Example 4: Try to combine PSSTs with different payloads (should fail)
    println!("\n4. Try to combine PSSTs with different payloads:");
    let psst3 = PSST::<Creator>::default().set_version(Version::One).constructor().payload(Some(vec![1, 2, 3]))?;

    let psst4 = PSST::<Creator>::default().set_version(Version::One).constructor().payload(Some(vec![4, 5, 6]))?;

    let combiner3 = psst3.combiner();
    let different_payload_result = combiner3 + psst4;

    match different_payload_result {
        Err(CombineError::Global(GlobalCombineError::PayloadMismatch { this, that })) => {
            println!("   ✓ Correctly captured payload mismatch error");
            println!("   ✓ First PSST's payload: {:?}", this);
            println!("   ✓ Second PSST's payload: {:?}", that);
        }
        Ok(_) => {
            println!("   ✗ Unexpected success, should fail");
        }
        Err(e) => {
            println!("   ✗ Unexpected error: {:?}", e);
        }
    }

    // Example 5: Combine a PSST with payload and one without payload
    println!("\n5. Combine a PSST with payload and one without payload:");
    let psst_with_payload = PSST::<Creator>::default().set_version(Version::One).constructor().payload(Some(payload_data.clone()))?;

    let psst_without_payload = PSST::<Creator>::default().set_version(Version::One).constructor().payload(None)?;

    let combiner_with = psst_with_payload.combiner();
    let mixed_result = combiner_with + psst_without_payload;

    match mixed_result {
        Ok(combined) => {
            println!("   ✓ Successfully combined PSST");
            println!("   ✓ Combined payload: {:?}", combined.global.payload);
            println!("   ✓ Used data from PSST with payload");
        }
        Err(e) => {
            println!("   ✗ Failed to combine: {:?}", e);
        }
    }

    println!("\n=== Example completed ===");
    Ok(())
}
