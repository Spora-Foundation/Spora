use camino::Utf8PathBuf;
use cellscript::{
    compile, compile_file, compile_file_with_entry_action, ActionMetadata, ArtifactFormat, CompileOptions, CompileResult,
    EntryWitnessArg,
};

pub const BUNDLED_CELLSCRIPT_EXAMPLES: [&str; 7] =
    ["amm_pool.cell", "launch.cell", "multisig.cell", "nft.cell", "timelock.cell", "token.cell", "vesting.cell"];
const CELLSCRIPT_TARGET_PROFILE_TYPED_CELL: &str = "typed-cell";

pub struct CompiledCellScriptContract {
    pub artifact_bytes: Vec<u8>,
    pub code_hash: [u8; 32],
}

pub struct CompiledParameterizedAmountContract {
    pub artifact_bytes: Vec<u8>,
    pub code_hash: [u8; 32],
    main_action: ActionMetadata,
}

impl CompiledParameterizedAmountContract {
    pub fn entry_witness_for_amount(&self, amount: u64) -> Vec<u8> {
        self.main_action.entry_witness_args(&[EntryWitnessArg::U64(amount)]).expect("parameterized amount entry witness must encode")
    }
}

pub struct CompiledCellScriptExample {
    pub name: &'static str,
    pub artifact_bytes: Vec<u8>,
    pub code_hash: [u8; 32],
    pub ckb_runtime_required: bool,
    pub action_artifacts: Vec<CompiledCellScriptActionArtifact>,
    pub action_names: Vec<String>,
    pub estimated_compute_mass: u64,
    pub estimated_storage_mass: u64,
    pub estimated_transient_mass: u64,
    pub estimated_code_deployment_mass: u64,
    pub requires_relaxed_mass_policy: bool,
}

pub struct CompiledCellScriptActionArtifact {
    pub name: String,
    pub artifact_bytes: Vec<u8>,
    pub code_hash: [u8; 32],
    pub action: ActionMetadata,
}

struct TypedCellMassEstimate {
    estimated_compute_mass: u64,
    estimated_storage_mass: u64,
    estimated_transient_mass: u64,
    estimated_code_deployment_mass: u64,
    requires_relaxed_mass_policy: bool,
}

fn spora_code_hash(artifact_bytes: &[u8]) -> [u8; 32] {
    *blake3::hash(artifact_bytes).as_bytes()
}

fn assert_typed_cell_spora_elf(result: &CompileResult, context: &str) -> [u8; 32] {
    assert_eq!(result.artifact_format, ArtifactFormat::RiscvElf, "{context} must compile to an ELF artifact");
    assert!(result.artifact_bytes.starts_with(b"\x7fELF"), "{context} artifact must start with the ELF magic");
    assert!(result.metadata.runtime.vm_abi.embedded_in_artifact, "{context} must embed the Molecule VM ABI");
    assert_eq!(result.metadata.target_profile.name, CELLSCRIPT_TARGET_PROFILE_TYPED_CELL, "{context} target profile");
    spora_code_hash(&result.artifact_bytes)
}

fn typed_cell_mass_estimate(result: &CompileResult) -> TypedCellMassEstimate {
    let estimated_compute_mass = result.metadata.actions.iter().map(|action| action.estimated_cycles).max().unwrap_or(0);
    let estimated_storage_mass = result.artifact_bytes.len() as u64;
    TypedCellMassEstimate {
        estimated_compute_mass,
        estimated_storage_mass,
        estimated_transient_mass: 0,
        estimated_code_deployment_mass: estimated_storage_mass,
        requires_relaxed_mass_policy: false,
    }
}

fn bundled_example_source_path(examples_dir: &camino::Utf8Path, name: &str) -> Utf8PathBuf {
    let package_name = name.strip_suffix(".cell").unwrap_or(name);
    let package_main = examples_dir.join(package_name).join("src/main.cell");
    if package_main.exists() {
        package_main
    } else {
        examples_dir.join(name)
    }
}

pub fn compile_all_spora_full_example_contracts() -> Vec<CompiledCellScriptExample> {
    let examples_dir = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../cellscript/examples");
    BUNDLED_CELLSCRIPT_EXAMPLES
        .into_iter()
        .map(|name| {
            let source_path = bundled_example_source_path(&examples_dir, name);
            let local_action_names = source_declared_action_names(&source_path)
                .unwrap_or_else(|err| panic!("{name} source-local action names must be parsed: {err}"));
            let result = compile_file(
                &source_path,
                CompileOptions {
                    opt_level: 3,
                    target: Some("riscv64-elf".to_string()),
                    target_profile: Some(CELLSCRIPT_TARGET_PROFILE_TYPED_CELL.to_string()),
                    ..CompileOptions::default()
                },
            )
            .unwrap_or_else(|err| panic!("{name} must compile to Spora ELF: {err}"));

            let code_hash = assert_typed_cell_spora_elf(&result, name);
            let mass = typed_cell_mass_estimate(&result);
            assert!(!result.metadata.actions.is_empty(), "{name} must expose action metadata");

            CompiledCellScriptExample {
                name,
                artifact_bytes: result.artifact_bytes,
                code_hash,
                ckb_runtime_required: result.metadata.runtime.ckb_runtime_required,
                action_artifacts: Vec::new(),
                action_names: local_action_names,
                estimated_compute_mass: mass.estimated_compute_mass,
                estimated_storage_mass: mass.estimated_storage_mass,
                estimated_transient_mass: mass.estimated_transient_mass,
                estimated_code_deployment_mass: mass.estimated_code_deployment_mass,
                requires_relaxed_mass_policy: mass.requires_relaxed_mass_policy,
            }
        })
        .collect()
}

pub fn compile_noop_spora_lock_contract() -> CompiledCellScriptContract {
    let source = r#"
module acceptance::noop_lock

action main() -> u64 {
    verification
        0
}
"#;

    let result = compile(
        source,
        CompileOptions {
            opt_level: 3,
            target: Some("riscv64-elf".to_string()),
            target_profile: Some(CELLSCRIPT_TARGET_PROFILE_TYPED_CELL.to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("CellScript no-op contract must compile to Spora ELF");

    let code_hash = assert_typed_cell_spora_elf(&result, "CellScript no-op contract");
    assert!(result.metadata.runtime.standalone_runner_compatible);
    assert!(!result.metadata.runtime.ckb_runtime_required);
    assert!(result.metadata.runtime.fail_closed_runtime_features.is_empty());

    CompiledCellScriptContract { artifact_bytes: result.artifact_bytes, code_hash }
}

pub fn compile_fixed_output_spora_contract() -> CompiledCellScriptContract {
    let source = r#"
module acceptance::fixed_output

resource Marker has store {
    amount: u64
}

action main() -> marker: Marker {
    verification
        create marker = Marker {
            amount: 42
        }
}
"#;

    let result = compile(
        source,
        CompileOptions {
            opt_level: 3,
            target: Some("riscv64-elf".to_string()),
            target_profile: Some(CELLSCRIPT_TARGET_PROFILE_TYPED_CELL.to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("CellScript fixed-output contract must compile to Spora ELF");

    let code_hash = assert_typed_cell_spora_elf(&result, "CellScript fixed-output contract");
    assert!(result.metadata.runtime.ckb_runtime_required);
    assert!(!result.metadata.runtime.standalone_runner_compatible);
    assert!(result.metadata.runtime.fail_closed_runtime_features.is_empty());

    let main = result.metadata.actions.iter().find(|action| action.name == "main").expect("main action metadata");
    assert!(main.elf_compatible);
    assert!(main.fail_closed_runtime_features.is_empty());
    assert!(main
        .create_set
        .iter()
        .any(|pattern| { pattern.operation == "output" && pattern.ty == "Marker" && pattern.binding == "marker" }));
    let create_obligation = main
        .verifier_obligations
        .iter()
        .find(|obligation| obligation.category == "transaction-invariant" && obligation.feature.starts_with("create-output:Marker:"))
        .unwrap_or_else(|| panic!("missing Marker create-output verifier obligation: {:?}", main.verifier_obligations));
    assert_eq!(create_obligation.status, "checked-runtime");

    CompiledCellScriptContract { artifact_bytes: result.artifact_bytes, code_hash }
}

pub fn compile_parameterized_amount_spora_contract() -> CompiledParameterizedAmountContract {
    let source = r#"
module acceptance::parameterized_amount

action main(amount: u64) -> u64 {
    verification
        if amount == 77 {
            return 0
        }
        41
}
"#;

    let result = compile(
        source,
        CompileOptions {
            opt_level: 3,
            target: Some("riscv64-elf".to_string()),
            target_profile: Some(CELLSCRIPT_TARGET_PROFILE_TYPED_CELL.to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("CellScript parameterized amount contract must compile to Spora ELF");

    let code_hash = assert_typed_cell_spora_elf(&result, "CellScript parameterized amount contract");
    assert!(!result.metadata.runtime.ckb_runtime_required);
    assert!(result.metadata.runtime.fail_closed_runtime_features.is_empty());

    let main = result.metadata.actions.iter().find(|action| action.name == "main").cloned().expect("main action metadata");
    assert!(main.elf_compatible);
    assert!(main.fail_closed_runtime_features.is_empty());
    assert!(main.verifier_obligations.is_empty());

    CompiledParameterizedAmountContract { artifact_bytes: result.artifact_bytes, code_hash, main_action: main }
}

pub fn compile_all_spora_example_contracts() -> Vec<CompiledCellScriptExample> {
    let examples_dir = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../cellscript/examples");
    BUNDLED_CELLSCRIPT_EXAMPLES
        .into_iter()
        .map(|name| {
            let source_path = bundled_example_source_path(&examples_dir, name);
            let local_action_names = source_declared_action_names(&source_path)
                .unwrap_or_else(|err| panic!("{name} source-local action names must be parsed: {err}"));
            let result = compile_file(
                &source_path,
                CompileOptions {
                    opt_level: 3,
                    target: Some("riscv64-elf".to_string()),
                    target_profile: Some(CELLSCRIPT_TARGET_PROFILE_TYPED_CELL.to_string()),
                    ..CompileOptions::default()
                },
            )
            .unwrap_or_else(|err| panic!("{name} must compile to Spora ELF: {err}"));

            let code_hash = assert_typed_cell_spora_elf(&result, name);
            let mass = typed_cell_mass_estimate(&result);
            assert!(!result.metadata.actions.is_empty(), "{name} must expose action metadata");
            let mut action_artifacts = Vec::new();
            for action_name in &local_action_names {
                let scoped = compile_file_with_entry_action(
                    &source_path,
                    CompileOptions {
                        opt_level: 3,
                        target: Some("riscv64-elf".to_string()),
                        target_profile: Some(CELLSCRIPT_TARGET_PROFILE_TYPED_CELL.to_string()),
                        ..CompileOptions::default()
                    },
                    action_name,
                )
                .unwrap_or_else(|err| panic!("{name}::{action_name} must compile to scoped Spora ELF: {err}"));
                let scoped_code_hash = assert_typed_cell_spora_elf(&scoped, &format!("{name}::{action_name}"));
                let scoped_action = scoped
                    .metadata
                    .actions
                    .iter()
                    .find(|action| &action.name == action_name)
                    .unwrap_or_else(|| panic!("{name}::{action_name} scoped metadata must contain the selected action"))
                    .clone();
                action_artifacts.push(CompiledCellScriptActionArtifact {
                    name: action_name.clone(),
                    artifact_bytes: scoped.artifact_bytes,
                    code_hash: scoped_code_hash,
                    action: scoped_action,
                });
            }

            CompiledCellScriptExample {
                name,
                artifact_bytes: result.artifact_bytes,
                code_hash,
                ckb_runtime_required: result.metadata.runtime.ckb_runtime_required,
                action_artifacts,
                action_names: local_action_names,
                estimated_compute_mass: mass.estimated_compute_mass,
                estimated_storage_mass: mass.estimated_storage_mass,
                estimated_transient_mass: mass.estimated_transient_mass,
                estimated_code_deployment_mass: mass.estimated_code_deployment_mass,
                requires_relaxed_mass_policy: mass.requires_relaxed_mass_policy,
            }
        })
        .collect()
}

fn source_declared_action_names(path: &camino::Utf8Path) -> Result<Vec<String>, std::io::Error> {
    let source = std::fs::read_to_string(path)?;
    let mut names = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("action ") else {
            continue;
        };
        let name = rest.split(|ch: char| !(ch == '_' || ch.is_ascii_alphanumeric())).next().unwrap_or_default();
        if !name.is_empty() {
            names.push(name.to_string());
        }
    }
    Ok(names)
}

pub fn compile_token_spora_example_contract() -> CompiledCellScriptContract {
    let source = include_str!("../../../../cellscript/examples/token.cell");
    let result = compile(
        source,
        CompileOptions {
            opt_level: 3,
            target: Some("riscv64-elf".to_string()),
            target_profile: Some(CELLSCRIPT_TARGET_PROFILE_TYPED_CELL.to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("bundled token.cell example must compile to Spora ELF");

    let code_hash = assert_typed_cell_spora_elf(&result, "bundled token.cell example");
    assert!(result.metadata.runtime.ckb_runtime_required);
    assert!(!result.metadata.runtime.standalone_runner_compatible);
    assert!(result.metadata.runtime.fail_closed_runtime_features.is_empty());

    let transfer = result
        .metadata
        .actions
        .iter()
        .find(|action| action.name == "transfer_token")
        .expect("token.cell must expose transfer_token action metadata");
    assert!(transfer.elf_compatible);
    assert!(transfer.fail_closed_runtime_features.is_empty());
    assert!(transfer.ckb_runtime_features.iter().any(|feature| feature == "consume-input-cell"));
    assert!(transfer.ckb_runtime_features.iter().any(|feature| feature == "verify-output-cell"));
    assert!(transfer.verifier_obligations.iter().any(|obligation| {
        obligation.category == "transaction-invariant"
            && obligation.feature == "resource-conservation:Token"
            && obligation.status == "checked-runtime"
    }));

    CompiledCellScriptContract { artifact_bytes: result.artifact_bytes, code_hash }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_spora_example_compile_metadata_acceptance() {
        let contract = compile_token_spora_example_contract();

        assert!(!contract.artifact_bytes.is_empty());
        assert_ne!(contract.code_hash, [0; 32]);
    }

    #[test]
    fn fixed_output_spora_contract_compile_metadata_acceptance() {
        let contract = compile_fixed_output_spora_contract();

        assert!(!contract.artifact_bytes.is_empty());
        assert_ne!(contract.code_hash, [0; 32]);
    }

    #[test]
    fn parameterized_amount_spora_contract_compile_metadata_acceptance() {
        let contract = compile_parameterized_amount_spora_contract();

        assert!(!contract.artifact_bytes.is_empty());
        assert_ne!(contract.code_hash, [0; 32]);
        let mut expected_witness = b"CSARGv1\0".to_vec();
        expected_witness.extend_from_slice(&77u64.to_le_bytes());
        assert_eq!(contract.entry_witness_for_amount(77), expected_witness);
    }

    #[test]
    fn all_spora_examples_compile_metadata_acceptance() {
        let contracts = compile_all_spora_example_contracts();

        assert_eq!(contracts.len(), BUNDLED_CELLSCRIPT_EXAMPLES.len());
        assert!(contracts.iter().all(|contract| !contract.artifact_bytes.is_empty()));
        assert!(contracts.iter().all(|contract| contract.code_hash != [0; 32]));
    }
}
