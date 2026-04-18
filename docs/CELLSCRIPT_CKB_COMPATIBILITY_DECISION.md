# CellScript CKB Compatibility and Molecule Decision

**Date**: 2026-04-18  
**Status**: Prelaunch design decision  
**Scope**: CellScript compiler, Spora VM ABI, CKB-style Cell compatibility, Molecule serialization boundaries

## Existing Documents

There are already related documents, but they answer adjacent questions rather than this exact decision:

- [`exec/src/serialization/README.md`](../exec/src/serialization/README.md) documents the execution-layer serialization split: consensus hashes are custom streaming Blake3, storage remains Borsh with version envelopes, and VM-facing objects can use Molecule ABI `0x8001`.
- [`docs/SERIALIZATION_LAYER_GOVERNANCE_MIGRATION_PLAN.md`](./SERIALIZATION_LAYER_GOVERNANCE_MIGRATION_PLAN.md) captures the original migration plan and the reason not to switch the whole stack to Molecule.
- [`docs/CELLSCRIPT_COMPATIBILITY_MATRIX.md`](./CELLSCRIPT_COMPATIBILITY_MATRIX.md) tracks implementation reality for CellScript, including current CKB-VM-compatible and fail-closed boundaries.
- [`docs/CKB_TO_SPORA_MAPPING.md`](./CKB_TO_SPORA_MAPPING.md) maps CKB primitives to Spora primitives, but it is a broad migration map rather than a CellScript design decision.
- [`docs/cell_ckb_comparison_audit.md`](./cell_ckb_comparison_audit.md) compares Spora Cell/CellTx design with CKB at the data-model level.

This document centralizes the decision:

1. Whether CellScript serialization should support Molecule.
2. Whether CellScript can be CKB-compatible as designed, or needs Spora-specific design.

## Decision Summary

CellScript should support Molecule at the **VM-facing object ABI, transaction-carried CellScript ABI, and persistent public schema boundary**. Because Spora has not launched yet, the project should avoid carrying public Borsh compatibility debt into launch.

CellScript should target a **CKB-compatible substrate**, not full CKB contract compatibility by default.

Molecule support and CKB support should be delivered as one prelaunch compatibility track:

- public CellScript byte surfaces launch on Molecule;
- `spora`, `ckb`, and `portable-cell` target profiles are introduced before launch;
- CKB support is a separate compiled artifact/profile, not a promise that one Spora artifact runs unchanged on CKB;
- any incomplete CKB profile should remain gated/experimental until the policy checks and byte-layout tests pass, rather than becoming a post-launch ABI migration.

The intended split is:

| Area | Decision |
|---|---|
| VM-loaded CKB-style objects | Launch with Molecule. This includes `Script`, `OutPoint`, `CellInput`, `CellOutput`, `ResolvedCell`, and `ResolvedHeader` where visible to scripts. |
| User-defined persistent CellScript cell data | Launch public/stateful contract data with generated Molecule schemas. Fixed-width current layouts are only a stepping stone. |
| Scheduler witness | Migrate before launch to one canonical Molecule witness format if the bytes are transaction-carried or externally parsed. Do not publish a Borsh legacy scheduler witness ABI. |
| Compile metadata sidecar | Keep JSON. It is an audit/tooling surface, not VM object bytes. |
| Internal storage and node bookkeeping | Keep Borsh plus version envelope unless a concrete ABI consumer needs Molecule. |
| TxID, script hash, sighash | Do not switch to CKB-style hash-of-Molecule bytes without an explicit protocol migration. Spora currently uses domain-separated Blake3 paths. |

Short form:

> Launch public CellScript ABI on Molecule. Keep Borsh only for private implementation details that never become transaction-carried or public cross-language contract ABI.

Version fields are still allowed. Their purpose is future schema evolution after launch, not prelaunch Borsh/Molecule dual-format compatibility.

## CKB Baseline

CKB treats Molecule as the canonical encoding for chain objects. In CKB, `Script`, `OutPoint`, `CellInput`, `CellOutput`, `RawTransaction`, and `Transaction` are defined in `util/gen-types/schemas/blockchain.mol`. CKB script hash and transaction hash helpers operate over the canonical packed bytes.

Important CKB properties:

- `Script` is a Molecule table:
  - `code_hash: Byte32`
  - `hash_type: byte`
  - `args: Bytes`
- `OutPoint` is a fixed Molecule struct:
  - `tx_hash: Byte32`
  - `index: Uint32`
- `CellInput` is a fixed Molecule struct:
  - `since: Uint64`
  - `previous_output: OutPoint`
- `CellOutput` is a Molecule table:
  - `capacity: Uint64`
  - `lock: Script`
  - `type_: ScriptOpt`
- `RawTransaction` and `Transaction` are Molecule tables.
- Script hash is based on the serialized packed script bytes in CKB.

That means CKB compatibility is not just a matter of matching Rust structs. The byte layout and hash domains matter.

## Current Spora and CellScript State

Spora already has the right partial architecture:

- `exec/src/serialization/molecule_compat.rs` implements CKB-style Molecule wire layouts for VM-facing values.
- `VmAbiNegotiator` exposes Molecule ABI version `0x8001`.
- `LOAD_SCRIPT`, `LOAD_INPUT`, `LOAD_CELL`, and `LOAD_HEADER` can select `VmAbiFormat::Molecule`.
- CellScript compile metadata declares `runtime.vm_abi.format = "molecule"` and `runtime.vm_abi.version = 0x8001`.
- CellScript ELF artifacts embed a fixed VM ABI trailer so Spora's verifier can strip the trailer and select the declared ABI before CKB-VM execution.
- CellScript scheduler witness bytes are currently Borsh with magic/version and admission checks. This is an implementation state, not a launch ABI commitment.

The key distinction is that Spora's current consensus hashes are not CKB hashes:

- Spora `Script::hash()` uses domain-separated Blake3 over explicit fields.
- Spora txid/wtxid/sighash use custom streaming Blake3 domains.
- CKB uses Blake2b over canonical packed Molecule bytes for the corresponding CKB objects.

This is an intentional protocol difference, not a missing serializer.

## Compatibility Assessment

### Compatible Without Major Special Design

These CellScript design surfaces can stay close to CKB:

| Surface | Compatibility judgment |
|---|---|
| VM instruction target | Compatible. CellScript emits RISC-V artifacts intended for CKB-VM-compatible execution. |
| Lock/type script split | Compatible at the model level. CellScript can emit authorization and state-transition scripts. |
| `Script` fields | Compatible shape: `code_hash`, `hash_type`, `args`. Spora also aligns `hash_type` values with CKB's `Data = 0`, `Type = 1`, `Data1 = 2`, `Data2 = 4`. |
| Cell primitives | Compatible shape for `OutPoint`, `CellInput`, `CellOutput`, `CellDep`, and separated `outputs_data`. |
| VM object loading ABI | Compatible when `VmAbiFormat::Molecule` is selected. |
| Basic syscall calling convention | Mostly compatible for CKB-style load syscalls and source/index/field access patterns. |
| Fixed-width scalar state data | Compatible as a lowest common subset if the schema is explicitly defined and byte offsets are stable. |

### Requires Spora-Specific Design

These surfaces cannot be made CKB-compatible by serialization alone:

| Surface | Why it is Spora-specific |
|---|---|
| Consensus ordering | Spora is DAG/GhostDAG-oriented; CKB is linear-chain oriented. Header context and finality assumptions differ. |
| Time/ordering locks | Spora uses DAA score semantics where CKB uses epoch-style chain semantics. |
| TxID/script hash/sighash | Spora uses domain-separated Blake3 and custom streaming hash material. CKB uses Blake2b over packed bytes. |
| Scheduler metadata | CellScript's `touches_shared`, scheduler witness, trusted summaries, and MPE `BlockAccessSummary` have no direct CKB equivalent. |
| VM ABI trailer | Spora ELF artifacts can embed `SPORABI` trailer bytes. CKB will not strip this trailer, so CKB-target artifacts must omit it or use a CKB-specific packaging path. |
| `ResolvedHeader` | Spora headers include DAG-specific fields such as parent levels, DAA score, and Spora roots. CKB header ABI is different. |
| Type identity | Current `#[type_id("...")]` is metadata-level stable identity, not a full CKB type-id lineage verifier. CKB-target semantics would need a real CKB type-id rule. |
| Pool/shared-state scheduling | CellScript's Pool and shared-state patterns are designed for Spora's parallel execution roadmap. |
| Native helper syscalls | Spora-specific syscalls or syscall numbers must be excluded or shimmed for a real CKB target. |

## Design Answer

CellScript can be **source-level portable for a constrained common subset**, but a single compiled artifact should not be treated as both Spora-compatible and CKB-compatible.

The right model is target profiles:

| Profile | Meaning |
|---|---|
| `spora` | Current native target. Uses Spora CellTx, Spora hash domains, Spora DAG header context, Spora scheduler metadata, and optional VM ABI trailer. |
| `ckb` | Prelaunch gated portability target. Uses CKB transaction/header assumptions, CKB packed Molecule bytes, CKB-compatible hash domains, no Spora ABI trailer, no Spora MPE scheduler witness requirements, and only CKB-supported syscalls. |
| `portable-cell` | A source-level subset that avoids Spora-only features and can be compiled separately for `spora` or `ckb`. |

This implies CellScript should not try to make every feature CKB-compatible. It should isolate the CKB-compatible substrate and mark Spora-native features explicitly.

## Molecule Scope

### Must Support Molecule

CellScript should support Molecule before launch for:

1. Full-object VM loads:
   - `LOAD_SCRIPT`
   - `LOAD_INPUT`
   - `LOAD_CELL`
   - `LOAD_HEADER`, with target-specific schema
2. CKB-style chain primitives:
   - `Script`
   - `OutPoint`
   - `CellInput`
   - `CellOutput`
   - `CellDep`
3. Public contract state schemas when the data is intended to be:
   - read by scripts
   - read by SDKs in multiple languages
   - preserved across versions
   - potentially portable to CKB
4. Transaction-carried CellScript scheduler metadata if it is consumed by:
   - mempool admission
   - mining/template policy
   - consensus scheduler summaries
   - wallet or RPC builders
   - external SDKs/indexers

### Should Not Require Molecule

CellScript does not need Molecule for:

1. Compiler metadata JSON.
2. Internal package manifests.
3. LSP/docgen/reporting data.
4. RocksDB/node-internal state that is already guarded by version envelopes.
5. Temporary prelaunch implementation fixtures that are removed or migrated before a public network release.

## Scheduler Witness Decision

Because Spora has not launched, the scheduler witness should not be designed as a Borsh `v1` followed by a Molecule `v2`. That approach is appropriate for an already-launched chain; it is unnecessary compatibility debt before launch.

Launch decision:

- If scheduler witness bytes are transaction-carried or externally parsed, the launch format should be Molecule.
- The compiler, wallet/builder, mempool/template policy, and consensus scheduler should all consume the same canonical Molecule witness bytes.
- Current Borsh witness generation can exist only as a private migration step while implementation is being changed.
- The public metadata field should stop naming the format as `scheduler_witness_borsh_hex` before launch.
- A version byte or schema version may remain, but it identifies the launch schema and future post-launch evolution, not a prelaunch Borsh legacy format.

The current Borsh scheduler witness is acceptable only as temporary implementation state because it is:

- explicitly versioned by magic/version
- decoded through a specific admission path
- checked against concrete transaction source bounds
- optionally checked against trusted compiler/builder summaries
- scoped to Spora scheduling policy, not CKB chain object encoding

Before launch, replace that temporary format with a Molecule schema instead of publishing both formats.

Required launch schema shape:

- `magic`
- `schema_version`
- `effect_class`
- `parallelizable`
- `touches_shared`
- `estimated_cycles`
- `accesses`
- `operation`
- `source`
- `index`
- `binding_hash`

The schema should preserve the existing admission rules: effect class validation, operation/source compatibility, transaction source-index bounds, and trusted summary matching.

## CKB Portability Rules

A CellScript program can be considered CKB-portable only if it avoids:

- Spora DAG header fields and DAA-only assumptions.
- Spora-native hash domains.
- Spora scheduler/MPE metadata as a required validity condition.
- Spora-only syscalls.
- Spora ABI trailer packaging.
- Metadata-only `type_id("...")` assumptions unless mapped to a real CKB type-id verifier.
- Pool/shared-state scheduling semantics that depend on Spora's parallel executor.

It may use:

- RISC-V CKB-VM-compatible code.
- Lock/type script structure.
- CKB-style cell primitives.
- Molecule object ABI.
- Fixed-width state schemas that have a generated or declared Molecule layout.
- CKB-supported syscalls only.

## One-Shot Prelaunch Plan

Because there is no public network compatibility burden yet, the recommended plan is a one-shot prelaunch compatibility pass rather than staged public compatibility.

The target end state before public release:

- Spora-native CellScript artifacts launch with Molecule at every public byte boundary.
- CKB-targeted CellScript artifacts are compiled through an explicit `ckb` profile with CKB packaging/hash/syscall/header rules.
- Shared portable source is expressed through `portable-cell`, then compiled separately to `spora` or `ckb`.
- Borsh remains allowed only behind private node/compiler boundaries that are not transaction-carried, consensus-facing, RPC-facing, or cross-language contract ABI.

### Phase A: Freeze Launch ABI Surfaces

Classify every CellScript byte format before implementation work:

| Surface | Launch ABI? | Launch format |
|---|---:|---|
| VM full-object loads | Yes | Molecule |
| Persistent public cell data | Yes | Molecule |
| Transaction-carried scheduler witness | Yes, if included in witness or RPC/template flow | Molecule |
| Compile metadata sidecar | No | JSON |
| Internal storage/cache | No | Borsh + version envelope allowed |
| Test-only compiler fixtures | No | Any, but must not leak into public ABI |

Exit gate: no public or transaction-carried CellScript format is described as Borsh in release-facing docs or metadata.

### Phase B: Add Generated Molecule Schemas

Add `.mol` schemas and generated bindings for:

- CKB-compatible chain primitives:
  - `Script`
  - `OutPoint`
  - `CellInput`
  - `CellOutput`
  - `CellDep`
- Spora VM-facing objects:
  - `ResolvedCell`
  - `ResolvedHeader`
- CellScript scheduler witness:
  - scheduler witness root table
  - access vector
  - shared-touch vector
- CellScript user-defined persistent schemas:
  - `resource`
  - `shared`
  - `receipt`
  - fixed-width `struct` where used as cell data

Exit gate: handwritten Molecule compatibility helpers either delegate to generated bindings or are covered by byte-for-byte tests against generated bindings.

### Phase C: Migrate CellScript Compiler Output

Change compiler metadata and APIs from Borsh-specific scheduler naming to format-neutral or Molecule-specific launch naming:

- Replace `scheduler_witness_borsh_hex` with a launch field such as `scheduler_witness_hex` plus `scheduler_witness_abi = "molecule"`.
- Keep `ActionMetadata::scheduler_witness_bytes()`, but make it return Molecule bytes.
- Keep strict admission semantics unchanged.
- Update wallet/mining/consensus tests to reject old Borsh witness bytes after the migration point.

Exit gate: no production code path emits Borsh scheduler witness bytes.

### Phase D: Add Target Profiles Before Launch

Add a target-profile concept in metadata and compile policy:

```text
target_profile = "spora" | "ckb" | "portable-cell"
```

Expose at least:

   - `target_chain`
   - `vm_abi`
   - `hash_domain`
   - `syscall_set`
   - `artifact_packaging`
   - `header_abi`
   - `scheduler_abi`

Profile rules:

| Profile | Rules |
|---|---|
| `spora` | Spora hash domains, Spora DAG headers, Molecule VM ABI, Spora scheduler witness, optional `SPORABI` trailer. |
| `ckb` | CKB Molecule objects, CKB hash semantics, CKB header/syscall assumptions, no `SPORABI` trailer, no Spora scheduler-required validity. |
| `portable-cell` | Source subset that avoids Spora-only semantics and can be compiled separately to `spora` or `ckb`. |

Exit gate: compiler metadata can clearly say whether an artifact is Spora-native, CKB-targeted, or only source-portable.

### Phase E: Add CKB Compatibility Policy Gates

Add lint/check gates for `portable-cell` and `ckb` targets:

- reject Spora-only syscalls
- reject DAA/header assumptions not expressible on CKB
- reject scheduler-required semantics
- require Molecule schemas for persistent state
- reject `SPORABI` trailer packaging for CKB target
- reject Spora hash-domain assumptions when producing CKB-target artifacts
- reject metadata-only `type_id("...")` when a real CKB type-id lineage verifier is required

Exit gate: a source file can be mechanically classified as Spora-only, CKB-targetable, or portable-subset.

### Phase F: Test Against CKB Baseline

Add byte-level compatibility tests against CKB's Molecule layouts:

- `Script`
- `OutPoint`
- `CellInput`
- `CellOutput`
- `CellDep`
- scheduler witness generated bytes
- generated persistent cell data schemas

For the `ckb` target, add tests that verify:

- no Spora ABI trailer
- no Spora-only syscall use
- no Spora DAG header dependency
- CKB-compatible hash/material selection
- CKB-compatible Molecule object layout

Exit gate: the compatibility claim is backed by generated schema tests, not by manual struct similarity.

## Required Follow-Up Work

The implementation tasks implied by the plan are:

1. Replace prelaunch Borsh scheduler witness output with a Molecule launch schema.
2. Add generated `.mol` schemas for Spora VM ABI objects instead of relying indefinitely on handwritten Molecule layout code.
3. Add a CellScript schema generator for user-defined persistent state types.
4. Add target-profile metadata and policy gates before launch:
   - `target_chain`
   - `vm_abi`
   - `hash_domain`
   - `syscall_set`
   - `artifact_packaging`
   - `header_abi`
   - `scheduler_abi`
5. Implement the gated `ckb` compiler profile as part of the same compatibility track, even if it remains experimental until all CKB policy and layout tests pass.
6. Split documentation wording:
   - "CKB-VM-compatible" for the execution substrate.
   - "CKB-compatible contract" only for an explicit gated restricted target profile.
7. Add lint/policy gates for "portable-cell" mode:
   - reject Spora-only syscalls
   - reject DAA/header assumptions
   - reject scheduler-required semantics
   - require Molecule schemas for persistent state
   - reject VM ABI trailer packaging

## Recommended External Wording

Use:

> CellScript targets Spora's CKB-VM-compatible Cell execution substrate. Its VM-facing object ABI supports CKB-style Molecule layouts, while Spora-native consensus hashes, DAG header context, and scheduler metadata remain protocol-specific.

Avoid:

> CellScript is fully CKB compatible.

More precise:

> CellScript supports a gated CKB target profile for a constrained source subset, but Spora-native artifacts are not CKB artifacts unless compiled and packaged with CKB-specific hash, header, syscall, and Molecule rules.
