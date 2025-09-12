use std::io::Write;
use tempfile::NamedTempFile;
use treasure_boy::{load_addresses_from_file, AddressDistributionTracker, Config, TxsFeeConfig, NetworkType};

#[test]
fn test_load_addresses_from_file_success() {
    // Create temporary file
    let mut temp_file = NamedTempFile::new().unwrap();
    let addresses_content = r#"# Test address file
tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6
tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6

# Another address
tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6
"#;

    temp_file.write_all(addresses_content.as_bytes()).unwrap();
    temp_file.flush().unwrap();

    // Test loading addresses
    let addresses = load_addresses_from_file(temp_file.path().to_str().unwrap()).unwrap();

    assert_eq!(addresses.len(), 3);
    assert!(addresses.iter().all(|addr| format!("{addr}").starts_with("tondidev:")));
}

#[test]
fn test_load_addresses_from_file_with_invalid_addresses() {
    // Create temporary file with invalid addresses
    let mut temp_file = NamedTempFile::new().unwrap();
    let addresses_content = r#"# Valid addresses
tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6

# Invalid addresses
invalid_address_123
another_invalid_address

# Another valid address
tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6
"#;

    temp_file.write_all(addresses_content.as_bytes()).unwrap();
    temp_file.flush().unwrap();

    // Test loading addresses (should skip invalid addresses)
    let addresses = load_addresses_from_file(temp_file.path().to_str().unwrap()).unwrap();

    assert_eq!(addresses.len(), 2);
    assert!(addresses.iter().all(|addr| format!("{addr}").starts_with("tondidev:")));
}

#[test]
fn test_load_addresses_from_file_empty() {
    // Create empty file
    let temp_file = NamedTempFile::new().unwrap();

    let addresses = load_addresses_from_file(temp_file.path().to_str().unwrap()).unwrap();

    assert_eq!(addresses.len(), 0);
}

#[test]
fn test_load_addresses_from_file_nonexistent() {
    let result = load_addresses_from_file("/nonexistent/file.txt");

    assert!(result.is_err());
}

#[test]
fn test_address_distribution_tracker_integration() {
    use tondi_addresses::{Address, Prefix, Version};

    let addresses = vec![
        Address::new(Prefix::Devnet, Version::PubKey, &[1; 32]),
        Address::new(Prefix::Devnet, Version::PubKey, &[2; 32]),
        Address::new(Prefix::Devnet, Version::PubKey, &[3; 32]),
    ];

    let mut tracker = AddressDistributionTracker::new(addresses.clone());

    // Simulate multiple rounds of distribution
    for round in 0..5 {
        let selected = tracker.get_next_addresses(2);
        assert_eq!(selected.len(), 2);

        // Validate distribution stats
        let stats = tracker.get_distribution_stats();
        assert!(stats.contains("total="));

        println!("Round {}: {}", round, stats);
    }

    // Validate final distribution stats
    let final_stats = tracker.get_distribution_stats();
    assert!(final_stats.contains("total=10")); // 5 rounds * 2 addresses = 10 distributions
}

#[test]
fn test_config_validation() {
    let config = Config {
        private_key: Some("test_key".to_string()),
        tps: 10,
        rpc_server: "localhost:16210".to_string(),
        threads: 4,
        unleashed: true,
        addr: Some("test_addr".to_string()),
        address_file: Some("test_file.txt".to_string()),
        outputs_per_tx: 5,
        priority_fee: 1000,
        randomize_fee: true,
        generate_addresses: None,
        output_file: None,
        network: NetworkType::Testnet,
    };

    assert_eq!(config.tps, 10);
    assert_eq!(config.threads, 4);
    assert_eq!(config.outputs_per_tx, 5);
    assert!(config.unleashed);
    assert!(config.randomize_fee);
    assert_eq!(config.priority_fee, 1000);
}

#[test]
fn test_txs_fee_config() {
    let fee_config = TxsFeeConfig { priority_fee: 500, randomize_fee: true };

    assert_eq!(fee_config.priority_fee, 500);
    assert!(fee_config.randomize_fee);

    let fee_config_no_random = TxsFeeConfig { priority_fee: 1000, randomize_fee: false };

    assert_eq!(fee_config_no_random.priority_fee, 1000);
    assert!(!fee_config_no_random.randomize_fee);
}

#[test]
fn test_address_distribution_fairness() {
    use tondi_addresses::{Address, Prefix, Version};

    let addresses =
        vec![Address::new(Prefix::Devnet, Version::PubKey, &[1; 32]), Address::new(Prefix::Devnet, Version::PubKey, &[2; 32])];

    let mut tracker = AddressDistributionTracker::new(addresses);

    // Distribute 100 times, once per address
    for _ in 0..100 {
        tracker.get_next_addresses(1);
    }

    let stats = tracker.get_distribution_stats();
    println!("Fairness test stats: {}", stats);

    // Validate distribution fairness (each address should receive approximately 50 times)
    assert!(stats.contains("min=50"));
    assert!(stats.contains("max=50"));
    assert!(stats.contains("total=100"));
}

#[test]
fn test_address_distribution_with_large_outputs() {
    use tondi_addresses::{Address, Prefix, Version};

    let addresses = vec![
        Address::new(Prefix::Devnet, Version::PubKey, &[1; 32]),
        Address::new(Prefix::Devnet, Version::PubKey, &[2; 32]),
        Address::new(Prefix::Devnet, Version::PubKey, &[3; 32]),
    ];

    let mut tracker = AddressDistributionTracker::new(addresses);

    // Test 5 outputs per transaction
    let selected = tracker.get_next_addresses(5);
    assert_eq!(selected.len(), 5);

    // Validate distribution stats
    let stats = tracker.get_distribution_stats();
    assert!(stats.contains("total=5")); // Total distributed 5 times
}
