//!
//! This module implements transaction-related primitives for JSON serialization
//! where all large integer values (`u64`) are serialized to and from JSON as strings.
//!

use crate::imports::*;
use crate::input::TransactionInputInner;
use crate::result::Result;
use crate::{CellEntry, CellEntryReference, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput};
use cctx::VerifiableTransaction;
use spora_addresses::Address;
use spora_consensus_core::mass::project_verifiable_transaction_mass;
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
    pub amount: String,
    #[serde(default)]
    pub capacity: Option<String>,
    #[serde(default)]
    pub data_bytes: Option<String>,
    #[serde(default)]
    pub lock_hash: Option<TransactionId>,
    #[serde(default)]
    pub type_hash: Option<TransactionId>,
    #[serde(default)]
    pub data_hash: Option<TransactionId>,
    pub block_daa_score: String,
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
            amount: cell_entry.amount.to_string(),
            capacity: cell_entry.capacity.map(|value| value.to_string()),
            data_bytes: cell_entry.data_bytes.map(|value| value.to_string()),
            lock_hash: cell_entry.lock_hash,
            type_hash: cell_entry.type_hash,
            data_hash: cell_entry.data_hash,
            block_daa_score: cell_entry.block_daa_score.to_string(),
            is_coinbase: cell_entry.is_coinbase,
        }
    }
}

impl From<&cctx::CellEntry> for SerializableCellEntry {
    fn from(cell_entry: &cctx::CellEntry) -> Self {
        let metadata = cell_entry.embedded_cell_metadata().expect("serializable client CellEntry requires canonical Cell metadata");
        Self {
            address: None,
            amount: cell_entry.amount().to_string(),
            capacity: Some(cell_entry.capacity().to_string()),
            data_bytes: Some(metadata.data_bytes.to_string()),
            lock_hash: Some(metadata.lock_hash.into()),
            type_hash: metadata.type_hash.map(Into::into),
            data_hash: Some(metadata.data_hash.into()),
            block_daa_score: cell_entry.block_daa_score.to_string(),
            is_coinbase: cell_entry.is_cellbase,
        }
    }
}

impl TryFrom<&SerializableCellEntry> for cctx::CellEntry {
    type Error = crate::error::Error;
    fn try_from(cell_entry: &SerializableCellEntry) -> Result<Self> {
        let lock_hash = cell_entry.lock_hash.ok_or_else(|| Error::Custom("SerializableCellEntry.lockHash is required".to_string()))?;
        let data_hash = cell_entry.data_hash.ok_or_else(|| Error::Custom("SerializableCellEntry.dataHash is required".to_string()))?;
        Ok(Self::from_cell_metadata(
            cell_entry.capacity.as_deref().unwrap_or(&cell_entry.amount).parse()?,
            cell_entry.data_bytes.as_deref().unwrap_or("0").parse()?,
            lock_hash.as_bytes(),
            cell_entry.type_hash.map(|hash| hash.as_bytes()),
            data_hash.as_bytes(),
            cell_entry.block_daa_score.parse()?,
            cell_entry.is_coinbase,
        ))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SerializableTransactionInput {
    pub transaction_id: TransactionId,
    pub index: SignedTransactionIndexType,
    pub since: String,
    #[serde(with = "hex::serde")]
    pub witness: Vec<u8>,
    pub cell_entry: SerializableCellEntry,
}

impl SerializableTransactionInput {
    /// Create from a Cell-model CellRef input with its witness data.
    pub fn from_cell_ref(input: &cctx::CellRef, witness: &[u8], cell_entry: &cctx::CellEntry) -> Self {
        let cell_entry = SerializableCellEntry::from(cell_entry);
        Self {
            transaction_id: TransactionId::from_slice(&input.out_point.tx_hash),
            index: input.out_point.index,
            witness: witness.to_vec(),
            since: input.since.to_string(),
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
            amount: input.cell_entry.amount.parse()?,
            capacity: input.cell_entry.capacity.as_ref().map(|value| value.parse()).transpose()?,
            data_bytes: input.cell_entry.data_bytes.as_ref().map(|value| value.parse()).transpose()?,
            lock_hash: input.cell_entry.lock_hash,
            type_hash: input.cell_entry.type_hash,
            data_hash: input.cell_entry.data_hash,
            block_daa_score: input.cell_entry.block_daa_score.parse()?,
            is_coinbase: input.cell_entry.is_coinbase,
        };

        Ok(Self { cell: Arc::new(cell_entry) })
    }
}

impl TryFrom<&SerializableTransactionInput> for TransactionInput {
    type Error = Error;
    fn try_from(serializable_input: &SerializableTransactionInput) -> Result<Self> {
        let cell_entry = CellEntryReference::try_from(serializable_input)?;

        let previous_outpoint = TransactionOutpoint::new(serializable_input.transaction_id, serializable_input.index);
        let inner = TransactionInputInner {
            previous_outpoint,
            witness: (!serializable_input.witness.is_empty()).then_some(serializable_input.witness.clone()),
            since: serializable_input.since.parse()?,
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
            witness: inner.witness.clone().unwrap_or_default(),
            since: inner.since.to_string(),
            cell_entry,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SerializableTransactionOutput {
    pub capacity: String,
    pub lock_script: cctx::ScriptRef,
    #[serde(default)]
    pub type_script: Option<cctx::ScriptRef>,
    #[serde(with = "spora_utils::serde_bytes_optional")]
    #[serde(default)]
    pub output_data: Option<Vec<u8>>,
}

impl SerializableTransactionOutput {
    fn from_cell_output(output: &cctx::CellOut, output_data: Option<&[u8]>) -> Self {
        Self {
            capacity: output.capacity.to_string(),
            lock_script: output.lock.clone(),
            type_script: output.type_.clone(),
            output_data: output_data.filter(|data| !data.is_empty()).map(|data| data.to_vec()),
        }
    }
}

impl TryFrom<&SerializableTransactionOutput> for TransactionOutput {
    type Error = Error;
    fn try_from(output: &SerializableTransactionOutput) -> Result<Self> {
        Ok(TransactionOutput::new_with_inner(crate::output::TransactionOutputInner {
            capacity: output.capacity.parse()?,
            lock_script: output.lock_script.clone(),
            type_script: output.type_script.clone(),
            output_data: output.output_data.clone(),
        }))
    }
}

impl TryFrom<&TransactionOutput> for SerializableTransactionOutput {
    type Error = Error;
    fn try_from(output: &TransactionOutput) -> Result<Self> {
        let inner = output.inner();
        Ok(Self {
            capacity: inner.capacity.to_string(),
            lock_script: inner.lock_script.clone(),
            type_script: inner.type_script.clone(),
            output_data: inner.output_data.clone(),
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SerializableTransaction {
    pub id: TransactionId,
    pub version: u16,
    pub inputs: Vec<SerializableTransactionInput>,
    pub outputs: Vec<SerializableTransactionOutput>,
    #[serde(default)]
    pub mass: String,
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
            let cell_entry = verifiable_tx.cell_entry(index).ok_or(Error::MissingSerializableCellEntry(index))?;
            let witness = transaction.witnesses.get(index).map(|w| w.as_slice()).unwrap_or_default();
            let input = SerializableTransactionInput::from_cell_ref(input, witness, cell_entry);
            inputs.push(input);
        }

        Ok(Self {
            id: Hash::from_bytes(transaction.id()),
            inputs,
            version: transaction.version(),
            outputs: transaction
                .outputs
                .iter()
                .enumerate()
                .map(|(index, output)| {
                    SerializableTransactionOutput::from_cell_output(output, transaction.outputs_data.get(index).map(Vec::as_slice))
                })
                .collect(),
            mass: project_verifiable_transaction_mass(&verifiable_tx, None).selection_mass.to_string(),
            payload: transaction.payload().map(|p| p.to_vec()).unwrap_or_default(),
        })
    }

    pub fn from_client_transaction(transaction: &Transaction) -> Result<Self> {
        Self::from_signable_transaction(&transaction.signable_transaction()?)
    }
}

impl TryFrom<SerializableTransaction> for cctx::SignableTransaction {
    type Error = Error;
    fn try_from(signable: SerializableTransaction) -> Result<Self> {
        let transaction = Transaction::try_from(signable)?;
        transaction.signable_transaction()
    }
}

impl TryFrom<SerializableTransaction> for crate::Transaction {
    type Error = Error;
    fn try_from(tx: SerializableTransaction) -> Result<Self> {
        let signable_transaction = cctx::SignableTransaction::try_from(tx)?;
        Ok(Transaction::from_signable_transaction(&signable_transaction))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spora_consensus_core::{
        cell_metadata::CellMetadata,
        mass::project_verifiable_transaction_mass,
        tx::{CellOut, CellRef, CellTx, ScriptRef, TransactionOutpoint},
    };
    use spora_hashes::Hash;

    #[test]
    fn metadata_only_signable_transaction_returns_explicit_error() {
        let input = CellRef::new(
            TransactionOutpoint::new(Hash::from_bytes([1; 32]).as_bytes(), 0),
            0, // since
        );
        let output = CellOut { capacity: 100, lock: ScriptRef::new([0x51u8; 32], 0, vec![]), type_: None };
        let tx = CellTx::new(
            vec![input],
            vec![], // cell_deps
            vec![output],
            vec![vec![]], // outputs_data
            vec![vec![]], // witnesses
        )
        .unwrap();

        let signable = cctx::SignableTransaction::with_resolved_metadata(
            tx,
            vec![CellMetadata {
                out_point: TransactionOutpoint::new(Hash::from_bytes([1; 32]).as_bytes(), 0),
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
        assert!(matches!(error, Error::MissingSerializableCellEntry(0)));
    }

    #[test]
    fn serializable_transaction_uses_projected_selection_mass() {
        let input = CellRef::new(TransactionOutpoint::new(Hash::from_bytes([4; 32]).as_bytes(), 0), 11);
        let output = CellOut { capacity: 800, lock: ScriptRef::new([0x61u8; 32], 0, vec![3, 4]), type_: None };
        let tx = CellTx::new(vec![input], vec![], vec![output], vec![vec![7, 7]], vec![vec![0xcd]]).unwrap();
        let entry = cctx::CellEntry::from_cell_metadata(1_200, 0, [0x77; 32], None, [0; 32], 0, false);
        let signable = cctx::SignableTransaction::with_entries(tx, vec![entry]);

        let serialized = SerializableTransaction::from_signable_transaction(&signable).expect("serializable transaction");
        let expected_mass = project_verifiable_transaction_mass(&signable.as_verifiable(), None).selection_mass;

        assert_eq!(serialized.mass, expected_mass.to_string());
    }
}
