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
fn cellc_verify_artifact_enforces_policy_flags() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("sample.cell");
    let output = dir.path().join("sample.s");
    let source = r#"
module test

resource Token has store, transfer, destroy {
    amount: u64,
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
    amount: u64,
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
    assert!(stderr.contains("transfer-output:Token"), "unexpected stderr: {}", stderr);
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
    amount: u64,
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
    amount: u64,
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
// cellscript-test: production
// cellscript-test: expect-error: transfer-expression
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
// cellscript-test: expect-fail-closed-runtime
// cellscript-test: expect-runtime-feature: transfer-expression
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
