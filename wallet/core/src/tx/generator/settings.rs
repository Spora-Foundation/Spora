//!
//! Transaction [`GeneratorSettings`] used when
//! constructing and instance of the [`Generator`](crate::tx::Generator).
//!

use crate::cell::{CellContext, CellEntryReference, CellIterator};
use crate::events::Events;
use crate::imports::*;
use crate::result::Result;
use crate::tx::{Fees, PaymentDestination};
use spora_addresses::Address;
use spora_exec::{celltx::decode_cellscript_scheduler_witness, CellDep, CkbSecp256k1Blake160SighashAllLockConfig, DepType, OutPoint};
use workflow_core::channel::Multiplexer;

const CELLSCRIPT_TARGET_PROFILE_SPORA: &str = "spora";
const CELLSCRIPT_TARGET_PROFILE_TYPED_CELL: &str = "typed-cell";
const CELLSCRIPT_TARGET_PROFILE_CKB: &str = "ckb";
const CELLSCRIPT_TARGET_PROFILE_PORTABLE_CELL: &str = "portable-cell";
const CELLSCRIPT_SCHEDULER_WITNESS_ABI_MOLECULE: &str = "molecule";
const CELLSCRIPT_SCHEDULER_WITNESS_HEX_FIELD: &str = "scheduler_witness_hex";
const CELLSCRIPT_SCHEDULER_WITNESS_MOLECULE_HEX_FIELD: &str = "scheduler_witness_molecule_hex";
const CELLSCRIPT_SCHEDULER_WITNESS_BORSH_HEX_FIELD: &str = "scheduler_witness_borsh_hex";

pub struct GeneratorSettings {
    // Network type
    pub network_id: NetworkId,
    // Event multiplexer
    pub multiplexer: Option<Multiplexer<Box<Events>>>,
    // Cell iterator
    pub cell_iterator: Box<dyn Iterator<Item = CellEntryReference> + Send + Sync + 'static>,
    // Source cell context
    pub source_cell_context: Option<CellContext>,
    // Priority cell entries that are consumed before others
    pub priority_cell_entries: Option<Vec<CellEntryReference>>,
    // number of minimum signatures required to sign the transaction
    pub minimum_signatures: u16,
    // change address
    pub change_address: Address,
    // fee rate
    #[allow(dead_code)]
    pub fee_rate: Option<f64>,
    // applies only to the final transaction
    pub final_transaction_priority_fee: Fees,
    // final transaction outputs
    pub final_transaction_destination: PaymentDestination,
    // payload
    pub final_transaction_payload: Option<Vec<u8>>,
    // CellScript compiled scheduler witness bytes for the final transaction.
    pub final_cellscript_compiled_scheduler_witness: Option<Vec<u8>>,
    // Cell dependencies to include in every generated transaction.
    pub cell_deps: Vec<CellDep>,
    // Header dependencies to include in every generated transaction.
    pub header_deps: Vec<[u8; 32]>,
    // Final-transaction user output indexes that should receive a CKB built-in TYPE_ID type script.
    pub ckb_type_id_output_indexes: Vec<usize>,
    // transaction is a transfer between accounts
    pub destination_cell_context: Option<CellContext>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellScriptActionGeneratorPlan {
    pub target_profile: String,
    pub final_cellscript_compiled_scheduler_witness: Option<Vec<u8>>,
    pub ckb_type_id_output_indexes: Vec<usize>,
}

// impl std::fmt::Debug for GeneratorSettings {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         f.debug_struct("GeneratorSettings")
//             .field("network_id", &self.network_id)
//             // .field("multiplexer", &self.multiplexer)
//             .field("cell_iterator", &"Box<dyn Iterator<Item = CellEntryReference> + Send + Sync + 'static>")
//             // .field("source_cell_context", &self.source_cell_context)
//             .field("minimum_signatures", &self.minimum_signatures)
//             .field("change_address", &self.change_address)
//             .field("final_transaction_priority_fee", &self.final_transaction_priority_fee)
//             .field("final_transaction_destination", &self.final_transaction_destination)
//             .field("final_transaction_payload", &self.final_transaction_payload)
//             // .field("destination_cell_context", &self.destination_cell_context)
//             .finish()
//     }
// }

impl GeneratorSettings {
    pub fn try_new_with_account(
        account: Arc<dyn Account>,
        final_transaction_destination: PaymentDestination,
        _fee_rate: Option<f64>,
        final_priority_fee: Fees,
        final_transaction_payload: Option<Vec<u8>>,
    ) -> Result<Self> {
        let network_id = account.cell_context().processor().network_id()?;
        let change_address = account.change_address()?;
        let multiplexer = account.wallet().multiplexer().clone();
        let minimum_signatures = account.minimum_signatures();

        let cell_iterator = CellIterator::new(account.cell_context());

        let settings = GeneratorSettings {
            network_id,
            multiplexer: Some(multiplexer),
            minimum_signatures,
            change_address,
            cell_iterator: Box::new(cell_iterator),
            source_cell_context: Some(account.cell_context().clone()),
            priority_cell_entries: None,
            fee_rate: None,
            final_transaction_priority_fee: final_priority_fee,
            final_transaction_destination,
            final_transaction_payload,
            final_cellscript_compiled_scheduler_witness: None,
            cell_deps: Vec::new(),
            header_deps: Vec::new(),
            ckb_type_id_output_indexes: Vec::new(),
            destination_cell_context: None,
        };

        Ok(settings)
    }

    pub fn try_new_with_context(
        cell_context: CellContext,
        priority_cell_entries: Option<Vec<CellEntryReference>>,
        change_address: Address,
        minimum_signatures: u16,
        final_transaction_destination: PaymentDestination,
        final_priority_fee: Fees,
        final_transaction_payload: Option<Vec<u8>>,
        multiplexer: Option<Multiplexer<Box<Events>>>,
    ) -> Result<Self> {
        let network_id = cell_context.processor().network_id()?;
        let cell_iterator = CellIterator::new(&cell_context);

        let settings = GeneratorSettings {
            network_id,
            multiplexer,
            minimum_signatures,
            change_address,
            cell_iterator: Box::new(cell_iterator),
            source_cell_context: Some(cell_context),
            priority_cell_entries,
            fee_rate: None,
            final_transaction_priority_fee: final_priority_fee,
            final_transaction_destination,
            final_transaction_payload,
            final_cellscript_compiled_scheduler_witness: None,
            cell_deps: Vec::new(),
            header_deps: Vec::new(),
            ckb_type_id_output_indexes: Vec::new(),
            destination_cell_context: None,
        };

        Ok(settings)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn try_new_with_iterator(
        network_id: NetworkId,
        cell_iterator: Box<dyn Iterator<Item = CellEntryReference> + Send + Sync + 'static>,
        priority_cell_entries: Option<Vec<CellEntryReference>>,
        change_address: Address,
        minimum_signatures: u16,
        final_transaction_destination: PaymentDestination,
        _fee_rate: Option<f64>,
        final_priority_fee: Fees,
        final_transaction_payload: Option<Vec<u8>>,
        multiplexer: Option<Multiplexer<Box<Events>>>,
    ) -> Result<Self> {
        let settings = GeneratorSettings {
            network_id,
            multiplexer,
            minimum_signatures,
            change_address,
            cell_iterator: Box::new(cell_iterator),
            source_cell_context: None,
            priority_cell_entries,
            fee_rate: _fee_rate,
            final_transaction_priority_fee: final_priority_fee,
            final_transaction_destination,
            final_transaction_payload,
            final_cellscript_compiled_scheduler_witness: None,
            cell_deps: Vec::new(),
            header_deps: Vec::new(),
            ckb_type_id_output_indexes: Vec::new(),
            destination_cell_context: None,
        };

        Ok(settings)
    }

    /// Configure raw compiled CellScript scheduler witness bytes.
    ///
    /// This preserves the original infallible builder API for existing Rust
    /// callers. New public paths should prefer
    /// [`try_with_cellscript_compiled_scheduler_witness`] so malformed or legacy
    /// scheduler witness bytes fail before generator construction.
    pub fn with_cellscript_compiled_scheduler_witness(mut self, witness: Vec<u8>) -> Self {
        self.final_cellscript_compiled_scheduler_witness = Some(witness);
        self
    }

    /// Configure compiled CellScript scheduler witness bytes after validating
    /// that they use the public Molecule scheduler witness ABI.
    pub fn try_with_cellscript_compiled_scheduler_witness(mut self, witness: Vec<u8>) -> Result<Self> {
        validate_cellscript_compiled_scheduler_witness(&witness)?;
        self.final_cellscript_compiled_scheduler_witness = Some(witness);
        Ok(self)
    }

    /// Apply the transaction-builder side effects declared by CellScript action metadata.
    ///
    /// This is profile-aware: Spora action metadata contributes the public Molecule
    /// scheduler witness, while CKB action metadata contributes final-output indexes
    /// where the generator must install the built-in TYPE_ID type script. The method
    /// deliberately does not consume legacy Borsh scheduler fields.
    pub fn with_cellscript_action_metadata_json(mut self, metadata_json: &str, action_name: &str) -> Result<Self> {
        let plan = cellscript_action_generator_plan_from_metadata_json(metadata_json, action_name)?;
        if let Some(witness) = plan.final_cellscript_compiled_scheduler_witness {
            if let Some(existing) = &self.final_cellscript_compiled_scheduler_witness {
                if existing != &witness {
                    return Err(Error::custom("CellScript scheduler witness is already configured with different bytes"));
                }
            } else {
                self.final_cellscript_compiled_scheduler_witness = Some(witness);
            }
        }
        self.ckb_type_id_output_indexes.extend(plan.ckb_type_id_output_indexes);
        Ok(self)
    }

    pub fn with_cell_deps(mut self, cell_deps: Vec<CellDep>) -> Self {
        self.cell_deps = cell_deps;
        self
    }

    pub fn with_cell_dep(mut self, cell_dep: CellDep) -> Self {
        self.cell_deps.push(cell_dep);
        self
    }

    pub fn with_header_deps(mut self, header_deps: Vec<[u8; 32]>) -> Self {
        self.header_deps = header_deps;
        self
    }

    pub fn with_header_dep(mut self, header_dep: [u8; 32]) -> Self {
        self.header_deps.push(header_dep);
        self
    }

    /// Configure final transaction user outputs that should receive a CKB
    /// built-in TYPE_ID type script.
    ///
    /// Indexes refer to the user-supplied final outputs, before any generated
    /// change output is appended. The generator validates these indexes at
    /// construction and installs the concrete TYPE_ID scripts only on the final
    /// transaction; relay/change-only transactions remain unaffected.
    pub fn with_ckb_type_id_output_indexes(mut self, output_indexes: Vec<usize>) -> Self {
        self.ckb_type_id_output_indexes = output_indexes;
        self
    }

    /// Add one final transaction user output that should receive a CKB TYPE_ID
    /// type script.
    pub fn with_ckb_type_id_output_index(mut self, output_index: usize) -> Self {
        self.ckb_type_id_output_indexes.push(output_index);
        self
    }

    /// Add CKB TYPE_ID output indexes declared by CellScript metadata for an action.
    ///
    /// This consumes the metadata JSON shape emitted by CellScript schema v26:
    /// `actions[].create_set[].ckb_type_id.output_index`. Existing explicitly
    /// configured indexes are preserved; duplicate or out-of-range indexes are
    /// rejected when the generator is constructed.
    pub fn with_cellscript_ckb_type_id_action_metadata_json(mut self, metadata_json: &str, action_name: &str) -> Result<Self> {
        self.ckb_type_id_output_indexes.extend(ckb_type_id_output_indexes_from_cellscript_metadata_json(metadata_json, action_name)?);
        Ok(self)
    }

    /// Add a CKB `DepType::DepGroup` dependency to generated transactions.
    ///
    /// The referenced dep-group cell data must be encoded separately with
    /// `spora_exec::encode_ckb_dep_group_data`; this method only places the
    /// `CellDep` reference into the transaction.
    pub fn with_ckb_dep_group_cell_dep(self, dep_group_out_point: OutPoint) -> Self {
        self.with_cell_dep(CellDep { out_point: dep_group_out_point, dep_type: DepType::DepGroup })
    }

    /// Include the configured CKB default lock dependency in generated transactions.
    pub fn with_ckb_secp256k1_blake160_sighash_all_lock_dep(self, config: &CkbSecp256k1Blake160SighashAllLockConfig) -> Self {
        self.with_cell_dep(config.cell_dep())
    }

    pub fn cell_context_transfer(mut self, destination_cell_context: &CellContext) -> Self {
        self.destination_cell_context = Some(destination_cell_context.clone());
        self
    }
}

pub fn ckb_type_id_output_indexes_from_cellscript_metadata_json(metadata_json: &str, action_name: &str) -> Result<Vec<usize>> {
    let metadata: serde_json::Value = serde_json::from_str(metadata_json)?;
    let target_profile = cellscript_metadata_target_profile(&metadata)?;
    if target_profile != CELLSCRIPT_TARGET_PROFILE_CKB {
        return Err(Error::custom(format!(
            "CellScript CKB TYPE_ID output plans require target_profile.name 'ckb', got '{target_profile}'"
        )));
    }
    let action = cellscript_metadata_action(&metadata, action_name)?;
    ckb_type_id_output_indexes_from_cellscript_action(action, action_name)
}

pub fn cellscript_action_generator_plan_from_metadata_json(
    metadata_json: &str,
    action_name: &str,
) -> Result<CellScriptActionGeneratorPlan> {
    let metadata: serde_json::Value = serde_json::from_str(metadata_json)?;
    let target_profile = cellscript_metadata_target_profile(&metadata)?;
    let action = cellscript_metadata_action(&metadata, action_name)?;

    let mut plan = CellScriptActionGeneratorPlan {
        target_profile: target_profile.to_string(),
        final_cellscript_compiled_scheduler_witness: None,
        ckb_type_id_output_indexes: Vec::new(),
    };

    match target_profile {
        CELLSCRIPT_TARGET_PROFILE_TYPED_CELL | CELLSCRIPT_TARGET_PROFILE_SPORA => {
            plan.final_cellscript_compiled_scheduler_witness = cellscript_spora_scheduler_witness_from_action(action, action_name)?;
        }
        CELLSCRIPT_TARGET_PROFILE_CKB => {
            plan.ckb_type_id_output_indexes = ckb_type_id_output_indexes_from_cellscript_action(action, action_name)?;
        }
        CELLSCRIPT_TARGET_PROFILE_PORTABLE_CELL => {}
        other => {
            return Err(Error::custom(format!("unsupported CellScript target_profile.name '{other}' in action generator metadata")));
        }
    }

    Ok(plan)
}

fn cellscript_metadata_target_profile(metadata: &serde_json::Value) -> Result<&str> {
    metadata
        .get("target_profile")
        .and_then(|profile| profile.get("name"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| Error::custom("CellScript metadata target_profile.name is missing"))
}

fn cellscript_metadata_action<'a>(metadata: &'a serde_json::Value, action_name: &str) -> Result<&'a serde_json::Value> {
    let actions = metadata
        .get("actions")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| Error::custom("CellScript metadata actions must be an array"))?;
    actions
        .iter()
        .find(|action| action.get("name").and_then(serde_json::Value::as_str) == Some(action_name))
        .ok_or_else(|| Error::custom(format!("CellScript metadata action '{action_name}' was not found")))
}

fn ckb_type_id_output_indexes_from_cellscript_action(action: &serde_json::Value, action_name: &str) -> Result<Vec<usize>> {
    let create_set = action
        .get("create_set")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| Error::custom(format!("CellScript metadata action '{action_name}' create_set must be an array")))?;

    let mut output_indexes = Vec::new();
    for (index, create) in create_set.iter().enumerate() {
        let Some(plan) = create.get("ckb_type_id") else {
            continue;
        };
        if plan.is_null() {
            continue;
        }
        let output_index = plan.get("output_index").and_then(serde_json::Value::as_u64).ok_or_else(|| {
            Error::custom(format!(
                "CellScript metadata action '{action_name}' create_set[{index}].ckb_type_id.output_index must be a non-negative integer"
            ))
        })?;
        let output_index = usize::try_from(output_index).map_err(|_| {
            Error::custom(format!(
                "CellScript metadata action '{action_name}' create_set[{index}].ckb_type_id.output_index is too large"
            ))
        })?;
        output_indexes.push(output_index);
    }

    Ok(output_indexes)
}

fn cellscript_spora_scheduler_witness_from_action(action: &serde_json::Value, action_name: &str) -> Result<Option<Vec<u8>>> {
    if cellscript_action_non_empty_string_field(action, CELLSCRIPT_SCHEDULER_WITNESS_BORSH_HEX_FIELD).is_some() {
        return Err(Error::custom(format!(
            "CellScript metadata action '{action_name}' legacy scheduler_witness_borsh_hex is not public scheduler witness metadata; use explicit migration tooling"
        )));
    }
    let Some((field, witness_hex)) = cellscript_spora_scheduler_witness_hex(action, action_name)? else {
        return Ok(None);
    };
    if field == CELLSCRIPT_SCHEDULER_WITNESS_HEX_FIELD {
        let abi = action
            .get("scheduler_witness_abi")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| Error::custom(format!("CellScript metadata action '{action_name}' scheduler_witness_abi is missing")))?;
        if abi != CELLSCRIPT_SCHEDULER_WITNESS_ABI_MOLECULE {
            return Err(Error::custom(format!(
                "CellScript metadata action '{action_name}' scheduler_witness_abi '{abi}' is unsupported; expected molecule"
            )));
        }
    }
    let witness = decode_cellscript_metadata_hex(witness_hex, field)
        .map_err(|err| Error::custom(format!("CellScript metadata action '{action_name}' has invalid {field}: {err}")))?;
    decode_cellscript_scheduler_witness(&witness).map_err(|err| {
        Error::custom(format!("CellScript metadata action '{action_name}' {field} is not a valid Molecule scheduler witness: {err}"))
    })?;
    Ok(Some(witness))
}

fn validate_cellscript_compiled_scheduler_witness(witness: &[u8]) -> Result<()> {
    decode_cellscript_scheduler_witness(witness).map(|_| ()).map_err(|err| {
        Error::custom(format!("CellScript compiled scheduler witness is not a valid Molecule scheduler witness: {err}"))
    })
}

fn cellscript_spora_scheduler_witness_hex<'a>(
    action: &'a serde_json::Value,
    action_name: &str,
) -> Result<Option<(&'static str, &'a str)>> {
    let primary = cellscript_action_non_empty_string_field(action, CELLSCRIPT_SCHEDULER_WITNESS_HEX_FIELD);
    let legacy_molecule = cellscript_action_non_empty_string_field(action, CELLSCRIPT_SCHEDULER_WITNESS_MOLECULE_HEX_FIELD);
    if let (Some(primary), Some(legacy_molecule)) = (primary, legacy_molecule) {
        if primary != legacy_molecule {
            return Err(Error::custom(format!(
                "CellScript metadata action '{action_name}' has conflicting scheduler_witness_hex and scheduler_witness_molecule_hex"
            )));
        }
    }
    if let Some(witness_hex) = primary {
        return Ok(Some((CELLSCRIPT_SCHEDULER_WITNESS_HEX_FIELD, witness_hex)));
    }
    Ok(legacy_molecule.map(|witness_hex| (CELLSCRIPT_SCHEDULER_WITNESS_MOLECULE_HEX_FIELD, witness_hex)))
}

fn cellscript_action_non_empty_string_field<'a>(action: &'a serde_json::Value, field: &str) -> Option<&'a str> {
    action.get(field).and_then(serde_json::Value::as_str).filter(|value| !value.is_empty())
}

fn decode_cellscript_metadata_hex(hex: &str, field: &str) -> Result<Vec<u8>> {
    if hex.len() % 2 != 0 {
        return Err(Error::custom(format!("{field} must contain full bytes")));
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for index in (0..hex.len()).step_by(2) {
        let byte = u8::from_str_radix(&hex[index..index + 2], 16)
            .map_err(|error| Error::custom(format!("{field} has invalid hex byte at offset {index}: {error}")))?;
        bytes.push(byte);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::{
        cellscript_action_generator_plan_from_metadata_json, ckb_type_id_output_indexes_from_cellscript_metadata_json,
        GeneratorSettings,
    };
    use crate::imports::{NetworkId, NetworkType};
    use crate::tx::{Fees, PaymentDestination};
    use spora_addresses::{Address, Prefix};
    use spora_exec::celltx::{
        encode_cellscript_scheduler_witness_molecule, CellScriptSchedulerWitness, CELLSCRIPT_SCHEDULER_EFFECT_CREATING,
        CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
    };

    fn valid_spora_scheduler_witness_bytes() -> Vec<u8> {
        valid_spora_scheduler_witness_bytes_with_cycles(64)
    }

    fn valid_spora_scheduler_witness_bytes_with_cycles(estimated_cycles: u64) -> Vec<u8> {
        encode_cellscript_scheduler_witness_molecule(&CellScriptSchedulerWitness {
            magic: 0xCE11,
            version: CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
            effect_class: CELLSCRIPT_SCHEDULER_EFFECT_CREATING,
            parallelizable: false,
            estimated_cycles,
            access_count: 0,
            accesses: vec![],
        })
    }

    fn bytes_to_hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn extracts_ckb_type_id_output_indexes_from_cellscript_metadata_json() {
        let metadata = r#"
{
  "target_profile": { "name": "ckb" },
  "actions": [
    {
      "name": "mint",
      "create_set": [
        { "operation": "create", "ty": "Token", "ckb_type_id": { "output_index": 0 } },
        { "operation": "create", "ty": "PlainToken" },
        { "operation": "create", "ty": "Nft", "ckb_type_id": { "output_index": 2 } }
      ]
    }
  ]
}
"#;

        let indexes = ckb_type_id_output_indexes_from_cellscript_metadata_json(metadata, "mint").unwrap();

        assert_eq!(indexes, vec![0, 2]);
    }

    #[test]
    fn rejects_non_ckb_cellscript_metadata_for_ckb_type_id_output_indexes() {
        let metadata = r#"{ "target_profile": { "name": "spora" }, "actions": [] }"#;

        let err = ckb_type_id_output_indexes_from_cellscript_metadata_json(metadata, "mint").unwrap_err();

        assert!(err.to_string().contains("target_profile.name 'ckb'"), "unexpected error: {err}");
    }

    #[test]
    fn profile_selected_cellscript_action_plan_extracts_ckb_type_id_outputs() {
        let metadata = r#"
{
  "target_profile": { "name": "ckb" },
  "actions": [
    {
      "name": "mint",
      "scheduler_witness_abi": "molecule",
      "scheduler_witness_hex": "ff",
      "create_set": [
        { "operation": "create", "ty": "Token", "ckb_type_id": { "output_index": 0 } }
      ]
    }
  ]
}
"#;

        let plan = cellscript_action_generator_plan_from_metadata_json(metadata, "mint").unwrap();

        assert_eq!(plan.target_profile, "ckb");
        assert_eq!(plan.ckb_type_id_output_indexes, vec![0]);
        assert!(plan.final_cellscript_compiled_scheduler_witness.is_none());
    }

    #[test]
    fn profile_selected_cellscript_action_plan_extracts_typed_cell_molecule_scheduler_witness() {
        let witness = valid_spora_scheduler_witness_bytes();
        let witness_hex = bytes_to_hex(&witness);
        let metadata = r#"
{
  "target_profile": { "name": "typed-cell" },
  "actions": [
    {
      "name": "mint",
      "scheduler_witness_abi": "molecule",
      "scheduler_witness_hex": "__WITNESS_HEX__",
      "create_set": [
        { "operation": "create", "ty": "Token", "ckb_type_id": { "output_index": 0 } }
      ]
    }
  ]
}
"#
        .replace("__WITNESS_HEX__", &witness_hex);

        let plan = cellscript_action_generator_plan_from_metadata_json(&metadata, "mint").unwrap();

        assert_eq!(plan.target_profile, "typed-cell");
        assert_eq!(plan.final_cellscript_compiled_scheduler_witness, Some(witness));
        assert!(plan.ckb_type_id_output_indexes.is_empty());
    }

    #[test]
    fn profile_selected_cellscript_action_plan_extracts_legacy_spora_molecule_scheduler_witness_alias() {
        let witness = valid_spora_scheduler_witness_bytes();
        let witness_hex = bytes_to_hex(&witness);
        let metadata = r#"
{
  "target_profile": { "name": "spora" },
  "actions": [
    {
      "name": "mint",
      "scheduler_witness_molecule_hex": "__WITNESS_HEX__",
      "create_set": []
    }
  ]
}
"#
        .replace("__WITNESS_HEX__", &witness_hex);

        let plan = cellscript_action_generator_plan_from_metadata_json(&metadata, "mint").unwrap();

        assert_eq!(plan.target_profile, "spora");
        assert_eq!(plan.final_cellscript_compiled_scheduler_witness, Some(witness));
        assert!(plan.ckb_type_id_output_indexes.is_empty());
    }

    #[test]
    fn profile_selected_cellscript_action_plan_rejects_conflicting_spora_molecule_scheduler_witness_alias() {
        let witness_hex = bytes_to_hex(&valid_spora_scheduler_witness_bytes_with_cycles(64));
        let alias_hex = bytes_to_hex(&valid_spora_scheduler_witness_bytes_with_cycles(65));
        let metadata = r#"
{
  "target_profile": { "name": "spora" },
  "actions": [
    {
      "name": "mint",
      "scheduler_witness_abi": "molecule",
      "scheduler_witness_hex": "__WITNESS_HEX__",
      "scheduler_witness_molecule_hex": "__ALIAS_HEX__",
      "create_set": []
    }
  ]
}
"#
        .replace("__WITNESS_HEX__", &witness_hex)
        .replace("__ALIAS_HEX__", &alias_hex);

        let err = cellscript_action_generator_plan_from_metadata_json(&metadata, "mint").unwrap_err();

        assert!(err.to_string().contains("conflicting scheduler_witness_hex"), "unexpected error: {err}");
    }

    #[test]
    fn profile_selected_cellscript_action_plan_rejects_non_molecule_spora_scheduler_witness() {
        let metadata = r#"
{
  "target_profile": { "name": "spora" },
  "actions": [
    {
      "name": "mint",
      "scheduler_witness_abi": "borsh",
      "scheduler_witness_hex": "0001",
      "create_set": []
    }
  ]
}
"#;

        let err = cellscript_action_generator_plan_from_metadata_json(metadata, "mint").unwrap_err();

        assert!(err.to_string().contains("unsupported; expected molecule"), "unexpected error: {err}");
    }

    #[test]
    fn profile_selected_cellscript_action_plan_rejects_legacy_borsh_spora_scheduler_witness_field() {
        let metadata = r#"
{
  "target_profile": { "name": "spora" },
  "actions": [
    {
      "name": "mint",
      "scheduler_witness_borsh_hex": "11ce01",
      "create_set": []
    }
  ]
}
"#;

        let err = cellscript_action_generator_plan_from_metadata_json(metadata, "mint").unwrap_err();

        assert!(err.to_string().contains("scheduler_witness_borsh_hex is not public"), "unexpected error: {err}");
    }

    #[test]
    fn profile_selected_cellscript_action_plan_rejects_invalid_spora_molecule_scheduler_witness() {
        let metadata = r#"
{
  "target_profile": { "name": "spora" },
  "actions": [
    {
      "name": "mint",
      "scheduler_witness_abi": "molecule",
      "scheduler_witness_hex": "0001ff",
      "create_set": []
    }
  ]
}
"#;

        let err = cellscript_action_generator_plan_from_metadata_json(metadata, "mint").unwrap_err();

        assert!(err.to_string().contains("not a valid Molecule scheduler witness"), "unexpected error: {err}");
    }

    #[test]
    fn try_with_cellscript_compiled_scheduler_witness_rejects_non_molecule_bytes() {
        let change_address = Address::new_std_single(Prefix::Testnet, &[0x21; 32]).unwrap();
        let settings = GeneratorSettings::try_new_with_iterator(
            NetworkId::with_suffix(NetworkType::Testnet, 10),
            Box::new(std::iter::empty()),
            None,
            change_address,
            1,
            PaymentDestination::Change,
            None,
            Fees::None,
            None,
            None,
        )
        .unwrap();

        let err = match settings.try_with_cellscript_compiled_scheduler_witness(vec![0x11, 0xce, 0x01]) {
            Ok(_) => panic!("expected invalid scheduler witness to be rejected"),
            Err(err) => err,
        };

        assert!(err.to_string().contains("not a valid Molecule scheduler witness"), "unexpected error: {err}");
    }

    #[test]
    fn profile_selected_cellscript_action_metadata_configures_generator_settings() {
        let change_address = Address::new_std_single(Prefix::Testnet, &[0x21; 32]).unwrap();
        let base_settings = || {
            GeneratorSettings::try_new_with_iterator(
                NetworkId::with_suffix(NetworkType::Testnet, 10),
                Box::new(std::iter::empty()),
                None,
                change_address.clone(),
                1,
                PaymentDestination::Change,
                None,
                Fees::None,
                None,
                None,
            )
            .unwrap()
        };
        let witness = valid_spora_scheduler_witness_bytes();
        let witness_hex = bytes_to_hex(&witness);
        let typed_cell_metadata = r#"
{
  "target_profile": { "name": "typed-cell" },
  "actions": [
    {
      "name": "mint",
      "scheduler_witness_abi": "molecule",
      "scheduler_witness_hex": "__WITNESS_HEX__",
      "create_set": []
    }
  ]
}
"#
        .replace("__WITNESS_HEX__", &witness_hex);
        let ckb_metadata = r#"
{
  "target_profile": { "name": "ckb" },
  "actions": [
    {
      "name": "mint",
      "create_set": [
        { "operation": "create", "ty": "Token", "ckb_type_id": { "output_index": 0 } }
      ]
    }
  ]
}
"#;

        let typed_cell_settings = base_settings().with_cellscript_action_metadata_json(&typed_cell_metadata, "mint").unwrap();
        let ckb_settings = base_settings().with_cellscript_action_metadata_json(ckb_metadata, "mint").unwrap();

        assert_eq!(typed_cell_settings.final_cellscript_compiled_scheduler_witness, Some(witness));
        assert!(typed_cell_settings.ckb_type_id_output_indexes.is_empty());
        assert_eq!(ckb_settings.ckb_type_id_output_indexes, vec![0]);
        assert!(ckb_settings.final_cellscript_compiled_scheduler_witness.is_none());
    }
}
