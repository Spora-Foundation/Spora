# CellScript V1 Feature Completeness Audit

**Snapshot date**: 2026-04-19
**Audit direction**: feature-first / reverse audit
**Scope**: CellScript v1 release promise, not the full design proposal

This document audits CellScript from the user-visible feature surface backwards
into implementation, tests, and release gates. It answers a narrower question
than "is the whole design complete?":

> If a user starts from the documented v1 features, is each feature complete
> enough for the v1 promise, or explicitly bounded as post-v1 / fail-closed /
> runtime-required debt?

## Verdict

No new P0 blocker was found inside the current v1 release scope.

The v1 feature set is complete only under the bounded release promise in
`CELLSCRIPT_V1_RELEASE_SCOPE.md`: supported paths are executable or
`checked-runtime`, while unsupported stateful semantics are rejected, fail
closed, or exposed as `runtime-required` with stable blocker classes.

CellScript is **not** complete as a full generalized protocol language, full CKB
contract compatibility layer, automatic transaction builder, first-class AMM /
Pool language, executable Wasm backend, registry package ecosystem, or dynamic
schema migration system.

## Audit Labels

- `V1-complete`: complete for the current public v1 promise and covered by the
  release gate.
- `V1-bounded`: usable, but only for a documented subset; unsupported shapes
  are fail-closed, policy-visible, or post-v1.
- `Post-v1`: intentionally not part of the v1 promise and must stay rejected or
  explicitly out of scope.

## Feature Matrix

| User-visible feature | V1 status | Completeness judgement |
|---|---:|---|
| Single-file and package compilation | V1-complete | `.cell`, package directory, and `Cell.toml` inputs are on the trusted path. Local source roots and local path dependencies are covered; registry/Git dependencies are post-v1/fail-closed. |
| CLI local workflow | V1-complete | `build`, `check`, `metadata`, `verify-artifact`, `test`, `doc`, `fmt`, `init`, `add`, `remove`, `clean`, and `info` are part of the local v1 workflow. `run` and REPL remain utility/limited surfaces, not release-critical execution promises. |
| RISC-V output | V1-bounded | RISC-V assembly is the stable output path. RISC-V ELF is supported for the restricted subset covered by codegen and artifact validation. Full lowering for every language construct is not claimed. |
| `spora` target profile | V1-complete | Spora remains the default native profile with Spora hash domains, DAG/header semantics, scheduler metadata, and Spora ELF trailer behavior. Public VM/CellScript ABI surfaces use Molecule. |
| `ckb` target profile | V1-bounded | Pure supported-subset artifacts can be emitted with CKB syscall/profile/hash/header/packaging rules and no `SPORABI` trailer. Stateful/generalized CKB contract compatibility and automatic CKB transaction builder integration remain post-v1. |
| `portable-cell` profile | V1-bounded | It is a source portability classifier, not a runtime target. It accepts pure portable source and fixed-width Molecule schemas, and rejects target-specific assumptions. |
| `resource` declarations and linear ownership | V1-complete | Resource declarations, capabilities, linear movement, branch/block/match ownership merging, loop restrictions, aggregate linear classification, and no silent discard are covered for the v1 language subset. |
| `shared` declarations and mutation metadata | V1-bounded | Shared state is metadata/scheduler-visible and has checked replacement identity/preservation for supported shapes. General shared-state transition proofs and Pool-specific admission/economics remain runtime-required/post-v1. |
| `receipt` declarations and claim mapping | V1-bounded | Receipt declaration, claim output mapping, lifecycle metadata, and signer-field claim checks are covered for supported conventions. General claim authorization policy remains bounded by runtime-required blocker classes. |
| `action` definitions | V1-complete | Actions are parsed, typed, effect-inferred, lowered, metadata-emitted, and policy-gated for the supported subset. Unsupported stateful paths are visible instead of silent. |
| `fn` helper definitions | V1-bounded | Pure helper functions are real and gated against hidden Cell/runtime effects. Full cross-module/general language function semantics are still partial. |
| `lock` definitions | V1-bounded | Lock-style predicates compile for supported read-only authorization paths. Stateful transitions inside locks are rejected. Broader lock-script authoring policy is not a complete framework. |
| `consume` and direct input data loading | V1-complete | Operation-tagged consumed input metadata and checked input data-load components exist for covered shapes. |
| `create` and output field verification | V1-bounded | Fixed scalar and schema-backed fixed-byte output verification is covered; unsupported lock/field shapes fail closed or expose stable blocker classes. |
| `transfer` | V1-bounded | Capability checks, operation provenance, supported output field preservation, destination lock/address binding, and blocker classes exist. General multi-cell conservation and unsupported output relations remain bounded. |
| `destroy` | V1-bounded | Named cell-backed destroy can check transaction Output TypeHash absence against the consumed Input's real CKB TypeHash for covered shapes. Broader burn policy remains outside full semantic closure. |
| `claim` | V1-bounded | Supported receipt claim output relation, witness envelope/domain checks, explicit signer-field ECDSA checks, and checked source predicates are covered. Generalized authorization remains runtime-required when unsupported. |
| `settle` | V1-bounded | Supported settle output relation and lifecycle final-state checks are covered. Non-lifecycle/general finalization policy remains runtime-required/post-v1. |
| Lifecycle annotations | V1-bounded | Declaration checks, metadata, static create/reset checks, and fixed-scalar transition checks are covered. Arbitrary lifecycle formulas are not complete. |
| Fixed-width schema metadata | V1-complete | Fixed-width persistent `resource` / `shared` / `receipt` / `struct` layouts emit Molecule `fixed-struct-v1` metadata with hash validation, including nested fixed structs and fixed tuple/array-of-tuple aggregates. |
| Dynamic/versioned schema migration | Post-v1 | Dynamic/table/dynvec/versioned schema evolution is not a v1 promise. |
| Stable type identity | V1-bounded | Stable `type_id` metadata and duplicate rejection exist. CKB TYPE_ID lowering exists for persistent Cell types in the `ckb` profile, with direct-create output plans. Higher-level automatic builder wiring remains post-v1. |
| Metadata and artifact self-validation | V1-complete | Metadata schema/version/hash/source/artifact/trailer/profile/schema consistency is checked before compile results are accepted or written. |
| Policy gates and blocker visibility | V1-complete | CLI policy gates expose and can reject fail-closed paths, symbolic runtime, CKB runtime requirements, runtime-required obligations, target-profile violations, and stable blocker classes. |
| Spora scheduler witness metadata | V1-complete for in-process path | Compiler metadata emits public Molecule scheduler witnesses; Borsh is crate-internal migration/regression coverage only. `ActionMetadata`, exec admission, consensus, mining, and wallet paths reject malformed/legacy/conflicting public inputs as covered by the release gate. |
| External RPC trusted-summary submission | Post-v1 | No authenticated cross-process trusted-summary trust policy is part of v1. This must stay out of scope unless a concrete trust model is added. |
| Wallet generator integration | V1-bounded | Explicit native/WASM settings can attach validated Spora Molecule scheduler witnesses, consume CellScript action metadata, inject CKB deps/header deps, and install CKB TYPE_ID output scripts. Automatic transaction-builder orchestration is post-v1. |
| WASM SDK surface | V1-bounded | WASM generator settings can consume explicit metadata/action/deps/type-id settings. Executable Wasm action/lock backend is post-v1. |
| SDK adaptor examples | V1-complete | Adaptor roundtrip and non-canonical scalar rejection are release-gate evidence for the SDK crypto example surface. |
| Examples | V1-complete as regression inputs | Bundled examples compile and exercise metadata/policy surfaces. They are regression evidence, not a promise that every protocol pattern in those examples has complete generalized semantics. |
| Package registry workflow | Post-v1 | Local package/path dependencies are supported. Registry publish/install/update remains fail-closed until a signed trust model exists. |
| First-class `launch` | Post-v1 | `launch` is reserved and explicitly rejected as a general expression. Controlled launch-like examples are ordinary actions plus explicit metadata obligations, not a v1 primitive. |
| First-class `pool` / AMM economics | Post-v1 | Pool remains a shared-state protocol pattern. Some controlled invariants are checked, but generalized AMM pricing, reserve conservation, admission, and launch-pool atomicity remain runtime-required/post-v1. |

## Reverse Findings

The feature-first audit changes the risk picture in three places:

1. The release is complete as a **bounded compiler/toolchain** release, not as a
   full smart-contract language release.
2. The strongest v1 guarantee is the **classification boundary**: unsupported
   semantics are not hidden. They are checked, rejected, fail-closed, or surfaced
   as `runtime-required` with blocker classes.
3. The highest-risk remaining items are not compiler parsing/codegen basics.
   They are integration/product promises: automatic transaction construction,
   external trusted-summary submission, generalized protocol economics, dynamic
   schema evolution, and full arbitrary CKB stateful compatibility.

## Allowed Release Claims

These claims match the current feature audit:

- CellScript v1 is a Cell lifecycle compiler/toolchain for the supported source
  subset.
- Spora and CKB are both supported through explicit target profiles.
- Public VM/CellScript ABI surfaces use Molecule; legacy Borsh scheduler witness
  bytes are not public admission bytes.
- The Spora scheduler witness path is operational for the in-process
  producer/mining/wallet path.
- CKB artifacts are supported for the pure admitted subset, with CKB-specific
  syscall/hash/header/packaging behavior.
- Unsupported stateful semantics are policy-visible through stable blocker
  classes or fail closed.

## Claims To Avoid

These claims would overstate v1 completeness:

- "CellScript is a complete stateful smart-contract language."
- "CellScript has full CKB contract compatibility for arbitrary programs."
- "Pool / AMM / launch semantics are fully executable language primitives."
- "The wallet automatically builds complete CellScript protocol transactions."
- "External RPC builders can submit trusted scheduler summaries safely."
- "Borsh is a public CellScript ABI compatibility format."
- "Dynamic/versioned user schemas are supported."
- "Executable Wasm action/lock backend is ready."

## Gate Evidence

The v1 gate is the operational proof for this audit:

```bash
CARGO_TARGET_DIR=/tmp/spora-v1-release-gate-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 ./scripts/cellscript_phase4_release_gate.sh v1
```

The gate covers:

- `cargo fmt --all --check`
- `cargo check --workspace --all-targets`
- full `cellscript` library / CLI / examples tests
- SDK adaptor tests and executable examples
- `spora-exec` scheduler witness and property tests
- `spora-consensus` strict trusted-summary and template policy tests
- `spora-wallet-core cellscript` tests, including checked raw Molecule scheduler
  witness configuration
- `spora-wallet-core ckb_type_id` and explicit deps/header deps tests
- `spora-mining` scheduler sidecar lifecycle tests
- `git diff --check`
- release scope, feature-audit, CKB compatibility decision, and status-doc
  boundary checks
- public README / CellScript README overclaim-boundary checks

## Final Completeness Answer

For v1 as scoped: **complete enough to close**, assuming the release notes keep
the same bounded promise.

For the full CellScript vision: **not complete**. The remaining work is
well-classified post-v1 work rather than hidden v1 blocker work.
