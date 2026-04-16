# CellScript Implementation Status

**Snapshot date**: 2026-04-16  
**Scope**: current code in `/Users/arthur/RustroverProjects/Spora/cellscript/`  
**Purpose**: this document tracks **implementation reality**, not design intent.

## Reading guide

Use this document when you need to answer:

- what parts of `CellScript` are stable enough to rely on now
- what parts are only MVP-grade
- what modules exist in the tree but are not part of the trusted main path

Do **not** use `SPORA_DSL_DESIGN_PROPOSAL_CN.md` as the source of truth for implementation completeness. That document is primarily a design/proposal document.

## Current headline

`CellScript` is currently best described as:

> **an MVP compiler with a working core compile path**

The trustworthy path today is:

1. Resolve input from `.cell`, package directory, or `Cell.toml`
2. `lex -> parse -> type check -> minimal IR lowering -> codegen`
3. Emit `riscv64-asm` or `riscv64-elf`
4. Write artifact to disk via `cellc`

## Trusted main path

These parts are real and currently wired into the main compiler entry:

| Area | Status | Notes |
|---|---|---|
| Lexer | Stable MVP | Main path in use |
| Parser | Stable MVP | Main path in use |
| AST | Stable MVP | Main path in use |
| Input resolution | Stable MVP | Supports single file, package dir, `Cell.toml` |
| Local package loading | Stable MVP | Supports local `path` dependencies and `source_roots` |
| Type checking | Usable MVP | Real checks exist, but still partly heuristic |
| Linear checks | Usable MVP | Real checks exist, but not complete semantic coverage |
| Minimal IR lowering | Usable MVP | Pure-compute path now lowers statements, locals, returns, parameter bindings |
| RISC-V assembly emission | Usable MVP | Main output path |
| RISC-V ELF emission | Usable subset | Main output path for pure computation, restricted fixed-width scalar schema-parameter field loads, and restricted `read_ref` field loads, with external toolchain support and built-in fallback |
| Schema layout metadata | Usable MVP | Type/field offset and fixed encoded-size metadata is emitted for audit/tooling |
| Consume input field verification | Partial | Consumed Input cell bytes are loaded with CKB `LOAD_CELL`; fixed-width scalar fields (`bool/u8/u16/u32/u64`) are read with unaligned-safe byte loads and exact-size/bounds checked when backed by loaded cell bytes, but resource conservation semantics are not complete |
| Create output verification | Partial | Simple fixed-width scalar output schemas are checked in assembly with exact-size checks and per-field equality against constants, parameters, or consumed/read schema field aliases; `u64` fields also support local-const expected values and left-associative `+/-` chains over prelude-available operands; simple move/alias propagation is supported; incomplete create verifiers now fail closed instead of continuing with comments; full resource creation semantics are not complete |
| Symbolic runtime lowering | Safer partial | Unsupported stateful/runtime operations now emit explicit fail-closed return paths instead of placeholder success values |
| Fail-closed metadata | Usable MVP | Runtime/action/lock metadata now expose `fail_closed_runtime_features` separately from broader symbolic and CKB runtime feature lists |
| Effect enforcement | Safer partial | Action effects are inferred from read/create/consume/destroy/transfer/claim/settle operations, same-module function calls, and local `path` dependency imports; explicit under-declarations are rejected |
| `fn` purity boundary | Safer partial | Helper `fn` definitions must infer `Pure`; direct, same-module indirect, and local imported Cell/runtime operations are rejected instead of being lowered as hidden actions |
| Main CLI compiler | Usable MVP | `cellscript/src/main.rs` is the real entry point |
| Examples / compiler regression tests | Stable MVP | Library, CLI, and examples coverage exist |

## Partial / still shallow

These parts exist and are useful, but should not be treated as complete:

| Area | Status | Why not “done” |
|---|---|---|
| IR | Partial | Lowering is still minimal, especially for complex control flow and resource semantics |
| Codegen | Partial | Pure compute path improved; broader language coverage remains incomplete |
| REPL | Partial | Basic utility exists, but not a mature workflow surface |
| Stdlib | Partial | Enough runtime scaffolding for current compiler path, not a complete language runtime |
| Module resolution | Partial-to-good | Local package/path dependency story is real, remote dependency story is not |
| Manifest build config | Partial-to-good | `target`, `out_dir`, `entry`, `source_roots` are wired; broader package workflow is not |
| CLI local workflow | Partial-to-good | `build`, `check`, `doc`, `fmt`, `metadata`, and compile-test discovery are real; registry/runtime testing remains incomplete |

## Present in tree, but not trusted as complete

These modules exist, but should be considered **experimental, prototype-level, or not on the trusted compiler path**:

| Module area | Status |
|---|---|
| `src/cli/` subcommand framework | Partial-to-good local workflow; registry commands fail-closed |
| `src/optimize/` | Prototype |
| `src/docgen/` | Partial |
| `src/fmt/` | Partial |
| `src/lsp/` | Minimal real path with metadata-aware action hover, diagnostics, and code actions; not mature |
| `src/package/` | Partial-to-good local package support |
| `src/test/` custom framework | Prototype; `cellc test` is compile-only discovery |
| `src/wasm/` | Compiled fail-closed scaffold; executable backend unsupported |
| `src/incremental/` | Stub / future work |
| `src/debug/` | Prototype |
| `src/lifecycle/` | Prototype / not integrated into trusted main path |

Important:

- Several prototype modules now intentionally **fail closed** instead of pretending to succeed.
- They should still be treated as implementation scaffolding, not supported product surface.

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

- `cellc build`
- `cellc check`
- `cellc doc --format markdown|html|json`
- `cellc fmt [--check]`
- `cellc metadata [INPUT]`
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

The in-tree `src/wasm/` module is now compiled and tested as a fail-closed scaffold. It can emit metadata-only Wasm module structure, but rejects executable `action` / `lock` lowering.

## Semantic completeness

The compiler is no longer just “shape-complete”. Some real semantics are now enforced end-to-end:

- function parameters get IR bindings
- parameters are spilled to stack slots in codegen
- local bindings participate in computation
- `return <expr>` now lowers into actual return-value generation
- minimal pure-compute functions such as `add(x, y)` now compile into meaningful arithmetic assembly
- named schema parameters expose fixed field layout metadata
- `param.scalar_field` on a named action/lock schema parameter lowers to unaligned-safe little-endian byte loads for fixed `bool/u8/u16/u32/u64` fields and can emit ELF; the generated ABI now passes schema values as `aN=borsh_ptr, aN+1=borsh_len`, so fixed schema field access performs exact-size and bounds checks
- `consume token` preloads the consumed Input cell bytes in assembly, retains the verifier pointer, and lets `token.scalar_field` lower through loaded-byte exact-size/bounds checks and byte-wise loads; ELF still rejects `consume-expression`
- `read_ref<T>().scalar_field` lowers through `LOAD_CELL Source::CellDep`, loaded-byte exact-size/bounds checks, and byte-wise loads; it can emit CKB-runtime ELF but requires transaction/syscall context
- simple fixed-scalar `create Type { ... }` outputs are verified with `LOAD_CELL Source::Output`, exact-size checks, per-field bounds checks, and equality checks against constants, parameters, or consumed/read schema field aliases; all fixed scalar fields must be covered before the verifier claims completeness
- incomplete `create` output verification paths fail closed in generated assembly instead of silently continuing after an audit comment
- symbolic runtime fallback paths for `transfer`, `destroy`, `claim`, `settle`, dynamic collections, `type_hash`, non-lowered field/index access, dynamic `len`, and non-preloaded `read_ref` now fail closed in generated assembly rather than pretending to produce executable verifier semantics
- metadata now exposes fail-closed runtime features explicitly, so CI/IDE/audit tools do not need to infer those paths from assembly comments
- explicit `#[effect(...)]` annotations are checked against inferred action behavior, including same-module calls and local `path` dependency imports; under-declared scheduler/effect metadata is now a compiler error
- `fn` definitions are enforced as pure helpers, so direct, same-module indirect, or locally imported stateful behavior cannot hide behind action-style lowering
- simple `consume input.u64_field -> create output.u64_field` aliases are verified by comparing loaded Input and Output fields in the prelude
- simple `consume input.u64_field +/- const_or_param_or_local_const +/- ... -> create output.u64_field` expressions are verified in the prelude for left-associative `u64` add/sub chains
- simple `LoadConst` and `Move` sources propagate into prelude-verifiable `u64` expressions, so local constants and aliases do not silently erase verification

Still not safe to call semantically complete:

- full control-flow lowering
- resource lifecycle semantics end-to-end
- generalized schema decoding from `consume` / `read_ref` / `create` loaded cell bytes
- generalized resource conservation and transition relation checks across consumed inputs and created outputs
- right-nested/arbitrary arithmetic state expressions in output verification
- full create expression/resource-handle executable semantics
- complete cross-module call semantics
- comprehensive pattern lowering
- complete mutation semantics for all language constructs

## Validation snapshot

At the time of this snapshot, the `cellscript` crate passed:

- `116` library tests
- `15` CLI tests with `vm-runner`; `13` CLI tests without `vm-runner`
- `1` examples test group

This is enough to justify “working MVP compiler core”, but not enough to justify “complete language toolchain”.

## Non-goals for current snapshot

These should **not** be described as complete today:

- a trusted in-language test runner
- a real language server
- a production-grade package manager / registry client
- a second fully supported backend beyond `asm/elf`
- a full IDE experience beyond the thin VS Code shell-out extension

## Recommended external wording

If you need a short public-facing status line, use:

> `CellScript` is an MVP compiler with a working `asm/elf` compile path, local package support, and partial semantic lowering. Broader tooling modules remain experimental.

If you need a short internal engineering status line, use:

> Core compiler path is real. Periphery is still prototype-heavy.
