// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Cell transaction core types (CKB-inspired, DAG-adapted)
//
// Reference: ckb/util/types/src/core/cell.rs

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt};

/// Serde helpers for serializing `[u8; 32]` as a hex string under the key `transactionId`
/// for human-readable formats (JSON), or raw bytes for binary formats (bincode).
mod outpoint_serde {
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(tx_hash: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            let hex: String = tx_hash.iter().map(|b| format!("{:02x}", b)).collect();
            serializer.serialize_str(&hex)
        } else {
            serde::Serialize::serialize(tx_hash, serializer)
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            if s.len() != 64 {
                return Err(serde::de::Error::custom(format!("expected 64 hex chars, got {}", s.len())));
            }
            let mut bytes = [0u8; 32];
            for i in 0..32 {
                bytes[i] = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).map_err(serde::de::Error::custom)?;
            }
            Ok(bytes)
        } else {
            <[u8; 32]>::deserialize(deserializer)
        }
    }
}

/// Cell transaction version: 0xC001
pub const CELL_TX_VERSION: u32 = 0xC001;
/// Little-endian bytes for the CellScript scheduler witness magic `0xCE11`.
pub const CELLSCRIPT_SCHEDULER_WITNESS_MAGIC: [u8; 2] = [0x11, 0xCE];
/// CellScript scheduler witness format version accepted for transaction placement.
pub const CELLSCRIPT_SCHEDULER_WITNESS_VERSION: u8 = 1;
/// Scheduler effect class id for pure actions.
pub const CELLSCRIPT_SCHEDULER_EFFECT_PURE: u8 = 0;
/// Scheduler effect class id for read-only actions.
pub const CELLSCRIPT_SCHEDULER_EFFECT_READ_ONLY: u8 = 1;
/// Scheduler effect class id for mutating actions.
pub const CELLSCRIPT_SCHEDULER_EFFECT_MUTATING: u8 = 2;
/// Scheduler effect class id for creating actions.
pub const CELLSCRIPT_SCHEDULER_EFFECT_CREATING: u8 = 3;
/// Scheduler effect class id for destroying actions.
pub const CELLSCRIPT_SCHEDULER_EFFECT_DESTROYING: u8 = 4;
/// Scheduler operation id for `consume`.
pub const CELLSCRIPT_SCHEDULER_OP_CONSUME: u8 = 1;
/// Scheduler operation id for `transfer`.
pub const CELLSCRIPT_SCHEDULER_OP_TRANSFER: u8 = 2;
/// Scheduler operation id for `destroy`.
pub const CELLSCRIPT_SCHEDULER_OP_DESTROY: u8 = 3;
/// Scheduler operation id for `claim`.
pub const CELLSCRIPT_SCHEDULER_OP_CLAIM: u8 = 4;
/// Scheduler operation id for `settle`.
pub const CELLSCRIPT_SCHEDULER_OP_SETTLE: u8 = 5;
/// Scheduler operation id for `read_ref`.
pub const CELLSCRIPT_SCHEDULER_OP_READ_REF: u8 = 6;
/// Scheduler operation id for `create`.
pub const CELLSCRIPT_SCHEDULER_OP_CREATE: u8 = 7;
/// Scheduler operation id for mutable input replacement checks.
pub const CELLSCRIPT_SCHEDULER_OP_MUTATE_INPUT: u8 = 8;
/// Scheduler operation id for mutable output replacement checks.
pub const CELLSCRIPT_SCHEDULER_OP_MUTATE_OUTPUT: u8 = 9;
/// Scheduler source id for transaction inputs.
pub const CELLSCRIPT_SCHEDULER_SOURCE_INPUT: u8 = 1;
/// Scheduler source id for cell dependencies.
pub const CELLSCRIPT_SCHEDULER_SOURCE_CELL_DEP: u8 = 2;
/// Scheduler source id for transaction outputs.
pub const CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT: u8 = 3;
/// Domain used by the versioned script hash format.
pub const SCRIPT_HASH_V1_DOMAIN: &[u8] = b"spora-cell/script-hash";
/// Additional bytes a live-cell state entry needs beyond the raw output body.
const CELL_ENTRY_OVERHEAD_EXCLUDING_OUTPUT_BODY: u64 = 32 + 4 + 8 + 1;
/// Static transient-mass factor used before block-context VM cycles are known.
///
/// This intentionally mirrors the consensus-side transient-byte policy until the
/// Cell-native mass model is fully centralized.
const TRANSIENT_BYTE_TO_MASS_FACTOR: u64 = 4;
/// Mass coefficient for each serialized transaction byte.
///
/// Kept in sync with the consensus-side default params so pre-VM estimates in the
/// exec crate match the non-contextual compute mass policy.
const MASS_PER_TX_BYTE: u64 = 1;
/// Mass coefficient for output lock/type script bytes.
const MASS_PER_SCRIPT_PUB_KEY_BYTE: u64 = 10;
/// Mass coefficient for each implicit input sigop.
const MASS_PER_SIG_OP: u64 = 1000;

/// Estimated serialized size of a `CellTx`.
///
/// This is the canonical estimator shared by exec-side estimate helpers and
/// consensus-side mass calculation. Keep this logic centralized to avoid
/// drift between compatibility estimates and the authoritative mass path.
pub fn cell_tx_estimated_serialized_size(tx: &CellTx) -> u64 {
    let mut size: u64 = 0;
    size += 2; // ver (u16)

    // Inputs: each CellInput = outpoint (32+4) + since (8) = 44 bytes
    size += 8; // number of inputs
    size += tx.inputs.len() as u64 * 44;

    // Deps: each CellDep = outpoint (32+4) + dep_type (1) = 37 bytes
    size += 8; // number of deps
    size += tx.cell_deps.len() as u64 * 37;

    // Header deps: each is a 32-byte hash
    size += 8; // number of header_deps
    size += tx.header_deps.len() as u64 * 32;

    // Outputs: each CellOutput = lock script + optional type script + capacity
    size += 8; // number of outputs
    for output in &tx.outputs {
        size += 32 + 1 + 8; // lock.code_hash + lock.hash_type + len(lock.args)
        size += output.lock.args.len() as u64;
        if let Some(ref type_script) = output.type_ {
            size += 1 + 32 + 1 + 8; // flag + code_hash + hash_type + len(args)
            size += type_script.args.len() as u64;
        } else {
            size += 1; // no-type flag
        }
        size += 8; // capacity
    }

    // Outputs data
    size += 8; // number of outputs_data
    for data in &tx.outputs_data {
        size += 8; // length prefix
        size += data.len() as u64;
    }

    // Witnesses
    size += 8; // number of witnesses
    for witness in &tx.witnesses {
        size += 8; // length prefix
        size += witness.len() as u64;
    }

    size
}

/// Returns true when bytes look like a CellScript scheduler witness.
///
/// This is an admission guard for transaction witness placement only. The
/// scheduler consumer must still Borsh-decode and validate the full payload
/// before using it for conflict or admission decisions.
pub fn is_cellscript_scheduler_witness_bytes(witness: &[u8]) -> bool {
    witness.len() >= 3
        && witness[0] == CELLSCRIPT_SCHEDULER_WITNESS_MAGIC[0]
        && witness[1] == CELLSCRIPT_SCHEDULER_WITNESS_MAGIC[1]
        && witness[2] == CELLSCRIPT_SCHEDULER_WITNESS_VERSION
}

/// CellScript scheduler witness access record.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CellScriptSchedulerAccessWitness {
    /// Operation id generated by CellScript (`consume`, `create`, `mutate-input`, etc.).
    pub operation: u8,
    /// Source id generated by CellScript (`Input`, `CellDep`, `Output`).
    pub source: u8,
    /// Source index.
    pub index: u32,
    /// Domain hash of the source-level binding.
    pub binding_hash: [u8; 32],
}

/// CellScript scheduler witness payload as emitted by `cellscript`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CellScriptSchedulerWitness {
    /// Magic marker; must be `0xCE11`.
    pub magic: u16,
    /// Witness format version; currently `1`.
    pub version: u8,
    /// Effect class id generated by CellScript.
    pub effect_class: u8,
    /// Whether the action is marked parallelizable by the compiler.
    pub parallelizable: bool,
    /// Redundant count for admission hardening.
    pub touches_shared_count: u32,
    /// Shared type hashes touched by the action.
    pub touches_shared: Vec<[u8; 32]>,
    /// Compiler-estimated cycles.
    pub estimated_cycles: u64,
    /// Redundant count for admission hardening.
    pub access_count: u32,
    /// Operation/source/index access records.
    pub accesses: Vec<CellScriptSchedulerAccessWitness>,
}

/// Admission error for CellScript scheduler witness bytes.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CellScriptSchedulerWitnessError {
    /// Borsh decode failed.
    #[error("failed to decode CellScript scheduler witness: {0}")]
    Decode(String),
    /// Wrong magic value.
    #[error("invalid CellScript scheduler witness magic: expected 0xCE11, got 0x{0:04x}")]
    InvalidMagic(u16),
    /// Unsupported version.
    #[error("unsupported CellScript scheduler witness version: {0}")]
    UnsupportedVersion(u8),
    /// Redundant count does not match the vector length.
    #[error("CellScript scheduler witness {field} count mismatch: declared {declared}, actual {actual}")]
    CountMismatch {
        /// Field with a redundant count.
        field: &'static str,
        /// Declared count.
        declared: u32,
        /// Actual vector length.
        actual: usize,
    },
    /// Unknown effect-class id.
    #[error("invalid CellScript scheduler effect class id: {0}")]
    InvalidEffectClass(u8),
    /// Unknown access operation id.
    #[error("invalid CellScript scheduler access operation id: {0}")]
    InvalidOperation(u8),
    /// Unknown access source id.
    #[error("invalid CellScript scheduler access source id: {0}")]
    InvalidSource(u8),
    /// Access operation cannot legally target the declared source class.
    #[error("CellScript scheduler access operation {operation} cannot target source {source_id}")]
    UnexpectedSourceForOperation {
        /// Operation id.
        operation: u8,
        /// Source id.
        source_id: u8,
    },
    /// Access source index is outside the transaction-derived source vector.
    #[error("CellScript scheduler access source {source_id} index {index} is out of bounds; available {available}")]
    SourceIndexOutOfBounds {
        /// Source id.
        source_id: u8,
        /// Requested index.
        index: u32,
        /// Number of available entries in the transaction source vector.
        available: usize,
    },
    /// Decoded witness access set does not match a trusted access summary.
    #[error(
        "CellScript scheduler access set mismatch for operation {operation} source {source_id} index {index} binding {binding_hash:?}: expected {expected_count}, actual {actual_count}"
    )]
    AccessSetMismatch {
        /// Operation id.
        operation: u8,
        /// Source id.
        source_id: u8,
        /// Source index.
        index: u32,
        /// Binding hash.
        binding_hash: [u8; 32],
        /// Expected multiset count from trusted metadata or builder summary.
        expected_count: usize,
        /// Actual multiset count in the decoded witness.
        actual_count: usize,
    },
    /// Decoded witness metadata does not match a trusted compiler/builder summary.
    #[error("CellScript scheduler trusted summary mismatch in {field}")]
    TrustedSummaryMismatch {
        /// Witness field whose authenticated summary did not match.
        field: &'static str,
    },
    /// More than one CellScript scheduler witness was found on the same transaction.
    #[error("duplicate CellScript scheduler witnesses on one transaction: {count}")]
    DuplicateSchedulerWitness {
        /// Number of scheduler witness slots discovered.
        count: usize,
    },
}

/// Decode and admit self-contained CellScript scheduler witness bytes.
///
/// This verifies the witness envelope, effect class, and operation/source
/// combinations. Runtime policy must still compare the decoded accesses with
/// transaction-derived source bounds before using the witness for scheduling.
pub fn decode_cellscript_scheduler_witness(bytes: &[u8]) -> Result<CellScriptSchedulerWitness, CellScriptSchedulerWitnessError> {
    let witness = CellScriptSchedulerWitness::try_from_slice(bytes)
        .map_err(|error| CellScriptSchedulerWitnessError::Decode(error.to_string()))?;
    if witness.magic != 0xCE11 {
        return Err(CellScriptSchedulerWitnessError::InvalidMagic(witness.magic));
    }
    if witness.version != CELLSCRIPT_SCHEDULER_WITNESS_VERSION {
        return Err(CellScriptSchedulerWitnessError::UnsupportedVersion(witness.version));
    }
    validate_cellscript_scheduler_witness_counts(&witness)?;
    validate_cellscript_scheduler_witness_envelope(&witness)?;
    Ok(witness)
}

/// Decode a CellScript scheduler witness and validate it against a concrete transaction.
pub fn decode_cellscript_scheduler_witness_for_tx(
    tx: &CellTx,
    bytes: &[u8],
) -> Result<CellScriptSchedulerWitness, CellScriptSchedulerWitnessError> {
    let witness = decode_cellscript_scheduler_witness(bytes)?;
    validate_cellscript_scheduler_witness_against_transaction(tx, &witness)?;
    Ok(witness)
}

/// Produce a trusted scheduler access summary from authenticated compiled metadata.
///
/// The `compiled_scheduler_witness` bytes must come from a trusted CellScript
/// compile artifact or transaction-builder input, not from an untrusted
/// transaction witness. The helper admits the bytes against the concrete
/// transaction shape and returns the operation/source/index/binding_hash
/// multiset that consensus policy can compare against the transaction witness.
pub fn cellscript_compiled_scheduler_accesses_for_tx(
    tx: &CellTx,
    compiled_scheduler_witness: &[u8],
) -> Result<Vec<CellScriptSchedulerAccessWitness>, CellScriptSchedulerWitnessError> {
    cellscript_compiled_scheduler_summary_for_tx(tx, compiled_scheduler_witness).map(|witness| witness.accesses)
}

/// Produce a trusted scheduler summary from authenticated compiled metadata.
///
/// The returned summary is the decoded scheduler witness itself, admitted
/// against the concrete transaction. Consumers can require an untrusted
/// transaction-carried witness to match this full summary before using effect,
/// shared-touch, cycle, or access metadata for scheduling.
pub fn cellscript_compiled_scheduler_summary_for_tx(
    tx: &CellTx,
    compiled_scheduler_witness: &[u8],
) -> Result<CellScriptSchedulerWitness, CellScriptSchedulerWitnessError> {
    decode_cellscript_scheduler_witness_for_tx(tx, compiled_scheduler_witness)
}

/// Validate decoded CellScript scheduler metadata against a concrete transaction.
pub fn validate_cellscript_scheduler_witness_against_transaction(
    tx: &CellTx,
    witness: &CellScriptSchedulerWitness,
) -> Result<(), CellScriptSchedulerWitnessError> {
    validate_cellscript_scheduler_witness_envelope(witness)?;
    for access in &witness.accesses {
        let available = match access.source {
            CELLSCRIPT_SCHEDULER_SOURCE_INPUT => tx.inputs.len(),
            CELLSCRIPT_SCHEDULER_SOURCE_CELL_DEP => tx.cell_deps.len(),
            CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT => tx.outputs.len(),
            source => return Err(CellScriptSchedulerWitnessError::InvalidSource(source)),
        };
        let index = usize::try_from(access.index).unwrap_or(usize::MAX);
        if index >= available {
            return Err(CellScriptSchedulerWitnessError::SourceIndexOutOfBounds {
                source_id: access.source,
                index: access.index,
                available,
            });
        }
    }
    Ok(())
}

/// Validate decoded CellScript scheduler accesses against a trusted access set.
///
/// This is the Phase-3 bridge from advisory metadata to policy-consumable
/// metadata: a transaction builder, compiler artifact, or other trusted source
/// can provide the expected operation/source/index/binding-hash multiset, and
/// the decoded witness must match it exactly before scheduler use.
pub fn validate_cellscript_scheduler_witness_access_set(
    witness: &CellScriptSchedulerWitness,
    expected_accesses: &[CellScriptSchedulerAccessWitness],
) -> Result<(), CellScriptSchedulerWitnessError> {
    validate_cellscript_scheduler_witness_counts(witness)?;
    validate_cellscript_scheduler_witness_envelope(witness)?;
    let mut expected = BTreeMap::<SchedulerAccessKey, usize>::new();
    for access in expected_accesses {
        validate_cellscript_scheduler_access_envelope(access)?;
        *expected.entry(SchedulerAccessKey::from(access)).or_default() += 1;
    }
    let mut actual = BTreeMap::<SchedulerAccessKey, usize>::new();
    for access in &witness.accesses {
        *actual.entry(SchedulerAccessKey::from(access)).or_default() += 1;
    }

    for key in expected.keys().chain(actual.keys()) {
        let expected_count = expected.get(key).copied().unwrap_or(0);
        let actual_count = actual.get(key).copied().unwrap_or(0);
        if expected_count != actual_count {
            return Err(CellScriptSchedulerWitnessError::AccessSetMismatch {
                operation: key.operation,
                source_id: key.source,
                index: key.index,
                binding_hash: key.binding_hash,
                expected_count,
                actual_count,
            });
        }
    }
    Ok(())
}

/// Validate decoded CellScript scheduler metadata against a trusted full summary.
///
/// Access records remain order-insensitive and multiplicity-sensitive, while
/// effect class, parallelizability, shared touch multiset, and cycle estimate
/// must match the authenticated compiler/builder summary exactly.
pub fn validate_cellscript_scheduler_witness_summary(
    witness: &CellScriptSchedulerWitness,
    expected: &CellScriptSchedulerWitness,
) -> Result<(), CellScriptSchedulerWitnessError> {
    validate_cellscript_scheduler_witness_counts(witness)?;
    validate_cellscript_scheduler_witness_counts(expected)?;
    validate_cellscript_scheduler_witness_envelope(witness)?;
    validate_cellscript_scheduler_witness_envelope(expected)?;

    if witness.magic != expected.magic {
        return Err(CellScriptSchedulerWitnessError::TrustedSummaryMismatch { field: "magic" });
    }
    if witness.version != expected.version {
        return Err(CellScriptSchedulerWitnessError::TrustedSummaryMismatch { field: "version" });
    }
    if witness.effect_class != expected.effect_class {
        return Err(CellScriptSchedulerWitnessError::TrustedSummaryMismatch { field: "effect_class" });
    }
    if witness.parallelizable != expected.parallelizable {
        return Err(CellScriptSchedulerWitnessError::TrustedSummaryMismatch { field: "parallelizable" });
    }
    if witness.estimated_cycles != expected.estimated_cycles {
        return Err(CellScriptSchedulerWitnessError::TrustedSummaryMismatch { field: "estimated_cycles" });
    }
    if counted_hashes(&witness.touches_shared) != counted_hashes(&expected.touches_shared) {
        return Err(CellScriptSchedulerWitnessError::TrustedSummaryMismatch { field: "touches_shared" });
    }
    validate_cellscript_scheduler_witness_access_set(witness, &expected.accesses)
}

impl CellScriptSchedulerWitness {
    /// Validate this decoded witness against a concrete transaction.
    pub fn validate_against_transaction(&self, tx: &CellTx) -> Result<(), CellScriptSchedulerWitnessError> {
        validate_cellscript_scheduler_witness_against_transaction(tx, self)
    }

    /// Validate this decoded witness against a trusted access multiset.
    pub fn validate_access_set(
        &self,
        expected_accesses: &[CellScriptSchedulerAccessWitness],
    ) -> Result<(), CellScriptSchedulerWitnessError> {
        validate_cellscript_scheduler_witness_access_set(self, expected_accesses)
    }

    /// Validate this decoded witness against a trusted full scheduler summary.
    pub fn validate_summary(&self, expected: &CellScriptSchedulerWitness) -> Result<(), CellScriptSchedulerWitnessError> {
        validate_cellscript_scheduler_witness_summary(self, expected)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct SchedulerAccessKey {
    operation: u8,
    source: u8,
    index: u32,
    binding_hash: [u8; 32],
}

impl From<&CellScriptSchedulerAccessWitness> for SchedulerAccessKey {
    fn from(access: &CellScriptSchedulerAccessWitness) -> Self {
        Self { operation: access.operation, source: access.source, index: access.index, binding_hash: access.binding_hash }
    }
}

fn validate_cellscript_scheduler_witness_envelope(
    witness: &CellScriptSchedulerWitness,
) -> Result<(), CellScriptSchedulerWitnessError> {
    if !matches!(
        witness.effect_class,
        CELLSCRIPT_SCHEDULER_EFFECT_PURE
            | CELLSCRIPT_SCHEDULER_EFFECT_READ_ONLY
            | CELLSCRIPT_SCHEDULER_EFFECT_MUTATING
            | CELLSCRIPT_SCHEDULER_EFFECT_CREATING
            | CELLSCRIPT_SCHEDULER_EFFECT_DESTROYING
    ) {
        return Err(CellScriptSchedulerWitnessError::InvalidEffectClass(witness.effect_class));
    }
    for access in &witness.accesses {
        validate_cellscript_scheduler_access_envelope(access)?;
    }
    Ok(())
}

fn validate_cellscript_scheduler_witness_counts(witness: &CellScriptSchedulerWitness) -> Result<(), CellScriptSchedulerWitnessError> {
    if witness.touches_shared_count as usize != witness.touches_shared.len() {
        return Err(CellScriptSchedulerWitnessError::CountMismatch {
            field: "touches_shared",
            declared: witness.touches_shared_count,
            actual: witness.touches_shared.len(),
        });
    }
    if witness.access_count as usize != witness.accesses.len() {
        return Err(CellScriptSchedulerWitnessError::CountMismatch {
            field: "accesses",
            declared: witness.access_count,
            actual: witness.accesses.len(),
        });
    }
    Ok(())
}

fn counted_hashes(hashes: &[[u8; 32]]) -> BTreeMap<[u8; 32], usize> {
    let mut counts = BTreeMap::new();
    for hash in hashes {
        *counts.entry(*hash).or_default() += 1;
    }
    counts
}

fn validate_cellscript_scheduler_access_envelope(
    access: &CellScriptSchedulerAccessWitness,
) -> Result<(), CellScriptSchedulerWitnessError> {
    if !matches!(
        access.operation,
        CELLSCRIPT_SCHEDULER_OP_CONSUME
            | CELLSCRIPT_SCHEDULER_OP_TRANSFER
            | CELLSCRIPT_SCHEDULER_OP_DESTROY
            | CELLSCRIPT_SCHEDULER_OP_CLAIM
            | CELLSCRIPT_SCHEDULER_OP_SETTLE
            | CELLSCRIPT_SCHEDULER_OP_READ_REF
            | CELLSCRIPT_SCHEDULER_OP_CREATE
            | CELLSCRIPT_SCHEDULER_OP_MUTATE_INPUT
            | CELLSCRIPT_SCHEDULER_OP_MUTATE_OUTPUT
    ) {
        return Err(CellScriptSchedulerWitnessError::InvalidOperation(access.operation));
    }
    if !matches!(
        access.source,
        CELLSCRIPT_SCHEDULER_SOURCE_INPUT | CELLSCRIPT_SCHEDULER_SOURCE_CELL_DEP | CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT
    ) {
        return Err(CellScriptSchedulerWitnessError::InvalidSource(access.source));
    }
    if !cellscript_scheduler_operation_accepts_source(access.operation, access.source) {
        return Err(CellScriptSchedulerWitnessError::UnexpectedSourceForOperation {
            operation: access.operation,
            source_id: access.source,
        });
    }
    Ok(())
}

fn cellscript_scheduler_operation_accepts_source(operation: u8, source: u8) -> bool {
    match operation {
        CELLSCRIPT_SCHEDULER_OP_CONSUME | CELLSCRIPT_SCHEDULER_OP_DESTROY | CELLSCRIPT_SCHEDULER_OP_MUTATE_INPUT => {
            source == CELLSCRIPT_SCHEDULER_SOURCE_INPUT
        }
        CELLSCRIPT_SCHEDULER_OP_READ_REF => source == CELLSCRIPT_SCHEDULER_SOURCE_CELL_DEP,
        CELLSCRIPT_SCHEDULER_OP_CREATE | CELLSCRIPT_SCHEDULER_OP_MUTATE_OUTPUT => source == CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
        CELLSCRIPT_SCHEDULER_OP_TRANSFER | CELLSCRIPT_SCHEDULER_OP_CLAIM | CELLSCRIPT_SCHEDULER_OP_SETTLE => {
            source == CELLSCRIPT_SCHEDULER_SOURCE_INPUT || source == CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT
        }
        _ => false,
    }
}

/// Structured capacity validation error shared by Cell outputs and Cell metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CapacityError {
    /// The declared capacity is below the minimum occupied capacity.
    #[error("insufficient capacity: required {required}, available {available}")]
    InsufficientCapacity {
        /// Minimum occupied capacity required by the cell shape.
        required: u64,
        /// Capacity declared by the offending value.
        available: u64,
    },
}

/// Script hash format selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptHashVersion {
    /// Domain-separated format with an explicit version byte.
    V1,
}

/// OutPoint: uniquely identifies a Cell (tx_hash || output_index)
///
/// Reference: CKB OutPoint
#[derive(
    Clone, Copy, Default, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub struct OutPoint {
    /// Transaction hash (32 bytes), serialized as hex string `transactionId` in JSON
    #[serde(rename = "transactionId", with = "outpoint_serde")]
    pub tx_hash: [u8; 32],
    /// Output index (u32)
    pub index: u32,
}

impl OutPoint {
    /// Create a new OutPoint
    pub fn new(tx_hash: [u8; 32], index: u32) -> Self {
        Self { tx_hash, index }
    }

    /// Encode to 36-byte key for indexing
    pub fn to_key(&self) -> [u8; 36] {
        let mut key = [0u8; 36];
        key[..32].copy_from_slice(&self.tx_hash);
        key[32..].copy_from_slice(&self.index.to_le_bytes());
        key
    }

    /// Decode from 36-byte key
    pub fn from_key(key: &[u8; 36]) -> Self {
        let mut tx_hash = [0u8; 32];
        tx_hash.copy_from_slice(&key[..32]);
        let index = u32::from_le_bytes([key[32], key[33], key[34], key[35]]);
        Self { tx_hash, index }
    }
}

impl fmt::Display for OutPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.tx_hash {
            write!(f, "{:02x}", byte)?;
        }
        write!(f, ":{}", self.index)
    }
}

/// Script reference (Lock or Type script)
///
/// Reference: CKB Script
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct Script {
    /// Script code hash (points to a Cell's data)
    pub code_hash: [u8; 32],
    /// Hash type: 0=Data, 1=Type, 2=Data1, 4=Data2
    ///
    /// NOTE: Aligned with CKB ScriptHashType encoding:
    /// - Data = 0
    /// - Type = 1
    /// - Data1 = 2
    /// - Data2 = 4 (NOT 3, to maintain CKB compatibility)
    pub hash_type: u8,
    /// Script arguments (passed to VM)
    pub args: Vec<u8>,
}

impl Script {
    /// Create a new script reference
    pub fn new(code_hash: [u8; 32], hash_type: u8, args: Vec<u8>) -> Self {
        Self { code_hash, hash_type, args }
    }

    /// Calculate the canonical script hash currently used by the protocol.
    pub fn hash(&self) -> [u8; 32] {
        self.hash_v1()
    }

    /// Calculate the V1 script hash with explicit domain separation and versioning.
    pub fn hash_v1(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(SCRIPT_HASH_V1_DOMAIN);
        hasher.update(&[1u8]);
        hasher.update(&self.code_hash);
        hasher.update(&[self.hash_type]);
        hasher.update(&(self.args.len() as u32).to_le_bytes());
        hasher.update(&self.args);
        *hasher.finalize().as_bytes()
    }

    /// Calculate the script hash using an explicit format version.
    pub fn hash_with_version(&self, version: ScriptHashVersion) -> [u8; 32] {
        match version {
            ScriptHashVersion::V1 => self.hash_v1(),
        }
    }

    /// Serialize the script reference to bytes.
    ///
    /// Format: code_hash (32) || hash_type (1) || args (variable)
    /// This is used by txscript opcodes that inspect output script data.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(33 + self.args.len());
        bytes.extend_from_slice(&self.code_hash);
        bytes.push(self.hash_type);
        bytes.extend_from_slice(&self.args);
        bytes
    }
}

/// Cell output structure
///
/// Note: data field is separated to CellTx.outputs_data (CKB optimization)
///
/// Reference: CKB CellOutput
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct CellOutput {
    /// Lock script: defines who can spend this Cell
    pub lock: Script,
    /// Type script (optional): defines state transition constraints
    pub type_: Option<Script>,
    /// Capacity (saus): amount + storage cost
    pub capacity: u64,
    // ⚠️ NO data field here! Data is in CellTx.outputs_data
}

impl CellOutput {
    /// Calculate occupied capacity (minimum required)
    pub fn occupied_capacity(&self, data_len: usize) -> u64 {
        let mut size = 8; // capacity field
        size += 32 + 1 + self.lock.args.len(); // lock script
        if let Some(ref type_script) = self.type_ {
            size += 32 + 1 + type_script.args.len(); // type script
        }
        size += data_len; // data
        size as u64
    }

    /// Verify capacity is sufficient
    pub fn verify_capacity(&self, data_len: usize) -> Result<(), CapacityError> {
        let occupied = self.occupied_capacity(data_len);
        if self.capacity < occupied {
            return Err(CapacityError::InsufficientCapacity { required: occupied, available: self.capacity });
        }
        Ok(())
    }
}

/// Cell input reference
///
/// Reference: CKB CellInput
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct CellInput {
    /// Previous output: which Cell to spend (CKB calls this previous_output)
    pub previous_output: OutPoint,
    /// Since: time lock (relative/absolute, timestamp/DAA)
    /// Bit 63: 0=absolute, 1=relative
    /// Bit 62: 0=timestamp, 1=DAA score
    /// Bit 61-0: lock value
    pub since: u64,
}

impl CellInput {
    /// Create a new cell reference
    pub fn new(previous_output: OutPoint, since: u64) -> Self {
        Self { previous_output, since }
    }

    /// Check if this is a relative time lock
    pub fn is_relative_lock(&self) -> bool {
        (self.since & 0x8000_0000_0000_0000) != 0
    }

    /// Check if this uses DAA score (vs timestamp)
    pub fn is_daa_lock(&self) -> bool {
        (self.since & 0x4000_0000_0000_0000) != 0
    }

    /// Get the lock value
    pub fn lock_value(&self) -> u64 {
        self.since & 0x3FFF_FFFF_FFFF_FFFF
    }
}

/// Cell dependency
///
/// Reference: CKB CellDep
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct CellDep {
    /// OutPoint: which Cell to depend on
    pub out_point: OutPoint,
    /// Dependency type
    pub dep_type: DepType,
}

/// Dependency type
///
/// Reference: CKB DepType
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum DepType {
    /// Code: single Cell as script code
    Code = 0,
    /// DepGroup: a Cell containing multiple OutPoints (batch dependency)
    DepGroup = 1,
}

/// Parse DepGroup cell data as a list of OutPoints.
///
/// Format: 4-byte LE count, then count × 36-byte entries
/// (32-byte tx_hash + 4-byte LE index per OutPoint).
pub fn parse_dep_group_data(data: &[u8]) -> Result<Vec<OutPoint>, String> {
    if data.len() < 4 {
        return Err("DepGroup data too short for count header".into());
    }
    let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let expected = 4 + count * 36;
    if data.len() != expected {
        return Err(format!("DepGroup data length mismatch: expected {} bytes for {} outpoints, got {}", expected, count, data.len()));
    }
    let mut outpoints = Vec::with_capacity(count);
    for i in 0..count {
        let offset = 4 + i * 36;
        let key: &[u8; 36] = data[offset..offset + 36].try_into().map_err(|_| "slice conversion failed")?;
        outpoints.push(OutPoint::from_key(key));
    }
    Ok(outpoints)
}

/// Encode a list of OutPoints into DepGroup cell data format.
///
/// This is the inverse of [`parse_dep_group_data`].
pub fn encode_dep_group_data(outpoints: &[OutPoint]) -> Vec<u8> {
    let mut data = Vec::with_capacity(4 + outpoints.len() * 36);
    data.extend_from_slice(&(outpoints.len() as u32).to_le_bytes());
    for op in outpoints {
        data.extend_from_slice(&op.to_key());
    }
    data
}

/// Cell transaction (complete structure)
///
/// Reference: CKB Transaction
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct CellTx {
    /// Transaction version: 0xC001 (Cell v1)
    pub version: u32,
    /// Inputs: Cells to spend
    pub inputs: Vec<CellInput>,
    /// Cell dependencies: read-only Cells (e.g., script code)
    pub cell_deps: Vec<CellDep>,
    /// Header dependencies available to VM scripts.
    pub header_deps: Vec<[u8; 32]>,
    /// Outputs: new Cells to create
    pub outputs: Vec<CellOutput>,
    /// Output data (1:1 with outputs)
    /// Note: CKB separates outputs and data for verification optimization
    pub outputs_data: Vec<Vec<u8>>,
    /// Witnesses: signatures, multi-sig scripts, etc.
    pub witnesses: Vec<Vec<u8>>,
}

impl CellTx {
    /// Create a new Cell transaction
    pub fn new(
        inputs: Vec<CellInput>,
        cell_deps: Vec<CellDep>,
        outputs: Vec<CellOutput>,
        outputs_data: Vec<Vec<u8>>,
        witnesses: Vec<Vec<u8>>,
    ) -> Result<Self, &'static str> {
        Self::new_with_header_deps(inputs, cell_deps, vec![], outputs, outputs_data, witnesses)
    }

    /// Create a new Cell transaction with explicit header dependencies.
    pub fn new_with_header_deps(
        inputs: Vec<CellInput>,
        cell_deps: Vec<CellDep>,
        header_deps: Vec<[u8; 32]>,
        outputs: Vec<CellOutput>,
        outputs_data: Vec<Vec<u8>>,
        witnesses: Vec<Vec<u8>>,
    ) -> Result<Self, &'static str> {
        if outputs.len() != outputs_data.len() {
            return Err("outputs and outputs_data length mismatch");
        }
        Ok(Self { version: CELL_TX_VERSION, inputs, cell_deps, header_deps, outputs, outputs_data, witnesses })
    }

    /// Append a CellScript scheduler witness to the transaction witness vector.
    ///
    /// The witness is kept as an ordinary transaction witness so existing txid /
    /// wtxid and VM loading rules remain unchanged. Phase-3 scheduler policy can
    /// scan for this marker without trusting source-level sidecars.
    pub fn push_cellscript_scheduler_witness(&mut self, scheduler_witness: Vec<u8>) -> Result<(), &'static str> {
        if !is_cellscript_scheduler_witness_bytes(&scheduler_witness) {
            return Err("invalid CellScript scheduler witness header");
        }
        if self.cellscript_scheduler_witnesses().next().is_some() {
            return Err("duplicate CellScript scheduler witness");
        }
        self.witnesses.push(scheduler_witness);
        Ok(())
    }

    /// Return a transaction with an appended CellScript scheduler witness.
    pub fn with_cellscript_scheduler_witness(mut self, scheduler_witness: Vec<u8>) -> Result<Self, &'static str> {
        self.push_cellscript_scheduler_witness(scheduler_witness)?;
        Ok(self)
    }

    /// Append a trusted compiled CellScript scheduler witness and return its access summary.
    ///
    /// This is the transaction-builder side of the Phase-3 trust boundary:
    /// callers pass bytes from authenticated compiled metadata, this method
    /// admits them against the concrete transaction's Input/CellDep/Output
    /// vectors, appends the witness bytes, and returns the access multiset that
    /// consensus policy can later require before scheduler merge.
    pub fn push_cellscript_compiled_scheduler_witness(
        &mut self,
        compiled_scheduler_witness: Vec<u8>,
    ) -> Result<CellScriptSchedulerWitness, CellScriptSchedulerWitnessError> {
        let existing_count = self.cellscript_scheduler_witnesses().count();
        if existing_count > 0 {
            return Err(CellScriptSchedulerWitnessError::DuplicateSchedulerWitness { count: existing_count + 1 });
        }
        let trusted_summary = cellscript_compiled_scheduler_summary_for_tx(self, &compiled_scheduler_witness)?;
        self.witnesses.push(compiled_scheduler_witness);
        Ok(trusted_summary)
    }

    /// Return a transaction with an appended compiled CellScript scheduler witness.
    pub fn with_cellscript_compiled_scheduler_witness(
        mut self,
        compiled_scheduler_witness: Vec<u8>,
    ) -> Result<(Self, CellScriptSchedulerWitness), CellScriptSchedulerWitnessError> {
        let trusted_summary = self.push_cellscript_compiled_scheduler_witness(compiled_scheduler_witness)?;
        Ok((self, trusted_summary))
    }

    /// Iterate over witness slots that carry CellScript scheduler metadata.
    pub fn cellscript_scheduler_witnesses(&self) -> impl Iterator<Item = &[u8]> {
        self.witnesses.iter().map(Vec::as_slice).filter(|witness| is_cellscript_scheduler_witness_bytes(witness))
    }

    /// Decode all CellScript scheduler witnesses carried by this transaction.
    pub fn decoded_cellscript_scheduler_witnesses(
        &self,
    ) -> impl Iterator<Item = Result<CellScriptSchedulerWitness, CellScriptSchedulerWitnessError>> + '_ {
        self.cellscript_scheduler_witnesses().map(decode_cellscript_scheduler_witness)
    }

    /// Decode and validate all CellScript scheduler witnesses against this transaction.
    pub fn admitted_cellscript_scheduler_witnesses(
        &self,
    ) -> impl Iterator<Item = Result<CellScriptSchedulerWitness, CellScriptSchedulerWitnessError>> + '_ {
        self.cellscript_scheduler_witnesses().map(move |witness| decode_cellscript_scheduler_witness_for_tx(self, witness))
    }

    /// Get transaction ID (same as compute_txid)
    pub fn id(&self) -> [u8; 32] {
        crate::celltx::compute_txid(self)
    }

    /// Get transaction version
    pub fn version(&self) -> u32 {
        self.version
    }

    /// Check if this is a cellbase (coinbase) transaction
    ///
    /// Cellbase transactions have no inputs (mining reward)
    pub fn is_coinbase(&self) -> bool {
        self.inputs.is_empty()
    }

    /// Get the compute-side mass hint of the transaction.
    ///
    /// This is a deterministic pre-VM compute hint aligned with the
    /// consensus-side non-contextual mass policy:
    ///
    /// - serialized bytes
    /// - output lock/type script bytes
    /// - one implicit sigop per input
    ///
    /// It does not include actual VM-verified cycles, so consensus and mempool
    /// callers must continue using the unified mass pipeline when they need the
    /// authoritative `effective_compute_mass`.
    pub fn estimated_compute_mass(&self) -> u64 {
        let serialized_size = self.serialized_size() as u64;
        let size_mass = serialized_size.saturating_mul(MASS_PER_TX_BYTE);
        let script_mass = self.total_output_script_bytes().saturating_mul(MASS_PER_SCRIPT_PUB_KEY_BYTE);
        let sigops_mass = (self.inputs.len() as u64).saturating_mul(MASS_PER_SIG_OP);
        size_mass.saturating_add(script_mass).saturating_add(sigops_mass)
    }

    /// Get the transient-storage mass of the transaction.
    ///
    /// This tracks temporary mempool/relay footprint using a deterministic
    /// serialized-size based factor before contextual execution data exists.
    pub fn estimated_transient_mass(&self) -> u64 {
        (self.serialized_size() as u64).saturating_mul(TRANSIENT_BYTE_TO_MASS_FACTOR)
    }

    fn total_output_script_bytes(&self) -> u64 {
        self.outputs
            .iter()
            .map(|output| {
                let mut script_size = 32 + 1 + output.lock.args.len() as u64;
                if let Some(ref type_script) = output.type_ {
                    script_size = script_size.saturating_add(32 + 1 + type_script.args.len() as u64);
                }
                script_size
            })
            .sum()
    }

    /// Get the storage-side mass of the transaction.
    ///
    /// This tracks the persistent live-cell footprint created by outputs,
    /// including per-entry overhead in the state commitment layer.
    ///
    /// It remains an output-footprint estimate and is not the contextual
    /// KIP-0009 storage truth used after input resolution.
    pub fn estimated_storage_mass(&self) -> u64 {
        self.outputs
            .iter()
            .zip(self.outputs_data.iter())
            .map(|(output, data)| CELL_ENTRY_OVERHEAD_EXCLUDING_OUTPUT_BODY + output.occupied_capacity(data.len()))
            .sum()
    }

    /// Get cellbase payload (first output data for coinbase tx).
    pub fn payload(&self) -> Option<&[u8]> {
        if !self.is_coinbase() {
            return None;
        }

        if let Some(first_output_data) = self.outputs_data.first() {
            return Some(first_output_data);
        }

        self.witnesses.first().map(Vec::as_slice)
    }

    /// Estimate serialized size using the canonical shared estimator.
    pub fn serialized_size(&self) -> usize {
        cell_tx_estimated_serialized_size(self) as usize
    }

    /// Calculate total input capacity (requires resolved inputs)
    pub fn input_capacity(&self, resolved_inputs: &[ResolvedCellMeta]) -> u64 {
        resolved_inputs.iter().map(|m| m.cell_output.capacity).sum()
    }

    /// Calculate total output capacity
    pub fn output_capacity(&self) -> u64 {
        self.outputs.iter().map(|o| o.capacity).sum()
    }

    /// Calculate fee (input_capacity - output_capacity)
    pub fn fee(&self, resolved_inputs: &[ResolvedCellMeta]) -> u64 {
        self.input_capacity(resolved_inputs).saturating_sub(self.output_capacity())
    }
}

/// Cell metadata (DAG-aware)
///
/// Reference: CKB CellMeta, specialized for resolved execution inputs.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ResolvedCellMeta {
    /// Cell output structure
    pub cell_output: CellOutput,
    /// OutPoint
    pub out_point: OutPoint,
    /// DAG transaction info
    pub transaction_info: Option<TransactionInfo>,
    /// Data size (bytes)
    pub data_bytes: u64,
    /// In-memory cell data cache
    pub mem_cell_data: Option<Vec<u8>>,
    /// In-memory cell data hash cache
    pub mem_cell_data_hash: Option<[u8; 32]>,
}

impl ResolvedCellMeta {
    /// Check if this is a cellbase (mining reward)
    pub fn is_cellbase(&self) -> bool {
        self.transaction_info.as_ref().map(|info| info.is_cellbase).unwrap_or(false)
    }

    /// Get capacity
    pub fn capacity(&self) -> u64 {
        self.cell_output.capacity
    }
}

/// DAG transaction information
///
/// CKB uses BlockNumber, Spora uses DAA Score
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TransactionInfo {
    /// Transaction hash
    pub tx_hash: [u8; 32],
    /// DAA score (GhostDAG blue score)
    pub daa_score: u64,
    /// Block hash (may be in multiple blocks in DAG)
    pub block_hash: [u8; 32],
    /// Is this a cellbase transaction?
    pub is_cellbase: bool,
}

/// Cell status (for queries)
///
/// Reference: CKB CellStatus
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellStatus {
    /// Cell exists and is unspent
    Live(Box<ResolvedCellMeta>),
    /// Cell has been spent (at given DAA score)
    Dead(u64),
    /// Cell not found in index
    Unknown,
}

/// Resolved Cell transaction (all inputs/deps loaded)
///
/// Reference: CKB ResolvedTransaction
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct ResolvedCellTx {
    /// The transaction
    pub transaction: CellTx,
    /// Resolved inputs
    pub resolved_inputs: Vec<ResolvedCellMeta>,
    /// Resolved dependencies
    pub resolved_deps: Vec<ResolvedCellMeta>,
}

impl AsRef<CellTx> for CellTx {
    fn as_ref(&self) -> &CellTx {
        self
    }
}

impl ResolvedCellTx {
    /// Calculate fee
    pub fn fee(&self) -> u64 {
        self.transaction.fee(&self.resolved_inputs)
    }

    /// Calculate effective fee rate (considering size and cycles)
    pub fn effective_fee_rate(&self, cycles: u64) -> f64 {
        const CYCLES_PER_BYTE: f64 = 100.0;
        let size = self.transaction.serialized_size() as f64;
        let cycles_size = cycles as f64 / CYCLES_PER_BYTE;
        let effective_size = size.max(cycles_size);
        self.fee() as f64 / effective_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn test_outpoint_key_encoding() {
        let op = OutPoint::new([0x42; 32], 0x12345678);
        let key = op.to_key();
        let decoded = OutPoint::from_key(&key);
        assert_eq!(op, decoded);
    }

    #[test]
    fn test_script_hash() {
        let script = Script::new([0x11; 32], 1, vec![0xAA, 0xBB]);
        let hash = script.hash();
        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_script_hash_v1_is_versioned_and_distinct() {
        let script = Script::new([0x11; 32], 1, vec![0xAA, 0xBB]);
        let canonical = script.hash();
        let versioned = script.hash_v1();

        assert_eq!(canonical, versioned);
        assert_eq!(versioned, script.hash_with_version(ScriptHashVersion::V1));
    }

    #[test]
    fn test_cell_out_capacity() {
        let lock = Script::new([0x00; 32], 0, vec![0; 20]);
        let cell = CellOutput { lock, type_: None, capacity: 1000 };
        let occupied = cell.occupied_capacity(100);
        assert!(occupied > 0);
        assert!(cell.verify_capacity(100).is_ok());
        assert_eq!(
            CellOutput { lock: Script::new([0x00; 32], 0, vec![0; 20]), type_: None, capacity: 10 }.verify_capacity(100),
            Err(CapacityError::InsufficientCapacity { required: occupied, available: 10 })
        );
    }

    #[test]
    fn test_time_lock_flags() {
        let relative_daa_lock = CellInput::new(
            OutPoint::new([0; 32], 0),
            0xC000_0000_0000_0064, // relative + DAA + value=100
        );
        assert!(relative_daa_lock.is_relative_lock());
        assert!(relative_daa_lock.is_daa_lock());
        assert_eq!(relative_daa_lock.lock_value(), 100);
    }

    #[test]
    fn test_celltx_creation() {
        let inputs = vec![CellInput::new(OutPoint::new([0; 32], 0), 0)];
        let deps = vec![];
        let lock = Script::new([0x00; 32], 0, vec![]);
        let outputs = vec![CellOutput { lock, type_: None, capacity: 1000 }];
        let outputs_data = vec![vec![]];
        let witnesses = vec![vec![0; 65]];

        let tx = CellTx::new(inputs, deps, outputs, outputs_data, witnesses);
        assert!(tx.is_ok());
        let tx = tx.unwrap();
        assert_eq!(tx.version, CELL_TX_VERSION);
    }

    #[test]
    fn test_cellscript_scheduler_witness_placement() {
        let lock = Script::new([0x00; 32], 0, vec![]);
        let outputs = vec![CellOutput { lock, type_: None, capacity: 1000 }];
        let outputs_data = vec![vec![]];
        let mut tx = CellTx::new(vec![], vec![], outputs, outputs_data, vec![vec![0xAA]]).unwrap();
        let expected_access = CellScriptSchedulerAccessWitness {
            operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
            source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
            index: 0,
            binding_hash: [0x24; 32],
        };
        let scheduler_witness = borsh::to_vec(&CellScriptSchedulerWitness {
            magic: 0xCE11,
            version: CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
            effect_class: CELLSCRIPT_SCHEDULER_EFFECT_CREATING,
            parallelizable: false,
            touches_shared_count: 1,
            touches_shared: vec![[0x42; 32]],
            estimated_cycles: 64,
            access_count: 1,
            accesses: vec![expected_access.clone()],
        })
        .unwrap();

        assert!(is_cellscript_scheduler_witness_bytes(&scheduler_witness));
        tx.push_cellscript_scheduler_witness(scheduler_witness.clone()).unwrap();

        assert_eq!(tx.witnesses.last().map(Vec::as_slice), Some(scheduler_witness.as_slice()));
        let scheduler_witnesses = tx.cellscript_scheduler_witnesses().collect::<Vec<_>>();
        assert_eq!(scheduler_witnesses, vec![scheduler_witness.as_slice()]);
        let decoded = tx.decoded_cellscript_scheduler_witnesses().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].magic, 0xCE11);
        assert_eq!(decoded[0].touches_shared, vec![[0x42; 32]]);
        assert_eq!(decoded[0].accesses[0].operation, CELLSCRIPT_SCHEDULER_OP_CREATE);
        decoded[0].validate_access_set(&[expected_access]).unwrap();
        let admitted = tx.admitted_cellscript_scheduler_witnesses().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(admitted.len(), 1);
        assert!(tx.with_cellscript_scheduler_witness(vec![0x11, 0xCE, 2]).is_err());
        assert!(!is_cellscript_scheduler_witness_bytes(&[0x11, 0xCE]));
        assert!(!is_cellscript_scheduler_witness_bytes(&[0xCE, 0x11, 1]));
    }

    #[test]
    fn test_cellscript_scheduler_witness_placement_rejects_duplicate_marker() {
        let lock = Script::new([0x00; 32], 0, vec![]);
        let outputs = vec![CellOutput { lock, type_: None, capacity: 1000 }];
        let outputs_data = vec![vec![]];
        let mut tx = CellTx::new(vec![], vec![], outputs, outputs_data, vec![]).unwrap();
        let scheduler_witness = borsh::to_vec(&CellScriptSchedulerWitness {
            magic: 0xCE11,
            version: CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
            effect_class: CELLSCRIPT_SCHEDULER_EFFECT_CREATING,
            parallelizable: false,
            touches_shared_count: 0,
            touches_shared: vec![],
            estimated_cycles: 64,
            access_count: 1,
            accesses: vec![CellScriptSchedulerAccessWitness {
                operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
                source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
                index: 0,
                binding_hash: [0x24; 32],
            }],
        })
        .unwrap();

        tx.push_cellscript_scheduler_witness(scheduler_witness.clone()).unwrap();
        assert_eq!(tx.push_cellscript_scheduler_witness(scheduler_witness), Err("duplicate CellScript scheduler witness"));
        assert_eq!(tx.cellscript_scheduler_witnesses().count(), 1);
    }

    #[test]
    fn test_cellscript_compiled_scheduler_witness_produces_trusted_accesses() {
        let lock = Script::new([0x00; 32], 0, vec![]);
        let outputs = vec![CellOutput { lock, type_: None, capacity: 1000 }];
        let outputs_data = vec![vec![]];
        let mut tx = CellTx::new(vec![], vec![], outputs, outputs_data, vec![]).unwrap();
        let expected_access = CellScriptSchedulerAccessWitness {
            operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
            source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
            index: 0,
            binding_hash: [0x24; 32],
        };
        let compiled_scheduler_witness = borsh::to_vec(&CellScriptSchedulerWitness {
            magic: 0xCE11,
            version: CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
            effect_class: CELLSCRIPT_SCHEDULER_EFFECT_CREATING,
            parallelizable: false,
            touches_shared_count: 0,
            touches_shared: vec![],
            estimated_cycles: 64,
            access_count: 1,
            accesses: vec![expected_access.clone()],
        })
        .unwrap();

        let trusted_summary = tx.push_cellscript_compiled_scheduler_witness(compiled_scheduler_witness.clone()).unwrap();

        assert_eq!(trusted_summary.accesses, vec![expected_access]);
        assert_eq!(tx.witnesses.last().map(Vec::as_slice), Some(compiled_scheduler_witness.as_slice()));
        let admitted = tx.admitted_cellscript_scheduler_witnesses().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(admitted.len(), 1);
        assert_eq!(admitted[0], trusted_summary);
    }

    #[test]
    fn test_cellscript_compiled_scheduler_witness_rejects_unmatched_transaction_shape() {
        let mut tx = CellTx::new(vec![], vec![], vec![], vec![], vec![]).unwrap();
        let compiled_scheduler_witness = borsh::to_vec(&CellScriptSchedulerWitness {
            magic: 0xCE11,
            version: CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
            effect_class: CELLSCRIPT_SCHEDULER_EFFECT_CREATING,
            parallelizable: false,
            touches_shared_count: 0,
            touches_shared: vec![],
            estimated_cycles: 64,
            access_count: 1,
            accesses: vec![CellScriptSchedulerAccessWitness {
                operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
                source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
                index: 0,
                binding_hash: [0x24; 32],
            }],
        })
        .unwrap();

        let error = tx.push_cellscript_compiled_scheduler_witness(compiled_scheduler_witness).unwrap_err();

        assert_eq!(
            error,
            CellScriptSchedulerWitnessError::SourceIndexOutOfBounds {
                source_id: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
                index: 0,
                available: 0,
            }
        );
        assert!(tx.witnesses.is_empty());
    }

    #[test]
    fn test_cellscript_compiled_scheduler_witness_rejects_duplicate_without_append() {
        let lock = Script::new([0x00; 32], 0, vec![]);
        let outputs = vec![CellOutput { lock, type_: None, capacity: 1000 }];
        let outputs_data = vec![vec![]];
        let mut tx = CellTx::new(vec![], vec![], outputs, outputs_data, vec![]).unwrap();
        let compiled_scheduler_witness = borsh::to_vec(&CellScriptSchedulerWitness {
            magic: 0xCE11,
            version: CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
            effect_class: CELLSCRIPT_SCHEDULER_EFFECT_CREATING,
            parallelizable: false,
            touches_shared_count: 0,
            touches_shared: vec![],
            estimated_cycles: 64,
            access_count: 1,
            accesses: vec![CellScriptSchedulerAccessWitness {
                operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
                source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
                index: 0,
                binding_hash: [0x24; 32],
            }],
        })
        .unwrap();

        tx.push_cellscript_compiled_scheduler_witness(compiled_scheduler_witness.clone()).unwrap();
        let error = tx.push_cellscript_compiled_scheduler_witness(compiled_scheduler_witness).unwrap_err();

        assert_eq!(error, CellScriptSchedulerWitnessError::DuplicateSchedulerWitness { count: 2 });
        assert_eq!(tx.cellscript_scheduler_witnesses().count(), 1);
    }

    #[test]
    fn test_cellscript_scheduler_witness_decode_rejects_malformed_counts() {
        let bytes = borsh::to_vec(&CellScriptSchedulerWitness {
            magic: 0xCE11,
            version: CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
            effect_class: 1,
            parallelizable: true,
            touches_shared_count: 2,
            touches_shared: vec![[0x42; 32]],
            estimated_cycles: 32,
            access_count: 0,
            accesses: vec![],
        })
        .unwrap();

        assert_eq!(
            decode_cellscript_scheduler_witness(&bytes),
            Err(CellScriptSchedulerWitnessError::CountMismatch { field: "touches_shared", declared: 2, actual: 1 })
        );
        assert_eq!(
            decode_cellscript_scheduler_witness(&[0x11, 0xCE]),
            Err(CellScriptSchedulerWitnessError::Decode("Unexpected length of input".to_string()))
        );
    }

    #[test]
    fn test_cellscript_scheduler_witness_decode_rejects_invalid_access_envelope() {
        let bytes = borsh::to_vec(&CellScriptSchedulerWitness {
            magic: 0xCE11,
            version: CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
            effect_class: CELLSCRIPT_SCHEDULER_EFFECT_READ_ONLY,
            parallelizable: true,
            touches_shared_count: 0,
            touches_shared: vec![],
            estimated_cycles: 32,
            access_count: 1,
            accesses: vec![CellScriptSchedulerAccessWitness {
                operation: CELLSCRIPT_SCHEDULER_OP_READ_REF,
                source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
                index: 0,
                binding_hash: [0x24; 32],
            }],
        })
        .unwrap();

        assert_eq!(
            decode_cellscript_scheduler_witness(&bytes),
            Err(CellScriptSchedulerWitnessError::UnexpectedSourceForOperation {
                operation: CELLSCRIPT_SCHEDULER_OP_READ_REF,
                source_id: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT
            })
        );
    }

    #[test]
    fn test_cellscript_scheduler_witness_admission_rejects_out_of_bounds_access() {
        let tx = CellTx::new(vec![], vec![], vec![], vec![], vec![]).unwrap();
        let witness = CellScriptSchedulerWitness {
            magic: 0xCE11,
            version: CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
            effect_class: CELLSCRIPT_SCHEDULER_EFFECT_CREATING,
            parallelizable: false,
            touches_shared_count: 0,
            touches_shared: vec![],
            estimated_cycles: 64,
            access_count: 1,
            accesses: vec![CellScriptSchedulerAccessWitness {
                operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
                source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
                index: 0,
                binding_hash: [0x24; 32],
            }],
        };
        let bytes = borsh::to_vec(&witness).unwrap();

        assert_eq!(
            witness.validate_against_transaction(&tx),
            Err(CellScriptSchedulerWitnessError::SourceIndexOutOfBounds {
                source_id: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
                index: 0,
                available: 0
            })
        );
        assert_eq!(
            decode_cellscript_scheduler_witness_for_tx(&tx, &bytes),
            Err(CellScriptSchedulerWitnessError::SourceIndexOutOfBounds {
                source_id: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
                index: 0,
                available: 0
            })
        );
    }

    #[test]
    fn test_cellscript_scheduler_witness_access_set_rejects_binding_mismatch() {
        let witness = CellScriptSchedulerWitness {
            magic: 0xCE11,
            version: CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
            effect_class: CELLSCRIPT_SCHEDULER_EFFECT_CREATING,
            parallelizable: false,
            touches_shared_count: 0,
            touches_shared: vec![],
            estimated_cycles: 64,
            access_count: 1,
            accesses: vec![CellScriptSchedulerAccessWitness {
                operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
                source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
                index: 0,
                binding_hash: [0x24; 32],
            }],
        };
        let expected = [CellScriptSchedulerAccessWitness {
            operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
            source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
            index: 0,
            binding_hash: [0x25; 32],
        }];

        assert_eq!(
            witness.validate_access_set(&expected),
            Err(CellScriptSchedulerWitnessError::AccessSetMismatch {
                operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
                source_id: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
                index: 0,
                binding_hash: [0x25; 32],
                expected_count: 1,
                actual_count: 0
            })
        );
    }

    #[test]
    fn test_cellscript_scheduler_witness_access_set_is_order_insensitive() {
        let create_access = CellScriptSchedulerAccessWitness {
            operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
            source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
            index: 0,
            binding_hash: [0x24; 32],
        };
        let read_access = CellScriptSchedulerAccessWitness {
            operation: CELLSCRIPT_SCHEDULER_OP_READ_REF,
            source: CELLSCRIPT_SCHEDULER_SOURCE_CELL_DEP,
            index: 1,
            binding_hash: [0x42; 32],
        };
        let witness = CellScriptSchedulerWitness {
            magic: 0xCE11,
            version: CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
            effect_class: CELLSCRIPT_SCHEDULER_EFFECT_CREATING,
            parallelizable: false,
            touches_shared_count: 0,
            touches_shared: vec![],
            estimated_cycles: 64,
            access_count: 2,
            accesses: vec![create_access.clone(), read_access.clone()],
        };

        witness.validate_access_set(&[read_access, create_access]).unwrap();
    }

    #[test]
    fn test_cellscript_scheduler_witness_access_set_rejects_unexpected_duplicate() {
        let access = CellScriptSchedulerAccessWitness {
            operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
            source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
            index: 0,
            binding_hash: [0x24; 32],
        };
        let witness = CellScriptSchedulerWitness {
            magic: 0xCE11,
            version: CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
            effect_class: CELLSCRIPT_SCHEDULER_EFFECT_CREATING,
            parallelizable: false,
            touches_shared_count: 0,
            touches_shared: vec![],
            estimated_cycles: 64,
            access_count: 2,
            accesses: vec![access.clone(), access.clone()],
        };

        assert_eq!(
            witness.validate_access_set(&[access]),
            Err(CellScriptSchedulerWitnessError::AccessSetMismatch {
                operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
                source_id: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
                index: 0,
                binding_hash: [0x24; 32],
                expected_count: 1,
                actual_count: 2
            })
        );
    }

    #[test]
    fn test_cellscript_scheduler_witness_access_set_rejects_missing_duplicate() {
        let access = CellScriptSchedulerAccessWitness {
            operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
            source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
            index: 0,
            binding_hash: [0x24; 32],
        };
        let witness = CellScriptSchedulerWitness {
            magic: 0xCE11,
            version: CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
            effect_class: CELLSCRIPT_SCHEDULER_EFFECT_CREATING,
            parallelizable: false,
            touches_shared_count: 0,
            touches_shared: vec![],
            estimated_cycles: 64,
            access_count: 1,
            accesses: vec![access.clone()],
        };

        assert_eq!(
            witness.validate_access_set(&[access.clone(), access]),
            Err(CellScriptSchedulerWitnessError::AccessSetMismatch {
                operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
                source_id: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
                index: 0,
                binding_hash: [0x24; 32],
                expected_count: 2,
                actual_count: 1
            })
        );
    }

    #[test]
    fn test_cellscript_scheduler_witness_summary_rejects_shared_touch_tampering() {
        let access = CellScriptSchedulerAccessWitness {
            operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
            source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
            index: 0,
            binding_hash: [0x24; 32],
        };
        let expected = CellScriptSchedulerWitness {
            magic: 0xCE11,
            version: CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
            effect_class: CELLSCRIPT_SCHEDULER_EFFECT_CREATING,
            parallelizable: false,
            touches_shared_count: 1,
            touches_shared: vec![[0x42; 32]],
            estimated_cycles: 64,
            access_count: 1,
            accesses: vec![access.clone()],
        };
        let actual =
            CellScriptSchedulerWitness { touches_shared_count: 0, touches_shared: vec![], accesses: vec![access], ..expected.clone() };

        assert_eq!(
            actual.validate_summary(&expected),
            Err(CellScriptSchedulerWitnessError::TrustedSummaryMismatch { field: "touches_shared" })
        );
    }

    #[test]
    fn test_cellscript_scheduler_witness_summary_rejects_effect_tampering() {
        let expected = CellScriptSchedulerWitness {
            magic: 0xCE11,
            version: CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
            effect_class: CELLSCRIPT_SCHEDULER_EFFECT_CREATING,
            parallelizable: false,
            touches_shared_count: 0,
            touches_shared: vec![],
            estimated_cycles: 64,
            access_count: 0,
            accesses: vec![],
        };
        let actual = CellScriptSchedulerWitness { effect_class: CELLSCRIPT_SCHEDULER_EFFECT_READ_ONLY, ..expected.clone() };

        assert_eq!(
            actual.validate_summary(&expected),
            Err(CellScriptSchedulerWitnessError::TrustedSummaryMismatch { field: "effect_class" })
        );
    }

    fn scheduler_witness_for_accesses(accesses: Vec<CellScriptSchedulerAccessWitness>) -> CellScriptSchedulerWitness {
        scheduler_witness_for_summary(CELLSCRIPT_SCHEDULER_EFFECT_CREATING, false, vec![], 64, accesses)
    }

    fn scheduler_witness_for_summary(
        effect_class: u8,
        parallelizable: bool,
        touches_shared: Vec<[u8; 32]>,
        estimated_cycles: u64,
        accesses: Vec<CellScriptSchedulerAccessWitness>,
    ) -> CellScriptSchedulerWitness {
        CellScriptSchedulerWitness {
            magic: 0xCE11,
            version: CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
            effect_class,
            parallelizable,
            touches_shared_count: touches_shared.len() as u32,
            touches_shared,
            estimated_cycles,
            access_count: accesses.len() as u32,
            accesses,
        }
    }

    fn effect_class_strategy() -> impl Strategy<Value = u8> {
        prop_oneof![
            Just(CELLSCRIPT_SCHEDULER_EFFECT_PURE),
            Just(CELLSCRIPT_SCHEDULER_EFFECT_READ_ONLY),
            Just(CELLSCRIPT_SCHEDULER_EFFECT_MUTATING),
            Just(CELLSCRIPT_SCHEDULER_EFFECT_CREATING),
            Just(CELLSCRIPT_SCHEDULER_EFFECT_DESTROYING),
        ]
    }

    fn binding_hash_strategy() -> impl Strategy<Value = [u8; 32]> {
        proptest::array::uniform32(any::<u8>())
    }

    fn scheduler_access_strategy() -> impl Strategy<Value = CellScriptSchedulerAccessWitness> {
        prop_oneof![
            (0u32..16, binding_hash_strategy()).prop_map(|(index, binding_hash)| CellScriptSchedulerAccessWitness {
                operation: CELLSCRIPT_SCHEDULER_OP_CONSUME,
                source: CELLSCRIPT_SCHEDULER_SOURCE_INPUT,
                index,
                binding_hash,
            }),
            (0u32..16, binding_hash_strategy()).prop_map(|(index, binding_hash)| CellScriptSchedulerAccessWitness {
                operation: CELLSCRIPT_SCHEDULER_OP_DESTROY,
                source: CELLSCRIPT_SCHEDULER_SOURCE_INPUT,
                index,
                binding_hash,
            }),
            (0u32..16, binding_hash_strategy()).prop_map(|(index, binding_hash)| CellScriptSchedulerAccessWitness {
                operation: CELLSCRIPT_SCHEDULER_OP_MUTATE_INPUT,
                source: CELLSCRIPT_SCHEDULER_SOURCE_INPUT,
                index,
                binding_hash,
            }),
            (0u32..16, binding_hash_strategy()).prop_map(|(index, binding_hash)| CellScriptSchedulerAccessWitness {
                operation: CELLSCRIPT_SCHEDULER_OP_READ_REF,
                source: CELLSCRIPT_SCHEDULER_SOURCE_CELL_DEP,
                index,
                binding_hash,
            }),
            (0u32..16, binding_hash_strategy()).prop_map(|(index, binding_hash)| CellScriptSchedulerAccessWitness {
                operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
                source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
                index,
                binding_hash,
            }),
            (0u32..16, binding_hash_strategy()).prop_map(|(index, binding_hash)| CellScriptSchedulerAccessWitness {
                operation: CELLSCRIPT_SCHEDULER_OP_MUTATE_OUTPUT,
                source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
                index,
                binding_hash,
            }),
            (0u32..16, binding_hash_strategy()).prop_map(|(index, binding_hash)| CellScriptSchedulerAccessWitness {
                operation: CELLSCRIPT_SCHEDULER_OP_TRANSFER,
                source: CELLSCRIPT_SCHEDULER_SOURCE_INPUT,
                index,
                binding_hash,
            }),
            (0u32..16, binding_hash_strategy()).prop_map(|(index, binding_hash)| CellScriptSchedulerAccessWitness {
                operation: CELLSCRIPT_SCHEDULER_OP_TRANSFER,
                source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
                index,
                binding_hash,
            }),
        ]
    }

    fn operation_tamper_strategy() -> impl Strategy<Value = (CellScriptSchedulerAccessWitness, u8)> {
        prop_oneof![
            (0u32..16, binding_hash_strategy()).prop_map(|(index, binding_hash)| {
                (
                    CellScriptSchedulerAccessWitness {
                        operation: CELLSCRIPT_SCHEDULER_OP_CONSUME,
                        source: CELLSCRIPT_SCHEDULER_SOURCE_INPUT,
                        index,
                        binding_hash,
                    },
                    CELLSCRIPT_SCHEDULER_OP_DESTROY,
                )
            }),
            (0u32..16, binding_hash_strategy()).prop_map(|(index, binding_hash)| {
                (
                    CellScriptSchedulerAccessWitness {
                        operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
                        source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
                        index,
                        binding_hash,
                    },
                    CELLSCRIPT_SCHEDULER_OP_MUTATE_OUTPUT,
                )
            }),
        ]
    }

    fn source_tamper_strategy() -> impl Strategy<Value = (CellScriptSchedulerAccessWitness, u8)> {
        prop_oneof![
            (0u32..16, binding_hash_strategy()).prop_map(|(index, binding_hash)| {
                (
                    CellScriptSchedulerAccessWitness {
                        operation: CELLSCRIPT_SCHEDULER_OP_TRANSFER,
                        source: CELLSCRIPT_SCHEDULER_SOURCE_INPUT,
                        index,
                        binding_hash,
                    },
                    CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
                )
            }),
            (0u32..16, binding_hash_strategy()).prop_map(|(index, binding_hash)| {
                (
                    CellScriptSchedulerAccessWitness {
                        operation: CELLSCRIPT_SCHEDULER_OP_CLAIM,
                        source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
                        index,
                        binding_hash,
                    },
                    CELLSCRIPT_SCHEDULER_SOURCE_INPUT,
                )
            }),
        ]
    }

    fn assert_access_set_mismatch(error: CellScriptSchedulerWitnessError) {
        assert!(matches!(error, CellScriptSchedulerWitnessError::AccessSetMismatch { .. }), "{error:?}");
    }

    proptest! {
        #[test]
        fn prop_cellscript_scheduler_access_set_accepts_reordered_multiset(
            accesses in prop::collection::vec(scheduler_access_strategy(), 0..24)
        ) {
            let witness = scheduler_witness_for_accesses(accesses.clone());
            let mut expected = accesses;
            expected.reverse();

            prop_assert!(witness.validate_access_set(&expected).is_ok());
        }

        #[test]
        fn prop_cellscript_scheduler_access_set_rejects_missing_access(
            accesses in prop::collection::vec(scheduler_access_strategy(), 1..24)
        ) {
            let mut actual = accesses.clone();
            actual.pop();
            let witness = scheduler_witness_for_accesses(actual);

            assert_access_set_mismatch(witness.validate_access_set(&accesses).unwrap_err());
        }

        #[test]
        fn prop_cellscript_scheduler_access_set_rejects_unexpected_duplicate(
            accesses in prop::collection::vec(scheduler_access_strategy(), 1..24)
        ) {
            let mut actual = accesses.clone();
            actual.push(accesses[0].clone());
            let witness = scheduler_witness_for_accesses(actual);

            assert_access_set_mismatch(witness.validate_access_set(&accesses).unwrap_err());
        }

        #[test]
        fn prop_cellscript_scheduler_access_set_rejects_binding_hash_tamper(
            accesses in prop::collection::vec(scheduler_access_strategy(), 1..24)
        ) {
            let mut actual = accesses.clone();
            actual[0].binding_hash[0] ^= 0x80;
            let witness = scheduler_witness_for_accesses(actual);

            assert_access_set_mismatch(witness.validate_access_set(&accesses).unwrap_err());
        }

        #[test]
        fn prop_cellscript_scheduler_access_set_rejects_index_tamper(
            accesses in prop::collection::vec(scheduler_access_strategy(), 1..24)
        ) {
            let mut actual = accesses.clone();
            actual[0].index += 1;
            let witness = scheduler_witness_for_accesses(actual);

            assert_access_set_mismatch(witness.validate_access_set(&accesses).unwrap_err());
        }

        #[test]
        fn prop_cellscript_scheduler_access_set_rejects_operation_tamper(
            (access, replacement_operation) in operation_tamper_strategy()
        ) {
            let mut actual = access.clone();
            actual.operation = replacement_operation;
            let witness = scheduler_witness_for_accesses(vec![actual]);

            assert_access_set_mismatch(witness.validate_access_set(&[access]).unwrap_err());
        }

        #[test]
        fn prop_cellscript_scheduler_access_set_rejects_source_tamper(
            (access, replacement_source) in source_tamper_strategy()
        ) {
            let mut actual = access.clone();
            actual.source = replacement_source;
            let witness = scheduler_witness_for_accesses(vec![actual]);

            assert_access_set_mismatch(witness.validate_access_set(&[access]).unwrap_err());
        }

        #[test]
        fn prop_cellscript_scheduler_summary_accepts_reordered_access_and_touch_multisets(
            effect_class in effect_class_strategy(),
            parallelizable in any::<bool>(),
            estimated_cycles in any::<u64>(),
            touches_shared in prop::collection::vec(binding_hash_strategy(), 0..24),
            accesses in prop::collection::vec(scheduler_access_strategy(), 0..24)
        ) {
            let expected = scheduler_witness_for_summary(
                effect_class,
                parallelizable,
                touches_shared.clone(),
                estimated_cycles,
                accesses.clone(),
            );
            let actual = scheduler_witness_for_summary(
                effect_class,
                parallelizable,
                touches_shared.into_iter().rev().collect(),
                estimated_cycles,
                accesses.into_iter().rev().collect(),
            );

            prop_assert!(actual.validate_summary(&expected).is_ok());
        }

        #[test]
        fn prop_cellscript_scheduler_summary_rejects_shared_touch_multiplicity_tamper(
            touch in binding_hash_strategy(),
            accesses in prop::collection::vec(scheduler_access_strategy(), 0..24)
        ) {
            let expected = scheduler_witness_for_summary(
                CELLSCRIPT_SCHEDULER_EFFECT_CREATING,
                false,
                vec![touch],
                64,
                accesses.clone(),
            );
            let actual = scheduler_witness_for_summary(
                CELLSCRIPT_SCHEDULER_EFFECT_CREATING,
                false,
                vec![touch, touch],
                64,
                accesses,
            );

            prop_assert_eq!(
                actual.validate_summary(&expected),
                Err(CellScriptSchedulerWitnessError::TrustedSummaryMismatch { field: "touches_shared" })
            );
        }

        #[test]
        fn prop_cellscript_scheduler_summary_rejects_parallelizable_and_cycle_tamper(
            parallelizable in any::<bool>(),
            estimated_cycles in any::<u64>(),
            accesses in prop::collection::vec(scheduler_access_strategy(), 0..24)
        ) {
            let expected = scheduler_witness_for_summary(
                CELLSCRIPT_SCHEDULER_EFFECT_CREATING,
                parallelizable,
                vec![],
                estimated_cycles,
                accesses.clone(),
            );
            let parallelizable_tamper = scheduler_witness_for_summary(
                CELLSCRIPT_SCHEDULER_EFFECT_CREATING,
                !parallelizable,
                vec![],
                estimated_cycles,
                accesses.clone(),
            );
            let cycle_tamper = scheduler_witness_for_summary(
                CELLSCRIPT_SCHEDULER_EFFECT_CREATING,
                parallelizable,
                vec![],
                estimated_cycles.wrapping_add(1),
                accesses,
            );

            prop_assert_eq!(
                parallelizable_tamper.validate_summary(&expected),
                Err(CellScriptSchedulerWitnessError::TrustedSummaryMismatch { field: "parallelizable" })
            );
            prop_assert_eq!(
                cycle_tamper.validate_summary(&expected),
                Err(CellScriptSchedulerWitnessError::TrustedSummaryMismatch { field: "estimated_cycles" })
            );
        }
    }

    #[test]
    fn test_celltx_compute_and_storage_mass_are_distinct() {
        let inputs = vec![CellInput::new(OutPoint::new([0; 32], 0), 0)];
        let deps = vec![];
        let lock = Script::new([0x10; 32], 1, vec![1; 20]);
        let outputs = vec![CellOutput { lock, type_: None, capacity: 10_000 }];
        let outputs_data = vec![vec![7; 128]];
        let witnesses = vec![vec![0; 65]];

        let tx = CellTx::new(inputs, deps, outputs, outputs_data, witnesses).unwrap();
        assert!(tx.estimated_compute_mass() > tx.serialized_size() as u64);
        assert!(tx.estimated_compute_mass() > 0);
        assert!(tx.estimated_transient_mass() > 0);
        assert!(tx.estimated_storage_mass() > 0);
        assert_eq!(tx.estimated_transient_mass(), (tx.serialized_size() as u64) * TRANSIENT_BYTE_TO_MASS_FACTOR);
        assert_ne!(tx.estimated_compute_mass(), tx.estimated_storage_mass());
    }

    #[test]
    fn test_celltx_compute_mass_matches_non_contextual_formula() {
        let inputs = vec![CellInput::new(OutPoint::new([0x01; 32], 0), 0), CellInput::new(OutPoint::new([0x02; 32], 1), 0)];
        let deps = vec![];
        let outputs = vec![
            CellOutput { lock: Script::new([0x10; 32], 1, vec![1; 20]), type_: None, capacity: 10_000 },
            CellOutput {
                lock: Script::new([0x20; 32], 1, vec![2; 32]),
                type_: Some(Script::new([0x30; 32], 1, vec![3; 12])),
                capacity: 20_000,
            },
        ];
        let outputs_data = vec![vec![0xAA; 16], vec![0xBB; 8]];
        let witnesses = vec![vec![0xCC; 65], vec![0xDD; 32]];

        let tx = CellTx::new(inputs, deps, outputs, outputs_data, witnesses).unwrap();
        let serialized_size = tx.serialized_size() as u64;
        let total_output_script_bytes = (32 + 1 + 20) + (32 + 1 + 32) + (32 + 1 + 12);
        let expected =
            serialized_size * MASS_PER_TX_BYTE + total_output_script_bytes as u64 * MASS_PER_SCRIPT_PUB_KEY_BYTE + 2 * MASS_PER_SIG_OP;

        assert_eq!(tx.estimated_compute_mass(), expected);
    }

    #[test]
    fn test_dep_group_roundtrip() {
        let ops = vec![OutPoint::new([0x11; 32], 0), OutPoint::new([0x22; 32], 7), OutPoint::new([0x33; 32], u32::MAX)];
        let data = encode_dep_group_data(&ops);
        let parsed = parse_dep_group_data(&data).unwrap();
        assert_eq!(parsed, ops);
    }

    #[test]
    fn test_dep_group_empty() {
        let data = encode_dep_group_data(&[]);
        assert_eq!(data, [0, 0, 0, 0]);
        let parsed = parse_dep_group_data(&data).unwrap();
        assert!(parsed.is_empty());
    }

    #[test]
    fn test_dep_group_invalid_data() {
        assert!(parse_dep_group_data(&[]).is_err());
        assert!(parse_dep_group_data(&[1, 0, 0, 0]).is_err()); // count=1 but no data
        assert!(parse_dep_group_data(&[1, 0, 0, 0, 0]).is_err()); // count=1 but only 1 byte
    }
}

// ============================================================================
// VersionedSerializable Implementations
// ============================================================================
//
// These implementations enable schema versioning for storage layer types.
// All Cell transaction types use version 1 as the initial schema version.

use crate::serialization::VersionedSerializable;

/// Current schema version for Cell transaction types
pub const CELLTX_SCHEMA_VERSION: u8 = 1;

impl VersionedSerializable for OutPoint {
    const CURRENT_VERSION: u8 = CELLTX_SCHEMA_VERSION;
}

impl VersionedSerializable for Script {
    const CURRENT_VERSION: u8 = CELLTX_SCHEMA_VERSION;
}

impl VersionedSerializable for CellOutput {
    const CURRENT_VERSION: u8 = CELLTX_SCHEMA_VERSION;
}

impl VersionedSerializable for CellInput {
    const CURRENT_VERSION: u8 = CELLTX_SCHEMA_VERSION;
}

impl VersionedSerializable for CellDep {
    const CURRENT_VERSION: u8 = CELLTX_SCHEMA_VERSION;
}

impl VersionedSerializable for DepType {
    const CURRENT_VERSION: u8 = CELLTX_SCHEMA_VERSION;
}

impl VersionedSerializable for CellTx {
    const CURRENT_VERSION: u8 = CELLTX_SCHEMA_VERSION;
}

impl VersionedSerializable for TransactionInfo {
    const CURRENT_VERSION: u8 = CELLTX_SCHEMA_VERSION;
}

impl VersionedSerializable for ResolvedCellMeta {
    const CURRENT_VERSION: u8 = CELLTX_SCHEMA_VERSION;
}

impl VersionedSerializable for ResolvedCellTx {
    const CURRENT_VERSION: u8 = CELLTX_SCHEMA_VERSION;
}
