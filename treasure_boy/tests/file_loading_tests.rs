use std::fs;
use std::io::Write;
use tempfile::NamedTempFile;
use treasure_boy::load_addresses_from_file;

#[test]
fn test_load_addresses_with_comments() {
    let mut temp_file = NamedTempFile::new().unwrap();
    let content = r#"# 这是注释行
# 另一个注释
tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6
# 中间注释
tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6
# 最后注释
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
    assert_eq!(addresses.len(), 3); // 只应该加载有效地址
}

#[test]
fn test_load_addresses_large_file() {
    let mut temp_file = NamedTempFile::new().unwrap();
    
    // 创建包含1000个地址的大文件
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
    let content = r#"# 中文注释
# 日本語コメント
# 한국어 주석
tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6
# Arabic comment: تعليق عربي
tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6"#;
    
    temp_file.write_all(content.as_bytes()).unwrap();
    temp_file.flush().unwrap();
    
    let addresses = load_addresses_from_file(temp_file.path().to_str().unwrap()).unwrap();
    assert_eq!(addresses.len(), 2);
}

#[test]
fn test_load_addresses_file_permissions() {
    // 测试文件权限问题
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();
    
    // 写入一些内容
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
    let content = r#"# 只有注释的文件
# 没有有效地址
# 只有空行和注释
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
