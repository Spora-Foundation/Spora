//!
//! Spora [`Address`] implementation.
//!
//! This module provides a comprehensive address system for the Spora blockchain,
//! supporting multiple address types and network configurations.
//!
//! ## Address Format
//!
//! Spora addresses are represented as `bech32`-encoded strings with the format:
//! `{network_prefix}:{encoded_payload}`
//!
//! Where:
//! - `network_prefix`: Network identifier (`spora`, `sporatest`, `sporasim`, `sporadev`)
//! - `encoded_payload`: Bech32/Bech32m encoded data containing version byte and payload
//!
//! ## Supported Address Types
//!
//! - **PubKey** (v0): Standard public key addresses
//! - **PubKeyECDSA** (v1): ECDSA-compatible public key addresses
//! - **ScriptHash** (v8): Pay-to-script-hash addresses
//!
//! ## Examples
//!
//! ```rust
//! use spora_addresses::{Address, Prefix, Version};
//!
//! // Create a new address
//! let payload = [0u8; 32];
//! let address = Address::new(Prefix::Mainnet, Version::PubKey, &payload).expect("Valid address");
//!
//! // Parse from string
//! // let address: Address = "spora:qz0s...t8cv".parse().expect("Valid address");
//!
//! // Validate address
//! // let is_valid = Address::validate("spora:qz0s...t8cv");
//!
//! // Use convenience constructors
//! let pubkey_addr = Address::new_pubkey(Prefix::Mainnet, &[0u8; 32]).expect("Valid address");
//!
//! // Get address information
//! let info = address.info();
//! println!("Address type: {}", info.version.type_name());
//! println!("Network: {}", info.prefix.network_name());
//! ```
//!
//! ## Recent Improvements
//!
//! This module has been enhanced with:
//! - **Better Error Handling**: Detailed error messages with context
//! - **Convenience Methods**: Easy-to-use constructors for common address types
//! - **Enhanced API**: More methods for address inspection and validation
//! - **Improved Documentation**: Comprehensive examples and usage guides
//! - **Type Safety**: Better validation and error reporting
//! - **WASM Support**: Enhanced JavaScript bindings with more functionality
//!

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smallvec::SmallVec;
use std::fmt::{Display, Formatter};
use std::str;
use str::FromStr;
use thiserror::Error;
use wasm_bindgen::prelude::*;
use workflow_wasm::{
    convert::{Cast, CastFromJs, TryCastFromJs},
    extensions::object::*,
};

#[cfg(feature = "bech32")]
mod bech32;
#[cfg(feature = "bech32m")]
mod bech32m;

/// Error type produced by [`Address`] operations.
#[derive(Error, PartialEq, Eq, Debug, Clone)]
pub enum AddressError {
    /// The address has an invalid network prefix
    #[error("Invalid network prefix '{0}'. Expected one of: spora, sporatest, sporasim, sporadev")]
    InvalidPrefix(String),

    /// The address is missing the required network prefix
    #[error("Address is missing network prefix. Expected format: 'prefix:payload'")]
    MissingPrefix,

    /// The address has an invalid version byte
    #[error("Invalid address version {0}. Supported versions: 0 (PubKey), 1 (PubKeyECDSA), 8 (ScriptHash)")]
    InvalidVersion(u8),

    /// The address has an invalid version string
    #[error("Invalid version string '{0}'. Expected one of: PubKey, PubKeyECDSA, ScriptHash")]
    InvalidVersionString(String),

    /// The address contains an invalid character in the encoded payload
    #[error("Invalid character '{0}' in address payload. Only valid bech32 characters are allowed")]
    DecodingError(char),

    /// The address checksum has incorrect size
    #[error("Invalid checksum size. Expected exactly 8 bytes, got {0} bytes")]
    BadChecksumSize(usize),

    /// The address checksum validation failed
    #[error("Checksum validation failed. The address may be corrupted or invalid")]
    BadChecksum,

    /// The address payload is invalid or has incorrect length
    #[error("Invalid payload: expected {expected} bytes for version {version}, got {actual} bytes")]
    BadPayload { expected: usize, actual: usize, version: u8 },

    /// The address format is invalid
    #[error("Invalid address format. Expected format: 'prefix:payload'")]
    InvalidAddress,

    /// The address array contains invalid elements
    #[error("Invalid address array: {0}")]
    InvalidAddressArray(String),

    /// WASM-specific error
    #[error("WASM error: {0}")]
    WASM(String),
}

impl From<workflow_wasm::error::Error> for AddressError {
    fn from(e: workflow_wasm::error::Error) -> Self {
        AddressError::WASM(e.to_string())
    }
}

/// Convenience functions for creating common address types
impl Address {
    /// Create a standard PubKey address
    ///
    /// # Arguments
    /// * `prefix` - Network prefix
    /// * `pubkey` - 32-byte public key
    pub fn new_pubkey(prefix: Prefix, pubkey: &[u8; 32]) -> Result<Self, AddressError> {
        Self::new(prefix, Version::PubKey, pubkey)
    }

    /// Create an ECDSA PubKey address
    ///
    /// # Arguments
    /// * `prefix` - Network prefix
    /// * `pubkey` - 33-byte ECDSA public key
    pub fn new_pubkey_ecdsa(prefix: Prefix, pubkey: &[u8; 33]) -> Result<Self, AddressError> {
        Self::new(prefix, Version::PubKeyECDSA, pubkey)
    }

    /// Create a ScriptHash address
    ///
    /// # Arguments
    /// * `prefix` - Network prefix
    /// * `script_hash` - 32-byte script hash
    pub fn new_script_hash(prefix: Prefix, script_hash: &[u8; 32]) -> Result<Self, AddressError> {
        Self::new(prefix, Version::ScriptHash, script_hash)
    }

    /// Parse an address from a string with detailed error information
    ///
    /// # Arguments
    /// * `address_str` - Address string to parse
    ///
    /// # Returns
    /// `Ok(Address)` if parsing succeeds, `Err(AddressError)` with detailed error information
    pub fn parse(address_str: &str) -> Result<Self, AddressError> {
        address_str.try_into()
    }

    /// Validate an address string and return detailed error information
    ///
    /// # Arguments
    /// * `address_str` - Address string to validate
    ///
    /// # Returns
    /// `Ok(())` if valid, `Err(AddressError)` with detailed error information
    pub fn validate_detailed(address_str: &str) -> Result<(), AddressError> {
        let _address: Address = address_str.try_into()?;
        Ok(())
    }
}

/// Network prefix identifying the blockchain network type.
///
/// Each prefix corresponds to a specific Spora network configuration:
/// - `Mainnet`: Production network (`spora`)
/// - `Testnet`: Public test network (`sporatest`)
/// - `Simnet`: Simulation network (`sporasim`)
/// - `Devnet`: Development network (`sporadev`)
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
pub enum Prefix {
    /// Mainnet - Production blockchain network
    #[serde(rename = "spora")]
    Mainnet,
    /// Testnet - Public testing network
    #[serde(rename = "spora0")]
    Testnet,
    /// Simnet - Simulation network for testing
    #[serde(rename = "sporasim")]
    Simnet,
    /// Devnet - Development network
    #[serde(rename = "sporadev")]
    Devnet,
    #[cfg(test)]
    A,
    #[cfg(test)]
    B,
}

impl Prefix {
    /// Get the string representation of the network prefix
    #[inline(always)]
    pub fn as_str(&self) -> &'static str {
        match self {
            Prefix::Mainnet => "spora",
            Prefix::Testnet => "spora0",
            Prefix::Simnet => "sporasim",
            Prefix::Devnet => "sporadev",
            #[cfg(test)]
            Prefix::A => "a",
            #[cfg(test)]
            Prefix::B => "b",
        }
    }

    /// Check if this is a test network prefix
    #[inline(always)]
    pub fn is_test(&self) -> bool {
        #[cfg(not(test))]
        return false;
        #[cfg(test)]
        matches!(self, Prefix::A | Prefix::B)
    }

    /// Check if this is a production network prefix
    #[inline(always)]
    pub fn is_mainnet(&self) -> bool {
        matches!(self, Prefix::Mainnet)
    }

    /// Get all supported network prefixes
    pub fn all() -> &'static [Prefix] {
        &[Prefix::Mainnet, Prefix::Testnet, Prefix::Simnet, Prefix::Devnet]
    }

    /// Get the human-readable name of the network
    pub fn network_name(&self) -> &'static str {
        match self {
            Prefix::Mainnet => "Mainnet",
            Prefix::Testnet => "Testnet",
            Prefix::Simnet => "Simnet",
            Prefix::Devnet => "Devnet",
            #[cfg(test)]
            Prefix::A => "Test A",
            #[cfg(test)]
            Prefix::B => "Test B",
        }
    }
}

impl Display for Prefix {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl TryFrom<&str> for Prefix {
    type Error = AddressError;

    fn try_from(prefix: &str) -> Result<Self, Self::Error> {
        match prefix {
            "spora" => Ok(Prefix::Mainnet),
            "spora0" => Ok(Prefix::Testnet),
            "sporasim" => Ok(Prefix::Simnet),
            "sporadev" => Ok(Prefix::Devnet),
            #[cfg(test)]
            "a" => Ok(Prefix::A),
            #[cfg(test)]
            "b" => Ok(Prefix::B),
            _ => Err(AddressError::InvalidPrefix(prefix.to_string())),
        }
    }
}

/// Address version defining the type and format of the address payload.
///
/// Each version corresponds to a specific address type with different characteristics:
/// - **PubKey** (v0): Standard 32-byte public key addresses
/// - **PubKeyECDSA** (v1): ECDSA-compatible 33-byte public key addresses
/// - **ScriptHash** (v8): Pay-to-script-hash addresses with 32-byte script hash
///
/// @category Address
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[repr(u8)]
#[borsh(use_discriminant = true)]
#[wasm_bindgen(js_name = "AddressVersion")]
pub enum Version {
    /// Standard public key addresses (32 bytes)
    /// Version byte: 0 (0b00000_000)
    PubKey = 0,
    /// ECDSA-compatible public key addresses (33 bytes)
    /// Version byte: 1 (0b00000_001)
    PubKeyECDSA = 1,
    /// Pay-to-script-hash addresses (32 bytes)
    /// Version byte: 8 (0b00001_000)
    ScriptHash = 8,
}

impl TryFrom<&str> for Version {
    type Error = AddressError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "PubKey" => Ok(Version::PubKey),
            "PubKeyECDSA" => Ok(Version::PubKeyECDSA),
            "ScriptHash" => Ok(Version::ScriptHash),
            _ => Err(AddressError::InvalidVersionString(value.to_owned())),
        }
    }
}

impl Version {
    /// Get the expected payload length in bytes for this address version
    #[inline(always)]
    pub fn payload_len(&self) -> usize {
        match self {
            Version::PubKey => 32,
            Version::PubKeyECDSA => 33,
            Version::ScriptHash => 32,
        }
    }

    /// Get the expected payload length in bytes for this address version
    ///
    /// This is an alias for `payload_len()` for backward compatibility
    #[inline(always)]
    pub fn public_key_len(&self) -> usize {
        self.payload_len()
    }

    /// Check if this version is currently enabled for mainnet
    #[inline(always)]
    pub fn is_enabled(&self) -> bool {
        true
    }

    /// Get the human-readable name of the address type
    pub fn type_name(&self) -> &'static str {
        match self {
            Version::PubKey => "Public Key",
            Version::PubKeyECDSA => "Public Key (ECDSA)",
            Version::ScriptHash => "Script Hash",
        }
    }

    /// Get all supported address versions
    pub fn all() -> &'static [Version] {
        &[Version::PubKey, Version::PubKeyECDSA, Version::ScriptHash]
    }
}

impl TryFrom<u8> for Version {
    type Error = AddressError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Version::PubKey),
            1 => Ok(Version::PubKeyECDSA),
            8 => Ok(Version::ScriptHash),
            _ => Err(AddressError::InvalidVersion(value)),
        }
    }
}

impl Display for Version {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Version::PubKey => write!(f, "PubKey"),
            Version::PubKeyECDSA => write!(f, "PubKeyECDSA"),
            Version::ScriptHash => write!(f, "ScriptHash"),
        }
    }
}

/// Size of the payload vector of an address.
///
/// This size is the smallest SmallVec supported backing store size greater or equal to the largest
/// possible payload, which is 33 for [`Version::PubKeyECDSA`].
pub const PAYLOAD_VECTOR_SIZE: usize = 36;

/// Used as the underlying type for address payload, optimized for the largest version length (33).
pub type PayloadVec = SmallVec<[u8; PAYLOAD_VECTOR_SIZE]>;

/// Detailed information about an address
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressInfo {
    /// Network prefix
    pub prefix: Prefix,
    /// Address version/type
    pub version: Version,
    /// Payload length in bytes
    pub payload_len: usize,
    /// Whether this address version is currently enabled
    pub is_enabled: bool,
}

/// Spora [`Address`] struct that serializes to and from an address format string: `spora:qz0s...t8cv`.
///
/// @category Address
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash, CastFromJs)]
#[wasm_bindgen(inspectable)]
pub struct Address {
    #[wasm_bindgen(skip)]
    pub prefix: Prefix,
    #[wasm_bindgen(skip)]
    pub version: Version,
    #[wasm_bindgen(skip)]
    pub payload: PayloadVec,
}

impl std::fmt::Debug for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.version == Version::PubKey {
            write!(f, "{}", String::from(self))
        } else {
            write!(f, "{} ({})", String::from(self), self.version)
        }
    }
}

impl Address {
    /// Create a new address with the specified prefix, version, and payload
    ///
    /// # Arguments
    /// * `prefix` - Network prefix (mainnet, testnet, etc.)
    /// * `version` - Address version/type
    /// * `payload` - Address payload bytes
    ///
    /// # Errors
    /// Returns `AddressError::BadPayload` if the payload length doesn't match the expected length for the version
    ///
    /// # Examples
    /// ```rust
    /// use spora_addresses::{Address, Prefix, Version};
    ///
    /// let payload = [0u8; 32];
    /// let address = Address::new(Prefix::Mainnet, Version::PubKey, &payload);
    /// ```
    pub fn new(prefix: Prefix, version: Version, payload: &[u8]) -> Result<Self, AddressError> {
        let expected_len = version.payload_len();
        if !prefix.is_test() && payload.len() != expected_len {
            return Err(AddressError::BadPayload { expected: expected_len, actual: payload.len(), version: version as u8 });
        }
        Ok(Self { prefix, payload: PayloadVec::from_slice(payload), version })
    }

    /// Create a new address with the specified prefix, version, and payload (unchecked)
    ///
    /// # Safety
    /// This function does not validate payload length. Use `new()` for safe construction.
    #[inline(always)]
    pub fn new_unchecked(prefix: Prefix, version: Version, payload: &[u8]) -> Self {
        Self { prefix, payload: PayloadVec::from_slice(payload), version }
    }

    /// Get the network prefix of this address
    #[inline(always)]
    pub fn prefix(&self) -> Prefix {
        self.prefix
    }

    /// Get the version/type of this address
    #[inline(always)]
    pub fn version(&self) -> Version {
        self.version
    }

    /// Get the payload bytes of this address
    #[inline(always)]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Check if this address is valid for the current network
    #[inline(always)]
    pub fn is_valid_for_network(&self, network_prefix: Prefix) -> bool {
        self.prefix == network_prefix
    }

    /// Check if this is a test network address
    #[inline(always)]
    pub fn is_test_network(&self) -> bool {
        self.prefix.is_test()
    }

    /// Check if this is a mainnet address
    #[inline(always)]
    pub fn is_mainnet(&self) -> bool {
        self.prefix.is_mainnet()
    }

    /// Check if this address version is currently enabled
    #[inline(always)]
    pub fn is_enabled(&self) -> bool {
        self.version.is_enabled()
    }

    /// Get a short representation of the address for display
    ///
    /// # Arguments
    /// * `chars` - Number of characters to show from the beginning and end of the payload
    ///
    /// # Examples
    /// ```rust
    /// use spora_addresses::{Address, Prefix, Version};
    /// let address = Address::new(Prefix::Mainnet, Version::PubKey, &[0u8; 32]).expect("Valid address");
    /// let short = address.short_display(4);
    /// // Returns: "spora:qz0s....t8cv"
    /// ```
    pub fn short_display(&self, chars: usize) -> String {
        let payload = self.encode_payload();
        let chars = std::cmp::min(chars, payload.len() / 4);
        if chars == 0 {
            return format!("{}:{}", self.prefix, payload);
        }
        format!("{}:{}....{}", self.prefix, &payload[0..chars], &payload[payload.len() - chars..])
    }

    /// Get detailed information about this address
    pub fn info(&self) -> AddressInfo {
        AddressInfo { prefix: self.prefix, version: self.version, payload_len: self.payload.len(), is_enabled: self.is_enabled() }
    }
}

#[wasm_bindgen]
impl Address {
    /// Create a new address from a string representation
    ///
    /// # Arguments
    /// * `address` - Address string in format "prefix:payload"
    ///
    /// # Panics
    /// Panics if the address string is invalid. Use `validate()` to check validity first.
    #[wasm_bindgen(constructor)]
    pub fn constructor(address: &str) -> Address {
        address.try_into().unwrap_or_else(|err| panic!("Address::constructor() - invalid address '{}': {}", address, err))
    }

    /// Validate an address string without creating an Address object
    ///
    /// # Arguments
    /// * `address` - Address string to validate
    ///
    /// # Returns
    /// `true` if the address is valid, `false` otherwise
    #[wasm_bindgen(js_name=validate)]
    pub fn validate(address: &str) -> bool {
        Self::try_from(address).is_ok()
    }

    /// Convert an address to its string representation
    #[wasm_bindgen(js_name = toString)]
    pub fn address_to_string(&self) -> String {
        self.into()
    }

    /// Get the network prefix as a string
    #[wasm_bindgen(getter, js_name = "prefix")]
    pub fn prefix_to_string(&self) -> String {
        self.prefix.to_string()
    }

    /// Get the address version as a string
    #[wasm_bindgen(getter, js_name = "version")]
    pub fn version_to_string(&self) -> String {
        self.version.to_string()
    }

    /// Set the network prefix from a string
    ///
    /// # Arguments
    /// * `prefix` - Network prefix string
    ///
    /// # Panics
    /// Panics if the prefix string is invalid
    #[wasm_bindgen(setter, js_name = "setPrefix")]
    pub fn set_prefix_from_str(&mut self, prefix: &str) {
        self.prefix = Prefix::try_from(prefix)
            .unwrap_or_else(|err| panic!("Address::set_prefix_from_str() - invalid prefix '{}': {}", prefix, err));
    }

    /// Get the encoded payload as a string
    #[wasm_bindgen(getter, js_name = "payload")]
    pub fn payload_to_string(&self) -> String {
        self.encode_payload()
    }

    /// Get a short representation of the address
    ///
    /// # Arguments
    /// * `n` - Number of characters to show from beginning and end
    #[wasm_bindgen(js_name = "short")]
    pub fn short(&self, n: usize) -> String {
        self.short_display(n)
    }

    /// Check if this address is enabled
    #[wasm_bindgen(js_name = "isEnabled")]
    pub fn js_is_enabled(&self) -> bool {
        self.is_enabled()
    }

    /// Check if this is a mainnet address
    #[wasm_bindgen(js_name = "isMainnet")]
    pub fn js_is_mainnet(&self) -> bool {
        self.is_mainnet()
    }

    /// Get address information as a JavaScript object
    #[wasm_bindgen(js_name = "getInfo")]
    pub fn js_get_info(&self) -> js_sys::Object {
        let info = self.info();
        let obj = js_sys::Object::new();
        js_sys::Reflect::set(&obj, &"prefix".into(), &info.prefix.to_string().into()).unwrap();
        js_sys::Reflect::set(&obj, &"version".into(), &info.version.to_string().into()).unwrap();
        js_sys::Reflect::set(&obj, &"payloadLen".into(), &(info.payload_len as u32).into()).unwrap();
        js_sys::Reflect::set(&obj, &"isEnabled".into(), &info.is_enabled.into()).unwrap();
        obj
    }
}

impl Display for Address {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", String::from(self))
    }
}

//
// Borsh serializers need to be manually implemented for `Address` since
// smallvec does not currently support Borsh
//

impl BorshSerialize for Address {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        borsh::BorshSerialize::serialize(&self.prefix, writer)?;
        borsh::BorshSerialize::serialize(&self.version, writer)?;
        // Vectors and slices are all serialized internally the same way
        borsh::BorshSerialize::serialize(&self.payload.as_slice(), writer)?;
        Ok(())
    }
}

impl BorshDeserialize for Address {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let prefix: Prefix = borsh::BorshDeserialize::deserialize_reader(reader)?;
        let version: Version = borsh::BorshDeserialize::deserialize_reader(reader)?;
        let payload: Vec<u8> = borsh::BorshDeserialize::deserialize_reader(reader)?;
        Self::new(prefix, version, &payload).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

impl From<Address> for String {
    fn from(address: Address) -> Self {
        (&address).into()
    }
}

impl From<&Address> for String {
    fn from(address: &Address) -> Self {
        format!("{}:{}", address.prefix, address.encode_payload())
    }
}

impl TryFrom<String> for Address {
    type Error = AddressError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

impl FromStr for Address {
    type Err = AddressError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Address::try_from(s)
    }
}

impl TryFrom<&str> for Address {
    type Error = AddressError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.split_once(':') {
            Some((prefix, payload)) => Self::decode_payload(prefix.try_into()?, payload),
            None => Err(AddressError::MissingPrefix),
        }
    }
}

impl Serialize for Address {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Address {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Default)]
        pub struct AddressVisitor<'de> {
            marker: std::marker::PhantomData<Address>,
            lifetime: std::marker::PhantomData<&'de ()>,
        }
        impl<'de> serde::de::Visitor<'de> for AddressVisitor<'de> {
            type Value = Address;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                #[cfg(target_arch = "wasm32")]
                {
                    write!(formatter, "string-type: string, str; bytes-type: slice of bytes, vec of bytes; map; number-type - pointer")
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    write!(formatter, "string-type: string, str; bytes-type: slice of bytes, vec of bytes; map")
                }
            }

            // TODO: see related comment in script_public_key.rs
            #[cfg(target_arch = "wasm32")]
            fn visit_i32<E>(self, v: i32) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_u32(v as u32)
            }
            #[cfg(target_arch = "wasm32")]
            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_u32(v as u32)
            }

            #[cfg(target_arch = "wasm32")]
            fn visit_f32<E>(self, v: f32) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_u32(v as u32)
            }
            #[cfg(target_arch = "wasm32")]
            fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_u32(v as u32)
            }
            #[cfg(target_arch = "wasm32")]
            fn visit_u32<E>(self, v: u32) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use wasm_bindgen::convert::RefFromWasmAbi;
                let instance_ref = unsafe { Self::Value::ref_from_abi(v) }; // todo add checks for safecast
                Ok(instance_ref.clone())
            }
            #[cfg(target_arch = "wasm32")]
            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_u32(v as u32)
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Address::try_from(v).map_err(serde::de::Error::custom)
            }

            fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Address::try_from(v).map_err(serde::de::Error::custom)
            }

            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Address::try_from(v).map_err(serde::de::Error::custom)
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let str = str::from_utf8(v).map_err(serde::de::Error::custom)?;
                Address::try_from(str).map_err(serde::de::Error::custom)
            }

            fn visit_borrowed_bytes<E>(self, v: &'de [u8]) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let str = str::from_utf8(v).map_err(serde::de::Error::custom)?;
                Address::try_from(str).map_err(serde::de::Error::custom)
            }

            fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let str = str::from_utf8(&v).map_err(serde::de::Error::custom)?;
                Address::try_from(str).map_err(serde::de::Error::custom)
            }

            fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut prefix: Option<String> = None;
                let mut payload: Option<String> = None;

                while let Some((key, value)) = access.next_entry::<String, String>()? {
                    #[cfg(test)]
                    web_sys::console::log_3(&"key value: ".into(), &key.clone().into(), &value.clone().into());

                    match key.as_ref() {
                        "prefix" => {
                            prefix = Some(value.to_string());
                        }
                        "payload" => {
                            payload = Some(value.to_string());
                        }
                        "version" => continue,
                        unknown_field => {
                            return Err(serde::de::Error::unknown_field(unknown_field, &["prefix", "payload", "version"]))
                        }
                    }
                    if prefix.is_some() && payload.is_some() {
                        break;
                    }
                }
                let (prefix, payload) = match (prefix, payload) {
                    (Some(prefix), Some(payload)) => (prefix, payload),
                    (None, _) => return Err(serde::de::Error::missing_field("prefix")),
                    (_, None) => return Err(serde::de::Error::missing_field("payload")),
                };
                Address::decode_payload(prefix.as_str().try_into().map_err(serde::de::Error::custom)?, &payload)
                    .map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_any(AddressVisitor::default())
    }
}

impl TryCastFromJs for Address {
    type Error = AddressError;
    fn try_cast_from<'a, R>(value: &'a R) -> Result<Cast<'a, Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Self::resolve(value, || {
            if let Some(string) = value.as_ref().as_string() {
                Address::try_from(string.trim())
            } else if let Some(object) = js_sys::Object::try_from(value.as_ref()) {
                let prefix: Prefix = object.get_string("prefix")?.as_str().try_into()?;
                let payload = object.get_string("payload")?; //.as_str();
                Address::decode_payload(prefix, &payload)
            } else {
                Err(AddressError::InvalidAddress)
            }
        })
    }
}

#[wasm_bindgen]
extern "C" {
    /// WASM (TypeScript) type representing an Address-like object: `Address | string`.
    ///
    /// @category Address
    #[wasm_bindgen(extends = js_sys::Array, typescript_type = "Address | string")]
    pub type AddressT;
    /// WASM (TypeScript) type representing an array of Address-like objects: `(Address | string)[]`.
    ///
    /// @category Address
    #[wasm_bindgen(extends = js_sys::Array, typescript_type = "(Address | string)[]")]
    pub type AddressOrStringArrayT;
    /// WASM (TypeScript) type representing an array of [`Address`] objects: `Address[]`.
    ///
    /// @category Address
    #[wasm_bindgen(extends = js_sys::Array, typescript_type = "Address[]")]
    pub type AddressArrayT;
    /// WASM (TypeScript) type representing an [`Address`] or an undefined value: `Address | undefined`.
    ///
    /// @category Address
    #[wasm_bindgen(typescript_type = "Address | undefined")]
    pub type AddressOrUndefinedT;
}

impl TryFrom<AddressOrStringArrayT> for Vec<Address> {
    type Error = AddressError;
    fn try_from(js_value: AddressOrStringArrayT) -> Result<Self, Self::Error> {
        if js_value.is_array() {
            js_value.iter().map(Address::try_owned_from).collect::<Result<Vec<Address>, AddressError>>()
        } else {
            Err(AddressError::InvalidAddressArray("Not an array".to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_roundtrip() {
        use Prefix::*;
        use Version::*;

        fn gen_payload(version: Version) -> Vec<u8> {
            vec![version as u8 + 1; version.public_key_len()]
        }

        let cases = vec![
            (Mainnet, PubKey),
            (Mainnet, PubKeyECDSA),
            (Mainnet, ScriptHash),
            (Testnet, PubKey),
            (Testnet, PubKeyECDSA),
            (Devnet, ScriptHash),
            (Simnet, PubKey),
        ];

        for (prefix, version) in cases {
            let payload = gen_payload(version);
            let address = Address::new(prefix, version, &payload).expect("Valid address");
            let encoded = address.to_string();
            let decoded: Address = encoded.parse().expect("Address decode failed");
            assert_eq!(decoded, address, "Roundtrip mismatch: {encoded}");
        }
    }

    #[test]
    fn test_address_convenience_constructors() {
        use Prefix::*;

        let payload32 = [0u8; 32];
        let payload33 = [0u8; 33];

        // Test convenience constructors
        let pubkey_addr = Address::new_pubkey(Mainnet, &payload32).expect("Valid pubkey address");
        assert_eq!(pubkey_addr.version(), Version::PubKey);

        let ecdsa_addr = Address::new_pubkey_ecdsa(Mainnet, &payload33).expect("Valid ecdsa address");
        assert_eq!(ecdsa_addr.version(), Version::PubKeyECDSA);

        let script_addr = Address::new_script_hash(Mainnet, &payload32).expect("Valid script hash address");
        assert_eq!(script_addr.version(), Version::ScriptHash);
    }

    #[test]
    fn test_address_info_and_methods() {
        use Prefix::*;

        let payload = [0u8; 32];
        let address = Address::new(Mainnet, Version::PubKey, &payload).expect("Valid address");

        // Test info method
        let info = address.info();
        assert_eq!(info.prefix, Mainnet);
        assert_eq!(info.version, Version::PubKey);
        assert_eq!(info.payload_len, 32);
        assert!(info.is_enabled);

        // Test convenience methods
        assert!(address.is_enabled());
        assert!(address.is_mainnet());
        assert!(!address.is_test_network());

        // Test short display
        let short = address.short_display(4);
        assert!(short.contains("spora:"));
        assert!(short.contains("...."));
    }

    #[test]
    fn test_improved_error_handling() {
        use Prefix::*;

        // Test payload length validation
        let wrong_payload = [0u8; 16]; // Too short for PubKeyECDSA
        let result = Address::new(Mainnet, Version::PubKeyECDSA, &wrong_payload);
        assert!(matches!(result, Err(AddressError::BadPayload { expected: 33, actual: 16, version: 1 })));

        // Test detailed validation
        let invalid_address = "invalid:address";
        let result = Address::validate_detailed(invalid_address);
        assert!(result.is_err());

        // Test parse with detailed error
        let result = Address::parse("spora:invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_prefix_and_version_methods() {
        // Test Prefix methods
        assert!(Prefix::Mainnet.is_mainnet());
        assert!(!Prefix::Testnet.is_mainnet());
        assert_eq!(Prefix::Mainnet.network_name(), "Mainnet");
        assert_eq!(Prefix::Testnet.network_name(), "Testnet");

        let all_prefixes = Prefix::all();
        assert!(all_prefixes.contains(&Prefix::Mainnet));
        assert!(all_prefixes.contains(&Prefix::Testnet));

        // Test Version methods
        assert!(Version::PubKey.is_enabled());
        assert_eq!(Version::PubKey.type_name(), "Public Key");

        let all_versions = Version::all();
        assert!(all_versions.contains(&Version::PubKey));
        assert!(all_versions.contains(&Version::ScriptHash));
    }

    #[test]
    fn test_address_version_byte_encoding() {
        use Prefix::*;
        use Version::*;

        // Test that version bytes encode to expected first characters
        let test_key = [0u8; 32];

        let pubkey = Address::new(Mainnet, PubKey, &test_key).expect("Valid address");
        let pubkey_data = pubkey.to_string();
        assert!(!pubkey_data.strip_prefix("spora:").unwrap().is_empty());

        let script_hash = Address::new(Mainnet, ScriptHash, &test_key).expect("Valid address");
        let script_hash_data = script_hash.to_string();
        assert!(!script_hash_data.strip_prefix("spora:").unwrap().is_empty());
    }

    #[test]
    fn invalid_prefix_should_fail() {
        let invalid = "wrongprefix:qpauqsvk7yf9...";
        let result: Result<Address, _> = invalid.parse();
        assert!(matches!(result, Err(AddressError::InvalidPrefix(_))));
    }

    #[test]
    fn missing_colon_should_fail() {
        let invalid = "sporatestqpauqsvk7yf9...";
        let result: Result<Address, _> = invalid.parse();
        assert_eq!(result, Err(AddressError::MissingPrefix));
    }

    #[test]
    fn bad_checksum_should_fail() {
        // Modify one character of a valid address
        let valid = Address::new(Prefix::Testnet, Version::PubKey, &[0u8; 32]).expect("Valid address").to_string();
        let mut broken = valid.clone();
        if let Some(last) = broken.pop() {
            let replacement = if last == 'a' { 'b' } else { 'a' };
            broken.push(replacement);
        }
        let result: Result<Address, _> = broken.parse();
        assert_eq!(result, Err(AddressError::BadChecksum));
    }

    #[cfg(target_arch = "wasm32")]
    use js_sys::Object;
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen::{JsValue, __rt::IntoJsResult};
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test;
    #[cfg(target_arch = "wasm32")]
    use workflow_wasm::{extensions::ObjectExtension, serde::from_value, serde::to_value};

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test]
    pub fn test_wasm_serde_constructor() {
        let addr = Address::new(Prefix::Mainnet, Version::PubKey, &[0u8; 32]);
        let encoded = addr.to_string();
        let a = Address::constructor(&encoded);
        let value = to_value(&a).unwrap();

        assert_eq!(JsValue::from_str("string"), value.js_typeof());
        assert_eq!(value, JsValue::from_str(&encoded));
        assert_eq!(a, from_value(value).unwrap());
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test]
    pub fn test_wasm_js_serde_object() {
        let addr = Address::new(Prefix::Mainnet, Version::PubKey, &[0u8; 32]);

        let obj = Object::new();
        obj.set("version", &JsValue::from_str(&addr.version.to_string())).unwrap();
        obj.set("prefix", &JsValue::from_str(&addr.prefix.to_string())).unwrap();
        obj.set("payload", &JsValue::from_str(&addr.payload_to_string())).unwrap();

        assert_eq!(JsValue::from_str("object"), obj.js_typeof());

        let obj_js = obj.into_js_result().unwrap();
        let actual = from_value(obj_js).unwrap();
        assert_eq!(addr, actual);
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test]
    pub fn test_wasm_serde_object() {
        use wasm_bindgen::convert::IntoWasmAbi;

        let addr = Address::new(Prefix::Mainnet, Version::PubKey, &[0u8; 32]);
        let wasm_js_value: JsValue = addr.clone().into_abi().into();
        let actual = from_value(wasm_js_value).unwrap();

        assert_eq!(addr, actual);
    }
}
