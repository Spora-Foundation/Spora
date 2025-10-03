pub mod error;
pub mod tracker;

pub mod test_helpers {
    use tondi_addresses::Address;
    use tondi_addresses::{Prefix, Version};

    pub const ADDRESS_PREFIX: Prefix = Prefix::Mainnet;

    pub fn get_3_addresses(sorted: bool) -> Vec<Address> {
        let mut addresses = vec![
            Address::new(ADDRESS_PREFIX, Version::PubKey, &[1u8; 32]).expect("Valid address"),
            Address::new(ADDRESS_PREFIX, Version::PubKey, &[2u8; 32]).expect("Valid address"),
            Address::new(ADDRESS_PREFIX, Version::PubKey, &[0u8; 32]).expect("Valid address"),
        ];
        if sorted {
            addresses.sort()
        }
        addresses
    }
}
