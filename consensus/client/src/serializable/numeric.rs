//!
//! This module implements transaction-related primitives for JSON serialization
//! where all large integer values (`u64`) are serialized to JSON using `serde` and
//! can exceed the largest integer value representable by the JavaScript `number` type.
//! (i.e. transactions serialized using this module can not be deserialized in JavaScript
//! but may be deserialized in other JSON-capable environments that support large integers)
//!

use crate::error::Error;
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

/// Bridge from CellOut to serializable output format.
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
    #[serde(default)]
    pub mass: u64,
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
            mass: project_verifiable_transaction_mass(&verifiable_tx, None).selection_mass,
            payload: transaction.payload().map(|p| p.to_vec()).unwrap_or_default(),
            id: Hash::from_bytes(transaction.id()),
        })
    }

    pub fn from_client_transaction(transaction: &Transaction) -> Result<Self> {
        Self::from_signable_transaction(&transaction.signable_transaction()?)
    }
}

impl TryFrom<SerializableTransaction> for cctx::SignableTransaction {
    type Error = Error;
    fn try_from(serializable: SerializableTransaction) -> Result<Self> {
        let transaction = Transaction::try_from(serializable)?;
        transaction.signable_transaction()
    }
}

impl TryFrom<SerializableTransaction> for Transaction {
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
        tx::{CellOut, CellRef, CellTx, ScriptPublicKey, ScriptRef, TransactionOutpoint},
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
        assert!(matches!(error, Error::MissingLegacyCellEntry(0)));
    }

    #[test]
    fn serializable_transaction_uses_projected_selection_mass() {
        let input = CellRef::new(TransactionOutpoint::new(Hash::from_bytes([3; 32]).as_bytes(), 0), 7);
        let output = CellOut { capacity: 600, lock: ScriptRef::new([0x41u8; 32], 0, vec![1, 2]), type_: None };
        let tx = CellTx::new(vec![input], vec![], vec![output], vec![vec![9, 9]], vec![vec![0xab]]).unwrap();
        let entry = cctx::CellEntry::from_cell_metadata(1_000, 0, [0x55; 32], None, [0; 32], 0, false);
        let signable = cctx::SignableTransaction::with_entries(tx, vec![entry]);

        let serialized = SerializableTransaction::from_signable_transaction(&signable).expect("serializable transaction");
        let expected_mass = project_verifiable_transaction_mass(&signable.as_verifiable(), None).selection_mass;

        assert_eq!(serialized.mass, expected_mass);
    }
}
