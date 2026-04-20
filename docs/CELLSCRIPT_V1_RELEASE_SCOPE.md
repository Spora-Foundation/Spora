# CellScript V1 Release Scope

**Snapshot date**: 2026-04-19

This document defines the release-v1 promise for CellScript. It is narrower than
the full design proposal and narrower than full CKB compatibility.

For the reverse, feature-first completeness audit, see
[CELLSCRIPT_V1_FEATURE_COMPLETENESS_AUDIT.md](./CELLSCRIPT_V1_FEATURE_COMPLETENESS_AUDIT.md).

## Release Promise

CellScript v1 is a Cell lifecycle compiler/toolchain for the supported source
subset:

- `resource`, `shared`, `receipt`, `action`, `fn`, and `lock`
- `consume`, `create`, `transfer`, `destroy`, `claim`, and `settle`
- fixed-width public CellScript schemas with generated Molecule metadata
- RISC-V assembly output and the supported restricted RISC-V ELF subset
- Spora-native profile support with Molecule VM/public CellScript ABI
- gated CKB profile support for the pure supported subset
- metadata policy gates that keep incomplete semantics explicit

The release promise is not that every generalized protocol semantic is fully
executable. The promise is that admitted v1 paths are either executable,
classified as `checked-runtime`, or explicitly rejected / policy-visible as
`runtime-required` or fail-closed.

## In Scope

- Stable compiler path: source resolution, parse, type check, IR lowering,
  codegen, metadata validation, and local package path dependencies.
- Stable local CLI path: `build`, `check`, `doc`, `fmt`, `metadata`,
  `verify-artifact`, `test`, `init`, `add`, `remove`, `clean`, and `info`.
  Artifact verification can pin the expected target profile so release/CI jobs
  reject Spora/CKB artifact mixups instead of only checking sidecar integrity.
- Release policy gates for production checks, fail-closed runtime paths,
  symbolic runtime paths, CKB runtime access, runtime-required obligations,
  and target-profile portability.
- Scheduler metadata and Molecule-only scheduler witness admission for Spora's
  in-process producer/mining path; legacy Borsh scheduler witnesses are explicit
  migration/regression inputs, not public admission bytes.
- ActionMetadata public scheduler witness decode rejects legacy Borsh fields.
- Wallet generator support for attaching Spora CellScript scheduler witnesses,
  with action metadata validated as Molecule scheduler witness bytes.
  Conflicting Molecule scheduler witness aliases rejected.
  legacy Borsh scheduler fields rejected on the public wallet metadata path.
  Raw scheduler witness configuration has a checked wallet setter for public
  Molecule ABI validation before generator construction.
  CKB TYPE_ID/dependency settings remain explicit and profile-aware.
- CKB compatibility only for artifacts explicitly compiled with the `ckb`
  profile and only where target-profile policy admits the source.
- Public README and CellScript README release claims are part of the v1 gate:
  CKB compatibility must stay described as bounded to the admitted subset,
  Molecule must stay the public VM/CellScript ABI, and Borsh must stay legacy-only
  for public CellScript/CKB-facing paths.

## Out of Scope

These are not v1 release promises:

- first-class `launch`
- first-class `pool`
- user-defined generics / generic persisted schemas
- registry package install/publish/update
- executable Wasm action/lock backend
- generalized AMM economics and generalized Pool admission
- generalized resource-conservation proofs beyond the restricted checked
  subsets
- generalized transfer, claim, settle, and finalization semantics beyond the
  currently checked/fail-closed boundary
- external RPC trusted-summary submission without an authenticated trust policy
- full CKB contract compatibility for arbitrary stateful CellScript programs

## SDK And Tooling Boundary

The SDK-facing release surface is limited to the reviewed local producer path:

- SDK adaptor examples must continue to pass the release gate.
- Wallet generator CellScript integration is explicit. Higher-level automatic
  CellScript transaction-builder orchestration is not a v1 release promise.
- WASM generator settings may pass explicit CKB `cellDeps`, `headerDeps`,
  `ckbTypeIdOutputs`, and `cellscriptMetadata` / `cellscriptAction`, but this
  is still explicit builder configuration, not automatic protocol synthesis.
- Registry/network package commands remain fail-closed until a signed package
  trust model exists.

## Release Gate

Before a v1 tag, rerun:

```bash
CARGO_TARGET_DIR=/tmp/spora-v1-release-gate-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 ./scripts/cellscript_phase4_release_gate.sh v1
```

The `v1` gate first checks that this scope document still contains the required
post-v1 / out-of-scope boundaries, that the feature-completeness audit and CKB
compatibility decision keep the bounded v1 status, and that public README
wording avoids known overclaims. It then runs the full release gate. The full
gate must include the CellScript compiler tests, SDK adaptor examples,
scheduler/trusted-summary tests, wallet CellScript generator tests, and
`git diff --check`.

If any scope item above changes, update this document, the feature completeness
audit, the phase table, and the release checklist before claiming v1 closure.
