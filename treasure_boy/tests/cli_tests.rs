use std::process::Command;
use tempfile::NamedTempFile;
use std::io::Write;

#[test]
fn test_cli_help() {
    let output = Command::new("cargo")
        .args(&["run", "--package", "treasure_boy", "--", "--help"])
        .output()
        .expect("Failed to execute command");
    
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
    assert!(stdout.contains("0.17.0"));
}

#[test]
fn test_cli_without_private_key() {
    let output = Command::new("cargo")
        .args(&["run", "--package", "treasure_boy"])
        .output()
        .expect("Failed to execute command");
    
    // 应该显示生成的私钥和地址信息
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Generated private key"));
    assert!(stdout.contains("Send some funds to this address"));
}

#[test]
fn test_cli_with_address_file() {
    // 创建临时地址文件
    let mut temp_file = NamedTempFile::new().unwrap();
    let addresses_content = r#"# 测试地址
tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6
tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6
"#;
    
    temp_file.write_all(addresses_content.as_bytes()).unwrap();
    temp_file.flush().unwrap();
    
    let output = Command::new("cargo")
        .args(&[
            "run", "--package", "treasure_boy", "--",
            "--private-key", "test_key",
            "--address-file", temp_file.path().to_str().unwrap(),
            "--outputs-per-tx", "2",
            "--tps", "1"
        ])
        .output()
        .expect("Failed to execute command");
    
    // 应该显示加载的地址信息
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Loaded 2 addresses from file"));
    assert!(stdout.contains("batch airdrop to 2 addresses"));
    assert!(stdout.contains("outputs per tx: 2"));
}

#[test]
fn test_cli_with_single_address() {
    let output = Command::new("cargo")
        .args(&[
            "run", "--package", "treasure_boy", "--",
            "--private-key", "test_key",
            "--to-addr", "tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6",
            "--tps", "5",
            "--threads", "4"
        ])
        .output()
        .expect("Failed to execute command");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("to address: tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6"));
}

#[test]
fn test_cli_with_priority_fee() {
    let output = Command::new("cargo")
        .args(&[
            "run", "--package", "treasure_boy", "--",
            "--private-key", "test_key",
            "--priority-fee", "1000",
            "--randomize-fee"
        ])
        .output()
        .expect("Failed to execute command");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("priority fee: 1000 SOMPS [randomize]"));
}

#[test]
fn test_cli_invalid_arguments() {
    let output = Command::new("cargo")
        .args(&[
            "run", "--package", "treasure_boy", "--",
            "--invalid-arg"
        ])
        .output()
        .expect("Failed to execute command");
    
    // 应该显示错误信息
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unexpected argument"));
}

#[test]
fn test_cli_default_values() {
    let output = Command::new("cargo")
        .args(&[
            "run", "--package", "treasure_boy", "--",
            "--private-key", "test_key"
        ])
        .output()
        .expect("Failed to execute command");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    // 验证默认值
    assert!(stdout.contains("localhost:16210")); // 默认 RPC 服务器
    // 其他默认值会在日志中显示
}

#[test]
fn test_cli_unleashed_mode() {
    let output = Command::new("cargo")
        .args(&[
            "run", "--package", "treasure_boy", "--",
            "--private-key", "test_key",
            "--unleashed",
            "--tps", "1000"
        ])
        .output()
        .expect("Failed to execute command");
    
    // unleashed 模式允许更高的 TPS
    let stdout = String::from_utf8_lossy(&output.stdout);
    // 应该能处理高 TPS 而不报错
    assert!(stdout.contains("treasure_boy") || stdout.contains("Generated private key"));
}
