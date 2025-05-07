//!
//! Tondi [`Address`] implementation.
//!
//! In it's string form, the Tondi [`Address`] is represented by a `bech32`-encoded
//! address string combined with a network type.  The `bech32` string encoding is
//! comprised of a public key, the public key version and the resulting checksum.
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

mod bech32;

/// Error type produced by [`Address`] operations.
#[derive(Error, PartialEq, Eq, Debug, Clone)]
pub enum AddressError {
    #[error("The address has an invalid prefix {0}")]
    InvalidPrefix(String),

    #[error("The address prefix is missing")]
    MissingPrefix,

    #[error("The address has an invalid version {0}")]
    InvalidVersion(u8),

    #[error("The address has an invalid version {0}")]
    InvalidVersionString(String),

    #[error("The address contains an invalid character {0}")]
    DecodingError(char),

    #[error("The address checksum is invalid (must be exactly 8 bytes)")]
    BadChecksumSize,

    #[error("The address checksum is invalid")]
    BadChecksum,

    #[error("The address payload is invalid")]
    BadPayload,

    #[error("The address is invalid")]
    InvalidAddress,

    #[error("The address array is invalid")]
    InvalidAddressArray,

    #[error("{0}")]
    WASM(String),
}

impl From<workflow_wasm::error::Error> for AddressError {
    fn from(e: workflow_wasm::error::Error) -> Self {
        AddressError::WASM(e.to_string())
    }
}

/// Address prefix identifying the network type this address belongs to (such as `tondi`, `tonditest`, `tondisim`, `tondidev`).
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
pub enum Prefix {
    #[serde(rename = "tondi")]
    Mainnet,
    #[serde(rename = "tonditest")]
    Testnet,
    #[serde(rename = "tondisim")]
    Simnet,
    #[serde(rename = "tondidev")]
    Devnet,
    #[cfg(test)]
    A,
    #[cfg(test)]
    B,
}

impl Prefix {
    fn as_str(&self) -> &'static str {
        match self {
            Prefix::Mainnet => "tondi",
            Prefix::Testnet => "tonditest",
            Prefix::Simnet => "tondisim",
            Prefix::Devnet => "tondidev",
            #[cfg(test)]
            Prefix::A => "a",
            #[cfg(test)]
            Prefix::B => "b",
        }
    }

    #[inline(always)]
    fn is_test(&self) -> bool {
        #[cfg(not(test))]
        return false;
        #[cfg(test)]
        matches!(self, Prefix::A | Prefix::B)
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
            "tondi" => Ok(Prefix::Mainnet),
            "tonditest" => Ok(Prefix::Testnet),
            "tondisim" => Ok(Prefix::Simnet),
            "tondidev" => Ok(Prefix::Devnet),
            #[cfg(test)]
            "a" => Ok(Prefix::A),
            #[cfg(test)]
            "b" => Ok(Prefix::B),
            _ => Err(AddressError::InvalidPrefix(prefix.to_string())),
        }
    }
}

///
///  Tondi `Address` version (`PubKey`, `PubKey ECDSA`, `ScriptHash`)
///
/// @category Address
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[repr(u8)]
#[borsh(use_discriminant = true)]
#[wasm_bindgen(js_name = "AddressVersion")]
pub enum Version {
    /// PubKey addresses always have the version byte set to 0
    PubKey = 0,
    /// PubKey ECDSA addresses always have the version byte set to 1
    PubKeyECDSA = 1,
    /// ScriptHash addresses always have the version byte set to 8
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
    pub fn public_key_len(&self) -> usize {
        match self {
            Version::PubKey => 32,
            Version::PubKeyECDSA => 33,
            Version::ScriptHash => 32,
        }
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

/// Tondi [`Address`] struct that serializes to and from an address format string: `tondi:qz0s...t8cv`.
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
    pub fn new(prefix: Prefix, version: Version, payload: &[u8]) -> Self {
        if !prefix.is_test() {
            assert_eq!(payload.len(), version.public_key_len());
        }
        Self { prefix, payload: PayloadVec::from_slice(payload), version }
    }
}

#[wasm_bindgen]
impl Address {
    #[wasm_bindgen(constructor)]
    pub fn constructor(address: &str) -> Address {
        address.try_into().unwrap_or_else(|err| panic!("Address::constructor() - address error `{}`: {err}", address))
    }

    #[wasm_bindgen(js_name=validate)]
    pub fn validate(address: &str) -> bool {
        Self::try_from(address).is_ok()
    }

    /// Convert an address to a string.
    #[wasm_bindgen(js_name = toString)]
    pub fn address_to_string(&self) -> String {
        self.into()
    }

    #[wasm_bindgen(getter, js_name = "version")]
    pub fn version_to_string(&self) -> String {
        self.version.to_string()
    }

    #[wasm_bindgen(getter, js_name = "prefix")]
    pub fn prefix_to_string(&self) -> String {
        self.prefix.to_string()
    }

    #[wasm_bindgen(setter, js_name = "setPrefix")]
    pub fn set_prefix_from_str(&mut self, prefix: &str) {
        self.prefix = Prefix::try_from(prefix).unwrap_or_else(|err| panic!("Address::prefix() - invalid prefix `{prefix}`: {err}"));
    }

    #[wasm_bindgen(getter, js_name = "payload")]
    pub fn payload_to_string(&self) -> String {
        self.encode_payload()
    }

    pub fn short(&self, n: usize) -> String {
        let payload = self.encode_payload();
        let n = std::cmp::min(n, payload.len() / 4);
        format!("{}:{}....{}", self.prefix, &payload[0..n], &payload[payload.len() - n..])
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
        Ok(Self::new(prefix, version, &payload))
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
                Address::try_from(string)
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
            Err(AddressError::InvalidAddressArray)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_roundtrip() {
        use Version::*;
        use Prefix::*;

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
            let address = Address::new(prefix, version, &payload);
            let encoded = address.to_string();
            let decoded: Address = encoded.parse().expect("Address decode failed");
            assert_eq!(decoded, address, "Roundtrip mismatch: {encoded}");
        }
    }
    #[test]
    fn invalid_prefix_should_fail() {
        let invalid = "wrongprefix:qpauqsvk7yf9...";
        let result: Result<Address, _> = invalid.parse();
        assert!(matches!(result, Err(AddressError::InvalidPrefix(_))));
    }

    #[test]
    fn missing_colon_should_fail() {
        let invalid = "tonditestqpauqsvk7yf9...";
        let result: Result<Address, _> = invalid.parse();
        assert_eq!(result, Err(AddressError::MissingPrefix));
    }

    #[test]
    fn bad_checksum_should_fail() {
        // Modify one character of a valid address
        let valid = Address::new(Prefix::Testnet, Version::PubKey, &[0u8; 32]).to_string();
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
