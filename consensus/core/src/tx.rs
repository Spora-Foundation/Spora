//!
//! # Transaction
//!
//! This module implements consensus [`Transaction`] structure and related types.
//!

#![allow(non_snake_case)]

mod script_public_key;

use borsh::{BorshDeserialize, BorshSerialize};
pub use script_public_key::{
    scriptvec, ScriptPublicKey, ScriptPublicKeyT, ScriptPublicKeyVersion, ScriptPublicKeys, ScriptVec, SCRIPT_VECTOR_SIZE,
};
use serde::{Deserialize, Serialize};

// Re-export CellTx from spora-exec (Cell model)
use crate::cell_metadata::{
    cell_metadata_placeholder_script_public_key_with_metadata, parse_cell_metadata_placeholder_script_public_key, CellMetadata,
};
use crate::mass::{ContextualMasses, NonContextualMasses};
use crate::subnets::{self, SubnetworkId};
use crate::hashing;
pub use spora_exec::celltx::{CellDep, CellOut, CellRef, CellTx, DepType, OutPoint, ScriptRef};
use spora_utils::hex::ToHex;
use spora_utils::mem_size::MemSizeEstimator;
use spora_utils::{serde_bytes, serde_bytes_fixed_ref};
use std::collections::HashSet;
use std::mem::{size_of, size_of_val};
use std::str;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering::SeqCst;

/// COINBASE_TRANSACTION_INDEX is the index of the coinbase transaction in every block
pub const COINBASE_TRANSACTION_INDEX: usize = 0;
/// A 32-byte Spora transaction identifier.
pub type TransactionId = spora_hashes::Hash;

/// CellEntry is now a type alias for CellMeta.
///
/// All code should migrate to using CellMeta directly.
/// During the transition, compat methods on CellMeta preserve the old API surface
/// (`capacity()`, `amount()`, `embedded_cell_metadata()`, `from_cell_metadata()`).
pub type CellEntry = crate::cell_diff::CellMeta;

/// Bridge a Cell-backed entry into a legacy placeholder ScriptPublicKey.
///
/// This preserves lock/type/data metadata inside the placeholder payload so
/// remaining legacy surfaces can continue operating without silently throwing
/// away Cell semantics.
pub fn cell_entry_legacy_script_public_key(cell_entry: &CellEntry) -> ScriptPublicKey {
    cell_metadata_placeholder_script_public_key_with_metadata(
        cell_entry.lock_hash,
        cell_entry.type_hash,
        cell_entry.data_hash,
        cell_entry.data_bytes,
    )
}

/// Bridge a Cell-backed entry into a legacy TransactionOutput.
pub fn legacy_transaction_output_from_cell_entry(cell_entry: &CellEntry) -> TransactionOutput {
    TransactionOutput { value: cell_entry.amount(), script_public_key: cell_entry_legacy_script_public_key(cell_entry) }
}

/// Deterministically derive the synthetic Cell lock hash used by legacy script bridges.
pub fn compute_lock_hash_for_script(script_public_key: &ScriptPublicKey) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"spora-cell/lock");
    hasher.update(&script_public_key.version().to_le_bytes());
    hasher.update(script_public_key.script());
    *hasher.finalize().as_bytes()
}

/// Bridge: create a CellMeta from a legacy TransactionOutput.
///
/// Parses placeholder cell metadata from the script if present; otherwise
/// computes a deterministic `lock_hash` from the raw script bytes.
pub fn cell_meta_from_legacy_output(
    value: u64,
    script_public_key: &ScriptPublicKey,
    block_daa_score: u64,
    is_cellbase: bool,
) -> CellEntry {
    if let Some(m) = parse_cell_metadata_placeholder_script_public_key(script_public_key) {
        CellEntry::from_cell_metadata(value, m.data_bytes, m.lock_hash, m.type_hash, m.data_hash, block_daa_score, is_cellbase)
    } else {
        CellEntry {
            out_point: TransactionOutpoint::default(),
            capacity: value,
            data_bytes: 0,
            lock_hash: compute_lock_hash_for_script(script_public_key),
            type_hash: None,
            data_hash: [0; 32],
            block_daa_score,
            is_cellbase,
        }
    }
}

/// Bridge: create a canonical `CellOut` from a legacy output shape.
///
/// If the script embeds placeholder cell metadata, preserve the lock/type hashes
/// carried by that placeholder instead of recomputing a lossy lock hash from the
/// serialized script bytes.
pub fn cell_out_from_legacy_script_public_key(value: u64, script_public_key: &ScriptPublicKey) -> CellOut {
    if let Some(metadata) = parse_cell_metadata_placeholder_script_public_key(script_public_key) {
        CellOut {
            lock: ScriptRef::new(metadata.lock_hash, 0, vec![]),
            type_: metadata.type_hash.map(|type_hash| ScriptRef::new(type_hash, 0, vec![])),
            capacity: value,
        }
    } else {
        CellOut {
            lock: ScriptRef::new(compute_lock_hash_for_script(script_public_key), 0, script_public_key.script().to_vec()),
            type_: None,
            capacity: value,
        }
    }
}

/// Bridge a legacy compatibility `Transaction` into a canonical `CellTx`.
pub fn cell_tx_from_legacy_transaction(tx: &Transaction) -> CellTx {
    let inputs = tx
        .inputs
        .iter()
        .map(|input| CellRef::new(input.previous_outpoint, legacy_sequence_to_cell_since(input.sequence)))
        .collect::<Vec<_>>();

    let witnesses = tx.inputs.iter().map(|input| input.signature_script.clone()).collect::<Vec<_>>();

    let outputs = tx
        .outputs
        .iter()
        .map(|output| cell_out_from_legacy_script_public_key(output.value, &output.script_public_key))
        .collect::<Vec<_>>();

    let mut outputs_data = vec![vec![]; outputs.len()];
    if tx.is_coinbase() && !tx.payload.is_empty() && !outputs_data.is_empty() {
        let first = outputs_data.first_mut().expect("checked outputs_data is not empty");
        *first = tx.payload.clone();
    }

    let coinbase_witnesses = if tx.is_coinbase() && outputs.is_empty() && !tx.payload.is_empty() {
        vec![tx.payload.clone()]
    } else {
        witnesses
    };

    CellTx::new(inputs, vec![], outputs, outputs_data, coinbase_witnesses)
        .expect("legacy compatibility transactions must convert into valid CellTx values")
}

/// Bridge a legacy sequence value into the canonical Cell `since` field.
#[inline]
pub fn legacy_sequence_to_cell_since(sequence: u64) -> u64 {
    if sequence == u64::MAX { 0 } else { sequence }
}

pub type TransactionIndexType = u32;

/// Represents a Spora transaction outpoint (type alias for exec OutPoint).
///
/// Migration: was a struct with `transaction_id: TransactionId` + `index: u32`,
/// now aliases `spora_exec::celltx::OutPoint` which uses `tx_hash: [u8; 32]` + `index: u32`.
pub type TransactionOutpoint = spora_exec::celltx::OutPoint;

/// Extension trait bridging OutPoint's `tx_hash: [u8; 32]` to the legacy `TransactionId` type.
pub trait OutPointCompat {
    /// Get the transaction ID as a `TransactionId` (Hash wrapper).
    fn transaction_id(&self) -> TransactionId;
}

impl OutPointCompat for TransactionOutpoint {
    #[inline]
    fn transaction_id(&self) -> TransactionId {
        TransactionId::from_bytes(self.tx_hash)
    }
}

/// Convenience constructor that accepts `TransactionId` instead of raw `[u8; 32]`.
pub fn outpoint_from_id(transaction_id: TransactionId, index: u32) -> TransactionOutpoint {
    TransactionOutpoint::new(transaction_id.as_bytes(), index)
}

/// Legacy transaction input compatibility shape.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionInput {
    pub previous_outpoint: TransactionOutpoint,
    #[serde(with = "serde_bytes")]
    pub signature_script: Vec<u8>,
    pub sequence: u64,
    pub sig_op_count: u8,
}

impl TransactionInput {
    pub fn new(previous_outpoint: TransactionOutpoint, signature_script: Vec<u8>, sequence: u64, sig_op_count: u8) -> Self {
        Self { previous_outpoint, signature_script, sequence, sig_op_count }
    }
}

impl std::fmt::Debug for TransactionInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransactionInput")
            .field("previous_outpoint", &self.previous_outpoint)
            .field("signature_script", &self.signature_script.to_hex())
            .field("sequence", &self.sequence)
            .field("sig_op_count", &self.sig_op_count)
            .finish()
    }
}

/// Legacy transaction output compatibility shape.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionOutput {
    pub value: u64,
    pub script_public_key: ScriptPublicKey,
}

impl TransactionOutput {
    pub fn new(value: u64, script_public_key: ScriptPublicKey) -> Self {
        Self { value, script_public_key }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TransactionMass(AtomicU64);

impl Eq for TransactionMass {}

impl PartialEq for TransactionMass {
    fn eq(&self, other: &Self) -> bool {
        self.0.load(SeqCst) == other.0.load(SeqCst)
    }
}

impl Clone for TransactionMass {
    fn clone(&self) -> Self {
        Self(AtomicU64::new(self.0.load(SeqCst)))
    }
}

impl BorshDeserialize for TransactionMass {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let mass: u64 = borsh::BorshDeserialize::deserialize_reader(reader)?;
        Ok(Self(AtomicU64::new(mass)))
    }
}

impl BorshSerialize for TransactionMass {
    fn serialize<W: std::io::prelude::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        borsh::BorshSerialize::serialize(&self.0.load(SeqCst), writer)
    }
}

/// Legacy transaction compatibility view.
///
/// New code should prefer [`CellTx`], but a reduced `Transaction` surface is
/// still required by hashing, merkle, RPC, and test-only mining shims.
#[allow(deprecated)]
#[deprecated(note = "Use CellTx directly; Transaction is a legacy compatibility type")]
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transaction {
    pub version: u16,
    pub inputs: Vec<TransactionInput>,
    pub outputs: Vec<TransactionOutput>,
    pub lock_time: u64,
    pub subnetwork_id: SubnetworkId,
    pub gas: u64,
    #[serde(with = "serde_bytes")]
    pub payload: Vec<u8>,
    #[serde(default)]
    mass: TransactionMass,
    #[serde(with = "serde_bytes_fixed_ref")]
    id: TransactionId,
}

#[allow(deprecated)]
impl Transaction {
    pub fn new(
        version: u16,
        inputs: Vec<TransactionInput>,
        outputs: Vec<TransactionOutput>,
        lock_time: u64,
        subnetwork_id: SubnetworkId,
        gas: u64,
        payload: Vec<u8>,
    ) -> Self {
        let mut tx = Self::new_non_finalized(version, inputs, outputs, lock_time, subnetwork_id, gas, payload);
        tx.finalize();
        tx
    }

    pub fn new_non_finalized(
        version: u16,
        inputs: Vec<TransactionInput>,
        outputs: Vec<TransactionOutput>,
        lock_time: u64,
        subnetwork_id: SubnetworkId,
        gas: u64,
        payload: Vec<u8>,
    ) -> Self {
        Self { version, inputs, outputs, lock_time, subnetwork_id, gas, payload, mass: Default::default(), id: Default::default() }
    }

    pub fn is_coinbase(&self) -> bool {
        self.subnetwork_id == subnets::SUBNETWORK_ID_COINBASE
    }

    pub fn finalize(&mut self) {
        self.id = hashing::tx::id(self);
    }

    pub fn id(&self) -> TransactionId {
        self.id
    }

    pub fn set_mass(&self, mass: u64) {
        self.mass.0.store(mass, SeqCst)
    }

    pub fn mass(&self) -> u64 {
        self.mass.0.load(SeqCst)
    }

    pub fn with_mass(self, mass: u64) -> Self {
        self.set_mass(mass);
        self
    }
}

#[allow(deprecated)]
impl MemSizeEstimator for Transaction {
    fn estimate_mem_bytes(&self) -> usize {
        size_of::<Self>()
            + self.payload.len()
            + self
                .inputs
                .iter()
                .map(|input| input.signature_script.len() + size_of::<TransactionInput>())
                .chain(self.outputs.iter().map(|output| {
                    output.script_public_key.script().len().saturating_sub(SCRIPT_VECTOR_SIZE) + size_of::<TransactionOutput>()
                }))
                .sum::<usize>()
    }
}













/// Represents any kind of transaction which has its inputs resolved either as
/// legacy Cell entries or as canonical Cell metadata and can be verified/signed.
///
/// Now based on [`CellTx`] rather than the legacy `Transaction` struct.
pub trait VerifiableTransaction {
    fn tx(&self) -> &CellTx;

    fn inputs(&self) -> &[CellRef] {
        &self.tx().inputs
    }

    fn outputs(&self) -> &[CellOut] {
        &self.tx().outputs
    }

    fn witnesses(&self) -> &[Vec<u8>] {
        &self.tx().witnesses
    }

    fn is_coinbase(&self) -> bool {
        self.tx().is_coinbase()
    }

    fn id(&self) -> TransactionId {
        TransactionId::from_bytes(self.tx().id())
    }

    fn cell_entry(&self, index: usize) -> Option<&CellEntry>;

    fn cell_metadata(&self, index: usize) -> Option<CellMetadata> {
        self.cell_entry(index).map(CellMetadata::from)
    }
}

/// Represents a read-only referenced transaction along with fully resolved input data.
#[derive(Debug)]
pub struct PopulatedTransaction<'a> {
    pub tx: &'a CellTx,
    pub entries: Vec<CellEntry>,
    pub resolved_cell_metadata: Vec<Option<CellMetadata>>,
}

impl<'a> PopulatedTransaction<'a> {
    pub fn new(tx: &'a CellTx, entries: Vec<CellEntry>) -> Self {
        assert_eq!(tx.inputs.len(), entries.len());
        let resolved_cell_metadata = entries.iter().map(|entry| Some(CellMetadata::from(entry))).collect();
        Self { tx, entries, resolved_cell_metadata }
    }
}

impl VerifiableTransaction for PopulatedTransaction<'_> {
    fn tx(&self) -> &CellTx {
        self.tx
    }

    fn cell_entry(&self, index: usize) -> Option<&CellEntry> {
        self.entries.get(index)
    }

    fn cell_metadata(&self, index: usize) -> Option<CellMetadata> {
        self.resolved_cell_metadata.get(index).and_then(Option::clone)
    }
}

/// Represents a validated transaction with fully resolved input data and a calculated fee.
pub struct ValidatedTransaction<'a> {
    pub tx: &'a CellTx,
    pub entries: Vec<CellEntry>,
    pub resolved_cell_metadata: Vec<Option<CellMetadata>>,
    pub calculated_fee: u64,
}

impl<'a> ValidatedTransaction<'a> {
    pub fn new(populated_tx: PopulatedTransaction<'a>, calculated_fee: u64) -> Self {
        Self {
            tx: populated_tx.tx,
            entries: populated_tx.entries,
            resolved_cell_metadata: populated_tx.resolved_cell_metadata,
            calculated_fee,
        }
    }

    pub fn new_coinbase(tx: &'a CellTx) -> Self {
        assert!(tx.is_coinbase());
        Self { tx, entries: Vec::new(), resolved_cell_metadata: Vec::new(), calculated_fee: 0 }
    }
}

impl VerifiableTransaction for ValidatedTransaction<'_> {
    fn tx(&self) -> &CellTx {
        self.tx
    }

    fn cell_entry(&self, index: usize) -> Option<&CellEntry> {
        self.entries.get(index)
    }

    fn cell_metadata(&self, index: usize) -> Option<CellMetadata> {
        self.resolved_cell_metadata.get(index).and_then(Option::clone)
    }
}

/// Canonical Cell transaction together with fully resolved input metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedCellTransaction {
    pub tx: CellTx,
    pub resolved_inputs: Vec<CellMetadata>,
}

impl ResolvedCellTransaction {
    pub fn new(tx: CellTx, resolved_inputs: Vec<CellMetadata>) -> Self {
        assert_eq!(tx.inputs.len(), resolved_inputs.len());
        Self { tx, resolved_inputs }
    }

    pub fn resolved_input(&self, index: usize) -> Option<&CellMetadata> {
        self.resolved_inputs.get(index)
    }

    /// Convert this canonical Cell transaction into the legacy signable view
    /// required by compatibility APIs.
    pub fn into_legacy_signable_transaction(self) -> SignableTransaction {
        MutableTransaction::with_resolved_metadata(self.tx, self.resolved_inputs)
    }

    /// Convert this canonical Cell transaction into the legacy compatibility
    /// transaction view used by older RPC and wallet interfaces.
    pub fn legacy_compat_view(&self) -> Transaction {
        legacy_compat_transaction_from_cell_tx(&self.tx)
    }
}

/// Build the legacy compatibility `Transaction` view for a canonical `CellTx`.
pub fn legacy_compat_transaction_from_cell_tx(cell_tx: &CellTx) -> Transaction {
    let inputs = cell_tx
        .inputs
        .iter()
        .enumerate()
        .map(|(index, input)| {
            let signature_script = cell_tx.witnesses.get(index).cloned().unwrap_or_default();
            TransactionInput::new(
                TransactionOutpoint::new(input.out_point.tx_hash.into(), input.out_point.index),
                signature_script,
                input.since,
                0,
            )
        })
        .collect();

    let outputs = cell_tx
        .outputs
        .iter()
        .enumerate()
        .map(|(index, output)| {
            let output_data = cell_tx.outputs_data.get(index).cloned().unwrap_or_default();
            TransactionOutput::new(
                output.capacity,
                cell_metadata_placeholder_script_public_key_with_metadata(
                    output.lock.hash(),
                    output.type_.as_ref().map(|script| script.hash()),
                    *blake3::hash(&output_data).as_bytes(),
                    output_data.len() as u64,
                ),
            )
        })
        .collect();

    let payload = cell_tx.payload().map(ToOwned::to_owned).unwrap_or_default();
    let subnetwork_id = if cell_tx.is_coinbase() { subnets::SUBNETWORK_ID_COINBASE } else { subnets::SUBNETWORK_ID_NATIVE };
    let transaction = Transaction::new(cell_tx.ver, inputs, outputs, 0, subnetwork_id, 0, payload);
    transaction.set_mass(cell_tx.storage_mass());
    transaction
}



/// Local access trait so owned and shared CellTx containers can back
/// `MutableTransaction` without requiring foreign-trait impls on `CellTx`.
pub trait CellTxContainer {
    fn cell_tx(&self) -> &CellTx;
}

impl CellTxContainer for CellTx {
    fn cell_tx(&self) -> &CellTx {
        self
    }
}

impl CellTxContainer for std::sync::Arc<CellTx> {
    fn cell_tx(&self) -> &CellTx {
        self.as_ref()
    }
}

impl<T: CellTxContainer + ?Sized> CellTxContainer for &T {
    fn cell_tx(&self) -> &CellTx {
        (*self).cell_tx()
    }
}



/// Represents a generic mutable/readonly/pointer transaction type along with
/// partially filled legacy Cell entries and/or resolved canonical Cell metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MutableTransaction<T: CellTxContainer = std::sync::Arc<CellTx>> {
    /// The inner CellTx transaction
    pub tx: T,
    /// Partially filled Cell entry data
    pub entries: Vec<Option<CellEntry>>,
    /// Resolved Cell metadata for each input when available.
    pub resolved_cell_metadata: Vec<Option<CellMetadata>>,
    /// Populated fee
    pub calculated_fee: Option<u64>,
    /// Populated non-contextual masses (does not include the storage mass)
    pub calculated_non_contextual_masses: Option<NonContextualMasses>,
}

impl<T: CellTxContainer> MutableTransaction<T> {
    pub fn new(tx: T) -> Self {
        let num_inputs = tx.cell_tx().inputs.len();
        Self {
            tx,
            entries: vec![None; num_inputs],
            resolved_cell_metadata: vec![None; num_inputs],
            calculated_fee: None,
            calculated_non_contextual_masses: None,
        }
    }

    pub fn id(&self) -> TransactionId {
        TransactionId::from_bytes(self.tx.cell_tx().id())
    }

    pub fn with_entries(tx: T, entries: Vec<CellEntry>) -> Self {
        assert_eq!(tx.cell_tx().inputs.len(), entries.len());
        let resolved_cell_metadata = entries.iter().map(|entry| Some(CellMetadata::from(entry))).collect();
        Self {
            tx,
            entries: entries.into_iter().map(Some).collect(),
            resolved_cell_metadata,
            calculated_fee: None,
            calculated_non_contextual_masses: None,
        }
    }

    pub fn with_resolved_metadata(tx: T, resolved_cell_metadata: Vec<CellMetadata>) -> Self {
        assert_eq!(tx.cell_tx().inputs.len(), resolved_cell_metadata.len());
        Self {
            tx,
            entries: vec![None; resolved_cell_metadata.len()],
            resolved_cell_metadata: resolved_cell_metadata.into_iter().map(Some).collect(),
            calculated_fee: None,
            calculated_non_contextual_masses: None,
        }
    }

    /// Returns the tx wrapped as a [`VerifiableTransaction`]. Note that this function
    /// must be called only once every input is resolved via either a Cell entry
    /// or canonical Cell metadata, otherwise it panics.
    pub fn as_verifiable(&self) -> impl VerifiableTransaction + '_ {
        assert!(self.is_verifiable());
        MutableTransactionVerifiableWrapper { inner: self }
    }

    pub fn is_verifiable(&self) -> bool {
        assert_eq!(self.entries.len(), self.tx.cell_tx().inputs.len());
        self.entries.iter().zip(self.resolved_cell_metadata.iter()).all(|(entry, metadata)| entry.is_some() || metadata.is_some())
    }

    pub fn is_fully_populated(&self) -> bool {
        self.is_verifiable() && self.calculated_fee.is_some() && self.calculated_non_contextual_masses.is_some()
    }

    pub fn missing_outpoints(&self) -> impl Iterator<Item = TransactionOutpoint> + '_ {
        assert_eq!(self.entries.len(), self.tx.cell_tx().inputs.len());
        self.entries.iter().zip(self.resolved_cell_metadata.iter()).enumerate().filter_map(|(i, (entry, metadata))| {
            if entry.is_none() && metadata.is_none() {
                Some(self.tx.cell_tx().inputs[i].out_point)
            } else {
                None
            }
        })
    }

    pub fn clear_entries(&mut self) {
        for entry in self.entries.iter_mut() {
            *entry = None;
        }
        for metadata in self.resolved_cell_metadata.iter_mut() {
            *metadata = None;
        }
    }

    pub fn resolved_cell_metadata(&self, index: usize) -> Option<&CellMetadata> {
        self.resolved_cell_metadata.get(index).and_then(Option::as_ref)
    }

    /// Returns the calculated feerate. The feerate is calculated as the amount of fee this
    /// transactions pays per gram of the aggregated contextual mass (max over compute, transient
    /// and storage masses). The function returns a value when calculated fee and calculated masses
    /// exist, otherwise `None` is returned.
    pub fn calculated_feerate(&self) -> Option<f64> {
        self.calculated_non_contextual_masses
            .map(|non_contextual_masses| ContextualMasses::new(self.tx.cell_tx().storage_mass()).max(non_contextual_masses))
            .and_then(|max_mass| self.calculated_fee.map(|fee| fee as f64 / max_mass as f64))
    }

    /// A function for estimating the amount of memory bytes used by this transaction (dedicated to mempool usage).
    /// We need consistency between estimation calls so only this function should be used for this purpose since
    /// `estimate_mem_bytes` is sensitive to pointer wrappers such as Arc
    pub fn mempool_estimated_bytes(&self) -> usize {
        self.estimate_mem_bytes()
    }

    pub fn has_parent(&self, possible_parent: TransactionId) -> bool {
        let parent_bytes = possible_parent.as_bytes();
        self.tx.cell_tx().inputs.iter().any(|x| x.out_point.tx_hash == parent_bytes)
    }

    pub fn has_parent_in_set(&self, possible_parents: &HashSet<TransactionId>) -> bool {
        self.tx.cell_tx().inputs.iter().any(|x| possible_parents.contains(&TransactionId::from_bytes(x.out_point.tx_hash)))
    }
}

impl<T: CellTxContainer> MemSizeEstimator for MutableTransaction<T> {
    fn estimate_mem_bytes(&self) -> usize {
        size_of::<Self>()
            + self
                .entries
                .iter()
                .zip(self.resolved_cell_metadata.iter())
                .map(|(_op, metadata)| {
                    size_of::<Option<CellEntry>>()
                        + size_of::<Option<CellMetadata>>()
                        + metadata.as_ref().and_then(|meta| meta.data.as_ref().map(Vec::len)).unwrap_or_default()
                })
                .sum::<usize>()
            + size_of_val(self.tx.cell_tx())
    }
}

impl<T: CellTxContainer> AsRef<CellTx> for MutableTransaction<T> {
    fn as_ref(&self) -> &CellTx {
        self.tx.cell_tx()
    }
}

/// Private struct used to wrap a [`MutableTransaction`] as a [`VerifiableTransaction`]
struct MutableTransactionVerifiableWrapper<'a, T: CellTxContainer> {
    inner: &'a MutableTransaction<T>,
}

impl<T: CellTxContainer> VerifiableTransaction for MutableTransactionVerifiableWrapper<'_, T> {
    fn tx(&self) -> &CellTx {
        self.inner.tx.cell_tx()
    }

    fn cell_entry(&self, index: usize) -> Option<&CellEntry> {
        self.inner.entries.get(index).and_then(Option::as_ref)
    }

    fn cell_metadata(&self, index: usize) -> Option<CellMetadata> {
        self.inner.resolved_cell_metadata(index).cloned().or_else(|| self.cell_entry(index).map(CellMetadata::from))
    }
}

/// Specialized impl for `T=Arc<CellTx>`
impl MutableTransaction {
    #[allow(deprecated)]
    pub fn from_tx(tx: Transaction) -> Self {
        Self::from_cell_tx(cell_tx_from_legacy_transaction(&tx))
    }

    pub fn from_cell_tx(tx: CellTx) -> Self {
        Self::new(std::sync::Arc::new(tx))
    }
}

/// Alias for a fully mutable and owned CellTx which can be populated with external data
/// and can also be modified internally and signed etc.
pub type SignableTransaction = MutableTransaction<CellTx>;

#[cfg(test)]
mod tests {
    use super::*;
    use smallvec::smallvec;

    #[allow(deprecated)]
    fn test_transaction() -> Transaction {
        let script_public_key = ScriptPublicKey::new(
            0,
            smallvec![
                0x76, 0xa9, 0x21, 0x03, 0x2f, 0x7e, 0x43, 0x0a, 0xa4, 0xc9, 0xd1, 0x59, 0x43, 0x7e, 0x84, 0xb9, 0x75, 0xdc, 0x76,
                0xd9, 0x00, 0x3b, 0xf0, 0x92, 0x2c, 0xf3, 0xaa, 0x45, 0x28, 0x46, 0x4b, 0xab, 0x78, 0x0d, 0xba, 0x5e
            ],
        );
        Transaction::new(
            1,
            vec![
                TransactionInput {
                    previous_outpoint: outpoint_from_id(
                        TransactionId::from_slice(&[
                            0x16, 0x5e, 0x38, 0xe8, 0xb3, 0x91, 0x45, 0x95, 0xd9, 0xc6, 0x41, 0xf3, 0xb8, 0xee, 0xc2, 0xf3, 0x46,
                            0x11, 0x89, 0x6b, 0x82, 0x1a, 0x68, 0x3b, 0x7a, 0x4e, 0xde, 0xfe, 0x2c, 0x00, 0x00, 0x00,
                        ]),
                        0xfffffffa,
                    ),
                    signature_script: vec![
                        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11,
                        0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
                    ],
                    sequence: 2,
                    sig_op_count: 3,
                },
                TransactionInput {
                    previous_outpoint: outpoint_from_id(
                        TransactionId::from_slice(&[
                            0x4b, 0xb0, 0x75, 0x35, 0xdf, 0xd5, 0x8e, 0x0b, 0x3c, 0xd6, 0x4f, 0xd7, 0x15, 0x52, 0x80, 0x87, 0x2a,
                            0x04, 0x71, 0xbc, 0xf8, 0x30, 0x95, 0x52, 0x6a, 0xce, 0x0e, 0x38, 0xc6, 0x00, 0x00, 0x00,
                        ]),
                        0xfffffffb,
                    ),
                    signature_script: vec![
                        0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f, 0x30, 0x31,
                        0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f,
                    ],
                    sequence: 4,
                    sig_op_count: 5,
                },
            ],
            vec![
                TransactionOutput { value: 6, script_public_key: script_public_key.clone() },
                TransactionOutput { value: 7, script_public_key },
            ],
            8,
            subnets::SUBNETWORK_ID_COINBASE,
            9,
            vec![
                0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12,
                0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25,
                0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38,
                0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b,
                0x4c, 0x4d, 0x4e, 0x4f, 0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x5b, 0x5c, 0x5d, 0x5e,
                0x5f, 0x60, 0x61, 0x62, 0x63,
            ],
        )
    }

    #[test]
    fn test_transaction_bincode() {
        let tx = test_transaction();
        let bts = bincode::serialize(&tx).unwrap();

        // standard, based on https://github.com/AvatoLabs/Spora/commit/7e947a06d2434daf4bc7064d4cd87dc1984b56fe
        let expected_bts = vec![
            1, 0, 2, 0, 0, 0, 0, 0, 0, 0, 22, 94, 56, 232, 179, 145, 69, 149, 217, 198, 65, 243, 184, 238, 194, 243, 70, 17, 137, 107,
            130, 26, 104, 59, 122, 78, 222, 254, 44, 0, 0, 0, 250, 255, 255, 255, 32, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8,
            9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 2, 0, 0, 0, 0, 0, 0, 0, 3, 75,
            176, 117, 53, 223, 213, 142, 11, 60, 214, 79, 215, 21, 82, 128, 135, 42, 4, 113, 188, 248, 48, 149, 82, 106, 206, 14, 56,
            198, 0, 0, 0, 251, 255, 255, 255, 32, 0, 0, 0, 0, 0, 0, 0, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47,
            48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 4, 0, 0, 0, 0, 0, 0, 0, 5, 2, 0, 0, 0, 0, 0, 0, 0, 6, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 36, 0, 0, 0, 0, 0, 0, 0, 118, 169, 33, 3, 47, 126, 67, 10, 164, 201, 209, 89, 67, 126, 132, 185,
            117, 220, 118, 217, 0, 59, 240, 146, 44, 243, 170, 69, 40, 70, 75, 171, 120, 13, 186, 94, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            36, 0, 0, 0, 0, 0, 0, 0, 118, 169, 33, 3, 47, 126, 67, 10, 164, 201, 209, 89, 67, 126, 132, 185, 117, 220, 118, 217, 0,
            59, 240, 146, 44, 243, 170, 69, 40, 70, 75, 171, 120, 13, 186, 94, 8, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 9, 0, 0, 0, 0, 0, 0, 0, 100, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12,
            13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42,
            43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72,
            73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98, 99, 0, 0, 0, 0, 0,
            0, 0, 0, 61, 188, 55, 192, 57, 96, 26, 206, 50, 63, 46, 214, 76, 28, 198, 69, 142, 39, 240, 188, 203, 112, 243, 237, 32,
            9, 181, 135, 129, 178, 212, 47,
        ];
        assert_eq!(expected_bts, bts);
        assert_eq!(tx, bincode::deserialize(&bts).unwrap());
    }

    #[test]
    fn test_transaction_json() {
        let tx = test_transaction();
        let str = serde_json::to_string_pretty(&tx).unwrap();
        let expected_str = r#"{
  "version": 1,
  "inputs": [
    {
      "previousOutpoint": {
        "transactionId": "165e38e8b3914595d9c641f3b8eec2f34611896b821a683b7a4edefe2c000000",
        "index": 4294967290
      },
      "signatureScript": "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
      "sequence": 2,
      "sigOpCount": 3
    },
    {
      "previousOutpoint": {
        "transactionId": "4bb07535dfd58e0b3cd64fd7155280872a0471bcf83095526ace0e38c6000000",
        "index": 4294967291
      },
      "signatureScript": "202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f",
      "sequence": 4,
      "sigOpCount": 5
    }
  ],
  "outputs": [
    {
      "value": 6,
      "scriptPublicKey": "000076a921032f7e430aa4c9d159437e84b975dc76d9003bf0922cf3aa4528464bab780dba5e"
    },
    {
      "value": 7,
      "scriptPublicKey": "000076a921032f7e430aa4c9d159437e84b975dc76d9003bf0922cf3aa4528464bab780dba5e"
    }
  ],
  "lockTime": 8,
  "subnetworkId": "0100000000000000000000000000000000000000",
  "gas": 9,
  "payload": "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f60616263",
  "mass": 0,
  "id": "3dbc37c039601ace323f2ed64c1cc6458e27f0bccb70f3ed2009b58781b2d42f"
}"#;
        assert_eq!(expected_str, str);
        assert_eq!(tx, serde_json::from_str(&str).unwrap());
    }

    #[test]
    fn test_spk_serde_json_helper() {
        let vec = (0..SCRIPT_VECTOR_SIZE as u8).collect::<Vec<_>>();
        let spk = ScriptPublicKey::from_vec(0xc0de, vec.clone());
        let hex: String = serde_json::to_string(&spk).unwrap();
        assert_eq!("\"c0de000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20212223\"", hex);
        let spk = serde_json::from_str::<ScriptPublicKey>(&hex).unwrap();
        assert_eq!(spk.version, 0xc0de);
        assert_eq!(spk.script.as_slice(), vec.as_slice());
        let result = "00".parse::<ScriptPublicKey>();
        assert!(matches!(result, Err(faster_hex::Error::InvalidLength(2))));
        let result = "0000".parse::<ScriptPublicKey>();
        let _empty = ScriptPublicKey { version: 0, script: ScriptVec::new() };
        assert!(matches!(result, Ok(_empty)));
    }

    #[test]
    fn test_spk_borsh() {
        // Tests for ScriptPublicKey Borsh ser/deser since we manually implemented them
        let spk = ScriptPublicKey::from_vec(12, vec![32; 20]);
        let bin = borsh::to_vec(&spk).unwrap();
        let spk2: ScriptPublicKey = BorshDeserialize::try_from_slice(&bin).unwrap();
        assert_eq!(spk, spk2);

        let spk = ScriptPublicKey::from_vec(55455, vec![11; 200]);
        let bin = borsh::to_vec(&spk).unwrap();
        let spk2: ScriptPublicKey = BorshDeserialize::try_from_slice(&bin).unwrap();
        assert_eq!(spk, spk2);
    }

    // use wasm_bindgen_test::wasm_bindgen_test;
    // #[wasm_bindgen_test]
    // pub fn test_wasm_serde_spk_constructor() {
    //     let str = "spora:qpauqsvk7yf9unexwmxsnmg547mhyga37csh0kj53q6xxgl24ydxjsgzthw5j";
    //     let a = Address::constructor(str);
    //     let value = to_value(&a).unwrap();
    //
    //     assert_eq!(JsValue::from_str("string"), value.js_typeof());
    //     assert_eq!(value, JsValue::from_str(str));
    //     assert_eq!(a, from_value(value).unwrap());
    // }
    //
    // #[wasm_bindgen_test]
    // pub fn test_wasm_js_serde_spk_object() {
    //     let expected = Address::constructor("spora:qpauqsvk7yf9unexwmxsnmg547mhyga37csh0kj53q6xxgl24ydxjsgzthw5j");
    //
    //     use web_sys::console;
    //     console::log_4(&"address: ".into(), &expected.version().into(), &expected.prefix().into(), &expected.payload().into());
    //
    //     let obj = Object::new();
    //     obj.set("version", &JsValue::from_str("PubKey")).unwrap();
    //     obj.set("prefix", &JsValue::from_str("spora")).unwrap();
    //     obj.set("payload", &JsValue::from_str("qpauqsvk7yf9unexwmxsnmg547mhyga37csh0kj53q6xxgl24ydxjsgzthw5j")).unwrap();
    //
    //     assert_eq!(JsValue::from_str("object"), obj.js_typeof());
    //
    //     let obj_js = obj.into_js_result().unwrap();
    //     let actual = from_value(obj_js).unwrap();
    //     assert_eq!(expected, actual);
    // }
    //
    // #[wasm_bindgen_test]
    // pub fn test_wasm_serde_spk_object() {
    //     use wasm_bindgen::convert::IntoWasmAbi;
    //
    //     let expected = Address::constructor("spora:qpauqsvk7yf9unexwmxsnmg547mhyga37csh0kj53q6xxgl24ydxjsgzthw5j");
    //     let wasm_js_value: JsValue = expected.clone().into_abi().into();
    //
    //     // use web_sys::console;
    //     // console::log_4(&"address: ".into(), &expected.version().into(), &expected.prefix().into(), &expected.payload().into());
    //
    //     let actual = from_value(wasm_js_value).unwrap();
    //     assert_eq!(expected, actual);
    // }
}
