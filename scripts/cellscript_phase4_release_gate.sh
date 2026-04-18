#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:-quick}"

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/tmp/spora-cellscript-release-gate-target}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"

cd "$ROOT_DIR"

run() {
    printf '\n==> %s\n' "$*"
    "$@"
}

check_trailing_whitespace() {
    local files=(
        "docs/CELLSCRIPT_EXECUTION_PHASES.md"
        "docs/CELLSCRIPT_RELEASE_CHECKLIST.md"
        "scripts/cellscript_phase4_release_gate.sh"
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
    )

    if rg -n '[ \t]+$' "${files[@]}"; then
        printf '\nTrailing whitespace found in release-gate files.\n' >&2
        exit 1
    fi
}

case "$MODE" in
    quick)
        run cargo fmt -p cellscript -p spora-adaptor --check
        run cargo check -p cellscript -p spora-adaptor --all-targets
        run cargo test -p cellscript -- --test-threads=1
        run cargo test -p spora-adaptor -- --test-threads=1
        run cargo run -p spora-adaptor --example adaptor_roundtrip
        run cargo run -p spora-adaptor --example adaptor_reject_noncanonical
        run git diff --check
        check_trailing_whitespace
        ;;
    full)
        run cargo fmt --all --check
        run cargo check --workspace --all-targets
        run cargo test -p cellscript -- --test-threads=1
        run cargo test -p spora-adaptor -- --test-threads=1
        run cargo run -p spora-adaptor --example adaptor_roundtrip
        run cargo run -p spora-adaptor --example adaptor_reject_noncanonical
        run cargo test -p spora-exec scheduler_witness -- --nocapture
        run cargo test -p spora-exec prop_cellscript_scheduler_ -- --nocapture
        run cargo test -p spora-consensus --lib trusted_access_set_path -- --nocapture
        run cargo test -p spora-consensus --lib template_scheduler_policy -- --nocapture
        run cargo test -p spora-wallet-core attach_cellscript_compiled_scheduler_witness -- --nocapture
        run cargo test -p spora-mining scheduler -- --nocapture --test-threads=1
        run git diff --check
        check_trailing_whitespace
        ;;
    *)
        printf 'usage: %s [quick|full]\n' "$0" >&2
        exit 2
        ;;
esac

printf '\nCellScript Phase 4 %s release gate passed.\n' "$MODE"
