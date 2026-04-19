# CellScript Design Proposal Implementation Audit

**Snapshot date**: 2026-04-19
**Scope**: `docs/SPORA_DSL_DESIGN_PROPOSAL_CN.md` compared with the current `cellscript/` implementation  
**Purpose**: track design-proposal coverage against code reality, not roadmap intent.

This document should be read together with:

- [CELLSCRIPT_IMPLEMENTATION_STATUS.md](./CELLSCRIPT_IMPLEMENTATION_STATUS.md)
- [CELLSCRIPT_EXECUTION_PHASES.md](./CELLSCRIPT_EXECUTION_PHASES.md)
- [CELLSCRIPT_COMPATIBILITY_MATRIX.md](./CELLSCRIPT_COMPATIBILITY_MATRIX.md)
- [SPORA_DSL_DESIGN_PROPOSAL_CN.md](./SPORA_DSL_DESIGN_PROPOSAL_CN.md)

## Executive Verdict

CellScript is no longer only a parser or syntax demo. It has a real compiler path, real metadata, partial CKB-style runtime lowering, a local CLI workflow, and increasingly strict fail-closed behavior.

It is still not a complete implementation of the design proposal.

V1 core-language convergence is a boundary-quality gate, not a claim that every generalized protocol semantic is executable. The v1 executable core is the Cell lifecycle language: `resource`, `shared`, `receipt`, `consume`, `create`, `transfer`, `destroy`, `claim`, `settle`, `action`, `fn`, and `lock`. First-class `launch`, first-class `pool`, user-defined generics, registry distribution, schema migration, and executable Wasm are excluded from this gate.

The known v1-core residual gaps are now policy-visible through stable blocker classes: `transfer-output-relation-gap`, `resource-conservation-proof-gap`, `claim-source-predicate-gap`, `finalization-policy-gap`, and `linear-collection-ownership-gap`. These blocker classes are covered by CLI JSON and `--deny-runtime-obligations` regressions, and the post-change `cargo test -p cellscript` gate passed, so v1 core-language convergence is closed for the executable-core boundary gate.

Current implementation facts:

- `cellscript/src/` contains `42,274` lines of Rust across `26` source files.
- `cellscript/src/` plus `cellscript/tests/` contains `47,221` lines of Rust across `28` files.
- `366` `#[test]` declarations are present in source/test files.
- A fresh default-feature `cargo test -p cellscript` run executed `353` tests: `287` library tests, `59` CLI integration tests, `7` examples integration tests, and `0` doctests. All passed.
- The repository includes `7` bundled `.cell` examples: `token`, `amm_pool`, `vesting`, `launch`, `nft`, `multisig`, and `timelock`.

Approximate implementation status:

| Area | Current coverage | Verdict |
|---|---:|---|
| Lexer / parser / AST | 88-92% | Stable main path for supported syntax |
| Type checking / linear checks | 78-85% | Stronger value/resource checks, including stable/unique schema field names, reference-free schema/enum payload storage, reference-free `Vec<T>` payload/push checks, Cell-backed `Vec<T>` rejection through pure/non-action Cell ownership signature gates, stable/unique callable parameter names, parser-level rejection of the legacy `ref` parameter modifier, top-level-only callable reference parameters, reference-to-Cell-aggregate rejection, action-only `&mut` state authority, state-transition-free lock predicates, Cell-free pure helper owned parameters/returns, reference-free callable return types, no local binding reuse or visible-scope shadowing, named-root-only assignment targets, read-only reference assignment rejection, local read-only reference alias rejection for linear Cell roots including aggregate/branch/block stored results and assignment RHS paths, local/assignment storage rejection for `&mut` reference aliases, duplicate `&mut` call-root rejection when a mutable parameter slot is involved including block/branch/match wrapper expressions, leading-`mut` Cell/read-ref parameter rejection, owned-linear field/index assignment rejection, linear `let` move semantics, aggregate-contained linear type tracking, wildcard-discard rejection, conservative rejection of linear field/index aggregate projection, explicit branch-return, tail-if return, `if` expression ownership merging, `match` expression arm ownership merging, block-expression parent-scope linear state propagation, block-tail-if value typing/linear merging, block-local linear completion checks, and conservative loop-local completion plus parent-state preservation checks; still not full semantic proof |
| IR and metadata | 76-86% | Real, includes verifier obligations and schema v22 target profile metadata; still not full protocol semantics |
| Pure compute lowering | 78-88% | Usable subset with stricter return/value semantics |
| CKB-style runtime lowering | 60-70% | Partial and intentionally fail-closed where semantics are incomplete |
| Stateful protocol primitives | 23-33% | Mostly not executable |
| DAG scheduler integration | 80-86% | Metadata and per-action scheduler witness bytes exist; `read_ref`, `&mut shared`, composed shared-return touches, and mutable Cell `mutate-input` / `mutate-output` access records are visible; mutable shared-state and mutable authority cell-state transition obligations are explicit; schema v22 keeps scheduler witnesses limited to Input/CellDep/Output cell-state accesses; `ActionMetadata::scheduler_witness_bytes()` exposes compiled witness bytes, and `spora-exec` CellTx can attach/discover/admit CellScript scheduler witnesses by `0xCE11` magic/version, decode/admit the Borsh envelope with magic/version/count, effect/operation/source, operation/source compatibility, concrete transaction source-index bounds checks, exact trusted operation/source/index/binding_hash access-set matching, and produce a trusted access summary from compiled metadata bytes while appending the witness to a concrete transaction. Consensus MPE `BlockAccessSummary` consumes admitted witnesses, has a strict trusted-access-set constructor that rejects missing/mismatched builder or compiled-metadata summaries before merge, and uses shared read/write touch domains for DAG serialization. Mempool validation and template prefiltering now reject malformed CellScript scheduler metadata; mempool entries and template selectors can carry producer-backed trusted summaries into the strict template policy path. Wallet transaction generation can attach a compiled scheduler witness to the final transaction and expose the returned trusted summary on `PendingTransaction`; focused mining coverage proves producer-returned summaries survive sidecar insertion into selector exposure, and focused consensus coverage proves selector-provided builder summaries are consumed/rejected by strict template prefiltering. RPC trusted-summary submission/authentication and broader adversarial/property coverage remain open |
| CLI local workflow | 74-84% | Usable local developer loop with pre-artifact CLI/manifest metadata policy gates and explicit `spora` / `ckb` / `portable-cell` target-profile selection; artifact-producing non-`spora` profiles fail closed, while `cellc check --target-profile ckb|portable-cell` now runs portability classification and reports concrete blockers |
| IDE/LSP/tooling ecosystem | 35-50% | Useful metadata surfaces, not full semantic IDE |
| Overall design proposal completion | 70-75% | Real implementation progress, not production-complete |

The critical distinction is:

> The compiler core is real. The full protocol language promised by the design proposal is not complete yet.

## Corrected Status Compared With Earlier Audits

Several older audit claims are now stale:

| Older claim | Current reality |
|---|---|
| Effects are mostly decorative metadata | Incorrect now. Explicit `#[effect(...)]` under-declarations are rejected for direct operations, same-module calls, and local `path` dependency imports. |
| `fn` can hide stateful behavior | Mostly fixed for local code. `fn` definitions are distinct AST/IR/metadata entries, must infer `Pure`, cannot call `action`, `lock`, `env::*` runtime builtins, or `type_hash()` Cell identity builtins, and reject direct, same-module indirect, and local imported stateful behavior. |
| Create output checks are only comments | Incorrect for the supported subset. Fixed-scalar output fields can be checked with exact-size, bounds, and equality checks; fixed-byte constants, schema-backed fixed-byte aliases, stack-backed `[u8; N<=8]` parameters, pointer+length `Address` / `Hash` parameters, trusted schema-parameter TypeHash ABI bytes, and created Output TypeHash fields can be checked byte-by-byte for output fields. Verifier-coverable `with_lock(...)` bindings now compare output `LockHash` through `LOAD_CELL_BY_FIELD`; unsupported field or lock sources still fail closed. |
| Symbolic runtime paths may silently continue | Improved. Unsupported runtime features emit explicit fail-closed assembly and metadata. |
| Scheduler metadata has no fail-closed signal | Improved. Metadata exposes `fail_closed_runtime_features` separately from `symbolic_runtime_features`, including direct create output verifier blocker classes, dedicated Cell-backed collection markers, and `linear-collection-ownership-gap` runtime input blockers for action-visible `Vec<CellType>` paths that still require a real linear collection model. |
| No-return helpers behave like `u64` values | Fixed for the local compiler. Helpers without a return type use internal `Unit`, lower to destinationless calls, and cannot be bound or returned as values. |
| `assert_invariant` behaves like a boolean value | Fixed for the local compiler. Assertions are `Unit`, lower to fail-closed verifier CFG, cannot be bound or used as value-returning tail expressions, and require static string literal messages. |
| Source after `return` can be silently ignored by lowering | Fixed for guaranteed-return source paths. The type checker rejects unreachable statements after `return` or complete branch returns. |
| Unknown call return types become implicit `u64` | Fixed in IR lowering. Unresolved call return types are rejected instead of fabricating a numeric result. |
| Tail expressions are only source sugar | Improved. Typed tail expressions and terminal `if` tails lower to real `Return(Some(...))` terminators for value-returning `action` / `fn` bodies. |
| Empty arrays silently become `[u64; 0]` | Fixed. Empty arrays require explicit zero-length array annotations and preserve the declared element type. |
| Local `Vec` values have no item-type enforcement | Improved. `Vec.push` propagates the first concrete item type from `Vec::new()` and rejects incompatible later pushes. |
| SchedulerWitness binary format is missing | Too broad. The compiler generates Borsh scheduler witness bytes with magic `0xCE11` in action metadata as `scheduler_witness_borsh_hex`; schema v22 filters those records to scheduler-visible cell-state accesses; `ActionMetadata::scheduler_witness_bytes()` exposes compiled bytes; `spora-exec` now has low-level CellTx witness placement/discovery helpers, a compiled-metadata producer helper, and a compatible decoder/admission check for envelope, enum, operation/source, transaction source-index validity, and exact access-set matching against trusted summaries; consensus MPE block summaries consume admitted witnesses for shared read/write DAG conflicts, mempool/template policy gates reject malformed metadata with strict trusted-summary fixtures, mining stores/propagates producer-backed summaries through mempool entries and selectors, the wallet generator can attach a compiled scheduler witness to the final transaction while preserving the returned trusted summary on `PendingTransaction`, focused mining coverage proves producer-returned summaries reach selector exposure, and focused consensus coverage proves selector-provided builder summaries are consumed/rejected by strict template prefiltering. Remaining hardening is broader adversarial/property coverage plus any explicit RPC trusted-summary submission/authentication path. |
| Lifecycle validation is not on the main compiler path | Stale. `lifecycle::check` is called by the main compile path and metadata path, but only declaration/static create/reset and restricted fixed-scalar transition checks are trusted today. |
| CLI subcommands are only a disconnected skeleton | Stale. The local workflow subcommands are wired into the main binary; registry/runtime execution surfaces remain fail-closed or feature-gated. |

The main negative findings remain valid:

- `transfer`, `claim`, and `settle` are still not complete executable protocol semantics; `destroy` now has a restricted executable grouped-output TypeHash absence scan, but full burn/conservation policy is still incomplete.
- `mint`, `burn`, `wrap`, and `unwrap` are not complete standard operations; `launch`, `seed_pool`, and `swap` remain transaction-builder/protocol-pattern targets rather than v1 language primitives.
- generalized resource conservation is not implemented.
- lifecycle rules now have main-path and LSP declaration/static create-state checks plus explicit state/transition metadata, but full transition verification remains incomplete.
- full DAG scheduling enforcement is still partially connected: MPE block summaries consume admitted shared-touch witnesses, the strict runtime path can require trusted access-set matching before merge, mempool validation and template prefiltering reject malformed CellScript scheduler metadata, mempool entries/template selectors can carry producer-backed summaries, the wallet generator can attach compiled scheduler witnesses, and the first adversarial summary/policy tests cover malformed, illegal, out-of-bounds, underreported, forged, missing, mismatched, transaction-shape-incompatible, selector-propagated witness summaries, and selector-provided builder summaries consumed/rejected by strict template prefiltering. Remaining work is broader adversarial/property coverage and any required external RPC trusted-summary submission/authentication path.

## Design Section Coverage

### 0. IR Shape Differences From The Proposal

The proposal sketches a `SporaIR` shape with fields such as `lifecycle_rules`, `write_intents`, and SSA-like basic blocks. The actual implementation is deliberately different in several places:

| Proposal shape | Current implementation | Status |
|---|---|---|
| `consume_set`, `read_refs`, `create_set` | Present on `IrBody` as operation-tagged summaries | Implemented |
| `mutate_set` | Present on `IrBody` as an operation-tagged mutable Cell parameter summary with binding/type/field names, replacement `Input#N -> Output#N` ABI, type/lock preservation requirements, checked TypeHash/LockHash preservation status, transition fields, preserved fields, field equality status, and simple scalar transition summaries | Implemented as audit/scheduler metadata plus executable TypeHash/LockHash preservation, fixed-width preserved-field equality checks, and scalar `old +/- operand` transition checks for verifier-coverable parameter, parameter-field, and add/sub/mul/div/min computed-local operands; non-covered transition/equality gaps now emit `state-transition-formula-gap` / `state-field-equality-gap` runtime input blocker classes; Pool-specific invariant/admission debt is separately exposed as runtime-required `pool-pattern` obligations and structured `pool_primitives[]` records with named checked/runtime invariant families and field-aware runtime input requirements; broader formula classes still missing |
| `effect_class` | Present on `IrAction` and exposed in metadata | Implemented |
| `scheduler_hints` | Present on `IrAction`; metadata includes `parallelizable`, `touches_shared`, `estimated_cycles`, and scheduler witness bytes | Implemented as metadata |
| `lifecycle_rules: Vec<StateTransition>` | IR now carries explicit per-type lifecycle transition rules derived from declared lifecycle state order, and metadata emits transitions from those IR rules; verifier checks still cover only the supported fixed-field lifecycle subset | Partial |
| `write_intents` | Present on `IrBody` as output/replacement-output write summaries derived from `create_set` and `mutate_set` | Implemented for current write-producing IR summaries |
| SSA body | Actual IR uses `IrBody`, `IrBlock`, `IrInstruction`, and explicit temporaries; it is not a formal SSA contract | Different implementation |
| Pure helper functions | Actual IR includes `IrPureFn`, which the proposal did not separately model | Implementation extension |

### 1. Core Semantic Model

| Design concept | Current code status | Coverage |
|---|---|---:|
| `resource` | Parsed, typed, lowered into IR, participates in partial linear checks and metadata; local `let` bindings over linear values move the source instead of copying it, aggregate types containing linear values are linear, wildcard bindings cannot discard linear values, field/index projection cannot move linear elements out of aggregate values before partial-move semantics exist, explicit branch returns, tail-if return expressions, `if` expression branches, `match` expression arms, block-expression scopes, and block expressions whose tail statement is an `if` with `else` now propagate parent-visible linear ownership state while rejecting unhandled block-local linear values and inconsistent branch/match moves; operation-tagged `consume` / `transfer` / `destroy` / `claim` / `settle` operands emit checked `<op>-input-data` transaction input components for the concrete Input data-load path, direct `create` outputs now emit status-classified `create-output:<T>:<binding>` obligations with checked field/lock components for verifier-covered shapes and blocker classes for uncovered shapes, one external resource Input preserved into one same-type Output by direct verifier-covered field aliases, restricted single-field `amount: u64` additive merges from multiple same-type Inputs into one Output, and restricted one-Input matched `amount` subtraction splits into sibling Outputs are now classified as `resource-conservation:<T>` `checked-runtime` and surfaced as checked `resource-conservation-proof` transaction input components | 89-94% |
| `shared` | Parsed and represented; `read_ref`, `&mut shared` parameters, composed calls returning shared values, trusted schema-parameter TypeHash ABI sources, checked `read-ref-cell-dep-data` transaction components for concrete CellDep data loads, replacement-bound `mutate_set` summaries, executable replacement TypeHash/LockHash checks, fixed-width preserved-field equality checks, and scalar `old +/- operand` transition checks for verifier-coverable parameter, parameter-field, and add/sub/mul/div/min computed-local operands are scheduler/verifier-visible; non-covered shared transition/equality formulas are now policy-visible through stable transaction runtime input blocker classes; Pool-specific invariant/admission debt is explicitly reported as runtime-required `pool-pattern` obligations and schema v22 structured `pool_primitives[]` metadata with named checked/runtime invariant families, field-aware runtime input requirements, blocker strings, and stable blocker classes; first-class executable shared/pool invariant semantics are missing | 78-84% |
| `receipt` | Parsed and represented; lifecycle declarations are statically checked; `receipt Name -> Output` claim outputs are type-checked and represented in IR/metadata; same-name same-type fixed-scalar and verifier-coverable fixed-byte output fields can be checked against consumed receipt bytes, the vesting example's computed scalar create outputs are source-order verified, complete fixed-field lifecycle updates are reported as `checked-runtime`, verifier-covered claim output relation obligations are not reported as unresolved runtime requirements, and claim output relations now emit status-classified `claim-output-relation` transaction input components with `claim-output-relation-gap` for unsupported output shapes; native `claim receipt` paths with a fixed `[u8; 20]` signer field named `signer_pubkey_hash`, `claim_pubkey_hash`, `owner_pubkey_hash`, `beneficiary_pubkey_hash`, or `pubkey_hash` now lower `GroupInput` witness envelope checks, canonical ECDSA authorization-domain loading, `SECP256K1_VERIFY`, and signer-key binding, and classify `claim-conditions:<Receipt>` as `checked-runtime` when any source-level `assert_invariant` / DAA predicates also have verifier-covered guards; guarded claims only stay top-level `runtime-required` with `source-predicate=runtime-required` when those source predicates are not fully covered, while keeping signature/key-binding subconditions checked and exposing `claim-source-predicate-gap`; receipts without such a field still keep signature verification/runtime claim policy requirements runtime-required | 75-79% |
| `launch` | Reserved and explicitly rejected in expression position until post-v1 transaction-builder lowering exists; controlled launch examples can still be modeled as ordinary actions using explicit `create` operations, shared Pool scheduler touches through `seed_pool` composition, fixed tuple-array distribution checks, and mutable MintAuthority replacement checks | 26-30% |
| pool pattern | Not a first-class language primitive; AMM pools are ordinary `shared` types plus actions and metadata. `&mut Pool` parameters and composed Pool returns produce scheduler `touches_shared` metadata, created Pool output TypeHash can verify `seed_pool` LPReceipt identity, replacement Pool TypeHash/LockHash preservation and fixed-width preserved-field equality are executable, controlled AMM reserve/LP transitions can be `checked-runtime`, controlled `seed_pool` checks token-pair identity admission by loading Input `token_a` / `token_b` TypeHash fields and rejecting equal 32-byte identities, controlled `launch_token -> seed_pool` discharges pool-id continuity through tuple return ABI, and controlled `swap_a_for_b` discharges LP supply consistency through preserved `Pool.total_lp` equality; broader AMM admission/economic invariant families remain protocol-pattern obligations exposed through `pool_primitives[]` with stable Phase-2-deferred blocker classes, not language-core semantics | 70-75% |
| `settle` | Parsed and lowered as operation-tagged consume plus settle-created output; the consumed Input data prelude load now emits checked `settle-input:<T>:<binding>` / `settle-input-data` metadata; same-name same-type fixed-scalar and schema-backed fixed-byte output fields can be verifier-checked against consumed value bytes, complete output relation obligations are classified as `checked-runtime`, fully verifier-covered settle output relations no longer emit expression-level symbolic fail-closed paths, and settle output relations now emit status-classified `settle-output-relation` transaction input components with `settle-output-relation-gap` for unsupported output shapes; lifecycle-backed settle paths with a fixed-scalar `state` field now verify Input/Output state equals the final lifecycle index, mark `settle-final-state-context` as `checked-runtime`, and classify `settle-finalization:<T>` itself as `checked-runtime` when the same settle-created output admission is fully covered; non-lifecycle/non-coverable finalization details still expose field-aware consumed-input requirements plus structured blocker metadata with `finalization-policy-gap`; generalized finalization semantics remain runtime-required, and unsupported output shapes still fail closed | 40-48% |
| transaction-local `let` values | Ordinary locals are parsed, typed, and lowered; they do not produce CellStateTree outputs unless used in `create` | 100% |
| CellStateTree commit via `create` | Cell-backed outputs are represented through the existing resource/shared/receipt creation model; no separate marker keyword is modeled | 70% |

Verdict: the vocabulary exists for several design concepts, but the executable protocol semantics are still partial. The language can describe intended resource/state operations more clearly than raw CKB scripts, but for many operations it still cannot prove or execute the intended transition.

### 2. Type System

| Feature | Current code status | Coverage |
|---|---|---:|
| Primitive integers / bool / hash-like values | Real parser/type/codegen support for core scalar paths | 80-90% |
| Fixed arrays | Supported in frontend/type checking; empty arrays require explicit zero-length annotations; local static index/foreach/len lowering exists, and fixed aggregate parameters such as `[u64; N]` / `[(Address, u64); N]` now lower through pointer+length ABI with static foreach unrolling in supported cases | 74-82% |
| Struct/resource/shared/receipt/enum shapes | Real AST/IR/type presence; schema field names must be stable and unique before layout/metadata lowering, field-less enum variants lower as discriminants, unknown/payload enum variant values are rejected when lowering would be unsound, enum match checks unknown/duplicate/non-exhaustive arms, and payload variant patterns are rejected until payload destructuring lowering exists | 80-84% |
| Linear usage checks | Present and useful across direct moves, aggregate-contained linear values, wildcard-discard rejection, conservative rejection of linear field/index aggregate projection, branch merges, tail returns, `if` expressions, `match` expressions, block-expression parent-scope propagation, block-tail-if value branches, block-local completion, loop-local completion, and conservative rejection of parent-visible loop ownership changes, but not a full resource proof system | 74-80% |
| Capabilities such as store/transfer/destroy | Parser/type checker now merge attribute and inline declarations, reject `transfer` without `transfer`, reject `destroy` without `destroy`, restrict `claim` to receipts, require declared receipt claim outputs to be resource/shared cells, and restrict `settle` to cell-backed linear values; full conservation/runtime proof is still incomplete | 60-67% |
| Immutable vs mutable fields | Not fully enforced as first-class transition constraints | 20% |
| Schema evolution/versioning | `#[type_id("...")]` stable type identity is parsed for `resource` / `shared` / `receipt` / `struct`, duplicate IDs are rejected across the visible module scope including imported types, IR carries the string, and metadata schema v22 emits both `types[].type_id` and a BLAKE3 hash. This is metadata identity only; executable CKB type-id lineage verification and schema migration rules remain incomplete | 28-38% |
| Lifecycle declaration/runtime checks | Main-path checks reject duplicate states, invalid state field types, missing create `state`, static out-of-range create states, non-initial static creates, and static reset-to-initial updates; complete fixed-scalar verifier paths emit state-range and `old_state + 1 == new_state` prelude checks and are classified as `checked-runtime` obligations | 50-58% |
| User-defined generics | Deliberately downgraded out of the v1 executable core per `CELLSCRIPT_CKB_COMPATIBILITY_DECISION.md`; parser/type checker reject generic type definitions and user-defined instantiations. Post-v1 template/codegen may generate concrete specialized `.cell` modules with generated/declarable Molecule schemas; `Vec<T>` remains a controlled builtin collection notation | N/A for v1 core |
| Result/Option/error propagation | Result/Option types and `?` propagation are not implemented as designed; `Option` / `Result` are reserved but rejected in user type positions, and `unwrap` / `expect` / `unwrap_or` are explicit compile errors in consensus code | 10-15% |

Verdict: the type system now rejects several previous false-value edges, but still cannot claim Move-grade resource safety or Solidity-grade practical completeness.

### 3. Syntax

| Syntax area | Current status | Coverage |
|---|---|---:|
| Module/type/action/lock syntax | Real | 85-95% |
| `consume` / `create` syntax | Real, with checked operation-specific consumed Input data-load metadata, checked/gap direct create output field/lock components, and partial executable checks | 72-82% |
| `read_ref` | Real for restricted fixed-scalar CKB CellDep field access, with checked CellDep data-load transaction metadata for expression and parameter paths | 76-84% |
| `transfer` | Parsed/lowered with operation-tagged consumed Inputs and transfer outputs; the consumed Input data prelude load now emits checked `transfer-input:<T>:<binding>` / `transfer-input-data` metadata; verifier-covered same-name same-type fixed-scalar and schema-backed fixed-byte output fields are checked against consumed asset bytes, verifier-coverable `Address` destinations are checked against the transfer-created Output LockHash, fully covered transfer output relations no longer emit `transfer-expression` fail-closed metadata, and `transfer-output:<T>` transaction invariants plus `transfer-output-relation` runtime input components are classified as `checked-runtime` for that covered subset; unsupported transfer output relations expose `transfer-output-relation-gap`; generalized multi-cell transfer conservation remains runtime-required, and unsupported output shapes still fail closed | 46-54% |
| `destroy` | Parsed/lowered; the consumed Input data prelude load now emits checked `destroy-input:<T>:<binding>` / `destroy-input-data` metadata; named cell-backed operands now emit an executable `GroupOutput` TypeHash absence scan that distinguishes scan end from missing type scripts, but full burn/conservation semantics remain incomplete | 30-38% |
| `claim` | Parsed/lowered with operation-tagged consumed receipts and claim outputs; the consumed Input data prelude load now emits checked `claim-input:<T>:<binding>` / `claim-input-data` metadata; declared `receipt -> output` cells are visible to type checker/IR/metadata, same-name same-type fixed-scalar and verifier-coverable fixed-byte output fields are checked against consumed receipt bytes, source-order `create` verification covers the vesting claim output formulas, complete output relation obligations are classified as `checked-runtime`, fully covered claim output relations no longer emit `claim-expression` fail-closed metadata, and unsupported claim output relations now expose `claim-output-relation-gap` through transaction runtime input metadata. Runtime-required claim condition details expose field-aware consumed-input requirements plus structured witness/signature/time/source-predicate runtime input metadata, and the `claim_vested` flow reports checked DAA/cliff/state/claimable/witness-format/authorization-domain subconditions; generated assembly now loads `GroupInput` witness bytes, enforces the 65/66-byte signature envelope, loads the canonical ECDSA sighash for domain separation, and emits `SECP256K1_VERIFY` when the consumed receipt exposes an explicit fixed 20-byte signer pubkey hash field; native claims with that signer-field ABI classify `claim-conditions:<Receipt>` itself as `checked-runtime` when source-level `assert_invariant` / DAA predicates are covered by checked guards, while signer-backed guarded claims stay top-level `runtime-required` and expose `source-predicate=runtime-required` plus blocker class `claim-source-predicate-gap` only when those guards are incomplete; generalized signer policy and time-lock semantics are still incomplete | 60-67% |
| `settle` | Parsed/lowered with operation-tagged consumed Inputs and settle outputs; the consumed Input data prelude load now emits checked `settle-input:<T>:<binding>` / `settle-input-data` metadata; fixed-scalar and schema-backed fixed-byte output preservation can be checked against consumed value bytes, complete output relation obligations are classified as `checked-runtime`, fully covered settle output relations no longer emit `settle-expression` fail-closed metadata, unsupported settle output relations now expose `settle-output-relation-gap` through transaction runtime input metadata, field-aware consumed-input requirements plus structured final-state/output-admission runtime input metadata are exposed, and lifecycle-backed fixed-scalar `state` finality plus output admission is checked for the supported path, allowing restricted `settle-finalization:<T>` to become `checked-runtime`; non-lifecycle/generalized finalization semantics remain runtime-required with blocker-class tags, and unsupported output shapes still fail closed | 40-48% |
| `launch` | Reserved and rejected as an expression until post-v1 transaction-builder lowering exists; ordinary actions can model current launch examples with explicit creates, verifier-coverable fixed tuple-array distribution paths, real tuple-return register ABI for composed `seed_pool` calls, checked controlled `pool-id-continuity`, and mutable MintAuthority replacement checks | 30-34% |
| `assert_invariant` | Lowers to fail-closed CFG, is typed as value-less `Unit`, and requires static string literal messages; full invariant proof/lowering story remains incomplete | 58-68% |
| Tail expressions / value returns | All value-returning `action` / `fn` paths must return; typed tail expressions and terminal `if` branches lower to real return terminators | 70-80% |
| `?` / Result propagation | Not implemented; hidden failure helpers `unwrap` / `expect` / `unwrap_or` are rejected | 5-10% |

Verdict: syntax is significantly ahead of executable semantics. This is acceptable only if every unsupported path remains fail-closed and clearly surfaced in metadata, which is now mostly true.

### 4. Compiler Pipeline

| Pipeline stage | Current status | Coverage |
|---|---|---:|
| Lexer | Stable main path | 90% |
| Parser | Stable main path for supported syntax | 88-92% |
| AST | Stable main path for supported syntax | 88-92% |
| Name/module resolution | Local path dependencies work; remote/registry story incomplete | 60-70% |
| Type checking | Useful and stricter on schema field identity, callable parameter identity, local binding identity, named-root/read-only-reference/owned-linear assignment identity, callable argument count/type checks, returns, unreachable statements, assertions, empty arrays, `Unit`, local `Vec` item propagation, aggregate-contained linear values, and scoped linear-state propagation; still not full semantic proof | 75-82% |
| IR lowering | Real, with action/lock/function/effect metadata, destinationless no-return calls, Unit-valued assertions, typed empty arrays, tail-return terminators, fixed aggregate index/projection, fixed parameter foreach unrolling, and known tuple-call return projection | 79-89% |
| Optimization | Not part of the trusted path | 10-15% |
| RISC-V assembly codegen | Real for pure and restricted runtime paths | 60-70% |
| RISC-V ELF output | Real for pure/restricted executable paths | 50-60% |
| Wasm | Metadata-only/fail-closed path, not executable backend | 10-15% |

Verdict: the main compiler pipeline is real. The trusted path should still be described as restricted, not production-complete.

### 5. Runtime / Execution Model

| Runtime requirement | Current status | Coverage |
|---|---|---:|
| Pure CKB-VM-compatible ELF | Real for no-argument pure programs | 70% |
| Parameter ABI | Real pointer+length ABI for fixed schema parameters and >8-byte fixed-byte values such as `Address` / `Hash` | 65-75% |
| `LOAD_CELL Source::Input` | Used for restricted operation-tagged consumed-input field/data access across consume/transfer/destroy/claim/settle | 48-58% |
| `LOAD_CELL Source::CellDep` | Used for restricted `read_ref<T>().field` access | 55-65% |
| `LOAD_CELL Source::Output` | Used for restricted fixed-scalar and fixed-byte `create` output verification; fully covered direct creates now emit checked field/lock transaction input components; verifier-coverable locked outputs load `LockHash`, created-output identity loads `TypeHash` through `LOAD_CELL_BY_FIELD`, and schema-parameter TypeHash ABI bytes can feed output checks; unsupported lock/type-hash sources still fail closed | 62-72% |
| Full `consume` expression semantics | Not complete; ELF remains fail-closed | 20% |
| Full `create` resource-handle semantics | Not complete; restricted verifier prelude plus checked/gap transaction metadata exists | 30-40% |
| Full lock/type script semantics | Partial entrypoint handling; witness/signature semantics incomplete | 25-35% |
| Lifecycle transition verification | Declaration, static create-state, static reset, state/transition metadata exposure, LSP diagnostics, complete fixed-scalar prelude checks, and `checked-runtime` obligation classification for complete fixed-field paths exist; dynamic/nested/locked output transition legality is not fully verified | 45-55% |
| Full transaction invariant checks | Not complete | 20% |

Verdict: CKB-style runtime integration is no longer imaginary, but only a narrow subset has concrete verifier lowering.

### 6. Standard Operations And Protocol Patterns

| Surface | Current status | Coverage |
|---|---|---:|
| `launch` | Not implemented as executable post-v1 transaction-builder lowering; expression-position use is explicitly rejected | 5-10% |
| `mint` | Not implemented as a standard primitive; `&mut MintAuthority` updates now emit explicit `mutable-cell:MintAuthority` metadata, executable replacement TypeHash/LockHash preservation checks, fixed-width preserved-field equality checks, and a scalar `minted = old + amount` transition check | 15-20% |
| `burn` / `destroy` | `destroy` recognized and now checks grouped-output TypeHash absence for named cell-backed operands; generalized burn policy and conservation accounting remain incomplete | 24-30% |
| `transfer` | recognized, can map same-name same-type fixed-scalar and schema-backed fixed-byte fields into output checks, can check verifier-coverable destination lock rebinding through Output LockHash, verifier-covered output relations no longer fail closed at the expression, covered `transfer-output:<T>` obligations are `checked-runtime`, and uncovered output relations now report `transfer-output-relation-gap`; generalized transfer conservation policy remains incomplete | 42-50% |
| `seed_pool` | Not a language primitive; ordinary action examples compile, created `Pool` outputs are scheduler-visible, callers can propagate Pool touches, created Pool output TypeHash can verify LPReceipt.pool_id, and `pool-create:Pool` exposes pool-pattern admission/invariant obligations in structured `pool_primitives[]` metadata. Token-pair symbol, positive-reserve, fee-policy, LP supply, and controlled token-pair asset identity/type-id inequality admission are covered for verifier-supported source/create-output/Input TypeHash patterns; generalized Pool identity/admission policy remains incomplete | 49-57% |
| `swap` | Not a language primitive; ordinary `&mut Pool` swap actions expose shared Pool scheduler touches and checked general shared-mutation obligations for supported formulas. Pool-specific fee accounting, constant-product pricing, LP consistency, and AMM economics remain protocol-pattern runtime requirements exposed through metadata | 23-27% |
| `wrap` / `unwrap` | Not implemented | 0-5% |
| `claim` | recognized, declared output type is visible to type checker/IR/metadata, verifier-covered output relations no longer fail closed at the expression, and witness envelope / authorization-domain checks exist for supported paths; generalized authorization remains runtime-required | 24-32% |
| `settle` | recognized, can map same-name same-type fixed-scalar and schema-backed fixed-byte fields into output checks, verifier-covered output relations no longer fail closed at the expression, and lifecycle final-state checks for fixed-scalar `state` can close restricted finalization when output admission is also covered; full generalized finalization semantics remain runtime-required | 34-42% |

Verdict: this is the largest gap between the design proposal and implementation. The standard operation layer and protocol-pattern metadata are still mostly design targets.

## Protocol Use-Case Coverage

| Use case | Current classification | Reason |
|---|---|---|
| Lock-style authorization | Expressible but under-specified | `lock` exists, bool return is enforced, but signature/witness/domain binding is incomplete. |
| Type-script state transition validation | Expressible only in restricted cases | Simple fixed-scalar input/output checks and fixed-byte preservation checks exist for no-lock outputs; generalized and locked transitions are missing. |
| Fungible assets / UDT invariants | Ambiguous / under-specified | Can model shapes, but generalized conservation/issuance/burning checks are incomplete. |
| NFT / singleton objects | Expressible but awkward | Resource syntax helps, but identity/type-id/versioning semantics are incomplete. |
| Vault / CDP / lending machines | Not properly expressible safely | Requires cross-cell invariants, prices, liquidation rules, witness proofs, and partial updates beyond current lowering. |
| DAO / governance transitions | Expressible only with unsafe off-chain burden | Multicell invariants and voting state transitions are not first-class enough yet. |
| Order matching / settlement | Not properly expressible safely | `settle` can expose restricted output field preservation, but finalization, intent binding, and witness binding are not complete. |
| Multi-party signing / delegated authority | Under-specified | Needs first-class witness/signature domains and replay resistance. |
| Upgrade / migration | Under-specified | No complete schema evolution/versioning model. |
| Capability boundaries | Partial | Capabilities exist syntactically, but enforcement is incomplete. |
| Resource conservation | Partial restricted subset | Simple fixed-scalar output equality, schema-backed fixed-byte preservation, and u64 arithmetic checks exist; direct one-external-input/one-created-output same-type field aliases, restricted single-field `amount: u64` additive merges from every consumed same-type Input into one Output, and restricted one-Input `amount - split_terms` splits where every split term is matched by a sibling created Output are classified as `resource-conservation:<T>` `checked-runtime` and emit checked `resource-conservation-proof` transaction input components; adversarial compile tests cover duplicate amount leaves, missing consumed input leaves, duplicate/unmatched split outputs, and extra fields so fee loss, extra-field, and broader cross-cell accounting stay runtime-required with a stable `resource-conservation-proof-gap` blocker class in transaction runtime input metadata. |
| Cross-cell invariants | Mostly missing | Metadata helps, verifier semantics are incomplete. |
| Transaction-level invariants | Mostly missing | No complete invariant language/lowering. |
| Composability | Partial local module support | Local path dependency effects are propagated; registry summaries and semantic composition remain incomplete. |

## Soundness and Security Risks

### 1. False completeness through syntax

Many high-value keywords exist before their executable semantics are complete. This is dangerous because examples can compile while the real protocol invariant is either fail-closed or not expressible.

Mitigation already present:

- unsupported runtime operations fail closed
- `fail_closed_runtime_features` and `verifier_obligations` appear in metadata
- docs now warn against claiming complete stateful semantics

Remaining need:

- CI gates that reject production builds with non-empty `fail_closed_runtime_features`
- explicit release channels for "compile-only", "asm-auditable", and "executable verifier"

### 2. Effect declarations can still overstate safety

Effect under-declaration is now rejected for local direct/same-module/path-dependency calls, which is a major improvement. However, effects are still not a full proof of resource behavior.

Remaining risks:

- registry dependency effect summaries are not signed/consumed as stable facts
- effect classes are coarse and cannot describe all cross-cell invariants
- scheduler metadata is not yet enforced by the real DAG scheduler

Required fix:

- signed effect summaries for packages
- CI/runtime checks that compare metadata to artifact policy
- scheduler admission tests using malicious transactions

### 3. Stateful lowering is still too narrow

The current fixed-scalar and fixed-byte verifier path is useful but limited. Serious protocols need nested schemas, dynamic fields, exact serialization rules, generalized output lock verification, and multi-cell conservation checks.

Required fix:

- generalized typed decoder
- field preservation/change rules
- transition-relation checker
- complete output coverage checks for all supported schema shapes

### 4. Witness and signature binding are not complete enough

Any serious smart contract language on a CKB-style model needs explicit domain separation and replay-resistant witness binding.

Required fix:

- formal signature preimage spec
- witness obligation language
- negative tests for replay, reordered inputs, wrong lock/type domain, wrong group, and stale state

## Lowering Correctness Audit

Current lowering strengths:

- source produces auditable RISC-V assembly
- unsupported runtime paths now fail closed
- metadata separates standalone ELF compatibility, CKB runtime access, symbolic features, fail-closed runtime features, and verifier obligations
- restricted fixed-scalar schema loads use byte-wise little-endian decoding instead of unsafe aligned loads
- create output verification requires full coverage for the supported fixed-scalar subset and now runs real `create` checks at source order so computed scalar locals are available
- fixed-byte output preservation compares `Address`, `Hash`, and fixed `[u8; N]` fields byte-by-byte for constants, schema-backed aliases, stack-backed `[u8; N<=8]` parameters, pointer+length `Address` / `Hash` parameters, trusted schema-parameter TypeHash ABI bytes, and created Output TypeHash fields
- verifier-coverable `with_lock(...)` bindings load output `LockHash` through `LOAD_CELL_BY_FIELD` and compare 32 bytes against constants, consumed/read schema-backed fixed-byte aliases, or 32-byte fixed parameters
- mutable replacement cell TypeHash/LockHash preservation loads Input and Output hash fields through `LOAD_CELL_BY_FIELD`, exact-checks both values as 32 bytes, and byte-compares them before the action body
- mutable replacement preserved-field equality loads Input and Output full cell bytes with `LOAD_CELL`, exact-checks schema size, bounds-checks fixed-width preserved fields, and byte-compares those field bytes
- the mutable transition formula path captures simple scalar `field = field +/- operand` assignments when the operand is verifier-coverable, checking `MintAuthority.minted = old + amount`, AMM parameter-field deltas such as `input.amount`, `token_a.amount`, `token_b.amount`, and `receipt.lp_amount`, and AMM computed-local formulas such as `output`, `lp_amount`, `amount_a`, and `amount_b` against replacement Output bytes
- verifier-coverable fixed-byte equality/inequality now lowers to byte-wise assembly for matching-width constants, schema-backed fields, small stack values, and fixed-byte pointer parameters
- scalar `let` annotations are preserved in IR, allowing source-order create checks to verify narrow lifecycle state fields such as `u8`

Current lowering gaps:

- no machine-checkable proof that IR semantics preserve source semantics
- no formal source-to-IR-to-ASM semantic spec
- no complete source map / trace explaining every verifier branch back to source obligations
- dynamic fixed-byte equality/inequality expressions outside the verifier-coverable source set still fail closed
- generalized claim authorization and witness/time-lock semantics remain outside the verified subset, even though verifier-covered claim/settle output relations are now classified as `checked-runtime`, unsupported claim/settle output relation shapes now expose dedicated blocker classes, explicit 20-byte signer pubkey hash receipt fields can drive `SECP256K1_VERIFY`, restricted lifecycle settle finalization can be checked when final-state and output admission are both covered, and remaining runtime-required claim/finalization obligations list field-aware consumed-input requirements
- broader mutable replacement transition-field formulas remain runtime-required obligations outside the covered add/sub/mul/div/min expression subset, even though TypeHash/LockHash identity preservation, fixed-width preserved-field equality, scalar parameter transitions, parameter-field delta transitions, and controlled AMM computed-local transitions are now checked in assembly; those gaps now have `state-transition-formula-gap` / `state-field-equality-gap` blocker classes in transaction runtime input metadata
- no complete invariant coverage report that says which source-level claims were proved, assumed, or rejected
- no generalized runtime test harness for malformed transaction contexts

Required fix:

- define formal lowering obligations for each source construct
- emit verifier obligation reports next to metadata
- add golden assembly tests for every supported construct
- add adversarial transaction fixtures for every CKB-style syscall path

## DAG Integration Audit

Real today:

- operation-tagged `consume_set`
- operation-tagged `read_refs`
- operation-tagged `create_set`
- operation-tagged `mutate_set` replacement ABI, checked TypeHash/LockHash preservation, fixed-width preserved-field equality checks, scalar parameter/parameter-field/computed-local transition checks, and field summaries for mutable Cell parameters
- `touches_shared`, including `read_ref`, `&mut shared` parameter-derived touches, and composed call return types containing shared values
- effect classes
- per-action scheduler witness bytes with operation/source/index/binding-hash access records in metadata (`scheduler_witness_borsh_hex`), filtered to scheduler-visible Input/CellDep/Output accesses
- `ActionMetadata::scheduler_witness_bytes()` for compiled metadata witness decoding
- CKB runtime access summaries with operation/source/index/binding provenance
- low-level `spora-exec` CellTx placement/discovery/decode/admission helpers, including operation/source compatibility, transaction source-index bounds checks, and exact access-set matching against trusted summaries
- compiled-metadata producer helper `CellTx::push_cellscript_compiled_scheduler_witness(...)`, which appends a concrete transaction witness only after admission and returns the trusted access multiset
- consensus MPE `BlockAccessSummary` consumption of transaction-admitted witnesses, including shared read/write touch domains where read/read remains parallelizable and write/read or write/write serializes the execution DAG
- strict consensus MPE constructor for trusted transaction-builder or compiled-metadata access multisets; missing or mismatched summaries fail before witness data is merged
- mempool validation and template prefilter policy gates for malformed CellScript scheduler metadata, plus strict template policy fixtures for trusted access-summary matching
- producer-backed trusted summary storage and carrying through `MempoolTransaction`, candidate snapshots, mining selectors, and `TemplateTransactionSelector::selected_cellscript_scheduler_accesses()`
- focused mining coverage for a producer-returned summary carried through sidecar insertion into selector exposure
- first malicious-schedule coverage for malformed candidate witness bytes, illegal operation/source pairs, out-of-bounds source indexes, missing scheduler witnesses over structural double-spend conflicts, underreported witnesses over structural CellDep read dependencies, forged extra shared-write touches that only add serialization, admitted read-only shared-touch parallelism, missing trusted summaries, mismatched trusted summaries, malformed mempool metadata, malformed/missing/mismatched template policy metadata, and selector-propagated producer sidecars

Missing:

- canonical access hash/domain derivation
- broader strict-template adversarial/property coverage beyond focused selector-provided builder-summary prefilter tests
- explicit RPC or external submission-surface support if trusted summaries must cross process boundaries

Verdict:

> CellScript emits useful DAG-oriented metadata and has first MPE scheduler-consumption plus mempool/template policy paths, including producer-backed summary storage through mempool/template selection, wallet-generator witness attachment, and focused selector-backed strict template enforcement. DAG integration is not production-complete until broader adversarial/property coverage and any required cross-process trusted-summary submission surface are closed.

## Toolchain Coverage

| Tooling area | Current status | Gap |
|---|---|---|
| `cellc build` | Real local package flow with pre-artifact policy gate | registry/distribution missing |
| `cellc check` | Real compile/check flow plus CLI and manifest production/fail-closed/symbolic/CKB/runtime-obligation policy gates, and target-profile portability classification for `ckb` / `portable-cell` without writing artifacts | broader CI presets missing |
| `cellc metadata` | Real JSON metadata | external CI policy integration missing |
| `cellc doc` | Real API docgen plus lowering audit report / verifier obligations | deeper invariant/spec docs missing |
| `cellc fmt` | Real formatter path | style stability needs more tests |
| `cellc test` | compile-test discovery | no property/fuzz/runtime transaction execution |
| `cellc run` | feature-gated pure ELF runner | no transaction/syscall runner |
| package manager | local path support | registry commands fail-closed |
| Wasm | metadata-only/fail-closed path | no executable backend |
| VS Code / LSP | useful metadata-aware path | not full semantic IDE |

## Milestone Coverage

| Design milestone | Current implementation status | Coverage |
|---|---|---:|
| M1: parsing, type checking, IR | Mostly complete for supported language core | 84-90% |
| M2: CKBVM verifier artifacts | Partial, restricted executable subset | 50-60% |
| M3: shared/receipt/lifecycle | Frontend exists; `read_ref`, mutable shared parameters, and composed shared-return calls are scheduler-visible; mutable shared TypeHash/LockHash replacement preservation, fixed-width preserved-field equality, and parameter-field/computed-local transition checks are executable; mutable shared-state transition obligations are explicit; Pool-specific primitive obligations are separate from general shared mutation; complete fixed-field lifecycle transitions are runtime-classified; semantics remain incomplete | 52-62% |
| M4: scheduler metadata | Metadata and operation-tagged scheduler witness bytes are emitted in compile metadata; read, mutable shared, composed shared-return touches, mutable Cell `mutate_set` replacement ABI/status, and `mutate-input` / `mutate-output` access records are visible; unresolved mutable shared/resource/receipt transition fields are explicit obligations with stable blocker classes for non-covered transition/equality formulas; CLI/docgen now separately report non-Pool runtime-required transaction invariants that already have checked source subconditions; schema v22 filters scheduler witness records to Input/CellDep/Output cell-state sources; compiled metadata exposes scheduler witness bytes; low-level CellTx witness placement/discovery, compiled-metadata producer helper, Borsh envelope admission, operation/source admission, transaction source-index bounds checks, exact trusted access-set matching, wallet-generator final transaction witness attachment, first consensus MPE shared-touch DAG consumption, strict trusted-summary MPE construction, first mempool/template policy gates, producer-backed summary storage through mempool/template selectors, focused producer-summary-to-selector coverage, selector-provided builder-summary strict prefilter coverage, and first adversarial scheduler-consumption/policy tests exist. RPC trusted-summary submission remains Phase 4 hardening because cross-process summaries need an explicit trust/authentication policy | 90-95% |
| M5: launch/pool/claim/settle E2E | Mostly not executable, although the controlled `launch.cell` flow and AMM `seed_pool` / `add_liquidity` / `remove_liquidity` LPReceipt identity paths now verify without fail-closed runtime features; mutable authority/shared replacement TypeHash/LockHash preservation and fixed-width preserved-field equality are executable, MintAuthority has a scalar transition check, AMM Pool general field transitions are checked for the controlled formulas, controlled `seed_pool` token-pair TypeHash inequality is executable, named `destroy` operands now have executable grouped-output TypeHash absence scans, verifier-covered transfer/claim/settle output relations no longer emit expression-level fail-closed paths, covered `transfer-output:<T>` transaction invariants are now `checked-runtime`, uncovered transfer/claim/settle output relations expose `transfer-output-relation-gap` / `claim-output-relation-gap` / `settle-output-relation-gap`, restricted direct, additive-merge, and matched amount-split resource conservation is classified as `checked-runtime` with checked `resource-conservation-proof` components, explicit claim witness envelope and authorization-domain checks are executable, claim ECDSA signature verification is executable for receipts with the explicit fixed 20-byte signer pubkey hash field convention, restricted lifecycle-backed `settle-finalization:<T>` is checked when fixed-scalar final-state and output admission are both covered, and pool-pattern debt is explicit as runtime-required obligations plus structured metadata; broader pool-pattern economics/admission/equality, generalized transfer/resource conservation, generalized claim authorization policy, and generalized settle finalization execution remain incomplete | 64-73% |

## Prioritized Gap List

### Tier 0: Fatal Semantic Blockers

| Gap | Why it matters | Failure mode | Blocks serious protocol use | Fix type |
|---|---|---|---|---|
| Full resource conservation verifier | Asset protocols need proof that inputs/outputs conserve, mint, or burn only under valid rules | inflation, unauthorized burn, hidden state transition bugs | Yes | semantic redesign + compiler checks + runtime tests |
| Executable `transfer` / `claim` / `settle` plus complete `destroy` policy | These are core language promises | incomplete state transitions, partial burn/conservation proof, or fail-closed paths for unsupported operations | Yes | lowering + verifier semantics |
| Witness/signature/domain binding | Authorization without replay resistance is unsafe | replay, wrong-domain signature acceptance, witness confusion | Yes | language spec + stdlib + negative tests |
| Complete typed cell decoding | Source fields must correspond exactly to loaded bytes | serialization ambiguity, wrong field reads, partial verification | Yes | decoder + layout spec + tests |
| DAG scheduler enforcement | Metadata without runtime enforcement is advisory | unsafe parallel execution or missed conflicts | Yes for DAG claims | runtime integration + adversarial tests |

### Tier 1: Major Completeness Gaps

| Gap | Why it matters | Failure mode | Blocks serious protocol use | Fix type |
|---|---|---|---|---|
| Full lifecycle transition verifier | lifecycle syntax must prove legal state movement, not only valid declarations/create literals | invalid state phase accepted | Often | compiler lowering + runtime checks |
| Schema evolution/versioning | real protocols migrate | stuck assets or unsafe migrations | Often | type system + spec |
| Cross-package signed effect summaries | package imports must preserve safety metadata | imported code hides effects | Often | package metadata + verification |
| Cross-cell/grouped invariants | real protocols are multi-cell | local checks pass while global invariant breaks | Often | invariant language + verifier |
| Transaction-level invariant language | important constraints are not per-cell | missing total balance/order/state constraints | Often | semantic extension |

### Tier 2: Important But Survivable Weaknesses

| Gap | Why it matters | Failure mode | Blocks serious protocol use | Fix type |
|---|---|---|---|---|
| Property/fuzz/adversarial transaction testing | protocols need hostile transaction tests | regressions escape compile tests | Not always, but high risk | tooling |
| Source maps and verifier obligation reports | auditors need traceability | source looks safe but ASM proof is unclear | Not always | compiler metadata |
| IDE semantic indexing | larger codebases need reliable navigation | wrong edits, hidden call graph mistakes | No, but slows adoption | LSP |
| Registry lifecycle | ecosystem distribution matters | dependency workflow remains manual | No for local dev | package manager |
| Formal lowering spec | compiler changes need a contract | semantic drift | No for early restricted usage, yes for production | spec |

### Tier 3: Polish / Ecosystem Issues

| Gap | Why it matters | Failure mode | Blocks serious protocol use | Fix type |
|---|---|---|---|---|
| Deeper docgen invariant sections | audits need generated obligation docs plus source-level invariants | docs miss invariant changes | No | docgen |
| Formatter stability policy | diffs should be reviewable | noisy diffs | No | fmt tests |
| Wasm decision | avoid unsupported target confusion | users assume Wasm is executable | No if documented | spec/product |
| Examples beyond toy flows | examples set user expectations | unsafe patterns get copied | No, but important | examples/tests |

## Go / No-Go Judgment

Go for:

- compiler core development
- pure compute examples
- restricted schema/field verifier experiments
- metadata and DAG access summary iteration
- local package/tooling workflows

No-Go for:

- production stateful protocol contracts
- public claims of Move/Solidity-grade completeness
- public claims of full CKB contract compatibility
- public claims that DAG scheduling is enforced end-to-end
- public claims that `transfer`/`claim`/`settle`/`launch` are complete executable semantics, or that pool flows are language primitives rather than shared-state protocol patterns

The accurate status is:

> CellScript has the bones of a serious protocol language, but is still a partially executable compiler/toolchain with incomplete stateful protocol semantics. It is on a plausible path, but it is not yet a complete smart contract language.
