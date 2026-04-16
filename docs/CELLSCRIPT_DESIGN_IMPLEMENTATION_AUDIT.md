# CellScript Design Proposal Implementation Audit

**Snapshot date**: 2026-04-16  
**Scope**: `docs/SPORA_DSL_DESIGN_PROPOSAL_CN.md` compared with the current `cellscript/` implementation  
**Purpose**: track design-proposal coverage against code reality, not roadmap intent.

This document should be read together with:

- [CELLSCRIPT_IMPLEMENTATION_STATUS.md](./CELLSCRIPT_IMPLEMENTATION_STATUS.md)
- [CELLSCRIPT_COMPATIBILITY_MATRIX.md](./CELLSCRIPT_COMPATIBILITY_MATRIX.md)
- [SPORA_DSL_DESIGN_PROPOSAL_CN.md](./SPORA_DSL_DESIGN_PROPOSAL_CN.md)

## Executive Verdict

CellScript is no longer only a parser or syntax demo. It has a real compiler path, real metadata, partial CKB-style runtime lowering, a local CLI workflow, and increasingly strict fail-closed behavior.

It is still not a complete implementation of the design proposal.

Approximate implementation status:

| Area | Current coverage | Verdict |
|---|---:|---|
| Lexer / parser / AST | 88-92% | Stable main path for supported syntax |
| Type checking / linear checks | 65-72% | Stronger value/resource checks, still not full semantic proof |
| IR and metadata | 75-85% | Real, includes verifier obligations, still not full protocol semantics |
| Pure compute lowering | 78-88% | Usable subset with stricter return/value semantics |
| CKB-style runtime lowering | 35-45% | Partial and intentionally fail-closed where semantics are incomplete |
| Stateful protocol primitives | 18-28% | Mostly not executable |
| DAG scheduler integration | 20-30% | Metadata exists, runtime consumption missing |
| CLI local workflow | 64-74% | Usable local developer loop with pre-artifact CLI and manifest metadata policy gates |
| IDE/LSP/tooling ecosystem | 35-50% | Useful metadata surfaces, not full semantic IDE |
| Overall design proposal completion | 48-55% | Real implementation progress, not production-complete |

The critical distinction is:

> The compiler core is real. The full protocol language promised by the design proposal is not complete yet.

## Corrected Status Compared With Earlier Audits

Several older audit claims are now stale:

| Older claim | Current reality |
|---|---|
| Effects are mostly decorative metadata | Incorrect now. Explicit `#[effect(...)]` under-declarations are rejected for direct operations, same-module calls, and local `path` dependency imports. |
| `fn` can hide stateful behavior | Mostly fixed for local code. `fn` definitions are distinct AST/IR/metadata entries, must infer `Pure`, cannot call `action` or `lock`, and reject direct, same-module indirect, and local imported stateful behavior. |
| Create output checks are only comments | Incorrect for the supported subset. Fixed-scalar output fields can be checked with exact-size, bounds, and equality checks. Unsupported create paths fail closed. |
| Symbolic runtime paths may silently continue | Improved. Unsupported runtime features emit explicit fail-closed assembly and metadata. |
| Scheduler metadata has no fail-closed signal | Improved. Metadata exposes `fail_closed_runtime_features` separately from `symbolic_runtime_features`. |
| No-return helpers behave like `u64` values | Fixed for the local compiler. Helpers without a return type use internal `Unit`, lower to destinationless calls, and cannot be bound or returned as values. |
| `assert_invariant` behaves like a boolean value | Fixed for the local compiler. Assertions are `Unit`, lower to fail-closed verifier CFG, cannot be bound or used as value-returning tail expressions, and require static string literal messages. |
| Source after `return` can be silently ignored by lowering | Fixed for guaranteed-return source paths. The type checker rejects unreachable statements after `return` or complete branch returns. |
| Unknown call return types become implicit `u64` | Fixed in IR lowering. Unresolved call return types are rejected instead of fabricating a numeric result. |
| Tail expressions are only source sugar | Improved. Typed tail expressions and terminal `if` tails lower to real `Return(Some(...))` terminators for value-returning `action` / `fn` bodies. |
| Empty arrays silently become `[u64; 0]` | Fixed. Empty arrays require explicit zero-length array annotations and preserve the declared element type. |
| Local `Vec` values have no item-type enforcement | Improved. `Vec.push` propagates the first concrete item type from `Vec::new()` and rejects incompatible later pushes. |

The main negative findings remain valid:

- `transfer`, `destroy`, `claim`, and `settle` are still not executable protocol semantics.
- `launch`, `mint`, `burn`, `seed_pool`, `swap`, `wrap`, and `unwrap` are not complete standard primitives.
- generalized resource conservation is not implemented.
- lifecycle rules now have main-path and LSP declaration/static create-state checks plus explicit state/transition metadata, but full transition verification remains incomplete.
- full DAG scheduling enforcement is not connected to runtime admission/conflict checks.

## Design Section Coverage

### 1. Core Semantic Model

| Design concept | Current code status | Coverage |
|---|---|---:|
| `resource` | Parsed, typed, lowered into IR, participates in partial linear checks and metadata | 70% |
| `shared` | Parsed and represented, can appear in metadata, but complete contention/runtime semantics are missing | 45% |
| `receipt` | Parsed and represented; lifecycle declarations are statically checked; `receipt Name -> Output` claim outputs are type-checked and represented in IR/metadata, but executable claim condition/output verification is still fail-closed | 45% |
| `launch` | Reserved/syntactic direction only; no compiler-known executable creation semantics | 10% |
| `pool` | Not a first-class semantic primitive; only approximated through shared state ideas | 5% |
| `settle` | Recognized but fail-closed in codegen | 10% |
| `ephemeral` | Frontend support exists, but full transaction-scoped semantics are incomplete | 40% |
| persistent state | Implicit through cell model assumptions; no complete first-class state-tree semantics in CellScript | 25% |

Verdict: the vocabulary exists for several design concepts, but the executable protocol semantics are still partial. The language can describe intended resource/state operations more clearly than raw CKB scripts, but for many operations it still cannot prove or execute the intended transition.

### 2. Type System

| Feature | Current code status | Coverage |
|---|---|---:|
| Primitive integers / bool / hash-like values | Real parser/type/codegen support for core scalar paths | 80-90% |
| Fixed arrays | Supported in frontend/type checking; empty arrays require explicit zero-length annotations; local static index/foreach/len lowering exists for supported cases | 70-78% |
| Struct/resource/shared/receipt shapes | Real AST/IR/type presence | 75% |
| Linear usage checks | Present and useful, but not a full resource proof system | 65-72% |
| Capabilities such as store/transfer/destroy | Parser/type checker now merge attribute and inline declarations, reject `transfer` without `transfer`, reject `destroy` without `destroy`, restrict `claim` to receipts, require declared receipt claim outputs to be resource/shared cells, and restrict `settle` to cell-backed linear values; full conservation/runtime proof is still incomplete | 60-67% |
| Immutable vs mutable fields | Not fully enforced as first-class transition constraints | 20% |
| Schema evolution/versioning | Not implemented as a complete language feature | 10% |
| Lifecycle declaration/runtime checks | Main-path checks reject duplicate states, invalid state field types, missing create `state`, static out-of-range create states, non-initial static creates, and static reset-to-initial updates; complete fixed-scalar verifier paths emit state-range and `old_state + 1 == new_state` prelude checks | 45-55% |
| Generics | Not implemented as a real monomorphized type system | 5% |
| Result/Option/error propagation | Not implemented as designed | 5% |

Verdict: the type system now rejects several previous false-value edges, but still cannot claim Move-grade resource safety or Solidity-grade practical completeness.

### 3. Syntax

| Syntax area | Current status | Coverage |
|---|---|---:|
| Module/type/action/lock syntax | Real | 85-95% |
| `consume` / `create` syntax | Real, with partial executable checks | 65-75% |
| `read_ref` | Real for restricted fixed-scalar CKB CellDep field access | 70-80% |
| `transfer` | Parsed/lowered as symbolic, codegen fail-closed | 20% |
| `destroy` | Parsed/lowered as symbolic, codegen fail-closed | 20% |
| `claim` | Parsed/lowered as symbolic, can type/lower declared `receipt -> output` cells into operation-tagged `create_set`, codegen still fail-closed for actual claim semantics | 20-25% |
| `settle` | Parsed/lowered as symbolic, codegen fail-closed | 10% |
| `launch` | Mostly reserved/design-level | 10% |
| `assert_invariant` | Lowers to fail-closed CFG, is typed as value-less `Unit`, and requires static string literal messages; full invariant proof/lowering story remains incomplete | 58-68% |
| Tail expressions / value returns | All value-returning `action` / `fn` paths must return; typed tail expressions and terminal `if` branches lower to real return terminators | 70-80% |
| `?` / Result propagation | Not implemented | 0-5% |

Verdict: syntax is significantly ahead of executable semantics. This is acceptable only if every unsupported path remains fail-closed and clearly surfaced in metadata, which is now mostly true.

### 4. Compiler Pipeline

| Pipeline stage | Current status | Coverage |
|---|---|---:|
| Lexer | Stable main path | 90% |
| Parser | Stable main path for supported syntax | 88-92% |
| AST | Stable main path for supported syntax | 88-92% |
| Name/module resolution | Local path dependencies work; remote/registry story incomplete | 60-70% |
| Type checking | Useful and stricter on returns, unreachable statements, assertions, empty arrays, `Unit`, and local `Vec` item propagation; still not full semantic proof | 67-74% |
| IR lowering | Real, with action/lock/function/effect metadata, destinationless no-return calls, Unit-valued assertions, typed empty arrays, and tail-return terminators | 76-86% |
| Optimization | Not part of the trusted path | 10-15% |
| RISC-V assembly codegen | Real for pure and restricted runtime paths | 60-70% |
| RISC-V ELF output | Real for pure/restricted executable paths | 50-60% |
| Wasm | Metadata-only/fail-closed path, not executable backend | 10-15% |

Verdict: the main compiler pipeline is real. The trusted path should still be described as restricted, not production-complete.

### 5. Runtime / Execution Model

| Runtime requirement | Current status | Coverage |
|---|---|---:|
| Pure CKB-VM-compatible ELF | Real for no-argument pure programs | 70% |
| Parameter ABI | Real pointer+length ABI for fixed schema parameter access | 60-70% |
| `LOAD_CELL Source::Input` | Used for restricted consumed-input field access | 45-55% |
| `LOAD_CELL Source::CellDep` | Used for restricted `read_ref<T>().field` access | 55-65% |
| `LOAD_CELL Source::Output` | Used for restricted `create` output verification | 40-50% |
| Full `consume` expression semantics | Not complete; ELF remains fail-closed | 20% |
| Full `create` resource-handle semantics | Not complete; only restricted verifier prelude exists | 25-35% |
| Full lock/type script semantics | Partial entrypoint handling; witness/signature semantics incomplete | 25-35% |
| Lifecycle transition verification | Declaration, static create-state, static reset, state/transition metadata exposure, LSP diagnostics, and complete fixed-scalar prelude checks exist; dynamic/nested/locked output transition legality is not fully verified | 40-50% |
| Full transaction invariant checks | Not complete | 20% |

Verdict: CKB-style runtime integration is no longer imaginary, but only a narrow subset has concrete verifier lowering.

### 6. Standard Primitives

| Primitive | Current status | Coverage |
|---|---|---:|
| `launch` | Not implemented as executable compiler-known primitive | 5-10% |
| `mint` | Not implemented as standard primitive | 5% |
| `burn` / `destroy` | `destroy` recognized but fail-closed | 15-20% |
| `transfer` | recognized but fail-closed | 15-20% |
| `seed_pool` | Not implemented | 0-5% |
| `swap` | Not implemented | 0-5% |
| `wrap` / `unwrap` | Not implemented | 0-5% |
| `claim` | recognized, declared output type is now visible to type checker/IR/metadata, executable verifier semantics remain fail-closed | 15-20% |
| `settle` | recognized but fail-closed | 10% |

Verdict: this is the largest gap between the design proposal and implementation. The standard primitive layer is still mostly a design target.

## Protocol Use-Case Coverage

| Use case | Current classification | Reason |
|---|---|---|
| Lock-style authorization | Expressible but under-specified | `lock` exists, bool return is enforced, but signature/witness/domain binding is incomplete. |
| Type-script state transition validation | Expressible only in restricted cases | Simple fixed-scalar input/output checks exist; generalized transitions are missing. |
| Fungible assets / UDT invariants | Ambiguous / under-specified | Can model shapes, but generalized conservation/issuance/burning checks are incomplete. |
| NFT / singleton objects | Expressible but awkward | Resource syntax helps, but identity/type-id/versioning semantics are incomplete. |
| Vault / CDP / lending machines | Not properly expressible safely | Requires cross-cell invariants, prices, liquidation rules, witness proofs, and partial updates beyond current lowering. |
| DAO / governance transitions | Expressible only with unsafe off-chain burden | Multicell invariants and voting state transitions are not first-class enough yet. |
| Order matching / settlement | Not properly expressible safely | `settle` is fail-closed and intent/witness binding is not complete. |
| Multi-party signing / delegated authority | Under-specified | Needs first-class witness/signature domains and replay resistance. |
| Upgrade / migration | Under-specified | No complete schema evolution/versioning model. |
| Capability boundaries | Partial | Capabilities exist syntactically, but enforcement is incomplete. |
| Resource conservation | Partial restricted subset | Simple fixed-scalar output equality and u64 arithmetic checks exist; generalized conservation is missing. |
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

The current fixed-scalar schema verifier path is useful but limited. Serious protocols need nested schemas, dynamic fields, exact serialization rules, and multi-cell conservation checks.

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
- create output verification requires full coverage for the supported fixed-scalar subset

Current lowering gaps:

- no machine-checkable proof that IR semantics preserve source semantics
- no formal source-to-IR-to-ASM semantic spec
- no complete source map / trace explaining every verifier branch back to source obligations
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
- `touches_shared`
- effect classes
- scheduler witness bytes with operation/source/index/binding-hash access records
- CKB runtime access summaries with operation/source/index/binding provenance

Missing:

- real DAG scheduler consumption
- conflict/admission checks
- malicious schedule tests
- canonical access hash/domain derivation
- metadata/artifact consistency enforcement

Verdict:

> CellScript emits useful DAG-oriented metadata, but the DAG integration is not complete until the runtime scheduler consumes and enforces it.

## Toolchain Coverage

| Tooling area | Current status | Gap |
|---|---|---|
| `cellc build` | Real local package flow with pre-artifact policy gate | registry/distribution missing |
| `cellc check` | Real compile/check flow plus CLI and manifest production/fail-closed/symbolic/CKB/runtime-obligation policy gates | broader CI presets missing |
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
| M3: shared/receipt/lifecycle | Frontend exists; semantics incomplete | 25-35% |
| M4: scheduler metadata | Metadata and operation-tagged scheduler witness emitted; scheduler enforcement missing | 35-45% |
| M5: launch/pool/claim/settle E2E | Mostly not executable | 5-15% |

## Prioritized Gap List

### Tier 0: Fatal Semantic Blockers

| Gap | Why it matters | Failure mode | Blocks serious protocol use | Fix type |
|---|---|---|---|---|
| Full resource conservation verifier | Asset protocols need proof that inputs/outputs conserve, mint, or burn only under valid rules | inflation, unauthorized burn, hidden state transition bugs | Yes | semantic redesign + compiler checks + runtime tests |
| Executable `transfer` / `destroy` / `claim` / `settle` | These are core language promises | programs compile only to fail-closed paths | Yes | lowering + verifier semantics |
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
- public claims that `transfer`/`claim`/`settle`/`launch` are complete language primitives

The accurate status is:

> CellScript has the bones of a serious protocol language, but is still a partially executable compiler/toolchain with incomplete stateful protocol semantics. It is on a plausible path, but it is not yet a complete smart contract language.
