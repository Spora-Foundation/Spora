use std::io::Write;
use std::process::Command;
use tempfile::NamedTempFile;

fn treasure_boy_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_treasure_boy"))
}

#[test]
fn test_cli_help() {
    let output = treasure_boy_command().arg("--help").output().expect("Failed to execute command");

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
    let output = treasure_boy_command().arg("--version").output().expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("treasure_boy"));
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn test_cli_without_private_key() {
    let output = treasure_boy_command().output().expect("Failed to execute command");

    // Should display error message about missing private key
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Error: --private-key is required for transaction operations"));
    assert!(stderr.contains("Use --generate-addresses to generate random addresses"));
}

#[test]
fn test_cli_generate_addresses() {
    let output = treasure_boy_command()
        .args(["--generate-addresses", "3"])
        .output()
        .expect("Failed to execute command");

    // Should display generated addresses
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Generated 3 addresses:"));
    assert!(stdout.contains("spora0:"));
}

#[test]
fn test_cli_generate_addresses_to_file() {
    let temp_file = NamedTempFile::new().unwrap();
    let temp_path = temp_file.path().to_str().unwrap();

    let output = treasure_boy_command()
        .args(["--generate-addresses", "2", "--output-file", temp_path])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());

    // Check file contents
    let file_content = std::fs::read_to_string(format!("{temp_path}.addresses")).unwrap();
    let lines: Vec<&str> = file_content.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].starts_with("spora0:"));
    assert!(lines[1].starts_with("spora0:"));
}

#[test]
fn test_cli_with_address_file() {
    // Create temporary address file
    let mut temp_file = NamedTempFile::new().unwrap();
    let addresses_content = r#"# Test addresses
spora0:qrgqpkue0tzhmqd77tljdhwjc757hc26uestam0gc4kycjx4k8uu6zn7sl0
spora0:qqmquth4lyayewfl32pj8w9w9dpzqk6c9ngyp4xxmyqusxruhjm0jr4ssye
"#;

    temp_file.write_all(addresses_content.as_bytes()).unwrap();
    temp_file.flush().unwrap();

    let output = treasure_boy_command()
        .args([
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

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("Invalid address"));
    assert!(stderr.contains("failed to connect to the RPC server") || stderr.contains("Connection refused"));
}

#[test]
fn test_cli_with_single_address() {
    let output = treasure_boy_command()
        .args([
            "--private-key",
            "c99b1ccf1087af2a56ffedb885943962e0159a7705cac583eef3e9958cd035b3",
            "--to-addr",
            "sporadev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvs63gd8",
            "--tps",
            "5",
            "--threads",
            "4",
        ])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("Invalid address"));
    assert!(stderr.contains("failed to connect to the RPC server") || stderr.contains("Connection refused"));
}

#[test]
fn test_cli_with_priority_fee() {
    let output = treasure_boy_command()
        .args([
            "--private-key",
            "c99b1ccf1087af2a56ffedb885943962e0159a7705cac583eef3e9958cd035b3",
            "--priority-fee",
            "1000",
            "--randomize-fee",
        ])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("failed to connect to the RPC server") || stderr.contains("Connection refused"));
}

#[test]
fn test_cli_invalid_arguments() {
    let output = treasure_boy_command().arg("--invalid-arg").output().expect("Failed to execute command");

    // Should display error information
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--invalid-arg"));
    assert!(stderr.contains("Usage:") || stderr.contains("Usage:\n"));
}

#[test]
fn test_cli_default_values() {
    let output = treasure_boy_command()
        .args([
            "--private-key",
            "c99b1ccf1087af2a56ffedb885943962e0159a7705cac583eef3e9958cd035b3",
        ])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("127.0.0.1:16210") || stderr.contains("Connection refused"));
}

#[test]
fn test_cli_unleashed_mode() {
    let output = treasure_boy_command()
        .args([
            "--private-key",
            "c99b1ccf1087af2a56ffedb885943962e0159a7705cac583eef3e9958cd035b3",
            "--unleashed",
            "--tps",
            "1000",
        ])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("failed to connect to the RPC server") || stderr.contains("Connection refused"));
}
