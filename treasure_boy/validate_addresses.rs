use tondi_addresses::{Address, Prefix, Version};

fn main() {
    let addresses = vec![
        "tondidev:qqngj6p2pxkz9357ulz8c2m5te2cx3try84ygdn9gv6ndz40x0au72j3wrf",
        "tondidev:qz3neekcudvfskhuvd44vuktjnxhn7wp9aakla64mnkxtwazfp67xauxsl6",
        "tondidev:qq99etejdhpt0x7tuy2padkhpql8y5jmhmtrlk3ysa7k9h7l2wtsg8s25uh",
        "tondidev:qpsp88ykd58wnrdsuvszn6jmenyvajp3su6z8dc3gjs2eghwnzzc2pljk8y",
        "tondidev:qqc40udyus2xvue9xfl888xsef38csnyh54mednp8pzcq6zkj5yr55l7wue",
    ];

    println!("Validating generated addresses:");
    println!("================================");

    for (i, addr_str) in addresses.iter().enumerate() {
        match Address::try_from(*addr_str) {
            Ok(addr) => {
                println!("✅ Address {}: {}", i + 1, addr_str);
                println!("   Prefix: {:?}", addr.prefix());
                println!("   Version: {:?}", addr.version());
                println!("   Script: {:?}", addr.script());
            }
            Err(e) => {
                println!("❌ Address {}: {} - INVALID: {}", i + 1, addr_str, e);
            }
        }
        println!();
    }
}
