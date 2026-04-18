# CellScript Execution Phase Table

**Snapshot date**: 2026-04-18
**Purpose**: maintain the operational execution plan for CellScript implementation work.

This file is the working phase table. It is not a historical changelog. Detailed release evidence belongs in `docs/CELLSCRIPT_RELEASE_CHECKLIST.md`.

## Operating Rule

When the user says `go on`, continue work against the first phase whose status is not `Closed`.

Execution loop:

1. Pick the active phase from the table below.
2. Work through that phase's remaining closure items, including code, tests, and docs when needed.
3. Do not stop at analysis if an actionable implementation step is available.
4. Update this table before reporting back: status, current evidence, and remaining blockers must stay current.
5. If the phase closes, mark it `Closed`, promote the next phase to `Active`, and immediately begin the next phase's first actionable item unless blocked by missing credentials, external dependency, destructive operation, or a user policy decision.

`Archived with debt` means the phase has enough implemented foundation to proceed, but listed debt must be resolved before a dependent later phase can be marked closed.

## Phase Table

| Phase | Status | Exit gate | Remaining closure items | Next `go on` action |
|---|---|---|---|---|
| Phase 0: freeze system contract | Archived with debt | Semantic kernel, primitive list, effect summary shape, and consensus/advisory boundaries are specific enough for compiler work to proceed. | Exact first-class `launch`/`pool`/`mint`/`burn`/`seed_pool`/`swap` semantics are still not fully frozen. This debt blocks full design completion, not Phase 1 compiler MVP closure. | Carry contract-debt checks forward; do not reopen Phase 0 unless the language contract changes. |
| Phase 1: compiler MVP | Closed | Supported source programs parse, type-check, lower to IR, emit `riscv64-asm`/restricted `riscv64-elf`, produce validated metadata, and pass the accepted example/test matrix. Unsupported semantics fail closed and are visible in metadata/policy gates. | None. | Completed; continue in Phase 2. |
| Phase 2: asset lifecycle and shared state core | Closed | `shared`, `receipt`, lifecycle, controlled launch-like composition, `claim`, `settle`, Pool pattern metadata, and scheduler metadata form an end-to-end controlled test flow with unsupported paths either executable or explicitly runtime-required with blocker reasons/classes. | None for the operational exit gate. Generalized claim authorization, non-lifecycle/generalized settle finalization, broader Pool economics/admission, launch/pool atomicity, and post-v1 launch builder semantics remain deferred through stable blocker classes. | Completed; continue in Phase 3. |
| Phase 3: node/scheduler integration | Closed | CellScript-generated metadata/witness material is consumed by transaction building, mempool/template policy, and scheduler conflict analysis without relying on source-level trust. | None for the operational exit gate. External RPC trusted-summary submission is intentionally deferred unless RPC-side builders are admitted into strict template policy. | Completed; continue in Phase 4. |
| Phase 4: production hardening | Closed | Compiler, tools, tests, docs, and release process are stable enough for audited external use. | None for the operational close gate. Final release tag creation remains a separate release action; external trusted-summary submission stays out of scope unless RPC-side builders are admitted with an authenticated trust policy. | No active implementation phase remains. If more release changes land, rerun the full release gate before tagging. |

## Current Phase

No implementation phase is currently active. Phase 4 is operationally closed.

Current close distance: **0% for the Phase 4 operational gate**. There are no known core scheduler/metadata security blockers after the latest full release gate. Release tagging remains a separate release-management action.

Known non-Phase-4 blockers:

- First-class `launch` language primitive.
- First-class `pool` language primitive.
- Registry package install/publish/update.
- Executable Wasm backend for CellScript actions or locks.
- External RPC trusted-summary submission, unless RPC-side builders are explicitly admitted and authenticated.

## Phase 4 Current Evidence

Latest full release gate:

```bash
CARGO_TARGET_DIR=/tmp/spora-phase4-full-gate-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 ./scripts/cellscript_phase4_release_gate.sh full
```

Result: passed.

Current gate coverage:

- `cargo fmt --all --check`
- `cargo check --workspace --all-targets`
- Full `cellscript` tests: `237` library tests, `46` CLI integration tests, `7` bundled example tests, `0` doctests
- `spora-adaptor`: `2` unit tests, `0` doctests, executable `adaptor_roundtrip` and `adaptor_reject_noncanonical` examples
- `spora-exec`: `14` scheduler witness tests and `10` scheduler property tests
- `spora-consensus`: `8` strict trusted-summary tests and `12` template scheduler policy tests
- `spora-wallet-core`: `3` compiled scheduler witness attachment tests
- `spora-mining`: `13` scheduler sidecar lifecycle tests
- `git diff --check`
- Targeted trailing-whitespace checks in the release gate

Security review checklist status: all checklist items in `docs/CELLSCRIPT_RELEASE_CHECKLIST.md` are checked.

## Phase 4 Close Checklist

Phase 4 operational close items:

1. SDK example coverage: closed with `adaptor_roundtrip` and `adaptor_reject_noncanonical`.
2. Final release-candidate hygiene: closed by the full release gate.
3. External RPC trusted-summary submission: out of scope unless an authenticated trust policy is added.
4. Release tag: separate release-management action; rerun the full gate if any further release changes land before tagging.

## Current Close Rule

Phase 4 can close when:

1. Whole-workspace formatting/check/test gates have no known local blockers.
2. Consensus-risky metadata and scheduler paths have adversarial and property/fuzz coverage proportional to their trust surface.
3. Optimizer, LSP, docgen, package, and SDK surfaces have explicit production support or fail-closed boundaries.
4. Security review and release checklist items have reproducible evidence.
5. Final release-candidate gate passes after the last Phase 4 change.

Percentages in audit documents are secondary. This table is authoritative for execution order.
