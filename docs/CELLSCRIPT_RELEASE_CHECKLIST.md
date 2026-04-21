# CellScript Phase 4 Release Checklist

**Status**: Operationally closed; release tag not created by this checklist.
**Owner surface**: CellScript compiler/tooling plus the scheduler metadata integration points used by execution, consensus, mining, wallet, RPC, and SDK callers.

This checklist is the Phase 4 release evidence index. It is intentionally a gate list, not a changelog.

## Gate Commands

Quick local gate:

```bash
./scripts/cellscript_phase4_release_gate.sh quick
```

Full release gate:

```bash
./scripts/cellscript_phase4_release_gate.sh full
```

V1 tag gate:

```bash
./scripts/cellscript_phase4_release_gate.sh v1
```

The GitHub Actions workflow `.github/workflows/cellscript-v1.yml` runs `./scripts/cellscript_phase4_release_gate.sh v1`
on relevant pull requests, pushes to the release branches, and manual dispatch.
The workflow is intentionally wired to the `v1` mode rather than the weaker
`quick` or `full` modes so the scope/document/profile-boundary checks remain CI
enforced.

All gate commands accept `CARGO_TARGET_DIR`, `CARGO_INCREMENTAL`, and `CARGO_BUILD_JOBS` from the environment. The default target directory is `/tmp/spora-cellscript-release-gate-target` so the gate does not contend with an interactive development build.
The CI runner must also provide `ripgrep` (`rg`), because the v1 release gate uses fixed-string repository boundary checks before it starts the Rust build/test phase. The GitHub Actions workflow installs `ripgrep` alongside the native LLVM/protobuf dependencies and the gate fails fast with `missing required command: rg` if the tool is absent.

## Required Evidence Before Phase 4 Close

- [x] `./scripts/cellscript_phase4_release_gate.sh quick` passes locally on the Phase 4 working tree.
- [x] `./scripts/cellscript_phase4_release_gate.sh full` passes locally on the Phase 4 working tree.
- [x] `cargo fmt --all --check` passes.
- [x] `cargo check --workspace --all-targets` passes without known release-blocking warnings.
- [x] `git diff --check` passes.
- [x] Full `cellscript` tests pass, including library, CLI integration, bundled examples, and doctests.
- [x] `spora-adaptor` tests and all-target check pass.
- [x] `spora-adaptor` executable SDK roundtrip and non-canonical scalar rejection examples run.
- [x] Scheduler witness/admission adversarial tests pass in `spora-exec`.
- [x] Scheduler witness trusted-summary tamper tests pass in `spora-exec` and `spora-consensus`.
- [x] Strict template scheduler policy tests pass in `spora-consensus`.
- [x] Wallet CellScript generator tests pass for action metadata, checked raw Molecule scheduler witness configuration, CKB TYPE_ID output plans, and explicit CKB deps/header deps.
- [x] Mining scheduler sidecar lifecycle tests pass in `spora-mining`.
- [x] `docs/CELLSCRIPT_V1_RELEASE_SCOPE.md` records the product, SDK, and post-v1 boundaries.
- [x] `docs/CELLSCRIPT_V1_FEATURE_COMPLETENESS_AUDIT.md` records the feature-first completeness verdict and overclaim boundaries.
- [x] Public README / CellScript README claims are gated against CKB/full-Molecule/Borsh overclaims.

Latest local evidence, 2026-04-18:

- `CARGO_TARGET_DIR=/tmp/spora-cellscript-tools-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 ./scripts/cellscript_phase4_release_gate.sh quick`
- `CARGO_TARGET_DIR=/tmp/spora-phase4-full-gate-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 ./scripts/cellscript_phase4_release_gate.sh full`

The full local gate passed with `cargo fmt --all --check`, `cargo check --workspace --all-targets`, full `cellscript` tests, `spora-adaptor` tests and executable examples, `spora-exec` scheduler witness access-set/summary/property tests, `spora-consensus` strict trusted-summary and template scheduler policy tests, `spora-mining` scheduler sidecar lifecycle tests, `git diff --check`, and targeted trailing-whitespace checks. As of 2026-04-19, the gate script also includes focused wallet CellScript generator tests for profile-aware action metadata, checked raw Molecule scheduler witness configuration, CKB TYPE_ID output plans, explicit CKB deps/header deps, and v1 scope/feature/CKB/status/public-doc boundary checks; use the `v1` gate before a final tag because it validates the release promise before running the full gate.

Latest SDK example evidence, 2026-04-18:

- `CARGO_TARGET_DIR=/tmp/spora-adaptor-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo run -p spora-adaptor --example adaptor_roundtrip`
- `CARGO_TARGET_DIR=/tmp/spora-adaptor-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo run -p spora-adaptor --example adaptor_reject_noncanonical`
- `CARGO_TARGET_DIR=/tmp/spora-cellscript-tools-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 ./scripts/cellscript_phase4_release_gate.sh quick`

The quick gate now runs both SDK examples. `adaptor_roundtrip` covers the successful proof, partial-signature, completion, recovery, and secret-verification path. `adaptor_reject_noncanonical` covers fail-closed public API behavior for non-canonical secp256k1 scalar byte inputs. The full gate has been rerun after both SDK examples, scheduler-summary security additions, and stale-summary policy hardening; repeat it again before a final release tag if more Phase 4 changes land.

Latest v1 scope evidence, 2026-04-19:

- `docs/CELLSCRIPT_V1_RELEASE_SCOPE.md`
- `docs/CELLSCRIPT_V1_FEATURE_COMPLETENESS_AUDIT.md`
- `docs/CELLSCRIPT_CKB_COMPATIBILITY_DECISION.md`
- `CARGO_TARGET_DIR=/tmp/spora-v1-release-gate-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 ./scripts/cellscript_phase4_release_gate.sh v1`
- `CARGO_TARGET_DIR=/tmp/spora-v1-release-gate-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 ./scripts/cellscript_phase4_release_gate.sh quick`
- `CARGO_TARGET_DIR=/tmp/spora-codex-target cargo test --locked -p spora-wallet-core cellscript -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/spora-codex-target cargo test --locked -p spora-wallet-core ckb_type_id -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/spora-codex-target cargo test --locked -p spora-wallet-core generator_settings_cell_and_header_deps_are_included_in_unsigned_transactions -- --nocapture`

The v1 release scope document fixes the public promise boundary, the feature completeness audit checks the same boundary from user-visible functionality backwards, and the CKB compatibility decision records that the original CKB P0 blockers are closed for the v1 admitted subset by implementation or fail-closed policy. Core Cell lifecycle semantics are closed through checked/runtime/fail-closed classifications, while first-class launch/pool, registry distribution, executable Wasm, generalized AMM economics, generalized conservation, automatic higher-level transaction-builder orchestration, and full arbitrary CKB contract compatibility remain outside the v1 promise. The updated v1 gate passed after validating those scope boundaries, the feature matrix, the CKB compatibility decision, status-doc boundaries, and public README/CellScript README wording against known CKB/full-Molecule/Borsh overclaims, then running the full gate with wallet generator coverage for CellScript metadata, checked raw Molecule scheduler witness configuration, CKB TYPE_ID output plans, and explicit CKB deps/header deps.

Latest scheduler trusted-summary security evidence, 2026-04-18:

- `CARGO_TARGET_DIR=/tmp/spora-phase4-full-gate-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo check -p spora-exec --all-targets`
- `CARGO_TARGET_DIR=/tmp/spora-phase4-full-gate-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo check -p spora-consensus --all-targets`
- `CARGO_TARGET_DIR=/tmp/spora-phase4-full-gate-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo check -p spora-mining --all-targets`
- `CARGO_TARGET_DIR=/tmp/spora-phase4-full-gate-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo check -p spora-wallet-core --all-targets`
- `CARGO_TARGET_DIR=/tmp/spora-phase4-full-gate-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p spora-exec scheduler_witness -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/spora-phase4-full-gate-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p spora-consensus --lib trusted_access_set_path -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/spora-phase4-full-gate-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p spora-consensus --lib template_scheduler_policy -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/spora-phase4-full-gate-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p spora-wallet-core attach_cellscript_compiled_scheduler_witness -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/spora-phase4-full-gate-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p spora-mining scheduler -- --nocapture --test-threads=1`

The strict scheduler sidecar now carries the full decoded CellScript scheduler witness summary, not only operation/source/index/binding_hash access records. Consensus strict mode authenticates effect class, parallelizability, shared-touch multiset, cycle hint, and the access multiset before scheduler merge. Transaction-builder helpers and consensus admission now reject duplicate CellScript scheduler witness slots, including duplicate matching witnesses that would otherwise merge idempotently.

Latest mempool/template scheduler policy evidence, 2026-04-18:

- `CARGO_TARGET_DIR=/tmp/spora-phase4-full-gate-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p spora-consensus --lib trusted_access_set_path -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/spora-phase4-full-gate-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p spora-consensus --lib template_scheduler_policy -- --nocapture`

The strict consensus path now rejects missing trusted summaries, malformed scheduler witnesses, mismatched summaries, shared-touch/effect/access tampering, duplicate unexpected accesses, extra or duplicate scheduler witnesses, and stale trusted summaries left on transactions that no longer carry a CellScript scheduler witness. Plain transactions still pass without a trusted summary when no stale summary is present.

Latest wallet/mining producer path evidence, 2026-04-18:

- `CARGO_TARGET_DIR=/tmp/spora-phase4-full-gate-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p spora-wallet-core attach_cellscript_compiled_scheduler_witness -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/spora-phase4-full-gate-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p spora-mining scheduler -- --nocapture --test-threads=1`

The wallet generator attaches compiled scheduler witnesses only after transaction-shape validation and now has negative coverage proving a mismatched compiled witness is rejected without appending bytes. Mining keeps trusted summaries on the internal producer path, propagates them through selector output, removes them on selector reject, and cleans them on accepted blocks, double-spend removal, RBF replacement, eviction, low-priority expiry, orphan expiry, and orphan promotion.

Latest package/tooling/SDK surface evidence, 2026-04-18:

- `CARGO_TARGET_DIR=/tmp/spora-phase4-full-gate-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 ./scripts/cellscript_phase4_release_gate.sh full`
- `CARGO_TARGET_DIR=/tmp/spora-v1-release-gate-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p cellscript package_manager -- --nocapture --test-threads=1`
- `CARGO_TARGET_DIR=/tmp/spora-v1-release-gate-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p cellscript lockfile_ -- --nocapture --test-threads=1`
- `CARGO_TARGET_DIR=/tmp/spora-v1-release-gate-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p cellscript --test cli cellc_install_path_updates_lockfile_and_remove_prunes_it -- --nocapture`

The full gate covers package dependency fail-closed tests for registry/Git dependencies and local path dependency resolution, local path install writes `Cell.lock`, normal dependency removal prunes stale lock entries, lockfile diagnostics report stale or source/version-mismatched dependencies, docgen HTML escaping for source-controlled metadata, LSP cross-file rename grouping, invalid-name rejection, Unicode identifier-boundary handling, and comment/string-literal skip behavior, SDK adaptor non-canonical scalar rejection, unit roundtrip coverage, and the executable `adaptor_roundtrip` and `adaptor_reject_noncanonical` examples.

Latest artifact and metadata schema security evidence, 2026-04-18:

- `CARGO_TARGET_DIR=/tmp/spora-cellscript-tools-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p cellscript --lib compile_result_validation_rejects -- --nocapture --test-threads=1`
- `CARGO_TARGET_DIR=/tmp/spora-cellscript-tools-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p cellscript cellc_verify_artifact_rejects_metadata_schema_downgrade -- --nocapture --test-threads=1`
- `CARGO_TARGET_DIR=/tmp/spora-cellscript-tools-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p cellscript cellc_verify_artifact_rejects_noncanonical_source_unit_hash -- --nocapture --test-threads=1`
- `CARGO_TARGET_DIR=/tmp/spora-cellscript-tools-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p cellscript cellc_verify_artifact_enforces_expected_hashes -- --nocapture --test-threads=1`
- `CARGO_TARGET_DIR=/tmp/spora-cellscript-tools-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p cellscript --test cli cellc_build_accepts_pure_ckb_target_profile_without_sporabi_trailer -- --nocapture`

The metadata validator rejects both future and older schema versions instead of downgrading, rejects mismatched compiler versions, enforces artifact hash/size/format/profile binding, enforces VM ABI trailer consistency, and now requires source-unit hashes and caller-supplied expected hashes to use canonical lowercase BLAKE3 hex. `cellc verify-artifact --expect-target-profile` additionally lets release/CI jobs pin Spora-vs-CKB intent and reject profile mixups.

## Security Review Items

- [x] Review CellScript metadata schema compatibility and downgrade behavior.
- [x] Review scheduler witness decoding, Molecule-only public admission, `ActionMetadata` default API rejection of legacy Borsh fields, wallet action metadata validation, conflicting Molecule alias rejection, legacy Borsh rejection, operation/source admission, index bounds, full trusted-summary comparison, and shared-touch/effect tamper handling.
- [x] Review mempool/template policy handling for missing, malformed, mismatched, and stale scheduler summaries.
- [x] Review wallet and mining producer paths that attach compiled scheduler witnesses or store trusted summaries.
- [x] Review `cellc verify-artifact` artifact/metadata/source binding checks.
- [x] Review package dependency behavior: local path dependencies are supported; registry/Git dependencies fail closed.
- [x] Review docgen/LSP generated text and rename surfaces for untrusted input handling.
- [x] Review SDK adaptor scalar-input canonicalization and proof/signature edge cases.

## Release Scope Boundaries

The following are not blockers for a Phase 4 operational close if they remain explicitly fail-closed or runtime-required in metadata:

- First-class `launch` language primitive.
- First-class `pool` language primitive.
- Registry package install/publish/update.
- Executable Wasm backend for CellScript actions or locks.
- External RPC trusted-summary submission, unless RPC-side builders are admitted into strict template policy.
- Higher-level automatic CellScript transaction-builder orchestration; v1 wallet generator integration remains explicit/profile-aware.

The following remain blockers until checked or explicitly deferred with an issue and owner:

- Any path that accepts caller-supplied trusted scheduler summaries across a process or network boundary without authentication/trust policy.
- Any production command path that writes artifacts before applying requested metadata policy gates.
- Any executable optimizer rewrite path that runs outside the reviewed `opt_level <= 3` codegen boundary.
- Any generated HTML/doc/IDE edit path that renders or writes untrusted text without escaping or syntax validation.
