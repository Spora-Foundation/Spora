use crate::{RpcCellEntry, RpcTransactionOutpoint};
use serde::{Deserialize, Serialize};
use workflow_serializer::prelude::*;

pub type RpcAddress = spora_addresses::Address;

/// Represents a Cell entry of an address returned by the `GetCellsByAddresses` RPC.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcCellsByAddressesEntry {
    pub address: Option<RpcAddress>,
    pub outpoint: RpcTransactionOutpoint,
    pub cell_entry: RpcCellEntry,
}

impl Serializer for RpcCellsByAddressesEntry {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?; // version
        store!(Option<RpcAddress>, &self.address, writer)?;
        serialize!(RpcTransactionOutpoint, &self.outpoint, writer)?;
        serialize!(RpcCellEntry, &self.cell_entry, writer)
    }
}

impl Deserializer for RpcCellsByAddressesEntry {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version: u8 = load!(u8, reader)?;
        let address = load!(Option<RpcAddress>, reader)?;
        let outpoint = deserialize!(RpcTransactionOutpoint, reader)?;
        let cell_entry = deserialize!(RpcCellEntry, reader)?;
        Ok(Self { address, outpoint, cell_entry })
    }
}

/// Represents a balance of an address returned by the `GetBalancesByAddresses` RPC.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcBalancesByAddressesEntry {
    pub address: RpcAddress,

    /// Balance of `address` if available
    pub balance: Option<u64>,
}

impl Serializer for RpcBalancesByAddressesEntry {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?; // version
        store!(RpcAddress, &self.address, writer)?;
        store!(Option<u64>, &self.balance, writer)
    }
}

impl Deserializer for RpcBalancesByAddressesEntry {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version: u8 = load!(u8, reader)?;
        let address = load!(RpcAddress, reader)?;
        let balance = load!(Option<u64>, reader)?;
        Ok(Self { address, balance })
    }
}
