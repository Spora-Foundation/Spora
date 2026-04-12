use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use spora_addresses::Address;
use spora_consensus_core::cell_metadata::cell_metadata_placeholder_script_public_key_with_metadata;
use spora_consensus_core::tx::{
    cell_entry_legacy_script_public_key, cell_meta_from_legacy_output, CellEntry, CellOut, CellRef, ScriptPublicKey, ScriptVec,
    TransactionId, TransactionIndexType, TransactionOutpoint,
};
use spora_utils::{hex::ToHex, serde_bytes_fixed_ref};
use workflow_serializer::prelude::*;

use crate::prelude::{RpcHash, RpcScriptClass, RpcSubnetworkId};

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

pub type RpcScriptVec = ScriptVec;
pub type RpcScriptPublicKey = ScriptPublicKey;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcCellEntry {
    pub amount: u64,
    pub capacity: u64,
    pub data_bytes: u64,
    pub lock_hash: [u8; 32],
    pub type_hash: Option<[u8; 32]>,
    pub data_hash: [u8; 32],
    pub script_public_key: ScriptPublicKey,
    pub block_daa_score: u64,
    pub is_coinbase: bool,
}

impl RpcCellEntry {
    pub fn new(amount: u64, script_public_key: ScriptPublicKey, block_daa_score: u64, is_coinbase: bool) -> Self {
        Self {
            amount,
            capacity: amount,
            data_bytes: 0,
            lock_hash: [0; 32],
            type_hash: None,
            data_hash: [0; 32],
            script_public_key,
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

impl From<CellEntry> for RpcCellEntry {
    fn from(entry: CellEntry) -> Self {
        let metadata = entry.embedded_cell_metadata();
        Self {
            amount: entry.amount(),
            capacity: entry.capacity(),
            data_bytes: metadata.map(|m| m.data_bytes).unwrap_or_default(),
            lock_hash: metadata.map(|m| m.lock_hash).unwrap_or([0; 32]),
            type_hash: metadata.and_then(|m| m.type_hash),
            data_hash: metadata.map(|m| m.data_hash).unwrap_or([0; 32]),
            script_public_key: cell_entry_legacy_script_public_key(&entry),
            block_daa_score: entry.block_daa_score,
            is_coinbase: entry.is_cellbase,
        }
    }
}

impl From<RpcCellEntry> for CellEntry {
    fn from(entry: RpcCellEntry) -> Self {
        let has_canonical_metadata = entry.capacity != entry.amount
            || entry.data_bytes != 0
            || entry.lock_hash != [0; 32]
            || entry.type_hash.is_some()
            || entry.data_hash != [0; 32];

        if has_canonical_metadata {
            CellEntry::from_cell_metadata(
                entry.capacity,
                entry.data_bytes,
                entry.lock_hash,
                entry.type_hash,
                entry.data_hash,
                entry.block_daa_score,
                entry.is_coinbase,
            )
        } else {
            cell_meta_from_legacy_output(entry.amount, &entry.script_public_key, entry.block_daa_score, entry.is_coinbase)
        }
    }
}

impl Serializer for RpcCellEntry {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &2, writer)?;
        store!(u64, &self.amount, writer)?;
        store!(u64, &self.capacity, writer)?;
        store!(u64, &self.data_bytes, writer)?;
        store!([u8; 32], &self.lock_hash, writer)?;
        store!(Option<[u8; 32]>, &self.type_hash, writer)?;
        store!([u8; 32], &self.data_hash, writer)?;
        store!(ScriptPublicKey, &self.script_public_key, writer)?;
        store!(u64, &self.block_daa_score, writer)?;
        store!(bool, &self.is_coinbase, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcCellEntry {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u8, reader)?;
        let amount = load!(u64, reader)?;
        let (capacity, data_bytes, lock_hash, type_hash, data_hash) = if version >= 2 {
            (
                load!(u64, reader)?,
                load!(u64, reader)?,
                load!([u8; 32], reader)?,
                load!(Option<[u8; 32]>, reader)?,
                load!([u8; 32], reader)?,
            )
        } else {
            (amount, 0, [0; 32], None, [0; 32])
        };
        let script_public_key = load!(ScriptPublicKey, reader)?;
        let block_daa_score = load!(u64, reader)?;
        let is_coinbase = load!(bool, reader)?;

        Ok(Self { amount, capacity, data_bytes, lock_hash, type_hash, data_hash, script_public_key, block_daa_score, is_coinbase })
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
    #[serde(with = "hex::serde")]
    pub signature_script: Vec<u8>,
    pub sequence: u64,
    pub sig_op_count: u8,
    pub since: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "option_hex_serde")]
    pub witness: Option<Vec<u8>>,
    pub verbose_data: Option<RpcTransactionInputVerboseData>,
}

impl std::fmt::Debug for RpcTransactionInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RpcTransactionInput")
            .field("previous_outpoint", &self.previous_outpoint)
            .field("signature_script", &self.signature_script.to_hex())
            .field("sequence", &self.sequence)
            .field("sig_op_count", &self.sig_op_count)
            .field("since", &self.since)
            .field("witness", &self.witness.as_ref().map(|w| w.to_hex()))
            .field("verbose_data", &self.verbose_data)
            .finish()
    }
}

impl RpcTransactionInput {
    pub fn with_cell_input(mut self, since: u64, witness: Vec<u8>) -> Self {
        self.since = Some(since);
        self.witness = Some(witness);
        self
    }

    pub fn from_cell_ref(input: &CellRef, witness: Vec<u8>) -> Self {
        Self {
            previous_outpoint: input.out_point.into(),
            signature_script: witness.clone(),
            sequence: input.since,
            sig_op_count: 0,
            since: Some(input.since),
            witness: Some(witness),
            verbose_data: None,
        }
    }

    pub fn from_cell_refs(inputs: &[CellRef], witnesses: &[Vec<u8>]) -> Vec<Self> {
        inputs
            .iter()
            .enumerate()
            .map(|(index, input)| Self::from_cell_ref(input, witnesses.get(index).cloned().unwrap_or_default()))
            .collect()
    }
}

impl Serializer for RpcTransactionInput {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &2, writer)?;
        serialize!(RpcTransactionOutpoint, &self.previous_outpoint, writer)?;
        store!(Vec<u8>, &self.signature_script, writer)?;
        store!(u64, &self.sequence, writer)?;
        store!(u8, &self.sig_op_count, writer)?;
        store!(Option<u64>, &self.since, writer)?;
        store!(Option<Vec<u8>>, &self.witness, writer)?;
        serialize!(Option<RpcTransactionInputVerboseData>, &self.verbose_data, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionInput {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u8, reader)?;
        let previous_outpoint = deserialize!(RpcTransactionOutpoint, reader)?;
        let signature_script = load!(Vec<u8>, reader)?;
        let sequence = load!(u64, reader)?;
        let sig_op_count = load!(u8, reader)?;
        let (since, witness) = match version {
            1 => (None, None),
            _ => (load!(Option<u64>, reader)?, load!(Option<Vec<u8>>, reader)?),
        };
        let verbose_data = deserialize!(Option<RpcTransactionInputVerboseData>, reader)?;

        Ok(Self { previous_outpoint, signature_script, sequence, sig_op_count, since, witness, verbose_data })
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

/// Represents a Sporad transaction output
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionOutput {
    pub value: u64,
    pub capacity: Option<u64>,
    pub data_bytes: Option<u64>,
    pub lock_hash: Option<[u8; 32]>,
    pub type_hash: Option<[u8; 32]>,
    pub data_hash: Option<[u8; 32]>,
    pub script_public_key: RpcScriptPublicKey,
    pub verbose_data: Option<RpcTransactionOutputVerboseData>,
}

impl RpcTransactionOutput {
    pub fn from_cell_output(output: &CellOut, output_data: &[u8]) -> Self {
        let data_hash = *blake3::hash(output_data).as_bytes();
        Self {
            value: output.capacity,
            capacity: Some(output.capacity),
            data_bytes: Some(output_data.len() as u64),
            lock_hash: Some(output.lock.hash()),
            type_hash: output.type_.as_ref().map(|script| script.hash()),
            data_hash: Some(data_hash),
            script_public_key: cell_metadata_placeholder_script_public_key_with_metadata(
                output.lock.hash(),
                output.type_.as_ref().map(|script| script.hash()),
                data_hash,
                output_data.len() as u64,
            ),
            verbose_data: None,
        }
    }

    pub fn from_cell_outputs(outputs: &[CellOut], outputs_data: &[Vec<u8>]) -> Vec<Self> {
        outputs
            .iter()
            .enumerate()
            .map(|(index, output)| Self::from_cell_output(output, outputs_data.get(index).map(Vec::as_slice).unwrap_or(&[])))
            .collect()
    }
}

impl Serializer for RpcTransactionOutput {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &2, writer)?;
        store!(u64, &self.value, writer)?;
        store!(Option<u64>, &self.capacity, writer)?;
        store!(Option<u64>, &self.data_bytes, writer)?;
        store!(Option<[u8; 32]>, &self.lock_hash, writer)?;
        store!(Option<[u8; 32]>, &self.type_hash, writer)?;
        store!(Option<[u8; 32]>, &self.data_hash, writer)?;
        store!(RpcScriptPublicKey, &self.script_public_key, writer)?;
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
        let script_public_key = load!(RpcScriptPublicKey, reader)?;
        let verbose_data = deserialize!(Option<RpcTransactionOutputVerboseData>, reader)?;

        Ok(Self { value, capacity, data_bytes, lock_hash, type_hash, data_hash, script_public_key, verbose_data })
    }
}

/// Represent Spora transaction output verbose data
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionOutputVerboseData {
    pub script_public_key_type: RpcScriptClass,
    pub script_public_key_address: Address,
}

impl Serializer for RpcTransactionOutputVerboseData {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(RpcScriptClass, &self.script_public_key_type, writer)?;
        store!(Address, &self.script_public_key_address, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionOutputVerboseData {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let script_public_key_type = load!(RpcScriptClass, reader)?;
        let script_public_key_address = load!(Address, reader)?;

        Ok(Self { script_public_key_type, script_public_key_address })
    }
}

/// Represents a Spora transaction
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransaction {
    pub version: u16,
    pub inputs: Vec<RpcTransactionInput>,
    pub outputs: Vec<RpcTransactionOutput>,
    pub lock_time: u64,
    pub subnetwork_id: RpcSubnetworkId,
    pub gas: u64,
    #[serde(with = "hex::serde")]
    pub payload: Vec<u8>,
    pub mass: u64,
    pub verbose_data: Option<RpcTransactionVerboseData>,
}

impl std::fmt::Debug for RpcTransaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RpcTransaction")
            .field("version", &self.version)
            .field("lock_time", &self.lock_time)
            .field("subnetwork_id", &self.subnetwork_id)
            .field("gas", &self.gas)
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
        store!(u16, &1, writer)?;
        store!(u16, &self.version, writer)?;
        serialize!(Vec<RpcTransactionInput>, &self.inputs, writer)?;
        serialize!(Vec<RpcTransactionOutput>, &self.outputs, writer)?;
        store!(u64, &self.lock_time, writer)?;
        store!(RpcSubnetworkId, &self.subnetwork_id, writer)?;
        store!(u64, &self.gas, writer)?;
        store!(Vec<u8>, &self.payload, writer)?;
        store!(u64, &self.mass, writer)?;
        serialize!(Option<RpcTransactionVerboseData>, &self.verbose_data, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransaction {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _struct_version = load!(u16, reader)?;
        let version = load!(u16, reader)?;
        let inputs = deserialize!(Vec<RpcTransactionInput>, reader)?;
        let outputs = deserialize!(Vec<RpcTransactionOutput>, reader)?;
        let lock_time = load!(u64, reader)?;
        let subnetwork_id = load!(RpcSubnetworkId, reader)?;
        let gas = load!(u64, reader)?;
        let payload = load!(Vec<u8>, reader)?;
        let mass = load!(u64, reader)?;
        let verbose_data = deserialize!(Option<RpcTransactionVerboseData>, reader)?;

        Ok(Self { version, inputs, outputs, lock_time, subnetwork_id, gas, payload, mass, verbose_data })
    }
}

/// Represent Spora transaction verbose data
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionVerboseData {
    pub transaction_id: RpcTransactionId,
    pub hash: RpcHash,
    pub compute_mass: u64,
    pub block_hash: RpcHash,
    pub block_time: u64,
}

impl Serializer for RpcTransactionVerboseData {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(RpcTransactionId, &self.transaction_id, writer)?;
        store!(RpcHash, &self.hash, writer)?;
        store!(u64, &self.compute_mass, writer)?;
        store!(RpcHash, &self.block_hash, writer)?;
        store!(u64, &self.block_time, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionVerboseData {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let transaction_id = load!(RpcTransactionId, reader)?;
        let hash = load!(RpcHash, reader)?;
        let compute_mass = load!(u64, reader)?;
        let block_hash = load!(RpcHash, reader)?;
        let block_time = load!(u64, reader)?;

        Ok(Self { transaction_id, hash, compute_mass, block_hash, block_time })
    }
}

/// Represents accepted transaction ids
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcAcceptedTransactionIds {
    pub accepting_block_hash: RpcHash,
    pub accepted_transaction_ids: Vec<RpcTransactionId>,
}
