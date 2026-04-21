use camino::Utf8PathBuf;
use cellscript::{compile, compile_file, ActionMetadata, ArtifactFormat, CompileOptions, EntryWitnessArg};

pub const BUNDLED_CELLSCRIPT_EXAMPLES: [&str; 7] =
    ["amm_pool.cell", "launch.cell", "multisig.cell", "nft.cell", "timelock.cell", "token.cell", "vesting.cell"];

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
}

pub fn compile_noop_spora_lock_contract() -> CompiledCellScriptContract {
    let source = r#"
module acceptance::noop_lock

action main() -> u64 {
    return 0
}
"#;

    let result = compile(
        source,
        CompileOptions {
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("spora".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("CellScript no-op contract must compile to Spora ELF");

    assert_eq!(result.artifact_format, ArtifactFormat::RiscvElf);
    assert!(result.artifact_bytes.starts_with(b"\x7fELF"));
    assert!(result.metadata.runtime.vm_abi.embedded_in_artifact);
    assert!(result.metadata.runtime.standalone_runner_compatible);
    assert!(!result.metadata.runtime.ckb_runtime_required);
    assert!(result.metadata.runtime.fail_closed_runtime_features.is_empty());
    assert_eq!(result.artifact_hash, *blake3::hash(&result.artifact_bytes).as_bytes());

    CompiledCellScriptContract { artifact_bytes: result.artifact_bytes, code_hash: result.artifact_hash }
}

pub fn compile_fixed_output_spora_contract() -> CompiledCellScriptContract {
    let source = r#"
module acceptance::fixed_output

resource Marker has store {
    amount: u64
}

action main() -> Marker {
    return create Marker {
        amount: 42
    }
}
"#;

    let result = compile(
        source,
        CompileOptions {
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("spora".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("CellScript fixed-output contract must compile to Spora ELF");

    assert_eq!(result.artifact_format, ArtifactFormat::RiscvElf);
    assert!(result.artifact_bytes.starts_with(b"\x7fELF"));
    assert!(result.metadata.runtime.vm_abi.embedded_in_artifact);
    assert!(result.metadata.runtime.ckb_runtime_required);
    assert!(!result.metadata.runtime.standalone_runner_compatible);
    assert!(result.metadata.runtime.fail_closed_runtime_features.is_empty());
    assert_eq!(result.artifact_hash, *blake3::hash(&result.artifact_bytes).as_bytes());

    let main = result.metadata.actions.iter().find(|action| action.name == "main").expect("main action metadata");
    assert!(main.elf_compatible);
    assert!(main.fail_closed_runtime_features.is_empty());
    assert!(main.verifier_obligations.iter().any(|obligation| {
        obligation.category == "transaction-invariant"
            && obligation.feature == "create-output:Marker:create_Marker"
            && obligation.status == "checked-runtime"
    }));

    CompiledCellScriptContract { artifact_bytes: result.artifact_bytes, code_hash: result.artifact_hash }
}

pub fn compile_parameterized_amount_spora_contract() -> CompiledParameterizedAmountContract {
    let source = r#"
module acceptance::parameterized_amount

resource Marker has store {
    amount: u64
}

action main(amount: u64) -> Marker {
    return create Marker {
        amount: amount
    }
}
"#;

    let result = compile(
        source,
        CompileOptions {
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("spora".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("CellScript parameterized amount contract must compile to Spora ELF");

    assert_eq!(result.artifact_format, ArtifactFormat::RiscvElf);
    assert!(result.artifact_bytes.starts_with(b"\x7fELF"));
    assert!(result.metadata.runtime.vm_abi.embedded_in_artifact);
    assert!(result.metadata.runtime.ckb_runtime_required);
    assert!(!result.metadata.runtime.standalone_runner_compatible);
    assert!(result.metadata.runtime.fail_closed_runtime_features.is_empty());
    assert_eq!(result.artifact_hash, *blake3::hash(&result.artifact_bytes).as_bytes());

    let main = result.metadata.actions.iter().find(|action| action.name == "main").cloned().expect("main action metadata");
    assert!(main.elf_compatible);
    assert!(main.fail_closed_runtime_features.is_empty());
    assert!(main.verifier_obligations.iter().any(|obligation| {
        obligation.category == "transaction-invariant"
            && obligation.feature == "create-output:Marker:create_Marker"
            && obligation.status == "checked-runtime"
    }));

    CompiledParameterizedAmountContract { artifact_bytes: result.artifact_bytes, code_hash: result.artifact_hash, main_action: main }
}

pub fn compile_all_spora_example_contracts() -> Vec<CompiledCellScriptExample> {
    let examples_dir = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../cellscript/examples");
    BUNDLED_CELLSCRIPT_EXAMPLES
        .into_iter()
        .map(|name| {
            let result = compile_file(
                examples_dir.join(name),
                CompileOptions {
                    target: Some("riscv64-elf".to_string()),
                    target_profile: Some("spora".to_string()),
                    ..CompileOptions::default()
                },
            )
            .unwrap_or_else(|err| panic!("{name} must compile to Spora ELF: {err}"));

            assert_eq!(result.artifact_format, ArtifactFormat::RiscvElf, "{name} must compile to an ELF artifact");
            assert!(result.artifact_bytes.starts_with(b"\x7fELF"), "{name} artifact must start with the ELF magic");
            assert!(result.metadata.runtime.vm_abi.embedded_in_artifact, "{name} must embed the Molecule VM ABI");
            assert_eq!(
                result.artifact_hash,
                *blake3::hash(&result.artifact_bytes).as_bytes(),
                "{name} artifact hash must match artifact bytes"
            );
            assert!(!result.metadata.actions.is_empty(), "{name} must expose action metadata");

            CompiledCellScriptExample {
                name,
                artifact_bytes: result.artifact_bytes,
                code_hash: result.artifact_hash,
                ckb_runtime_required: result.metadata.runtime.ckb_runtime_required,
            }
        })
        .collect()
}

pub fn compile_token_spora_example_contract() -> CompiledCellScriptContract {
    let source = include_str!("../../../../cellscript/examples/token.cell");
    let result = compile(
        source,
        CompileOptions {
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("spora".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("bundled token.cell example must compile to Spora ELF");

    assert_eq!(result.artifact_format, ArtifactFormat::RiscvElf);
    assert!(result.artifact_bytes.starts_with(b"\x7fELF"));
    assert!(result.metadata.runtime.vm_abi.embedded_in_artifact);
    assert!(result.metadata.runtime.ckb_runtime_required);
    assert!(!result.metadata.runtime.standalone_runner_compatible);
    assert!(result.metadata.runtime.fail_closed_runtime_features.is_empty());
    assert_eq!(result.artifact_hash, *blake3::hash(&result.artifact_bytes).as_bytes());

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

    CompiledCellScriptContract { artifact_bytes: result.artifact_bytes, code_hash: result.artifact_hash }
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
