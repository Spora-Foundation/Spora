use std::fs;
use std::io::Write;
use tempfile::NamedTempFile;
use treasure_boy::load_addresses_from_file;

#[test]
fn test_load_addresses_with_comments() {
    let mut temp_file = NamedTempFile::new().unwrap();
    let content = r#"# This is a comment line
# Another comment
tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6
# Middle comment
tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6
# Last comment
"#;

    temp_file.write_all(content.as_bytes()).unwrap();
    temp_file.flush().unwrap();

    let addresses = load_addresses_from_file(temp_file.path().to_str().unwrap()).unwrap();
    assert_eq!(addresses.len(), 2);
}

#[test]
fn test_load_addresses_with_empty_lines() {
    let mut temp_file = NamedTempFile::new().unwrap();
    let content = r#"tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6

tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6

tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6
"#;

    temp_file.write_all(content.as_bytes()).unwrap();
    temp_file.flush().unwrap();

    let addresses = load_addresses_from_file(temp_file.path().to_str().unwrap()).unwrap();
    assert_eq!(addresses.len(), 3);
}

#[test]
fn test_load_addresses_with_whitespace() {
    let mut temp_file = NamedTempFile::new().unwrap();
    let content = r#"   tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6   
tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6
	tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6	"#;

    temp_file.write_all(content.as_bytes()).unwrap();
    temp_file.flush().unwrap();

    let addresses = load_addresses_from_file(temp_file.path().to_str().unwrap()).unwrap();
    assert_eq!(addresses.len(), 3);
}

#[test]
fn test_load_addresses_mixed_valid_invalid() {
    let mut temp_file = NamedTempFile::new().unwrap();
    let content = r#"tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6
invalid_address_1
tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6
another_invalid_address
tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6"#;

    temp_file.write_all(content.as_bytes()).unwrap();
    temp_file.flush().unwrap();

    let addresses = load_addresses_from_file(temp_file.path().to_str().unwrap()).unwrap();
    assert_eq!(addresses.len(), 3); // Only load valid addresses
}

#[test]
fn test_load_addresses_large_file() {
    let mut temp_file = NamedTempFile::new().unwrap();

    // Create a file with 1000 addresses
    let mut content = String::new();
    for _i in 0..1000 {
        content.push_str(&format!("tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6\n"));
    }

    temp_file.write_all(content.as_bytes()).unwrap();
    temp_file.flush().unwrap();

    let addresses = load_addresses_from_file(temp_file.path().to_str().unwrap()).unwrap();
    assert_eq!(addresses.len(), 1000);
}

#[test]
fn test_load_addresses_unicode_content() {
    let mut temp_file = NamedTempFile::new().unwrap();
    let content = r#"# Chinese comment
# Japanese comment
# Korean comment
tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6
# Arabic comment: Arabic comment
tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6"#;

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
    fs::write(path, "tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6\n").unwrap();

    let addresses = load_addresses_from_file(path).unwrap();
    assert_eq!(addresses.len(), 1);
}

#[test]
fn test_load_addresses_different_line_endings() {
    let mut temp_file = NamedTempFile::new().unwrap();
    let content = "tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6\r\ntondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6\r\ntondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6";

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
    let content = "tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6";

    temp_file.write_all(content.as_bytes()).unwrap();
    temp_file.flush().unwrap();

    let addresses = load_addresses_from_file(temp_file.path().to_str().unwrap()).unwrap();
    assert_eq!(addresses.len(), 1);
}
