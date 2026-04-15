use super::Script;
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
#[cfg(test)]
const OP_DATA32: u8 = 0x20;
const OP_CHECK_MULTI_SIG_ECDSA: u8 = 0xa9;
#[cfg(test)]
const OP_CHECK_SIG_ECDSA: u8 = 0xab;
#[cfg(test)]
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
const MAX_FULL_SCRIPT_ARGS_LEN: usize = 10_000;

const NON_STANDARD: &str = "nonstandard";
const STD_SINGLE: &str = "stdsingle";
const STD_SINGLE_ECDSA: &str = "stdsingleecdsa";
const ACCOUNT: &str = "account";
const FULL_SCRIPT: &str = "fullscript";

pub const HASH_TYPE_TYPE: u8 = 1;

const BUILTIN_CODE_HASH_DOMAIN: &[u8] = b"spora/builtin-lock/v1";
const BUILTIN_SCHNORR_BLAKE3_160_TAG: &[u8] = b"schnorr-blake3-160";
const BUILTIN_ECDSA_BLAKE3_160_TAG: &[u8] = b"ecdsa-blake3-160";
const BUILTIN_ACCOUNT_DESCRIPTOR_TAG: &[u8] = b"account-descriptor";

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
pub enum MultisigWitnessTemplateError {
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
    StdSingle = 3,
    StdSingleECDSA = 4,
    Account = 5,
    FullScript = 6,
}

impl ScriptClass {
    fn as_str(&self) -> &'static str {
        match self {
            ScriptClass::NonStandard => NON_STANDARD,
            ScriptClass::StdSingle => STD_SINGLE,
            ScriptClass::StdSingleECDSA => STD_SINGLE_ECDSA,
            ScriptClass::Account => ACCOUNT,
            ScriptClass::FullScript => FULL_SCRIPT,
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
            STD_SINGLE => Ok(ScriptClass::StdSingle),
            STD_SINGLE_ECDSA => Ok(ScriptClass::StdSingleECDSA),
            ACCOUNT => Ok(ScriptClass::Account),
            FULL_SCRIPT => Ok(ScriptClass::FullScript),
            _ => Err(StandardScriptError::InvalidScriptClass(script_class.to_string())),
        }
    }
}

impl From<Version> for ScriptClass {
    fn from(value: Version) -> Self {
        match value {
            Version::StdSingle => ScriptClass::StdSingle,
            Version::StdSingleECDSA => ScriptClass::StdSingleECDSA,
            Version::Account => ScriptClass::Account,
            Version::FullScript => ScriptClass::FullScript,
        }
    }
}

impl From<&Script> for ScriptClass {
    fn from(lock_script: &Script) -> Self {
        classify_script(lock_script)
    }
}

pub fn classify_script(script: &Script) -> ScriptClass {
    if script.hash_type == HASH_TYPE_TYPE
        && script.code_hash == builtin_code_hash(BUILTIN_SCHNORR_BLAKE3_160_TAG)
        && script.args.len() == 20
    {
        return ScriptClass::StdSingle;
    }
    if script.hash_type == HASH_TYPE_TYPE
        && script.code_hash == builtin_code_hash(BUILTIN_ECDSA_BLAKE3_160_TAG)
        && script.args.len() == 20
    {
        return ScriptClass::StdSingleECDSA;
    }
    if script.hash_type == HASH_TYPE_TYPE
        && script.code_hash == builtin_code_hash(BUILTIN_ACCOUNT_DESCRIPTOR_TAG)
        && script.args.len() == 32
    {
        return ScriptClass::Account;
    }
    if script.hash_type <= 4 {
        return ScriptClass::FullScript;
    }
    ScriptClass::NonStandard
}

pub fn address_to_builtin_standard_lock(address: &Address) -> Option<Script> {
    match address.version {
        Version::StdSingle => {
            Some(Script::new(builtin_code_hash(BUILTIN_SCHNORR_BLAKE3_160_TAG), HASH_TYPE_TYPE, address.payload.to_vec()))
        }
        Version::StdSingleECDSA => {
            Some(Script::new(builtin_code_hash(BUILTIN_ECDSA_BLAKE3_160_TAG), HASH_TYPE_TYPE, address.payload.to_vec()))
        }
        Version::Account => {
            Some(Script::new(builtin_code_hash(BUILTIN_ACCOUNT_DESCRIPTOR_TAG), HASH_TYPE_TYPE, address.payload.to_vec()))
        }
        Version::FullScript => None,
    }
}

pub fn address_to_full_script_lock(address: &Address) -> Option<Script> {
    match address.version {
        Version::FullScript => decode_full_script_payload(address.payload.as_slice()),
        _ => None,
    }
}

pub fn address_to_lock_script(address: &Address) -> Script {
    if let Some(script) = address_to_builtin_standard_lock(address) {
        return script;
    }
    if let Some(script) = address_to_full_script_lock(address) {
        return script;
    }
    Script::new([0u8; 32], 0, vec![])
}

pub fn pay_to_address_lock_script(address: &Address) -> Script {
    address_to_lock_script(address)
}

pub fn push_data_script(data: &[u8]) -> Result<Vec<u8>, StandardScriptError> {
    let mut script = Vec::new();
    push_data(&mut script, data)?;
    Ok(script)
}

pub fn multisig_witness_template(
    pub_keys: impl Iterator<Item = impl Borrow<[u8; 32]>>,
    required: usize,
) -> Result<Vec<u8>, MultisigWitnessTemplateError> {
    if required > MAX_PUB_KEYS_PER_MULTISIG as usize || pub_keys.size_hint().1.is_some_and(|upper| upper < required) {
        return Err(MultisigWitnessTemplateError::TooManyRequiredSignatures);
    }

    let mut script = Vec::new();
    push_script_int(&mut script, required as i64)?;

    let mut count = 0usize;
    for pub_key in pub_keys {
        count += 1;
        if count > MAX_PUB_KEYS_PER_MULTISIG as usize {
            return Err(MultisigWitnessTemplateError::TooManyRequiredSignatures);
        }
        push_data(&mut script, pub_key.borrow().as_slice())?;
    }

    if count < required {
        return Err(MultisigWitnessTemplateError::TooManyRequiredSignatures);
    }
    if count == 0 {
        return Err(MultisigWitnessTemplateError::EmptyKeys);
    }

    push_script_int(&mut script, count as i64)?;
    script.push(OP_CHECK_MULTI_SIG);
    Ok(script)
}

pub fn multisig_witness_template_ecdsa(
    pub_keys: impl Iterator<Item = impl Borrow<[u8; 33]>>,
    required: usize,
) -> Result<Vec<u8>, MultisigWitnessTemplateError> {
    if required > MAX_PUB_KEYS_PER_MULTISIG as usize || pub_keys.size_hint().1.is_some_and(|upper| upper < required) {
        return Err(MultisigWitnessTemplateError::TooManyRequiredSignatures);
    }

    let mut script = Vec::new();
    push_script_int(&mut script, required as i64)?;

    let mut count = 0usize;
    for pub_key in pub_keys {
        count += 1;
        if count > MAX_PUB_KEYS_PER_MULTISIG as usize {
            return Err(MultisigWitnessTemplateError::TooManyRequiredSignatures);
        }
        push_data(&mut script, pub_key.borrow().as_slice())?;
    }

    if count < required {
        return Err(MultisigWitnessTemplateError::TooManyRequiredSignatures);
    }
    if count == 0 {
        return Err(MultisigWitnessTemplateError::EmptyKeys);
    }

    push_script_int(&mut script, count as i64)?;
    script.push(OP_CHECK_MULTI_SIG_ECDSA);
    Ok(script)
}

pub fn extract_address_from_script(script: &Script, prefix: Prefix) -> Result<Address, StandardScriptError> {
    match classify_script(script) {
        ScriptClass::StdSingle => Ok(Address::new(prefix, Version::StdSingle, script.args.as_slice())?),
        ScriptClass::StdSingleECDSA => Ok(Address::new(prefix, Version::StdSingleECDSA, script.args.as_slice())?),
        ScriptClass::Account => Ok(Address::new(prefix, Version::Account, script.args.as_slice())?),
        ScriptClass::FullScript => {
            let payload = encode_full_script_payload(script);
            Ok(Address::new(prefix, Version::FullScript, payload.as_slice())?)
        }
        ScriptClass::NonStandard => Err(StandardScriptError::NonStandardLockScript),
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

pub fn is_cell_lock_unspendable(_lock: &Script) -> bool {
    false
}

#[cfg(test)]
pub fn get_witness_sig_op_count_upper_bound(witness_script: &[u8], prev_lock_script: &Script) -> u64 {
    let _ = witness_script;
    count_sig_ops(prev_lock_script.args.as_slice())
}

#[cfg(test)]
#[derive(Clone, Copy)]
struct ParsedOpcode {
    code: u8,
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
fn parse_next_opcode(script: &[u8], offset: &mut usize) -> Option<ParsedOpcode> {
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

    let _ = script.get(*offset..(*offset + data_len))?;
    *offset += data_len;
    Some(ParsedOpcode { code })
}

#[cfg(test)]
fn as_small_int(opcode: u8) -> Option<u8> {
    if (OP_TRUE..=OP_SMALL_INT_MAX).contains(&opcode) {
        Some(opcode - (OP_TRUE - 1))
    } else {
        None
    }
}

#[cfg(test)]
fn compute_lock_hash(script: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"spora-cell/lock");
    hasher.update(&0u16.to_le_bytes());
    hasher.update(script);
    *hasher.finalize().as_bytes()
}

pub fn builtin_schnorr_blake3_160_code_hash() -> [u8; 32] {
    builtin_code_hash(BUILTIN_SCHNORR_BLAKE3_160_TAG)
}

pub fn builtin_ecdsa_blake3_160_code_hash() -> [u8; 32] {
    builtin_code_hash(BUILTIN_ECDSA_BLAKE3_160_TAG)
}

pub fn builtin_account_descriptor_code_hash() -> [u8; 32] {
    builtin_code_hash(BUILTIN_ACCOUNT_DESCRIPTOR_TAG)
}

fn builtin_code_hash(tag: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(BUILTIN_CODE_HASH_DOMAIN);
    hasher.update(tag);
    *hasher.finalize().as_bytes()
}

fn encode_varint(mut value: usize) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
    out
}

fn decode_varint(bytes: &[u8]) -> Option<(usize, usize)> {
    let mut shift = 0usize;
    let mut value = 0usize;
    for (idx, byte) in bytes.iter().copied().enumerate() {
        let low = usize::from(byte & 0x7f);
        value |= low.checked_shl(shift as u32)?;
        if byte & 0x80 == 0 {
            let consumed = idx + 1;
            let canonical = encode_varint(value);
            if canonical.len() != consumed || canonical.as_slice() != &bytes[..consumed] {
                return None;
            }
            return Some((value, idx + 1));
        }
        shift += 7;
        if shift > usize::BITS as usize {
            return None;
        }
    }
    None
}

pub fn encode_full_script_payload(script: &Script) -> Vec<u8> {
    let mut payload = Vec::with_capacity(33 + 5 + script.args.len());
    payload.extend_from_slice(&script.code_hash);
    payload.push(script.hash_type);
    payload.extend_from_slice(&encode_varint(script.args.len()));
    payload.extend_from_slice(&script.args);
    payload
}

pub fn decode_full_script_payload(payload: &[u8]) -> Option<Script> {
    if payload.len() < 34 {
        return None;
    }
    let mut code_hash = [0u8; 32];
    code_hash.copy_from_slice(&payload[..32]);
    let hash_type = payload[32];
    if hash_type > 4 {
        return None;
    }
    let (args_len, varint_bytes) = decode_varint(&payload[33..])?;
    if args_len > MAX_FULL_SCRIPT_ARGS_LEN {
        return None;
    }
    let args_offset = 33 + varint_bytes;
    let args_end = args_offset.checked_add(args_len)?;
    if args_end != payload.len() {
        return None;
    }
    Some(Script::new(code_hash, hash_type, payload[args_offset..args_end].to_vec()))
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
        let address = Address::new(Prefix::Testnet, Version::StdSingle, &[0x11; 20]).unwrap();
        let lock_script = pay_to_address_lock_script(&address);
        let decoded = extract_address_from_script(&lock_script, Prefix::Testnet).unwrap();
        assert_eq!(decoded, address);
        assert_eq!(classify_script(&lock_script), ScriptClass::StdSingle);
    }

    #[test]
    fn full_script_payload_roundtrip() {
        let script = Script::new([0x42; 32], HASH_TYPE_TYPE, vec![0xAA, 0xBB, 0xCC]);
        let payload = encode_full_script_payload(&script);
        let decoded = decode_full_script_payload(&payload).expect("full-script decode should work");
        assert_eq!(decoded, script);
    }

    #[test]
    fn full_script_payload_rejects_non_minimal_varint() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&[0x42; 32]);
        payload.push(HASH_TYPE_TYPE);
        payload.extend_from_slice(&[0x81, 0x00]); // non-minimal encoding for 1
        payload.push(0xAA);
        assert!(decode_full_script_payload(&payload).is_none());
    }

    #[test]
    fn full_script_payload_rejects_trailing_bytes() {
        let script = Script::new([0x42; 32], HASH_TYPE_TYPE, vec![0xAA]);
        let mut payload = encode_full_script_payload(&script);
        payload.push(0xFF);
        assert!(decode_full_script_payload(&payload).is_none());
    }

    #[test]
    fn full_script_payload_rejects_invalid_hash_type() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&[0x42; 32]);
        payload.push(0xFF);
        payload.push(0x00);
        assert!(decode_full_script_payload(&payload).is_none());
    }

    #[test]
    fn script_class_rejects_unknown_labels() {
        assert_eq!(ScriptClass::StdSingle.to_string(), "stdsingle");
        assert_eq!(ScriptClass::StdSingleECDSA.to_string(), "stdsingleecdsa");

        assert!("unsupported".parse::<ScriptClass>().is_err());
        assert!("unsupported_ecdsa".parse::<ScriptClass>().is_err());
    }

    #[test]
    fn script_unspendable_for_op_return_and_parse_errors() {
        assert!(is_script_unspendable(&[OP_RETURN]));
        assert!(is_script_unspendable(&[OP_PUSH_DATA1]));
        assert!(!is_script_unspendable(&[]));
        assert!(!is_script_unspendable(&[OP_TRUE]));
        assert!(!is_cell_lock_unspendable(&Script::new([0u8; 32], 0, vec![])));
        assert!(!is_cell_lock_unspendable(&Script::new([0u8; 32], 0, vec![OP_RETURN])));
    }

    #[test]
    fn sig_op_counter_handles_p2pk_and_multisig() {
        let mut p2pk_args = vec![OP_DATA32];
        p2pk_args.extend_from_slice(&[0x11; 32]);
        p2pk_args.push(OP_CHECK_SIG);
        let p2pk = Script::new(compute_lock_hash(&p2pk_args), 0, p2pk_args);
        assert_eq!(get_witness_sig_op_count_upper_bound(&[], &p2pk), 1);

        let multisig_args = vec![OP_TRUE + 1, OP_CHECK_MULTI_SIG];
        let multisig = Script::new(compute_lock_hash(&multisig_args), 0, multisig_args);
        assert_eq!(get_witness_sig_op_count_upper_bound(&[], &multisig), 2);
    }

    #[test]
    fn push_data_script_encodes_canonical_pushes() {
        assert_eq!(push_data_script(&[0x11, 0x22]).unwrap(), vec![0x02, 0x11, 0x22]);
    }

    #[test]
    fn multisig_witness_template_uses_small_int_encoding() {
        let script = multisig_witness_template([[0x11; 32], [0x22; 32]].into_iter(), 2).unwrap();
        assert_eq!(script[0], OP_TRUE + 1);
        assert_eq!(script[1], OP_DATA32);
        assert_eq!(script[34], OP_DATA32);
        assert_eq!(script[67], OP_TRUE + 1);
        assert_eq!(script[68], OP_CHECK_MULTI_SIG);
    }

    #[test]
    fn multisig_witness_template_supports_counts_above_small_int_range() {
        let keys = (0..17).map(|idx| [idx as u8; 32]);
        let script = multisig_witness_template(keys, 17).unwrap();
        assert_eq!(script.last(), Some(&OP_CHECK_MULTI_SIG));
    }
}
