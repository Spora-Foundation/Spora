#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:-quick}"

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/tmp/spora-cellscript-release-gate-target}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"
export CELLSCRIPT_BACKEND_SHAPE_REPORT="${CELLSCRIPT_BACKEND_SHAPE_REPORT:-$ROOT_DIR/target/cellscript-backend-shape/backend-shape-report-$MODE.json}"
export CELLSCRIPT_MOLECULE_SCHEMA_MANIFEST_REPORT="${CELLSCRIPT_MOLECULE_SCHEMA_MANIFEST_REPORT:-$ROOT_DIR/target/cellscript-schema-manifest/schema-manifest-report-$MODE.json}"

cd "$ROOT_DIR"
mkdir -p "$(dirname "$CELLSCRIPT_BACKEND_SHAPE_REPORT")"
mkdir -p "$(dirname "$CELLSCRIPT_MOLECULE_SCHEMA_MANIFEST_REPORT")"

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        printf 'missing required command: %s\n' "$1" >&2
        exit 127
    fi
}

require_cmd rg

run() {
    printf '\n==> %s\n' "$*"
    "$@"
}

check_trailing_whitespace() {
    local files=(
        ".github/workflows/cellscript-dual-chain.yml"
        ".github/workflows/spora-devnet-acceptance.yml"
        "cellscript/CHANGELOG.md"
        "cellscript/README.md"
        "cellscript/README_CH.md"
        "cellscript/docs/CELLSCRIPT_DUAL_CHAIN_PRODUCTION_PLAN.md"
        "scripts/cellscript_dual_chain_release_gate.sh"
        "scripts/ckb_cellscript_acceptance.sh"
        "scripts/spora_cellscript_acceptance.sh"
        "scripts/regenerate_snapshots.sh"
        "scripts/regenerate_test_data.sh"
        "cellscript/src/docgen/mod.rs"
        "cellscript/src/lsp/mod.rs"
        "cellscript/src/package/mod.rs"
        "cellscript/tests/cli.rs"
        "consensus/core/src/block.rs"
        "consensus/src/pipeline/virtual_processor/access_summary.rs"
        "consensus/src/pipeline/virtual_processor/processor.rs"
        "exec/src/celltx/mod.rs"
        "exec/src/celltx/types.rs"
        "mining/src/block_template/selector.rs"
        "mining/src/manager_tests.rs"
        "sdk/adaptor/examples/adaptor_reject_noncanonical.rs"
        "sdk/adaptor/examples/adaptor_roundtrip.rs"
        "sdk/adaptor/src/lib.rs"
        "wallet/core/src/tx/generator/generator.rs"
        "wallet/core/src/tx/generator/settings.rs"
        "wallet/core/src/wasm/tx/generator/generator.rs"
    )

    if rg -n '[ \t]+$' "${files[@]}"; then
        printf '\nTrailing whitespace found in release-gate files.\n' >&2
        exit 1
    fi
}

check_dual_chain_production_plan() {
    local doc="cellscript/docs/CELLSCRIPT_DUAL_CHAIN_PRODUCTION_PLAN.md"
    local required=(
        "**Status**: Canonical production roadmap"
        "This document replaces the older CellScript v1 scope"
        "Current Truth"
        "Production Definition"
        "Bundled Example Closure Matrix"
        "Phase A: CKB Strict Original Closure"
        "Phase B: Molecule Schema Productionization"
        "Phase C: Action Transaction Builder"
        "Phase D: Dual-Chain Acceptance Gates"
        "Phase E: Package Manager and Tooling RC"
        "Phase F: Security and External Audit Readiness"
        "strict_original_ckb_compile_policy_fail_closed == []"
        '`token.cell`'
        '`nft.cell`'
        '`timelock.cell`'
        '`multisig.cell`'
        '`vesting.cell`'
        '`amm_pool.cell`'
        '`launch.cell`'
        "Base devnet probes remain useful only as regression tests."
        "Do not claim full CKB production support until all original bundled examples"
        "Do not let Spora support regress while closing CKB support."
        "Do not reintroduce public Borsh CellScript/CKB wire formats."
        "Every bundled example has a generated schema manifest."
        "Acceptance scripts use the builder instead of bespoke Python transaction"
    )
    local forbidden=(
        "V1-complete"
        "V1-bounded"
        "Phase 4 release evidence"
        "Operationally closed"
        "complete enough to close"
        "all original CKB P0 blockers are closed"
    )

    if [[ ! -f "$doc" ]]; then
        printf 'missing dual-chain production plan: %s\n' "$doc" >&2
        exit 1
    fi

    local pattern
    for pattern in "${required[@]}"; do
        if ! rg --quiet --fixed-strings "$pattern" "$doc"; then
            printf 'dual-chain production plan is missing required boundary: %s\n' "$pattern" >&2
            exit 1
        fi
    done

    for pattern in "${forbidden[@]}"; do
        if rg --quiet --fixed-strings "$pattern" "$doc"; then
            printf 'dual-chain production plan contains stale v1 wording: %s\n' "$pattern" >&2
            rg -n --fixed-strings "$pattern" "$doc" >&2
            exit 1
        fi
    done
}

check_v1_public_docs_boundaries() {
    local required=(
        "cellscript/README.md::The \`ckb\` profile is intentionally bounded in v1."
        "cellscript/README.md::Bounded ckb-vm artifact profile for the admitted Cell subset"
        "cellscript/README_CH.md::v1 的 \`ckb\` profile 是有边界的 artifact profile。"
        "cellscript/README_CH.md::对受支持 Cell 子集提供有边界的 ckb-vm artifact profile"
        "README.md::CKB strict profile 使用 CKB syscall/source/hash/header/Molecule 规则。"
        "README.md::采用 Molecule 作为 VM/CellScript 公共 ABI"
        "README.md::CKB strict profile 不接受 Borsh 作为公开 wire format"
        "README.md::pub const CKB_SYSCALL_LOAD_SCRIPT: u64 = 2052;"
    )
    local forbidden=(
        "Direct ckb-vm target profile"
        "直接提供 ckb-vm target profile"
        "Molecule 完全对齐 CKB"
        "完全复刻 CKB syscall"
        "系统调用**完全对齐 CKB**"
        "实际使用 Borsh"
        "Serialization**: **Borsh"
        "pub const SYSCALL_LOAD_SCRIPT: u64 = 2075;"
    )
    local public_docs=(
        "README.md"
        "cellscript/README.md"
        "cellscript/README_CH.md"
    )

    local item file pattern
    for item in "${required[@]}"; do
        file="${item%%::*}"
        pattern="${item#*::}"
        if ! rg --quiet --fixed-strings "$pattern" "$file"; then
            printf 'public v1 docs are missing required boundary in %s: %s\n' "$file" "$pattern" >&2
            exit 1
        fi
    done

    for pattern in "${forbidden[@]}"; do
        if rg --quiet --fixed-strings "$pattern" "${public_docs[@]}"; then
            printf 'public v1 docs contain forbidden overclaim: %s\n' "$pattern" >&2
            rg -n --fixed-strings "$pattern" "${public_docs[@]}" >&2
            exit 1
        fi
    done
}

check_v1_ci_workflow() {
    local workflow=".github/workflows/cellscript-dual-chain.yml"
    local standalone_workflow="cellscript/.github/workflows/ci.yml"
    local devnet_workflow=".github/workflows/spora-devnet-acceptance.yml"
    local required=(
        "name: CellScript Dual-Chain Gate"
        "workflow_dispatch:"
        "rustup toolchain install 1.85.0 --profile minimal --component rustfmt"
        "ripgrep"
        "CARGO_TARGET_DIR: /tmp/spora-dual-chain-release-gate-target"
        "CELLSCRIPT_BACKEND_SHAPE_REPORT:"
        '"cellscript"'
        '"cellscript/**"'
        "./scripts/cellscript_dual_chain_release_gate.sh v1"
        "actions/upload-artifact@v4"
        "if-no-files-found: error"
        "target/cellscript-backend-shape/"
        "target/ckb-cellscript-acceptance/"
    )
    local forbidden=(
        "./scripts/cellscript_dual_chain_release_gate.sh quick"
        "./scripts/cellscript_dual_chain_release_gate.sh full"
    )

    if [[ ! -f "$workflow" ]]; then
        printf 'missing v1 CI workflow: %s\n' "$workflow" >&2
        exit 1
    fi
    if [[ ! -f "$standalone_workflow" ]]; then
        printf 'missing CellScript standalone CI workflow: %s\n' "$standalone_workflow" >&2
        exit 1
    fi
    if [[ ! -f "$devnet_workflow" ]]; then
        printf 'missing Spora devnet acceptance workflow: %s\n' "$devnet_workflow" >&2
        exit 1
    fi

    local pattern
    for pattern in "${required[@]}"; do
        if ! rg --quiet --fixed-strings "$pattern" "$workflow"; then
            printf 'v1 CI workflow is missing required gate boundary: %s\n' "$pattern" >&2
            exit 1
        fi
    done

    for pattern in "${forbidden[@]}"; do
        if rg --quiet --fixed-strings "$pattern" "$workflow"; then
            printf 'v1 CI workflow must run the v1 gate, not a weaker mode: %s\n' "$pattern" >&2
            rg -n --fixed-strings "$pattern" "$workflow" >&2
            exit 1
        fi
    done

    local standalone_required=(
        "CELLSCRIPT_BACKEND_SHAPE_REPORT: /tmp/cellscript-backend-shape/backend-shape-report.json"
        "key: \${{ runner.os }}-cargo-\${{ hashFiles('**/Cargo.lock') }}"
        "cargo test --locked --manifest-path Cargo.toml -- --test-threads=1"
        "actions/upload-artifact@v4"
        "if-no-files-found: error"
        "cellscript-backend-shape-report"
        "/tmp/cellscript-backend-shape/"
    )
    for pattern in "${standalone_required[@]}"; do
        if ! rg --quiet --fixed-strings "$pattern" "$standalone_workflow"; then
            printf 'CellScript standalone CI workflow is missing required production artifact boundary: %s\n' "$pattern" >&2
            exit 1
        fi
    done

    local devnet_required=(
        "actions/upload-artifact@v4"
        "if-no-files-found: error"
        '"cellscript"'
        '"cellscript/**"'
        "spora-devnet-base-acceptance"
        "spora-devnet-full-acceptance"
        "spora-devnet-production-gate"
        'spora-devnet-${{ inputs.profile }}-acceptance'
        "target/devnet-acceptance/"
    )
    for pattern in "${devnet_required[@]}"; do
        if ! rg --quiet --fixed-strings "$pattern" "$devnet_workflow"; then
            printf 'Spora devnet acceptance workflow is missing required artifact boundary: %s\n' "$pattern" >&2
            exit 1
        fi
    done
}

check_v1_code_boundaries() {
    local required=(
        'exec/src/vm/syscalls/mod.rs::pub const LOAD_SCRIPT_SYSCALL_NUMBER: u64 = 2075;'
        'exec/src/vm/syscalls/mod.rs::pub const CKB_LOAD_SCRIPT_SYSCALL_NUMBER: u64 = 2052;'
        'exec/src/vm/syscalls/mod.rs::pub const SOURCE_GROUP_FLAG: u64 = 0x0100_0000_0000_0000;'
        'exec/src/vm/syscalls/mod.rs::assert_eq!(Source::parse_for_semantics(0x0100, crate::vm::VmSemantics::CkbStrict), None);'
        'exec/src/vm/syscalls/load_script.rs::VmSemantics::CkbStrict => CKB_LOAD_SCRIPT_SYSCALL_NUMBER'
        'exec/src/vm/mod.rs::Whether Spora-only helper syscalls in the `3001..3004` range are exposed.'
        'cellscript/src/stdlib/mod.rs::TargetProfile::Ckb => 2052'
        'cellscript/src/stdlib/mod.rs::TargetProfile::Spora | TargetProfile::PortableCell => 2075'
        'cellscript/src/codegen/mod.rs::const SPORA_SECP256K1_VERIFY_SYSCALL_NUMBER: u64 = 3002;'
        'cellscript/src/codegen/mod.rs::const SPORA_LOAD_ECDSA_SIGNATURE_HASH_SYSCALL_NUMBER: u64 = 3004;'
        'cellscript/src/codegen/mod.rs::source_group_input: CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT'
        'cellscript/src/codegen/mod.rs::source if source == (CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_OUTPUT) => "GroupOutput"'
        'cellscript/src/codegen/mod.rs::fn reject_unresolved_calls'
        'cellscript/src/codegen/mod.rs::fn encode_large_li_sequence'
        'cellscript/src/codegen/mod.rs::fn emit_entry_direct_wrapper'
        'cellscript/src/codegen/mod.rs::struct MachineLayoutPlan'
        'cellscript/src/codegen/mod.rs::cfg: MachineCfg'
        'cellscript/src/codegen/mod.rs::struct BackendLayoutMetrics'
        'cellscript/src/codegen/mod.rs::pub struct BackendShapeMetrics'
        'cellscript/src/codegen/mod.rs::Serialize'
        'cellscript/src/codegen/mod.rs::pub fn analyze_backend_shape'
        'cellscript/src/codegen/mod.rs::struct MachineBlock'
        'cellscript/src/codegen/mod.rs::struct MachineCfg'
        'cellscript/src/codegen/mod.rs::struct MachineBlockCoverage'
        'cellscript/src/codegen/mod.rs::fn validate_machine_block_coverage'
        'cellscript/src/codegen/mod.rs::struct MachineLayoutOrder'
        'cellscript/src/codegen/mod.rs::struct MachinePlacedBlock'
        'cellscript/src/codegen/mod.rs::fn machine_layout_order'
        'cellscript/src/codegen/mod.rs::fn build_machine_layout_order'
        'cellscript/src/codegen/mod.rs::fn validate_machine_layout_order'
        'cellscript/src/codegen/mod.rs::struct MachineCfgEdge'
        'cellscript/src/codegen/mod.rs::enum MachineCfgEdgeKind'
        'cellscript/src/codegen/mod.rs::MachineCfgEdgeKind::Call'
        'cellscript/src/codegen/mod.rs::machine_call_edge_count'
        'cellscript/src/codegen/mod.rs::enum MachineTerminator'
        'cellscript/src/codegen/mod.rs::fn machine_layout_plan_reports_branch_relaxation_metrics()'
        'cellscript/src/codegen/mod.rs::fn machine_layout_plan_builds_explicit_machine_blocks()'
        'cellscript/src/codegen/mod.rs::fn machine_cfg_tracks_call_edges_to_local_helpers()'
        'cellscript/src/codegen/mod.rs::fn machine_reachability_uses_entry_label_not_every_global()'
        'cellscript/src/codegen/mod.rs::fn machine_layout_order_rejects_missing_duplicate_or_unknown_blocks()'
        'cellscript/src/codegen/mod.rs::fn machine_layout_plan_rejects_branch_target_outside_text()'
        'cellscript/tests/examples.rs::fn bundled_examples_stay_within_backend_shape_budgets()'
        'cellscript/tests/examples.rs::struct BackendShapeReportRow'
        'cellscript/tests/examples.rs::fn bundled_examples_backend_shape_report_serializes()'
        'cellscript/tests/examples.rs::fn bundled_examples_stay_near_backend_shape_release_baseline()'
        'cellscript/tests/examples.rs::fn bundled_examples_emit_molecule_schema_manifest_report()'
        'cellscript/tests/examples.rs::CELLSCRIPT_BACKEND_SHAPE_REPORT'
        'cellscript/tests/examples.rs::CELLSCRIPT_MOLECULE_SCHEMA_MANIFEST_REPORT'
        'cellscript/tests/backend_shape_baseline.json::"example": "token.cell"'
        'cellscript/tests/examples.rs::analyze_backend_shape'
        'cellscript/tests/examples.rs::max_relaxed_branches'
        'cellscript/tests/examples.rs::max_cond_branch_abs_distance'
        'cellscript/tests/examples.rs::max_machine_block_bytes'
        'cellscript/tests/examples.rs::max_call_edges'
        'cellscript/tests/examples.rs::machine_call_edge_count'
        'cellscript/tests/examples.rs::max_unreachable_machine_blocks'
        'cellscript/tests/examples.rs::unreachable_machine_block_count'
        'cellscript/tests/examples.rs::max_fail_handlers'
        'scripts/cellscript_dual_chain_release_gate.sh::target/cellscript-backend-shape/backend-shape-report-$MODE.json'
        'scripts/cellscript_dual_chain_release_gate.sh::target/cellscript-schema-manifest/schema-manifest-report-$MODE.json'
        'scripts/cellscript_dual_chain_release_gate.sh::CellScript backend shape report:'
        'scripts/cellscript_dual_chain_release_gate.sh::CellScript Molecule schema manifest report:'
        'scripts/cellscript_dual_chain_release_gate.sh::scripts/validate_cellscript_tooling_release.py'
        'scripts/validate_cellscript_tooling_release.py::valid CellScript tooling release boundary'
        'scripts/cellscript_dual_chain_release_gate.sh::.github/workflows/spora-devnet-acceptance.yml'
        'scripts/cellscript_dual_chain_release_gate.sh::scripts/spora_cellscript_acceptance.sh'
        'scripts/cellscript_dual_chain_release_gate.sh::scripts/regenerate_snapshots.sh'
        'scripts/cellscript_dual_chain_release_gate.sh::scripts/regenerate_test_data.sh'
        'scripts/spora_cellscript_acceptance.sh::mark_json_status "$BASE_REPORT_JSON" "passed"'
        'scripts/spora_cellscript_acceptance.sh::"status": os.environ["RESULT"]'
        'cellscript/src/lib.rs::const VM_ABI_TRAILER_MAGIC: &[u8; 8] = b"SPORABI\0";'
        'cellscript/src/lib.rs::scheduler_witness_borsh_hex is not public scheduler witness metadata'
        'cellscript/src/lib.rs::fn compile_rejects_spora_claim_signature_helpers_under_ckb_profile()'
        'cellscript/src/lib.rs::pub molecule_schema_manifest: MoleculeSchemaManifestMetadata'
        'cellscript/src/lib.rs::fn molecule_schema_manifest_metadata'
        'cellscript/src/lib.rs::fn validate_molecule_schema_manifest_metadata'
        'cellscript/src/lib.rs::fn compile_metadata_exposes_authoritative_molecule_schema_manifest()'
        'cellscript/src/lib.rs::fn compile_lowers_ckb_group_source_large_immediate_to_riscv_elf()'
        'cellscript/src/lib.rs::fn compile_prefers_no_arg_main_for_entry_wrapper()'
        'cellscript/src/cli/commands.rs::fn validate_expected_target_profile'
        'cellscript/src/cli/commands.rs::expect_target_profile: m.get_one::<String>("expect-target-profile").cloned(),'
        'cellscript/src/cli/commands.rs::fn prune_locked_dependencies(removed: &[String]) -> Result<()>'
        'cellscript/src/package/mod.rs::pub fn replace_with_resolved(&mut self, resolved: &HashMap<String, ResolvedPackage>)'
        'cellscript/src/package/mod.rs::pub fn consistency_issues(&self, manifest: &PackageManifest) -> Vec<String>'
        'cellscript/tests/cli.rs::fn cellc_build_accepts_pure_ckb_target_profile_without_sporabi_trailer()'
        'cellscript/tests/cli.rs::fn cellc_install_path_updates_lockfile_and_remove_prunes_it()'
        'cellscript/tests/cli.rs::.arg("--expect-target-profile")'
        'cellscript/tests/cli.rs::assert!(!artifact.ends_with(b"SPORABI'
        'wallet/core/src/tx/generator/settings.rs::CellScript metadata action'
        'wallet/core/src/tx/generator/settings.rs::legacy scheduler_witness_borsh_hex is not public scheduler witness metadata'
        'wallet/core/src/wasm/tx/generator/generator.rs::headerDeps'
        'wallet/core/src/wasm/tx/generator/generator.rs::cellDeps'
        'wallet/core/src/wasm/tx/generator/generator.rs::ckbTypeIdOutputs'
        'scripts/ckb_cellscript_acceptance.sh::Usage: scripts/ckb_cellscript_acceptance.sh [--ckb-repo <path>] [--ckb-bin <path>] [--compile-only]'
        'scripts/ckb_cellscript_acceptance.sh::artifact_has_sporabi_trailer'
        'scripts/ckb_cellscript_acceptance.sh::collect_spendable_cellbases'
        'scripts/ckb_cellscript_acceptance.sh::ckb-default-hash'
        'scripts/ckb_cellscript_acceptance.sh::dry_run_transaction'
        'scripts/ckb_cellscript_acceptance.sh::bundled_examples_exact_order'
        'scripts/ckb_cellscript_acceptance.sh::strict_original_ckb_compile_policy_fail_closed'
        'scripts/ckb_cellscript_acceptance.sh::strict_original_ckb_compile_unexpected_failures'
        'scripts/ckb_cellscript_acceptance.sh::all_artifacts_deployed_and_spent'
        'scripts/ckb_cellscript_acceptance.sh::all_token_actions_exercised'
        'scripts/ckb_cellscript_acceptance.sh::all_nft_actions_exercised'
        'scripts/ckb_cellscript_acceptance.sh::all_timelock_actions_exercised'
        'scripts/ckb_cellscript_acceptance.sh::incomplete token action coverage'
        'scripts/ckb_cellscript_acceptance.sh::incomplete nft action coverage'
        'scripts/ckb_cellscript_acceptance.sh::incomplete timelock action coverage'
        'scripts/ckb_cellscript_acceptance.sh::malformed_spend_without_code_dep'
        'scripts/ckb_cellscript_acceptance.sh::policy_or_capacity_reason'
        'scripts/ckb_cellscript_acceptance.sh::send_test_transaction'
        'scripts/ckb_cellscript_acceptance.sh::valid_spend_dry_run'
    )
    local forbidden=(
        "const CKB_SECP256K1_VERIFY_SYSCALL_NUMBER"
        "const CKB_LOAD_ECDSA_SIGNATURE_HASH_SYSCALL_NUMBER"
        "TargetProfile::Ckb => 2075"
        "TargetProfile::Spora | TargetProfile::PortableCell => 2052"
    )
    local files=(
        "exec/src/vm/syscalls/mod.rs"
        "exec/src/vm/syscalls/load_script.rs"
        "exec/src/vm/mod.rs"
        "cellscript/src/stdlib/mod.rs"
        "cellscript/src/codegen/mod.rs"
        "cellscript/src/lib.rs"
        "cellscript/src/cli/commands.rs"
        "cellscript/src/package/mod.rs"
        "cellscript/tests/cli.rs"
        "wallet/core/src/tx/generator/settings.rs"
        "wallet/core/src/wasm/tx/generator/generator.rs"
        "scripts/ckb_cellscript_acceptance.sh"
    )

    local item file pattern
    for item in "${required[@]}"; do
        file="${item%%::*}"
        pattern="${item#*::}"
        if ! rg --quiet --fixed-strings "$pattern" "$file"; then
            printf 'v1 code boundary check is missing required pattern in %s: %s\n' "$file" "$pattern" >&2
            exit 1
        fi
    done

    for pattern in "${forbidden[@]}"; do
        if rg --quiet --fixed-strings "$pattern" "${files[@]}"; then
            printf 'v1 code boundary check found forbidden pattern: %s\n' "$pattern" >&2
            rg -n --fixed-strings "$pattern" "${files[@]}" >&2
            exit 1
        fi
    done
}

run_quick_gate() {
    run cargo fmt -p cellscript -p spora-adaptor --check
    run cargo check --locked -p cellscript -p spora-adaptor --all-targets
    run cargo test --locked -p cellscript -- --test-threads=1
    run cargo test --locked -p spora-adaptor -- --test-threads=1
    run cargo run --locked -p spora-adaptor --example adaptor_roundtrip
    run cargo run --locked -p spora-adaptor --example adaptor_reject_noncanonical
    run cargo test --locked -p spora-wallet-core cellscript -- --nocapture
    run cargo test --locked -p spora-wallet-core ckb_type_id -- --nocapture
    run cargo test --locked -p spora-wallet-core generator_settings_cell_and_header_deps_are_included_in_unsigned_transactions -- --nocapture
    run python3 scripts/validate_cellscript_tooling_release.py
    run ./scripts/ckb_cellscript_acceptance.sh --compile-only
    run git diff --check
    check_trailing_whitespace
    printf '\nCellScript backend shape report: %s\n' "$CELLSCRIPT_BACKEND_SHAPE_REPORT"
    printf 'CellScript Molecule schema manifest report: %s\n' "$CELLSCRIPT_MOLECULE_SCHEMA_MANIFEST_REPORT"
}

run_full_gate() {
    run cargo fmt --all --check
    run cargo check --locked --workspace --all-targets
    run cargo test --locked -p cellscript -- --test-threads=1
    run cargo test --locked -p spora-adaptor -- --test-threads=1
    run cargo run --locked -p spora-adaptor --example adaptor_roundtrip
    run cargo run --locked -p spora-adaptor --example adaptor_reject_noncanonical
    run cargo test --locked -p spora-exec scheduler_witness -- --nocapture
    run cargo test --locked -p spora-exec prop_cellscript_scheduler_ -- --nocapture
    run cargo test --locked -p spora-consensus --lib trusted_access_set_path -- --nocapture
    run cargo test --locked -p spora-consensus --lib template_scheduler_policy -- --nocapture
    run cargo test --locked -p spora-wallet-core cellscript -- --nocapture
    run cargo test --locked -p spora-wallet-core ckb_type_id -- --nocapture
    run cargo test --locked -p spora-wallet-core generator_settings_cell_and_header_deps_are_included_in_unsigned_transactions -- --nocapture
    run cargo test --locked -p spora-wallet-core attach_cellscript_compiled_scheduler_witness -- --nocapture
    run cargo test --locked -p spora-mining scheduler -- --nocapture --test-threads=1
    run python3 scripts/validate_cellscript_tooling_release.py
    run ./scripts/ckb_cellscript_acceptance.sh --compile-only
    run git diff --check
    check_trailing_whitespace
    printf '\nCellScript backend shape report: %s\n' "$CELLSCRIPT_BACKEND_SHAPE_REPORT"
    printf 'CellScript Molecule schema manifest report: %s\n' "$CELLSCRIPT_MOLECULE_SCHEMA_MANIFEST_REPORT"
}

case "$MODE" in
    quick)
        run_quick_gate
        ;;
    full)
        run_full_gate
        ;;
    v1)
        check_dual_chain_production_plan
        check_v1_public_docs_boundaries
        check_v1_ci_workflow
        check_v1_code_boundaries
        run_full_gate
        ;;
    *)
        printf 'usage: %s [quick|full|v1]\n' "$0" >&2
        exit 2
        ;;
esac

printf '\nCellScript dual-chain %s release gate passed.\n' "$MODE"
