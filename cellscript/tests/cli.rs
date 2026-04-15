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
