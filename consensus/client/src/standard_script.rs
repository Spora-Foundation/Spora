use crate::{error::Error, result::Result};
use spora_addresses::{Address, Prefix, Version};
use spora_consensus_core::tx::ScriptRef;

const OP_DATA32: u8 = 0x20;
const OP_DATA33: u8 = 0x21;
const OP_EQUAL: u8 = 0x87;
const OP_BLAKE3: u8 = 0xaa;
const OP_CHECK_SIG_ECDSA: u8 = 0xab;
const OP_CHECK_SIG: u8 = 0xac;

#[cfg(any(feature = "wasm32-sdk", test))]
const OP_FALSE: u8 = 0x00;
#[cfg(any(feature = "wasm32-sdk", test))]
const OP_1_NEGATE: u8 = 0x4f;
#[cfg(any(feature = "wasm32-sdk", test))]
const OP_TRUE: u8 = 0x51;
#[cfg(any(feature = "wasm32-sdk", test))]
const OP_PUSH_DATA1: u8 = 0x4c;
#[cfg(any(feature = "wasm32-sdk", test))]
const OP_PUSH_DATA2: u8 = 0x4d;
#[cfg(any(feature = "wasm32-sdk", test))]
const OP_PUSH_DATA4: u8 = 0x4e;

#[cfg(any(feature = "wasm32-sdk", test))]
const MAX_SCRIPT_ELEMENT_SIZE: usize = 520;
#[cfg(any(feature = "wasm32-sdk", test))]
const MAX_SCRIPT_SIZE: usize = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockScriptClass {
    NonStandard,
    PubKey,
    PubKeyECDSA,
    ScriptHash,
}

pub fn is_pay_to_pubkey(script_public_key: &[u8]) -> bool {
    (script_public_key.len() == 34) && (script_public_key[0] == OP_DATA32) && (script_public_key[33] == OP_CHECK_SIG)
}

pub fn is_pay_to_pubkey_ecdsa(script_public_key: &[u8]) -> bool {
    (script_public_key.len() == 35) && (script_public_key[0] == OP_DATA33) && (script_public_key[34] == OP_CHECK_SIG_ECDSA)
}

pub fn is_pay_to_script_hash(script_public_key: &[u8]) -> bool {
    (script_public_key.len() == 35)
        && (script_public_key[0] == OP_BLAKE3)
        && (script_public_key[1] == OP_DATA32)
        && (script_public_key[34] == OP_EQUAL)
}

pub fn classify_lock_script(script: &[u8]) -> LockScriptClass {
    if is_pay_to_pubkey(script) {
        LockScriptClass::PubKey
    } else if is_pay_to_pubkey_ecdsa(script) {
        LockScriptClass::PubKeyECDSA
    } else if is_pay_to_script_hash(script) {
        LockScriptClass::ScriptHash
    } else {
        LockScriptClass::NonStandard
    }
}

pub fn pay_to_address_lock_script(address: &Address) -> ScriptRef {
    let script = match address.version {
        Version::PubKey => pay_to_pub_key(address.payload.as_slice()),
        Version::PubKeyECDSA => pay_to_pub_key_ecdsa(address.payload.as_slice()),
        Version::ScriptHash => pay_to_script_hash(address.payload.as_slice()),
    };
    ScriptRef::new(compute_lock_hash(&script), 0, script)
}

pub fn pay_to_script_hash_lock_script(redeem_script: &[u8]) -> ScriptRef {
    let redeem_script_hash = blake3::hash(redeem_script);
    let script = pay_to_script_hash(redeem_script_hash.as_bytes());
    ScriptRef::new(compute_lock_hash(&script), 0, script)
}

#[cfg(any(feature = "wasm32-sdk", test))]
pub fn pay_to_script_hash_witness_script(redeem_script: &[u8], signature: Vec<u8>) -> Result<Vec<u8>> {
    let mut script = Vec::new();
    push_data(&mut script, &signature)?;
    push_data(&mut script, redeem_script)?;
    Ok(script)
}

pub fn extract_address_from_lock_script(lock_script: &[u8], prefix: Prefix) -> Result<Address> {
    match classify_lock_script(lock_script) {
        LockScriptClass::NonStandard => Err(Error::custom("non-standard lock script")),
        LockScriptClass::PubKey => Ok(Address::new(prefix, Version::PubKey, &lock_script[1..33])?),
        LockScriptClass::PubKeyECDSA => Ok(Address::new(prefix, Version::PubKeyECDSA, &lock_script[1..34])?),
        LockScriptClass::ScriptHash => Ok(Address::new(prefix, Version::ScriptHash, &lock_script[2..34])?),
    }
}

fn pay_to_pub_key(address_payload: &[u8]) -> Vec<u8> {
    assert_eq!(address_payload.len(), 32);
    let mut script = Vec::with_capacity(34);
    script.push(OP_DATA32);
    script.extend_from_slice(address_payload);
    script.push(OP_CHECK_SIG);
    script
}

fn pay_to_pub_key_ecdsa(address_payload: &[u8]) -> Vec<u8> {
    assert_eq!(address_payload.len(), 33);
    let mut script = Vec::with_capacity(35);
    script.push(OP_DATA33);
    script.extend_from_slice(address_payload);
    script.push(OP_CHECK_SIG_ECDSA);
    script
}

fn pay_to_script_hash(script_hash: &[u8]) -> Vec<u8> {
    assert_eq!(script_hash.len(), 32);
    let mut script = Vec::with_capacity(35);
    script.push(OP_BLAKE3);
    script.push(OP_DATA32);
    script.extend_from_slice(script_hash);
    script.push(OP_EQUAL);
    script
}

fn compute_lock_hash(script: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"spora-cell/lock");
    hasher.update(&0u16.to_le_bytes());
    hasher.update(script);
    *hasher.finalize().as_bytes()
}

#[cfg(any(feature = "wasm32-sdk", test))]
fn push_data(script: &mut Vec<u8>, data: &[u8]) -> Result<()> {
    let data_len = data.len();
    if data_len > MAX_SCRIPT_ELEMENT_SIZE {
        return Err(Error::ScriptEncoding(format!("script element exceeds max size: {data_len} > {MAX_SCRIPT_ELEMENT_SIZE}")));
    }

    let data_size = canonical_data_size(data);
    if script.len() + data_size > MAX_SCRIPT_SIZE {
        return Err(Error::ScriptEncoding(format!("script exceeds max size: {} > {}", script.len() + data_size, MAX_SCRIPT_SIZE)));
    }

    if data_len == 0 || (data_len == 1 && data[0] == 0) {
        script.push(OP_FALSE);
        return Ok(());
    }
    if data_len == 1 && data[0] <= 16 {
        script.push((OP_TRUE - 1) + data[0]);
        return Ok(());
    }
    if data_len == 1 && data[0] == 0x81 {
        script.push(OP_1_NEGATE);
        return Ok(());
    }

    if data_len <= 75 {
        script.push(data_len as u8);
    } else if data_len <= u8::MAX as usize {
        script.push(OP_PUSH_DATA1);
        script.push(data_len as u8);
    } else if data_len <= u16::MAX as usize {
        script.push(OP_PUSH_DATA2);
        script.extend_from_slice(&(data_len as u16).to_le_bytes());
    } else {
        script.push(OP_PUSH_DATA4);
        script.extend_from_slice(&(data_len as u32).to_le_bytes());
    }

    script.extend_from_slice(data);
    Ok(())
}

#[cfg(any(feature = "wasm32-sdk", test))]
fn canonical_data_size(data: &[u8]) -> usize {
    let data_len = data.len();
    if data_len == 0 || (data_len == 1 && (data[0] <= 16 || data[0] == 0x81)) {
        return 1;
    }

    data_len
        + if data_len <= 75 {
            1
        } else if data_len <= u8::MAX as usize {
            2
        } else if data_len <= u16::MAX as usize {
            3
        } else {
            5
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_address_scripts_roundtrip() {
        let address = Address::new(Prefix::Testnet, Version::PubKey, &[0x11; 32]).unwrap();
        let lock_script = pay_to_address_lock_script(&address);
        let decoded = extract_address_from_lock_script(&lock_script.args, Prefix::Testnet).unwrap();
        assert_eq!(decoded, address);
        assert_eq!(classify_lock_script(&lock_script.args), LockScriptClass::PubKey);
    }

    #[test]
    fn p2sh_witness_script_uses_canonical_pushes() {
        let redeem_script = vec![0x51];
        let signature = vec![0x33; 64];
        let script = pay_to_script_hash_witness_script(&redeem_script, signature.clone()).unwrap();

        assert_eq!(script[0], 64);
        assert_eq!(&script[1..65], signature.as_slice());
        assert_eq!(script[65], 1);
        assert_eq!(script[66], OP_TRUE);
    }
}
