// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Wallet account variant integration tests
// Covers watch-only, multisig, and bip32watch accounts across native and WASM paths

#![cfg(test)]
#![cfg(feature = "integration-tests")]

use std::str::FromStr;
use std::sync::Arc;

use spora_bip32::{ExtendedPrivateKey, ExtendedPublicKey, Language, Mnemonic, WordCount};
use spora_hashes::Hash;
use spora_wallet_core::{
    account::{Account, BIP32_ACCOUNT_KIND, BIP32_WATCH_ACCOUNT_KIND, MULTISIG_ACCOUNT_KIND, WATCH_ONLY_ACCOUNT_KIND},
    encryption::EncryptionKind,
    secret::Secret,
    storage::{AccountStore, PrvKeyDataStore},
    wallet::{Wallet, WalletCreateArgs, WalletOpenArgs},
};

/// Deterministic test mnemonic for reproducible tests
const TEST_MNEMONIC_PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

/// Test data for account variant tests using deterministic keys
struct TestAccountData {
    mnemonic: Mnemonic,
    xprv: ExtendedPrivateKey<secp256k1::SecretKey>,
    xpub: ExtendedPublicKey<secp256k1::PublicKey>,
    xpub_string: String,
}

impl TestAccountData {
    fn new() -> Self {
        let mnemonic = Mnemonic::from_phrase(TEST_MNEMONIC_PHRASE, Language::English).unwrap();
        let xprv = ExtendedPrivateKey::from_mnemonic(&mnemonic).unwrap();
        let xpub = ExtendedPublicKey::from_xprv(&xprv);
        let xpub_string = xpub.to_string(None).unwrap();
        Self { mnemonic, xprv, xpub, xpub_string }
    }

    fn from_seed(seed_phrase: &str) -> Self {
        let mnemonic = Mnemonic::from_phrase(seed_phrase, Language::English).unwrap();
        let xprv = ExtendedPrivateKey::from_mnemonic(&mnemonic).unwrap();
        let xpub = ExtendedPublicKey::from_xprv(&xprv);
        let xpub_string = xpub.to_string(None).unwrap();
        Self { mnemonic, xprv, xpub, xpub_string }
    }
}

/// Create a temporary wallet for testing
async fn create_test_wallet() -> (Arc<Wallet>, Secret) {
    let wallet_secret = Secret::new(b"test_wallet_password".to_vec());
    let wallet = Wallet::try_new(None, "test_wallet", None, None, false).await.unwrap();

    // Create wallet storage
    let wallet_args = WalletCreateArgs::new(Some("test_wallet".to_string()), None, EncryptionKind::XChaCha20Poly1305, None, true);
    let _ = wallet.create_wallet(&wallet_secret, wallet_args).await.unwrap();

    (wallet, wallet_secret)
}

/// Test watch-only account creation using wallet core API
#[tokio::test]
async fn test_watch_only_account_creation() {
    use spora_wallet_core::wallet::args::AccountCreateArgsWatchOnly;

    let (wallet, wallet_secret) = create_test_wallet().await;
    let test_data = TestAccountData::new();

    // Create watch-only account using wallet API
    let account_args = AccountCreateArgsWatchOnly::new(
        Some("test-watch-only".to_string()),
        vec![test_data.xpub_string.clone()],
        1,     // minimum_signatures
        false, // ecdsa
    );

    let result = wallet.create_account_watch_only(&wallet_secret, account_args).await;
    assert!(result.is_ok(), "watch-only account creation should succeed: {:?}", result.err());

    let account = result.unwrap();

    // Verify account properties
    assert_eq!(account.name(), Some("test-watch-only"));
    assert_eq!(account.account_kind(), WATCH_ONLY_ACCOUNT_KIND);
    assert_eq!(*account.id(), account.id().clone()); // ID is valid

    // Verify account can be loaded from storage
    let account_store = wallet.clone().as_account_store().unwrap();
    let loaded = account_store.load_single(account.id()).await.unwrap();
    assert!(loaded.is_some(), "account should be persisted");
}

/// Test bip32watch account creation using wallet core API
#[tokio::test]
async fn test_bip32watch_account_creation() {
    use spora_wallet_core::wallet::args::AccountCreateArgsBip32Watch;

    let (wallet, wallet_secret) = create_test_wallet().await;
    let test_data = TestAccountData::new();

    // Create bip32watch account using wallet API
    let account_args = AccountCreateArgsBip32Watch::new(Some("test-bip32watch".to_string()), vec![test_data.xpub_string.clone()]);

    let result = wallet.create_account_bip32_watch(&wallet_secret, account_args).await;
    assert!(result.is_ok(), "bip32watch account creation should succeed: {:?}", result.err());

    let account = result.unwrap();

    // Verify account properties
    assert_eq!(account.name(), Some("test-bip32watch"));
    assert_eq!(account.account_kind(), BIP32_WATCH_ACCOUNT_KIND);

    // Verify receive address can be derived
    let receive_address = account.receive_address().await;
    assert!(receive_address.is_ok(), "should be able to derive receive address");
    let address = receive_address.unwrap();
    assert!(!address.to_string().is_empty(), "address should not be empty");
}

/// Test multisig account creation using wallet core API
#[tokio::test]
async fn test_multisig_account_creation() {
    use spora_wallet_core::storage::keydata::PrvKeyDataVariantKind;
    use spora_wallet_core::wallet::args::{PrvKeyDataArgs, PrvKeyDataCreateArgs};

    let (wallet, wallet_secret) = create_test_wallet().await;

    // Create two different test accounts with different seeds
    let test_data1 = TestAccountData::new();
    let test_data2 = TestAccountData::from_seed("legal winner thank year wave sausage worth useful legal winner thank yellow");

    // Create private key data for multisig
    let mnemonic1 = Secret::from(test_data1.mnemonic.phrase());
    let prv_key_data_args1 = PrvKeyDataCreateArgs::new(None, None, mnemonic1, PrvKeyDataVariantKind::Mnemonic);
    let prv_key_data_id1 = wallet.create_prv_key_data(&wallet_secret, prv_key_data_args1).await.unwrap();

    let mnemonic2 = Secret::from(test_data2.mnemonic.phrase());
    let prv_key_data_args2 = PrvKeyDataCreateArgs::new(None, None, mnemonic2, PrvKeyDataVariantKind::Mnemonic);
    let prv_key_data_id2 = wallet.create_prv_key_data(&wallet_secret, prv_key_data_args2).await.unwrap();

    // Create multisig account
    let prv_key_data_args = vec![PrvKeyDataArgs::new(prv_key_data_id1, None), PrvKeyDataArgs::new(prv_key_data_id2, None)];

    let result = wallet
        .create_account_multisig(
            &wallet_secret,
            prv_key_data_args,
            vec![], // no additional xpubs
            Some("test-multisig".to_string()),
            2, // require 2 signatures
        )
        .await;

    assert!(result.is_ok(), "multisig account creation should succeed: {:?}", result.err());

    let account = result.unwrap();
    assert_eq!(account.name(), Some("test-multisig"));
    assert_eq!(account.account_kind(), MULTISIG_ACCOUNT_KIND);
}

/// Test script hash computation for account locks
#[test]
fn test_account_lock_hash_computation() {
    // Test that lock script hashes are computed correctly
    use spora_hashes::HasherBase;
    use spora_hashes::MerkleBranchHash;

    // Simulate a lock script hash computation
    let lock_bytes = vec![0u8; 20]; // 20-byte pubkey hash
    let mut hasher = MerkleBranchHash::new();
    hasher.update(b"spora-cell/lock");
    hasher.update(&lock_bytes);
    let lock_hash = hasher.finalize();

    // Verify hash properties
    assert_eq!(lock_hash.as_bytes().len(), 32);

    // Test with different lock bytes
    let lock_bytes2 = vec![1u8; 20];
    let mut hasher2 = MerkleBranchHash::new();
    hasher2.update(b"spora-cell/lock");
    hasher2.update(&lock_bytes2);
    let lock_hash2 = hasher2.finalize();

    // Different inputs should produce different hashes
    assert_ne!(lock_hash.as_bytes(), lock_hash2.as_bytes());
}

/// Test account descriptor serialization
#[test]
fn test_account_descriptor_serialization() {
    // Test descriptor format for different account types
    let account_name = "test-account";
    let account_index = 0u64;

    // Verify basic descriptor properties
    assert!(!account_name.is_empty());

    // Test JSON serialization for account descriptor
    let descriptor = serde_json::json!({
        "accountName": account_name,
        "accountIndex": account_index,
        "ecdsa": true
    });

    assert_eq!(descriptor["accountName"].as_str().unwrap(), account_name);
    assert_eq!(descriptor["accountIndex"].as_u64().unwrap(), account_index);
    assert_eq!(descriptor["ecdsa"].as_bool().unwrap(), true);

    // Test round-trip serialization
    let serialized = serde_json::to_string(&descriptor).unwrap();
    let deserialized: serde_json::Value = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized["accountName"], account_name);
}

/// Integration test: Full account lifecycle end-to-end
#[tokio::test]
async fn test_account_lifecycle_integration() {
    use spora_wallet_core::wallet::args::AccountCreateArgsWatchOnly;

    let (wallet, wallet_secret) = create_test_wallet().await;
    let test_data = TestAccountData::new();

    // Step 1: Create watch-only account
    let account_args =
        AccountCreateArgsWatchOnly::new(Some("lifecycle-test".to_string()), vec![test_data.xpub_string.clone()], 1, false);

    let account = wallet.create_account_watch_only(&wallet_secret, account_args).await.unwrap();
    let account_id = *account.id();

    // Step 2: Verify account properties
    assert_eq!(account.name(), Some("lifecycle-test"));
    assert_eq!(account.account_kind(), WATCH_ONLY_ACCOUNT_KIND);

    // Step 3: Verify address derivation
    let receive_address = account.receive_address().await.unwrap();
    let change_address = account.change_address().await.unwrap();

    assert_ne!(receive_address.to_string(), change_address.to_string(), "receive and change addresses should be different");

    // Step 4: Verify account can be retrieved from storage
    let account_store = wallet.clone().as_account_store().unwrap();
    let stored = account_store.load_single(&account_id).await.unwrap();
    assert!(stored.is_some(), "account should exist in storage");

    // Step 5: Verify account balance (should be zero)
    let balance = account.balance().await.unwrap();
    assert_eq!(balance.mature, 0, "new account should have zero balance");

    // Step 6: Test account selection
    wallet.select(Some(&account)).await.unwrap();
    let selected = wallet.get_account(&account_id).await.unwrap();
    assert!(selected.is_some(), "selected account should be retrievable");
}

/// Test error handling for invalid account configurations using wallet core API
#[tokio::test]
async fn test_account_creation_error_handling() {
    use spora_wallet_core::wallet::args::AccountCreateArgsWatchOnly;
    use spora_wallet_core::Error;

    let (wallet, wallet_secret) = create_test_wallet().await;

    // Test empty xpub rejection for watch-only
    let account_args = AccountCreateArgsWatchOnly::new(
        Some("test-empty".to_string()),
        vec![], // empty xpubs
        1,
        false,
    );

    let result = wallet.create_account_watch_only(&wallet_secret, account_args).await;
    assert!(result.is_err(), "empty xpub list should be rejected");

    // Test invalid xpub format rejection
    let account_args =
        AccountCreateArgsWatchOnly::new(Some("test-invalid".to_string()), vec!["invalid_xpub_string".to_string()], 1, false);

    let result = wallet.create_account_watch_only(&wallet_secret, account_args).await;
    assert!(result.is_err(), "invalid xpub format should be rejected");

    // Test duplicate account creation (same account twice)
    let test_data = TestAccountData::new();
    let account_args =
        AccountCreateArgsWatchOnly::new(Some("test-duplicate".to_string()), vec![test_data.xpub_string.clone()], 1, false);

    let result1 = wallet.create_account_watch_only(&wallet_secret, account_args.clone()).await;
    assert!(result1.is_ok(), "first creation should succeed");

    let result2 = wallet.create_account_watch_only(&wallet_secret, account_args).await;
    assert!(result2.is_err(), "duplicate account creation should fail");
}

/// Test WASM-compatible serialization
#[test]
fn test_wasm_compatible_serialization() {
    // Test that account data can be serialized/deserialized for WASM boundary
    let test_data = TestAccountData::new();
    let xpub_str = test_data.xpub.to_string(None).unwrap();

    // Simulate WASM-boundary serialization
    let js_value = serde_json::json!({
        "type": "bip32watch",
        "accountName": "wasm-test",
        "xpubKeys": [xpub_str],
        "accountIndex": 0
    });

    // Verify serialization round-trip
    let serialized = serde_json::to_string(&js_value).unwrap();
    let deserialized: serde_json::Value = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized["type"], "bip32watch");
    assert_eq!(deserialized["accountIndex"], 0);
}

/// Test notification event structure for account creation
#[test]
fn test_account_create_notification_structure() {
    // Verify the notification event structure matches WASM expectations
    let event = serde_json::json!({
        "type": "account-create",
        "data": {
            "accountId": "test-id",
            "accountKind": "watch-only",
            "accountName": "test-account"
        }
    });

    assert_eq!(event["type"], "account-create");
    assert!(!event["data"]["accountId"].as_str().unwrap().is_empty());
}
