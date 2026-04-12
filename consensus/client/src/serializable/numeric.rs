//!
//! This module implements transaction-related primitives for JSON serialization
//! where all large integer values (`u64`) are serialized to JSON using `serde` and
//! can exceed the largest integer value representable by the JavaScript `number` type.
//! (i.e. transactions serialized using this module can not be deserialized in JavaScript
//! but may be deserialized in other JSON-capable environments that support large integers)
//!

use crate::error::Error;
use crate::imports::*;
use crate::result::Result;
use crate::{CellEntry, CellEntryId, CellEntryReference, Transaction, TransactionInput, TransactionInputInner, TransactionOutpoint, TransactionOutpointInner, TransactionOutput};
use ahash::AHashMap;
use cctx::VerifiableTransaction;
use spora_addresses::Address;
use spora_consensus_core::subnets::SubnetworkId;
use spora_hashes::Hash;
use workflow_wasm::serde::{from_value, to_value};

pub type SignedTransactionIndexType = u32;

pub struct Options {
    pub include_cell_entry: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SerializableCellEntry {
    pub address: Option<Address>,
    pub amount: u64,
    #[serde(default)]
    pub capacity: Option<u64>,
    #[serde(default)]
    pub data_bytes: Option<u64>,
    #[serde(default)]
    pub lock_hash: Option<TransactionId>,
    #[serde(default)]
    pub type_hash: Option<TransactionId>,
    #[serde(default)]
    pub data_hash: Option<TransactionId>,
    pub script_public_key: ScriptPublicKey,
    pub block_daa_score: u64,
    pub is_coinbase: bool,
}

impl AsRef<SerializableCellEntry> for SerializableCellEntry {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl From<&CellEntryReference> for SerializableCellEntry {
    fn from(cell_entry: &CellEntryReference) -> Self {
        let cell_entry = cell_entry.cell.as_ref();
        Self {
            address: cell_entry.address.clone(),
            amount: cell_entry.amount,
            capacity: cell_entry.capacity,
            data_bytes: cell_entry.data_bytes,
            lock_hash: cell_entry.lock_hash,
            type_hash: cell_entry.type_hash,
            data_hash: cell_entry.data_hash,
            script_public_key: cell_entry.script_public_key.clone(),
            block_daa_score: cell_entry.block_daa_score,
            is_coinbase: cell_entry.is_coinbase,
        }
    }
}

impl From<&cctx::CellEntry> for SerializableCellEntry {
    fn from(cell_entry: &cctx::CellEntry) -> Self {
        let metadata = cell_entry.embedded_cell_metadata();
        Self {
            address: None,
            amount: cell_entry.amount(),
            capacity: metadata.map(|_| cell_entry.capacity()),
            data_bytes: metadata.map(|m| m.data_bytes),
            lock_hash: metadata.map(|m| m.lock_hash.into()),
            type_hash: metadata.and_then(|m| m.type_hash.map(Into::into)),
            data_hash: metadata.map(|m| m.data_hash.into()),
            script_public_key: cctx::cell_entry_legacy_script_public_key(cell_entry),
            block_daa_score: cell_entry.block_daa_score,
            is_coinbase: cell_entry.is_cellbase,
        }
    }
}

impl TryFrom<&SerializableCellEntry> for cctx::CellEntry {
    type Error = crate::error::Error;
    fn try_from(cell_entry: &SerializableCellEntry) -> Result<Self> {
        match (cell_entry.lock_hash, cell_entry.data_hash) {
            (Some(lock_hash), Some(data_hash)) => Ok(Self::from_cell_metadata(
                cell_entry.capacity.unwrap_or(cell_entry.amount),
                cell_entry.data_bytes.unwrap_or_default(),
                lock_hash.as_bytes(),
                cell_entry.type_hash.map(|hash| hash.as_bytes()),
                data_hash.as_bytes(),
                cell_entry.block_daa_score,
                cell_entry.is_coinbase,
            )),
            _ => Ok(cctx::cell_meta_from_legacy_output(
                cell_entry.amount,
                &cell_entry.script_public_key,
                cell_entry.block_daa_score,
                cell_entry.is_coinbase,
            )),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SerializableTransactionInput {
    pub transaction_id: TransactionId,
    pub index: SignedTransactionIndexType,
    pub sequence: u64,
    pub sig_op_count: u8,
    #[serde(with = "hex::serde")]
    // TODO - convert to Option<Vec<u8>> and use hex serialization over Option
    pub signature_script: Vec<u8>,
    pub cell_entry: SerializableCellEntry,
}

impl SerializableTransactionInput {
    #[allow(deprecated)]
    pub fn new(input: &cctx::TransactionInput, cell_entry: &cctx::CellEntry) -> Self {
        let cell_entry = SerializableCellEntry::from(cell_entry);

        Self {
            transaction_id: TransactionId::from_slice(&input.previous_outpoint.tx_hash),
            index: input.previous_outpoint.index,
            // TODO - convert signature_script to Option<Vec<u8>>
            // signature_script: (!input.signature_script.is_empty()).then_some(input.signature_script.clone()),
            signature_script: input.signature_script.clone(),
            sequence: input.sequence,
            sig_op_count: input.sig_op_count,
            cell_entry: cell_entry.clone(),
        }
    }

    /// Create from a Cell-model CellRef input with its witness data.
    pub fn from_cell_ref(input: &cctx::CellRef, witness: &[u8], cell_entry: &cctx::CellEntry) -> Self {
        let cell_entry = SerializableCellEntry::from(cell_entry);
        Self {
            transaction_id: TransactionId::from_slice(&input.out_point.tx_hash),
            index: input.out_point.index,
            signature_script: witness.to_vec(),
            sequence: input.since,
            sig_op_count: 1, // Cell model: implicit 1 sigop per input
            cell_entry,
        }
    }
}

impl TryFrom<&SerializableTransactionInput> for CellEntryReference {
    type Error = Error;
    fn try_from(input: &SerializableTransactionInput) -> Result<Self> {
        let outpoint = TransactionOutpoint::new(input.transaction_id, input.index);

        let cell_entry = CellEntry {
            outpoint,
            address: input.cell_entry.address.clone(),
            amount: input.cell_entry.amount,
            capacity: input.cell_entry.capacity,
            data_bytes: input.cell_entry.data_bytes,
            lock_hash: input.cell_entry.lock_hash,
            type_hash: input.cell_entry.type_hash,
            data_hash: input.cell_entry.data_hash,
            script_public_key: input.cell_entry.script_public_key.clone(),
            block_daa_score: input.cell_entry.block_daa_score,
            is_coinbase: input.cell_entry.is_coinbase,
        };

        Ok(Self { cell: Arc::new(cell_entry) })
    }
}

impl TryFrom<SerializableTransactionInput> for cctx::TransactionInput {
    type Error = Error;
    fn try_from(signable_input: SerializableTransactionInput) -> Result<Self> {
        Ok(Self {
            previous_outpoint: cctx::TransactionOutpoint {
                tx_hash: signable_input.transaction_id.as_bytes(),
                index: signable_input.index,
            },
            signature_script: signable_input.signature_script,
            sequence: signable_input.sequence,
            sig_op_count: signable_input.sig_op_count,
        })
    }
}

impl TryFrom<&SerializableTransactionInput> for TransactionInput {
    type Error = Error;
    fn try_from(serializable_input: &SerializableTransactionInput) -> Result<Self> {
        let cell_entry = CellEntryReference::try_from(serializable_input)?;

        let previous_outpoint = TransactionOutpoint::new(serializable_input.transaction_id, serializable_input.index);
        let inner = TransactionInputInner {
            previous_outpoint,
            // TODO - convert to Option<Vec<u8>> and use hex serialization over Option
            signature_script: (!serializable_input.signature_script.is_empty()).then_some(serializable_input.signature_script.clone()),
            sequence: serializable_input.sequence,
            sig_op_count: serializable_input.sig_op_count,
            cell_entry: Some(cell_entry),
        };

        Ok(TransactionInput::new_with_inner(inner))
    }
}

impl TryFrom<&TransactionInput> for SerializableTransactionInput {
    type Error = Error;
    fn try_from(input: &TransactionInput) -> Result<Self> {
        let inner = input.inner();
        let cell_entry = inner.cell_entry.as_ref().ok_or(Error::MissingCellEntry)?;
        let cell_entry = SerializableCellEntry::from(cell_entry);
        Ok(Self {
            transaction_id: inner.previous_outpoint.transaction_id(),
            index: inner.previous_outpoint.index(),
            // TODO - convert to Option<Vec<u8>> and use hex serialization over Option
            signature_script: inner.signature_script.clone().unwrap_or_default(),
            sequence: inner.sequence,
            sig_op_count: inner.sig_op_count,
            cell_entry,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SerializableTransactionOutput {
    pub value: u64,
    pub script_public_key: ScriptPublicKey,
}

impl From<cctx::TransactionOutput> for SerializableTransactionOutput {
    #[allow(deprecated)]
    fn from(output: cctx::TransactionOutput) -> Self {
        Self { value: output.value, script_public_key: output.script_public_key }
    }
}

impl From<&cctx::TransactionOutput> for SerializableTransactionOutput {
    #[allow(deprecated)]
    fn from(output: &cctx::TransactionOutput) -> Self {
        Self { value: output.value, script_public_key: output.script_public_key.clone() }
    }
}

/// Bridge from CellOut to legacy serializable output format.
impl From<cctx::CellOut> for SerializableTransactionOutput {
    fn from(output: cctx::CellOut) -> Self {
        // Use lock script bytes as the script_public_key payload
        let script_public_key = cctx::ScriptPublicKey::from_vec(0, output.lock.to_bytes());
        Self { value: output.capacity, script_public_key }
    }
}

impl From<&cctx::CellOut> for SerializableTransactionOutput {
    fn from(output: &cctx::CellOut) -> Self {
        let script_public_key = cctx::ScriptPublicKey::from_vec(0, output.lock.to_bytes());
        Self { value: output.capacity, script_public_key }
    }
}

impl TryFrom<SerializableTransactionOutput> for cctx::TransactionOutput {
    type Error = Error;
    fn try_from(output: SerializableTransactionOutput) -> Result<Self> {
        Ok(Self { value: output.value, script_public_key: output.script_public_key })
    }
}

impl TryFrom<&SerializableTransactionOutput> for TransactionOutput {
    type Error = Error;
    fn try_from(output: &SerializableTransactionOutput) -> Result<Self> {
        Ok(TransactionOutput::new(output.value, output.script_public_key.clone()))
    }
}

impl TryFrom<&TransactionOutput> for SerializableTransactionOutput {
    type Error = Error;
    fn try_from(output: &TransactionOutput) -> Result<Self> {
        let inner = output.inner();
        Ok(Self { value: inner.value, script_public_key: inner.script_public_key.clone() })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SerializableTransaction {
    // pub version: u32,
    pub id: TransactionId,
    pub version: u16,
    pub inputs: Vec<SerializableTransactionInput>,
    pub outputs: Vec<SerializableTransactionOutput>,
    pub lock_time: u64,
    pub gas: u64,
    #[serde(default)]
    pub mass: u64,
    pub subnetwork_id: SubnetworkId,
    #[serde(with = "hex::serde")]
    pub payload: Vec<u8>,
}

impl SerializableTransaction {
    pub fn serialize_to_object(&self) -> Result<JsValue> {
        Ok(to_value(self)?)
    }

    pub fn deserialize_from_object(object: JsValue) -> Result<Self> {
        Ok(from_value(object)?)
    }

    pub fn serialize_to_json(&self) -> Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn deserialize_from_json(json: &str) -> Result<Self> {
        Ok(serde_json::from_str(json)?)
    }

    pub fn from_signable_transaction(tx: &cctx::SignableTransaction) -> Result<Self> {
        let verifiable_tx = tx.as_verifiable();
        let mut inputs = vec![];
        let transaction = tx.as_ref(); // &CellTx
        for index in 0..transaction.inputs.len() {
            let input = &verifiable_tx.inputs()[index];
            let cell_entry = verifiable_tx.cell_entry(index).ok_or(Error::MissingLegacyCellEntry(index))?;
            let witness = transaction.witnesses.get(index).map(|w| w.as_slice()).unwrap_or_default();
            let input = SerializableTransactionInput::from_cell_ref(input, witness, cell_entry);
            inputs.push(input);
        }

        let outputs = transaction.outputs.clone();

        Ok(Self {
            inputs,
            outputs: outputs.into_iter().map(Into::into).collect(),
            version: transaction.version(),
            lock_time: transaction.lock_time(),
            subnetwork_id: Default::default(),
            gas: 0,
            mass: transaction.mass(),
            payload: transaction.payload().map(|p| p.to_vec()).unwrap_or_default(),
            id: Hash::from_bytes(transaction.id()),
        })
    }

    pub fn from_client_transaction(transaction: &Transaction) -> Result<Self> {
        let inner = transaction.inner();

        let inputs = inner.inputs.iter().map(TryFrom::try_from).collect::<Result<Vec<SerializableTransactionInput>>>()?;
        let outputs = inner.outputs.iter().map(TryFrom::try_from).collect::<Result<Vec<SerializableTransactionOutput>>>()?;

        Ok(Self {
            inputs,
            outputs,
            version: inner.version,
            lock_time: inner.lock_time,
            subnetwork_id: inner.subnetwork_id.clone(),
            gas: inner.gas,
            payload: inner.payload.clone(),
            mass: inner.mass,
            id: inner.id,
        })
    }

    #[allow(deprecated)]
    pub fn from_cctx_transaction(
        transaction: &cctx::Transaction,
        cell_entries: &AHashMap<CellEntryId, CellEntryReference>,
    ) -> Result<Self> {
        let inputs = transaction
            .inputs
            .iter()
            .map(|input| {
                let id = TransactionOutpointInner::new(
                    TransactionId::from_slice(&input.previous_outpoint.tx_hash),
                    input.previous_outpoint.index,
                );
                let cell_entry = cell_entries.get(&id).ok_or(Error::MissingCellEntry)?;
                let cell_entry = cctx::CellEntry::from(cell_entry);
                let input = SerializableTransactionInput::new(input, &cell_entry);
                Ok(input)
            })
            .collect::<Result<Vec<SerializableTransactionInput>>>()?;

        let outputs = transaction.outputs.iter().map(Into::into).collect::<Vec<SerializableTransactionOutput>>();

        Ok(Self {
            id: transaction.id(),
            version: transaction.version,
            inputs,
            outputs,
            lock_time: transaction.lock_time,
            subnetwork_id: transaction.subnetwork_id.clone(),
            gas: transaction.gas,
            mass: transaction.mass(),
            payload: transaction.payload.clone(),
        })
    }
}

impl TryFrom<SerializableTransaction> for cctx::SignableTransaction {
    type Error = Error;
    fn try_from(serializable: SerializableTransaction) -> Result<Self> {
        let mut entries = vec![];
        let mut inputs = vec![];
        for input in serializable.inputs {
            entries.push(input.cell_entry.as_ref().try_into()?);
            inputs.push(input.try_into()?);
        }

        let outputs = serializable.outputs.into_iter().map(TryInto::try_into).collect::<Result<Vec<_>>>()?;

        #[allow(deprecated)]
        let tx = cctx::Transaction::new(
            serializable.version,
            inputs,
            outputs,
            serializable.lock_time,
            serializable.subnetwork_id,
            serializable.gas,
            serializable.payload,
        )
        .with_mass(serializable.mass);

        Ok(Self::with_entries(cctx::cell_tx_from_legacy_transaction(&tx), entries))
    }
}

impl TryFrom<SerializableTransaction> for Transaction {
    type Error = Error;
    fn try_from(tx: SerializableTransaction) -> Result<Self> {
        let id = tx.id;
        let inputs: Vec<TransactionInput> = tx.inputs.iter().map(TryInto::try_into).collect::<Result<Vec<_>>>()?;
        let outputs: Vec<TransactionOutput> = tx.outputs.iter().map(TryInto::try_into).collect::<Result<Vec<_>>>()?;

        Transaction::new(Some(id), tx.version, inputs, outputs, tx.lock_time, tx.subnetwork_id, tx.gas, tx.payload, tx.mass)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smallvec::smallvec;
    use spora_consensus_core::{
        cell_metadata::CellMetadata,
        subnets,
        tx::{ScriptPublicKey, TransactionInput, TransactionOutpoint},
    };
    use spora_hashes::Hash;

    #[test]
    fn metadata_only_signable_transaction_returns_explicit_error() {
        let tx = cctx::Transaction::new(
            0,
            vec![TransactionInput::new(TransactionOutpoint::new(Hash::from_bytes([1; 32]), 0), vec![], 0, 0)],
            vec![cctx::TransactionOutput::new(100, ScriptPublicKey::new(0, smallvec![0x51]))],
            0,
            subnets::SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        let signable = cctx::SignableTransaction::with_resolved_metadata(
            tx,
            vec![CellMetadata {
                out_point: TransactionOutpoint::new(Hash::from_bytes([1; 32]), 0),
                capacity: 100,
                data_bytes: 0,
                lock_hash: [2; 32],
                type_hash: None,
                data_hash: [0; 32],
                block_daa_score: 0,
                is_cellbase: false,
                block_hash: Hash::default(),
                lock_code_hash: None,
                type_code_hash: None,
                lock_script: None,
                type_script: None,
                data: None,
            }],
        );

        let error = SerializableTransaction::from_signable_transaction(&signable).unwrap_err();
        assert!(matches!(error, Error::MissingLegacyCellEntry(0)));
    }
}
