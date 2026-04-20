# CellScript CKB Compatibility and Molecule Decision

**Date**: 2026-04-18
**Status**: V1 bounded compatibility decision
**Scope**: CellScript compiler, Spora VM ABI, CKB-style Cell compatibility, Molecule serialization boundaries

## Existing Documents

There are already related documents, but they answer adjacent questions rather than this exact decision:

- [`exec/src/serialization/README.md`](../exec/src/serialization/README.md) documents the execution-layer serialization split: consensus hashes are custom streaming Blake3, storage remains Borsh with version envelopes, and VM-facing objects can use Molecule ABI `0x8001`.
- [`docs/SERIALIZATION_LAYER_GOVERNANCE_MIGRATION_PLAN.md`](./SERIALIZATION_LAYER_GOVERNANCE_MIGRATION_PLAN.md) captures the original migration plan and the reason not to switch the whole stack to Molecule.
- [`docs/CELLSCRIPT_COMPATIBILITY_MATRIX.md`](./CELLSCRIPT_COMPATIBILITY_MATRIX.md) tracks implementation reality for CellScript, including current CKB-VM-compatible and fail-closed boundaries.
- [`docs/CKB_TO_SPORA_MAPPING.md`](./CKB_TO_SPORA_MAPPING.md) maps CKB primitives to Spora primitives, but it is a broad migration map rather than a CellScript design decision.
- [`docs/cell_ckb_comparison_audit.md`](./cell_ckb_comparison_audit.md) compares Spora Cell/CellTx design with CKB at the data-model level.
- [`docs/CELLSCRIPT_V1_RELEASE_SCOPE.md`](./CELLSCRIPT_V1_RELEASE_SCOPE.md) and [`docs/CELLSCRIPT_V1_FEATURE_COMPLETENESS_AUDIT.md`](./CELLSCRIPT_V1_FEATURE_COMPLETENESS_AUDIT.md) define the bounded v1 promise and the features that remain post-v1.

This document centralizes the decision:

1. Whether CellScript serialization should support Molecule.
2. Whether CellScript can be CKB-compatible as designed, or needs Spora-specific design.

## Decision Summary

CellScript should support Molecule at the **VM-facing object ABI, transaction-carried CellScript ABI, and persistent public schema boundary**. This requirement applies to **Spora as well as CKB**: Spora-native artifacts should launch with Molecule for VM-loaded objects and public CellScript bytes, while still keeping Spora hash domains, DAG header semantics, scheduler policy, and artifact packaging where they intentionally differ from CKB. Because Spora has not launched yet, the project should avoid carrying public Borsh compatibility debt into launch.

CellScript must support **both Spora and CKB** as explicit target profiles:

- `spora` remains a first-class native target with Spora hash domains, DAG header semantics, scheduler metadata, and Spora runtime extensions.
- `ckb` is a first-class CKB-targeted artifact profile for the admitted pure subset, with CKB syscall numbers, CKB Molecule bytes, CKB Blake2b hash domains, CKB header/time assumptions, and no Spora-only packaging or syscalls.
- `portable-cell` is a source subset, not a runtime. It exists so one source package can avoid target-specific assumptions and then be compiled separately to `spora` and `ckb`.

This is a dual-target strategy, not a migration from Spora semantics to CKB semantics. Spora-native behavior must remain supported while the CKB profile is added.

CellScript should target a **CKB-compatible substrate** for shared VM and Cell concepts, but CKB compatibility claims are bounded to artifacts explicitly compiled with the `ckb` profile and admitted by the profile gates. Full arbitrary CKB contract compatibility remains post-v1 work.

Molecule support and CKB support should be delivered as one prelaunch compatibility track:

- public CellScript byte surfaces launch on Molecule;
- Spora VM-facing objects and CellScript public/stateful schema bytes also launch on Molecule;
- `spora`, `ckb`, and `portable-cell` target profiles are introduced before launch;
- CKB support is a separate compiled artifact/profile, not a promise that one Spora artifact runs unchanged on CKB;
- Spora support and CKB support must be maintained in parallel, with profile-specific lowering where their semantics differ;
- any incomplete CKB profile should remain gated/experimental until the policy checks and byte-layout tests pass, rather than becoming a post-launch ABI migration.

The intended split is:

| Area | Decision |
|---|---|
| VM-loaded Cell/CKB-style objects | Launch with Molecule for both `spora` and `ckb`. This includes `Script`, `OutPoint`, `CellInput`, `CellOutput`, `ResolvedCell`, and target-specific header objects where visible to scripts. |
| User-defined persistent CellScript cell data | Launch public/stateful contract data with generated Molecule schemas. Current compiler metadata now emits self-contained Molecule `fixed-struct-v1` schemas for fixed-width layouts, including nested fixed structs plus fixed tuple and array-of-tuple aggregate fields; dynamic and versioned layouts still fail portability policy. |
| Scheduler witness | Migrate before launch to one canonical Molecule witness format if the bytes are transaction-carried or externally parsed. Do not publish a Borsh legacy scheduler witness ABI. |
| Compile metadata sidecar | Keep JSON. It is an audit/tooling surface, not VM object bytes. |
| Internal storage and node bookkeeping | Keep Borsh plus version envelope unless a concrete ABI consumer needs Molecule. |
| TxID, script hash, sighash | Molecule object bytes do not imply CKB hash domains. Spora keeps domain-separated Blake3 paths unless explicitly migrated; `ckb` uses CKB Blake2b over canonical Molecule packed bytes where CKB requires it. |

Short form:

> Launch public CellScript ABI on Molecule. Keep Borsh only for private implementation details that never become transaction-carried or public cross-language contract ABI.

Dual-target short form:

> One source language, two production artifact profiles. `spora` and `ckb` share the portable Cell/CKB-VM subset, but each profile owns its own hash domain, syscall table, header/time model, artifact packaging, and runtime policy.

Version fields are still allowed. Their purpose is future schema evolution after launch, not prelaunch Borsh/Molecule dual-format compatibility.

## Generic and Template Boundary

This compatibility decision is also the source of truth for CellScript's v1 generic boundary:

- User-defined generics are not part of the v1 executable language core.
- Missing monomorphization for syntax such as `resource Vault<T>` or `Vault<Token>` is not a v1 contradiction; that syntax belongs to post-v1 package/codegen/template tooling.
- Template tooling may generate specialized `.cell` modules, but the generated source must contain concrete `resource`, `shared`, `receipt`, and `struct` schemas.
- Every generated persistent/stateful schema that reaches a public byte boundary must have a generated or declared Molecule layout before launch.
- `Vec<T>` remains a controlled builtin collection notation for local bounded APIs and compiler/runtime metadata; it is not evidence of a general user-defined generic type system and must not be used to justify generic persisted schemas.
- Audits should classify user-defined generics as **N/A for v1 core / post-v1 tooling**, while tracking generated Molecule schemas and schema evolution as the real launch-compatibility work.

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

- `exec/src/serialization/molecule_compat.rs` implements CKB-style Molecule wire layouts for VM-facing values and CKB transaction hash material helpers.
- `VmAbiNegotiator` exposes Molecule ABI version `0x8001`.
- `LOAD_SCRIPT`, `LOAD_INPUT`, `LOAD_CELL`, and `LOAD_HEADER` can select `VmAbiFormat::Molecule`.
- CellScript compile metadata declares `runtime.vm_abi.format = "molecule"` and `runtime.vm_abi.version = 0x8001`.
- CellScript compile metadata schema v26 also declares `target_profile` fields for `target_chain`, `vm_abi`, `hash_domain`, `syscall_set`, `artifact_packaging`, `header_abi`, and `scheduler_abi`, plus generated Molecule schema metadata for fixed-width CellScript types.
- CellScript ELF artifacts embed a fixed VM ABI trailer so Spora's verifier can strip the trailer and select the declared ABI before CKB-VM execution.
- CellScript compile metadata now emits only `scheduler_witness_hex` plus `scheduler_witness_abi = "molecule"` for the launch/public scheduler witness bytes. New metadata does not emit `scheduler_witness_molecule_hex` or `scheduler_witness_borsh_hex`; public default consumers reject legacy Borsh fields, reject conflicting Molecule aliases, and accept the old Molecule alias only as a migration read path when the launch field is absent. Legacy Borsh bytes require explicit transition tooling.

The key distinction is that Spora's current consensus hashes are not CKB hashes:

- Spora `Script::hash()` uses domain-separated Blake3 over explicit fields.
- Spora txid/wtxid/sighash use custom streaming Blake3 domains.
- CKB uses Blake2b over canonical packed Molecule bytes for the corresponding CKB objects. Spora now has low-level helpers for CKB script hash, cell data hash, raw transaction hash, transaction witness hash, typed CKB `WitnessArgs`, zeroed-lock `SIGHASH_ALL` message material, CKB Blake160 pubkey hashes, default secp256k1-blake160-sighash-all witness signing plus explicit input and discovered lock-group witness placement, explicit wallet pending-transaction entry points for those CKB signing and verification rules, local recoverable-signature verification against CKB Blake160 lock args, local zeroed-lock CKB sighash-all witness verification, CKB `RawHeader` / `Header` Molecule bytes plus pow/header hash helpers, and CKB `EpochNumberWithFraction` field parsing for header epoch field calculations. `VmSemantics::CkbStrict` now threads the CKB script/data/transaction hash material into VM `LOAD_TX`, `LOAD_SCRIPT_HASH`, `LOAD_CELL_BY_FIELD` hash fields, script-group hash keys, and `LOAD_HEADER` / `LOAD_HEADER_BY_FIELD` when the provider supplies CKB header objects; the source-level CKB epoch and input-since helper subset now lowers explicitly, wallet generation can carry explicit header deps, while automatic stateful CellScript lowering, standard-lock policy integration, profile-selected wallet flows, and real CKB header provider resolution still need full end-to-end wiring.

This is an intentional protocol difference, not a missing serializer.

## 2026-04-19 V1 CKB Audit Update

The current implementation has a real CKB artifact profile for the pure admitted CellScript subset. It is still not full arbitrary CKB contract compatibility. The audit compared the current CellScript/Spora code against the parent CKB repository at `../ckb`.

The compatibility bar used here is strict:

> A `ckb` CellScript artifact must be accepted by CKB's verifier/syscall/hash/Molecule rules without relying on Spora node behavior.

Under that bar, the original P0 blockers are closed for the v1 admitted subset by implementation or by fail-closed policy rejection. The following items are the remaining post-v1 expansion criteria before `ckb` can admit broader stateful features.

| Area | Current implementation | CKB baseline | Required change |
|---|---|---|---|
| Target profile | `--target-profile ckb` can now produce artifacts for the pure supported subset. Stateful and Spora-specific features are rejected by target-profile policy before CKB artifact codegen. `portable-cell` remains source-check-only. | CKB-target artifacts must be emitted with CKB packaging, hashes, syscall table, and ABI assumptions. | Continue threading `TargetProfile` into remaining runtime, transaction, hash, type-id, and schema paths. Keep `spora` artifact generation unchanged. |
| `LOAD_SCRIPT` syscall number | CellScript `ckb` stdlib emits `2052`, while `spora` and legacy fixtures keep `2075`. `VmSemantics::CkbStrict` now handles `LOAD_SCRIPT = 2052` and does not accept the Spora `2075` number; `SporaExtended` keeps the inverse behavior. | Parent CKB defines `LOAD_SCRIPT_SYSCALL_NUMBER = 2052`. | Keep the profile-specific syscall table boundary explicit while auditing remaining syscall numbers. `spora` may keep its current number if that is a Spora protocol decision; `ckb` must not rely on `2075`. |
| Spora-only syscalls | CellScript/profile policy rejects CKB artifacts that require `BLAKE3_HASH = 3001`, `SECP256K1_VERIFY = 3002`, or signature-hash helpers `3003/3004`; `VmSemantics::CkbStrict` no longer registers those Spora extension syscalls. Spora still exposes them under `SporaExtended`. Low-level and wallet CKB signing helpers now exist for zeroed-lock `WitnessArgs`, Blake160 pubkey hashes, default secp256k1-blake160-sighash-all witness signing plus input and discovered lock-group witness placement, recoverable signature verification, local zeroed-lock sighash-all witness verification, and a chain-supplied standard-lock config that builds the CKB type-hash lock script plus dep-group `CellDep`. These are still explicit builder APIs, not automatic CellScript CKB artifact lowering. | These are not CKB syscalls. CKB standard locks use deployed script code/cell deps and CKB signing conventions. | Replace any admitted `ckb` signing/verifier path with CKB-compatible lock scripts/deps or script-local code, not Spora syscalls. |
| Source enum encoding | Spora accepts legacy group values such as `0x0100` / `0x0200`; `ckb` codegen emits canonical CKB source values, and `VmSemantics::CkbStrict` now rejects legacy group encodings. | CKB group sources use `SOURCE_GROUP_FLAG = 0x0100_0000_0000_0000` OR'd with source entry `1..4`. | Keep strict CKB runtime parsing on canonical source values only. `spora` can keep compatibility shims if desired. |
| Header/input time ABI | Spora `LOAD_HEADER_BY_FIELD field=0` is DAA score in the current runtime helper. `VmSemantics::CkbStrict` does not fall back to Spora DAG headers and now uses CKB field meanings 0/1/2 when the provider supplies CKB headers. CellScript now has explicit `ckb::header_epoch_number()`, `ckb::header_epoch_start_block_number()`, `ckb::header_epoch_length()`, and `ckb::input_since()` APIs that are accepted only under the `ckb` profile; `env::current_daa_score()` remains rejected for `ckb`. | CKB `HeaderField::EpochNumber = 0`, `EpochStartBlockNumber = 1`, `EpochLength = 2`; CKB `InputField::Since = 1` via `LOAD_INPUT_BY_FIELD` over `Source::GroupInput`. | Keep DAA and CKB epoch/since APIs separate. `portable-cell` must still reject target-specific clock/header/input APIs unless they are passed as explicit parameters. |
| Header object layout | Spora `ResolvedHeader` is a DAG header table with parent levels, DAA score, cell roots, blue work, etc.; strict CKB runtime no longer exposes those bytes as if they were CKB headers. CKB `RawHeader` / `Header` Molecule bytes, pow/header hash helpers, epoch field helpers, strict `LOAD_HEADER` provider dispatch, source-level epoch helper lowering, and wallet generator `header_deps` propagation now exist. | CKB `Header` is the canonical Molecule `RawHeader + nonce` layout, and scripts can only read headers that are present in transaction `header_deps`. | Keep separate `spora` and `ckb` header ABI serializers/loaders. Wire real CKB header provider resolution before admitting source features that depend on real chain header availability beyond the explicit epoch helper subset. |
| Hash domains | Spora script hash, txid, wtxid, data hash, header hash, and sighash use Blake3/domain-separated or Spora-native material. CKB Blake2b helpers now exist for packed Molecule `Script`, empty-special-cased cell data hash, packed `RawTransaction` tx hash, packed `Transaction` witness hash, typed `WitnessArgs`, zeroed-lock `SIGHASH_ALL` message material, CKB Blake160 pubkey hashes, default secp256k1-blake160-sighash-all witness signing/verification plus input and discovered lock-group witness placement, and packed CKB `RawHeader` / `Header` pow/header hashes. `CkbStrict` VM semantics use CKB hash material for `LOAD_TX`, `LOAD_SCRIPT_HASH`, `LOAD_CELL_BY_FIELD` data/lock/type hash fields, and script-group keys. | CKB uses Blake2b over packed Molecule bytes for script/tx/data/header/signing material. | Thread the remaining CKB helper layer into wallet/transaction builders, standard-lock policy integration, CKB header APIs, and CellScript profile selection. Keep Spora Blake3/native mode for `spora`. |
| Artifact packaging | Spora ELF artifacts append `SPORABI` trailer so Spora verifier can strip it and select VM ABI. | CKB loads code cell bytes directly and will not strip a Spora trailer. | `ckb` ELF/code bytes must have no `SPORABI` trailer. `spora` may keep the trailer. |
| DepGroup bytes | Spora DepGroup helper uses `u32 count + 36-byte OutPoint[]` and permits empty lists. A CKB `OutPointVec` helper now exists at the CellTx helper layer and rejects empty DepGroups while preserving Spora's existing empty-list behavior. Consensus validation now has an explicit `DepGroupDataAbi` selector: Spora defaults to the existing ABI, while CKB-targeted validation can select `CkbMolecule` and fail empty groups. Native wallet transaction generator settings and the WASM generator `cellDeps` option can now carry explicit `CellDep` values, including CKB `DepType::DepGroup` references; the referenced dep-group cell data must still be encoded with `encode_ckb_dep_group_data`. | CKB DepGroup cell data is Molecule `OutPointVec`; empty dep groups are invalid. | Wire profile-selected standard deps into CKB transaction/dependency builders. Preserve Spora format by profile decision. |
| Type ID | CellScript's current stable type identity remains metadata/type-name oriented under Spora, with Blake3-derived internal type hashes. CKB TYPE_ID creation-args, built-in TYPE_ID script construction, output-args builder, full output type-script installer, explicit script-group verifier, resolved-cell group discovery verifier helpers, native/WASM wallet pending-transaction entry points, and wallet generator final-output index installation now exist at the low-level transaction/Molecule layer. Under the `ckb` profile, persistent Cell types with `#[type_id("...")]` now emit a `types[].ckb_type_id` lowering contract that names the built-in `TYPE_ID_CODE_HASH`, `hash_type = Type`, first-input/output-index args rule, builder helper, and verifier helper. Metadata schema v26 also emits `actions[].create_set[].ckb_type_id` / `locks[].create_set[].ckb_type_id` output plans for direct `create` outputs of those types, including the concrete Output index and wallet setting names; native wallet settings can consume those action plans through profile-aware `with_cellscript_action_metadata_json`, and the WASM generator can consume `cellscriptMetadata` plus `cellscriptAction` without depending on the CellScript crate. Metadata-only struct IDs and portable metadata-only IDs remain rejected. | CKB TYPE_ID has a specific built-in rule: `TYPE_ID_CODE_HASH` with `hash_type = Type`, one 32-byte arg, at most one in/out in the TYPE_ID group, and creation arg is Blake2b(first input + output index). | Wire higher-level CellScript builders to pass metadata/action/deps into the wallet generator automatically; keep Spora metadata IDs separate. |
| Scheduler witness | CellScript now emits Molecule scheduler witness bytes for Spora launch/public metadata, while legacy Borsh decode remains an old-sidecar migration path. The witness remains Spora MPE-specific. | CKB has no Spora scheduler witness validity path. | `ckb` artifacts must not require scheduler witness validity. Spora consumers should use the format-neutral launch field and keep legacy decode out of new production output. |
| Stateful lowering | Several stateful paths still depend on metadata obligations, fail-closed assembly, or restricted fixed-field verifier coverage. | A CKB contract must enforce its validity entirely through transaction scripts and CKB consensus rules. | Complete executable lowering for every stateful feature admitted by `ckb`, or reject that feature in `ckb`/`portable-cell`. |

Immediate conclusion:

- `spora` can continue as the default native profile.
- `ckb` can produce v1 artifacts for the currently supported pure subset; broader blockers above remain fail-closed for CKB until implemented.
- `portable-cell` should stay conservative: it can pass only source that avoids target-specific clocks, hash assumptions, scheduler semantics, Spora-only syscalls, and unsupported persistent schema layouts.

This audit does not reduce Spora support. It clarifies that CKB compatibility must be implemented as a parallel profile with its own concrete ABI and policy.

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
| `ResolvedHeader` | Spora headers include DAG-specific fields such as parent levels, DAA score, and Spora roots. CKB header ABI is different; low-level CKB `RawHeader` / `Header` helpers exist but are not yet runtime syscalls. |
| Type identity | Current `#[type_id("...")]` is metadata-level stable identity under Spora. Under `ckb`, persistent Cell types now emit a `types[].ckb_type_id` contract for the CKB built-in TYPE_ID script, creation-args, builder, and verifier helpers. Direct `create` outputs now carry `create_set[].ckb_type_id` output plans; native and WASM wallet generator settings can consume action metadata in a profile-aware way, installing CKB TYPE_ID scripts for CKB while keeping Spora scheduler witness handling separate. WASM generator settings also expose explicit CKB `cellDeps` so standard deps can be passed without native-only glue. Higher-level CellScript transaction builders still need to pass metadata/action/deps automatically. |
| Pool/shared-state scheduling | CellScript's Pool and shared-state patterns are designed for Spora's parallel execution roadmap. |
| Native helper syscalls | Spora-specific syscalls or syscall numbers must be excluded or shimmed for a real CKB target. |

### Time Basis / Chain Clock Semantics

CellScript must not treat Spora DAA score as a portable chain clock.

`env::current_daa_score()` is a Spora-native time/ordering primitive. It depends on Spora DAG header semantics and lowers through the current Spora header ABI. CKB has different consensus time surfaces such as `since`, epoch-oriented locks, block/header fields, and transaction header dependencies. These are not a unit conversion of DAA score, so the compiler must not rewrite DAA predicates into CKB predicates automatically.

Policy:

- `spora` may use `env::current_daa_score()` and Spora DAG header fields.
- `ckb` must reject `env::current_daa_score()` until a CKB-specific clock/header API and lowering are designed.
- `portable-cell` must also reject `env::current_daa_score()`, because portable source must avoid target-specific chain-clock assumptions.
- Portable application logic that needs time should accept an explicit `u64`/domain-specific parameter and bind that parameter off-chain or in a target-specific wrapper.
- CKB support now includes a minimal `ckb::input_since()` helper for the current script group's first input. Future broader APIs such as indexed `ckb::since`, `ckb::header_timestamp`, or `ckb::block_number` should be added only with explicit ABI, header-dep, and consensus-validity rules.

## Design Answer

CellScript can be **source-level portable for a constrained common subset**, but a single compiled artifact should not be treated as both Spora-compatible and CKB-compatible.

The right model is target profiles:

| Profile | Meaning |
|---|---|
| `spora` | Current native target. Uses Spora CellTx, Spora hash domains, Spora DAG header context, Spora scheduler metadata, Molecule VM/public CellScript ABI, and optional VM ABI trailer. |
| `ckb` | Prelaunch gated portability target. Uses CKB transaction/header assumptions, CKB packed Molecule bytes, CKB-compatible hash domains, no Spora ABI trailer, no Spora MPE scheduler witness requirements, and only CKB-supported syscalls. |
| `portable-cell` | A source-level subset that avoids Spora-only features and can be compiled separately for `spora` or `ckb`. |

This implies CellScript should not try to make every feature CKB-compatible. It should isolate the CKB-compatible substrate and mark Spora-native features explicitly.

## Molecule Scope

### Must Support Molecule

This is not a CKB-only requirement. Spora must support Molecule at the VM and CellScript public byte layers so launched Spora contracts have stable, cross-language ABI bytes. The CKB profile additionally requires CKB's exact packed Molecule layouts and Blake2b hash domains where CKB consensus/script rules depend on them.

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
3. Public CellScript contract state schemas for both `spora` and `ckb` when the data is intended to be:
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
- Legacy Borsh witness generation can exist only as an explicit private migration/debug step.
- The launch/public metadata fields are `scheduler_witness_hex` and `scheduler_witness_abi = "molecule"`; format-specific aliases, including `scheduler_witness_molecule_hex` and legacy `scheduler_witness_borsh_hex`, are hidden from new compiler output. Public default consumers reject legacy Borsh fields and conflicting Molecule aliases; the old Molecule alias remains a migration read path only when the launch field is absent.
- A version byte or schema version may remain, but it identifies the launch schema and future post-launch evolution, not a prelaunch Borsh legacy format.

The legacy Borsh scheduler witness is acceptable only as temporary implementation state because it is:

- explicitly versioned by magic/version
- decoded through a specific admission path
- checked against concrete transaction source bounds
- optionally checked against trusted compiler/builder summaries
- scoped to Spora scheduling policy, not CKB chain object encoding

Before launch, make the Molecule scheduler witness the only public format instead of publishing both formats.

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
- Metadata-only `type_id("...")` assumptions unless mapped to a real CKB type-id lowering contract and builder/verifier path.
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

- Keep `scheduler_witness_hex` plus `scheduler_witness_abi = "molecule"` as the launch field; new compiler output no longer publishes `scheduler_witness_molecule_hex` or `scheduler_witness_borsh_hex`.
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
| `spora` | Spora hash domains, Spora DAG headers, Molecule VM ABI, Molecule public CellScript schemas, Spora scheduler witness, optional `SPORABI` trailer. |
| `ckb` | CKB Molecule objects, CKB hash semantics, CKB header/syscall assumptions, no `SPORABI` trailer, no Spora scheduler-required validity. |
| `portable-cell` | Source subset that avoids Spora-only semantics and can be compiled separately to `spora` or `ckb`. |

Exit gate: compiler metadata can clearly say whether an artifact is Spora-native, CKB-targeted, or only source-portable.

Current implementation status (2026-04-19): Phase D is implemented for metadata and profile selection. `cellc` emits schema v26 `target_profile` metadata and accepts `--target-profile` / `[build].target_profile`. `spora` is the default native profile, still emits the Spora ABI trailer for ELF, and still declares Molecule VM ABI metadata. `ckb` can produce artifacts for the pure supported subset and emits CKB profile metadata with no Spora ELF trailer. `portable-cell` remains a source compatibility profile and does not directly produce artifacts.

Dual-support requirement: as `ckb` artifact production expands beyond the pure subset, it must not remove or weaken `spora`. The compiler must carry profile-specific lowering choices instead of globally changing Spora to match CKB.

### Phase E: Add CKB Compatibility Policy Gates

Add lint/check gates for `portable-cell` and `ckb` targets:

- reject Spora-only syscalls
- reject DAA/header assumptions not expressible on CKB
- reject scheduler-required semantics
- require Molecule schemas for persistent state
- reject `SPORABI` trailer packaging for CKB target
- reject Spora hash-domain assumptions when producing CKB-target artifacts
- reject metadata-only `type_id("...")` when no profile-specific CKB type-id lowering contract exists

Exit gate: a source file can be mechanically classified as Spora-only, CKB-targetable, or portable-subset.

Current implementation status (2026-04-19): Phase E is implemented for v1 classification and pure-subset admission. `check --target-profile ckb` compiles through the CKB profile path, while `check --target-profile portable-cell` still compiles through Spora lowering and applies source portability classification. Pure portable source can pass the check gate, and fixed-width persistent Cell types can pass when compiler metadata includes generated Molecule `fixed-struct-v1` schemas. The gate still rejects metadata evidence for symbolic/fail-closed/runtime-required verifier obligations, runtime-required transaction inputs, persistent Cell types whose layouts cannot yet generate Molecule schemas, metadata-only `type_id` declarations that lack a profile-specific CKB TYPE_ID lowering contract, Spora shared-state scheduler touch domains, pool-pattern scheduler/admission metadata, DAA/header assumptions for both `ckb` and `portable-cell`, and Spora-only claim helper syscall features.

### Phase F: Test Against CKB Baseline

Add byte-level compatibility tests against CKB's Molecule layouts:

- `Script`
- `OutPoint`
- `CellInput`
- `CellOutput`
- `CellDep`
- `RawTransaction`
- `Transaction`
- CKB script/data/raw transaction/transaction witness/`WitnessArgs`/zeroed-lock sighash/Blake160/standard witness signing/header hash material
- scheduler witness generated bytes
- generated persistent cell data schemas

For the `ckb` target, add tests that verify:

- no Spora ABI trailer
- no Spora-only syscall use
- no Spora DAG header dependency
- CKB-compatible hash/material selection wired into the profile builders and verifier paths
- CKB-compatible Molecule object layout
- CKB `LOAD_SCRIPT = 2052`
- canonical CKB group source encoding
- CKB header field numbers and Molecule `Header`
- CKB `OutPointVec` DepGroup bytes
- CKB TYPE_ID creation/continuation behavior where CellScript exposes type-id semantics

Exit gate: the compatibility claim is backed by generated schema tests, not by manual struct similarity.

Current implementation status (2026-04-19): Phase F is implemented for the v1 pure subset covered by the release gate. The suite proves CKB no-trailer ELF output, `LOAD_SCRIPT = 2052`, canonical CKB source values, strict rejection of Spora-only helper syscalls, CKB Molecule transaction/header/hash/signing helper material, CKB `OutPointVec` DepGroup validation, generated fixed-width Molecule schema metadata, and CKB TYPE_ID metadata/output-plan surfaces. Broader stateful CKB verifier flows remain post-v1 and must add matching byte-level tests when admitted.

### Phase G: Implement Dual Artifact Profiles

Once the profile gates and byte tests exist, make both artifact profiles real:

| Compiler surface | `spora` behavior | `ckb` behavior |
|---|---|---|
| Artifact packaging | Spora ELF with optional `SPORABI` trailer and metadata ABI negotiation; VM/public CellScript bytes still use Molecule. | CKB-compatible ELF/code cell bytes with no Spora trailer. |
| Syscall table | Spora's current table, including Spora extensions where intentionally supported. | Parent CKB syscall numbers and no Spora-only syscall dependencies. |
| Hash mode | Domain-separated Blake3 for Spora tx/script/data/signing material. | CKB Blake2b over canonical Molecule packed bytes. |
| Header/time model | Spora DAG header and DAA APIs. | CKB header/epoch/since APIs only. |
| Scheduler metadata | Spora scheduler witness and MPE access summaries, with transaction-carried/public witness bytes migrated to Molecule before launch. | No scheduler witness as a validity requirement. |
| Type identity | Spora metadata/type identity rules. | CKB TYPE_ID or explicit CKB-compatible type script rules. |
| Persistent schema | Generated Molecule schema with Spora policy. | Generated Molecule schema compatible with CKB cell data and script decoding. |

Exit gate: `cellc build --target-profile spora` and `cellc build --target-profile ckb` can both produce artifacts for the supported subset, and the test suite proves that one profile's ABI choices do not leak into the other.

Current implementation status (2026-04-19): Phase G is implemented for the v1 pure subset. Implemented pieces are profile-aware codegen options, Spora-default Molecule VM ABI metadata, public `VmSerializable` and full-load syscall defaults on Molecule with explicit legacy Borsh/custom v1 only, CellScript `scheduler_witness_hex` + `scheduler_witness_abi = "molecule"` launch metadata without newly emitted Borsh scheduler sidecars or Molecule alias sidecars, public ActionMetadata and wallet metadata paths that reject legacy Borsh scheduler fields and conflicting Molecule aliases, generated Molecule `fixed-struct-v1` schema metadata for fixed-width, nested fixed, and fixed tuple/array-of-tuple CellScript layouts, execution-layer explicit legacy decode for old migration inputs, CKB canonical group source constants plus strict runtime rejection of legacy group source encodings, CKB stdlib `LOAD_SCRIPT = 2052`, `CkbStrict` VM dispatch for `LOAD_SCRIPT = 2052` while preserving Spora `2075` under `SporaExtended`, CKB ELF packaging without `SPORABI`, profile-aware VM ABI trailer validation, CKB Blake2b script hash over packed Molecule `Script`, CKB Molecule `CellDep` / `RawTransaction` / `Transaction` / `WitnessArgs` / `RawHeader` / `Header` serialization helpers, CKB cell data hash, raw transaction hash, transaction witness hash, raw-header pow hash, header hash, CKB `EpochNumberWithFraction` packed field helpers, zeroed-lock `SIGHASH_ALL` message material helpers, CKB Blake160 pubkey hash helpers, default secp256k1-blake160-sighash-all witness signing plus explicit input and discovered lock-group placement helpers, chain-supplied CKB standard-lock config helpers for type-hash lock scripts and dep-group `CellDep`s, wallet pending-transaction CKB signing / configured-lock signing / lock-group verification entry points, native wallet generator explicit `CellDep` / CKB DepGroup reference injection plus explicit `header_deps` propagation, WASM generator explicit `cellDeps` / `headerDeps` propagation, wallet generator explicit CKB TYPE_ID final-output installation by native `with_ckb_type_id_output_indexes` / WASM `ckbTypeIdOutputs`, profile-aware native/WASM wallet action metadata consumption for Spora scheduler witnesses and CKB TYPE_ID final-output indexes, local CKB Blake160 recoverable-signature verification helpers, local zeroed-lock CKB sighash-all witness verification helpers, `CkbStrict` VM wiring for `LOAD_TX`, `LOAD_SCRIPT_HASH`, `LOAD_CELL_BY_FIELD` hash fields, script-group hash keys, and profile-correct `LOAD_HEADER` / `LOAD_HEADER_BY_FIELD` over provider-supplied CKB headers, source-level CellScript CKB epoch helper lowering for epoch number/start/length and `ckb::input_since()` lowering through `LOAD_INPUT_BY_FIELD Source::GroupInput`, `CkbStrict` omission of Spora-only helper syscalls `3001..3004`, no-fallback behavior for Spora DAG header loads under CKB strict mode, explicit Spora-vs-CKB DepGroup helper APIs and consensus validation selection with CKB Molecule `OutPointVec` non-empty validation, CKB TYPE_ID creation-args hash material, built-in script construction, output-args builder, full output type-script installer, explicit and resolved-cell script-group verifier helpers, native/WASM wallet pending-transaction CKB TYPE_ID args and full type-script entry points, `types[].ckb_type_id` lowering-contract metadata plus `create_set[].ckb_type_id` direct-create output plans for CKB persistent Cell types that declare `#[type_id("...")]`, and fail-closed CKB policy before artifact codegen for non-portable features. Remaining Phase G work is post-v1 expansion: wiring those CKB signing/dependency/type-id helpers into higher-level CellScript CKB artifact and transaction flows, passing emitted CKB TYPE_ID output plans plus standard deps automatically into builders, profile-selected standard dep/depgroups in CKB builders, real CKB header provider resolution beyond explicit transaction header deps and the epoch/input-since helper subset, dynamic/versioned persistent schema coverage beyond the fixed-struct subset, eventual removal of old-sidecar read aliases after migration, and executable stateful lowering.

## Required Follow-Up Work

The remaining post-v1 tasks implied by the plan are:

1. Keep the scheduler witness migration closed for the public path: `scheduler_witness_hex` with `scheduler_witness_abi = "molecule"` remains the launch/public field, new compiler output stays free of format-specific aliases, and old Molecule/Borsh sidecar fields stay confined to explicit migration tooling until removed.
2. Add generated `.mol` schemas for Spora VM ABI objects instead of relying indefinitely on handwritten Molecule layout code.
3. Extend the CellScript schema generator beyond fixed-width/nested/tuple-aggregate `fixed-struct-v1` layouts to dynamic, table/dynvec, and evolution/versioned schemas.
4. Continue the target-profile compatibility track after v1:
   - `target_chain`
   - `vm_abi`
   - `hash_domain`
   - `syscall_set`
   - `artifact_packaging`
   - `header_abi`
   - `scheduler_abi`
   - `check --target-profile ckb|portable-cell` classification gates for portability blockers
5. Implement profile-specific syscall lowering:
   - `spora` keeps the current Spora syscall table unless separately migrated.
   - `ckb` uses parent CKB syscall numbers, including `LOAD_SCRIPT = 2052`.
   - `ckb` emits canonical CKB source values, including the high-bit group source flag.
   - Current status: CellScript `ckb` emits `LOAD_SCRIPT = 2052`, `CkbStrict` VM runtime dispatches that number and rejects the Spora `2075` path, CKB profile policy rejects Spora-only `3001..3004` helper features, `CkbStrict` VM runtime does not register those helper syscalls, strict source parsing rejects legacy Spora group source values such as `0x0100`, and wallet/builders can now use explicit CKB standard-lock config/deps rather than Spora helper syscalls. Remaining work is automatic CellScript/profile lowering to those CKB-compatible script code/deps and continued audit of the non-load syscall surface.
6. Implement profile-specific hash and signing material:
   - `spora` keeps domain-separated Blake3.
   - `ckb` uses Blake2b over canonical Molecule packed bytes for script hash, tx hash, data hash, and sighash.
   - Current status: low-level CKB helpers exist for script hash, empty-special-cased cell data hash, raw transaction hash, transaction witness hash, typed `WitnessArgs`, zeroed-lock `SIGHASH_ALL` message material, CKB Blake160 pubkey hashes, default secp256k1-blake160-sighash-all witness signing plus explicit input and discovered lock-group witness placement, chain-supplied standard-lock config helpers, wallet pending-transaction CKB signing/configured-lock signing and lock-group verification entry points, native wallet generator `CellDep` / `header_deps` propagation, WASM generator `cellDeps` / `headerDeps` propagation, local recoverable-signature verification against CKB Blake160 lock args, local zeroed-lock CKB sighash-all witness verification, and CKB `RawHeader` / `Header` pow/header hashes; `CkbStrict` VM wiring now uses the tx/script/cell/header/input hash and field material visible through syscalls and script-grouping, and CellScript has explicit CKB epoch plus input-since helper lowering. Profile-aware wallet settings can now consume CellScript action metadata, while higher-level CellScript builders, real header provider resolution, and broader CellScript profile selection still need the remaining end-to-end integration.
7. Implement profile-specific header/time APIs:
   - `spora` keeps `env::current_daa_score()`.
   - `ckb` rejects DAA APIs and introduces only explicitly specified CKB header/epoch/since APIs.
   - Current status: low-level CKB `RawHeader` / `Header` Molecule bytes, pow/header hash helpers, packed `EpochNumberWithFraction` field helpers, strict VM `LOAD_HEADER` / `LOAD_HEADER_BY_FIELD` dispatch over provider-supplied CKB headers, wallet generator `header_deps` propagation, source-level CellScript CKB epoch APIs for epoch number/start/length, and `ckb::input_since()` over `LOAD_INPUT_BY_FIELD Source::GroupInput` exist. The remaining gap is real CKB header provider resolution and any broader indexed/header APIs beyond this explicit epoch/input-since subset; Spora DAG `ResolvedHeader` is not used as a fallback in CKB strict mode.
   - `portable-cell` rejects target-specific clock APIs unless they are passed as explicit parameters.
8. Extend CKB artifact packaging beyond the v1 pure subset:
   - no `SPORABI` trailer;
   - no Spora scheduler witness requirement;
   - no Spora DAG header dependency;
   - CKB-compatible code cell bytes and dependency model.
9. Implement CKB Molecule DepGroup and TYPE_ID support:
   - CKB DepGroup uses Molecule `OutPointVec`; empty DepGroups are invalid.
   - Current status: CellTx helpers and consensus validation now accept an explicit DepGroup ABI selector. Spora default still accepts existing empty DepGroup bytes; `CkbMolecule` validation rejects empty `OutPointVec`. Native wallet generator settings and WASM `cellDeps` can include explicit `CellDep` values and CKB `DepType::DepGroup` references in generated transactions, while CKB dep-group cell data is still produced by the explicit `encode_ckb_dep_group_data` helper.
   - CKB type-id semantics follow CKB's first-input/output-index lineage rule.
   - Current status: CKB TYPE_ID creation-args, built-in script construction, output-args builder, full output type-script installer, explicit script-group verifier, resolved-cell group discovery verifier helpers, native/WASM wallet pending-transaction entry points, wallet generator final-output index installation, wallet JSON extraction of CellScript action output plans, profile-aware native/WASM action metadata consumption, `types[].ckb_type_id` lowering-contract metadata, and `create_set[].ckb_type_id` direct-create output plans for persistent Cell `#[type_id]` declarations exist; automatic higher-level CellScript transaction-builder invocation remains pending, while metadata-only IDs without a CKB contract stay fail-closed.
10. Extend the gated `ckb` compiler profile beyond the v1 pure subset only when the corresponding CKB policy, builder, and byte-layout tests pass.
11. Keep documentation wording split:
   - "CKB-VM-compatible" for the execution substrate.
   - "CKB-compatible contract" only for an explicit gated restricted target profile.
12. Expand lint/policy gates for `portable-cell` mode:
   - reject Spora-only syscalls
   - reject DAA/header assumptions
   - reject scheduler-required semantics
   - require Molecule schemas for persistent state
   - reject VM ABI trailer packaging
   - keep broad artifact-producing `ckb` features fail-closed until profile-specific byte layout and packaging tests exist

## Recommended External Wording

Use:

> CellScript targets Spora's CKB-VM-compatible Cell execution substrate. Its VM-facing object ABI supports CKB-style Molecule layouts, while Spora-native consensus hashes, DAG header context, and scheduler metadata remain protocol-specific.

Avoid:

> CellScript is fully CKB compatible.

More precise:

> CellScript supports a gated CKB target profile for a constrained source subset, but Spora-native artifacts are not CKB artifacts unless compiled and packaged with CKB-specific hash, header, syscall, and Molecule rules.
