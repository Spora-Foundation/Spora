//!
//! # Transaction
//!
//! This module implements consensus transaction structures and related types.
//!

#![allow(non_snake_case)]

mod script_cache;
mod standard_script;

pub use script_cache::{ScriptCacheCounters, ScriptCacheCountersSnapshot};
pub use standard_script::{
    address_to_builtin_standard_lock, address_to_full_script_lock, address_to_lock_script, builtin_account_descriptor_code_hash,
    builtin_ecdsa_blake3_160_code_hash, builtin_schnorr_blake3_160_code_hash, classify_script, decode_full_script_payload,
    encode_full_script_payload, extract_address_from_script, is_cell_lock_unspendable, multisig_witness_template,
    multisig_witness_template_ecdsa, pay_to_address_lock_script, push_data_script, MultisigWitnessTemplateError, ScriptClass,
    StandardScriptError, HASH_TYPE_TYPE,
};

use crate::cell_diff::CellMeta;
use crate::cell_metadata::CellMetadata;
use crate::mass::{cell_tx_estimated_serialized_size, ContextualMasses, NonContextualMasses};
pub use spora_exec::celltx::{CellDep, CellInput, CellOutput, CellTx, DepType, OutPoint, Script};
use spora_exec::vm::VmLimits;
use spora_utils::mem_size::MemSizeEstimator;
use std::mem::size_of_val;

/// COINBASE_TRANSACTION_INDEX is the index of the coinbase transaction in every block
pub const COINBASE_TRANSACTION_INDEX: usize = 0;
/// A 32-byte Spora transaction identifier.
pub type TransactionId = spora_hashes::Hash;

pub type TransactionIndexType = u32;

/// Represents a Spora transaction outpoint (type alias for exec OutPoint).
///
/// Migration: was a struct with `transaction_id: TransactionId` + `index: u32`,
/// now aliases `spora_exec::celltx::OutPoint` which uses `tx_hash: [u8; 32]` + `index: u32`.
pub type TransactionOutpoint = spora_exec::celltx::OutPoint;

/// Convenience constructor that accepts `TransactionId` instead of raw `[u8; 32]`.
pub fn outpoint_from_id(transaction_id: TransactionId, index: u32) -> TransactionOutpoint {
    TransactionOutpoint::new(transaction_id.as_bytes(), index)
}

/// Represents any kind of transaction which has its inputs resolved either as
/// entry views or as canonical Cell metadata and can be verified/signed.
///
/// Now based on [`CellTx`] rather than the old `Transaction` struct.
pub trait VerifiableTransaction {
    fn tx(&self) -> &CellTx;

    fn inputs(&self) -> &[CellInput] {
        &self.tx().inputs
    }

    fn outputs(&self) -> &[CellOutput] {
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

    fn cell_entry(&self, index: usize) -> Option<&CellMeta>;

    fn cell_metadata(&self, index: usize) -> Option<CellMetadata> {
        self.cell_entry(index).map(CellMetadata::from)
    }
}

/// Represents a read-only referenced transaction along with fully resolved input data.
#[derive(Debug)]
pub struct PopulatedTransaction<'a> {
    pub tx: &'a CellTx,
    pub entries: Vec<CellMeta>,
    pub resolved_cell_metadata: Vec<Option<CellMetadata>>,
}

impl<'a> PopulatedTransaction<'a> {
    pub fn new(tx: &'a CellTx, entries: Vec<CellMeta>) -> Self {
        assert_eq!(tx.inputs.len(), entries.len());
        let resolved_cell_metadata = entries.iter().map(|entry| Some(CellMetadata::from(entry))).collect();
        Self { tx, entries, resolved_cell_metadata }
    }
}

impl VerifiableTransaction for PopulatedTransaction<'_> {
    fn tx(&self) -> &CellTx {
        self.tx
    }

    fn cell_entry(&self, index: usize) -> Option<&CellMeta> {
        self.entries.get(index)
    }

    fn cell_metadata(&self, index: usize) -> Option<CellMetadata> {
        self.resolved_cell_metadata.get(index).and_then(Option::clone)
    }
}

/// Represents a validated transaction with fully resolved input data and a calculated fee.
pub struct ValidatedTransaction<'a> {
    pub tx: &'a CellTx,
    pub entries: Vec<CellMeta>,
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

    fn cell_entry(&self, index: usize) -> Option<&CellMeta> {
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

    /// Convert this canonical Cell transaction into the signable view.
    pub fn into_signable_transaction(self) -> SignableTransaction {
        MutableTransaction::with_resolved_metadata(self.tx, self.resolved_inputs)
    }
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
/// partially filled entry views and/or resolved canonical Cell metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MutableTransaction<T: CellTxContainer = std::sync::Arc<CellTx>> {
    /// The inner CellTx transaction
    pub tx: T,
    /// Partially filled Cell entry data
    pub entries: Vec<Option<CellMeta>>,
    /// Resolved Cell metadata for each input when available.
    pub resolved_cell_metadata: Vec<Option<CellMetadata>>,
    /// Populated fee
    pub calculated_fee: Option<u64>,
    /// Populated non-contextual masses (does not include the storage mass)
    pub calculated_non_contextual_masses: Option<NonContextualMasses>,
    /// Populated contextual masses once inputs are resolved.
    pub calculated_contextual_masses: Option<ContextualMasses>,
    /// Actual VM script cycles returned by the Cell validator when available.
    pub verified_cycles: Option<u64>,
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
            calculated_contextual_masses: None,
            verified_cycles: None,
        }
    }

    pub fn id(&self) -> TransactionId {
        TransactionId::from_bytes(self.tx.cell_tx().id())
    }

    pub fn with_entries(tx: T, entries: Vec<CellMeta>) -> Self {
        assert_eq!(tx.cell_tx().inputs.len(), entries.len());
        let resolved_cell_metadata = entries.iter().map(|entry| Some(CellMetadata::from(entry))).collect();
        Self {
            tx,
            entries: entries.into_iter().map(Some).collect(),
            resolved_cell_metadata,
            calculated_fee: None,
            calculated_non_contextual_masses: None,
            calculated_contextual_masses: None,
            verified_cycles: None,
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
            calculated_contextual_masses: None,
            verified_cycles: None,
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
        self.is_verifiable()
            && self.calculated_fee.is_some()
            && self.calculated_non_contextual_masses.is_some()
            && self.calculated_contextual_masses.is_some()
    }

    pub fn missing_outpoints(&self) -> impl Iterator<Item = TransactionOutpoint> + '_ {
        assert_eq!(self.entries.len(), self.tx.cell_tx().inputs.len());
        self.entries.iter().zip(self.resolved_cell_metadata.iter()).enumerate().filter_map(|(i, (entry, metadata))| {
            if entry.is_none() && metadata.is_none() {
                Some(self.tx.cell_tx().inputs[i].previous_output)
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
    pub fn effective_compute_mass(&self) -> Option<u64> {
        self.calculated_non_contextual_masses.map(|non_contextual_masses| {
            let effective_size = self
                .verified_cycles
                .map(|verified_cycles| {
                    VmLimits::default().effective_size(cell_tx_estimated_serialized_size(self.tx.cell_tx()) as usize, verified_cycles)
                        as u64
                })
                .unwrap_or(0);
            non_contextual_masses.compute_mass.max(effective_size)
        })
    }

    pub fn contextual_storage_mass(&self) -> Option<u64> {
        self.calculated_contextual_masses.map(|contextual_masses| contextual_masses.storage_mass)
    }

    /// Returns the current mempool/block-template selection mass.
    ///
    /// This is the current one-dimensional projection used by mempool ordering and
    /// block-template selection. It combines the contextual storage mass with the
    /// effective compute mass, which prefers actual VM-verified cycles whenever
    /// they are available.
    pub fn selection_mass(&self) -> Option<u64> {
        self.calculated_non_contextual_masses.zip(self.contextual_storage_mass()).map(|(non_contextual_masses, storage_mass)| {
            let effective_compute_mass = self.effective_compute_mass().unwrap_or(non_contextual_masses.compute_mass);
            effective_compute_mass.max(non_contextual_masses.transient_mass).max(storage_mass)
        })
    }

    pub fn calculated_feerate(&self) -> Option<f64> {
        self.selection_mass().and_then(|selection_mass| self.calculated_fee.map(|fee| fee as f64 / selection_mass as f64))
    }

    /// Returns the cycles value that should be projected into CellPool scoring.
    ///
    /// Prefer actual VM-verified cycles. When they are unavailable, synthesize a
    /// cycles value from the unified effective compute mass so CellPool keeps the
    /// same ordering as the main mempool selection logic.
    pub fn projected_cell_pool_cycles(&self) -> Option<u64> {
        self.verified_cycles.or_else(|| {
            self.effective_compute_mass().map(|effective_compute_mass| {
                let projected_size = effective_compute_mass.max(cell_tx_estimated_serialized_size(self.tx.cell_tx()));
                projected_size.saturating_mul(VmLimits::default().cycles_per_byte)
            })
        })
    }

    /// A function for estimating the amount of memory bytes used by this transaction (dedicated to mempool usage).
    /// We need consistency between estimation calls so only this function should be used for this purpose since
    /// `estimate_mem_bytes` is sensitive to pointer wrappers such as Arc
    pub fn mempool_estimated_bytes(&self) -> usize {
        self.estimate_mem_bytes()
    }

    pub fn has_parent(&self, possible_parent: TransactionId) -> bool {
        let parent_bytes = possible_parent.as_bytes();
        self.tx.cell_tx().inputs.iter().any(|x| x.previous_output.tx_hash == parent_bytes)
    }

    pub fn has_parent_in_set(&self, possible_parents: &std::collections::HashSet<TransactionId>) -> bool {
        self.tx.cell_tx().inputs.iter().any(|x| possible_parents.contains(&TransactionId::from_bytes(x.previous_output.tx_hash)))
    }
}

impl<T: CellTxContainer> MemSizeEstimator for MutableTransaction<T> {
    fn estimate_mem_bytes(&self) -> usize {
        let tx = self.tx.cell_tx();
        let tx_heap_bytes = tx.inputs.capacity() * std::mem::size_of::<CellInput>()
            + tx.cell_deps.capacity() * std::mem::size_of::<CellDep>()
            + tx.header_deps.capacity() * std::mem::size_of::<spora_hashes::Hash>()
            + tx.outputs
                .iter()
                .map(|output| {
                    std::mem::size_of::<CellOutput>()
                        + output.lock.args.len()
                        + output.type_.as_ref().map(|script| script.args.len()).unwrap_or_default()
                })
                .sum::<usize>()
            + tx.outputs_data.capacity() * std::mem::size_of::<Vec<u8>>()
            + tx.outputs_data.iter().map(Vec::len).sum::<usize>()
            + tx.witnesses.capacity() * std::mem::size_of::<Vec<u8>>()
            + tx.witnesses.iter().map(Vec::len).sum::<usize>();
        std::mem::size_of::<Self>()
            + self
                .entries
                .iter()
                .zip(self.resolved_cell_metadata.iter())
                .map(|(_op, metadata)| {
                    std::mem::size_of::<Option<CellMeta>>()
                        + std::mem::size_of::<Option<CellMetadata>>()
                        + metadata.as_ref().and_then(|meta| meta.data.as_ref().map(Vec::len)).unwrap_or_default()
                })
                .sum::<usize>()
            + size_of_val(tx)
            + tx_heap_bytes
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

    fn cell_entry(&self, index: usize) -> Option<&CellMeta> {
        self.inner.entries.get(index).and_then(Option::as_ref)
    }

    fn cell_metadata(&self, index: usize) -> Option<CellMetadata> {
        self.inner.resolved_cell_metadata(index).cloned().or_else(|| self.cell_entry(index).map(CellMetadata::from))
    }
}

/// Specialized impl for `T=Arc<CellTx>`
impl MutableTransaction {
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

    fn lock_script_from_bytes(script: &[u8]) -> Script {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"spora-cell/lock");
        hasher.update(&0u16.to_le_bytes());
        hasher.update(script);
        Script::new(*hasher.finalize().as_bytes(), 0, script.to_vec())
    }

    fn test_cell_tx() -> CellTx {
        let script_bytes: Vec<u8> = vec![
            0x76, 0xa9, 0x21, 0x03, 0x2f, 0x7e, 0x43, 0x0a, 0xa4, 0xc9, 0xd1, 0x59, 0x43, 0x7e, 0x84, 0xb9, 0x75, 0xdc, 0x76, 0xd9,
            0x00, 0x3b, 0xf0, 0x92, 0x2c, 0xf3, 0xaa, 0x45, 0x28, 0x46, 0x4b, 0xab, 0x78, 0x0d, 0xba, 0x5e,
        ];

        // Create a CellTx with 2 inputs and 2 outputs
        let input1 = CellInput::new(
            outpoint_from_id(
                TransactionId::from_slice(&[
                    0x16, 0x5e, 0x38, 0xe8, 0xb3, 0x91, 0x45, 0x95, 0xd9, 0xc6, 0x41, 0xf3, 0xb8, 0xee, 0xc2, 0xf3, 0x46, 0x11, 0x89,
                    0x6b, 0x82, 0x1a, 0x68, 0x3b, 0x7a, 0x4e, 0xde, 0xfe, 0x2c, 0x00, 0x00, 0x00,
                ]),
                0xfffffffa,
            ),
            2, // since value (converted from sequence)
        );
        let input2 = CellInput::new(
            outpoint_from_id(
                TransactionId::from_slice(&[
                    0x4b, 0xb0, 0x75, 0x35, 0xdf, 0xd5, 0x8e, 0x0b, 0x3c, 0xd6, 0x4f, 0xd7, 0x15, 0x52, 0x80, 0x87, 0x2a, 0x04, 0x71,
                    0xbc, 0xf8, 0x30, 0x95, 0x52, 0x6a, 0xce, 0x0e, 0x38, 0xc6, 0x00, 0x00, 0x00,
                ]),
                0xfffffffb,
            ),
            4, // since value (converted from sequence)
        );

        let output1 = CellOutput { lock: lock_script_from_bytes(&script_bytes), type_: None, capacity: 6 };
        let output2 = CellOutput { lock: lock_script_from_bytes(&script_bytes), type_: None, capacity: 7 };

        let witnesses: Vec<Vec<u8>> = vec![
            vec![
                0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12,
                0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
            ],
            vec![
                0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f, 0x30, 0x31, 0x32,
                0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f,
            ],
        ];

        CellTx::new(
            vec![input1, input2],
            vec![], // cell_deps
            vec![output1, output2],
            vec![vec![], vec![]], // outputs_data
            witnesses,
        )
        .expect("test CellTx must be valid")
    }

    #[test]
    fn test_cell_tx_bincode() {
        let tx = test_cell_tx();
        let bts = bincode::serialize(&tx).unwrap();
        let tx2: CellTx = bincode::deserialize(&bts).unwrap();
        assert_eq!(tx, tx2);
    }

    #[test]
    fn test_cell_tx_json() {
        let tx = test_cell_tx();
        let str = serde_json::to_string_pretty(&tx).unwrap();
        let tx2: CellTx = serde_json::from_str(&str).unwrap();
        assert_eq!(tx, tx2);
    }

    #[test]
    fn test_mutable_transaction() {
        let cell_tx = test_cell_tx();
        let mutable_tx = MutableTransaction::from_cell_tx(cell_tx.clone());

        assert_eq!(mutable_tx.id(), TransactionId::from_bytes(cell_tx.id()));
        assert_eq!(mutable_tx.tx.cell_tx().inputs.len(), 2);
        assert_eq!(mutable_tx.tx.cell_tx().outputs.len(), 2);
    }

    #[test]
    fn test_verifiable_transaction() {
        let cell_tx = test_cell_tx();
        let entries = vec![
            CellMeta {
                out_point: cell_tx.inputs[0].previous_output,
                capacity: 1000,
                data_bytes: 0,
                lock_hash: [1u8; 32],
                type_hash: None,
                data_hash: [0u8; 32],
                block_daa_score: 100,
                is_cellbase: false,
                lock_script: None,
                type_script: None,
                data: None,
            },
            CellMeta {
                out_point: cell_tx.inputs[1].previous_output,
                capacity: 2000,
                data_bytes: 0,
                lock_hash: [2u8; 32],
                type_hash: None,
                data_hash: [0u8; 32],
                block_daa_score: 100,
                is_cellbase: false,
                lock_script: None,
                type_script: None,
                data: None,
            },
        ];

        let populated = PopulatedTransaction::new(&cell_tx, entries);
        assert_eq!(populated.tx.inputs.len(), 2);
        assert_eq!(populated.tx.outputs.len(), 2);
    }

    #[test]
    fn test_effective_compute_mass_prefers_verified_cycles() {
        let cell_tx = test_cell_tx();
        let mut mutable_tx = MutableTransaction::from_cell_tx(cell_tx.clone());
        mutable_tx.calculated_non_contextual_masses = Some(NonContextualMasses::new(100, 50));
        mutable_tx.verified_cycles = Some(1_000_000);

        let expected_effective_size =
            VmLimits::default().effective_size(cell_tx_estimated_serialized_size(&cell_tx) as usize, 1_000_000) as u64;
        assert_eq!(mutable_tx.effective_compute_mass(), Some(expected_effective_size.max(100)));
    }

    #[test]
    fn test_selection_mass_and_feerate_use_verified_cycles() {
        let cell_tx = test_cell_tx();
        let mut mutable_tx = MutableTransaction::from_cell_tx(cell_tx.clone());
        mutable_tx.calculated_non_contextual_masses = Some(NonContextualMasses::new(100, 50));
        mutable_tx.calculated_contextual_masses = Some(ContextualMasses::new(12_500));
        mutable_tx.calculated_fee = Some(20_000);
        mutable_tx.verified_cycles = Some(1_000_000);

        let expected_effective_compute_mass =
            VmLimits::default().effective_size(cell_tx_estimated_serialized_size(&cell_tx) as usize, 1_000_000) as u64;
        let expected_selection_mass = expected_effective_compute_mass.max(100).max(50).max(12_500);

        assert_eq!(mutable_tx.selection_mass(), Some(expected_selection_mass));
        assert_eq!(mutable_tx.calculated_feerate(), Some(20_000f64 / expected_selection_mass as f64));
    }
}
