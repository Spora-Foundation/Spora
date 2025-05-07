fn main() {
    let hash_hex = "46ecf45be3baca349dfe8a78deaf053b0aa6d538974da50fd6efb4d266bc8d21";

    // 将十六进制哈希字符串转为字节数组
    let bytes = hex::decode(hash_hex).expect("Invalid hex string");

    // 转换为 0x 格式
    let formatted_hash = bytes.iter()
        .map(|byte| format!("0x{:02x}", byte))
        .collect::<Vec<String>>()
        .join(", ");

    println!("Formatted hash: {}", formatted_hash);
}