use borsh::{BorshDeserialize, BorshSerialize};
use spora_consensus_core::subnets::SUBNETWORK_ID_SIZE;
use spora_utils::hex::{FromHex, ToHex};
use spora_utils::{serde_impl_deser_fixed_bytes_ref, serde_impl_ser_fixed_bytes_ref};
use std::fmt::{Debug, Display, Formatter};
use std::str::{self, FromStr};

#[derive(Clone, Copy, Default, Eq, PartialEq, Ord, PartialOrd, Hash, BorshSerialize, BorshDeserialize)]
pub struct RpcSubnetworkId([u8; SUBNETWORK_ID_SIZE]);

serde_impl_ser_fixed_bytes_ref!(RpcSubnetworkId, SUBNETWORK_ID_SIZE);
serde_impl_deser_fixed_bytes_ref!(RpcSubnetworkId, SUBNETWORK_ID_SIZE);

impl RpcSubnetworkId {
    pub const fn from_byte(byte: u8) -> Self {
        let mut bytes = [0u8; SUBNETWORK_ID_SIZE];
        bytes[0] = byte;
        Self(bytes)
    }

    pub const fn from_bytes(bytes: [u8; SUBNETWORK_ID_SIZE]) -> Self {
        Self(bytes)
    }

    pub const fn native() -> Self {
        Self::from_byte(0)
    }

    pub const fn coinbase() -> Self {
        Self::from_byte(1)
    }

    pub const fn as_bytes(self) -> [u8; SUBNETWORK_ID_SIZE] {
        self.0
    }
}

impl AsRef<[u8; SUBNETWORK_ID_SIZE]> for RpcSubnetworkId {
    fn as_ref(&self) -> &[u8; SUBNETWORK_ID_SIZE] {
        &self.0
    }
}

impl AsRef<[u8]> for RpcSubnetworkId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Debug for RpcSubnetworkId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RpcSubnetworkId").field("", &self.to_hex()).finish()
    }
}

impl Display for RpcSubnetworkId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut hex = [0u8; SUBNETWORK_ID_SIZE * 2];
        faster_hex::hex_encode(&self.0, &mut hex).expect("output size matches input size");
        f.write_str(str::from_utf8(&hex).expect("hex is valid UTF-8"))
    }
}

impl FromStr for RpcSubnetworkId {
    type Err = faster_hex::Error;

    fn from_str(hex_str: &str) -> Result<Self, Self::Err> {
        let mut bytes = [0u8; SUBNETWORK_ID_SIZE];
        faster_hex::hex_decode(hex_str.as_bytes(), &mut bytes)?;
        Ok(Self(bytes))
    }
}

impl ToHex for RpcSubnetworkId {
    fn to_hex(&self) -> String {
        let mut hex = [0u8; SUBNETWORK_ID_SIZE * 2];
        faster_hex::hex_encode(&self.0, &mut hex).expect("output size matches input size");
        str::from_utf8(&hex).expect("hex is valid UTF-8").to_string()
    }
}

impl FromHex for RpcSubnetworkId {
    type Error = faster_hex::Error;

    fn from_hex(hex_str: &str) -> Result<Self, Self::Error> {
        Self::from_str(hex_str)
    }
}

impl From<[u8; SUBNETWORK_ID_SIZE]> for RpcSubnetworkId {
    fn from(bytes: [u8; SUBNETWORK_ID_SIZE]) -> Self {
        Self(bytes)
    }
}

#[allow(deprecated)]
impl From<spora_consensus_core::subnets::SubnetworkId> for RpcSubnetworkId {
    fn from(value: spora_consensus_core::subnets::SubnetworkId) -> Self {
        Self::from_bytes(*value.as_ref())
    }
}

#[allow(deprecated)]
impl From<RpcSubnetworkId> for spora_consensus_core::subnets::SubnetworkId {
    fn from(value: RpcSubnetworkId) -> Self {
        Self::from_bytes(value.as_bytes())
    }
}
