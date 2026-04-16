# CellScript Compatibility Matrix

**Snapshot date**: 2026-04-16  
**Scope**: `/Users/arthur/RustroverProjects/Spora/cellscript/` and immediate CKB-style runtime integration assumptions  
**Purpose**: keep the implementation claims precise for four recurring questions:

1. Is CellScript fully compatible with CKB-VM?
2. Do `fn`, `action`, and `lock` have clear boundaries and work together cleanly?
3. How much real integration exists with Spora's DAG-oriented execution model?
4. Has CellScript already completed what the design documentation claims?

This document tracks **implementation reality**, not design intent.  
For broader implementation status, also see [CELLSCRIPT_IMPLEMENTATION_STATUS.md](./CELLSCRIPT_IMPLEMENTATION_STATUS.md).  
For proposal-vs-implementation coverage, see [CELLSCRIPT_DESIGN_IMPLEMENTATION_AUDIT.md](./CELLSCRIPT_DESIGN_IMPLEMENTATION_AUDIT.md).
For the original design target, see [SPORA_DSL_DESIGN_PROPOSAL_CN.md](./SPORA_DSL_DESIGN_PROPOSAL_CN.md).

## Executive Summary

CellScript currently has a real compiler path:

- source -> lexer/parser/type checker -> IR -> RISC-V assembly
- restricted pure-compute source -> generated RISC-V ELF
- restricted schema-parameter field access -> generated RISC-V ELF for fixed-width scalar fields (`bool/u8/u16/u32/u64`) through unaligned-safe little-endian byte loads
- restricted consumed-input field access -> generated CKB-runtime assembly lowering with `LOAD_CELL Source::Input` and loaded-byte bounds checks, while full consume semantics remain symbolic and ELF remains fail-closed
- restricted `read_ref<T>().scalar_field` access -> generated CKB-runtime RISC-V ELF with `LOAD_CELL Source::CellDep`, loaded-byte bounds checks, and byte-wise scalar loads
- restricted fixed-scalar `create Type { ... }` output verification in assembly with `LOAD_CELL Source::Output`, exact-size checks, bounds checks, and field equality checks
- restricted consumed-input-to-created-output `u64` field alias verification in assembly
- restricted left-associative `u64` add/sub chain verification from consumed input fields, constants, local constants, and entry parameters into created output fields, with simple move/alias propagation
- explicit fail-closed assembly paths for symbolic runtime features that do not yet have complete verifier lowering
- optional `vm-runner` feature -> no-argument pure ELF execution through `ckb-vm 0.24`
- compile metadata sidecar -> lowering/runtime/scheduler JSON, including explicit `fail_closed_runtime_features`
- compile metadata sidecar -> explicit VM object ABI declaration (`runtime.vm_abi.format = "molecule"`, `version = 0x8001`) for CKB-style full-object load syscalls; RISC-V ELF artifacts also embed a fixed ABI trailer
- compile result self-validation -> metadata schema version, compiler version, artifact bytes, BLAKE3 hash, metadata hash/size binding, ELF trailer, and standalone-runtime claims are checked before results are returned
- checked action effect declarations -> explicit under-declared `#[effect(...)]` annotations are rejected, including same-module indirect calls and local `path` dependency imports
- checked `fn` purity and call boundaries -> helper functions cannot hide direct, same-module indirect, or locally imported Cell/runtime operations; `fn` cannot call `action` or `lock`; no-return helper calls lower without a result destination and cannot be bound or returned as values
- checked return/value semantics -> value-returning `action` and `fn` must return on all paths; explicit returns, typed tail expressions, and terminal `if` tail expressions lower into real return terminators
- checked local collection semantics -> empty arrays require explicit zero-length type annotations; `Vec.push` propagates item type from `Vec::new()` and rejects incompatible later pushes
- checked call lowering semantics -> unresolved calls no longer default to `u64`; lowering rejects unknown return types instead of fabricating callable values
- CKB-style `LOAD_CELL` ABI prelude in assembly for `consume`, `read_ref`, and `create` summaries

This still does **not** mean full CKB contract compatibility or full stateful runtime completion. The executable ELF path remains fail-closed for symbolic Cell/runtime features such as full `consume` / `create` resource-handle semantics, collections, transfer/claim/settle, lock/type script verification, and broader schema decoding beyond fixed-width scalar fields.

The most accurate short status line today is:

> CellScript targets Spora's CKB-VM-compatible execution substrate, has no-argument pure ELF runner support plus restricted parameterized schema ELF generation, emits auditable lowering/runtime metadata, and has partial CKB-style Cell runtime ABI lowering, but it is not yet a complete executable stateful contract language.

## Recommended Wording

Use:

- `CellScript compiles to RISC-V artifacts intended for Spora's CKB-VM-compatible runtime.`
- `CellScript ELF artifacts embed the Molecule VM object ABI (`0x8001`) for CKB-style full-object load syscalls; verifier loaders strip the trailer before CKB-VM execution and select the declared ABI.`
- `Pure-compute CellScript programs, plus restricted fixed-width schema parameter/read_ref field subsets, can be emitted as ELF or CKB-runtime ELF where no symbolic expression semantics remain.`
- `Consumed-input fixed-width fields are now concretely lowered in assembly, but consume expressions still keep ELF fail-closed until resource conservation semantics are implemented.`
- `A narrow input-field-to-output-field equality check exists for fixed-width u64 aliases, but generalized UDT/resource conservation is still incomplete.`
- `Left-associative u64 add/sub chains over loaded fields, constants, local constants, and entry parameters can be checked, with simple move/alias propagation, but right-nested/arbitrary arithmetic state proofs are still incomplete.`
- `Read-only CKB-runtime ELF can use `LOAD_CELL Source::CellDep`, but it requires real transaction/syscall context and is not runnable through standalone `cellc run`.`
- `Mutating Cell/runtime programs currently emit assembly and metadata, but ELF remains fail-closed until resource/state-transition lowering is complete.`

Avoid:

- `CellScript is fully CKB compatible`
- `CellScript is complete`
- `CellScript has complete DAG integration`
- `CellScript already implements the full design proposal`

## Matrix

| Topic | Current Implementation Reality | Risk / Interpretation | Next Required Step |
|---|---|---|---|
| CKB-VM execution substrate | Real `riscv64-elf` generation exists for pure compute, restricted schema-parameter field loads, and restricted `read_ref` field loads through CKB `LOAD_CELL`. Consumed-input field loads now have concrete CKB-runtime assembly lowering, but `consume-expression` still makes ELF fail-closed. The optional `vm-runner` feature executes only no-argument pure ELF through `ckb-vm 0.24`. | This proves the substrate path, not the full stateful contract surface. Parameterized and CKB-runtime ELF need transaction/ABI/syscall context, not the standalone runner. | Add end-to-end execution through Spora's full verifier context with real transaction data. |
| Full CKB contract compatibility | Not complete. CellScript uses a CKB-style VM/syscall substrate but does not claim binary or semantic compatibility with arbitrary CKB contracts. | `Runs on CKB-VM` is true for the supported subset. `Full CKB contract compatibility` is false. | Keep substrate compatibility and contract compatibility separate in docs and release notes. |
| `riscv64-asm` target | Real and used by regression tests. Stateful features preserve symbolic operations, emit CKB-style `LOAD_CELL` ABI setup for access summaries, and fail closed for runtime features that lack complete verifier lowering. | Assembly is auditable, and unsupported verifier semantics are no longer silent success paths, but this is still not proof of executable state semantics. | Continue using assembly as transparent lowering output while building real schema decoding and invariant checks. |
| `riscv64-elf` target | Real for pure computation/control flow, fixed-width scalar field loads from named action/lock schema parameters, and fixed-width scalar fields loaded from `read_ref` CellDep bytes. Consumed Input field loads are concretely emitted in assembly, but `consume-expression` still rejects ELF. | Fail-closed behavior is correct; the executable target is intentionally a subset. Named schema parameter loads now use a pointer+length ABI, so fixed scalar field access can be bounds-checked. | Extend executable lowering feature by feature and keep rejection tests for unsupported operations. |
| CKB-style syscall ABI | Summary prelude uses `LOAD_CELL` with `A0=buffer`, `A1=size pointer`, `A2=offset`, `A3=index`, `A4=source`; `consume=Input`, `read_ref=CellDep`, `create=Output`. CellScript metadata declares Molecule VM object ABI `0x8001` for full-object `LOAD_SCRIPT` / `LOAD_INPUT` / `LOAD_CELL` / `LOAD_HEADER`; RISC-V ELF artifacts embed a fixed ABI trailer; the exec verifier strips it and selects `VmAbiFormat::Molecule`. Consumed-input field reads, `read_ref` field reads, simple `create` output fields, simple input-field-to-output-field equality checks, and left-associative `u64` add/sub expected expressions now perform loaded-byte bounds checks. | ABI shape is now aligned for these paths, but assembly/non-ELF artifacts still rely on sidecar/verifier policy. Full payload decoding and semantic validation are still incomplete. | Add authenticated manifests for non-ELF artifacts, then add generalized typed schema decoding and compare source-level invariants against loaded cell bytes. |
| Typed schema layout | IR and metadata now expose field offsets and fixed encoded sizes. Codegen can lower `param.scalar_field` for named schema parameters, consumed `input.scalar_field`, and `read_ref<T>().scalar_field` into unaligned-safe byte-wise loads for `bool/u8/u16/u32/u64`. Schema parameters, consumed-input, and `read_ref` fields include exact-size and bounds checks when their source supplies a length word. Local fixed-array literals now parse, type-check homogeneous elements, require explicit zero-length annotations for empty arrays, reject immutable element writes, lower compile-time-constant element reads/writes, unroll local foreach, and fold local `len()` calls without symbolic runtime fallback. Local tuple literals lower static field projection/assignment/destructuring without symbolic field access, and array-of-tuples static index projection plus local foreach destructuring preserve tuple metadata. `Vec.push` propagates item types from `Vec::new()` and rejects incompatible pushes once the vector has a concrete item type. Simple fixed-scalar `create` outputs now require all fields to be verifier-covered, enforce exact schema size, and check field equality against constants, parameters, or consumed/read schema field aliases; `u64` fields additionally support local constants or left-associative `u64` add/sub chains over prelude-available operands; simple move/alias propagation is supported. Incomplete `create` verifier summaries fail closed at runtime instead of continuing after a warning comment. | This is not full schema decoding. Full consume/create expression/resource-handle semantics remain fail-closed, generalized resource conservation is not complete, dynamic array indexes are not fully executable, and nested/dynamic cell fields are not handled. | Add nested/dynamic decoding and verify source-level invariants against loaded cell bytes. |
| Protocol semantic lowering | IR records `consume_set`, `read_refs`, and `create_set`; generated metadata includes runtime access summaries and scheduler witness hex. Generated assembly now rejects unsupported symbolic runtime operations instead of leaving them as successful silent paths. | Access summaries and fail-closed paths are real, but not yet equivalent to a verified state-transition proof. | Connect access summaries to executable verifier code and scheduler admission checks. |
| `fn` semantics | `fn` is now a distinct AST/IR/metadata category (`FnDef` -> `IrPureFn` -> `functions[]`). It is enforced as pure: directly inferred, same-module call-derived, and local `path` dependency import-derived `ReadOnly`, `Creating`, `Destroying`, or `Mutating` behavior is rejected. A `fn` cannot call an `action` or a `lock`. Helpers without a return type use internal `Unit`, lower to `Call { dest: None }`, and cannot be bound or returned as values. | Pure helper behavior is no longer represented as a scheduler-visible action, stateful behavior can no longer hide behind local `fn` call chains or imported local package calls, and no-return helpers no longer masquerade as `u64` values in IR. Remote registry dependency summaries still need a stable signed format. | Extend this through registry package summaries and richer purity/effect manifests. |
| `action` semantics | Real state-transition entry abstraction. Effects, scheduler hints, access summaries, and metadata are emitted. Value-returning actions must return on all paths via explicit `return`, typed tail expressions, or terminal `if` branches with typed tail expressions, and lowering emits real return terminators for those forms. Calls whose return type cannot be resolved are rejected instead of lowered as `u64`. | The surface is meaningful, but resource/effect correctness is not yet fully enforced as a theorem. | Add compiler checks proving declared effects match all lowered stateful operations. |
| `lock` semantics | Has a distinct pipeline and non-bool lock return definitions are rejected. | Stronger than before, but authorization domain separation and witness binding still need hardening. | Add explicit signature/witness binding specs and negative tests for replay/domain mistakes. |
| `fn` / `action` / `lock` cooperation | `fn` is pure helper code, `action` is state-transition entry code, and `lock` is authorization predicate code. `action` and `lock` may call `fn`; `fn` may call only `fn`; `lock` cannot call `action` or another `lock`. Codegen emits `action`/`lock` entrypoints before helper functions so helpers are not accidentally selected as VM entrypoints. | Boundaries are usable and tested, but witness/signature domain rules for `lock` still need stronger protocol-level specs. | Document exact witness binding and authorization helper rules; extend call-boundary checks across signed registry packages. |
| Effect classes | Effect classes are inferred from `read_ref`, `consume`, `create`, `destroy`, `transfer`, `claim`, `settle`, same-module calls, and local `path` dependency imports; explicit under-declarations are rejected. | This blocks low-reported scheduler metadata for local sources, but it is still not a complete static effect/resource proof. | Extend effect checking through signed registry summaries and future resource lifecycle rules. |
| `touches_shared` | Metadata now exposes non-empty shared-touch summaries when stateful shared reads are present. | Better scheduler visibility, but derivation still needs a formal spec and verifier consumer. | Specify hash/domain derivation and wire it into the real scheduler path. |
| Access summary metadata | `metadata_schema_version`, `compiler_version`, `artifact_hash_blake3`, `artifact_size_bytes`, `source_hash_blake3`, `source_content_hash_blake3`, `source_units[]`, `params`, `functions[]`, `consume_set`, `read_refs`, `create_set`, `ckb_runtime_accesses`, `ckb_runtime_features`, `runtime.vm_abi`, `standalone_runner_compatible`, `symbolic_runtime_features`, `fail_closed_runtime_features`, `verifier_obligations`, and `elf_compatible` are emitted as JSON. | CI/audit tooling can distinguish pure helpers, source provenance, path-bound source identity, path-independent source content identity, CKB-runtime ELF, standalone runner ELF, required VM object ABI, symbolic requirements, explicit fail-closed runtime paths, and lowering obligations. Compile results now self-validate schema/toolchain compatibility plus artifact/source/metadata binding, but scheduler/runtime policy still has to consume it. | Add CI policies and runtime checks that reject inconsistent metadata/artifacts. |
| DAG integration | Scheduler metadata and Borsh witness bytes are emitted. | This is a real compiler output path, but not a completed DAG scheduler integration. | Feed metadata into DAG access/conflict logic and add adversarial scheduling tests. |
| CLI ecosystem | `build`, `check`, `doc`, `fmt`, `init`, `add`, `remove`, `clean`, `info`, `metadata`, `verify-artifact`, package compilation, compile-test discovery, and feature-gated `run` have real paths. `build` and `check` now have CLI and manifest `[policy]` metadata gates for production, fail-closed, symbolic runtime, and CKB runtime requirements; `build` applies the gate before writing artifacts and `build --json` emits a machine-readable artifact/metadata/hash summary; `check --all-targets` validates both asm and ELF lowering without writing artifacts, and `check --json` emits a machine-readable checked-target/policy summary. `init --json`, `add --json`, `remove --json`, `clean --json`, and `info --json` cover local package lifecycle summaries for CI/IDE callers; `add/remove --dev/--build` now target the correct manifest sections and `add --git/--path` writes detailed dependency sources. `verify-artifact` validates artifact/metadata binding for CI without recompilation, `--verify-sources` checks source-unit hashes against files on disk, `--expect-*hash` pins artifact/source/source-content BLAKE3 values, `--production` / `--deny-*` applies the same metadata policy gates to stored artifacts, and `--json` emits a machine-readable summary. `doc` emits API docs plus lowering audit reports and verifier obligations. Registry/package network commands remain fail-closed. | Core local developer workflow is usable; ecosystem distribution is not complete. | Finish registry install/publish/update and expand CI policy presets. |
| Docgen/fmt/package/test/wasm | `doc` and `fmt` are real enough for package sources. `doc --json` reports generated output metadata, and `fmt --json` reports clean/dirty changed-file summaries. Package discovery and path dependencies exist. `cellc test` discovers `tests/**/*.cell` and supports positive compile tests, strict `// cellscript-test: expect-success` / `expect-fail` / `expect-error` diagnostics, per-file target selection, per-file production/symbolic/CKB runtime policy gates, runtime metadata assertions, action/function/lock metadata classification assertions, and `--json` CI summaries. Unknown or conflicting test directives fail. `src/wasm/` is now compiled and tested, but explicitly fail-closed for executable action/lock lowering. | Tooling has real local paths, but not yet Solidity/Move-grade. | Implement real property/invariant/fuzz testing and either build a real Wasm backend or keep it explicitly unsupported. |
| IDE/LSP | VS Code extension and in-crate LSP path exist with diagnostics/formatting/navigation support. Action hover, receipt hover, diagnostics, and code actions now surface lowering metadata, ELF compatibility, symbolic runtime features, verifier obligations, lifecycle states/transitions, and CKB access summaries. | Useful but not yet a mature language server with semantic indexing, full build graph awareness, and rename safety. | Add semantic tokens, cross-package indexing hardening, safer rename, and compiler-version synchronization. |
| Design proposal completion | The design proposal remains ahead of code. Current code is a working compiler/toolchain under production hardening with metadata paths, effect checks, lifecycle state/transition metadata, static lifecycle guards, LSP lifecycle diagnostics, fixed-scalar lifecycle transition prelude checks, local fixed-array static index/foreach/len lowering, and local tuple destructuring / array-of-tuples static projection lowering for complete output verifier paths. | Treat the proposal as target architecture, not shipped truth. | Keep this matrix and implementation-status docs updated after each semantic milestone. |

## Direct Answers

### 1. Is CellScript fully compatible with CKB-VM?

No, if “compatible” means complete CKB contract semantic compatibility.

Yes, only in the narrower substrate sense:

- pure CellScript ELF can target CKB-VM-compatible RISC-V
- named schema parameters with fixed scalar field access can compile to ELF using a pointer+length Borsh blob ABI and runtime bounds checks
- consumed input fixed scalar fields can be loaded from `LOAD_CELL Source::Input` bytes with bounds checks
- created output fixed `u64` fields can be compared against consumed input fixed `u64` field aliases
- created output fixed `u64` fields can be compared against left-associative `+/-` chains over consumed fields, constants, local constants, and entry parameters
- `read_ref<T>().scalar_field` can compile to CKB-runtime ELF using `LOAD_CELL Source::CellDep` plus bounds checks
- simple `create` output fields can be checked in assembly against constant/parameter `u64` expected values
- `cellc run` can execute no-argument pure ELF through `ckb-vm 0.24` when built with `vm-runner`
- stateful assembly lowering now uses CKB-style `LOAD_CELL` register conventions
- full-object syscall output can be selected as Molecule ABI `0x8001` from the embedded ELF ABI trailer or by passing the artifact metadata ABI into the exec verifier

The current hard boundary is:

> Pure compute, narrow schema-parameter field access, and narrow read-only `read_ref` field access are executable. Narrow consumed-input field access is concretely auditable in assembly, but broader mutating Cell/runtime semantics are not yet fully executable as resource-conserving state transitions.

Unsupported mutating/runtime operations now fail closed in generated assembly. This is a safety improvement, not a completeness claim.

### 2. Do `fn`, `action`, and `lock` work together cleanly?

Yes for the local compiler boundary; still incomplete at the broader authorization/protocol-spec level.

- `fn` is a distinct pure helper category in AST, IR, codegen, and metadata.
- no-return `fn` calls have internal `Unit` type, produce no IR destination, and are accepted only as statement calls.
- `action` is the main state-transition entrypoint and carries effect/scheduler/access metadata.
- `lock` is a distinct authorization entrypoint and now rejects non-bool return definitions.
- `action` and `lock` can call `fn`.
- `fn` can call only `fn`; attempts to call `action` or `lock` are compile errors.
- `lock` cannot call `action` or another `lock`.

The remaining weakness is not parsing. It is semantic specification:

- how witness/signature domains are bound for `lock`
- how authorization helpers interact with stateful action validation

### 3. How much real DAG integration exists?

More than structural metadata, but not complete.

Real today:

- effect class metadata
- `touches_shared`
- `consume_set`, `read_refs`, `create_set`
- per-action scheduler witness Borsh hex
- `ckb_runtime_accesses` with operation/source/index/binding
- `fail_closed_runtime_features` for unsupported runtime paths that the generated assembly rejects
- entrypoint parameter ABI metadata
- CLI-accessible JSON through `cellc metadata`

Still missing:

- runtime consumption of this metadata by the real DAG scheduler
- conflict/admission checks based on the metadata
- adversarial tests proving unsafe parallel schedules are rejected
- formal derivation rules for every source-level effect and access path

### 4. Has CellScript completed what the design document specifies?

No.

Completed or materially real:

- compiler core
- RISC-V assembly
- pure RISC-V ELF
- optional CKB-VM runner for no-argument pure ELF
- restricted read-only CKB-runtime ELF for `read_ref<T>().scalar_field`
- restricted consumed-input field verification for fixed scalar fields
- restricted create-output field verification for fixed scalar fields
- restricted input-field-to-output-field equality verification for fixed `u64` aliases
- restricted left-associative `u64` add/sub output verification
- simple local-constant and move/alias source propagation for prelude expressions
- package build/check/doc/fmt flows
- compiler-test discovery, expected-failure diagnostics, per-file target/policy directives, runtime metadata assertions, and entrypoint classification assertions through `cellc test`
- metadata sidecars and `cellc metadata`
- `cellc verify-artifact` for emitted artifact/metadata consistency checks
- partial IDE/LSP path
- action hover with lowering/runtime metadata
- lowering/runtime diagnostics for ELF-incompatible symbolic actions
- code actions for inspecting metadata or using asm while executable stateful lowering is incomplete
- partial CKB-style runtime ABI lowering
- typed schema layout metadata and restricted executable `u64` parameter/consume/read_ref field lowering
- no-return `Unit` helper calls, tail-expression return lowering, typed empty arrays, `Vec.push` item propagation, and unknown-call return rejection

Not complete:

- full executable stateful runtime lowering
- full schema decoding from all loaded cell bytes and field invariant enforcement
- generalized conservation checks connecting consumed inputs to created outputs
- right-nested/arbitrary arithmetic state expressions
- full create expression/resource-handle executable semantics
- full resource/effect/lifecycle-transition soundness
- full witness/signature/domain binding story
- full DAG scheduler integration
- full custom runtime/property/fuzz/invariant testing
- full package registry lifecycle
- executable wasm backend, if it remains a claimed target

## Go / No-Go Judgment

For pure-compute scripts and compiler/tooling development: **Go**.

For serious production stateful protocol contracts: **No-Go** until the following are complete:

- generalized typed schema decoding from CKB-style loaded cell bytes
- executable `consume`, `create`, `transfer`, `claim`, `settle`, and `destroy` semantics
- static checks proving effect/resource declarations match lowered behavior
- witness/signature/domain binding rules with negative tests
- scheduler metadata enforcement in the real DAG/runtime path
- property/fuzz/adversarial transaction tests around invariants

The safe external claim remains:

> CellScript has a real CKB-VM-targeting compiler core and auditable metadata path, but it is not yet a complete production-grade stateful smart contract language.
