use std::process::Command;

#[test]
fn cellc_writes_requested_output_file() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("sample.cell");
    let output = dir.path().join("sample.s");
    let source = r#"
module test

action add(x: u64, y: u64) -> u64 {
    let z = x + y
    return z
}
"#;
    std::fs::write(&input, source).unwrap();

    let status = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&input).arg("-o").arg(&output).status().unwrap();

    assert!(status.success());

    let written = std::fs::read_to_string(&output).unwrap();
    assert!(written.contains(".section .text"));
    assert!(written.contains(".global add"));

    let metadata = std::fs::read_to_string(dir.path().join("sample.s.meta.json")).unwrap();
    assert!(metadata.contains("\"actions\""));
    assert!(metadata.contains("\"add\""));
    assert!(metadata.contains("\"scheduler_witness_borsh_hex\""));
    assert!(metadata.contains("\"metadata_schema_version\""));
    assert!(metadata.contains("\"compiler_version\""));
    assert!(metadata.contains("\"artifact_hash_blake3\""));
    assert!(metadata.contains("\"artifact_size_bytes\""));
    assert!(metadata.contains("\"source_hash_blake3\""));
    assert!(metadata.contains("\"source_content_hash_blake3\""));
    assert!(metadata.contains("\"source_units\""));
}

#[test]
fn cellc_verify_artifact_accepts_matching_sidecar() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("sample.cell");
    let output = dir.path().join("sample.s");
    let source = r#"
module test

action add(x: u64, y: u64) -> u64 {
    x + y
}
"#;
    std::fs::write(&input, source).unwrap();

    let build = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&input).arg("-o").arg(&output).status().unwrap();
    assert!(build.success());

    let verify = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("verify-artifact").arg(&output).output().unwrap();

    assert!(verify.status.success(), "{}", String::from_utf8_lossy(&verify.stderr));
    let stdout = String::from_utf8_lossy(&verify.stdout);
    assert!(stdout.contains("Artifact verification succeeded"));
    assert!(stdout.contains("Metadata schema"));
    assert!(stdout.contains("Compiler"));
    assert!(stdout.contains("RISC-V assembly"));

    let verify_sources =
        Command::new(env!("CARGO_BIN_EXE_cellc")).arg("verify-artifact").arg(&output).arg("--verify-sources").output().unwrap();
    assert!(verify_sources.status.success(), "{}", String::from_utf8_lossy(&verify_sources.stderr));
    let stdout = String::from_utf8_lossy(&verify_sources.stdout);
    assert!(stdout.contains("Sources: verified 1 unit(s)"), "{}", stdout);
}

#[test]
fn cellc_verify_artifact_rejects_tampered_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("sample.cell");
    let output = dir.path().join("sample.s");
    let source = r#"
module test

action add(x: u64, y: u64) -> u64 {
    x + y
}
"#;
    std::fs::write(&input, source).unwrap();

    let build = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&input).arg("-o").arg(&output).status().unwrap();
    assert!(build.success());
    std::fs::write(&output, b"tampered").unwrap();

    let verify = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("verify-artifact").arg(&output).output().unwrap();

    assert!(!verify.status.success());
    let stderr = String::from_utf8_lossy(&verify.stderr);
    assert!(stderr.contains("metadata artifact_hash_blake3") || stderr.contains("artifact_hash"), "{}", stderr);
}

#[test]
fn cellc_verify_artifact_rejects_tampered_source_when_requested() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("sample.cell");
    let output = dir.path().join("sample.s");
    let source = r#"
module test

action add(x: u64, y: u64) -> u64 {
    x + y
}
"#;
    std::fs::write(&input, source).unwrap();

    let build = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&input).arg("-o").arg(&output).status().unwrap();
    assert!(build.success());
    std::fs::write(
        &input,
        r#"
module test

action add(x: u64, y: u64) -> u64 {
    x + y + 1
}
"#,
    )
    .unwrap();

    let verify =
        Command::new(env!("CARGO_BIN_EXE_cellc")).arg("verify-artifact").arg(&output).arg("--verify-sources").output().unwrap();

    assert!(!verify.status.success());
    let stderr = String::from_utf8_lossy(&verify.stderr);
    assert!(stderr.contains("source unit") && stderr.contains("does not match metadata"), "{}", stderr);
}

#[test]
fn cellc_verify_artifact_rejects_metadata_schema_downgrade() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("sample.cell");
    let output = dir.path().join("sample.s");
    let tampered_metadata = dir.path().join("schema-old.meta.json");
    let source = r#"
module test

action add(x: u64, y: u64) -> u64 {
    x + y
}
"#;
    std::fs::write(&input, source).unwrap();

    let build = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&input).arg("-o").arg(&output).status().unwrap();
    assert!(build.success());

    let metadata_path = dir.path().join("sample.s.meta.json");
    let mut metadata_json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&metadata_path).unwrap()).unwrap();
    let current_schema = metadata_json["metadata_schema_version"].as_u64().unwrap();
    metadata_json["metadata_schema_version"] = serde_json::json!(current_schema - 1);
    std::fs::write(&tampered_metadata, serde_json::to_vec_pretty(&metadata_json).unwrap()).unwrap();

    let verify = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("verify-artifact")
        .arg(&output)
        .arg("--metadata")
        .arg(&tampered_metadata)
        .output()
        .unwrap();

    assert!(!verify.status.success(), "unexpected success: {}", String::from_utf8_lossy(&verify.stdout));
    let stderr = String::from_utf8_lossy(&verify.stderr);
    assert!(stderr.contains("unsupported metadata_schema_version"), "{}", stderr);
}

#[test]
fn cellc_verify_artifact_rejects_noncanonical_source_unit_hash() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("sample.cell");
    let output = dir.path().join("sample.s");
    let tampered_metadata = dir.path().join("uppercase-source-hash.meta.json");
    let source = r#"
module test

action add(x: u64, y: u64) -> u64 {
    x + y
}
"#;
    std::fs::write(&input, source).unwrap();

    let build = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&input).arg("-o").arg(&output).status().unwrap();
    assert!(build.success());

    let metadata_path = dir.path().join("sample.s.meta.json");
    let mut metadata_json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&metadata_path).unwrap()).unwrap();
    let source_hash = metadata_json["source_units"][0]["hash_blake3"].as_str().unwrap().to_uppercase();
    metadata_json["source_units"][0]["hash_blake3"] = serde_json::json!(source_hash);
    std::fs::write(&tampered_metadata, serde_json::to_vec_pretty(&metadata_json).unwrap()).unwrap();

    let verify = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("verify-artifact")
        .arg(&output)
        .arg("--metadata")
        .arg(&tampered_metadata)
        .output()
        .unwrap();

    assert!(!verify.status.success(), "unexpected success: {}", String::from_utf8_lossy(&verify.stdout));
    let stderr = String::from_utf8_lossy(&verify.stderr);
    assert!(stderr.contains("expected 64 lowercase hex characters"), "{}", stderr);
}

#[test]
fn cellc_verify_artifact_enforces_policy_flags() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("sample.cell");
    let output = dir.path().join("sample.s");
    let source = r#"
module test

resource Token has store, transfer, destroy {
    amount: u128,
}

action move_token(token: Token, to: Address) -> Token {
    return transfer token to to
}
"#;
    std::fs::write(&input, source).unwrap();

    let build = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&input).arg("-o").arg(&output).status().unwrap();
    assert!(build.success());

    let verify = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("verify-artifact").arg(&output).arg("--production").output().unwrap();

    assert!(!verify.status.success(), "unexpected success: {}", String::from_utf8_lossy(&verify.stdout));
    let stderr = String::from_utf8_lossy(&verify.stderr);
    assert!(stderr.contains("check policy failed"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("transfer-expression"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("fail-closed"), "unexpected stderr: {}", stderr);
}

#[test]
fn cellc_verify_artifact_enforces_expected_hashes() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("sample.cell");
    let output = dir.path().join("sample.s");
    let source = r#"
module test

action add(x: u64, y: u64) -> u64 {
    x + y
}
"#;
    std::fs::write(&input, source).unwrap();

    let build = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&input).arg("-o").arg(&output).status().unwrap();
    assert!(build.success());

    let metadata_path = dir.path().join("sample.s.meta.json");
    let metadata_json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&metadata_path).unwrap()).unwrap();
    let artifact_hash = metadata_json["artifact_hash_blake3"].as_str().unwrap();
    let source_content_hash = metadata_json["source_content_hash_blake3"].as_str().unwrap();

    let verify = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("verify-artifact")
        .arg(&output)
        .arg("--expect-artifact-hash")
        .arg(artifact_hash)
        .arg("--expect-source-content-hash")
        .arg(source_content_hash)
        .output()
        .unwrap();
    assert!(verify.status.success(), "{}", String::from_utf8_lossy(&verify.stderr));
    let stdout = String::from_utf8_lossy(&verify.stdout);
    assert!(stdout.contains("Expected hashes: verified"), "{}", stdout);

    let verify = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("verify-artifact")
        .arg(&output)
        .arg("--json")
        .arg("--expect-artifact-hash")
        .arg(artifact_hash)
        .arg("--expect-source-content-hash")
        .arg(source_content_hash)
        .output()
        .unwrap();
    assert!(verify.status.success(), "{}", String::from_utf8_lossy(&verify.stderr));
    let stdout: serde_json::Value = serde_json::from_slice(&verify.stdout).unwrap();
    assert_eq!(stdout["status"], "ok");
    assert_eq!(stdout["artifact_hash_blake3"], artifact_hash);
    assert_eq!(stdout["source_content_hash_blake3"], source_content_hash);
    assert_eq!(stdout["expected_hashes_verified"], true);
    assert_eq!(stdout["policy_verified"], false);
    assert_eq!(stdout["sources_verified"], false);
    assert_eq!(stdout["runtime_required_verifier_obligations"], 0);
    assert_eq!(stdout["fail_closed_verifier_obligations"], 0);

    let verify = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("verify-artifact")
        .arg(&output)
        .arg("--expect-source-content-hash")
        .arg("00".repeat(32))
        .output()
        .unwrap();
    assert!(!verify.status.success(), "unexpected success: {}", String::from_utf8_lossy(&verify.stdout));
    let stderr = String::from_utf8_lossy(&verify.stderr);
    assert!(stderr.contains("source_content_hash_blake3") && stderr.contains("does not match expected"), "{}", stderr);

    let verify = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("verify-artifact")
        .arg(&output)
        .arg("--expect-artifact-hash")
        .arg(artifact_hash.to_uppercase())
        .output()
        .unwrap();
    assert!(!verify.status.success(), "unexpected success: {}", String::from_utf8_lossy(&verify.stdout));
    let stderr = String::from_utf8_lossy(&verify.stderr);
    assert!(stderr.contains("lowercase BLAKE3 hex digest"), "{}", stderr);
}

#[test]
fn cellc_compiles_bundled_examples_to_requested_outputs() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let examples_dir = manifest_dir.join("examples");
    let output_dir = tempfile::tempdir().unwrap();

    for example in ["amm_pool.cell", "launch.cell", "multisig.cell", "nft.cell", "timelock.cell", "token.cell", "vesting.cell"] {
        let input = examples_dir.join(example);
        let output = output_dir.path().join(example.replace(".cell", ".s"));

        let status = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&input).arg("-o").arg(&output).status().unwrap();
        assert!(status.success(), "cellc failed for {}", example);

        let written = std::fs::read_to_string(&output).unwrap();
        assert!(written.contains(".section .text"), "missing text section for {}", example);
        assert!(!written.trim().is_empty(), "empty output for {}", example);
    }
}

#[test]
fn cellc_compiles_package_with_local_path_dependency() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let dep_root = root.join("dep_pkg");
    let app_root = root.join("app_pkg");

    std::fs::create_dir_all(dep_root.join("src")).unwrap();
    std::fs::create_dir_all(app_root.join("src")).unwrap();

    std::fs::write(
        dep_root.join("Cell.toml"),
        r#"
[package]
name = "dep_pkg"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        dep_root.join("src").join("token.cell"),
        r#"
module dep::token

resource Token has store, transfer, destroy {
    amount: u64
}
"#,
    )
    .unwrap();

    std::fs::write(
        app_root.join("Cell.toml"),
        r#"
[package]
name = "app_pkg"
version = "0.1.0"

[dependencies]
dep_pkg = { path = "../dep_pkg" }
"#,
    )
    .unwrap();

    let app_entry = app_root.join("src").join("main.cell");
    std::fs::write(
        &app_entry,
        r#"
module app::main

use dep::token::Token

action pass_through(token: Token) -> Token {
    token
}
"#,
    )
    .unwrap();

    let output = app_root.join("build").join("main.s");
    let status = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&app_root).status().unwrap();

    assert!(status.success());

    let written = std::fs::read_to_string(&output).unwrap();
    assert!(written.contains(".section .text"));
    assert!(written.contains(".global pass_through"));
    assert!(!app_entry.with_extension("s").exists());
}

#[test]
fn cellc_rejects_registry_package_dependencies_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"

[dependencies]
remote = "1.2.3"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(root).output().unwrap();

    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("dependency 'remote' uses version requirement '1.2.3'"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("only local path dependencies are supported"), "unexpected stderr: {}", stderr);
    assert!(!root.join("build").join("main.s").exists());
    assert!(!root.join("build").join("main.s.meta.json").exists());
}

#[test]
fn cellc_rejects_underdeclared_effects_from_path_dependency_calls() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let dep_root = root.join("dep_pkg");
    let app_root = root.join("app_pkg");

    std::fs::create_dir_all(dep_root.join("src")).unwrap();
    std::fs::create_dir_all(app_root.join("src")).unwrap();

    std::fs::write(
        dep_root.join("Cell.toml"),
        r#"
[package]
name = "dep_pkg"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        dep_root.join("src").join("token.cell"),
        r#"
module dep::token

resource Token {
    amount: u64
}

action issue(amount: u64) -> Token {
    let out = create Token {
        amount: amount
    }
    return out
}
"#,
    )
    .unwrap();

    std::fs::write(
        app_root.join("Cell.toml"),
        r#"
[package]
name = "app_pkg"
version = "0.1.0"

[dependencies]
dep_pkg = { path = "../dep_pkg" }
"#,
    )
    .unwrap();
    std::fs::write(
        app_root.join("src").join("main.cell"),
        r#"
module app::main

use dep::token::Token
use dep::token::issue

#[effect(ReadOnly)]
action wrapper(amount: u64) -> Token {
    return issue(amount)
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&app_root).output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("declared effect ReadOnly is too weak"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("inferred effect is Creating"), "unexpected stderr: {}", stderr);

    std::fs::write(
        app_root.join("src").join("main.cell"),
        r#"
module app::main

use dep::token::Token

#[effect(ReadOnly)]
action wrapper(amount: u64) -> Token {
    return dep::token::issue(amount)
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&app_root).output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("declared effect ReadOnly is too weak"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("inferred effect is Creating"), "unexpected stderr: {}", stderr);
}

#[test]
fn cellc_rejects_external_dependency_function_calls_until_linking_exists() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let dep_root = root.join("dep_pkg");
    let app_root = root.join("app_pkg");

    std::fs::create_dir_all(dep_root.join("src")).unwrap();
    std::fs::create_dir_all(app_root.join("src")).unwrap();

    std::fs::write(
        dep_root.join("Cell.toml"),
        r#"
[package]
name = "dep_pkg"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        dep_root.join("src").join("math.cell"),
        r#"
module dep::math

fn add_one(x: u64) -> u64 {
    return x + 1
}
"#,
    )
    .unwrap();

    std::fs::write(
        app_root.join("Cell.toml"),
        r#"
[package]
name = "app_pkg"
version = "0.1.0"

[dependencies]
dep_pkg = { path = "../dep_pkg" }
"#,
    )
    .unwrap();
    std::fs::write(
        app_root.join("src").join("main.cell"),
        r#"
module app::main

action run(x: u64) -> u64 {
    return dep::math::add_one(x)
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&app_root).output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("external function call 'dep::math::add_one' is not linkable yet"), "unexpected stderr: {}", stderr);
}

#[test]
fn cellc_uses_manifest_build_out_dir_for_package_input() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"

[build]
out_dir = "artifacts"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
    )
    .unwrap();

    let output = root.join("artifacts").join("main.s");
    let status = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(root).status().unwrap();

    assert!(status.success());

    let written = std::fs::read_to_string(&output).unwrap();
    assert!(written.contains(".section .text"));
    assert!(!root.join("build").join("main.s").exists());
}

#[test]
fn cellc_cli_target_overrides_manifest_build_target() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"

[build]
target = "riscv64-elf"
out_dir = "artifacts"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
    )
    .unwrap();

    let output = root.join("artifacts").join("main.s");
    let status = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(root).arg("--target").arg("riscv64-asm").status().unwrap();

    assert!(status.success());

    let written = std::fs::read_to_string(&output).unwrap();
    assert!(written.contains(".section .text"));
    assert!(!written.trim().is_empty());
}

#[test]
fn cellc_uses_manifest_build_target_by_default() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"

[build]
target = "riscv64-elf"
out_dir = "artifacts"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
    )
    .unwrap();

    let output = root.join("artifacts").join("main.elf");
    let status = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(root).status().unwrap();

    assert!(status.success());

    let written = std::fs::read(&output).unwrap();
    assert!(written.starts_with(b"\x7fELF"));
    assert!(!root.join("artifacts").join("main.s").exists());
}

#[test]
fn cellc_build_and_check_subcommands_use_package_flow() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
    )
    .unwrap();

    let check = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").status().unwrap();
    assert!(check.success());

    let build = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").status().unwrap();
    assert!(build.success());

    let output = root.join("build").join("main.s");
    let written = std::fs::read_to_string(output).unwrap();
    assert!(written.contains(".section .text"));
    let metadata = std::fs::read_to_string(root.join("build").join("main.s.meta.json")).unwrap();
    assert!(metadata.contains("\"module\": \"demo::main\""));
    assert!(metadata.contains("\"scheduler_witness_borsh_hex\""));

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").arg("--json").output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(stdout["status"], "ok");
    assert_eq!(stdout["artifact_format"], "RISC-V assembly");
    assert_eq!(stdout["policy_verified"], false);
    assert_eq!(stdout["runtime_required_verifier_obligations"], 0);
    assert_eq!(stdout["fail_closed_verifier_obligations"], 0);
    assert!(stdout["artifact"].as_str().unwrap().ends_with("build/main.s"));
    assert!(stdout["metadata"].as_str().unwrap().ends_with("build/main.s.meta.json"));
    assert!(stdout["artifact_hash_blake3"].as_str().unwrap().len() == 64);
    assert!(stdout["source_content_hash_blake3"].as_str().unwrap().len() == 64);
}

#[test]
fn cellc_check_all_targets_checks_asm_and_elf_without_writing_artifacts() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"

[build]
target = "riscv64-elf"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--all-targets").output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Check succeeded"), "unexpected stdout: {}", stdout);
    assert!(stdout.contains("riscv64-asm (RISC-V assembly)"), "unexpected stdout: {}", stdout);
    assert!(stdout.contains("riscv64-elf (RISC-V ELF)"), "unexpected stdout: {}", stdout);
    assert!(!root.join("build").join("main.s").exists());
    assert!(!root.join("build").join("main.elf").exists());
    assert!(!root.join("build").join("main.s.meta.json").exists());
    assert!(!root.join("build").join("main.elf.meta.json").exists());

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--all-targets").arg("--json").output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(stdout["status"], "ok");
    assert_eq!(stdout["all_targets"], true);
    assert_eq!(stdout["policy_verified"], false);
    let checked_targets = stdout["checked_targets"].as_array().unwrap();
    assert_eq!(checked_targets.len(), 2);
    assert!(checked_targets.iter().all(|target| target["runtime_required_verifier_obligations"] == 0));
    assert!(checked_targets.iter().all(|target| target["fail_closed_verifier_obligations"] == 0));
    assert!(checked_targets.iter().any(|target| target["requested_target"] == "riscv64-asm"));
    assert!(checked_targets.iter().any(|target| target["requested_target"] == "riscv64-elf"));
}

#[test]
fn cellc_check_production_rejects_fail_closed_runtime_paths() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

resource Token has store, transfer, destroy {
    amount: u128,
}

action move_token(token: Token, to: Address) -> Token {
    return transfer token to to
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--production").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("check policy failed"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("transfer-expression"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("fail-closed"), "unexpected stderr: {}", stderr);
}

#[test]
fn cellc_check_production_rejects_incomplete_output_verification() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

resource Fingerprint {
    digest: Hash,
}

fn make_digest() -> Hash {
    return Hash::zero()
}

action issue() -> Fingerprint {
    let digest = make_digest()
    let token = create Fingerprint {
        digest: digest
    }
    return token
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--production").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("check policy failed"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("output-verification-incomplete"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("fail-closed"), "unexpected stderr: {}", stderr);
}

#[test]
fn cellc_check_can_reject_symbolic_runtime_requirements() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

resource Token has store, transfer, destroy {
    amount: u64,
}

action issue(amount: u64) -> Token {
    return create Token { amount: amount }
}
"#,
    )
    .unwrap();

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--deny-symbolic-runtime").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("check policy failed"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("symbolic Cell/runtime"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("verify-output-cell"), "unexpected stderr: {}", stderr);
}

#[test]
fn cellc_check_can_reject_runtime_required_obligations() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

resource Token has store, transfer, destroy {
    amount: u64,
}

action move_token(token: Token, to: Address) -> Token {
    return transfer token to to
}
"#,
    )
    .unwrap();

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--deny-runtime-obligations").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("check policy failed"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("runtime-required verifier obligations"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("runtime-required transaction invariants with checked subconditions"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("transfer-output:Token"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("transfer-lock-rebinding"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("transfer-destination-address-binding"), "unexpected stderr: {}", stderr);
    assert!(
        !stderr.contains("runtime-required transaction runtime input requirements"),
        "checked transfer destination inputs should not be reported as runtime-required: {}",
        stderr
    );
    assert!(
        !stderr.contains("transfer-destination-lock"),
        "checked transfer lock input should not be reported as runtime-required: {}",
        stderr
    );
}

#[test]
fn cellc_check_reports_transaction_invariant_checked_subconditions() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

resource Token has store {
    amount: u64
    owner: Address
}

#[lifecycle(Granted -> Claimable -> FullyClaimed)]
receipt VestingGrant has store {
    state: u8
    beneficiary: Address
    total_amount: u64
    claimed_amount: u64
    cliff_daa_score: u64
    end_daa_score: u64
}

action claim_vested(grant: VestingGrant) -> (Token, VestingGrant) {
    let now = env::current_daa_score()

    assert_invariant(now >= grant.cliff_daa_score, "cliff not reached")
    assert_invariant(grant.state < 2, "already fully claimed")

    let vested_total = grant.total_amount
    let claimable = vested_total - grant.claimed_amount
    assert_invariant(claimable > 0, "nothing to claim")

    consume grant

    let new_state: u8 = if vested_total == grant.total_amount { 2 } else { 1 }

    let tokens = create Token {
        amount: claimable,
        owner: grant.beneficiary
    } with_lock(grant.beneficiary)

    let updated_grant = create VestingGrant {
        state: new_state,
        beneficiary: grant.beneficiary,
        total_amount: grant.total_amount,
        claimed_amount: grant.claimed_amount + claimable,
        cliff_daa_score: grant.cliff_daa_score,
        end_daa_score: grant.end_daa_score
    } with_lock(grant.beneficiary)

    (tokens, updated_grant)
}
"#,
    )
    .unwrap();

    let json_output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--json").output().unwrap();
    assert!(json_output.status.success(), "unexpected failure: {}", String::from_utf8_lossy(&json_output.stderr));
    let stdout: serde_json::Value = serde_json::from_slice(&json_output.stdout).unwrap();
    let target = &stdout["checked_targets"][0];
    assert!(target["runtime_required_transaction_invariants"].as_u64().unwrap() > 0, "unexpected stdout: {}", stdout);
    assert_eq!(target["runtime_required_transaction_invariant_checked_subconditions"], 5, "unexpected stdout: {}", stdout);
    assert_eq!(target["transaction_runtime_input_requirements"], 3, "unexpected stdout: {}", stdout);
    assert_eq!(target["runtime_required_transaction_runtime_input_requirements"], 1, "unexpected stdout: {}", stdout);
    assert_eq!(target["checked_transaction_runtime_input_requirements"], 2, "unexpected stdout: {}", stdout);
    assert_eq!(target["runtime_required_transaction_runtime_input_blockers"], 1, "unexpected stdout: {}", stdout);
    assert_eq!(target["runtime_required_transaction_runtime_input_blocker_classes"], 1, "unexpected stdout: {}", stdout);
    let summaries = target["runtime_required_transaction_invariant_checked_subcondition_summaries"]
        .as_array()
        .expect("transaction invariant summaries array");
    assert!(
        summaries.iter().any(|value| value.as_str().is_some_and(|summary| {
            summary.contains("action:claim_vested:claim-conditions:VestingGrant")
                && summary.contains("daa-cliff-reached")
                && summary.contains("state-not-fully-claimed")
                && summary.contains("positive-claimable")
        })),
        "unexpected transaction invariant summaries: {}",
        stdout
    );
    let runtime_inputs =
        target["transaction_runtime_input_requirement_summaries"].as_array().expect("transaction runtime input summaries array");
    assert!(
        runtime_inputs.iter().any(|value| value.as_str().is_some_and(|summary| {
            summary.contains("claim-conditions:VestingGrant:claim-witness-signature=Witness:VestingGrant.signature")
                && summary.contains("claim-witness-signature-65[65]")
        })),
        "unexpected transaction runtime input summaries: {}",
        stdout
    );
    let checked_runtime_inputs = target["checked_transaction_runtime_input_requirement_summaries"]
        .as_array()
        .expect("checked transaction runtime input summaries array");
    assert!(
        checked_runtime_inputs.iter().any(|value| value.as_str().is_some_and(|summary| {
            summary.contains("claim-conditions:VestingGrant:claim-time-context=Header:VestingGrant.daa_score")
                && summary.contains("claim-time-daa-score-u64[8]")
                && summary.contains("(checked-runtime)")
                && !summary.contains("blocker=")
                && !summary.contains("blocker_class=")
        })),
        "unexpected checked transaction runtime input summaries: {}",
        stdout
    );
    assert!(
        checked_runtime_inputs.iter().any(|value| value.as_str().is_some_and(|summary| {
            summary.contains("claim-conditions:VestingGrant:claim-authorization-domain=Witness:VestingGrant.authorization-domain")
                && summary.contains("claim-witness-authorization-domain")
                && summary.contains("(checked-runtime)")
                && !summary.contains("blocker=")
                && !summary.contains("blocker_class=")
        })),
        "unexpected checked transaction runtime input summaries: {}",
        stdout
    );
    let runtime_required_inputs = target["runtime_required_transaction_runtime_input_requirement_summaries"]
        .as_array()
        .expect("runtime-required transaction runtime input summaries array");
    assert!(
        runtime_required_inputs.iter().any(|value| value.as_str().is_some_and(|summary| {
            summary.contains("claim-conditions:VestingGrant:claim-witness-signature=Witness:VestingGrant.signature")
                && summary.contains("claim-witness-signature-65[65]")
                && summary.contains("(runtime-required)")
                && summary.contains(
                    "blocker=claim lowering checks witness shape but has no verifier-coverable signer key binding or secp256k1 verification call"
                )
                && summary.contains("blocker_class=witness-verification-gap")
        })),
        "unexpected runtime-required transaction runtime input summaries: {}",
        stdout
    );
    let runtime_input_blockers = target["runtime_required_transaction_runtime_input_blocker_summaries"]
        .as_array()
        .expect("runtime-required transaction runtime input blocker summaries array");
    assert!(
        runtime_input_blockers.iter().any(|value| value.as_str().is_some_and(|summary| {
            summary.contains("claim-conditions:VestingGrant:claim-witness-signature")
                && summary.contains(
                    "blocker=claim lowering checks witness shape but has no verifier-coverable signer key binding or secp256k1 verification call"
                )
                && summary.contains("blocker_class=witness-verification-gap")
        })),
        "unexpected runtime-required transaction runtime input blocker summaries: {}",
        stdout
    );
    let runtime_input_blocker_classes = target["runtime_required_transaction_runtime_input_blocker_class_summaries"]
        .as_array()
        .expect("runtime-required transaction runtime input blocker class summaries array");
    assert!(
        runtime_input_blocker_classes.iter().any(|value| value.as_str().is_some_and(|summary| {
            summary.contains("claim-conditions:VestingGrant:claim-witness-signature")
                && summary.contains("blocker_class=witness-verification-gap")
        })),
        "unexpected runtime-required transaction runtime input blocker class summaries: {}",
        stdout
    );

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--deny-runtime-obligations").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("check policy failed"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("runtime-required transaction invariants with checked subconditions"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("runtime-required transaction runtime input requirements"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("runtime-required transaction runtime input blockers"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("runtime-required transaction runtime input blocker classes"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("claim-conditions:VestingGrant"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("claim-witness-signature"), "unexpected stderr: {}", stderr);
    assert!(
        stderr.contains(
            "claim lowering checks witness shape but has no verifier-coverable signer key binding or secp256k1 verification call"
        ),
        "unexpected stderr: {}",
        stderr
    );
    assert!(stderr.contains("witness-verification-gap"), "unexpected stderr: {}", stderr);
    assert!(
        !stderr.contains("claim-authorization-domain=Witness"),
        "checked authorization-domain runtime input should not be reported as runtime-required: {}",
        stderr
    );
    assert!(!stderr.contains("authorization-domain-separation-gap"), "unexpected stderr: {}", stderr);
    assert!(!stderr.contains("claim-time-context"), "checked runtime input should not be reported as runtime-required: {}", stderr);
    assert!(stderr.contains("daa-cliff-reached"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("positive-claimable"), "unexpected stderr: {}", stderr);
}

#[test]
fn cellc_check_reports_pool_invariant_policy_families() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

resource Token has store {
    symbol: [u8; 8]
    amount: u64
}

receipt LPReceipt has store {
    pool_id: Hash
    lp_amount: u64
    provider: Address
}

shared Pool has store {
    token_a_symbol: [u8; 8]
    token_b_symbol: [u8; 8]
    reserve_a: u64
    reserve_b: u64
    total_lp: u64
    fee_rate_bps: u16
}

action seed_pool(token_a: Token, token_b: Token, fee_rate_bps: u16, provider: Address) -> (Pool, LPReceipt) {
    assert_invariant(token_a.symbol != token_b.symbol, "same token")
    assert_invariant(token_a.amount > 0 && token_b.amount > 0, "zero liquidity")
    assert_invariant(fee_rate_bps <= 10000, "fee too high")

    let initial_lp: u64 = token_a.amount
    consume token_a
    consume token_b

    let pool = create Pool {
        token_a_symbol: token_a.symbol,
        token_b_symbol: token_b.symbol,
        reserve_a: token_a.amount,
        reserve_b: token_b.amount,
        total_lp: initial_lp,
        fee_rate_bps: fee_rate_bps
    }

    let receipt = create LPReceipt {
        pool_id: pool.type_hash(),
        lp_amount: initial_lp,
        provider: provider
    } with_lock(provider)

    (pool, receipt)
}
"#,
    )
    .unwrap();

    let json_output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--json").output().unwrap();
    assert!(json_output.status.success(), "unexpected failure: {}", String::from_utf8_lossy(&json_output.stderr));
    let stdout: serde_json::Value = serde_json::from_slice(&json_output.stdout).unwrap();
    let target = &stdout["checked_targets"][0];
    assert!(target["checked_pool_invariant_families"].as_u64().unwrap() > 0, "unexpected stdout: {}", stdout);
    assert_eq!(target["runtime_required_pool_invariant_families"].as_u64().unwrap(), 0, "unexpected stdout: {}", stdout);
    assert_eq!(target["pool_runtime_input_requirements"].as_u64().unwrap(), 0, "unexpected stdout: {}", stdout);
    let runtime_inputs = target["pool_runtime_input_requirement_summaries"].as_array().expect("runtime input summaries array");
    assert!(runtime_inputs.is_empty(), "checked seed_pool identity should leave no Pool runtime inputs: {}", stdout);

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--deny-runtime-obligations").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("check policy failed"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("runtime-required verifier obligations"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("pool-create:Pool"), "unexpected stderr: {}", stderr);
    assert!(!stderr.contains("runtime-required Pool invariant families"), "unexpected stderr: {}", stderr);
    assert!(!stderr.contains("runtime-required Pool runtime input requirements"), "unexpected stderr: {}", stderr);
    assert!(!stderr.contains("token-pair-identity-admission=Input#0:token_a"), "unexpected stderr: {}", stderr);
    assert!(!stderr.contains("token-input-type-id-abi"), "unexpected stderr: {}", stderr);
    assert!(!stderr.contains("token-pair-symbol-admission"), "unexpected stderr: {}", stderr);
    assert!(!stderr.contains("positive-reserve-admission"), "unexpected stderr: {}", stderr);
    assert!(!stderr.contains("fee-policy"), "unexpected stderr: {}", stderr);
    assert!(!stderr.contains("lp-supply-invariant"), "unexpected stderr: {}", stderr);
}

#[test]
fn cellc_check_reports_runtime_required_pool_blocker_classes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let amm_source = std::fs::read_to_string(manifest_dir.join("examples").join("amm_pool.cell"))
        .unwrap()
        .replace("use spora::fungible_token::Token", "resource Token has store {\n    symbol: [u8; 8]\n    amount: u64\n}");

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(root.join("src").join("main.cell"), amm_source).unwrap();

    let json_output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--json").output().unwrap();
    assert!(json_output.status.success(), "unexpected failure: {}", String::from_utf8_lossy(&json_output.stderr));
    let stdout: serde_json::Value = serde_json::from_slice(&json_output.stdout).unwrap();
    let target = &stdout["checked_targets"][0];
    assert!(target["runtime_required_pool_invariant_families"].as_u64().unwrap() > 0, "unexpected stdout: {}", stdout);
    assert!(target["runtime_required_pool_invariant_blocker_classes"].as_u64().unwrap() > 0, "unexpected stdout: {}", stdout);
    let blocker_classes = target["runtime_required_pool_invariant_blocker_class_summaries"]
        .as_array()
        .expect("runtime-required Pool invariant blocker class summaries array");
    assert!(
        blocker_classes.iter().any(|value| value.as_str().is_some_and(|summary| {
            summary.contains("pool-mutation-invariants:Pool:reserve-conservation")
                && summary.contains("blocker_class=phase2-deferred-amm-reserve-conservation")
        })),
        "unexpected Pool blocker class summaries: {}",
        stdout
    );
    assert!(
        blocker_classes.iter().any(|value| value.as_str().is_some_and(|summary| {
            summary.contains("pool-mutation-invariants:Pool:pool-specific-admission")
                && summary.contains("blocker_class=phase2-deferred-pool-admission")
        })),
        "unexpected Pool blocker class summaries: {}",
        stdout
    );
    let runtime_inputs = target["pool_runtime_input_requirement_summaries"].as_array().expect("runtime input summaries array");
    assert!(
        runtime_inputs.iter().any(|value| value.as_str().is_some_and(|summary| {
            summary.contains("reserve-conservation=Input#")
                && summary.contains(":pool.reserve_a:")
                && summary.contains("blocker_class=phase2-deferred-amm-reserve-conservation")
        })),
        "unexpected Pool runtime input summaries: {}",
        stdout
    );

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--deny-runtime-obligations").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("runtime-required Pool invariant blocker classes"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("phase2-deferred-amm-reserve-conservation"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("phase2-deferred-pool-admission"), "unexpected stderr: {}", stderr);
}

#[test]
fn cellc_check_uses_manifest_policy_defaults() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"

[policy]
production = true
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

resource Token has store, transfer, destroy {
    amount: u128,
}

action move_token(token: Token, to: Address) -> Token {
    return transfer token to to
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("check policy failed"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("transfer-expression"), "unexpected stderr: {}", stderr);
}

#[test]
fn cellc_build_uses_manifest_policy_before_writing_artifacts() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"

[policy]
production = true
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

resource Token has store, transfer, destroy {
    amount: u128,
}

action move_token(token: Token, to: Address) -> Token {
    return transfer token to to
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("check policy failed"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("transfer-expression"), "unexpected stderr: {}", stderr);
    assert!(!root.join("build").join("main.s").exists());
    assert!(!root.join("build").join("main.s.meta.json").exists());
}

#[test]
fn cellc_test_subcommand_compiles_test_sources() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("tests").join("math.cell"),
        r#"
module demo::tests::math

action adds() -> u64 {
    1 + 2
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("test").arg("--no-run").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Test compile complete"));
    assert!(stdout.contains("Compiled 1 test file(s)"));

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("test").arg("--no-run").arg("--json").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(stdout["status"], "ok");
    assert_eq!(stdout["test_files"], 1);
    assert_eq!(stdout["passed"], 1);
    assert_eq!(stdout["failed"], 0);
    assert_eq!(stdout["no_run"], true);
    assert_eq!(stdout["execution"], "disabled");
    let tests = stdout["tests"].as_array().unwrap();
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0]["status"], "passed");
    assert!(tests[0]["path"].as_str().unwrap().ends_with("tests/math.cell"));
}

#[test]
fn cellc_test_subcommand_supports_expected_compile_failures() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("tests").join("negative.cell"),
        r#"
// cellscript-test: expect-error: pure function cannot call action
module demo::tests::negative

action impure() -> u64 {
    1
}

fn helper() -> u64 {
    impure()
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("test").arg("--no-run").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Test compile complete"));
    assert!(stdout.contains("Compiled 1 test file(s)"));
}

#[test]
fn cellc_test_subcommand_rejects_missing_expected_error_text() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("tests").join("negative.cell"),
        r#"
// cellscript-test: expect-error: this text is intentionally absent
module demo::tests::negative

action impure() -> u64 {
    1
}

fn helper() -> u64 {
    impure()
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("test").arg("--no-run").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("expected error text not found"), "unexpected stderr: {}", stderr);
}

#[test]
fn cellc_test_subcommand_supports_target_directive() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("tests").join("elf.cell"),
        r#"
// cellscript-test: target: riscv64-elf
module demo::tests::elf

action main() -> u64 {
    0
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("test").arg("--no-run").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Compiled 1 test file(s)"), "unexpected stdout: {}", stdout);
}

#[test]
fn cellc_test_subcommand_supports_policy_directives() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("tests").join("policy.cell"),
        r#"
// cellscript-test: deny-runtime-obligations
// cellscript-test: expect-error: transfer-output:Token
module demo::tests::policy

resource Token has store, transfer, destroy {
    amount: u64,
}

action move_token(token: Token, to: Address) -> Token {
    return transfer token to to
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("test").arg("--no-run").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Compiled 1 test file(s)"), "unexpected stdout: {}", stdout);
}

#[test]
fn cellc_test_subcommand_supports_runtime_metadata_directives() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("tests").join("metadata.cell"),
        r#"
// cellscript-test: expect-not-standalone
// cellscript-test: expect-ckb-runtime
// cellscript-test: expect-symbolic-runtime
// cellscript-test: expect-no-fail-closed-runtime
// cellscript-test: expect-runtime-feature: verify-output-cell
// cellscript-test: expect-no-runtime-feature: transfer-expression
// cellscript-test: expect-verifier-obligation: transfer:Token
// cellscript-test: expect-verifier-obligation: transfer-output:Token
// cellscript-test: expect-runtime-required-obligation: transfer-output:Token
// cellscript-test: expect-no-verifier-obligation: not-present
// cellscript-test: expect-no-runtime-required-obligation: destroy-output-scan:Token
module demo::tests::metadata

resource Token has store, transfer, destroy {
    amount: u64,
}

action move_token(token: Token, to: Address) -> Token {
    return transfer token to to
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("test").arg("--no-run").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Compiled 1 test file(s)"), "unexpected stdout: {}", stdout);
}

#[test]
fn cellc_test_subcommand_rejects_missing_runtime_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("tests").join("metadata.cell"),
        r#"
// cellscript-test: expect-runtime-feature: not-present
module demo::tests::metadata

action ping() -> u64 {
    1
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("test").arg("--no-run").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("expected runtime metadata to contain 'not-present'"), "unexpected stderr: {}", stderr);
}

#[test]
fn cellc_test_subcommand_supports_entrypoint_metadata_directives() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("tests").join("entries.cell"),
        r#"
// cellscript-test: expect-artifact-format: RISC-V assembly
// cellscript-test: expect-action: run
// cellscript-test: expect-function: helper
// cellscript-test: expect-no-action: helper
// cellscript-test: expect-no-lock: run
module demo::tests::entries

fn helper(x: u64) -> u64 {
    x + 1
}

action run(x: u64) -> u64 {
    helper(x)
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("test").arg("--no-run").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Compiled 1 test file(s)"), "unexpected stdout: {}", stdout);
}

#[test]
fn cellc_test_subcommand_rejects_missing_entrypoint_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("tests").join("entries.cell"),
        r#"
// cellscript-test: expect-function: missing_helper
module demo::tests::entries

action run(x: u64) -> u64 {
    x
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("test").arg("--no-run").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("expected function metadata to contain 'missing_helper'"), "unexpected stderr: {}", stderr);
}

#[test]
fn cellc_test_subcommand_rejects_unknown_directives() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("tests").join("typo.cell"),
        r#"
// cellscript-test: expect-eror: typo should not be ignored
module demo::tests::typo

action ping() -> u64 {
    1
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("test").arg("--no-run").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown cellscript-test directive"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("expect-eror"), "unexpected stderr: {}", stderr);
}

#[test]
fn cellc_test_subcommand_rejects_conflicting_expectations() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("tests").join("conflict.cell"),
        r#"
// cellscript-test: expect-success
// cellscript-test: expect-fail
module demo::tests::conflict

action ping() -> u64 {
    1
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("test").arg("--no-run").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("conflicting cellscript-test directives"), "unexpected stderr: {}", stderr);
}

#[test]
fn cellc_doc_subcommand_generates_markdown_docs() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action ping() -> u64 {
    1
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("doc")
        .arg("--format")
        .arg("markdown")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["status"], "ok");
    assert_eq!(summary["format"], "markdown");
    assert!(summary["output"].as_str().unwrap().ends_with("docs/cellscript-api.md"));
    assert!(summary["output_size_bytes"].as_u64().unwrap() > 0);

    let docs = std::fs::read_to_string(root.join("docs").join("cellscript-api.md")).unwrap();
    assert!(docs.contains("## Module `demo::main`"));
    assert!(docs.contains("### action `ping`"));
    assert!(docs.contains("## Lowering Audit Report"));
    assert!(docs.contains("### Verifier Obligations"));
}

#[test]
fn cellc_init_subcommand_supports_json_summary() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("demo_pkg");

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).arg("init").arg("demo").arg(&root).arg("--lib").arg("--json").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["status"], "ok");
    assert_eq!(summary["kind"], "library");
    assert_eq!(summary["package"], "demo");
    assert!(summary["manifest"].as_str().unwrap().ends_with("demo_pkg/Cell.toml"));
    assert_eq!(summary["entry"], "src/lib.cell");
    assert!(root.join("Cell.toml").exists());
    assert!(root.join("src").join("lib.cell").exists());
}

#[test]
fn cellc_clean_subcommand_supports_json_summary() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("target")).unwrap();
    std::fs::create_dir_all(root.join(".cell").join("cache")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("clean").arg("--json").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["status"], "ok");
    assert_eq!(summary["removed"], 2);
    assert_eq!(summary["removed_paths"].as_array().unwrap().len(), 2);
    assert!(!root.join("target").exists());
    assert!(!root.join(".cell").join("cache").exists());
}

#[test]
fn cellc_info_subcommand_supports_json_summary() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
authors = ["Audit Bot"]
description = "demo package"
license = "MIT"
entry = "src/main.cell"

[dependencies]
math = "1"

[policy]
deny_fail_closed = true
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("info").arg("--json").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["status"], "ok");
    assert_eq!(summary["manifest"], "Cell.toml");
    assert_eq!(summary["package"]["name"], "demo");
    assert_eq!(summary["package"]["authors"][0], "Audit Bot");
    assert_eq!(summary["dependencies"]["math"], "1");
    assert_eq!(summary["policy"]["deny_fail_closed"], true);
}

#[test]
fn cellc_add_and_remove_subcommands_honor_dev_path_and_json() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();

    let add_output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("add")
        .arg("--dev")
        .arg("--path")
        .arg("../math")
        .arg("--json")
        .arg("math")
        .output()
        .unwrap();
    assert!(add_output.status.success(), "stderr: {}", String::from_utf8_lossy(&add_output.stderr));

    let add_summary: serde_json::Value = serde_json::from_slice(&add_output.stdout).unwrap();
    assert_eq!(add_summary["status"], "ok");
    assert_eq!(add_summary["target"], "dev-dependencies");
    assert_eq!(add_summary["added"][0], "math");
    assert_eq!(add_summary["dependency"]["path"], "../math");

    let manifest: toml::Value = std::fs::read_to_string(root.join("Cell.toml")).unwrap().parse().unwrap();
    assert_eq!(manifest["dev_dependencies"]["math"]["path"].as_str().unwrap(), "../math");
    assert!(manifest.get("dependencies").and_then(|value| value.get("math")).is_none());

    let remove_output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("remove")
        .arg("--dev")
        .arg("--json")
        .arg("math")
        .output()
        .unwrap();
    assert!(remove_output.status.success(), "stderr: {}", String::from_utf8_lossy(&remove_output.stderr));

    let remove_summary: serde_json::Value = serde_json::from_slice(&remove_output.stdout).unwrap();
    assert_eq!(remove_summary["status"], "ok");
    assert_eq!(remove_summary["target"], "dev-dependencies");
    assert_eq!(remove_summary["removed"][0], "math");
    assert!(remove_summary["missing"].as_array().unwrap().is_empty());

    let manifest_after: toml::Value = std::fs::read_to_string(root.join("Cell.toml")).unwrap().parse().unwrap();
    assert!(manifest_after.get("dev_dependencies").and_then(|value| value.get("math")).is_none());
}

#[test]
fn cellc_metadata_subcommand_emits_lowering_runtime_json() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

shared Config {
    threshold: u64
}

resource Token has store, transfer, destroy {
    amount: u64
}

action update(amount: u64) -> u64 {
    let cfg = read_ref<Config>()
    let token = create Token { amount: amount }
    consume token
    return cfg.threshold
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("metadata").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"lowering\""));
    assert!(stdout.contains("\"runtime\""));
    assert!(stdout.contains("\"symbolic_cell_runtime_required\": true"));
    assert!(stdout.contains("\"fail_closed_runtime_features\""));
    assert!(stdout.contains("\"verifier_obligations\""));
    assert!(stdout.contains("\"source\": \"Input\""));
    assert!(stdout.contains("\"source\": \"CellDep\""));
    assert!(stdout.contains("\"source\": \"Output\""));
    assert!(stdout.contains("\"elf_compatible\": false"));
    assert!(stdout.contains("\"ckb_runtime_required\": true"));
    assert!(stdout.contains("read-cell-dep"));
    assert!(stdout.contains("create-expression"));
    assert!(!stdout.contains("schema-field-access"));
}

#[test]
fn cellc_fmt_subcommand_formats_sources() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    let source_path = root.join("src").join("main.cell");
    std::fs::write(&source_path, "module demo::main\naction ping(x:u64)->u64{x}\n").unwrap();

    let dirty_check =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("fmt").arg("--check").arg("--json").output().unwrap();
    assert!(!dirty_check.status.success(), "unexpected success: {}", String::from_utf8_lossy(&dirty_check.stdout));
    let stdout: serde_json::Value = serde_json::from_slice(&dirty_check.stdout).unwrap();
    assert_eq!(stdout["status"], "failed");
    assert_eq!(stdout["mode"], "check");
    assert_eq!(stdout["changed"], 1);
    assert!(stdout["changed_files"][0].as_str().unwrap().ends_with("src/main.cell"));

    let status = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("fmt").status().unwrap();
    assert!(status.success());

    let formatted = std::fs::read_to_string(&source_path).unwrap();
    assert!(formatted.contains("action ping(x: u64) -> u64 {"));

    let check = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("fmt").arg("--check").arg("--json").output().unwrap();
    assert!(check.status.success(), "{}", String::from_utf8_lossy(&check.stderr));
    let stdout: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    assert_eq!(stdout["status"], "ok");
    assert_eq!(stdout["mode"], "check");
    assert_eq!(stdout["changed"], 0);
}

#[cfg(not(feature = "vm-runner"))]
#[test]
fn cellc_run_subcommand_is_fail_closed_without_vm_runner_feature() {
    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("run").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cellc run is still experimental"));
    assert!(stderr.contains("feature-gated VM backend"));
}

#[cfg(feature = "vm-runner")]
#[test]
fn cellc_run_subcommand_executes_pure_elf_package() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action main() -> u64 {
    0
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("run").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Run complete"));
    assert!(stdout.contains("Artifact format: RISC-V ELF"));
    assert!(stdout.contains("Cycles:"));
}

#[cfg(feature = "vm-runner")]
#[test]
fn cellc_run_subcommand_rejects_parameterized_schema_elf() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

struct Snapshot {
    amount: u64,
}

action main(snapshot: Snapshot) -> u64 {
    snapshot.amount
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("run").output().unwrap();
    assert!(!output.status.success(), "stdout: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no-argument pure ELF entrypoints"), "stderr: {}", stderr);
    assert!(stderr.contains("action main"), "stderr: {}", stderr);
}

#[cfg(feature = "vm-runner")]
#[test]
fn cellc_run_subcommand_rejects_ckb_runtime_elf() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

shared Config {
    threshold: u64,
}

action main() -> u64 {
    let cfg = read_ref<Config>()
    cfg.threshold
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("run").output().unwrap();
    assert!(!output.status.success(), "stdout: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cannot provide CKB transaction/syscall context"), "stderr: {}", stderr);
    assert!(stderr.contains("read-cell-dep"), "stderr: {}", stderr);
}
