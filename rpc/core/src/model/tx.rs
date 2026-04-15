use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use spora_addresses::{Address, Version as AddressVersion};
use spora_consensus_core::cell_diff::CellMeta;
use spora_consensus_core::tx::{CellInput, CellOutput, Script, ScriptClass, TransactionId, TransactionIndexType, TransactionOutpoint};
use spora_utils::{hex::ToHex, serde_bytes_fixed, serde_bytes_fixed_ref};
use std::{
    fmt::{Display, Formatter},
    str::FromStr,
};
use workflow_serializer::prelude::*;

use crate::prelude::{RpcHash, RpcScriptClass};

mod option_hex_serde {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &Option<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(bytes) => serializer.serialize_some(&hex::encode(bytes)),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Vec<u8>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Option::<String>::deserialize(deserializer)?;
        value.map(|hex| hex::decode(&hex).map_err(serde::de::Error::custom)).transpose()
    }
}

/// Represents the ID of a Spora transaction
pub type RpcTransactionId = TransactionId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcScript {
    #[serde(with = "serde_bytes_fixed")]
    pub code_hash: [u8; 32],
    pub hash_type: u8,
    #[serde(with = "hex::serde")]
    pub args: Vec<u8>,
}

impl From<Script> for RpcScript {
    fn from(script: Script) -> Self {
        Self { code_hash: script.code_hash, hash_type: script.hash_type, args: script.args }
    }
}

impl From<&Script> for RpcScript {
    fn from(script: &Script) -> Self {
        Self { code_hash: script.code_hash, hash_type: script.hash_type, args: script.args.clone() }
    }
}

impl From<RpcScript> for Script {
    fn from(script: RpcScript) -> Self {
        Script::new(script.code_hash, script.hash_type, script.args)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcCellEntry {
    pub amount: u64,
    pub capacity: u64,
    pub data_bytes: u64,
    pub lock_hash: [u8; 32],
    pub type_hash: Option<[u8; 32]>,
    pub data_hash: [u8; 32],
    pub block_daa_score: u64,
    pub is_coinbase: bool,
}

impl RpcCellEntry {
    pub fn new(amount: u64, block_daa_score: u64, is_coinbase: bool) -> Self {
        Self {
            amount,
            capacity: amount,
            data_bytes: 0,
            lock_hash: [0; 32],
            type_hash: None,
            data_hash: [0; 32],
            block_daa_score,
            is_coinbase,
        }
    }

    pub fn with_cell_metadata(
        mut self,
        capacity: u64,
        data_bytes: u64,
        lock_hash: [u8; 32],
        type_hash: Option<[u8; 32]>,
        data_hash: [u8; 32],
    ) -> Self {
        self.amount = capacity;
        self.capacity = capacity;
        self.data_bytes = data_bytes;
        self.lock_hash = lock_hash;
        self.type_hash = type_hash;
        self.data_hash = data_hash;
        self
    }
}

impl From<CellMeta> for RpcCellEntry {
    fn from(entry: CellMeta) -> Self {
        let metadata = entry.embedded_cell_metadata().expect("RpcCellEntry requires canonical Cell metadata");
        Self {
            amount: entry.amount(),
            capacity: entry.capacity(),
            data_bytes: metadata.data_bytes,
            lock_hash: metadata.lock_hash,
            type_hash: metadata.type_hash,
            data_hash: metadata.data_hash,
            block_daa_score: entry.block_daa_score,
            is_coinbase: entry.is_cellbase,
        }
    }
}

impl From<RpcCellEntry> for CellMeta {
    fn from(entry: RpcCellEntry) -> Self {
        CellMeta::from_cell_metadata(
            entry.capacity,
            entry.data_bytes,
            entry.lock_hash,
            entry.type_hash,
            entry.data_hash,
            entry.block_daa_score,
            entry.is_coinbase,
        )
    }
}

impl Serializer for RpcCellEntry {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &3, writer)?;
        store!(u64, &self.amount, writer)?;
        store!(u64, &self.capacity, writer)?;
        store!(u64, &self.data_bytes, writer)?;
        store!([u8; 32], &self.lock_hash, writer)?;
        store!(Option<[u8; 32]>, &self.type_hash, writer)?;
        store!([u8; 32], &self.data_hash, writer)?;
        store!(u64, &self.block_daa_score, writer)?;
        store!(bool, &self.is_coinbase, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcCellEntry {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u8, reader)?;
        if version < 3 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "RpcCellEntry version < 3 is no longer supported"));
        }
        let amount = load!(u64, reader)?;
        let capacity = load!(u64, reader)?;
        let data_bytes = load!(u64, reader)?;
        let lock_hash = load!([u8; 32], reader)?;
        let type_hash = load!(Option<[u8; 32]>, reader)?;
        let data_hash = load!([u8; 32], reader)?;
        let block_daa_score = load!(u64, reader)?;
        let is_coinbase = load!(bool, reader)?;

        Ok(Self { amount, capacity, data_bytes, lock_hash, type_hash, data_hash, block_daa_score, is_coinbase })
    }
}

/// Represents a Spora transaction outpoint
#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionOutpoint {
    #[serde(with = "serde_bytes_fixed_ref")]
    pub transaction_id: TransactionId,
    pub index: TransactionIndexType,
}

impl From<TransactionOutpoint> for RpcTransactionOutpoint {
    fn from(outpoint: TransactionOutpoint) -> Self {
        Self { transaction_id: TransactionId::from_bytes(outpoint.tx_hash), index: outpoint.index }
    }
}

impl From<RpcTransactionOutpoint> for TransactionOutpoint {
    fn from(outpoint: RpcTransactionOutpoint) -> Self {
        Self::new(outpoint.transaction_id.as_bytes(), outpoint.index)
    }
}

impl From<spora_consensus_client::TransactionOutpoint> for RpcTransactionOutpoint {
    fn from(outpoint: spora_consensus_client::TransactionOutpoint) -> Self {
        TransactionOutpoint::from(outpoint).into()
    }
}

impl From<RpcTransactionOutpoint> for spora_consensus_client::TransactionOutpoint {
    fn from(outpoint: RpcTransactionOutpoint) -> Self {
        TransactionOutpoint::from(outpoint).into()
    }
}

impl Serializer for RpcTransactionOutpoint {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(TransactionId, &self.transaction_id, writer)?;
        store!(TransactionIndexType, &self.index, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionOutpoint {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let transaction_id = load!(TransactionId, reader)?;
        let index = load!(TransactionIndexType, reader)?;

        Ok(Self { transaction_id, index })
    }
}

/// Represents a Spora transaction input
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionInput {
    pub previous_outpoint: RpcTransactionOutpoint,
    pub since: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty", with = "hex::serde")]
    pub witness: Vec<u8>,
    pub verbose_data: Option<RpcTransactionInputVerboseData>,
}

impl std::fmt::Debug for RpcTransactionInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RpcTransactionInput")
            .field("previous_outpoint", &self.previous_outpoint)
            .field("since", &self.since)
            .field("witness", &self.witness.to_hex())
            .field("verbose_data", &self.verbose_data)
            .finish()
    }
}

impl RpcTransactionInput {
    pub fn from_cell_ref(input: &CellInput, witness: Vec<u8>) -> Self {
        Self { previous_outpoint: input.previous_output.into(), since: input.since, witness, verbose_data: None }
    }

    pub fn from_cell_refs(inputs: &[CellInput], witnesses: &[Vec<u8>]) -> Vec<Self> {
        inputs
            .iter()
            .enumerate()
            .map(|(index, input)| Self::from_cell_ref(input, witnesses.get(index).cloned().unwrap_or_default()))
            .collect()
    }
}

impl Serializer for RpcTransactionInput {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &3, writer)?;
        serialize!(RpcTransactionOutpoint, &self.previous_outpoint, writer)?;
        store!(u64, &self.since, writer)?;
        store!(Vec<u8>, &self.witness, writer)?;
        serialize!(Option<RpcTransactionInputVerboseData>, &self.verbose_data, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionInput {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u8, reader)?;
        let previous_outpoint = deserialize!(RpcTransactionOutpoint, reader)?;
        if version < 3 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "RpcTransactionInput version < 3 is no longer supported",
            ));
        }
        let since = load!(u64, reader)?;
        let witness = load!(Vec<u8>, reader)?;
        let verbose_data = deserialize!(Option<RpcTransactionInputVerboseData>, reader)?;

        Ok(Self { previous_outpoint, since, witness, verbose_data })
    }
}

/// Represent Spora transaction input verbose data
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionInputVerboseData {}

impl Serializer for RpcTransactionInputVerboseData {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcTransactionInputVerboseData {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        Ok(Self {})
    }
}

/// Represents a transaction output in the RPC model
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionOutput {
    pub value: u64,
    pub capacity: Option<u64>,
    pub data_bytes: Option<u64>,
    pub lock_hash: Option<[u8; 32]>,
    pub type_hash: Option<[u8; 32]>,
    pub data_hash: Option<[u8; 32]>,
    pub lock_script: RpcScript,
    pub type_script: Option<RpcScript>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "option_hex_serde")]
    pub output_data: Option<Vec<u8>>,
    pub verbose_data: Option<RpcTransactionOutputVerboseData>,
}

impl RpcTransactionOutput {
    pub fn from_cell_output(output: &CellOutput, output_data: &[u8]) -> Self {
        let data_hash = *blake3::hash(output_data).as_bytes();
        Self {
            value: output.capacity,
            capacity: Some(output.capacity),
            data_bytes: Some(output_data.len() as u64),
            lock_hash: Some(output.lock.hash()),
            type_hash: output.type_.as_ref().map(|script| script.hash()),
            data_hash: Some(data_hash),
            lock_script: (&output.lock).into(),
            type_script: output.type_.as_ref().map(Into::into),
            output_data: (!output_data.is_empty()).then(|| output_data.to_vec()),
            verbose_data: None,
        }
    }

    pub fn from_cell_outputs(outputs: &[CellOutput], outputs_data: &[Vec<u8>]) -> Vec<Self> {
        outputs
            .iter()
            .enumerate()
            .map(|(index, output)| Self::from_cell_output(output, outputs_data.get(index).map(Vec::as_slice).unwrap_or(&[])))
            .collect()
    }
}

impl Serializer for RpcTransactionOutput {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &3, writer)?;
        store!(u64, &self.value, writer)?;
        store!(Option<u64>, &self.capacity, writer)?;
        store!(Option<u64>, &self.data_bytes, writer)?;
        store!(Option<[u8; 32]>, &self.lock_hash, writer)?;
        store!(Option<[u8; 32]>, &self.type_hash, writer)?;
        store!(Option<[u8; 32]>, &self.data_hash, writer)?;
        store!(RpcScript, &self.lock_script, writer)?;
        store!(Option<RpcScript>, &self.type_script, writer)?;
        store!(Option<Vec<u8>>, &self.output_data, writer)?;
        serialize!(Option<RpcTransactionOutputVerboseData>, &self.verbose_data, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionOutput {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u8, reader)?;
        let value = load!(u64, reader)?;
        let (capacity, data_bytes, lock_hash, type_hash, data_hash) = if version >= 2 {
            (
                load!(Option<u64>, reader)?,
                load!(Option<u64>, reader)?,
                load!(Option<[u8; 32]>, reader)?,
                load!(Option<[u8; 32]>, reader)?,
                load!(Option<[u8; 32]>, reader)?,
            )
        } else {
            (None, None, None, None, None)
        };
        let (lock_script, type_script, output_data) = if version >= 3 {
            (load!(RpcScript, reader)?, load!(Option<RpcScript>, reader)?, load!(Option<Vec<u8>>, reader)?)
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "RpcTransactionOutput version < 3 is no longer supported",
            ));
        };
        let verbose_data = deserialize!(Option<RpcTransactionOutputVerboseData>, reader)?;

        Ok(Self { value, capacity, data_bytes, lock_hash, type_hash, data_hash, lock_script, type_script, output_data, verbose_data })
    }
}

/// Represent Spora transaction output verbose data
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionOutputVerboseData {
    pub lock_script_type: RpcScriptClass,
    pub lock_script_address: Address,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_lock_kind: Option<RpcResolvedLockKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_address_kind: Option<RpcResolvedAddressKind>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum RpcResolvedLockKind {
    NonStandard = 0,
    StdSingle = 1,
    StdSingleECDSA = 2,
    Account = 3,
    FullScript = 4,
}

impl Display for RpcResolvedLockKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            RpcResolvedLockKind::NonStandard => "nonstandard",
            RpcResolvedLockKind::StdSingle => "std_single",
            RpcResolvedLockKind::StdSingleECDSA => "std_single_ecdsa",
            RpcResolvedLockKind::Account => "account",
            RpcResolvedLockKind::FullScript => "full_script",
        };
        f.write_str(label)
    }
}

impl FromStr for RpcResolvedLockKind {
    type Err = crate::RpcError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "nonstandard" => Ok(Self::NonStandard),
            "std_single" => Ok(Self::StdSingle),
            "std_single_ecdsa" => Ok(Self::StdSingleECDSA),
            "account" => Ok(Self::Account),
            "full_script" => Ok(Self::FullScript),
            _ => Err(crate::RpcError::General(format!("invalid resolved lock kind: {value}"))),
        }
    }
}

impl From<ScriptClass> for RpcResolvedLockKind {
    fn from(value: ScriptClass) -> Self {
        match value {
            ScriptClass::NonStandard => Self::NonStandard,
            ScriptClass::StdSingle => Self::StdSingle,
            ScriptClass::StdSingleECDSA => Self::StdSingleECDSA,
            ScriptClass::Account => Self::Account,
            ScriptClass::FullScript => Self::FullScript,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum RpcResolvedAddressKind {
    StdSingle = 0,
    StdSingleECDSA = 1,
    Account = 2,
    FullScript = 3,
}

impl Display for RpcResolvedAddressKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            RpcResolvedAddressKind::StdSingle => "std_single",
            RpcResolvedAddressKind::StdSingleECDSA => "std_single_ecdsa",
            RpcResolvedAddressKind::Account => "account",
            RpcResolvedAddressKind::FullScript => "full_script",
        };
        f.write_str(label)
    }
}

impl FromStr for RpcResolvedAddressKind {
    type Err = crate::RpcError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "std_single" => Ok(Self::StdSingle),
            "std_single_ecdsa" => Ok(Self::StdSingleECDSA),
            "account" => Ok(Self::Account),
            "full_script" => Ok(Self::FullScript),
            _ => Err(crate::RpcError::General(format!("invalid resolved address kind: {value}"))),
        }
    }
}

impl From<AddressVersion> for RpcResolvedAddressKind {
    fn from(value: AddressVersion) -> Self {
        match value {
            AddressVersion::StdSingle => Self::StdSingle,
            AddressVersion::StdSingleECDSA => Self::StdSingleECDSA,
            AddressVersion::Account => Self::Account,
            AddressVersion::FullScript => Self::FullScript,
        }
    }
}

impl Serializer for RpcTransactionOutputVerboseData {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &2, writer)?;
        store!(RpcScriptClass, &self.lock_script_type, writer)?;
        store!(Address, &self.lock_script_address, writer)?;
        store!(Option<RpcResolvedLockKind>, &self.resolved_lock_kind, writer)?;
        store!(Option<RpcResolvedAddressKind>, &self.resolved_address_kind, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionOutputVerboseData {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u8, reader)?;
        let lock_script_type = load!(RpcScriptClass, reader)?;
        let lock_script_address = load!(Address, reader)?;
        let (resolved_lock_kind, resolved_address_kind) = if version >= 2 {
            (load!(Option<RpcResolvedLockKind>, reader)?, load!(Option<RpcResolvedAddressKind>, reader)?)
        } else {
            (None, None)
        };

        Ok(Self { lock_script_type, lock_script_address, resolved_lock_kind, resolved_address_kind })
    }
}

/// Represents a Spora transaction
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransaction {
    pub version: u32,
    pub inputs: Vec<RpcTransactionInput>,
    pub outputs: Vec<RpcTransactionOutput>,
    #[serde(with = "hex::serde")]
    pub payload: Vec<u8>,
    /// Best-available one-dimensional selection mass used for feerate and template ranking.
    pub mass: u64,
    pub verbose_data: Option<RpcTransactionVerboseData>,
}

impl std::fmt::Debug for RpcTransaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RpcTransaction")
            .field("version", &self.version)
            .field("payload", &self.payload.to_hex())
            .field("mass", &self.mass)
            .field("inputs", &self.inputs) // Inputs and outputs are placed purposely at the end for better debug visibility 
            .field("outputs", &self.outputs)
            .field("verbose_data", &self.verbose_data)
            .finish()
    }
}

impl Serializer for RpcTransaction {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &3, writer)?;
        store!(u32, &self.version, writer)?;
        serialize!(Vec<RpcTransactionInput>, &self.inputs, writer)?;
        serialize!(Vec<RpcTransactionOutput>, &self.outputs, writer)?;
        store!(Vec<u8>, &self.payload, writer)?;
        store!(u64, &self.mass, writer)?;
        serialize!(Option<RpcTransactionVerboseData>, &self.verbose_data, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransaction {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let struct_version = load!(u16, reader)?;
        let version = load!(u32, reader)?;
        let inputs = deserialize!(Vec<RpcTransactionInput>, reader)?;
        let outputs = deserialize!(Vec<RpcTransactionOutput>, reader)?;
        if struct_version != 3 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unsupported RpcTransaction serialization version: {struct_version}"),
            ));
        }
        let payload = load!(Vec<u8>, reader)?;
        let mass = load!(u64, reader)?;
        let verbose_data = deserialize!(Option<RpcTransactionVerboseData>, reader)?;

        Ok(Self { version, inputs, outputs, payload, mass, verbose_data })
    }
}

/// Represent Spora transaction verbose data
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionVerboseData {
    pub transaction_id: RpcTransactionId,
    pub hash: RpcHash,
    /// Effective compute-side mass after applying the VM-cycles projection when available.
    pub compute_mass: u64,
    /// Non-contextual transient mass used for relay and block-body occupancy accounting.
    pub transient_mass: Option<u64>,
    /// Contextual storage mass when the transaction inputs are resolved.
    pub storage_mass: Option<u64>,
    /// Actual VM-verified script cycles when available.
    pub verified_cycles: Option<u64>,
    pub block_hash: RpcHash,
    pub block_time: u64,
}

impl Serializer for RpcTransactionVerboseData {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &2, writer)?;
        store!(RpcTransactionId, &self.transaction_id, writer)?;
        store!(RpcHash, &self.hash, writer)?;
        store!(u64, &self.compute_mass, writer)?;
        store!(Option<u64>, &self.transient_mass, writer)?;
        store!(Option<u64>, &self.storage_mass, writer)?;
        store!(Option<u64>, &self.verified_cycles, writer)?;
        store!(RpcHash, &self.block_hash, writer)?;
        store!(u64, &self.block_time, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionVerboseData {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u8, reader)?;
        let transaction_id = load!(RpcTransactionId, reader)?;
        let hash = load!(RpcHash, reader)?;
        let compute_mass = load!(u64, reader)?;
        let (transient_mass, storage_mass, verified_cycles) = if version >= 2 {
            (load!(Option<u64>, reader)?, load!(Option<u64>, reader)?, load!(Option<u64>, reader)?)
        } else {
            (None, None, None)
        };
        let block_hash = load!(RpcHash, reader)?;
        let block_time = load!(u64, reader)?;

        Ok(Self { transaction_id, hash, compute_mass, transient_mass, storage_mass, verified_cycles, block_hash, block_time })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpc_transaction_verbose_data_deserializes_v1_payloads() {
        let legacy = RpcTransactionVerboseData {
            transaction_id: [0x11; 32].into(),
            hash: [0x22; 32].into(),
            compute_mass: 123,
            transient_mass: Some(456),
            storage_mass: Some(789),
            verified_cycles: Some(999),
            block_hash: [0x33; 32].into(),
            block_time: 456,
        };

        let mut bytes = Vec::new();
        store!(u8, &1, &mut bytes).unwrap();
        store!(RpcTransactionId, &legacy.transaction_id, &mut bytes).unwrap();
        store!(RpcHash, &legacy.hash, &mut bytes).unwrap();
        store!(u64, &legacy.compute_mass, &mut bytes).unwrap();
        store!(RpcHash, &legacy.block_hash, &mut bytes).unwrap();
        store!(u64, &legacy.block_time, &mut bytes).unwrap();

        let decoded =
            <RpcTransactionVerboseData as workflow_serializer::serializer::Deserializer>::deserialize(&mut bytes.as_slice()).unwrap();
        assert_eq!(decoded.transaction_id, legacy.transaction_id);
        assert_eq!(decoded.hash, legacy.hash);
        assert_eq!(decoded.compute_mass, legacy.compute_mass);
        assert_eq!(decoded.transient_mass, None);
        assert_eq!(decoded.storage_mass, None);
        assert_eq!(decoded.verified_cycles, None);
        assert_eq!(decoded.block_hash, legacy.block_hash);
        assert_eq!(decoded.block_time, legacy.block_time);
    }
}

/// Represents accepted transaction ids
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcAcceptedTransactionIds {
    pub accepting_block_hash: RpcHash,
    pub accepted_transaction_ids: Vec<RpcTransactionId>,
}
