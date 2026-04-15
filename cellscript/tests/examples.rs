use camino::Utf8PathBuf;
use cellscript::{compile_file, ArtifactFormat, CompileOptions};

fn example_path(name: &str) -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples").join(name)
}

#[test]
fn bundled_examples_compile_to_non_empty_assembly() {
    for example in ["amm_pool.cell", "launch.cell", "multisig.cell", "nft.cell", "timelock.cell", "token.cell", "vesting.cell"] {
        let result = compile_file(example_path(example), CompileOptions::default()).unwrap_or_else(|err| {
            panic!("failed to compile {}: {}", example, err);
        });

        assert_eq!(result.artifact_format, ArtifactFormat::RiscvAssembly, "unexpected artifact format for {}", example);
        assert!(!result.artifact_bytes.is_empty(), "empty artifact for {}", example);
    }
}
