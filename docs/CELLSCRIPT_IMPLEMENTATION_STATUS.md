# CellScript Implementation Status

**Snapshot date**: 2026-04-19
**Scope**: current code in `/Users/arthur/RustroverProjects/Spora/cellscript/`  
**Purpose**: this document tracks **implementation reality**, not design intent.

## Reading guide

Use this document when you need to answer:

- what parts of `CellScript` are stable enough to rely on now
- what parts are production-grade, production-hardening, or still partial
- what modules exist in the tree but are not part of the trusted main path

For design-proposal coverage specifically, see
[CELLSCRIPT_DESIGN_IMPLEMENTATION_AUDIT.md](./CELLSCRIPT_DESIGN_IMPLEMENTATION_AUDIT.md).

For the operational execution order, current active phase, and `go on` continuation rule, see
[CELLSCRIPT_EXECUTION_PHASES.md](./CELLSCRIPT_EXECUTION_PHASES.md).

Do **not** use `SPORA_DSL_DESIGN_PROPOSAL_CN.md` as the source of truth for implementation completeness. That document is primarily a design/proposal document.

## Current audit snapshot

This snapshot incorporates the 2026-04-19 continuation audit of `SPORA_DSL_DESIGN_PROPOSAL_CN.md`, the current `cellscript/` source tree, the current design-implementation audit, and a fresh default-feature test run.

- Rust source size: `38,527` lines across `26` files under `cellscript/src/`; `42,896` lines across `28` Rust files when `cellscript/tests/` integration tests are included.
- Test inventory: `332` `#[test]` declarations are present in source/test files; the default-feature active suite run below executed `319` tests.
- Example programs: `7` bundled `.cell` examples compile under the default assembly target: `amm_pool`, `launch`, `multisig`, `nft`, `timelock`, `token`, and `vesting`.
- Design-proposal completion: roughly `70-75%` overall. The compiler core is real; the stateful protocol language is not complete.

## Current headline

`CellScript` is currently best described as:

> **a working compiler/toolchain under production hardening, with fail-closed boundaries for incomplete semantics**

Or, more bluntly:

> **Core compiler path is real. Complete stateful contract semantics are still incomplete.**

The trustworthy path today is:

1. Resolve input from `.cell`, package directory, or `Cell.toml`
2. `lex -> parse -> type check -> minimal IR lowering -> codegen`
3. Emit `riscv64-asm` or `riscv64-elf`
4. Write artifact to disk via `cellc`

## Compiler component status

This table is the short component-level view. It is intentionally conservative:
`Implemented` means the component is on the trusted main path for the currently
supported language subset; `Production-hardening`, `Partially integrated`, and
`Prototype` mark real but incomplete surfaces.

| Component | Status | Path | Notes |
|---|---|---|---|
| Lexer | Implemented | `cellscript/src/lexer/` | Stable main path |
| Parser | Implemented | `cellscript/src/parser/` | Stable for the supported syntax subset; reserved surfaces such as expression-position `launch(...)` fail explicitly |
| AST | Implemented | `cellscript/src/ast/` | Stable main path |
| Type checker | Production-hardening | `cellscript/src/types/` | Real type/effect/call-argument/resource-operation checks exist, but this is not a complete semantic proof system |
| Linear checker | Production-hardening | `cellscript/src/types/` | Real linear/ownership checks exist; `let` initializers that bind an existing linear value now move the source binding instead of copying it; explicit branch returns, tail-if return branches, linear `if` expression moves, and block-expression parent-scope state propagation now participate in linear state merging, so returning or binding the same resource on all branches or through a block tail is accepted while inconsistent branch ownership is rejected; block-local linear bindings must also be handled or moved out through the block tail instead of disappearing with the child scope; the metadata verifier classifies one external resource Input preserved into one same-type Output by direct field aliases, restricted single-field `amount: u64` additive merges from multiple same-type Inputs into one Output, and restricted matched `amount` subtraction splits from one Input into sibling Outputs as `resource-conservation:<T>` `checked-runtime`; generalized multi-output/net-amount conservation proof is not complete |
| Spora IR | Production-hardening | `cellscript/src/ir/` | Lowering main path is real; complex control flow and full stateful semantics remain partial |
| RISC-V codegen | Stable `asm` path; usable `ELF` subset | `cellscript/src/codegen/` | Pure compute and restricted CKB-style verifier paths are wired; this is not a complete backend for all language constructs |
| Lifecycle validation | Partially integrated | `cellscript/src/lifecycle/` | Main compile path and LSP diagnostics cover declaration/static checks plus restricted fixed-scalar transition checks; full runtime transition verification is incomplete |
| Optimizer | Partially integrated | `cellscript/src/optimize/` | Conservative AST constant folding, algebraic simplification, and literal-condition pruning run for `opt_level > 0` after original type/lifecycle checks and before IR lowering; the optimized AST is rechecked, but this is not a full SSA optimizer/inliner |
| LSP server | Minimal real path | `cellscript/src/lsp/` | Metadata-aware hover, diagnostics, and code actions exist; not a mature IDE surface |
| Package manager | Local package/path dependencies usable | `cellscript/src/package/` | `Cell.toml`, local `path` dependencies, and `source_roots` are wired; registry/remote workflows remain incomplete |
| Standard library | Basic runtime support | `cellscript/src/stdlib/` | Basic syscall/env/math/hash/collection support exists; not a complete standard runtime |

## Design-proposal progress

Audit percentages are descriptive. The executable phase order is maintained in
[CELLSCRIPT_EXECUTION_PHASES.md](./CELLSCRIPT_EXECUTION_PHASES.md), where Phase 4 is operationally closed.

| Design phase | Goal | Current progress |
|---|---|---:|
| Phase 0: freeze system contract | Semantic kernel, standard operation list, protocol-pattern metadata shape | ~70% |
| Phase 1: compiler MVP | Parse, validate, lower to IR, emit ckbvm-oriented artifact | ~90% |
| Phase 2: asset lifecycle and shared state | `shared`, `receipt`, lifecycle, `claim`, `settle`, scheduler metadata, and pool-pattern metadata | Closed for the operational exit gate; generalized semantics remain blocker-classed debt |
| Phase 3: node/scheduler integration | Mempool/effect checks, scheduler consumption, contention analysis | Closed for the operational exit gate; external trusted-summary submission stays out of scope unless RPC-side builders are admitted with an authenticated trust policy |
| Phase 4: production hardening | Optimizer, adversarial testing, security audit, stable tools | Closed for the operational gate; release tagging remains a separate release action |

| Design concept | Current implementation status | Practical coverage |
|---|---|---:|
| `resource` | Full frontend path plus partial linear/resource metadata; `let` bindings over existing linear values now move the source binding instead of copying it; explicit branch returns, tail-if return expressions, `if` expression branches, and block-expression scopes now propagate linear ownership states for parent-visible resources; restricted one-input/one-output field-for-field conservation, single-field `amount: u64` additive same-type merges, and one-input matched `amount` subtraction splits are now classified as `checked-runtime` and also emitted as checked `resource-conservation-proof` transaction input components; generalized multi-output/net-amount conservation remains `runtime-required` with `resource-conservation-proof-gap` | ~82-87% |
| `shared` | Parsed, typed, lowered/metadata-visible; `read_ref`, `&mut shared` parameters, composed calls returning shared values, trusted schema-parameter TypeHash ABI sources, replacement Input/Output TypeHash/LockHash checks, fixed-width preserved-field equality checks, and scalar `old +/- operand` transition checks for verifier-coverable parameter, parameter-field, and add/sub/mul/div/min computed-local operands are scheduler/verifier-visible; general mutable shared field-transition obligations can be `checked-runtime` when all written fields are covered, and Pool-specific invariant/admission debt is now represented separately as runtime-required obligations plus schema v20 `pool_primitives[]` metadata with named invariant families, field-aware runtime input requirements, blocker strings, and stable blocker classes | ~74-80% |
| `receipt` | Parsed/typed/lowered; lifecycle declarations and claim output mapping exist; IR now carries explicit lifecycle transition rules derived from declared lifecycle state order; verifier-coverable claim output preservation now covers the vesting example's computed scalar outputs, complete fixed-field lifecycle updates are marked `checked-runtime`, and verifier-covered claim output relation obligations are no longer reported as unresolved runtime requirements; claim output relations now also emit status-classified `claim-output-relation` transaction runtime input components with `claim-output-relation-gap` for unsupported output shapes; native `claim receipt` paths with a fixed `[u8; 20]` signer field named `signer_pubkey_hash`, `claim_pubkey_hash`, `owner_pubkey_hash`, `beneficiary_pubkey_hash`, or `pubkey_hash` now lower `GroupInput` witness envelope checks, canonical ECDSA sighash loading, signer-field bounds checks, `SECP256K1_VERIFY`, and signer-key binding, and classify `claim-conditions:<Receipt>` itself as `checked-runtime` only when the action body has no source-level `assert_invariant` / DAA predicate; signer-backed claims with source predicates stay top-level `runtime-required` with `source-predicate=runtime-required` while preserving checked signature/key-binding subconditions and now expose a dedicated `claim-source-predicate` transaction runtime input blocker class `claim-source-predicate-gap`; receipt claims without that explicit signer ABI still expose runtime-required field-aware consumed-input details plus schema v20 `transaction_runtime_input_requirements[]` for witness signature, authorization domain, and DAA time context; application-level vesting claim flows that consume `VestingGrant` still keep signer-key binding/signature verification runtime-required because `VestingGrant` has no verifier-coverable 20-byte signer field, and that generalized policy gap is exposed with blocker class metadata | ~77-81% |
| `consume` / `create` | Real syntax/type/IR/codegen path, with restricted fixed-scalar and schema-backed fixed-byte verifier support; direct consumed-field aliases into a same-type created resource, restricted multi-input `amount: u64` additive merges, and restricted one-input `amount - split_terms` splits whose terms match sibling created Outputs now emit `resource-conservation:<T>` plus checked `resource-conservation-proof` transaction input components, while unmatched/duplicate splits or richer cross-cell conservation remain `runtime-required` and expose a stable `resource-conservation-proof-gap` blocker class | ~73-83% |
| `transfer` / `destroy` | Parsed/lowered and capability-gated; `transfer` can preserve verifier-coverable fields and now carries the destination `Address` into the transfer-created output pattern so verifier-coverable Output LockHash rebinding plus destination address binding are `checked-runtime`; when that operation-tagged output relation is fully verifier-covered, the `transfer` expression no longer emits the symbolic fail-closed path, `transfer-expression` fail-closed metadata is absent, and the `transfer-output:<T>` transaction invariant plus `transfer-output-relation` runtime input component are classified as `checked-runtime`; unsupported transfer output relations now expose a stable `transfer-output-relation-gap` blocker class instead of only appearing as a broad runtime-required invariant; named cell-backed `destroy` now emits an executable `GroupOutput` TypeHash absence scan and marks destroy output absence/group-boundary inputs as `checked-runtime`; generalized multi-cell conservation, broader burn policy, and unsupported transfer output shapes remain incomplete | ~44-52% |
| `claim` / `settle` | Parsed/lowered with operation-tagged outputs and restricted field preservation; source-order create verification now covers computed scalar output formulas, verifier-covered claim/settle output relation obligations are classified as `checked-runtime`, fully covered `claim` / `settle` output relations no longer emit symbolic fail-closed expression paths, and both checked plus unsupported output relations now surface as status-classified `claim-output-relation` / `settle-output-relation` transaction runtime input components with `claim-output-relation-gap` / `settle-output-relation-gap` blockers for unsupported shapes; native claim-condition obligations are `checked-runtime` only for receipts with an explicit verifier-coverable 20-byte signer pubkey hash field, generated `SECP256K1_VERIFY`, and no source-level `assert_invariant` / DAA predicate; signer-backed guarded claims remain top-level `runtime-required` while preserving checked signature/key-binding subconditions and exposing `claim-source-predicate` / `claim-source-predicate-gap` for the unchecked source-level guard. Remaining runtime-required claim condition and settle finalization obligations expose field-aware consumed-input requirements plus structured transaction runtime input requirements for witness/signature/time/finalization contexts, with verifier-covered sub-inputs status-classified (`claim-time-context` for checked DAA claim predicates, `claim-authorization-domain` for checked witness envelope + canonical ECDSA sighash lowering, `claim-witness-signature` for the explicit signer-field ABI, `settle-output-admission` when same-scope settle output relations are checked, and `settle-final-state-context` when lifecycle metadata plus a fixed-scalar `state` field let the verifier prove Input/Output state equals the final lifecycle index); fully verifier-covered lifecycle-backed settle paths now classify `settle-finalization:<T>` itself as `checked-runtime` when the final-state check and settle output admission are both covered; the vesting `claim_vested` flow reports checked DAA/cliff/state/claimable/witness-format/authorization-domain subconditions but keeps signature verification runtime-required because it lacks the 20-byte signer field convention; generalized claim authorization policy and non-lifecycle/generalized finalization semantics remain incomplete | ~60-67% |
| `action` | Full frontend-to-lowering/codegen path for supported action bodies, including effect inference, metadata emission, scheduler witness metadata, CLI summaries, and policy gates | Implemented for the supported language subset |
| pure helper `fn` | Distinct AST/IR/metadata entries; must infer `Pure`; may call only other pure helpers; cannot call `action`, `lock`, `env::*` runtime builtins, `type_hash()` Cell identity builtins, or direct Cell/runtime operations. Broader cross-module and full language coverage remains partial | Safer partial |
| `launch` | Reserved and explicitly rejected in expression position until post-v1 transaction-builder lowering exists; ordinary launch examples are modeled with explicit `create` operations and actions, while metadata can expose composition obligations for controlled launch-like flows and now discharges controlled `launch_token -> seed_pool` pool-id continuity through tuple return ABI metadata | ~30-34% |
| pool pattern | Not a first-class language primitive; modeled as ordinary `shared` state plus user actions. `&mut Pool` parameters and composed Pool returns feed scheduler touch metadata, created Pool output TypeHash can verify `seed_pool` LPReceipt identity, replacement Pool TypeHash/LockHash preservation and fixed-width preserved-field equality are checked in generated assembly, supported AMM reserve/LP transitions can be `checked-runtime`, controlled `seed_pool` token-pair identity admission now loads Input `token_a` / `token_b` TypeHash fields and rejects equal 32-byte identities, controlled `launch_token -> seed_pool` pool-id continuity is `checked-runtime` when tuple return fields are projected through the return-register ABI, and controlled `swap_a_for_b` LP supply consistency is `checked-runtime` through preserved `Pool.total_lp` equality; broader AMM admission/economic invariant families remain protocol-pattern runtime requirements exposed through schema v20 `pool_primitives[]` with stable Phase-2-deferred blocker classes, not language-core semantics | ~70-75% |
| Post-v1 templates / `#[type_id]` / schema evolution | Per `CELLSCRIPT_CKB_COMPATIBILITY_DECISION.md`, user-defined generics are no longer a v1 executable-core target: generic type definitions and user-defined instantiations fail closed, while `Vec<T>` remains a controlled builtin collection notation. Parametric authoring is delegated to a post-v1 package/codegen/template layer that must generate concrete `.cell` schemas and, for persistent public state, generated or declared Molecule layouts. `#[type_id("...")]` is now a real item-level attribute for `resource` / `shared` / `receipt` / `struct`: the parser accepts it, the type checker rejects duplicate stable IDs across the visible module scope including imported types, IR preserves it, and metadata schema v20 emits `types[].type_id` plus `types[].type_id_hash_blake3`. Executable CKB type-id lineage verification and full schema migration/versioning rules remain incomplete | ~35-45% for v1 scope |

## Trusted main path

These parts are real and currently wired into the main compiler entry:

| Area | Status | Notes |
|---|---|---|
| Lexer | Stable main path | Main path in use |
| Parser | Stable main path | Main path in use; array literals now parse as first-class expressions instead of AST-only dead syntax |
| AST | Stable main path | Main path in use |
| Input resolution | Stable main path | Supports single file, package dir, `Cell.toml` |
| Local package loading | Stable main path | Supports local `path` dependencies and `source_roots`; duplicate modules and duplicate local/imported symbols are rejected instead of being overwritten |
| Type checking | Production-hardening | Real checks exist; duplicate top-level symbols are rejected; unknown named types are rejected in schemas, callable signatures, explicit local annotations, and namespaced constructor calls; callable signatures now enforce argument count and argument types for local, same-module qualified, and local path imported calls, with `&mut T` accepted where a read-only `&T` is expected; builtins and methods such as `env::current_daa_score`, `Address::zero`, `Hash::zero`, `Vec::new`, `min/max/isqrt`, `len`, `type_hash`, `push`, and `extend_from_slice` have arity/type gates; `Option` / `Result` are reserved but rejected until the explicit error model is implemented; `create` and `read_ref` targets must be cell-backed `resource`, `shared`, or `receipt` types; `consume`, `transfer`, `destroy`, `claim`, and `settle` require named cell-backed linear operands so ownership state can be tracked; `let` initializers move existing linear values before introducing the new binding, preventing a resource from being copied through aliases; `if` statements and `if` expressions conservatively merge linear ownership state and reject paths where a resource is consumed/transferred/destroyed/returned/moved only on some continuing, terminal, tail-return, or expression branches; block expressions now propagate parent-visible linear state changes from their scoped child environment while keeping block-local bindings scoped, so `{ destroy token }`, `{ token }`, and `{ let inner = token; inner }` participate in the same move model, while unhandled block-local linear bindings such as `{ let out = create Token { ... }; 1 }` are rejected; `create` and struct literals must exactly match declared fields, reject duplicate/unknown/missing fields, and type-check field initializers; empty `Vec::new()` may initialize a concrete `Vec<T>` field; array literals now require homogeneous element types, empty arrays require explicit zero-length array annotations, aggregate element writes require mutable roots, `Vec.push` propagates item types, `unwrap` / `expect` / `unwrap_or` are forbidden in consensus code, `assert_invariant` is value-less `Unit` with static string literal messages only, unreachable statements after guaranteed returns are rejected, and value-returning action/fn bodies must have explicit return paths or typed tail expressions on all paths |
| Linear checks | Production-hardening | Real checks exist, but not complete semantic coverage |
| IR lowering | Production-hardening | Pure-compute path lowers statements, locals, returns, tail-expression returns, terminal-if tail returns, parameter bindings, no-return helper calls as destinationless calls, `assert_invariant` as Unit-valued fail-closed CFG, typed empty fixed arrays, local fixed-array static index reads/writes, local fixed-array foreach unrolling, fixed parameter array foreach unrolling, local fixed-array `len()` folding, local tuple static field reads/writes/destructuring, fixed aggregate pointer index/projection, tuple-valued call return projection through the return-register ABI, and array-of-tuples static index/foreach destructuring projections. Stateful IR now also carries explicit `write_intents` derived from `create_set` and replacement-bound `mutate_set` summaries |
| Enum / match semantics | Production-hardening | Field-less enum variants lower as discriminants; unknown enum variant values are rejected; payload enum variants cannot be used as bare values until payload construction lowering exists; enum match rejects unknown variants, duplicate variant arms, non-exhaustive matches without `_`, and payload-variant patterns until payload destructuring lowering exists; exhaustive enum matches no longer require a wildcard and retain an invalid-discriminant fail-closed branch |
| RISC-V assembly emission | Stable main path | Main output path |
| RISC-V ELF emission | Usable subset | Main output path for pure computation, restricted fixed-width scalar schema-parameter field loads, and restricted `read_ref` field loads, with external toolchain support and built-in fallback |
| Schema layout metadata | Production-hardening | Type/field offset and fixed encoded-size metadata is emitted for audit/tooling |
| Consume input field verification | Partial | Consumed, transferred, claimed, settled, and destroyed Input cell bytes are represented in `consume_set` with operation provenance and loaded with CKB `LOAD_CELL`; fixed-width scalar fields (`bool/u8/u16/u32/u64`) are read with unaligned-safe byte loads and exact-size/bounds checked when backed by loaded cell bytes; schema-backed fixed-byte fields (`Address`, `Hash`, `[u8; N]`) can be used as preservation sources for output byte comparisons; one external resource Input preserved into one same-type created Output by direct field aliases, restricted multiple same-type Inputs merged into one single-field `amount: u64` Output through a verifier-recomputed `Add` expression, and restricted one-Input `amount` splits into sibling created Outputs are classified as `resource-conservation:<T>` `checked-runtime` and surface checked `resource-conservation-proof` transaction input components, but generalized conservation semantics are not complete |
| Create output verification | Partial | Simple fixed-width scalar output schemas are checked in assembly with exact-size checks and per-field equality against constants, parameters, consumed/read schema field aliases, and already-computed scalar locals at the source `create` instruction; fixed-byte fields (`Address`, `Hash`, `[u8; N]`) can be checked byte-by-byte against constants, consumed/read schema field aliases, stack-backed fixed `[u8; N<=8]` parameters, pointer+length `Address` / `Hash` parameters, supported fixed aggregate tuple-array projections such as `[(Address, u64); N]`, created Output `TypeHash` fields loaded with `LOAD_CELL_BY_FIELD`, and trusted schema-parameter TypeHash ABI bytes; verifier-coverable `with_lock(...)` bindings load output `LockHash` through `LOAD_CELL_BY_FIELD` and compare 32 bytes against constants, consumed/read schema-backed aliases, 32-byte fixed parameters, supported aggregate projections, or loaded fixed bytes; imported dependency type layouts feed these checks; typed scalar `let` annotations are preserved in IR so narrow output fields such as `u8` lifecycle state can be verified; incomplete field verifiers or unsupported lock bindings still fail closed instead of continuing with comments; full resource creation and general output lock semantics are not complete |
| Symbolic runtime lowering | Safer partial | Unsupported stateful/runtime operations now emit explicit fail-closed return paths instead of silent success values; named cell-backed `destroy` has moved out of the symbolic path for the covered grouped-output absence scan, and verifier-covered `transfer` / `claim` / `settle` operation-tagged output relations now reuse their prelude output checks instead of emitting expression-level symbolic fail-closed paths |
| Fail-closed metadata | Production-hardening | Runtime/action/lock metadata now expose `fail_closed_runtime_features` separately from broader symbolic and CKB runtime feature lists; incomplete output verifier coverage is reported as `output-verification-incomplete` instead of being visible only in generated assembly |
| Verifier obligation metadata | Production-hardening | Runtime/action/fn/lock metadata now emit explicit obligations for CKB runtime access, standalone-ELF limitations, checked-static resource-operation gates, transaction invariants including restricted one-input/one-output, single-field additive-merge, and matched amount-split `resource-conservation:<T>` checked/runtime classification, mutable shared-state and mutable resource/receipt cell-state transition obligations with `mutate_set` replacement ABI and field summaries, explicit `pool-pattern` runtime-required obligations plus structured `pool_primitives[]` records for Pool creation/mutation/composition semantics, named checked/runtime invariant families, and Pool field-aware runtime input requirements, including launch composition `Param` / `Output` ABI requirements plus checked tuple-return pool-id continuity for the controlled `seed_pool` path, fail-closed runtime paths, lifecycle transition checks classified as `checked-runtime` when the fixed-field verifier is complete or `checked-partial` otherwise, transfer/claim/settle output relation checks classified as `checked-runtime` when operation-tagged outputs are fully verifier-covered, checked native `claim-conditions:<Receipt>` classification for explicit signer-field receipts without source predicates, source-predicate runtime-required classification for signer-backed guarded claims, checked `settle-finalization:<T>` classification for the restricted lifecycle final-state plus output-admission path, schema v20 status-classified `transaction_runtime_input_requirements[]` for transfer/destroy/claim/settle witness/time/finalization contexts, resource-conservation proof coverage, and mutable state transition/equality gaps, including checked/runtime-required `transfer-output-relation`, `claim-output-relation`, `settle-output-relation`, and `resource-conservation-proof` components with `transfer-output-relation-gap`, `claim-output-relation-gap`, `settle-output-relation-gap`, and `resource-conservation-proof-gap`, runtime-required `mutate-field-transition` / `mutate-field-equality` components with `state-transition-formula-gap` / `state-field-equality-gap`, runtime-required `claim-source-predicate` components with `claim-source-predicate-gap`, checked transfer destination lock/address binding, checked destroy grouped-output absence/group-boundary, checked claim authorization/signature components, checked claim-time, checked settle-output-admission, checked lifecycle settle final-state sub-inputs, and blocker strings/classes for remaining runtime-required transaction inputs, plus application-level `claim-conditions:VestingGrant` details with checked DAA/cliff/state/claimable source-predicate subconditions; CLI JSON summaries, `--deny-runtime-obligations` diagnostics, and docgen Markdown/HTML/JSON now separately expose non-Pool runtime-required `transaction-invariant` checked subconditions plus blocker and blocker-class summaries for runtime-required transaction inputs; CLI policy gates count checked/runtime-required Pool invariant families, mark controlled `seed_pool` `token-pair-identity-admission` as `checked-runtime` from `input-type-id-abi+load-cell-by-field`, mark controlled `launch_token -> seed_pool` `pool-id-continuity` as `checked-runtime` from `callee-output-field-coupling+tuple-return-abi`, mark controlled `swap_a_for_b` `lp-supply-consistency` as `checked-runtime` from `mutate-preserved-field-equality`, reject remaining runtime-required Pool families under `--deny-runtime-obligations`, and separately report flattened Pool runtime input requirement summaries plus runtime-required Pool invariant blocker-class summaries |
| VM object ABI metadata | Usable partial | Compile metadata declares Molecule VM object ABI `0x8001` for CKB-style full-object load syscalls; RISC-V ELF artifacts embed a fixed ABI trailer that exec strips before CKB-VM loading; assembly/non-ELF artifacts still need sidecar/verifier policy |
| Artifact/metadata self-validation | Production hardening | `CompileResult::validate()` checks metadata schema version, compiler version, artifact hash, metadata hash/size binding, source-unit hash binding, artifact format, ELF magic, VM ABI trailer presence/version, standalone-runtime metadata consistency, and stable type-id metadata consistency/uniqueness before compile results are returned; metadata schema v20 exposes stable declared type identity (`types[].type_id` plus `types[].type_id_hash_blake3`), status-classified transfer/destroy/claim/settle/resource-conservation/mutable-state transaction runtime input requirements with transfer output-relation, claim/settle output-relation, claim source-predicate, and mutable state blocker classes, checked destroy GroupOutput absence scans, blocker strings, blocker classes, and CLI blocker summaries for remaining runtime-required components, type capability declarations, receipt claim output declarations, parameter TypeHash ABI requirements, Input/CellDep/Output cell access operation provenance, operation-tagged scheduler witness access records, `mutate_set` replacement ABI plus checked TypeHash/LockHash preservation and field-equality status for mutable Cell parameters, and structured `pool_primitives[]` metadata with named invariant families plus field-aware runtime input requirements and stable Pool blocker classes |
| Scheduler witness metadata | Phase 3 operational path closed | Per-action metadata includes `scheduler_witness_borsh_hex` generated from a Borsh `SchedulerWitness` with magic `0xCE11`; `read_ref`, mutable shared parameters, calls returning shared values, and parameter TypeHash ABI requirements feed the metadata surface; mutable shared/resource/receipt params also expose verifier obligations plus `mutate-input` / `mutate-output` access records derived from `mutate_set`; schema v20 filters runtime-only claim witness/signature accesses out of scheduler witnesses so witness records stay within Input/CellDep/Output cell-state sources; `ActionMetadata::scheduler_witness_bytes()` exposes those compiled witness bytes, and `spora-exec` CellTx now has helpers to attach/discover/admit ordinary CellScript scheduler witnesses plus `push_cellscript_compiled_scheduler_witness(...)`, which admits compiled metadata bytes against a concrete transaction, appends the witness, and returns the trusted operation/source/index/binding_hash access multiset. Consensus MPE block summaries now consume admitted witnesses to merge scheduler-visible Input/CellDep/Output accesses and shared read/write contention domains into `BlockAccessSummary`; write/read and write/write shared overlaps serialize the execution DAG while read/read overlaps remain parallelizable. A strict MPE constructor can require trusted builder/metadata access summaries and reject missing or mismatched summaries before merge. Mempool validation and template prefiltering now reject malformed CellScript scheduler metadata before acceptance/selection; `MempoolTransaction` and the mining template selectors now carry optional producer-backed trusted summaries, including trusted empty summaries, into the strict template policy path. The wallet transaction generator can attach compiled scheduler witness bytes to the final transaction and expose the returned trusted summary on `PendingTransaction`; focused mining and consensus tests prove producer-returned summaries survive sidecar insertion into selector exposure and are consumed/rejected by strict template prefiltering. Remaining work belongs to Phase 4 hardening: external trusted-summary submission policy, broader adversarial/property coverage, and release-grade verification gates |
| Effect enforcement | Safer partial | Action effects are inferred from read/create/consume/destroy/transfer/claim/settle operations, mutable shared parameters, same-module function calls, and local `path` dependency imports; explicit under-declarations are rejected |
| Capability enforcement | Safer partial | `transfer` now requires the source type to declare `transfer`, `destroy` requires `destroy`, `claim` is restricted to receipt values, `receipt Name -> Output` must target a resource/shared cell type rather than a scalar or another receipt, and `settle` is restricted to cell-backed linear values; parser merges `#[capability(...)]` and inline `has ...` capability declarations |
| Transfer output field preservation | Safer partial | `transfer asset to addr` records transfer-created outputs in `create_set`, verifier-coverable same-name same-type fixed-scalar or schema-backed fixed-byte fields are checked against the consumed asset bytes, and verifier-coverable destination `Address` operands are checked against the transfer-created Output LockHash with `LOAD_CELL_BY_FIELD`; when those checks fully cover the operation-tagged output relation, `transfer` no longer contributes `transfer-expression` fail-closed metadata and `transfer-output:<T>` plus the `transfer-output-relation` runtime input requirement are reported as `checked-runtime`. Broader multi-cell conservation policy still remains runtime-required; unsupported output relations now expose `transfer-output-relation-gap`, while unsupported output shapes still fail closed. |
| Receipt claim output mapping | Safer partial | `claim receipt` now uses the declared `receipt Name -> Output` return type, records claim-created outputs in `create_set`, and verifier-coverable same-name same-type fixed-scalar or schema-backed fixed-byte fields are checked against the consumed receipt bytes; when that operation-tagged output pattern is complete, `claim-output:<T>` and its `claim-output-relation` transaction input are reported as `checked-runtime`, and the `claim` expression no longer emits a symbolic fail-closed output path; unsupported claim output shapes still fail closed and expose `claim-output-relation-gap`; explicit claim and vesting-style application claim flows can emit `LOAD_WITNESS` from `GroupInput`, validate a 65/66-byte signature envelope, and load the canonical ECDSA signature hash, so witness format and authorization-domain subconditions can be `checked-runtime`; receipts with an explicit fixed `[u8; 20]` signer pubkey hash field convention also emit `SECP256K1_VERIFY` and mark claim witness signature/signature key binding checked; receipts without that field convention and generalized time-lock authorization still remain runtime-required. |
| Settle output field preservation | Safer partial | `settle value` records settle-created outputs in `create_set`, and verifier-coverable same-name same-type fixed-scalar or schema-backed fixed-byte fields are checked against the consumed value bytes; when that operation-tagged output pattern is complete, `settle-output:<T>` and its `settle-output-relation` transaction input are reported as `checked-runtime`, and the `settle` expression no longer emits a symbolic fail-closed output path. Unsupported settle output shapes still fail closed and expose `settle-output-relation-gap`. If the settled type also has lifecycle metadata and a fixed-scalar `state` field, and the same output relation is verifier-covered, `settle-finalization:<T>` is also reported as `checked-runtime` for the restricted final-state path. Non-lifecycle/generalized finalization semantics remain runtime-required. |
| Mutable Cell mutation summary | Safer partial | `&mut` cell-backed parameters now produce `mutate_set` IR/metadata summaries with the mutable binding, cell type, `mutate` operation, replacement `Input#N -> Output#N` ABI binding, type-hash/lock-hash preservation requirements and `checked-runtime` preservation status, directly assigned transition fields, and preserved equality fields. Generated assembly loads mutable replacement Input/Output TypeHash and LockHash with `LOAD_CELL_BY_FIELD`, exact-checks 32-byte lengths, and byte-compares them. It also loads replacement Input/Output full cell bytes and byte-compares verifier-coverable fixed-width preserved fields. Token `mint` exposes `MintAuthority.auth.minted = old + amount` as `checked-runtime` plus checked preserved `max_supply` / `token_symbol`; AMM Pool updates expose `reserve_a`, `reserve_b`, and `total_lp` where written, with `checked-runtime` transition coverage for deltas directly reloadable from parameter fields and computed-local formulas over add/sub/mul/div/min. Pool-specific admission and invariant semantics are now separate `pool-pattern` runtime-required obligations instead of being conflated with general checked shared mutation. Broader non-covered formulas still remain runtime-required, but non-covered transition/equality gaps now surface as `mutate-field-transition` / `mutate-field-equality` transaction runtime input requirements with stable blocker classes. |
| Launch builder boundary | Explicitly blocked | `launch` remains reserved for post-v1 asset-launch transaction-builder lowering, but expression-position `launch(...)` now fails with an explicit diagnostic instead of being parsed as an ordinary unresolved call |
| `fn` purity boundary | Safer partial | Helper `fn` definitions are distinct AST/IR/metadata entries, must infer `Pure`, and cannot call `action`, `lock`, `env::*` runtime builtins, or `type_hash()` Cell identity builtins; direct, same-module indirect, and local imported Cell/runtime operations are rejected instead of being lowered as hidden actions; no-return helpers use an internal `Unit` type and cannot be bound or returned as values |
| Lifecycle declaration/runtime checks | Safer partial | `#[lifecycle(...)]` receipts are now checked on the main compile path and in LSP diagnostics for declaration/create/reset rules; lifecycle states and adjacent transition edges are emitted in type metadata; complete fixed-scalar consume-to-create verifier paths emit state-range and `old_state + 1 == new_state` prelude checks and are reported as `checked-runtime` obligations |
| Main CLI compiler | Production-hardening | `cellscript/src/main.rs` is the real entry point |
| Examples / compiler regression tests | Stable main path | Library, CLI, and examples coverage exist |

## Partial / still shallow

These parts exist and are useful, but should not be treated as complete:

| Area | Status | Why not “done” |
|---|---|---|
| IR | Partial | Lowering is still minimal, especially for complex control flow and resource semantics |
| Codegen | Partial | Pure compute path improved; broader language coverage remains incomplete |
| REPL | Partial | Basic utility exists, but not a mature workflow surface |
| Stdlib | Partial | Enough runtime support for current compiler path, not a complete language runtime |
| Module resolution | Partial-to-good | Local package/path dependency story is real, remote dependency story is not |
| Manifest build config | Partial-to-good | `target`, `out_dir`, `entry`, `source_roots` are wired; broader package workflow is not |
| CLI local workflow | Partial-to-good | `build`, `check`, `doc`, `fmt`, `metadata`, `verify-artifact`, and compiler-test discovery are real; `cellc test` supports positive compile tests, strict `expect-success` / `expect-fail` / `expect-error` diagnostics, per-file target selection, per-file production/symbolic/CKB/runtime-obligation policy gates, runtime metadata assertions, verifier-obligation and runtime-required-obligation assertions, action/function/lock metadata classification assertions, and `--json` CI summaries; unknown or conflicting directives fail instead of being ignored; `fmt --json` emits clean/dirty changed-file summaries; `build` and `check` can enforce command-line or manifest `[policy]` production/symbolic/CKB/runtime-obligation metadata policies before artifacts are accepted; `build --json`, `check --json`, and `verify-artifact --json` include fail-closed/runtime-required verifier obligation counts plus checked/runtime-required Pool invariant-family counts, runtime-required transaction runtime input blocker-class summaries including `transfer-output-relation-gap`, `state-transition-formula-gap`, `state-field-equality-gap`, and `resource-conservation-proof-gap`, and runtime-required Pool blocker-class summaries; `--deny-runtime-obligations` rejects unresolved Pool invariant families, mutable state gaps, transfer relation gaps, and resource-conservation proof gaps and reports their blocker classes; `doc` includes lowering audit reports and verifier obligations; registry/runtime testing remains incomplete |

## Present in tree, but not trusted as complete

These modules exist, but should be considered **limited, unsupported for production semantics, or not on the trusted compiler path**:

| Module area | Status |
|---|---|
| `src/cli/` subcommand framework | Partial-to-good local workflow; registry commands fail-closed |
| `src/optimize/` | Partially integrated conservative AST optimization path for nonzero `opt_level`; not a complete optimizer or semantic proof pass |
| `src/docgen/` | Partial-to-good API docs plus lowering audit report / verifier obligation output |
| `src/fmt/` | Partial |
| `src/lsp/` | Minimal real path with metadata-aware action hover, diagnostics, and code actions; not mature |
| `src/package/` | Partial-to-good local package support |
| `src/test/` custom framework | Compiler-test discovery with expected-failure diagnostics plus per-file target/policy/runtime/entrypoint-metadata directives, not a runtime/property/fuzz framework |
| `src/wasm/` | Compiled metadata-only/fail-closed path; executable backend unsupported |
| `src/incremental/` | Not on the trusted main path; no supported incremental-compilation contract |
| `src/debug/` | Limited diagnostic helpers; not a supported debugger |
| `src/lifecycle/` | Partially integrated; declaration/static create-state/static reset checks are trusted, and codegen emits transition prelude checks for complete fixed-scalar output verifiers; full transition verifier remains future work |

Important:

- Several limited modules now intentionally **fail closed** instead of pretending to succeed.
- They should still be treated as unsupported product surface until their executable semantics and tests are complete.

## CLI reality

The currently trusted CLI includes the single-entry compiler in:

- [main.rs](/Users/arthur/RustroverProjects/Spora/cellscript/src/main.rs)

Real supported direct flows:

- `cellc <input>`
- `cellc <input> --target riscv64-asm`
- `cellc <input> --target riscv64-elf`
- `cellc <input> -o <path>`
- `cellc <input> --lex`
- `cellc <input> --parse`
- `cellc --interactive`
- `cellc --gen-stdlib`

Real local subcommand flows:

- `cellc build [--json] [--production] [--deny-fail-closed] [--deny-symbolic-runtime] [--deny-ckb-runtime] [--deny-runtime-obligations]`
- `cellc check [--all-targets] [--json] [--production] [--deny-fail-closed] [--deny-symbolic-runtime] [--deny-ckb-runtime] [--deny-runtime-obligations]`
- `cellc doc --format markdown|html|json [--json]`
- `cellc fmt [--check] [--json]`
- `cellc init [NAME] [PATH] [--lib] [--json]`
- `cellc add CRATE... [--dev] [--build] [--git URL] [--path PATH] [--json]`
- `cellc remove CRATE... [--dev] [--build] [--json]`
- `cellc clean [--json]`
- `cellc info [--json]`
- `cellc metadata [INPUT]`
- `cellc verify-artifact ARTIFACT [--metadata FILE] [--verify-sources] [--json] [--expect-artifact-hash HASH] [--expect-source-hash HASH] [--expect-source-content-hash HASH] [--production] [--deny-fail-closed] [--deny-symbolic-runtime] [--deny-ckb-runtime] [--deny-runtime-obligations]`
- `cellc test [--no-run]`
- `cellc run` only when built with the `vm-runner` feature and only for no-argument pure ELF programs

Still fail-closed / incomplete:

- `publish`
- `install`
- `update`
- `login`
- runtime/property/fuzz execution under `cellc test`

## Output reality

Currently real output targets:

- `riscv64-asm`
- `riscv64-elf`

Important note:

- `ELF` output is real and generated now.
- External RISC-V GNU toolchains are used when available.
- Built-in fallback ELF assembly/writing still exists for environments without a working external toolchain.

Not currently an executable output target:

- `WebAssembly`

The in-tree `src/wasm/` module is now compiled and tested as a metadata-only/fail-closed path. It can emit metadata-only Wasm module structure, but rejects executable `action` / `lock` lowering.

## Stdlib and syscall reality

The in-tree stdlib is enough for the current compiler path, but it is not the complete standard runtime described by the design proposal.

Real exposed wrappers or generated helpers include:

- `syscall_load_tx_hash` (`2061`)
- `syscall_load_script_hash` (`2062`)
- `syscall_load_cell` (`2071`)
- `syscall_load_header` (`2072`)
- `syscall_load_input` (`2073`)
- `syscall_load_witness` (`2074`)
- `syscall_load_script` (`2075`)
- `syscall_load_cell_by_field` (`2081`)
- `syscall_load_cell_data` (`2092`)
- `syscall_current_cycles` (`2042`)
- `syscall_debug_print` (`2177`)
- `env_current_daa_score`, which emits a `LOAD_HEADER` (`2072`) runtime path
- `env_remaining_cycles`
- `math_min`, `math_max`, `math_isqrt`, `math_abs_diff`
- `hash_blake3`, still tied to the Spora extension syscall placeholder in generated assembly

Not yet exposed as complete first-class stdlib wrappers:

- a complete VM-side Borsh serialize/deserialize API for arbitrary schemas

## Scheduler and DAG reality

CellScript emits useful DAG-oriented metadata, and the first consensus-side MPE consumer plus first mempool/template policy gate now exist. The low-level compiled-metadata producer helper also exists, producer-backed trusted summaries can be stored on mempool entries and carried by template selectors into the strict policy path, the wallet transaction generator can attach one compiled scheduler witness to the final transaction while preserving the returned trusted summary on `PendingTransaction`, focused mining coverage carries a producer-returned summary through sidecar insertion into selector exposure, and focused consensus coverage proves selector-provided builder summaries are consumed and rejected by strict template prefiltering. The Phase 3 operational path is closed; remaining work is Phase 4 hardening: RPC-side submission does not yet expose an authenticated trusted-summary field, broader adversarial/property coverage is still needed, and release-grade verification gates need to be kept green across the workspace/release process.

Real today:

- effect class inference and explicit under-declaration rejection
- `consume_set`, `read_refs`, `create_set`, and replacement-bound `mutate_set` in IR/metadata
- operation/source/index/binding provenance for Input, CellDep, Output, `mutate-input`, and `mutate-output` accesses
- per-action Borsh scheduler witness bytes in `scheduler_witness_borsh_hex`, filtered to scheduler-visible Input/CellDep/Output cell-state accesses
- `ActionMetadata::scheduler_witness_bytes()` for decoding compiled scheduler sidecars into bytes
- `parallelizable`, `estimated_cycles`, and `touches_shared` metadata fields; `read_ref`, `&mut shared` parameters, and composed calls returning shared values are included in shared-touch inference
- low-level `spora-exec` CellTx helpers to place/discover/decode/admit those witnesses, including operation/source compatibility, transaction source-index bounds checks, and exact operation/source/index/binding_hash access-set comparison against a trusted summary
- `CellTx::push_cellscript_compiled_scheduler_witness(...)`, which validates compiled metadata bytes against a concrete transaction, appends the witness, and returns the trusted access summary for strict policy
- wallet `GeneratorSettings::with_cellscript_compiled_scheduler_witness(...)`, which attaches the compiled witness to the final generated transaction and stores the returned summary on `PendingTransaction::cellscript_scheduler_accesses()`
- consensus MPE `BlockAccessSummary` consumption of admitted CellScript scheduler witnesses: Input/CellDep/Output accesses are merged into the structural read/write sets, and `touches_shared` is classified as shared reads for `Pure`/`ReadOnly` effects and shared writes for mutating/creating/destroying effects
- strict consensus MPE access-summary construction with trusted transaction-builder/compiled-metadata access multisets; missing or mismatched trusted summaries fail before witness data is merged into scheduler state
- mempool validation admission policy for malformed CellScript scheduler metadata, plus template prefilter policy for malformed metadata before conflict selection
- strict template policy fixture path for trusted transaction-builder/compiled-metadata access multisets; missing or mismatched trusted summaries reject the selected transaction
- producer-backed trusted summary storage on `MempoolTransaction`, candidate snapshots, `TakeAllSelector`, `SequenceSelector`, and `RebalancingWeightedTransactionSelector`, exposed through `TemplateTransactionSelector::selected_cellscript_scheduler_accesses()`
- `MiningManager::validate_and_insert_cell_transaction_with_scheduler_accesses(...)` for submitting a transaction with the trusted summary returned by the producer helper
- first adversarial MPE summary and policy tests for malformed candidate witness bytes, illegal operation/source pairs, out-of-bounds source indexes, missing scheduler witnesses over structural double-spend conflicts, underreported witnesses over structural CellDep read dependencies, forged extra shared-write touches, admitted read-only shared touch classification, missing trusted summaries, mismatched trusted summaries, malformed mempool metadata, malformed/missing/mismatched template policy metadata, producer rejection when compiled metadata does not match transaction shape, and producer-sidecar preservation through mining selectors

Still missing:

- broader strict-template adversarial/property coverage beyond the focused selector-provided builder-summary prefilter tests
- explicit RPC or submission-surface support if external wallet submission must deliver trusted summaries into strict template policy rather than using the in-process mining API

## Semantic completeness

The compiler is no longer just “shape-complete”. Some real semantics are now enforced end-to-end:

- function parameters get IR bindings
- parameters are spilled to stack slots in codegen
- local bindings participate in computation
- `return <expr>` now lowers into actual return-value generation
- no-return `fn` calls lower as destinationless IR calls; their internal `Unit` result cannot be bound to locals or returned from value-returning entrypoints
- `assert_invariant` lowers to a fail-closed branch and has internal `Unit` type, so it cannot be bound as a value or used as a tail return expression; assertion messages must be static string literals rather than runtime expressions
- statements after guaranteed-return source paths are rejected instead of being silently ignored by IR/codegen lowering
- unresolved call return types no longer default to `u64`; IR lowering rejects unknown calls instead of fabricating a value type
- function/action/lock calls now validate argument count and argument types before lowering; read-only reference parameters accept mutable references, but mismatched arity or value/reference types fail during type checking
- empty array literals no longer silently default to `[u64; 0]`; they require an explicit zero-length array type annotation and preserve that element type through IR lowering
- value-returning `action` and `fn` bodies now require all paths to return through explicit `return`, typed tail expressions, or terminal `if` branches with typed tail expressions; codegen lowers those tail forms into real `Return(Some(...))` terminators
- `Vec.push` now propagates element types from an initially untyped `Vec::new()` and rejects pushes whose item type disagrees with an already typed `Vec<T>`
- minimal pure-compute functions such as `add(x, y)` now compile into meaningful arithmetic assembly
- named schema parameters expose fixed field layout metadata
- `param.scalar_field` on a named action/lock schema parameter lowers to unaligned-safe little-endian byte loads for fixed `bool/u8/u16/u32/u64` fields and can emit ELF; the generated ABI now passes schema values as `aN=borsh_ptr, aN+1=borsh_len`, so fixed schema field access performs exact-size and bounds checks; schema-backed fixed-byte fields (`Address`, `Hash`, `[u8; N]`) can be materialized as byte sources for output preservation checks and verifier-coverable `Eq` / `Ne`; `param.type_hash()` on named schema parameters now requires an additional trusted 32-byte pointer+length ABI pair instead of using a compile-time type-name hash as instance identity
- `consume token` preloads the consumed Input cell bytes in assembly, retains the verifier pointer, and lets `token.scalar_field` lower through loaded-byte exact-size/bounds checks and byte-wise loads; ELF still rejects `consume-expression`
- `read_ref<T>().scalar_field` lowers through `LOAD_CELL Source::CellDep`, loaded-byte exact-size/bounds checks, and byte-wise loads; it can emit CKB-runtime ELF but requires transaction/syscall context
- simple fixed-scalar `create Type { ... }` outputs are verified with `LOAD_CELL Source::Output`, exact-size checks, per-field bounds checks, and equality checks against constants, parameters, consumed/read schema field aliases, or already-computed scalar locals at the source `create` instruction; fixed-byte constants, schema-backed aliases, stack-backed fixed `[u8; N<=8]` parameters, pointer+length `Address` / `Hash` parameters, trusted schema-parameter TypeHash ABI bytes, supported fixed aggregate tuple-array projections, and created Output `TypeHash` fields are verified byte-by-byte for create output fields; verifier-coverable `with_lock(...)` bindings are checked by loading output `LockHash` through `LOAD_CELL_BY_FIELD` field `3`; all verifier-coverable fields must be covered before the verifier claims completeness
- incomplete `create` output verification paths fail closed in generated assembly instead of silently continuing after an audit comment
- symbolic runtime fallback paths for unsupported `transfer` / `claim` / `settle` output shapes, unsupported destroy operands, dynamic collections, unsupported `type_hash` sources, non-lowered field/index access, dynamic `len`, and non-preloaded `read_ref` now fail closed in generated assembly rather than pretending to produce executable verifier semantics; verifier-covered operation output relations reuse prelude output checks, and covered named `destroy` operands emit an executable `GroupOutput` TypeHash absence scan
- metadata now exposes fail-closed runtime features and verifier obligations explicitly, so CI/IDE/audit tools do not need to infer those paths from assembly comments
- metadata now declares `runtime.vm_abi = { format: "molecule", version: 0x8001 }` for VM-facing full-object syscall bytes; RISC-V ELF artifacts also embed a fixed ABI trailer so the exec verifier can select Molecule and strip the trailer before loading the ELF
- compile-result metadata now includes schema version, compiler version, BLAKE3 artifact hash, byte size, path-bound source set hash, path-independent source content hash, source unit hashes, type capability declarations, receipt claim output declarations, operation-tagged Input/CellDep/Output cell access summaries, operation-tagged scheduler witness access records, `mutate_set` replacement ABI, field summaries, checked TypeHash/LockHash preservation status, checked preserved-field equality status, and checked/partial transition status for mutable Cell parameters, checked-static resource-operation obligations, checked-runtime transaction-invariant obligations for verifier-covered transfer/claim/settle outputs plus restricted direct, additive-merge, and matched amount-split resource conservation, runtime-required transaction-invariant obligations for unresolved authorization/finalization/generalized conservation work, checked source-predicate subconditions for the vesting `claim_vested` flow, runtime-required `claim-source-predicate` metadata with `claim-source-predicate-gap` for signer-backed guarded claims, status-classified transaction runtime input requirements for checked and generalized resource-conservation proof coverage plus mutable state transition/equality gaps, checked-runtime shared-state obligations when every covered `&mut shared` field transition is proved, runtime-required `pool-pattern` obligations for Pool creation/mutation/composition semantics, structured `pool_primitives[]` records with checked/runtime component lists, named `invariant_families[]`, field-aware runtime input requirements, and ABI/source details, and runtime-required cell-state obligations for unproved mutable resource/receipt transition fields; `seed_pool` token-pair symbol admission, positive-reserve admission, fee-policy admission, LP supply admission, controlled token-pair asset identity/type-id inequality admission, controlled `launch_token -> seed_pool` pool-id continuity, and controlled `swap_a_for_b` LP supply consistency are checked when backed by source guard CFG/create-output field verification, same-source Pool/LPReceipt output coupling, executable Input TypeHash loads, tuple return ABI coverage, and preserved `Pool.total_lp` equality; `swap_a_for_b` fee-accounting and constant-product pricing expose exact runtime field ABI sources while remaining runtime-required; `add_liquidity` proportional-liquidity accounting, `remove_liquidity` proportional-withdrawal / LP supply consistency, all AMM mutation `reserve-conservation` families, and all AMM mutation `pool-specific-admission` families now expose token/receipt amount, token symbols, Pool reserve/total_lp, Pool token symbols, Pool type_hash, created LP receipt, created token amount/symbol, and remaining Pool field ABI requirements while remaining runtime-required; the compiler validates metadata/artifact/source/trailer consistency before returning or writing artifacts
- claim-created outputs can now reuse the fixed-scalar and schema-backed fixed-byte create-output verifier path when output fields have the same name and type as fields on the consumed receipt, and complete output relation obligations are classified as `checked-runtime`; vesting-style source predicates for DAA/cliff/state/claimable guards are visible as checked subconditions, signer-backed guarded source predicates now expose `claim-source-predicate-gap`, and actual claim authorization/time-lock/witness conditions remain explicit runtime-required obligations
- transfer-created outputs can now reuse the fixed-scalar and schema-backed fixed-byte create-output verifier path when output fields have the same name and type as fields on the consumed asset; verifier-coverable destination lock/address rebinding is checked with `LOAD_CELL_BY_FIELD`, and complete transfer output relation obligations are classified as `checked-runtime`; unmatched multi-output, net-amount, or richer cross-cell conservation remains an explicit runtime-required obligation
- settle-created outputs can now reuse the fixed-scalar and schema-backed fixed-byte create-output verifier path when output fields have the same name and type as fields on the consumed value, and complete output relation obligations are classified as `checked-runtime`; restricted lifecycle final-state settle finalization is also checked when output admission is covered, while non-lifecycle/generalized finalization semantics remain explicit runtime-required obligations
- `cellc verify-artifact` can validate already-emitted artifacts and metadata sidecars in CI without recompiling; `--verify-sources` also checks metadata `source_units[]` against files on disk; `--expect-*hash` pins expected artifact/source/source-content BLAKE3 values; `--production` / `--deny-*` apply the same metadata policy gates to stored artifacts, including `--deny-runtime-obligations`; `--json` emits a machine-readable verification summary with verifier-obligation, Pool invariant-family, and Pool blocker-class counts
- `cellc build` and `cellc check` can enforce metadata policy gates from CLI flags or `Cell.toml [policy]`: `production` rejects fail-closed lowering, `deny_symbolic_runtime` rejects non-standalone Cell/runtime requirements, `deny_ckb_runtime` rejects transaction/syscall runtime requirements, and `deny_runtime_obligations` rejects unresolved runtime-required verifier obligations plus runtime-required Pool invariant families; `check --all-targets` validates both asm and ELF lowering without writing artifacts; `build` applies the gate before writing artifacts
- `cellc doc` includes a lowering audit report and verifier obligation table in generated Markdown/HTML/JSON docs; `--json` emits a machine-readable doc output summary
- explicit `#[effect(...)]` annotations are checked against inferred action behavior, including same-module calls and local `path` dependency imports; under-declared scheduler/effect metadata is now a compiler error
- action signatures with `&mut shared` parameters force `Mutating`, expose the shared type hash in `touches_shared`, default to non-parallel scheduler metadata, and emit `shared-state` / `shared-mutation:<Type>` obligations that become `checked-runtime` once every updated shared output field transition is proved; for `Pool`, separate `pool-pattern` obligations still mark admission and AMM invariant semantics as runtime-required
- mutable Cell replacement identity, preservation, and first transition checks now lower to executable CKB-style checks: for each `mutate_set` replacement, generated assembly loads the Input and Output TypeHash/LockHash fields with `LOAD_CELL_BY_FIELD`, exact-checks both as 32 bytes, and compares every byte before continuing; it also loads Input/Output full cell bytes and byte-compares fixed-width `preserved_fields` that fit the verifier scratch buffer. Simple scalar `field = field +/- operand` transitions are captured in IR and checked when the operand is verifier-coverable, covering `MintAuthority.minted = old + amount`, AMM deltas directly reloadable from parameter fields, and AMM computed-local formulas over add/sub/mul/div/min such as `output`, `lp_amount`, `amount_a`, and `amount_b`. Broader non-covered transition/equality formulas still stay visible as runtime-required obligations and now emit `state-transition-formula-gap` / `state-field-equality-gap` blocker classes.
- action bodies that compose calls returning `shared` values propagate those shared type hashes into caller scheduler metadata, so `launch_token -> seed_pool -> Pool` is visible without a first-class `launch` primitive
- action bodies that create, mutate, or compose values of the shared type named `Pool` now also emit `pool-pattern` obligations: `pool-create:Pool`, `pool-mutation-invariants:Pool`, and `pool-composition:Pool` remain `runtime-required` until pool-pattern admission/invariant semantics are executable
- metadata schema v20 exposes Pool entries as structured `pool_primitives[]` records and separately exposes `transaction_runtime_input_requirements[]` at runtime and action/fn/lock scope, including checked components, runtime-required components, blocker strings/classes, `transfer-output-relation-gap`, `claim-output-relation-gap`, `settle-output-relation-gap`, `state-transition-formula-gap`, `state-field-equality-gap`, checked/runtime `resource-conservation-proof` components plus generalized `resource-conservation-proof-gap`, named `invariant_families[]`, source invariant guard counts, optional binding/callee/Input/Output indices, field-aware runtime input requirements, and transition/preserved field summaries; docgen includes a Pool Pattern Metadata section with invariant-family status/source data, Pool blocker classes, and field-aware runtime input requirements plus transaction blocker classes in generated audit docs; CLI policy/JSON summaries now consume the same invariant-family and blocker-class data
- fixed-width aggregate parameters such as `[(Address, u64); N]` and `[u64; N]` use pointer+length ABI metadata and codegen. Fixed parameter array foreach loops are unrolled, aggregate indexes perform exact-size/bounds checks, tuple fields project by fixed offsets, scalar elements are loaded as values, and `simple_launch` now has no fail-closed runtime features for recipient distribution/locks.
- tuple-valued calls with known return types can project `.0` through `.7` from RISC-V return registers. This covers `launch_token` destructuring of the imported `seed_pool(...) -> (Pool, LPReceipt)` result, so the controlled `launch.cell` example now reports no fail-closed runtime features without changing `type_hash()` semantics.
- `type_hash()` on a value produced by a real `create` instruction now loads that created Output's TypeHash field with `LOAD_CELL_BY_FIELD Source::Output field=5` and can feed fixed-byte output verification. This closes `seed_pool` LPReceipt.pool_id verification.
- `type_hash()` on a named schema parameter now requires a trusted pointer+length TypeHash ABI pair, exact-checks the length as 32 bytes, records `type_hash_pointer_abi` / `type_hash_length_abi` / `type_hash_len: 32` in parameter metadata, and can feed fixed-byte output verification and fixed-byte comparisons. This closes the controlled AMM `add_liquidity` and `remove_liquidity` fail-closed paths without pretending Pool instance identity is a compile-time type-name hash.
- `fn` definitions are enforced as pure helpers with distinct `functions[]` metadata; `fn` can call only `fn`, while `action` and `lock` may call `fn`; `env::*` runtime builtins and `type_hash()` Cell identity builtins are rejected inside `fn` so runtime ABI access cannot be hidden behind helper calls; helpers without a return type are typed as internal `Unit`, emit no call destination, and are legal only as statement calls
- `#[lifecycle(...)]` receipt declarations now participate in the main compile path and LSP diagnostics: duplicate lifecycle states are rejected, lifecycle `state` fields must be unsigned integers, create expressions for lifecycle receipts with a `state` field must set it, static integer state values are range-checked, initial creates must use state `0`, consumed same-type updates cannot statically reset to state `0`, lifecycle states and adjacent transition edges are exposed in type metadata, complete fixed-scalar output verifier paths emit `old_state < state_count`, `new_state < state_count`, and `old_state + 1 == new_state` checks against loaded Input/Output bytes, and the matching verifier obligation is classified as `checked-runtime` instead of `checked-partial`
- simple `consume input.u64_field -> create output.u64_field` aliases are verified by comparing loaded Input and Output fields in the prelude
- simple fixed-byte aliases such as `consume input.owner -> create output.owner`, fixed-byte constants such as `Hash::zero()`, stack-backed `[u8; N<=8]` parameters, pointer+length `Address` / `Hash` parameters, and supported fixed aggregate projections are verified byte-by-byte for create output fields; verifier-coverable output lock hashes are checked byte-by-byte for constants, consumed/read schema-backed fixed-byte aliases, 32-byte fixed parameters, and supported aggregate projections
- simple `consume input.u64_field +/- const_or_param_or_local_const +/- ... -> create output.u64_field` expressions are verified in the prelude for left-associative `u64` add/sub chains; real source-order `create` expressions can additionally verify scalar locals after regular codegen has computed them
- restricted `Token { amount: u64 }`-style resource merges where every consumed same-type Input contributes exactly one `amount` leaf to one created Output's left-associative `u64 Add` expression are classified as `resource-conservation:<T>` `checked-runtime`; restricted one-Input splits where one created Output is `input.amount - split_terms` and every split term is matched by a sibling created `amount` Output are also classified as `checked-runtime`; adversarial compile tests now keep subtractive fee loss, duplicate leaves, missing consumed inputs, duplicate/unmatched split outputs, extra fields, and richer accounting at `runtime-required`
- simple `LoadConst` and `Move` sources propagate into prelude-verifiable `u64` expressions, and explicit scalar `let` annotations are preserved in IR, so local constants, aliases, and narrow lifecycle state values do not silently erase verification

Still not safe to call semantically complete:

- full control-flow lowering
- resource lifecycle semantics end-to-end; current lifecycle checks cover static declaration/create/reset guards plus complete fixed-scalar output verifier transition checks, not all loaded-byte transition cases
- generalized schema decoding from `consume` / `read_ref` / `create` loaded cell bytes
- fixed-byte equality/inequality expressions now lower when both sides are constants, schema-backed fields, small fixed-byte stack values, fixed-byte pointer parameters, or loaded fixed-byte sources such as Output/parameter TypeHash bytes with matching widths; other dynamic fixed-byte comparisons remain fail-closed
- generalized resource conservation and transition relation checks across consumed inputs and created outputs
- full output lock verification for unsupported `with_lock(...)` sources outside constants, schema-backed fixed bytes, and fixed-byte parameters
- arbitrary fixed-byte values derived from unsupported symbolic operations, including `type_hash()` on values without a trusted created-output source
- full create expression/resource-handle executable semantics
- complete cross-module call semantics
- comprehensive pattern lowering
- complete mutation semantics for all language constructs
- complete shared-state and pool-pattern verifier semantics; current coverage includes scheduler-visible metadata, executable replacement TypeHash/LockHash preservation, fixed-width preserved-field equality checks, scalar `old +/- operand` transition checks for verifier-coverable parameter/parameter-field/computed-local operands, and controlled `seed_pool` token-pair TypeHash inequality checks, while broader mutable state formulas expose stable blocker classes and broader Pool-specific invariant/admission semantics are represented as runtime-required `pool-pattern` obligations but are not yet executable or scheduler-enforced

## Validation snapshot

At the time of this snapshot, `cargo test -p cellscript` passed under default features:

- `263` library tests
- `49` CLI integration tests
- `7` examples integration tests
- `0` doctests
- total executed: `319` tests, `0` failures

This is enough to justify “working compiler core under production hardening”, but not enough to justify “complete language toolchain”.

## Non-goals for current snapshot

These should **not** be described as complete today:

- a trusted in-language test runner
- a real language server
- a production-grade package manager / registry client
- a second fully supported backend beyond `asm/elf`
- a full IDE experience beyond the thin VS Code shell-out extension

## Recommended external wording

If you need a short public-facing status line, use:

> `CellScript` is a working compiler/toolchain under production hardening with `asm/elf` compile paths, local package support, and partial semantic lowering. Incomplete semantics must remain explicit and fail closed until fully implemented.

If you need a short internal engineering status line, use:

> Core compiler path is real. Periphery remains limited and policy-gated.
