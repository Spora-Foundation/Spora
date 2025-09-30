//! Example: Show how to use payload functionality in PSTT
//!
//! This example shows:
//! 1. How to set the PSTT version to One to support payload
//! 2. How to add payload data
//! 3. How to combine PSTTs with payload
//! 4. Error handling

use tondi_wallet_pstt::global::CombineError as GlobalCombineError;
use tondi_wallet_pstt::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== PSTT Payload functionality example ===\n");

    // Example 1: Create a PSTT with payload
    println!("1. Create a PSTT with payload:");
    let payload_data = b"Hello, Tondi!".to_vec();

    let pstt_with_payload = PSTT::<Creator>::default()
        .set_version(Version::One)  // Set version to One to support payload
        .constructor()
        .payload(Some(payload_data.clone()))?; // Add payload data

    println!("   ✓ Successfully created a PSTT with payload");
    println!("   ✓ Payload data: {:?}", pstt_with_payload.global.payload);
    println!("   ✓ PSTT version: {:?}", pstt_with_payload.global.version);

    // Example 2: Try to set payload on Version::Zero (should fail)
    println!("\n2. Try to set payload on Version::Zero:");
    let result = PSTT::<Creator>::default().set_version(Version::Zero).constructor().payload(Some(vec![1, 2, 3]));

    match result {
        Err(Error::PayloadRequiresVersion1(version)) => {
            println!("   ✓ Correctly captured error: Payload requires version 1 or higher, but current version is {:?}", version);
        }
        _ => {
            println!("   ✗ Unexpected result");
        }
    }

    // Example 3: Combine two PSTTs with the same payload
    println!("\n3. Combine two PSTTs with the same payload:");
    let pstt1 = PSTT::<Creator>::default().set_version(Version::One).constructor().payload(Some(payload_data.clone()))?;

    let pstt2 = PSTT::<Creator>::default().set_version(Version::One).constructor().payload(Some(payload_data.clone()))?;

    let combiner = pstt1.combiner();
    let combined_result = combiner + pstt2;

    match combined_result {
        Ok(combined) => {
            println!("   ✓ Successfully combined PSTT");
            println!("   ✓ Combined payload: {:?}", combined.global.payload);
        }
        Err(e) => {
            println!("   ✗ Failed to combine: {:?}", e);
        }
    }

    // Example 4: Try to combine PSTTs with different payloads (should fail)
    println!("\n4. Try to combine PSTTs with different payloads:");
    let pstt3 = PSTT::<Creator>::default().set_version(Version::One).constructor().payload(Some(vec![1, 2, 3]))?;

    let pstt4 = PSTT::<Creator>::default().set_version(Version::One).constructor().payload(Some(vec![4, 5, 6]))?;

    let combiner3 = pstt3.combiner();
    let different_payload_result = combiner3 + pstt4;

    match different_payload_result {
        Err(CombineError::Global(GlobalCombineError::PayloadMismatch { this, that })) => {
            println!("   ✓ Correctly captured payload mismatch error");
            println!("   ✓ First PSTT's payload: {:?}", this);
            println!("   ✓ Second PSTT's payload: {:?}", that);
        }
        Ok(_) => {
            println!("   ✗ Unexpected success, should fail");
        }
        Err(e) => {
            println!("   ✗ Unexpected error: {:?}", e);
        }
    }

    // Example 5: Combine a PSTT with payload and one without payload
    println!("\n5. Combine a PSTT with payload and one without payload:");
    let pstt_with_payload = PSTT::<Creator>::default().set_version(Version::One).constructor().payload(Some(payload_data.clone()))?;

    let pstt_without_payload = PSTT::<Creator>::default().set_version(Version::One).constructor().payload(None)?;

    let combiner_with = pstt_with_payload.combiner();
    let mixed_result = combiner_with + pstt_without_payload;

    match mixed_result {
        Ok(combined) => {
            println!("   ✓ Successfully combined PSTT");
            println!("   ✓ Combined payload: {:?}", combined.global.payload);
            println!("   ✓ Used data from PSTT with payload");
        }
        Err(e) => {
            println!("   ✗ Failed to combine: {:?}", e);
        }
    }

    println!("\n=== Example completed ===");
    Ok(())
}
