use tondi_wallet_core::tests::keys::make_xpub;

fn main() {
    match make_xpub() {
        Ok(xpub) => println!("Success: {:?}", xpub),
        Err(e) => println!("Error: {}", e),
    }
}
