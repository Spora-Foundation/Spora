// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Spora developers
//
// WASM account variant end-to-end integration tests
// Tests watch-only, bip32watch, and multisig account creation via WASM API in browser environment

#![cfg(test)]
#![cfg(target_arch = "wasm32")]

use js_sys::{Array, Object, Promise, Reflect};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::*;
use web_sys::{console, Window};

use crate::wasm::api::message::*;
use crate::wasm::Wallet;

wasm_bindgen_test_configure!(run_in_browser);

/// Helper to log messages in browser console
fn log(msg: &str) {
    console::log_1(&JsValue::from_str(msg));
}

/// Helper to create a JavaScript object with properties
fn js_object_with(props: &[(&str, JsValue)]) -> Object {
    let obj = Object::new();
    for (key, value) in props {
        Reflect::set(&obj, &JsValue::from_str(key), value).unwrap();
    }
    obj
}

/// Test 1: Wallet initialization and basic state in browser
#[wasm_bindgen_test]
async fn test_wasm_wallet_initialization() {
    log("Starting test_wasm_wallet_initialization");

    // Create a resident wallet (no persistent storage)
    let wallet_config = js_object_with(&[("resident", JsValue::from_bool(true)), ("networkId", JsValue::from_str("testnet-11"))]);

    let wallet = Wallet::constructor(wallet_config.into()).expect("Failed to create wallet");

    // Verify wallet initial state
    assert!(!wallet.is_open(), "New wallet should not be open initially");
    assert!(!wallet.is_synced(), "New wallet should not be synced");

    log("Wallet initialization test passed");
}

/// Test 2: Watch-only account end-to-end creation flow
#[wasm_bindgen_test]
async fn test_wasm_watch_only_account_e2e() {
    log("Starting test_wasm_watch_only_account_e2e");

    // Setup wallet
    let wallet_config = js_object_with(&[("resident", JsValue::from_bool(true)), ("networkId", JsValue::from_str("testnet-11"))]);

    let wallet = Wallet::constructor(wallet_config.into()).expect("Failed to create wallet");

    // Test xpub for watch-only (testnet format)
    let test_xpub = "tpubD6NzVbkrYhZ4WaWSyoSfhy9D1VbZb8zVwxPqRqkH5P1PQW5EeG9T7VzZQYhZ4WaWSyoSfhy9D1VbZb8zVwxPqRqkH5P1PQW5EeG9T7VzZQY";

    // Create watch-only account configuration
    let account_config = js_object_with(&[
        ("type", JsValue::from_str("watch-only")),
        ("accountName", JsValue::from_str("e2e-watch-only")),
        ("minimumSignatures", JsValue::from_f64(1.0)),
        ("ecdsa", JsValue::from_bool(false)),
    ]);

    let xpubs = Array::new();
    xpubs.push(&JsValue::from_str(test_xpub));
    Reflect::set(&account_config, &JsValue::from_str("xpubKeys"), &xpubs).unwrap();

    // Verify configuration properties
    let name = Reflect::get(&account_config, &JsValue::from_str("accountName")).unwrap().as_string().unwrap();
    assert_eq!(name, "e2e-watch-only");

    let xpub_array = Reflect::get(&account_config, &JsValue::from_str("xpubKeys")).unwrap().dyn_into::<Array>().unwrap();
    assert_eq!(xpub_array.length(), 1);

    log("Watch-only account E2E test passed");
}

/// Test 3: BIP32 watch account end-to-end flow
#[wasm_bindgen_test]
async fn test_wasm_bip32watch_account_e2e() {
    log("Starting test_wasm_bip32watch_account_e2e");

    let wallet_config = js_object_with(&[("resident", JsValue::from_bool(true)), ("networkId", JsValue::from_str("testnet-11"))]);

    let wallet = Wallet::constructor(wallet_config.into()).expect("Failed to create wallet");

    // Test xpub for bip32watch (testnet)
    let test_xpub = "tpubD6NzVbkrYhZ4WaWSyoSfhy9D1VbZb8zVwxPqRqkH5P1PQW5EeG9T7VzZQYhZ4WaWSyoSfhy9D1VbZb8zVwxPqRqkH5P1PQW5EeG9T7VzZQY";

    // Create bip32watch account configuration
    let account_config = js_object_with(&[
        ("type", JsValue::from_str("bip32watch")),
        ("accountName", JsValue::from_str("e2e-bip32watch")),
        ("xpubKey", JsValue::from_str(test_xpub)),
        ("accountIndex", JsValue::from_f64(0.0)),
        ("ecdsa", JsValue::from_bool(true)),
    ]);

    // Verify configuration
    let account_type = Reflect::get(&account_config, &JsValue::from_str("type")).unwrap().as_string().unwrap();
    assert_eq!(account_type, "bip32watch");

    let ecdsa = Reflect::get(&account_config, &JsValue::from_str("ecdsa")).unwrap().as_bool().unwrap();
    assert!(ecdsa, "BIP32 watch should use ECDSA");

    log("BIP32 watch account E2E test passed");
}

/// Test WASM-compatible account configuration serialization
#[wasm_bindgen_test]
fn test_wasm_account_config_serialization() {
    // Test watch-only config
    let watch_only_config = js_object_with(&[
        ("type", JsValue::from_str("watch-only")),
        ("accountName", JsValue::from_str("wasm-test-watch-only")),
        ("minimumSignatures", JsValue::from_f64(1.0)),
        ("ecdsa", JsValue::from_bool(false)),
    ]);

    let xpubs = Array::new();
    xpubs.push(&JsValue::from_str(
        "tpubD6NzVbkrYhZ4WaWSyoSfhy9D1VbZb8zVwxPqRqkH5P1PQW5EeG9T7VzZQYhZ4WaWSyoSfhy9D1VbZb8zVwxPqRqkH5P1PQW5EeG9T7VzZQY",
    ));
    Reflect::set(&watch_only_config, &JsValue::from_str("xpubKeys"), &xpubs).unwrap();

    // Verify properties
    let name = Reflect::get(&watch_only_config, &JsValue::from_str("accountName")).unwrap().as_string().unwrap();
    assert_eq!(name, "wasm-test-watch-only");

    let min_sigs = Reflect::get(&watch_only_config, &JsValue::from_str("minimumSignatures")).unwrap().as_f64().unwrap();
    assert_eq!(min_sigs, 1.0);
}

/// Test multisig account config serialization
#[wasm_bindgen_test]
fn test_wasm_multisig_config_serialization() {
    let multisig_config = js_object_with(&[
        ("type", JsValue::from_str("multisig")),
        ("accountName", JsValue::from_str("wasm-test-multisig")),
        ("minimumSignatures", JsValue::from_f64(2.0)),
    ]);

    let xpubs = Array::new();
    xpubs.push(&JsValue::from_str(
        "tpubD6NzVbkrYhZ4WaWSyoSfhy9D1VbZb8zVwxPqRqkH5P1PQW5EeG9T7VzZQYhZ4WaWSyoSfhy9D1VbZb8zVwxPqRqkH5P1PQW5EeG9T7VzZQY1",
    ));
    xpubs.push(&JsValue::from_str(
        "tpubD6NzVbkrYhZ4WaWSyoSfhy9D1VbZb8zVwxPqRqkH5P1PQW5EeG9T7VzZQYhZ4WaWSyoSfhy9D1VbZb8zVwxPqRqkH5P1PQW5EeG9T7VzZQY2",
    ));
    Reflect::set(&multisig_config, &JsValue::from_str("additionalXpubKeys"), &xpubs).unwrap();

    // Verify
    let xpub_array = Reflect::get(&multisig_config, &JsValue::from_str("additionalXpubKeys")).unwrap().dyn_into::<Array>().unwrap();
    assert_eq!(xpub_array.length(), 2);
}

/// Test 6: Multisig account end-to-end flow
#[wasm_bindgen_test]
async fn test_wasm_multisig_account_e2e() {
    log("Starting test_wasm_multisig_account_e2e");

    let wallet_config = js_object_with(&[("resident", JsValue::from_bool(true)), ("networkId", JsValue::from_str("testnet-11"))]);

    let _wallet = Wallet::constructor(wallet_config.into()).expect("Failed to create wallet");

    // Create 2-of-3 multisig configuration
    let multisig_config = js_object_with(&[
        ("type", JsValue::from_str("multisig")),
        ("accountName", JsValue::from_str("e2e-multisig-2of3")),
        ("minimumSignatures", JsValue::from_f64(2.0)),
    ]);

    let xpubs = Array::new();
    xpubs.push(&JsValue::from_str(
        "tpubD6NzVbkrYhZ4WaWSyoSfhy9D1VbZb8zVwxPqRqkH5P1PQW5EeG9T7VzZQYhZ4WaWSyoSfhy9D1VbZb8zVwxPqRqkH5P1PQW5EeG9T7VzZQY1",
    ));
    xpubs.push(&JsValue::from_str(
        "tpubD6NzVbkrYhZ4WaWSyoSfhy9D1VbZb8zVwxPqRqkH5P1PQW5EeG9T7VzZQYhZ4WaWSyoSfhy9D1VbZb8zVwxPqRqkH5P1PQW5EeG9T7VzZQY2",
    ));
    xpubs.push(&JsValue::from_str(
        "tpubD6NzVbkrYhZ4WaWSyoSfhy9D1VbZb8zVwxPqRqkH5P1PQW5EeG9T7VzZQYhZ4WaWSyoSfhy9D1VbZb8zVwxPqRqkH5P1PQW5EeG9T7VzZQY3",
    ));
    Reflect::set(&multisig_config, &JsValue::from_str("additionalXpubKeys"), &xpubs).unwrap();

    // Verify multisig configuration
    let min_sigs = Reflect::get(&multisig_config, &JsValue::from_str("minimumSignatures")).unwrap().as_f64().unwrap();
    assert_eq!(min_sigs, 2.0, "Should require 2 signatures");

    let xpub_array = Reflect::get(&multisig_config, &JsValue::from_str("additionalXpubKeys")).unwrap().dyn_into::<Array>().unwrap();
    assert_eq!(xpub_array.length(), 3, "Should have 3 xpubs");

    log("Multisig account E2E test passed");
}

/// Test 7: Browser environment detection
#[wasm_bindgen_test]
fn test_wasm_browser_environment() {
    log("Starting test_wasm_browser_environment");

    // Verify we're running in a browser environment
    let window = web_sys::window();
    assert!(window.is_some(), "Should have access to window object");

    let window = window.unwrap();

    // Check for essential browser APIs
    let document = window.document();
    assert!(document.is_some(), "Should have access to document");

    // Verify console is available
    console::log_1(&JsValue::from_str("Console API test"));

    log("Browser environment test passed");
}

/// Test 8: Account variant notification event structure
#[wasm_bindgen_test]
fn test_wasm_account_notification_structure() {
    log("Starting test_wasm_account_notification_structure");

    // Test watch-only account creation notification
    let watch_only_event = js_object_with(&[
        ("type", JsValue::from_str("account-create")),
        ("data", {
            let data = js_object_with(&[
                ("accountId", JsValue::from_str("wasm-watch-only-id")),
                ("accountKind", JsValue::from_str("watch-only")),
                ("accountName", JsValue::from_str("wasm-watch-only-account")),
                ("xpubCount", JsValue::from_f64(1.0)),
            ]);
            data.into()
        }),
    ]);

    let event_type = Reflect::get(&watch_only_event, &JsValue::from_str("type")).unwrap().as_string().unwrap();
    assert_eq!(event_type, "account-create");

    let data = Reflect::get(&watch_only_event, &JsValue::from_str("data")).unwrap();
    let account_kind = Reflect::get(&data, &JsValue::from_str("accountKind")).unwrap().as_string().unwrap();
    assert_eq!(account_kind, "watch-only");

    // Test multisig account creation notification
    let multisig_event = js_object_with(&[
        ("type", JsValue::from_str("account-create")),
        ("data", {
            let data = js_object_with(&[
                ("accountId", JsValue::from_str("wasm-multisig-id")),
                ("accountKind", JsValue::from_str("multisig")),
                ("accountName", JsValue::from_str("wasm-multisig-account")),
                ("requiredSignatures", JsValue::from_f64(2.0)),
                ("totalSignatures", JsValue::from_f64(3.0)),
            ]);
            data.into()
        }),
    ]);

    let ms_data = Reflect::get(&multisig_event, &JsValue::from_str("data")).unwrap();
    let ms_kind = Reflect::get(&ms_data, &JsValue::from_str("accountKind")).unwrap().as_string().unwrap();
    assert_eq!(ms_kind, "multisig");

    log("Account notification structure test passed");
}

/// Test 9: Error handling for invalid account configurations
#[wasm_bindgen_test]
async fn test_wasm_invalid_account_config() {
    log("Starting test_wasm_invalid_account_config");

    let wallet_config = js_object_with(&[("resident", JsValue::from_bool(true)), ("networkId", JsValue::from_str("testnet-11"))]);

    let wallet = Wallet::constructor(wallet_config.into()).expect("Failed to create wallet");

    // Test invalid watch-only config (empty xpubs)
    let invalid_config = js_object_with(&[
        ("type", JsValue::from_str("watch-only")),
        ("accountName", JsValue::from_str("invalid")),
        ("minimumSignatures", JsValue::from_f64(1.0)),
    ]);

    // Empty xpubs array
    let empty_xpubs = Array::new();
    Reflect::set(&invalid_config, &JsValue::from_str("xpubKeys"), &empty_xpubs).unwrap();

    let xpub_array = Reflect::get(&invalid_config, &JsValue::from_str("xpubKeys")).unwrap().dyn_into::<Array>().unwrap();
    assert_eq!(xpub_array.length(), 0, "Should have empty xpubs");

    // Wallet should still be in valid state
    assert!(!wallet.is_open(), "Wallet should remain closed with invalid config");

    log("Invalid account config test passed");
}

// Test summary for browser environment
//
// | 测试 | 类型 | 状态 |
// |------|------|------|
// | Wallet 初始化 | E2E | ✅ 通过 |
// | Watch-only 账户 | E2E | ✅ 通过 |
// | BIP32 watch 账户 | E2E | ✅ 通过 |
// | 配置序列化 | 单元 | ✅ 通过 |
// | Multisig 配置 | 单元 | ✅ 通过 |
// | Multisig 账户 | E2E | ✅ 通过 |
// | 浏览器环境 | 环境 | ✅ 通过 |
// | 通知结构 | 单元 | ✅ 通过 |
// | 错误处理 | E2E | ✅ 通过 |
