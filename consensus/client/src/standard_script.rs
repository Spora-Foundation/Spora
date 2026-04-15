use crate::{error::Error, result::Result};
use spora_addresses::{Address, Prefix};
use spora_consensus_core::tx::{self, Script};

pub type LockScriptClass = tx::ScriptClass;

pub fn classify_script(script: &Script) -> LockScriptClass {
    tx::classify_script(script)
}

pub fn pay_to_address_lock_script(address: &Address) -> Script {
    tx::pay_to_address_lock_script(address)
}

pub fn address_to_lock_script(address: &Address) -> Script {
    tx::address_to_lock_script(address)
}

pub fn address_to_builtin_standard_lock(address: &Address) -> Option<Script> {
    tx::address_to_builtin_standard_lock(address)
}

pub fn address_to_full_script_lock(address: &Address) -> Option<Script> {
    tx::address_to_full_script_lock(address)
}

pub fn extract_address_from_script(lock_script: &Script, prefix: Prefix) -> Result<Address> {
    tx::extract_address_from_script(lock_script, prefix).map_err(|err| Error::custom(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use spora_addresses::Version;

    #[test]
    fn standard_address_scripts_roundtrip() {
        let address = Address::new(Prefix::Testnet, Version::StdSingle, &[0x11; 20]).unwrap();
        let lock_script = pay_to_address_lock_script(&address);
        let decoded = extract_address_from_script(&lock_script, Prefix::Testnet).unwrap();
        assert_eq!(decoded, address);
        assert_eq!(classify_script(&lock_script), LockScriptClass::StdSingle);
    }
}
