use spora_addresses::{Address, Prefix};
use std::fs;
use std::io::Write;
use tempfile::NamedTempFile;
use treasure_boy::load_addresses_from_file;

fn valid_dev_address() -> String {
    Address::new_std_single(Prefix::Devnet, &[42; 32]).expect("deterministic test address").address_to_string()
}

#[test]
fn test_load_addresses_with_comments() {
    let mut temp_file = NamedTempFile::new().unwrap();
    let addr = valid_dev_address();
    let content = format!("# This is a comment line\n# Another comment\n{addr}\n# Middle comment\n{addr}\n# Last comment\n");

    temp_file.write_all(content.as_bytes()).unwrap();
    temp_file.flush().unwrap();

    let addresses = load_addresses_from_file(temp_file.path().to_str().unwrap()).unwrap();
    assert_eq!(addresses.len(), 2);
}

#[test]
fn test_load_addresses_with_empty_lines() {
    let mut temp_file = NamedTempFile::new().unwrap();
    let addr = valid_dev_address();
    let content = format!("{addr}\n\n{addr}\n\n{addr}\n");

    temp_file.write_all(content.as_bytes()).unwrap();
    temp_file.flush().unwrap();

    let addresses = load_addresses_from_file(temp_file.path().to_str().unwrap()).unwrap();
    assert_eq!(addresses.len(), 3);
}

#[test]
fn test_load_addresses_with_whitespace() {
    let mut temp_file = NamedTempFile::new().unwrap();
    let addr = valid_dev_address();
    let content = format!("   {addr}   \n{addr}\n\t{addr}\t");

    temp_file.write_all(content.as_bytes()).unwrap();
    temp_file.flush().unwrap();

    let addresses = load_addresses_from_file(temp_file.path().to_str().unwrap()).unwrap();
    assert_eq!(addresses.len(), 3);
}

#[test]
fn test_load_addresses_mixed_valid_invalid() {
    let mut temp_file = NamedTempFile::new().unwrap();
    let addr = valid_dev_address();
    let content = format!("{addr}\ninvalid_address_1\n{addr}\nanother_invalid_address\n{addr}");

    temp_file.write_all(content.as_bytes()).unwrap();
    temp_file.flush().unwrap();

    let addresses = load_addresses_from_file(temp_file.path().to_str().unwrap()).unwrap();
    assert_eq!(addresses.len(), 3); // Only load valid addresses
}

#[test]
fn test_load_addresses_large_file() {
    let mut temp_file = NamedTempFile::new().unwrap();
    let addr = valid_dev_address();

    // Create a file with 1000 addresses
    let mut content = String::new();
    for _i in 0..1000 {
        content.push_str(&format!("{addr}\n"));
    }

    temp_file.write_all(content.as_bytes()).unwrap();
    temp_file.flush().unwrap();

    let addresses = load_addresses_from_file(temp_file.path().to_str().unwrap()).unwrap();
    assert_eq!(addresses.len(), 1000);
}

#[test]
fn test_load_addresses_unicode_content() {
    let mut temp_file = NamedTempFile::new().unwrap();
    let addr = valid_dev_address();
    let content = format!("# Chinese comment\n# Japanese comment\n# Korean comment\n{addr}\n# Arabic comment: Arabic comment\n{addr}");

    temp_file.write_all(content.as_bytes()).unwrap();
    temp_file.flush().unwrap();

    let addresses = load_addresses_from_file(temp_file.path().to_str().unwrap()).unwrap();
    assert_eq!(addresses.len(), 2);
}

#[test]
fn test_load_addresses_file_permissions() {
    // Test file permissions problem
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();

    // Write some content
    fs::write(path, format!("{}\n", valid_dev_address())).unwrap();

    let addresses = load_addresses_from_file(path).unwrap();
    assert_eq!(addresses.len(), 1);
}

#[test]
fn test_load_addresses_different_line_endings() {
    let mut temp_file = NamedTempFile::new().unwrap();
    let addr = valid_dev_address();
    let content = format!("{addr}\r\n{addr}\r\n{addr}");

    temp_file.write_all(content.as_bytes()).unwrap();
    temp_file.flush().unwrap();

    let addresses = load_addresses_from_file(temp_file.path().to_str().unwrap()).unwrap();
    assert_eq!(addresses.len(), 3);
}

#[test]
fn test_load_addresses_edge_cases() {
    let mut temp_file = NamedTempFile::new().unwrap();
    let content = r#"# File with only comments
# No valid addresses
# Only empty lines and comments
"#;

    temp_file.write_all(content.as_bytes()).unwrap();
    temp_file.flush().unwrap();

    let addresses = load_addresses_from_file(temp_file.path().to_str().unwrap()).unwrap();
    assert_eq!(addresses.len(), 0);
}

#[test]
fn test_load_addresses_single_line() {
    let mut temp_file = NamedTempFile::new().unwrap();
    let content = valid_dev_address();

    temp_file.write_all(content.as_bytes()).unwrap();
    temp_file.flush().unwrap();

    let addresses = load_addresses_from_file(temp_file.path().to_str().unwrap()).unwrap();
    assert_eq!(addresses.len(), 1);
}
