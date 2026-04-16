# CellScript Implementation Status

**Snapshot date**: 2026-04-16  
**Scope**: current code in `/Users/arthur/RustroverProjects/Spora/cellscript/`  
**Purpose**: this document tracks **implementation reality**, not design intent.

## Reading guide

Use this document when you need to answer:

- what parts of `CellScript` are stable enough to rely on now
- what parts are production-grade, production-hardening, or still partial
- what modules exist in the tree but are not part of the trusted main path

For design-proposal coverage specifically, see
[CELLSCRIPT_DESIGN_IMPLEMENTATION_AUDIT.md](./CELLSCRIPT_DESIGN_IMPLEMENTATION_AUDIT.md).

Do **not** use `SPORA_DSL_DESIGN_PROPOSAL_CN.md` as the source of truth for implementation completeness. That document is primarily a design/proposal document.

## Current headline

`CellScript` is currently best described as:

> **a working compiler/toolchain under production hardening, with fail-closed boundaries for incomplete semantics**

The trustworthy path today is:

1. Resolve input from `.cell`, package directory, or `Cell.toml`
2. `lex -> parse -> type check -> minimal IR lowering -> codegen`
3. Emit `riscv64-asm` or `riscv64-elf`
4. Write artifact to disk via `cellc`

## Trusted main path

These parts are real and currently wired into the main compiler entry:

| Area | Status | Notes |
|---|---|---|
| Lexer | Stable main path | Main path in use |
| Parser | Stable main path | Main path in use; array literals now parse as first-class expressions instead of AST-only dead syntax |
| AST | Stable main path | Main path in use |
| Input resolution | Stable main path | Supports single file, package dir, `Cell.toml` |
| Local package loading | Stable main path | Supports local `path` dependencies and `source_roots` |
| Type checking | Production-hardening | Real checks exist; array literals now require homogeneous element types, empty arrays require explicit zero-length array annotations, aggregate element writes require mutable roots, `Vec.push` propagates item types, and value-returning action/fn bodies must have explicit return paths or typed tail expressions on all paths |
| Linear checks | Production-hardening | Real checks exist, but not complete semantic coverage |
| IR lowering | Production-hardening | Pure-compute path lowers statements, locals, returns, tail-expression returns, terminal-if tail returns, parameter bindings, no-return helper calls as destinationless calls, typed empty fixed arrays, local fixed-array static index reads/writes, local fixed-array foreach unrolling, local fixed-array `len()` folding, local tuple static field reads/writes/destructuring, and array-of-tuples static index/foreach destructuring projections |
| RISC-V assembly emission | Stable main path | Main output path |
| RISC-V ELF emission | Usable subset | Main output path for pure computation, restricted fixed-width scalar schema-parameter field loads, and restricted `read_ref` field loads, with external toolchain support and built-in fallback |
| Schema layout metadata | Production-hardening | Type/field offset and fixed encoded-size metadata is emitted for audit/tooling |
| Consume input field verification | Partial | Consumed Input cell bytes are loaded with CKB `LOAD_CELL`; fixed-width scalar fields (`bool/u8/u16/u32/u64`) are read with unaligned-safe byte loads and exact-size/bounds checked when backed by loaded cell bytes, but resource conservation semantics are not complete |
| Create output verification | Partial | Simple fixed-width scalar output schemas are checked in assembly with exact-size checks and per-field equality against constants, parameters, or consumed/read schema field aliases; `u64` fields also support local-const expected values and left-associative `+/-` chains over prelude-available operands; simple move/alias propagation is supported; incomplete create verifiers now fail closed instead of continuing with comments; full resource creation semantics are not complete |
| Symbolic runtime lowering | Safer partial | Unsupported stateful/runtime operations now emit explicit fail-closed return paths instead of silent success values |
| Fail-closed metadata | Production-hardening | Runtime/action/lock metadata now expose `fail_closed_runtime_features` separately from broader symbolic and CKB runtime feature lists |
| Verifier obligation metadata | Production-hardening | Runtime/action/fn/lock metadata now emit explicit obligations for CKB runtime access, standalone-ELF limitations, fail-closed runtime paths, and partial lifecycle transition checks |
| VM object ABI metadata | Usable partial | Compile metadata declares Molecule VM object ABI `0x8001` for CKB-style full-object load syscalls; RISC-V ELF artifacts embed a fixed ABI trailer that exec strips before CKB-VM loading; assembly/non-ELF artifacts still need sidecar/verifier policy |
| Artifact/metadata self-validation | Production hardening | `CompileResult::validate()` checks metadata schema version, compiler version, artifact hash, metadata hash/size binding, source-unit hash binding, artifact format, ELF magic, VM ABI trailer presence/version, and standalone-runtime metadata consistency before compile results are returned |
| Effect enforcement | Safer partial | Action effects are inferred from read/create/consume/destroy/transfer/claim/settle operations, same-module function calls, and local `path` dependency imports; explicit under-declarations are rejected |
| `fn` purity boundary | Safer partial | Helper `fn` definitions are distinct AST/IR/metadata entries, must infer `Pure`, and cannot call `action` or `lock`; direct, same-module indirect, and local imported Cell/runtime operations are rejected instead of being lowered as hidden actions; no-return helpers use an internal `Unit` type and cannot be bound or returned as values |
| Lifecycle declaration/runtime checks | Safer partial | `#[lifecycle(...)]` receipts are now checked on the main compile path and in LSP diagnostics for declaration/create/reset rules; lifecycle states and adjacent transition edges are emitted in type metadata; complete fixed-scalar consume-to-create verifier paths emit state-range and `old_state + 1 == new_state` prelude checks |
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
| CLI local workflow | Partial-to-good | `build`, `check`, `doc`, `fmt`, `metadata`, `verify-artifact`, and compiler-test discovery are real; `cellc test` supports positive compile tests, strict `expect-success` / `expect-fail` / `expect-error` diagnostics, per-file target selection, per-file production/symbolic/CKB runtime policy gates, runtime metadata assertions, action/function/lock metadata classification assertions, and `--json` CI summaries; unknown or conflicting test directives fail instead of being ignored; `fmt --json` emits clean/dirty changed-file summaries; `build` and `check` can enforce command-line or manifest `[policy]` production/symbolic/CKB runtime metadata policies before artifacts are accepted; `build --json` emits a machine-readable artifact/metadata/hash summary after writing outputs; `check --json` emits a machine-readable checked-target/policy summary; `verify-artifact` checks emitted artifact/metadata binding, optional source-unit hashes, expected artifact/source hash pins, command-line production/symbolic/CKB runtime policy gates, and optional JSON summaries for CI; `doc` includes lowering audit reports and verifier obligations; registry/runtime testing remains incomplete |

## Present in tree, but not trusted as complete

These modules exist, but should be considered **limited, unsupported for production semantics, or not on the trusted compiler path**:

| Module area | Status |
|---|---|
| `src/cli/` subcommand framework | Partial-to-good local workflow; registry commands fail-closed |
| `src/optimize/` | Limited optimizer research path; not on the trusted release path |
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

- `cellc build [--json] [--production] [--deny-fail-closed] [--deny-symbolic-runtime] [--deny-ckb-runtime]`
- `cellc check [--all-targets] [--json] [--production] [--deny-fail-closed] [--deny-symbolic-runtime] [--deny-ckb-runtime]`
- `cellc doc --format markdown|html|json [--json]`
- `cellc fmt [--check] [--json]`
- `cellc init [NAME] [PATH] [--lib] [--json]`
- `cellc add CRATE... [--dev] [--build] [--git URL] [--path PATH] [--json]`
- `cellc remove CRATE... [--dev] [--build] [--json]`
- `cellc clean [--json]`
- `cellc info [--json]`
- `cellc metadata [INPUT]`
- `cellc verify-artifact ARTIFACT [--metadata FILE] [--verify-sources] [--json] [--expect-artifact-hash HASH] [--expect-source-hash HASH] [--expect-source-content-hash HASH] [--production] [--deny-fail-closed] [--deny-symbolic-runtime] [--deny-ckb-runtime]`
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

## Semantic completeness

The compiler is no longer just “shape-complete”. Some real semantics are now enforced end-to-end:

- function parameters get IR bindings
- parameters are spilled to stack slots in codegen
- local bindings participate in computation
- `return <expr>` now lowers into actual return-value generation
- no-return `fn` calls lower as destinationless IR calls; their internal `Unit` result cannot be bound to locals or returned from value-returning entrypoints
- unresolved call return types no longer default to `u64`; IR lowering rejects unknown calls instead of fabricating a value type
- empty array literals no longer silently default to `[u64; 0]`; they require an explicit zero-length array type annotation and preserve that element type through IR lowering
- value-returning `action` and `fn` bodies now require all paths to return through explicit `return`, typed tail expressions, or terminal `if` branches with typed tail expressions; codegen lowers those tail forms into real `Return(Some(...))` terminators
- `Vec.push` now propagates element types from an initially untyped `Vec::new()` and rejects pushes whose item type disagrees with an already typed `Vec<T>`
- minimal pure-compute functions such as `add(x, y)` now compile into meaningful arithmetic assembly
- named schema parameters expose fixed field layout metadata
- `param.scalar_field` on a named action/lock schema parameter lowers to unaligned-safe little-endian byte loads for fixed `bool/u8/u16/u32/u64` fields and can emit ELF; the generated ABI now passes schema values as `aN=borsh_ptr, aN+1=borsh_len`, so fixed schema field access performs exact-size and bounds checks
- `consume token` preloads the consumed Input cell bytes in assembly, retains the verifier pointer, and lets `token.scalar_field` lower through loaded-byte exact-size/bounds checks and byte-wise loads; ELF still rejects `consume-expression`
- `read_ref<T>().scalar_field` lowers through `LOAD_CELL Source::CellDep`, loaded-byte exact-size/bounds checks, and byte-wise loads; it can emit CKB-runtime ELF but requires transaction/syscall context
- simple fixed-scalar `create Type { ... }` outputs are verified with `LOAD_CELL Source::Output`, exact-size checks, per-field bounds checks, and equality checks against constants, parameters, or consumed/read schema field aliases; all fixed scalar fields must be covered before the verifier claims completeness
- incomplete `create` output verification paths fail closed in generated assembly instead of silently continuing after an audit comment
- symbolic runtime fallback paths for `transfer`, `destroy`, `claim`, `settle`, dynamic collections, `type_hash`, non-lowered field/index access, dynamic `len`, and non-preloaded `read_ref` now fail closed in generated assembly rather than pretending to produce executable verifier semantics
- metadata now exposes fail-closed runtime features and verifier obligations explicitly, so CI/IDE/audit tools do not need to infer those paths from assembly comments
- metadata now declares `runtime.vm_abi = { format: "molecule", version: 0x8001 }` for VM-facing full-object syscall bytes; RISC-V ELF artifacts also embed a fixed ABI trailer so the exec verifier can select Molecule and strip the trailer before loading the ELF
- compile-result metadata now includes schema version, compiler version, BLAKE3 artifact hash, byte size, path-bound source set hash, path-independent source content hash, and source unit hashes; the compiler validates metadata/artifact/source/trailer consistency before returning or writing artifacts
- `cellc verify-artifact` can validate already-emitted artifacts and metadata sidecars in CI without recompiling; `--verify-sources` also checks metadata `source_units[]` against files on disk; `--expect-*hash` pins expected artifact/source/source-content BLAKE3 values; `--production` / `--deny-*` apply the same metadata policy gates to stored artifacts; `--json` emits a machine-readable verification summary
- `cellc build` and `cellc check` can enforce metadata policy gates from CLI flags or `Cell.toml [policy]`: `production` rejects fail-closed lowering, `deny_symbolic_runtime` rejects non-standalone Cell/runtime requirements, and `deny_ckb_runtime` rejects transaction/syscall runtime requirements; `check --all-targets` validates both asm and ELF lowering without writing artifacts; `build` applies the gate before writing artifacts
- `cellc doc` includes a lowering audit report and verifier obligation table in generated Markdown/HTML/JSON docs; `--json` emits a machine-readable doc output summary
- explicit `#[effect(...)]` annotations are checked against inferred action behavior, including same-module calls and local `path` dependency imports; under-declared scheduler/effect metadata is now a compiler error
- `fn` definitions are enforced as pure helpers with distinct `functions[]` metadata; `fn` can call only `fn`, while `action` and `lock` may call `fn`; helpers without a return type are typed as internal `Unit`, emit no call destination, and are legal only as statement calls
- `#[lifecycle(...)]` receipt declarations now participate in the main compile path and LSP diagnostics: duplicate lifecycle states are rejected, lifecycle `state` fields must be unsigned integers, create expressions for lifecycle receipts with a `state` field must set it, static integer state values are range-checked, initial creates must use state `0`, consumed same-type updates cannot statically reset to state `0`, lifecycle states and adjacent transition edges are exposed in type metadata, and complete fixed-scalar output verifier paths emit `old_state < state_count`, `new_state < state_count`, and `old_state + 1 == new_state` checks against loaded Input/Output bytes
- simple `consume input.u64_field -> create output.u64_field` aliases are verified by comparing loaded Input and Output fields in the prelude
- simple `consume input.u64_field +/- const_or_param_or_local_const +/- ... -> create output.u64_field` expressions are verified in the prelude for left-associative `u64` add/sub chains
- simple `LoadConst` and `Move` sources propagate into prelude-verifiable `u64` expressions, so local constants and aliases do not silently erase verification

Still not safe to call semantically complete:

- full control-flow lowering
- resource lifecycle semantics end-to-end; current lifecycle checks cover static declaration/create/reset guards plus complete fixed-scalar output verifier transition checks, not all loaded-byte transition cases
- generalized schema decoding from `consume` / `read_ref` / `create` loaded cell bytes
- generalized resource conservation and transition relation checks across consumed inputs and created outputs
- right-nested/arbitrary arithmetic state expressions in output verification
- full create expression/resource-handle executable semantics
- complete cross-module call semantics
- comprehensive pattern lowering
- complete mutation semantics for all language constructs

## Validation snapshot

At the time of this snapshot, the `cellscript` crate passed:

- `176` library tests
- `38` CLI tests
- `1` examples test group

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
