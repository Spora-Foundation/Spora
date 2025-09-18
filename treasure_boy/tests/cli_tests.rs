use std::io::Write;
use std::process::Command;
use tempfile::NamedTempFile;

#[test]
fn test_cli_help() {
    let output =
        Command::new("cargo").args(&["run", "--package", "treasure_boy", "--", "--help"]).output().expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("treasure_boy"));
    assert!(stdout.contains("--private-key"));
    assert!(stdout.contains("--tps"));
    assert!(stdout.contains("--address-file"));
    assert!(stdout.contains("--outputs-per-tx"));
}

#[test]
fn test_cli_version() {
    let output = Command::new("cargo")
        .args(&["run", "--package", "treasure_boy", "--", "--version"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("treasure_boy"));
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn test_cli_without_private_key() {
    let output = Command::new("cargo").args(&["run", "--package", "treasure_boy"]).output().expect("Failed to execute command");

    // Should display error message about missing private key
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Error: --private-key is required for transaction operations"));
    assert!(stderr.contains("Use --generate-addresses to generate random addresses"));
}

#[test]
fn test_cli_generate_addresses() {
    let output = Command::new("cargo")
        .args(&["run", "--package", "treasure_boy", "--", "--generate-addresses", "3"])
        .output()
        .expect("Failed to execute command");

    // Should display generated addresses
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Generated 3 addresses:"));
    assert!(stdout.contains("tonditest:"));
}

#[test]
fn test_cli_generate_addresses_to_file() {
    let temp_file = NamedTempFile::new().unwrap();
    let temp_path = temp_file.path().to_str().unwrap();

    let output = Command::new("cargo")
        .args(&["run", "--package", "treasure_boy", "--", "--generate-addresses", "2", "--output-file", temp_path])
        .output()
        .expect("Failed to execute command");

    // Should save addresses to file
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Generated 2 addresses and saved to:"));

    // Check file contents
    let file_content = std::fs::read_to_string(format!("{temp_path}.addresses")).unwrap();
    let lines: Vec<&str> = file_content.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].starts_with("tonditest:"));
    assert!(lines[1].starts_with("tonditest:"));
}

#[test]
fn test_cli_with_address_file() {
    // Create temporary address file
    let mut temp_file = NamedTempFile::new().unwrap();
    let addresses_content = r#"# Test addresses
tonditest:qr556222uq03hzf3nvxfl45x3ek07lrh7tp88xw2eh6tpuw2m9qs5e8tzc8
tonditest:qpl979v8dyhfw8v2d7x5rwre5ghph9d8jy0z8md06dnldkark3fs6wntgqx
"#;

    temp_file.write_all(addresses_content.as_bytes()).unwrap();
    temp_file.flush().unwrap();

    let output = Command::new("cargo")
        .args(&[
            "run",
            "--package",
            "treasure_boy",
            "--",
            "--private-key",
            "c99b1ccf1087af2a56ffedb885943962e0159a7705cac583eef3e9958cd035b3",
            "--address-file",
            temp_file.path().to_str().unwrap(),
            "--outputs-per-tx",
            "2",
            "--tps",
            "1",
        ])
        .output()
        .expect("Failed to execute command");

    // Should display loaded addresses information
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Loaded 2 addresses from file"));
    assert!(stdout.contains("batch airdrop to 2 addresses"));
    assert!(stdout.contains("outputs per tx: 2"));
}

#[test]
fn test_cli_with_single_address() {
    let output = Command::new("cargo")
        .args(&[
            "run",
            "--package",
            "treasure_boy",
            "--",
            "--private-key",
            "c99b1ccf1087af2a56ffedb885943962e0159a7705cac583eef3e9958cd035b3",
            "--to-addr",
            "tonditest:qr556222uq03hzf3nvxfl45x3ek07lrh7tp88xw2eh6tpuw2m9qs5e8tzc8",
            "--tps",
            "5",
            "--threads",
            "4",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("to address: tonditest:qr556222uq03hzf3nvxfl45x3ek07lrh7tp88xw2eh6tpuw2m9qs5e8tzc8"));
}

#[test]
fn test_cli_with_priority_fee() {
    let output = Command::new("cargo")
        .args(&[
            "run",
            "--package",
            "treasure_boy",
            "--",
            "--private-key",
            "c99b1ccf1087af2a56ffedb885943962e0159a7705cac583eef3e9958cd035b3",
            "--priority-fee",
            "1000",
            "--randomize-fee",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("priority fee: 1000 SOMPS [randomize]"));
}

#[test]
fn test_cli_invalid_arguments() {
    let output = Command::new("cargo")
        .args(&["run", "--package", "treasure_boy", "--", "--invalid-arg"])
        .output()
        .expect("Failed to execute command");

    // Should display error information
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unexpected argument"));
}

#[test]
fn test_cli_default_values() {
    let output = Command::new("cargo")
        .args(&[
            "run",
            "--package",
            "treasure_boy",
            "--",
            "--private-key",
            "c99b1ccf1087af2a56ffedb885943962e0159a7705cac583eef3e9958cd035b3",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Validate default values
    assert!(stdout.contains("localhost:16210")); // Default RPC server
                                                 // Other default values will be displayed in logs
}

#[test]
fn test_cli_unleashed_mode() {
    let output = Command::new("cargo")
        .args(&[
            "run",
            "--package",
            "treasure_boy",
            "--",
            "--private-key",
            "c99b1ccf1087af2a56ffedb885943962e0159a7705cac583eef3e9958cd035b3",
            "--unleashed",
            "--tps",
            "1000",
        ])
        .output()
        .expect("Failed to execute command");

    // unleashed mode allows higher TPS
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should be able to handle high TPS without errors
    assert!(stdout.contains("Using Treasure Boy with") || stdout.contains("Generated private key"));
}
