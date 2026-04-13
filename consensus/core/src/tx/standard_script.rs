use super::ScriptRef;
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use spora_addresses::{Address, Prefix, Version};
use std::{
    borrow::Borrow,
    fmt::{Display, Formatter},
    str::FromStr,
};
use thiserror::Error;

const OP_FALSE: u8 = 0x00;
const OP_1_NEGATE: u8 = 0x4f;
const OP_TRUE: u8 = 0x51;
const OP_PUSH_DATA1: u8 = 0x4c;
const OP_PUSH_DATA2: u8 = 0x4d;
const OP_PUSH_DATA4: u8 = 0x4e;
#[cfg(test)]
const OP_RETURN: u8 = 0x6a;
const OP_DATA32: u8 = 0x20;
const OP_DATA33: u8 = 0x21;
const OP_EQUAL: u8 = 0x87;
const OP_BLAKE3: u8 = 0xaa;
const OP_CHECK_MULTI_SIG_ECDSA: u8 = 0xa9;
const OP_CHECK_SIG_ECDSA: u8 = 0xab;
const OP_CHECK_SIG: u8 = 0xac;
#[cfg(test)]
const OP_CHECK_SIG_VERIFY: u8 = 0xad;
const OP_CHECK_MULTI_SIG: u8 = 0xae;
#[cfg(test)]
const OP_CHECK_MULTI_SIG_VERIFY: u8 = 0xaf;
#[cfg(test)]
const OP_SMALL_INT_MAX: u8 = 0x60;

const MAX_SCRIPT_ELEMENT_SIZE: usize = 520;
const MAX_SCRIPT_SIZE: usize = 10_000;
const MAX_PUB_KEYS_PER_MULTISIG: u64 = 20;

const NON_STANDARD: &str = "nonstandard";
const PUB_KEY: &str = "pubkey";
const PUB_KEY_ECDSA: &str = "pubkeyecdsa";
const SCRIPT_HASH: &str = "scripthash";

#[derive(Error, PartialEq, Eq, Debug, Clone)]
pub enum StandardScriptError {
    #[error("invalid script class {0}")]
    InvalidScriptClass(String),

    #[error("non-standard lock script")]
    NonStandardLockScript,

    #[error("{0}")]
    ScriptEncoding(String),

    #[error(transparent)]
    Address(#[from] spora_addresses::AddressError),
}

#[derive(Error, PartialEq, Eq, Debug, Clone)]
pub enum MultisigRedeemScriptError {
    #[error("too many required signatures")]
    TooManyRequiredSignatures,

    #[error("provided public keys should not be empty")]
    EmptyKeys,

    #[error(transparent)]
    StandardScript(#[from] StandardScriptError),
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum ScriptClass {
    NonStandard = 0,
    PubKey,
    PubKeyECDSA,
    ScriptHash,
}

impl ScriptClass {
    #[inline(always)]
    pub fn is_pay_to_pubkey(lock_script: &[u8]) -> bool {
        (lock_script.len() == 34) && (lock_script[0] == OP_DATA32) && (lock_script[33] == OP_CHECK_SIG)
    }

    #[inline(always)]
    pub fn is_pay_to_pubkey_ecdsa(lock_script: &[u8]) -> bool {
        (lock_script.len() == 35) && (lock_script[0] == OP_DATA33) && (lock_script[34] == OP_CHECK_SIG_ECDSA)
    }

    #[inline(always)]
    pub fn is_pay_to_script_hash(lock_script: &[u8]) -> bool {
        (lock_script.len() == 35) && (lock_script[0] == OP_BLAKE3) && (lock_script[1] == OP_DATA32) && (lock_script[34] == OP_EQUAL)
    }

    fn as_str(&self) -> &'static str {
        match self {
            ScriptClass::NonStandard => NON_STANDARD,
            ScriptClass::PubKey => PUB_KEY,
            ScriptClass::PubKeyECDSA => PUB_KEY_ECDSA,
            ScriptClass::ScriptHash => SCRIPT_HASH,
        }
    }
}

impl Display for ScriptClass {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ScriptClass {
    type Err = StandardScriptError;

    fn from_str(script_class: &str) -> Result<Self, Self::Err> {
        match script_class {
            NON_STANDARD => Ok(ScriptClass::NonStandard),
            PUB_KEY => Ok(ScriptClass::PubKey),
            PUB_KEY_ECDSA => Ok(ScriptClass::PubKeyECDSA),
            SCRIPT_HASH => Ok(ScriptClass::ScriptHash),
            _ => Err(StandardScriptError::InvalidScriptClass(script_class.to_string())),
        }
    }
}

impl From<Version> for ScriptClass {
    fn from(value: Version) -> Self {
        match value {
            Version::PubKey => ScriptClass::PubKey,
            Version::PubKeyECDSA => ScriptClass::PubKeyECDSA,
            Version::ScriptHash => ScriptClass::ScriptHash,
        }
    }
}

impl From<&ScriptRef> for ScriptClass {
    fn from(lock_script: &ScriptRef) -> Self {
        classify_lock_script(lock_script.args.as_slice())
    }
}

pub fn classify_lock_script(lock_script: &[u8]) -> ScriptClass {
    if ScriptClass::is_pay_to_pubkey(lock_script) {
        ScriptClass::PubKey
    } else if ScriptClass::is_pay_to_pubkey_ecdsa(lock_script) {
        ScriptClass::PubKeyECDSA
    } else if ScriptClass::is_pay_to_script_hash(lock_script) {
        ScriptClass::ScriptHash
    } else {
        ScriptClass::NonStandard
    }
}

pub fn pay_to_address_lock_script(address: &Address) -> ScriptRef {
    let lock_script = match address.version {
        Version::PubKey => pay_to_pub_key(address.payload.as_slice()),
        Version::PubKeyECDSA => pay_to_pub_key_ecdsa(address.payload.as_slice()),
        Version::ScriptHash => pay_to_script_hash(address.payload.as_slice()),
    };
    ScriptRef::new(compute_lock_hash(&lock_script), 0, lock_script)
}

pub fn pay_to_script_hash_lock_script(redeem_script: &[u8]) -> ScriptRef {
    let redeem_script_hash = blake3::hash(redeem_script);
    let lock_script = pay_to_script_hash(redeem_script_hash.as_bytes());
    ScriptRef::new(compute_lock_hash(&lock_script), 0, lock_script)
}

pub fn push_data_script(data: &[u8]) -> Result<Vec<u8>, StandardScriptError> {
    let mut script = Vec::new();
    push_data(&mut script, data)?;
    Ok(script)
}

pub fn pay_to_script_hash_witness_script(redeem_script: &[u8], signature: Vec<u8>) -> Result<Vec<u8>, StandardScriptError> {
    let mut script = Vec::new();
    push_data(&mut script, &signature)?;
    push_data(&mut script, redeem_script)?;
    Ok(script)
}

pub fn multisig_redeem_script(
    pub_keys: impl Iterator<Item = impl Borrow<[u8; 32]>>,
    required: usize,
) -> Result<Vec<u8>, MultisigRedeemScriptError> {
    if required > MAX_PUB_KEYS_PER_MULTISIG as usize || pub_keys.size_hint().1.is_some_and(|upper| upper < required) {
        return Err(MultisigRedeemScriptError::TooManyRequiredSignatures);
    }

    let mut script = Vec::new();
    push_script_int(&mut script, required as i64)?;

    let mut count = 0usize;
    for pub_key in pub_keys {
        count += 1;
        if count > MAX_PUB_KEYS_PER_MULTISIG as usize {
            return Err(MultisigRedeemScriptError::TooManyRequiredSignatures);
        }
        push_data(&mut script, pub_key.borrow().as_slice())?;
    }

    if count < required {
        return Err(MultisigRedeemScriptError::TooManyRequiredSignatures);
    }
    if count == 0 {
        return Err(MultisigRedeemScriptError::EmptyKeys);
    }

    push_script_int(&mut script, count as i64)?;
    script.push(OP_CHECK_MULTI_SIG);
    Ok(script)
}

pub fn multisig_redeem_script_ecdsa(
    pub_keys: impl Iterator<Item = impl Borrow<[u8; 33]>>,
    required: usize,
) -> Result<Vec<u8>, MultisigRedeemScriptError> {
    if required > MAX_PUB_KEYS_PER_MULTISIG as usize || pub_keys.size_hint().1.is_some_and(|upper| upper < required) {
        return Err(MultisigRedeemScriptError::TooManyRequiredSignatures);
    }

    let mut script = Vec::new();
    push_script_int(&mut script, required as i64)?;

    let mut count = 0usize;
    for pub_key in pub_keys {
        count += 1;
        if count > MAX_PUB_KEYS_PER_MULTISIG as usize {
            return Err(MultisigRedeemScriptError::TooManyRequiredSignatures);
        }
        push_data(&mut script, pub_key.borrow().as_slice())?;
    }

    if count < required {
        return Err(MultisigRedeemScriptError::TooManyRequiredSignatures);
    }
    if count == 0 {
        return Err(MultisigRedeemScriptError::EmptyKeys);
    }

    push_script_int(&mut script, count as i64)?;
    script.push(OP_CHECK_MULTI_SIG_ECDSA);
    Ok(script)
}

pub fn extract_address_from_lock_script(lock_script: &[u8], prefix: Prefix) -> Result<Address, StandardScriptError> {
    match classify_lock_script(lock_script) {
        ScriptClass::NonStandard => Err(StandardScriptError::NonStandardLockScript),
        ScriptClass::PubKey => Ok(Address::new(prefix, Version::PubKey, &lock_script[1..33])?),
        ScriptClass::PubKeyECDSA => Ok(Address::new(prefix, Version::PubKeyECDSA, &lock_script[1..34])?),
        ScriptClass::ScriptHash => Ok(Address::new(prefix, Version::ScriptHash, &lock_script[2..34])?),
    }
}

#[cfg(test)]
pub fn is_script_unspendable(script: &[u8]) -> bool {
    let mut offset = 0;
    let mut index = 0;
    while offset < script.len() {
        match parse_next_opcode(script, &mut offset) {
            Some(opcode) => {
                if index == 0 && opcode.code == OP_RETURN {
                    return true;
                }
            }
            None => return true,
        }
        index += 1;
    }
    false
}

#[cfg(test)]
pub fn get_witness_sig_op_count_upper_bound(witness_script: &[u8], prev_lock_script: &ScriptRef) -> u64 {
    if !ScriptClass::is_pay_to_script_hash(prev_lock_script.args.as_slice()) {
        return count_sig_ops(prev_lock_script.args.as_slice());
    }

    let mut offset = 0;
    let mut last_push = None;
    while offset < witness_script.len() {
        let Some(opcode) = parse_next_opcode(witness_script, &mut offset) else {
            return 0;
        };
        if !opcode.is_push() {
            return 0;
        }
        last_push = Some(opcode.data);
    }

    last_push.map_or(0, count_sig_ops)
}

#[cfg(test)]
#[derive(Clone, Copy)]
struct ParsedOpcode<'a> {
    code: u8,
    data: &'a [u8],
}

#[cfg(test)]
impl ParsedOpcode<'_> {
    fn is_push(self) -> bool {
        self.code <= OP_SMALL_INT_MAX
    }
}

#[cfg(test)]
fn count_sig_ops(script: &[u8]) -> u64 {
    let mut offset = 0;
    let mut count = 0;
    let mut previous = None;

    while offset < script.len() {
        let Some(opcode) = parse_next_opcode(script, &mut offset) else {
            break;
        };

        match opcode.code {
            OP_CHECK_SIG | OP_CHECK_SIG_ECDSA | OP_CHECK_SIG_VERIFY => count += 1,
            OP_CHECK_MULTI_SIG | OP_CHECK_MULTI_SIG_ECDSA | OP_CHECK_MULTI_SIG_VERIFY => {
                count += previous.and_then(as_small_int).map_or(MAX_PUB_KEYS_PER_MULTISIG, u64::from);
            }
            _ => {}
        }

        previous = Some(opcode.code);
    }

    count
}

#[cfg(test)]
fn parse_next_opcode<'a>(script: &'a [u8], offset: &mut usize) -> Option<ParsedOpcode<'a>> {
    let code = *script.get(*offset)?;
    *offset += 1;

    let data_len = match code {
        0x01..=0x4b => usize::from(code),
        OP_PUSH_DATA1 => usize::from(*script.get(*offset)?),
        OP_PUSH_DATA2 => {
            let bytes = script.get(*offset..(*offset + 2))?;
            usize::from(u16::from_le_bytes([bytes[0], bytes[1]]))
        }
        OP_PUSH_DATA4 => {
            let bytes = script.get(*offset..(*offset + 4))?;
            usize::try_from(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])).ok()?
        }
        _ => 0,
    };

    match code {
        OP_PUSH_DATA1 => *offset += 1,
        OP_PUSH_DATA2 => *offset += 2,
        OP_PUSH_DATA4 => *offset += 4,
        _ => {}
    }

    let data = script.get(*offset..(*offset + data_len))?;
    *offset += data_len;
    Some(ParsedOpcode { code, data })
}

#[cfg(test)]
fn as_small_int(opcode: u8) -> Option<u8> {
    if (OP_TRUE..=OP_SMALL_INT_MAX).contains(&opcode) {
        Some(opcode - (OP_TRUE - 1))
    } else {
        None
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

fn push_script_int(script: &mut Vec<u8>, val: i64) -> Result<(), StandardScriptError> {
    if script.len() + 1 > MAX_SCRIPT_SIZE {
        return Err(StandardScriptError::ScriptEncoding(format!(
            "script exceeds max size: {} > {}",
            script.len() + 1,
            MAX_SCRIPT_SIZE
        )));
    }

    if val == 0 {
        script.push(OP_FALSE);
        return Ok(());
    }
    if val == -1 || (1..=16).contains(&val) {
        script.push(((OP_TRUE as i64 - 1) + val) as u8);
        return Ok(());
    }

    push_data(script, &serialize_i64(val))
}

fn push_data(script: &mut Vec<u8>, data: &[u8]) -> Result<(), StandardScriptError> {
    let data_len = data.len();
    if data_len > MAX_SCRIPT_ELEMENT_SIZE {
        return Err(StandardScriptError::ScriptEncoding(format!(
            "script element exceeds max size: {data_len} > {MAX_SCRIPT_ELEMENT_SIZE}"
        )));
    }

    let data_size = canonical_data_size(data);
    if script.len() + data_size > MAX_SCRIPT_SIZE {
        return Err(StandardScriptError::ScriptEncoding(format!(
            "script exceeds max size: {} > {}",
            script.len() + data_size,
            MAX_SCRIPT_SIZE
        )));
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

fn serialize_i64(value: i64) -> Vec<u8> {
    let sign = value.signum();
    let mut positive = value.unsigned_abs();
    let mut last_saturated = false;
    let mut number = Vec::new();

    while positive != 0 || last_saturated {
        if positive == 0 {
            last_saturated = false;
            number.push(0);
            continue;
        }

        let byte = (positive & 0xff) as u8;
        last_saturated = (byte & 0x80) != 0;
        positive >>= 8;
        number.push(byte);
    }

    if sign == -1 {
        match number.last_mut() {
            Some(last) => *last |= 0x80,
            None => unreachable!("negative values always serialize to at least one byte"),
        }
    }

    number
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_address_scripts_roundtrip() {
        let address = Address::new(Prefix::Testnet, Version::PubKey, &[0x11; 32]).unwrap();
        let lock_script = pay_to_address_lock_script(&address);
        let decoded = extract_address_from_lock_script(lock_script.args.as_slice(), Prefix::Testnet).unwrap();
        assert_eq!(decoded, address);
        assert_eq!(classify_lock_script(lock_script.args.as_slice()), ScriptClass::PubKey);
    }

    #[test]
    fn script_unspendable_for_op_return_and_parse_errors() {
        assert!(is_script_unspendable(&[OP_RETURN]));
        assert!(is_script_unspendable(&[OP_PUSH_DATA1]));
        assert!(!is_script_unspendable(&[OP_TRUE]));
    }

    #[test]
    fn sig_op_counter_handles_p2pk_and_multisig() {
        let mut p2pk_args = vec![OP_DATA32];
        p2pk_args.extend_from_slice(&[0x11; 32]);
        p2pk_args.push(OP_CHECK_SIG);
        let p2pk = ScriptRef::new(compute_lock_hash(&p2pk_args), 0, p2pk_args);
        assert_eq!(get_witness_sig_op_count_upper_bound(&[], &p2pk), 1);

        let multisig_args = vec![OP_TRUE + 1, OP_CHECK_MULTI_SIG];
        let multisig = ScriptRef::new(compute_lock_hash(&multisig_args), 0, multisig_args);
        assert_eq!(get_witness_sig_op_count_upper_bound(&[], &multisig), 2);
    }

    #[test]
    fn sig_op_counter_reads_p2sh_redeem_script() {
        let redeem_script = vec![OP_TRUE + 2, OP_CHECK_MULTI_SIG];
        let p2sh = pay_to_script_hash_lock_script(&redeem_script);
        let mut witness_script = Vec::new();
        push_data(&mut witness_script, &[0x33; 64]).unwrap();
        push_data(&mut witness_script, &redeem_script).unwrap();
        assert_eq!(get_witness_sig_op_count_upper_bound(&witness_script, &p2sh), 3);
    }

    #[test]
    fn sig_op_counter_rejects_non_push_p2sh_witness_script() {
        let redeem_script = vec![OP_TRUE, OP_CHECK_MULTI_SIG];
        let p2sh = pay_to_script_hash_lock_script(&redeem_script);
        assert_eq!(get_witness_sig_op_count_upper_bound(&[OP_CHECK_SIG], &p2sh), 0);
    }

    #[test]
    fn push_data_script_encodes_canonical_pushes() {
        assert_eq!(push_data_script(&[0x11, 0x22]).unwrap(), vec![0x02, 0x11, 0x22]);
    }

    #[test]
    fn multisig_redeem_script_uses_small_int_encoding() {
        let script = multisig_redeem_script([[0x11; 32], [0x22; 32]].into_iter(), 2).unwrap();
        assert_eq!(script[0], OP_TRUE + 1);
        assert_eq!(script[1], OP_DATA32);
        assert_eq!(script[34], OP_DATA32);
        assert_eq!(script[67], OP_TRUE + 1);
        assert_eq!(script[68], OP_CHECK_MULTI_SIG);
    }

    #[test]
    fn multisig_redeem_script_supports_counts_above_small_int_range() {
        let keys = (0..17).map(|idx| [idx as u8; 32]);
        let script = multisig_redeem_script(keys, 17).unwrap();
        assert_eq!(script.last(), Some(&OP_CHECK_MULTI_SIG));
    }
}
