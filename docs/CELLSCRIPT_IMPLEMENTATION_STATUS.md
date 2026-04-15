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
| RISC-V ELF emission | Usable MVP | Main output path, with external toolchain support and built-in fallback |
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

## Present in tree, but not trusted as complete

These modules exist, but should be considered **experimental, prototype-level, or not on the trusted compiler path**:

| Module area | Status |
|---|---|
| `src/cli/` subcommand framework | Prototype |
| `src/optimize/` | Prototype |
| `src/docgen/` | Prototype |
| `src/fmt/` | Prototype |
| `src/lsp/` | Prototype |
| `src/package/` | Prototype |
| `src/test/` custom framework | Prototype |
| `src/wasm/` | Stub / future target |
| `src/incremental/` | Stub / future work |
| `src/debug/` | Prototype |
| `src/lifecycle/` | Prototype / not integrated into trusted main path |

## CLI reality

The currently trusted CLI is the single-entry compiler in:

- [main.rs](/Users/arthur/RustroverProjects/Spora/cellscript/src/main.rs)

Real supported flows:

- `cellc <input>`
- `cellc <input> --target riscv64-asm`
- `cellc <input> --target riscv64-elf`
- `cellc <input> -o <path>`
- `cellc <input> --lex`
- `cellc <input> --parse`
- `cellc --interactive`
- `cellc --gen-stdlib`

There is also a larger subcommand framework under `src/cli/`, but it should currently be treated as **separate prototype code**, not as the production CLI surface.

## Output reality

Currently real output targets:

- `riscv64-asm`
- `riscv64-elf`

Important note:

- `ELF` output is real and generated now.
- External RISC-V GNU toolchains are used when available.
- Built-in fallback ELF assembly/writing still exists for environments without a working external toolchain.

Not currently a trusted output target:

- `WebAssembly`

## Semantic completeness

The compiler is no longer just “shape-complete”. Some real semantics are now enforced end-to-end:

- function parameters get IR bindings
- parameters are spilled to stack slots in codegen
- local bindings participate in computation
- `return <expr>` now lowers into actual return-value generation
- minimal pure-compute functions such as `add(x, y)` now compile into meaningful arithmetic assembly

Still not safe to call semantically complete:

- full control-flow lowering
- resource lifecycle semantics end-to-end
- complete cross-module call semantics
- comprehensive pattern lowering
- complete mutation semantics for all language constructs

## Validation snapshot

At the time of this snapshot, the `cellscript` crate passed:

- `44` library tests
- `6` CLI tests
- `1` examples test group

This is enough to justify “working MVP compiler core”, but not enough to justify “complete language toolchain”.

## Recommended external wording

If you need a short public-facing status line, use:

> `CellScript` is an MVP compiler with a working `asm/elf` compile path, local package support, and partial semantic lowering. Broader tooling modules remain experimental.

If you need a short internal engineering status line, use:

> Core compiler path is real. Periphery is still prototype-heavy.
