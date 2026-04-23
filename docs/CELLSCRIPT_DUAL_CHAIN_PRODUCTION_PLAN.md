# CellScript Dual-Chain Production Plan

**Date**: 2026-04-23
**Status**: Canonical production roadmap
**Scope**: CellScript, Spora profile, CKB profile, package/tooling, acceptance gates

This document replaces the older CellScript v1 scope, feature-audit, execution-phase,
CKB compatibility decision, compatibility matrix, implementation-status, release
checklist, and Spora CellScript devnet acceptance plan documents. Those older
documents described the bounded v1 and Phase 4 closure state. The active goal is
now stronger: production-grade dual-chain usability on both Spora and CKB.

## Current Truth

CellScript is no longer a syntax prototype. It has a real compiler, metadata
sidecars, RISC-V assembly/ELF output, target profiles, package-manager beta,
LSP surfaces, backend shape gates, Molecule VM/public ABI boundaries, and
acceptance scripts.

The current production-readiness verdict is deliberately stricter:

| Area | Current state | Production verdict |
|---|---|---|
| Spora examples | All seven bundled examples compile under `spora`; the current devnet coverage still includes smoke-style deployment/spend checks. | Not production complete until every bundled example has action-specific Spora transaction builders, valid lifecycle transactions, malformed script-logic rejection, and documented mass/cycle boundaries. |
| CKB examples | `token.cell` is strict-admitted under `ckb`; action-scoped CKB artifacts can now be produced for portable entries inside otherwise-unportable examples. Bounded on-chain coverage reaches every strict action. `token.cell` mint/transfer/burn/merge, all non-batch `nft.cell` actions, all non-batch `timelock.cell` actions, all original `multisig.cell` actions, every AMM action/helper, and `launch.cell::simple_launch` now run on CKB local devnet. Batch dynamic collections and launch pool composition still stay fail-closed. | Not production complete. The default CKB acceptance command is now a production gate and must fail until there are no smoke bypasses, no expected fail-closed entries, and every bundled example compiles/runs as original CKB business artifacts. Use `--bounded` only for development coverage. |
| Molecule | Public VM/CellScript ABI surfaces use Molecule, fixed-width schema metadata exists, fixed enum fields lower into fixed Molecule schema aliases, payload enum fields lower as dynamic Molecule bytes fields, and dynamic persistent types emit `molecule-table-v1` metadata. Fixed-width fields inside Molecule tables, fixed-element `Vec<T>.len()`/index/iteration paths for table fields and schema-pointer parameters, selected dynamic table mutation replacement checks, empty dynamic vectors, fixed-element dynamic vector append checks, and constructed local byte vectors can now be decoded for verifier paths; generic dynamic table mutation, batch collection construction, and selected scalar-push byte-vector construction remain fail-closed. | Needs production schema manifest, broader generated dynamic mutation/table preservation decoders, snapshot tests, and builder integration. |
| Package/tooling | Local package workflow, lockfile validation, README/wiki docs, LSP, and CI reports exist. Registry and release distribution are still beta/RC quality. | Needs release packaging, reproducible builds, package verification, and stable CLI workflows. |
| Backend | Branch relaxation, shared fail handlers, machine-block/CFG metrics, call-edge accounting, and backend shape budgets exist. | Usable, but code size, branch distance, and CFG metrics must stay release artifacts. |

## Latest Local Verification

Last updated: 2026-04-23.

The latest local dual-chain verification established bounded development
coverage, not production readiness:

- Spora full devnet acceptance passed, including in-process smoke, external
  `sporad` boot/probe with 101 preallocated cells, propagation, and focused
  CellScript/package tests.
- Spora smoke reports now record explicit `status: passed` in both the smoke
  report and the aggregate acceptance report.
- CKB local devnet bounded acceptance passed against the parent CKB checkout for
  the legacy smoke deployment/spend path and strict action harnesses that have
  complete CKB witnesses/input/output construction today. This is no longer the
  production gate.
- `scripts/ckb_cellscript_acceptance.sh` defaults to production mode. Production
  mode fails closed if any coverage still depends on smoke bypasses,
  standalone/portable harnesses, expected fail-closed entries, or non-original
  artifacts. Use `scripts/ckb_cellscript_acceptance.sh --bounded` only for the
  development coverage matrix.
- Latest production compile-only gate failed as expected because the tracked
  production blockers still exist:
  `target/ckb-cellscript-acceptance/20260423-080841-44395/ckb-cellscript-acceptance-report.json`.
  The remaining strict original bundled-example policy failures are
  `amm_pool.cell`, `launch.cell`, `nft.cell`, and `timelock.cell`.
- CKB scoped artifact coverage is now a hard compile/verify gate:
  - latest bounded full report:
    `target/ckb-cellscript-acceptance/20260423-080754-42844/ckb-cellscript-acceptance-report.json`;
  - latest bounded compile-only report:
    `target/ckb-cellscript-acceptance/20260423-080743-42069/ckb-cellscript-acceptance-report.json`;
  - original scoped actions admitted: 40;
  - original scoped locks admitted: 15;
  - expected original scoped action gaps fail-closed by policy: 3;
  - expected original scoped lock gaps fail-closed by policy: 0.
- CKB acceptance now emits `ckb_business_coverage`, which compares source
  action/lock definitions against strict CKB compile coverage and real CKB
  on-chain action harness coverage. The matrix is source-validated at runtime,
  so adding or removing an example action/lock without updating the production
  coverage expectations fails the gate.
- Latest bounded CKB coverage is action-complete under the development matrix:
  - source actions: 43;
  - strict CKB actions: 40;
  - expected fail-closed actions: 3;
  - source locks: 15;
  - strict CKB locks: 15;
  - real on-chain CKB action harnesses: 40;
  - `ckb_business_coverage.status: complete`;
  - `ckb_business_coverage.missing_ckb_onchain_actions: {}`.
- CKB action harness coverage remains intentionally narrower than scoped
  compile coverage and is therefore not a production claim:
  - original scoped token harnesses cover `mint`, `transfer_token`, `burn`,
    and `merge` from `cellscript/examples/token.cell`;
  - original scoped NFT harnesses cover `mint`, `transfer`, `create_listing`,
    `cancel_listing`, `buy_from_listing`, `create_offer`, `accept_offer`,
    and `burn` from `cellscript/examples/nft.cell`;
  - original scoped timelock harnesses cover `create_absolute_lock`,
    `create_relative_lock`, `lock_asset`, `request_release`,
    `request_emergency_release`, `approve_emergency_release`,
    `execute_release`, `execute_emergency_release`, and `extend_lock` from
    `cellscript/examples/timelock.cell`;
  - original scoped multisig harnesses cover `create_wallet`,
    `propose_transfer`, `add_signature`, `propose_remove_signer`,
    `propose_add_signer`, `propose_change_threshold`, `execute_proposal`,
    and `cancel_proposal` from `cellscript/examples/multisig.cell`;
  - original scoped launch harness covers `simple_launch` from
    `cellscript/examples/launch.cell`;
  - original scoped AMM harnesses cover `seed_pool`, `swap_a_for_b`,
    `add_liquidity`, `remove_liquidity`, `isqrt`, and `min` from
    `cellscript/examples/amm_pool.cell`;
  - original scoped vesting harnesses cover `create_vesting_config`,
    `grant_vesting`, `claim_vested`, and `revoke_grant`.
- Entry witness ABI now supports scalar arguments that spill past a0-a7 onto
  the caller stack. Schema-backed and fixed-byte pointer/length arguments still
  fail closed if their two-slot ABI pair would cross the register boundary.
- CKB entry-scoped compilation is available through `--entry-action` and
  `--entry-lock`. It narrows IR, metadata, and target-profile policy to the
  selected entry and its reachable pure functions/types, so portable actions or
  locks can produce CKB artifacts without admitting unrelated dynamic entries
  from the same file.
- Fixed enum fields are now represented in fixed Molecule schema metadata as a
  one-byte enum tag alias, closing the previous false blocker for entries such
  as `TimeLock.lock_type`. Payload enums remain dynamic and fail closed until
  their Molecule layout and verifier semantics are implemented.
- Entry-scoped type closure now keeps inline `Vec<T>` element dependencies, so
  scoped CKB compiles retain nested fixed structs such as `Vec<Signature>` in
  generated Molecule schemas.
- Standalone CellScript and the Spora submodule have matching source changes
  for the entry witness ABI, entry-scoped compile, and fixed enum schema fixes.
- The CKB acceptance script now records both positive scoped coverage and
  expected fail-closed scoped gaps. A gap entry that starts compiling is treated
  as a failing gate until its transaction harness and malformed matrix are
  reviewed and the matrix is updated deliberately.
- Dynamic persistent layouts no longer use fake offset-0 field access. Read-only
  fixed-width table fields are decoded through Molecule offsets; mutable state
  transitions for types whose fixed encoded size is unknown still report
  explicit mutable-state runtime requirements until dynamic preserved-field
  verification exists.
- Dynamic persistent types now still receive `molecule-table-v1` schema metadata
  with explicit `dynamic_fields`, so package/build tooling can see the intended
  table layout. CKB verifier codegen now supports read-only fixed-width field
  access through Molecule table offsets, which admits `nft.cell::collection_creator`
  as an original scoped CKB lock. CKB verifier codegen also supports read-only
  fixed-element Molecule vector length, index, and iteration checks for table
  fields, which admits `timelock.cell::emergency_approved`,
  `multisig.cell::is_signer_lock`, and the read-only `multisig.cell` proposal
  locks. Payload enum fields are represented as dynamic Molecule bytes fields
  rather than one-byte enum tags, which admits read-only fixed-field paths such
  as `timelock.cell::asset_matches`, `execute_release`, and
  `execute_emergency_release`. Dynamic Molecule table mutation now has
  table-aware preserved-field equality and fixed scalar transition checks for
  selected replacement-output paths, which admits `nft.cell::mint` and removes
  mutable-state debt from multisig proposal/signature mutation metadata.
  Dynamic Molecule table create-output verification can now compare dynamic
  output fields against schema-pointer entry arguments, which admits
  `timelock.cell::lock_asset`. Fixed-element Molecule vector length/index over
  schema-pointer entry parameters now also supports duplicate-signer guards,
  which admits `multisig.cell::create_wallet`. Dynamic Molecule table
  create-output verification now also checks fixed/scalar table fields through
  Molecule field offsets instead of fixed-struct offsets, which lets original
  `multisig.cell::create_wallet` run on-chain.
  Scalar create-output and mutate-transition verifier paths now preserve
  decoded actual values across expected-expression evaluation and dynamic table
  output decoding, which lets original `multisig.cell::propose_transfer`
  verify `Proposal.proposal_id` and `MultisigWallet.nonce` on-chain.
  Empty dynamic vectors, fixed-element vector append checks, and local
  constructed byte-vector outputs now cover selected create/mutate paths:
  `multisig.cell::propose_transfer`, `multisig.cell::add_signature`,
  `multisig.cell::propose_remove_signer`,
  `multisig.cell::propose_change_threshold`,
  `timelock.cell::request_emergency_release`, and
  `timelock.cell::approve_emergency_release` are strict-admitted. CKB action
  harness coverage now matches scoped compile coverage under the bounded
  development matrix. This is still not a production claim while any expected
  fail-closed entry, smoke bypass artifact, or full-file strict original policy
  failure remains.
- CKB target-profile policy now treats Spora scheduler touch metadata as
  metadata, not as an automatic portability blocker. A shared create/read/mutate
  path is rejected only when its actual state semantics remain runtime-required.
  This admits `vesting.cell::create_vesting_config`, whose shared create output
  fields and lock binding are verifier-covered.
- AMM `seed_pool` now runs as an original scoped CKB harness with real Token
  inputs, Pool and LPReceipt outputs, token-pair admission, positive reserve
  checks, fee bounds, LP supply coupling, output lock binding, and malformed
  output rejection. This closure exposed and fixed two compiler bugs: scoped
  entry artifacts did not retain called action helpers such as `isqrt`, and
  mutable `let` bindings could alias a parameter stack slot (`let mut x = n`)
  and corrupt helper semantics. `add_liquidity` closure then exposed and fixed
  two CKB entry/runtime ABI bugs: stack-spilled fixed-byte parameters past a0-a7
  were fail-closed, and runtime-loaded cell `type_hash()` values used scratch
  storage whose size word could be overwritten before output-field coupling.
  `remove_liquidity` closure then proved the same typed runtime path for
  LPReceipt burn, Pool reserve/LP supply subtraction, Token withdrawal outputs,
  and malformed withdrawal rejection. `swap_a_for_b` closure then made AMM swap
  resource conservation CKB-admitted through checked pool symbol admission,
  fee accounting, constant-product pricing, TokenB output verification, Pool
  reserve replacement, and malformed swap output rejection. AMM now has no
  expected fail-closed scoped actions under the bounded CKB matrix.
- `env::current_timepoint()` is the cross-chain time API. It lowers to Spora
  DAA score under the Spora target profile and to the CKB header epoch number
  under the CKB target profile. `env::current_daa_score()` remains Spora-only
  and still fails CKB policy.
- Fixed-byte schema field comparisons now preserve both source pointers across
  verifier bounds checks before calling the shared memcmp helper. This fixed the
  CKB on-chain `token.merge` harness, where `a.symbol == b.symbol` previously
  failed because the right-side bounds check clobbered the left pointer register.
- Fixed-byte entry parameters whose width is eight bytes or smaller can now be
  used as create-output field expectations. Fixed aggregate tuple fields can
  now be used as addressable byte sources for output lock-hash verification,
  and verifier expression temp slots are large enough for the original
  eight-recipient launch sum. The original scoped CKB
  `launch.cell::simple_launch` harness now covers a valid launch transaction
  and a malformed output rejection.
- The CKB NFT marketplace harnesses now cover `buy_from_listing` and
  `accept_offer` on-chain. These tests exposed a verifier-shape constraint:
  create-output verification cannot safely read receipt fields after that
  receipt has been destroyed and cleared, and expression aliases over destroyed
  receipts can be re-expanded during output verification. The portable CKB
  path now makes marketplace counterparties and accepted price explicit entry
  ABI arguments, so valid transactions verify on-chain and malformed payment
  outputs are rejected by script logic.
- The CKB multisig harnesses now cover every `multisig.cell` action on-chain
  with valid transactions, malformed script-logic rejections, and committed
  outputs. Original scoped artifacts are used for `create_wallet`,
  `propose_transfer`, `add_signature`, `propose_remove_signer`,
  `propose_change_threshold`, `execute_proposal`, and `cancel_proposal`.
  `propose_add_signer` and `propose_change_threshold` now use original scoped
  artifacts after the metadata/codegen path learned to prove local constructed
  byte vectors (`Vec::new` plus `extend_from_slice` or `push`) as Molecule
  bytes create-output fields.
  The harness also exposed a
  real CKB packaging constraint: typed data outputs need enough capacity and a
  nonzero effective fee, because dry-run can pass while a local node refuses to
  package an otherwise valid zero-fee or under-capacity transaction.

The important remaining production gap is now narrower but still real: only
`token.cell` is strict-admitted as a whole original CKB bundled example today.
Scoped CKB artifacts now cover every strict action counted by
`ckb_business_coverage`, including original scoped
`launch.cell::simple_launch`.
Full-file CKB admission still requires closing dynamic schema/state semantics
before the smoke bypass can be removed from CKB compatibility claims.

The latest CKB transaction-harness report has no missing on-chain actions under
the bounded coverage matrix. This is not the same as claiming original bundled
example production closure: all on-chain action harnesses now use original scoped artifacts, but full-file
strict admission still requires
closing original dynamic collections, pool transition semantics, launch
composition, and full lifecycle malformed-case matrices.

The timelock `lock_asset`, `request_release`, and `request_emergency_release`
bounded CKB harnesses now use original scoped `timelock.cell` artifacts.
`lock_asset` exercises a mixed dynamic Molecule table where
`LockedAsset.asset_type` is dynamic and `amount`/`lock_hash` are fixed fields.
`request_emergency_release` exercises a dynamic `EmergencyRelease` table with
a dynamic reason field and an empty `Vec<Address>` approval set.
`approve_emergency_release` now verifies dynamic `Vec<Address>` append
semantics against the original artifact, and both release execution paths now
verify original `ReleaseRecord` outputs. The remaining timelock production gap
is concentrated in CKB time/header semantics, broader malformed lifecycle
matrices, and `batch_create_locks` dynamic collection construction.

The CKB harness for `nft.cell::create_listing` exposed and then closed a real
production gap: strict compilation admitted the action, but the entry wrapper
did not bind read-only schema parameters such as `&NFT` to input cell data for
transaction execution. The compiler now binds uncovered read-only schema
entry parameters to CKB Inputs before verifier field checks run, allowing
created output fields copied from a read-only input cell to be checked on-chain.

## Production Definition

Production-grade dual-chain support means:

- One CellScript source semantics layer with explicit `spora`, `ckb`, and
  `portable-cell` target profiles.
- Spora artifacts compile, deploy, execute valid actions, reject malformed
  actions, and preserve Spora scheduler/Molecule ABI behavior.
- CKB artifacts compile with CKB syscall/source/hash/header/packaging rules,
  deploy to a local CKB devnet, execute valid original actions, and reject
  malformed transactions by script logic.
- All bundled examples are release-gate contracts, not only documentation
  examples.
- Public CellScript and VM-facing bytes use Molecule; Borsh is not a public
  CellScript/CKB wire format.
- Artifact metadata, schema metadata, package lockfiles, backend shape reports,
  and acceptance reports are deterministic CI artifacts.

Smoke artifacts remain useful only as VM-plumbing regression tests. They must
not be used as evidence that original business actions are production-ready.

## Bundled Example Closure Matrix

The release target is to move every bundled example from bounded/smoke coverage
to strict original execution on both chains.

| Example | Spora target | CKB current state | CKB production closure |
|---|---|---|---|
| `token.cell` | Compiles and passes Spora smoke malformed-spend coverage. | Strict admitted; original scoped CKB mint/transfer/burn/merge harnesses run on-chain with valid output liveness and malformed script rejection. | Harden capacity, TYPE_ID, malformed witness/data/type/dep matrix, and builder output. |
| `nft.cell` | Compiles and passes Spora smoke malformed-spend coverage. | Scoped CKB compile and original scoped on-chain harnesses work for `mint`, `transfer`, `create_listing`, `cancel_listing`, `buy_from_listing`, `create_offer`, `accept_offer`, and `burn`; lock `collection_creator` compiles. `batch_mint` now checks the `Collection.total_supply += recipients.len()` mutation through a dynamic Molecule vector length source, but the action remains fail-closed because returning `Vec<NFT>` still needs a real cell-backed linear collection output model. | Close batch mint collection semantics, collection lineage, metadata/data-hash rules, marketplace counterparty binding, and malformed owner/type/data cases. |
| `timelock.cell` | Compiles and passes Spora smoke malformed-spend coverage. | Scoped CKB compile works for `create_absolute_lock`, `create_relative_lock`, `lock_asset`, `request_release`, `request_emergency_release`, `approve_emergency_release`, `execute_release`, `execute_emergency_release`, `extend_lock`, locks `can_unlock_lock`, `is_owner`, `asset_matches`, `not_expired`, and `emergency_approved`; original scoped on-chain harnesses now cover every non-batch timelock action with valid transactions and malformed output rejection. `batch_create_locks` remains fail-closed because it needs dynamic vector construction and batch output indexing. | Add CKB epoch/since/header semantics, broaden malformed time/output/type/dependency cases, and close `batch_create_locks` dynamic collection construction. |
| `multisig.cell` | Compiles and passes Spora smoke malformed-spend coverage. | All original scoped CKB actions compile and run on-chain: `create_wallet`, `propose_transfer`, `add_signature`, `propose_add_signer`, `propose_remove_signer`, `propose_change_threshold`, `execute_proposal`, and `cancel_proposal`; all original locks compile: `is_signer_lock`, `can_execute`, `can_cancel`, `has_enough_signatures`, `not_expired`. | Broaden malformed signer/threshold/signature/expiry matrices and remove full-file dynamic-schema blockers so the whole bundled example is strict-admitted. |
| `vesting.cell` | Compiles and passes Spora smoke malformed-spend coverage. | All original scoped CKB actions now compile and run on-chain: `create_vesting_config`, `grant_vesting`, `claim_vested`, and `revoke_grant`. `grant_vesting` uses `env::current_timepoint()` and verifies a real Token input, VestingConfig input, VestingGrant output, header-dep timepoint, and malformed output rejection. `claim_vested` uses CKB-compatible input lock-hash authorization binding for `VestingGrant.beneficiary`, verifies claim output plus updated grant output, and rejects malformed claim output data. `revoke_grant` now requires `admin == config.admin`, verifies the config read_ref input, employee/admin token outputs, and malformed revoke output rejection. | Broaden malformed schedule/claim/revoke cases and replace the lock-script harness with a type-script deployment harness where possible. |
| `amm_pool.cell` | Compiles and passes Spora smoke malformed-spend coverage. | All original scoped CKB AMM entries compile and run on-chain: `seed_pool`, `swap_a_for_b`, `add_liquidity`, `remove_liquidity`, `isqrt`, and `min`. The harnesses verify real Token inputs, Pool/LPReceipt outputs, Pool replacement identity, LP supply coupling, add/remove proportional accounting, swap fee accounting, constant-product output pricing, Token output symbols/amounts, TypeHash binding, and malformed output rejection. | Broaden malformed slippage/symbol/type/capacity matrices and remove full-file strict blockers once only dynamic unrelated entries remain. |
| `launch.cell` | Compiles and passes Spora smoke malformed-spend coverage. | Original `launch_token` remains policy fail-closed due pool-composition semantics; original scoped `simple_launch` now runs on-chain with the eight-recipient fixed aggregate ABI, valid output coverage, and malformed-output rejection. | Add sale lifecycle, cap/allocation/finalization, pool composition wiring, and malformed phase/allocation cases for the remaining launch action. |

Production exit criterion:

- `scripts/ckb_cellscript_acceptance.sh` passes in default production mode.
- `strict_original_ckb_compile_policy_fail_closed == []`.
- `strict_original_ckb_compile_unexpected_failures == []`.
- `bundled_examples_smoke_bypass == []`.
- `production_gate.status == "passed"`.
- Every on-chain CKB action harness is compiled from the original bundled
  source with `kind == "original-scoped-action-strict"`.
- Each bundled example has at least one valid Spora action transaction and one
  valid CKB action transaction in acceptance.
- Each bundled example has malformed transactions rejected by script logic, not
  by standardness, mass, capacity, transient node state, missing plumbing, or
  cycle-limit accidents.

## Phase A: CKB Strict Original Closure

Goal: make every bundled example compile and verify as an original `ckb` profile
artifact without the acceptance smoke bypass.

Work items:

1. Keep `token.cell` strict admitted and expand its malformed matrix.
2. Finish `nft.cell` strict mint plus dynamic/fixed schema split.
3. Finish `timelock.cell` create/release/emergency flows and CKB time semantics.
4. Add `multisig.cell` dynamic signer/proposal Molecule schema and action harness.
5. Keep all `vesting.cell` actions strict/on-chain and expand malformed
   schedule/claim/revoke cases.
6. Implement `amm_pool.cell` reserve/LP conservation checks and CKB transaction
   harnesses.
7. Keep original `launch.cell::simple_launch` covered and implement
   lifecycle/composition checks for the remaining launch actions.

Required tests:

- `cargo test -p cellscript --test examples`
- `cargo test -p cellscript --test cli ckb`
- `scripts/ckb_cellscript_acceptance.sh --compile-only --production`
- `scripts/ckb_cellscript_acceptance.sh --production` against the parent CKB
  local devnet
- `scripts/ckb_cellscript_acceptance.sh --bounded` only as a development matrix
  while an explicit production gap is being closed

## Phase B: Molecule Schema Productionization

Goal: make generated persistent CellScript schemas the authoritative layout for
Spora and CKB.

Work items:

- Generate schema manifests for `resource`, `shared`, and `receipt`.
- Cover fixed-width structs, nested fixed structs, enums, fixed arrays/tuples,
  dynamic vectors/strings, and versioned layout migration policy.
- Emit schema hash, version, field offsets, dynamic sections, and target-profile
  compatibility in metadata.
- Make verifiers decode cell data through generated schema logic rather than
  ad hoc offsets.
- Make transaction builders use the same schema manifest for input/output data.
- Add schema snapshot tests for every bundled example.

Exit criteria:

- Every bundled example has a generated schema manifest.
- Spora and CKB acceptance construct cell data from the manifest.
- Schema changes are either backward-compatible or intentionally versioned.

## Phase C: Action Transaction Builder

Goal: users should not hand-write Spora or CKB transaction JSON to use a
CellScript contract.

CLI target:

```bash
cellc action build examples/token.cell \
  --target-profile ckb \
  --action transfer_token \
  --arg to=... \
  --arg amount=100 \
  --input token_cell=... \
  --out tx.json
```

Builder responsibilities:

- Read artifact metadata and schema manifest.
- Encode action arguments and witnesses.
- Select required code deps, cell deps, header deps, input cells, and output
  templates.
- Emit Spora and CKB transaction skeletons through profile adapters.
- Support `dry-run`, `explain`, `inspect`, and malformed-case generation for
  tests.

Exit criteria:

- Every bundled example tutorial can build a valid Spora transaction and a valid
  CKB transaction from CLI inputs.
- Acceptance scripts use the builder instead of bespoke Python transaction
  constructors for the main path.

## Phase D: Dual-Chain Acceptance Gates

Goal: release gates prove both chains still work, with comparable artifacts.

Fast gate:

- format/check/test for CellScript and Spora integration crates;
- all examples compile to assembly and ELF;
- Spora/CKB target-profile policy tests;
- backend shape budget JSON;
- schema snapshot tests;
- package manager and LSP smoke tests.

Medium gate:

- Spora smoke devnet acceptance;
- CKB compile-only acceptance;
- strict original metadata verification for every bundled example;
- package lock reproducibility.

Full gate:

- Spora full devnet acceptance;
- CKB full local devnet acceptance against the parent CKB checkout;
- every bundled example valid action path;
- every bundled example malformed transaction matrix;
- artifact upload for backend shape, schemas, Spora report, CKB report, and
  package lock verification.

Exit criteria:

- GitHub Actions saves all reports as artifacts.
- Release tags cannot be created without a passing full dual-chain gate.

## Phase E: Package Manager and Tooling RC

Goal: make CellScript usable as an independent production toolchain.

Work items:

- Keep CellScript as the canonical standalone repository and Spora as the
  submodule consumer.
- Publish deterministic release binaries with checksums.
- Stabilize `Cell.toml`, `Cell.lock`, local path dependencies, remove/prune,
  install, info, doc, fmt, check, build, metadata, and verify-artifact.
- Keep registry publishing and remote package resolution fail-closed until the
  verification model is finished.
- Extend LSP with target-profile diagnostics, action metadata preview, schema
  preview, package errors, and production-gate warnings.

Exit criteria:

- A fresh user can install CellScript, compile bundled examples, build
  transactions, and run Spora/CKB local devnet tutorials without repo-internal
  scripts.

## Phase F: Security and External Audit Readiness

Goal: make the dual-chain toolchain auditable.

Required audit package:

- syscall/source/hash/header profile delta;
- Molecule schema and witness ABI spec;
- artifact metadata and verification spec;
- transaction builder threat model;
- package manager trust model;
- backend CFG/branch-relaxation/code-size report;
- Spora and CKB acceptance reports;
- known limitations list.

Required adversarial coverage:

- malformed witness fuzzing;
- Molecule decode fuzzing;
- random output mutation;
- wrong cell dep/type hash/lock hash;
- capacity/mass/cycles boundary tests;
- profile isolation tests ensuring Spora-only syscalls cannot leak into CKB
  artifacts.

## Non-Negotiable Boundaries

- Do not claim full CKB production support until all original bundled examples
  strict compile and run action-specific CKB transactions.
- Do not claim smoke artifact success as business-action support.
- Do not use `--bounded` results as release evidence for CKB production.
- Do not mark `production_ready=true` unless the default production CKB gate
  passes.
- Do not reintroduce public Borsh CellScript/CKB wire formats.
- Do not let Spora support regress while closing CKB support.
- Do not weaken target-profile policy gates to pass examples.
- Do not remove backend shape and report artifacts; code size is part of
  production safety for on-chain deployment.

## Immediate Next Work

1. Remove smoke bypass coverage from the CKB production path now that strict
   action harnesses use original scoped artifacts.
2. Close original `nft.cell` `batch_mint` collection blockers now that scoped
   CKB compile works for mint and the non-batch NFT action set.
3. Close `timelock.cell` batch/time semantics now that every non-batch
   timelock action runs as an original scoped CKB artifact. Batch paths remain
   expected fail-closed original scoped gaps. Track the lowering rules exposed
   by these harnesses:
   create-output verification cannot use a schema pointer after that Cell value
   has been destroyed and cleared, scalar entry witness ABI arguments may spill
   past a0-a7 to the caller stack, while schema-backed and fixed-byte
   pointer/length arguments still fail closed if their two-slot ABI pair would
   cross the register boundary. The NFT marketplace harnesses additionally show
   that destroyed receipt fields and expression aliases over destroyed receipts
   must be materialized before destroy or exposed as explicit portable entry
   ABI arguments.
4. Add generated schema manifests and snapshot tests for `nft`, `timelock`, and
   `multisig`.
5. Start the action transaction builder around completed token, NFT, timelock,
   multisig, vesting, and AMM harnesses before launch composition flows.
6. Convert CKB acceptance to use builder-generated transactions for completed
   examples.
7. Once CKB strict fail-closed list reaches zero, remove the smoke bypass from
   compatibility claims and keep it only as a VM plumbing regression path.
