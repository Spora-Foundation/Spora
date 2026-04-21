#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:-quick}"

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/tmp/spora-cellscript-release-gate-target}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"
export CELLSCRIPT_BACKEND_SHAPE_REPORT="${CELLSCRIPT_BACKEND_SHAPE_REPORT:-$ROOT_DIR/target/cellscript-backend-shape/backend-shape-report-$MODE.json}"

cd "$ROOT_DIR"
mkdir -p "$(dirname "$CELLSCRIPT_BACKEND_SHAPE_REPORT")"

run() {
    printf '\n==> %s\n' "$*"
    "$@"
}

check_trailing_whitespace() {
    local files=(
        ".github/workflows/cellscript-v1.yml"
        "cellscript/CHANGELOG.md"
        "cellscript/README.md"
        "cellscript/README_CN.md"
        "docs/CELLSCRIPT_CKB_COMPATIBILITY_DECISION.md"
        "docs/CELLSCRIPT_COMPATIBILITY_MATRIX.md"
        "docs/CELLSCRIPT_DESIGN_IMPLEMENTATION_AUDIT.md"
        "docs/CELLSCRIPT_EXECUTION_PHASES.md"
        "docs/CELLSCRIPT_IMPLEMENTATION_STATUS.md"
        "docs/CELLSCRIPT_RELEASE_CHECKLIST.md"
        "docs/CELLSCRIPT_V1_FEATURE_COMPLETENESS_AUDIT.md"
        "docs/CELLSCRIPT_V1_RELEASE_SCOPE.md"
        "docs/SPORA_CELLSCRIPT_DEVNET_ACCEPTANCE_PLAN.md"
        "scripts/cellscript_phase4_release_gate.sh"
        "scripts/ckb_cellscript_acceptance.sh"
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

check_v1_release_scope() {
    local doc="docs/CELLSCRIPT_V1_RELEASE_SCOPE.md"
    local required=(
        "first-class \`launch\`"
        "first-class \`pool\`"
        "registry package install/publish/update"
        "executable Wasm action/lock backend"
        "full CKB contract compatibility"
        "legacy Borsh scheduler witnesses are explicit"
        "ActionMetadata public scheduler witness decode rejects legacy Borsh fields"
        "action metadata validated as Molecule scheduler witness bytes"
        "Conflicting Molecule scheduler witness aliases rejected"
        "legacy Borsh scheduler fields rejected on the public wallet metadata path"
        "Raw scheduler witness configuration has a checked wallet setter"
        "Artifact verification can pin the expected target profile"
        "reject Spora/CKB artifact mixups"
        "Wallet generator CellScript integration is explicit."
        "not automatic protocol synthesis"
        "Public README and CellScript README release claims are part of the v1 gate"
        "Borsh must stay legacy-only"
        "for public CellScript/CKB-facing paths"
        "feature-completeness audit and CKB"
        "compatibility decision keep the bounded v1 status"
        "wording avoids known overclaims"
    )

    if [[ ! -f "$doc" ]]; then
        printf 'missing v1 release scope document: %s\n' "$doc" >&2
        exit 1
    fi

    for pattern in "${required[@]}"; do
        if ! rg --quiet --fixed-strings "$pattern" "$doc"; then
            printf 'v1 release scope document is missing required boundary: %s\n' "$pattern" >&2
            exit 1
        fi
    done
}

check_v1_feature_completeness_audit() {
    local doc="docs/CELLSCRIPT_V1_FEATURE_COMPLETENESS_AUDIT.md"
    local required=(
        "No new P0 blocker was found inside the current v1 release scope."
        "V1-complete"
        "V1-bounded"
        "Post-v1"
        "CellScript is **not** complete as a full generalized protocol language"
        "Claims To Avoid"
        "For v1 as scoped: **complete enough to close**"
        "For the full CellScript vision: **not complete**"
        '| Single-file and package compilation | V1-complete |'
        '| CLI local workflow | V1-complete |'
        '| RISC-V output | V1-bounded |'
        '| `spora` target profile | V1-complete |'
        '| `ckb` target profile | V1-bounded |'
        '| `portable-cell` profile | V1-bounded |'
        '| `resource` declarations and linear ownership | V1-complete |'
        '| `shared` declarations and mutation metadata | V1-bounded |'
        '| `receipt` declarations and claim mapping | V1-bounded |'
        '| `action` definitions | V1-complete |'
        '| `fn` helper definitions | V1-bounded |'
        '| `lock` definitions | V1-bounded |'
        '| `consume` and direct input data loading | V1-complete |'
        '| `create` and output field verification | V1-bounded |'
        '| `transfer` | V1-bounded |'
        '| `destroy` | V1-bounded |'
        '| `claim` | V1-bounded |'
        '| `settle` | V1-bounded |'
        '| Lifecycle annotations | V1-bounded |'
        '| Fixed-width schema metadata | V1-complete |'
        '| Dynamic/versioned schema migration | Post-v1 |'
        '| Stable type identity | V1-bounded |'
        '| Metadata and artifact self-validation | V1-complete |'
        '| Policy gates and blocker visibility | V1-complete |'
        '| Spora scheduler witness metadata | V1-complete for in-process path |'
        '| External RPC trusted-summary submission | Post-v1 |'
        '| Wallet generator integration | V1-bounded |'
        '| WASM SDK surface | V1-bounded |'
        '| SDK adaptor examples | V1-complete |'
        '| Examples | V1-complete as regression inputs |'
        '| Package registry workflow | Post-v1 |'
        '| First-class `launch` | Post-v1 |'
        '| First-class `pool` / AMM economics | Post-v1 |'
        "Spora and CKB are both supported through explicit target profiles."
        "Public VM/CellScript ABI surfaces use Molecule"
        "legacy Borsh scheduler witness"
        "CKB artifacts are supported for the pure admitted subset"
        "release scope, feature-audit, CKB compatibility decision, and status-doc"
        "boundary checks"
    )

    if [[ ! -f "$doc" ]]; then
        printf 'missing v1 feature completeness audit: %s\n' "$doc" >&2
        exit 1
    fi

    for pattern in "${required[@]}"; do
        if ! rg --quiet --fixed-strings "$pattern" "$doc"; then
            printf 'v1 feature completeness audit is missing required boundary: %s\n' "$pattern" >&2
            exit 1
        fi
    done
}

check_v1_ckb_compatibility_decision() {
    local doc="docs/CELLSCRIPT_CKB_COMPATIBILITY_DECISION.md"
    local required=(
        "**Status**: V1 bounded compatibility decision"
        "The current implementation has a real CKB artifact profile for the pure admitted CellScript subset."
        'CKB compatibility claims are bounded to artifacts explicitly compiled with the `ckb` profile and admitted by the profile gates.'
        "Full arbitrary CKB contract compatibility remains post-v1 work."
        "the original P0 blockers are closed for the v1 admitted subset"
        "Phase E is implemented for v1 classification and pure-subset admission"
        "Phase F is implemented for the v1 pure subset covered by the release gate"
        "Phase G is implemented for the v1 pure subset"
        "The remaining post-v1 tasks implied by the plan are"
        "2026-04-21 CKB Local Devnet Acceptance"
        "scripts/ckb_cellscript_acceptance.sh"
        "On-chain status: passed."
        "bundled_examples_exact_order"
        "strict_original_ckb_compile_policy_fail_closed"
        "strict_original_ckb_compile_unexpected_failures = []"
        "onchain.all_artifacts_deployed_and_spent = true"
    )
    local forbidden=(
        "not yet a real CKB artifact profile"
        "full CKB contract compatibility only applies to artifacts explicitly compiled"
        "Phase E has started"
        "Phase G has started"
        "The implementation tasks implied by the plan are"
    )

    if [[ ! -f "$doc" ]]; then
        printf 'missing CKB compatibility decision document: %s\n' "$doc" >&2
        exit 1
    fi

    local pattern
    for pattern in "${required[@]}"; do
        if ! rg --quiet --fixed-strings "$pattern" "$doc"; then
            printf 'CKB compatibility decision is missing required v1 status: %s\n' "$pattern" >&2
            exit 1
        fi
    done

    for pattern in "${forbidden[@]}"; do
        if rg --quiet --fixed-strings "$pattern" "$doc"; then
            printf 'CKB compatibility decision contains stale pre-v1 status: %s\n' "$pattern" >&2
            rg -n --fixed-strings "$pattern" "$doc" >&2
            exit 1
        fi
    done
}

check_v1_public_docs_boundaries() {
    local required=(
        "cellscript/README.md::The \`ckb\` profile is intentionally bounded in v1."
        "cellscript/README.md::Bounded ckb-vm artifact profile for the admitted Cell subset"
        "cellscript/README_CN.md::v1 的 \`ckb\` profile 是有边界的 artifact profile。"
        "cellscript/README_CN.md::对受支持 Cell 子集提供有边界的 ckb-vm artifact profile"
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
        "cellscript/README_CN.md"
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

check_v1_status_docs_boundaries() {
    local required=(
        'docs/CELLSCRIPT_IMPLEMENTATION_STATUS.md::V1 pure-subset profile path implemented'
        'docs/CELLSCRIPT_IMPLEMENTATION_STATUS.md::The Phase 3 operational path is closed; remaining work is post-v1 hardening'
        'docs/CELLSCRIPT_COMPATIBILITY_MATRIX.md::CKB artifacts are supported for the pure admitted subset'
        'docs/CELLSCRIPT_COMPATIBILITY_MATRIX.md::Remaining work is post-v1 hardening'
        'docs/CELLSCRIPT_DESIGN_IMPLEMENTATION_AUDIT.md::`ckb` can produce artifacts for the pure supported subset'
        'docs/CELLSCRIPT_DESIGN_IMPLEMENTATION_AUDIT.md::RPC trusted-summary submission/authentication and broader adversarial/property coverage remain post-v1 hardening'
        'docs/SPORA_DSL_DESIGN_PROPOSAL_CN.md::已消费 transaction-admitted witness'
        'docs/SPORA_DSL_DESIGN_PROPOSAL_CN.md::剩余 post-v1 未闭合的是外部提交路径'
    )
    local forbidden=(
        "not yet a real CKB artifact profile"
        "CKB ELF packaging is not implemented"
        "artifact-producing non-spora profiles"
        "Phase E has started"
        "Phase G has started"
        "Remaining work belongs to Phase 4 hardening"
        "Remaining work is Phase 4 hardening"
        "remaining work is Phase 4 hardening"
        "RPC trusted-summary submission remains Phase 4 hardening"
        "release-grade verification gates need to be kept green"
        "add profile-specific CKB artifact generation"
        "已开始消费 transaction-admitted witness"
        "剩余未闭合的是外部提交路径"
    )
    local status_docs=(
        "docs/CELLSCRIPT_IMPLEMENTATION_STATUS.md"
        "docs/CELLSCRIPT_COMPATIBILITY_MATRIX.md"
        "docs/CELLSCRIPT_DESIGN_IMPLEMENTATION_AUDIT.md"
        "docs/SPORA_DSL_DESIGN_PROPOSAL_CN.md"
    )

    local item file pattern
    for item in "${required[@]}"; do
        file="${item%%::*}"
        pattern="${item#*::}"
        if ! rg --quiet --fixed-strings "$pattern" "$file"; then
            printf 'v1 status docs are missing required boundary in %s: %s\n' "$file" "$pattern" >&2
            exit 1
        fi
    done

    for pattern in "${forbidden[@]}"; do
        if rg --quiet --fixed-strings "$pattern" "${status_docs[@]}"; then
            printf 'v1 status docs contain stale pre-v1 wording: %s\n' "$pattern" >&2
            rg -n --fixed-strings "$pattern" "${status_docs[@]}" >&2
            exit 1
        fi
    done
}

check_v1_release_process_docs() {
    local required=(
        'docs/CELLSCRIPT_RELEASE_CHECKLIST.md::docs/CELLSCRIPT_CKB_COMPATIBILITY_DECISION.md'
        'docs/CELLSCRIPT_RELEASE_CHECKLIST.md::original CKB P0 blockers are closed for the v1 admitted subset'
        'docs/CELLSCRIPT_RELEASE_CHECKLIST.md::the feature matrix, the CKB compatibility decision, status-doc boundaries'
        'docs/CELLSCRIPT_RELEASE_CHECKLIST.md::full arbitrary CKB contract compatibility remain outside the v1 promise'
        'docs/CELLSCRIPT_RELEASE_CHECKLIST.md::.github/workflows/cellscript-v1.yml'
        'docs/CELLSCRIPT_RELEASE_CHECKLIST.md::runs `./scripts/cellscript_phase4_release_gate.sh v1`'
        'docs/CELLSCRIPT_RELEASE_CHECKLIST.md::cellc verify-artifact --expect-target-profile'
        'docs/CELLSCRIPT_RELEASE_CHECKLIST.md::reject profile mixups'
        'docs/CELLSCRIPT_RELEASE_CHECKLIST.md::local path install writes `Cell.lock`'
        'docs/CELLSCRIPT_RELEASE_CHECKLIST.md::normal dependency removal prunes stale lock entries'
        'docs/CELLSCRIPT_EXECUTION_PHASES.md::Status: **Closed for the v1 core-language convergence gate**.'
        'docs/CELLSCRIPT_EXECUTION_PHASES.md::Release-v1 can remain closed only if these surfaces stay outside the v1 core'
        'docs/CELLSCRIPT_EXECUTION_PHASES.md::feature-completeness, the feature matrix, CKB-compatibility decision, status-doc'
        'docs/CELLSCRIPT_EXECUTION_PHASES.md::boundaries, and public README overclaim boundaries'
        'docs/SPORA_CELLSCRIPT_DEVNET_ACCEPTANCE_PLAN.md::CKB 本地集成 devnet 验收'
        'docs/SPORA_CELLSCRIPT_DEVNET_ACCEPTANCE_PLAN.md::scripts/ckb_cellscript_acceptance.sh'
        'docs/SPORA_CELLSCRIPT_DEVNET_ACCEPTANCE_PLAN.md::InsufficientCellCapacity(Outputs[0])'
        'docs/SPORA_CELLSCRIPT_DEVNET_ACCEPTANCE_PLAN.md::strict_original_ckb_compile_unexpected_failures == []'
    )
    local forbidden=(
        "because it validates the release-scope boundary before running the full gate"
        "feature-completeness, CKB-compatibility decision, and public README overclaim"
    )
    local docs=(
        "docs/CELLSCRIPT_RELEASE_CHECKLIST.md"
        "docs/CELLSCRIPT_EXECUTION_PHASES.md"
        "docs/SPORA_CELLSCRIPT_DEVNET_ACCEPTANCE_PLAN.md"
    )

    local item file pattern
    for item in "${required[@]}"; do
        file="${item%%::*}"
        pattern="${item#*::}"
        if ! rg --quiet --fixed-strings "$pattern" "$file"; then
            printf 'v1 release-process docs are missing required boundary in %s: %s\n' "$file" "$pattern" >&2
            exit 1
        fi
    done

    for pattern in "${forbidden[@]}"; do
        if rg --quiet --fixed-strings "$pattern" "${docs[@]}"; then
            printf 'v1 release-process docs contain stale gate wording: %s\n' "$pattern" >&2
            rg -n --fixed-strings "$pattern" "${docs[@]}" >&2
            exit 1
        fi
    done
}

check_v1_ci_workflow() {
    local workflow=".github/workflows/cellscript-v1.yml"
    local standalone_workflow="cellscript/.github/workflows/ci.yml"
    local devnet_workflow=".github/workflows/spora-devnet-acceptance.yml"
    local required=(
        "name: CellScript V1 Gate"
        "workflow_dispatch:"
        "rustup toolchain install 1.85.0 --profile minimal --component rustfmt"
        "CARGO_TARGET_DIR: /tmp/spora-v1-release-gate-target"
        "CELLSCRIPT_BACKEND_SHAPE_REPORT:"
        '"cellscript"'
        '"cellscript/**"'
        "cargo check --locked --workspace --all-targets"
        "cargo test --locked -p cellscript -- --test-threads=1"
        "./scripts/cellscript_phase4_release_gate.sh v1"
        "actions/upload-artifact@v4"
        "target/cellscript-backend-shape/"
        "target/ckb-cellscript-acceptance/"
    )
    local forbidden=(
        "./scripts/cellscript_phase4_release_gate.sh quick"
        "./scripts/cellscript_phase4_release_gate.sh full"
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
        "cargo test --locked --manifest-path Cargo.toml -- --test-threads=1"
        "actions/upload-artifact@v4"
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
        '"cellscript"'
        '"cellscript/**"'
        "spora-devnet-smoke-acceptance"
        "spora-devnet-full-acceptance"
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
        'cellscript/src/codegen/mod.rs::fn assembly_with_external_call_stubs'
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
        'cellscript/tests/examples.rs::CELLSCRIPT_BACKEND_SHAPE_REPORT'
        'cellscript/tests/examples.rs::analyze_backend_shape'
        'cellscript/tests/examples.rs::max_relaxed_branches'
        'cellscript/tests/examples.rs::max_cond_branch_abs_distance'
        'cellscript/tests/examples.rs::max_machine_block_bytes'
        'cellscript/tests/examples.rs::max_call_edges'
        'cellscript/tests/examples.rs::machine_call_edge_count'
        'cellscript/tests/examples.rs::max_unreachable_machine_blocks'
        'cellscript/tests/examples.rs::unreachable_machine_block_count'
        'cellscript/tests/examples.rs::max_fail_handlers'
        'scripts/cellscript_phase4_release_gate.sh::target/cellscript-backend-shape/backend-shape-report-$MODE.json'
        'scripts/cellscript_phase4_release_gate.sh::CellScript backend shape report:'
        'cellscript/src/lib.rs::const VM_ABI_TRAILER_MAGIC: &[u8; 8] = b"SPORABI\0";'
        'cellscript/src/lib.rs::const CKB_ACCEPTANCE_SMOKE_POLICY_BYPASS_ENV'
        'cellscript/src/lib.rs::fn ckb_acceptance_smoke_policy_bypass_allowed_for_env'
        'cellscript/src/lib.rs::scheduler_witness_borsh_hex is not public scheduler witness metadata'
        'cellscript/src/lib.rs::fn compile_rejects_spora_claim_signature_helpers_under_ckb_profile()'
        'cellscript/src/lib.rs::fn compile_lowers_ckb_group_source_large_immediate_to_riscv_elf()'
        'cellscript/src/lib.rs::fn ckb_acceptance_smoke_policy_bypass_requires_explicit_env_and_no_arg_u64_main()'
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
        'scripts/ckb_cellscript_acceptance.sh::acceptance_smoke_policy_bypass'
        'scripts/ckb_cellscript_acceptance.sh::all_artifacts_deployed_and_spent'
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
    run ./scripts/ckb_cellscript_acceptance.sh --compile-only
    run git diff --check
    check_trailing_whitespace
    printf '\nCellScript backend shape report: %s\n' "$CELLSCRIPT_BACKEND_SHAPE_REPORT"
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
    run ./scripts/ckb_cellscript_acceptance.sh --compile-only
    run git diff --check
    check_trailing_whitespace
    printf '\nCellScript backend shape report: %s\n' "$CELLSCRIPT_BACKEND_SHAPE_REPORT"
}

case "$MODE" in
    quick)
        run_quick_gate
        ;;
    full)
        run_full_gate
        ;;
    v1)
        check_v1_release_scope
        check_v1_feature_completeness_audit
        check_v1_ckb_compatibility_decision
        check_v1_public_docs_boundaries
        check_v1_status_docs_boundaries
        check_v1_release_process_docs
        check_v1_ci_workflow
        check_v1_code_boundaries
        run_full_gate
        ;;
    *)
        printf 'usage: %s [quick|full|v1]\n' "$0" >&2
        exit 2
        ;;
esac

printf '\nCellScript Phase 4 %s release gate passed.\n' "$MODE"
