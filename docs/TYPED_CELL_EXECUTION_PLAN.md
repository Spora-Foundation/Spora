# Typed Cell Execution Plan

Branch: `spora-typed`

## Principle

This branch resets scheduler witness semantics around typed cells.

The old `binding_hash` model is removed because it conflates two different concepts:

- stable conflict identity for scheduling;
- mutable state/content commitment for audit and state roots.

Typed cell execution uses two explicit hashes:

```text
conflict_hash
    stable across ordinary data updates
    used by CellDAG conflict detection

typed_data_hash
    changes when typed cell data changes
    used for audit and typed-data commitment
```

No backward-compatibility layer is maintained in this branch.

## Non-Goals

- No backward compatibility with the old `binding_hash` scheduler witness.
- No v1/v2 bridge.
- No aliasing `binding_hash` to `conflict_hash`.
- No deprecated fields.
- No reserved-but-unsupported enum variants.
- No CellScript dependency in the runtime-first phase.
- No BFT, settlement, checkpoint, or exit model in this branch.

## Execution Order

```text
Step 1  Spora runtime: typed cell types, conflict_hash, typed_data_hash, scheduler witness, CellDAG, TypedCellStore, BlockAccessSummary, tests
Step 2  Spora integration tests with hand-crafted typed-cell witnesses
Step 3  CellScript: TargetProfile::TypedCellL2 and typed-cell witness generation
Step 4  End-to-end runtime/compiler integration
```

Rationale: the execution semantics must be validated before the compiler emits them.

---

## Step 1: Spora Runtime — Typed Cell Core

### 1.1 Typed Cell Classification Types

File: `exec/src/celltx/types.rs`

```rust
/// Ownership class — determines parallel execution and access rules
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum CellOwnership {
    Owned,      // one owner, easy to parallelise
    Shared,     // public mutable cell (AMM pool, oracle)
    Party,      // bounded multi-party session
    Immutable,  // read-only after creation
    Ephemeral,  // batch-local intermediate, not admitted to scheduler
}

/// Mutability class — determines state transition pattern
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum CellMutability {
    Linear,     // consume + create
    Versioned,  // consume + create with version field
    AppendOnly, // successor output, data only appends
    Migratable, // explicit data layout migration
}

/// Accounting class — domain constraint on data layout
///
/// Multi-label: Vec<CellAccounting> in TypedCellDecl.
/// E.g. an ExitClaim can be both Receipt + StorageClaim.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum CellAccounting {
    Fungible,
    NonFungible,
    Receipt,
    StorageClaim,  // claim over occupied-capacity-backed L1 storage space (not a token class)
}

/// Identity class — how identity is preserved across updates
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum CellIdentity {
    OutPoint,           // natural OutPoint identity
    TypeId,             // TYPE_ID pattern
    Singleton,          // one-of-a-kind, identified by type_script alone
    Field(String),      // named field as identity key
    Composite(Vec<String>), // composite key from multiple fields
}

/// Settlement class — determines L1 commitment participation
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum CellSettlement {
    L1Settled,       // settles back to CKB L1
    L2Only,          // stays in L2
    RollupCommitted, // committed as part of rollup batch
    ExitClaim,       // exit claim cell
}

/// Conflict key specification — determines how conflict_hash is derived
///
/// Rule: mutable cells must not use ConflictKeySpec::None.
/// None is only valid for Pure / ReadOnly / Ephemeral cells.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum ConflictKeySpec {
    /// Concrete cell identity — default for owned mutable cells
    CellId,
    /// Single field name (e.g. "pool_id")
    Field(String),
    /// Composite key from multiple fields (e.g. ["asset_id", "owner", "shard_id"])
    Composite(Vec<String>),
    /// Owner-level serialisation (explicit coarse opt-in)
    Owner,
    /// No conflict key — only valid for Pure / ReadOnly / Ephemeral
    None,
}

/// Typed cell declaration
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct TypedCellDecl {
    pub ownership: CellOwnership,
    pub mutability: CellMutability,
    /// Accounting labels (multi-label)
    pub accounting: Vec<CellAccounting>,
    pub identity: CellIdentity,
    pub settlement: CellSettlement,
    /// Conflict key specification.
    /// conflict_hash = blake3(domain || full_script_id || conflict_key_value)
    pub conflict_key: ConflictKeySpec,
}
```

### 1.2 Hash Computation

File: `exec/src/celltx/types.rs`

```rust
/// Stable conflict hash.
/// blake3(domain || code_hash || hash_type || args || conflict_key_value)
/// Does NOT change when cell data is updated.
pub fn compute_conflict_hash(type_script: &Script, conflict_key_value: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"spora-typed-cell/conflict-hash/v1");
    hasher.update(&type_script.code_hash);
    hasher.update(&[type_script.hash_type]);
    hasher.update(&type_script.args);
    hasher.update(conflict_key_value);
    *hasher.finalize().as_bytes()
}

/// Typed data hash.
/// blake3(domain || code_hash || hash_type || args || data)
/// Changes with every data update.
/// Named typed_data_hash (not cell_state_hash) because it does NOT
/// include lock/capacity — only type script identity + data.
pub fn compute_typed_data_hash(type_script: &Script, data: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"spora-typed-cell/typed-data-hash/v1");
    hasher.update(&type_script.code_hash);
    hasher.update(&[type_script.hash_type]);
    hasher.update(&type_script.args);
    hasher.update(data);
    *hasher.finalize().as_bytes()
}
```

### 1.3 Scheduler Witness (clean break)

File: `exec/src/celltx/types.rs`

`binding_hash` is removed. The scheduler witness is defined around typed cells.

```rust
pub const TYPED_CELL_SCHEDULER_WITNESS_VERSION: u8 = 1;
pub const TYPED_CELL_SCHEDULER_WITNESS_MAGIC: u16 = 0xCE11;
```

Access record (70 bytes):

```rust
pub struct CellScriptSchedulerAccessWitness {
    pub operation: SchedulerOperation,
    pub source: SchedulerSource,
    pub index: u32,
    pub conflict_hash: [u8; 32],     // stable, from type_script + conflict_key
    pub typed_data_hash: [u8; 32],    // changes with data
}
```

Witness header:

```rust
pub struct CellScriptSchedulerWitness {
    pub magic: u16,
    pub version: u8,
    pub effect_class: SchedulerEffectClass,
    pub parallelizable: bool,
    pub touches_shared_count: u16,
    pub touches_shared: Vec<[u8; 32]>,  // conflict_hash values for shared cells
    pub estimated_cycles: u64,
    pub access_count: u16,
    pub accesses: Vec<CellScriptSchedulerAccessWitness>,
}
```

No `binding_hash`. No v1 decode path. No alias. The old 38-byte record format
is gone from this branch.

### 1.4 CellDAG — conflict_hash + access-mode awareness

File: `exec/src/scheduler/dag.rs`

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessMode {
    Read,   // READ_REF
    Write,  // CONSUME, CREATE, DESTROY, TRANSFER
}

impl AccessMode {
    pub fn from_operation(op: SchedulerOperation) -> Self {
        match op {
            SchedulerOperation::ReadRef => AccessMode::Read,
            SchedulerOperation::Consume
            | SchedulerOperation::Create
            | SchedulerOperation::Destroy
            | SchedulerOperation::Transfer => AccessMode::Write,
        }
    }
}

pub struct ConflictEntry {
    pub node_id: NodeId,
    pub mode: AccessMode,
}

// Added to CellDAG:
pub conflict_hash_conflicts: BTreeMap<[u8; 32], Vec<ConflictEntry>>,
```

Conflict rules:

```text
READ  + READ  same conflict_hash → same layer (no conflict)
READ  + WRITE same conflict_hash → dependency edge
WRITE + WRITE same conflict_hash → dependency edge, therefore different topological layers
```

Phase 1 scheduler operations: CONSUME, CREATE, DESTROY, TRANSFER, READ_REF.
No MUTATE_INPUT / MUTATE_OUTPUT / CLAIM / SETTLE in this branch.

### 1.5 TypedCellStore

File: `exec/src/celltx/types.rs`

```rust
/// Canonical script identity for typed cell registry key.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ScriptId {
    pub code_hash: [u8; 32],
    pub hash_type: u8,
    pub args_hash: [u8; 32],
}

impl ScriptId {
    pub fn from_script(script: &Script) -> Self {
        let args_hash = *blake3::hash(&script.args).as_bytes();
        Self { code_hash: script.code_hash, hash_type: script.hash_type, args_hash }
    }
}

/// Registry of typed cell declarations keyed by full script identity.
pub trait TypedCellStore {
    fn get_decl(&self, type_script: &Script) -> Option<&TypedCellDecl>;
    fn insert_decl(&mut self, type_script: Script, decl: TypedCellDecl);
}

/// In-memory typed cell store.
pub struct InMemoryTypedCellStore {
    decls: BTreeMap<ScriptId, TypedCellDecl>,
}

impl TypedCellStore for InMemoryTypedCellStore {
    fn get_decl(&self, type_script: &Script) -> Option<&TypedCellDecl> {
        let id = ScriptId::from_script(type_script);
        self.decls.get(&id)
    }

    fn insert_decl(&mut self, type_script: Script, decl: TypedCellDecl) {
        let id = ScriptId::from_script(&type_script);
        self.decls.insert(id, decl);
    }
}
```

### 1.6 BlockAccessSummary

File: `consensus/src/pipeline/virtual_processor/access_summary.rs`

Replace `cellscript_shared_reads`/`cellscript_shared_writes` (keyed by `binding_hash`)
with `conflict_hash`-keyed maps.

### 1.7 Tests

Positive:

- `compute_conflict_hash` determinism
- `compute_typed_data_hash` determinism
- Shared Pool: conflict_hash stable across data updates
- Owned fungible: hand-crafted conflict_key_value encodes (asset_id, owner, shard_id)
- Shared cell: conflict_key MUST be explicitly declared (not defaulted to identity)
- `TypedCellDecl` serialization round-trip
- CellDAG: WRITE+WRITE same conflict_hash → dependency
- CellDAG: different conflict_hash → parallel
- CellDAG: READ + READ same conflict_hash → same layer
- CellDAG: READ + WRITE same conflict_hash → dependency
- BlockAccessSummary by conflict_hash

Negative:

- conflict_hash differs when conflict_key_value differs (data same)
- typed_data_hash changes when data changes (conflict_hash unchanged)
- Forged witness fails validate_summary() on conflict_hash mismatch
- Mutable cell with ConflictKeySpec::None is rejected
- Shared mutable cell without explicit conflict_key is rejected

Access-mode:

- Immutable config read by many txs → same layer
- Shared pool read-only quotes → same layer
- Shared pool swaps → dependency
- Quote + swap same pool → dependency

---

## Step 2: Integration Tests with Hand-Crafted Witnesses

No CellScript dependency. Build typed-cell witnesses manually in test code:

```rust
#[test]
fn typed_cell_witness_round_trip() {
    let type_script = Script::new([0xAA; 32], 1, vec![0xBB; 4]);
    let conflict_key = b"pool_id=A";
    let data = b"reserve_a=100;reserve_b=200";

    let conflict_hash = compute_conflict_hash(&type_script, conflict_key);
    let typed_data_hash = compute_typed_data_hash(&type_script, data);

    let access = CellScriptSchedulerAccessWitness {
        operation: SchedulerOperation::Consume,
        source: SchedulerSource::Input,
        index: 0,
        conflict_hash,
        typed_data_hash,
    };

    let trusted_summary = CellScriptSchedulerWitness {
        magic: TYPED_CELL_SCHEDULER_WITNESS_MAGIC,
        version: TYPED_CELL_SCHEDULER_WITNESS_VERSION,
        effect_class: SchedulerEffectClass::Mutating,
        parallelizable: false,
        touches_shared_count: 1,
        touches_shared: vec![conflict_hash],
        estimated_cycles: 500,
        access_count: 1,
        accesses: vec![access],
    };

    let encoded = encode_typed_cell_scheduler_witness(&trusted_summary);
    let runtime_witness = decode_typed_cell_scheduler_witness(&encoded).unwrap();

    // runtime witness is checked against trusted summary
    runtime_witness.validate_summary(&trusted_summary).unwrap();
}

#[test]
fn forged_witness_rejected() {
    // ... build trusted_summary and runtime_witness as above ...
    let mut forged = runtime_witness.clone();
    forged.accesses[0].conflict_hash = [0xFF; 32];
    assert!(forged.validate_summary(&trusted_summary).is_err());
}
```

Test that:

- conflict_hash stable after data update (change data → new typed_data_hash, same conflict_hash)
- CellDAG uses conflict_hash for scheduling (hand-craft two txs touching same shared pool)
- validate_summary rejects mismatched conflict_hash (runtime witness vs trusted summary)

---

### 1.8 Scheduler Witness Validation Rules

- `magic` must equal `TYPED_CELL_SCHEDULER_WITNESS_MAGIC`.
- `version` must equal `TYPED_CELL_SCHEDULER_WITNESS_VERSION`.
- `touches_shared_count == touches_shared.len()`.
- `access_count == accesses.len()`.
- Mutable operations must not use all-zero `conflict_hash`.
- `typed_data_hash` may be zero only for operations that do not bind concrete typed data.
- Operation/source pairs must be valid:
  ```text
  CONSUME / DESTROY / TRANSFER  → source must be INPUT
  CREATE                        → source must be OUTPUT
  READ_REF                      → source must be CELL_DEP or INPUT
  ```
- Mutable cells must not use `ConflictKeySpec::None`.
- Runtime witness must match trusted summary exactly for operation, source, index,
  conflict_hash, and typed_data_hash policy.

### 1.9 conflict_key_value Canonical Encoding

`conflict_key_value` must be canonical encoded. Composite keys must NOT use
raw concatenation (avoids `["ab", "c"]` vs `["a", "bc"]` ambiguity).

```text
conflict_key_value = len(field1_le_u32) || field1 || len(field2_le_u32) || field2 || ...
```

Length-delimited encoding. Hand-crafted tests may use raw bytes for
single-field keys, but composite keys must use this canonical form.

CellScript Step 3 lowers `#[conflict_key(composite(...))]` into canonical
conflict_key_value bytes using this encoding.

### 1.10 Conflict Key Semantic Integrity

`conflict_hash` correctness depends on `conflict_key_value` being correctly
derived from the typed cell's protocol semantics. The runtime cannot verify
this — it only hashes what it receives. Therefore:

- Shared mutable cells MUST declare an explicit `ConflictKeySpec` (not `None`).
- `ConflictKeySpec::Field` / `Composite` values MUST be validated by the
  compiler (Step 3) to cover all write-conflict state. Incorrect declarations
  produce "correct execution of a wrong conflict model."
- The runtime enforces: `ConflictKeySpec::None` on mutable cells is rejected.
  The compiler enforces: `#[conflict_key]` coverage on shared/mutable cells.

### 1.11 CellIdentity vs ConflictKeySpec Independence

`CellIdentity` and `ConflictKeySpec` are separate axes. They MUST NOT be
implicitly equated. Example: an orderbook cell:

```text
identity   = order_id     (unique per order)
conflict_key = market_id  (grouped by market for parallel execution)
```

The compiler MUST NOT default `conflict_key = identity` unless explicitly
declared. Default rules:

```text
owned mutable:   default conflict_key = CellId  (one cell, no shared conflict)
shared mutable:  conflict_key MUST be declared explicitly
immutable:       conflict_key = None (no write conflicts)
ephemeral:       conflict_key = None (not admitted to scheduler)
```

### 1.12 typed_data_hash vs Full cell_state_hash

`typed_data_hash = blake3(domain || script_id || data)` covers only
`type_script identity + data`. It does NOT include `lock` or `capacity`.

This is intentional for Phase 1. When settlement / state root requires
covering the full cell output, a separate `cell_state_hash` will be added:

```text
cell_state_hash = blake3(domain || capacity || lock_hash || type_hash || data_hash)
```

Phase 1 uses `typed_data_hash` for scheduler audit. Phase 2 introduces
`cell_state_hash` when the state root / exit root design is finalised.
The two hashes coexist: `typed_data_hash` for typed-cell audit,
`cell_state_hash` for full state commitment.

---

## Step 3: CellScript — TargetProfile::TypedCellL2

File: `CellScript/src/lib.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetProfile {
    Ckb,
    TypedCellL2,
}
```

File: `CellScript/src/stdlib/mod.rs`

```rust
impl SchedulerMetadata {
    // existing Ckb profile
    pub fn generate_molecule(...) -> Vec<u8> { ... }

    // new TypedCellL2 profile
    pub fn generate_molecule_typed_cell(
        effect_class: &str,
        parallelizable: bool,
        touches_shared: Vec<[u8; 32]>,
        estimated_cycles: u64,
        accesses: Vec<SchedulerAccess>,
    ) -> Vec<u8> {
        // 70-byte access records: operation(1) + source(1) + index(4) + conflict_hash(32) + typed_data_hash(32)
        // blake3 with spora-typed-cell/ domain separation
        // version = 1 (TYPED_CELL_SCHEDULER_WITNESS_VERSION)
    }
}
```

File: `CellScript/src/parser/mod.rs`

Add attribute parsing under TypedCellL2 profile:

- `#[conflict_key(field_name)]`
- `#[cell_class(party | immutable | ephemeral)]`
- `#[identity(singleton | type_id | field(name) | composite(...))]`
- `#[settlement(l1_settled | l2_only | rollup_committed | exit_claim)]`

---

## Step 4: End-to-End Integration

CellScript compiles with `--target-profile typed-cell-l2` → produces typed-cell witness →
Spora runtime decodes → scheduler uses conflict_hash → parallel execution.

---

## File Change Summary

### Spora runtime (`spora-typed` branch)

| File | Change |
|------|--------|
| `exec/src/celltx/types.rs` | Typed cell types, ScriptId, compute_conflict_hash, compute_typed_data_hash (blake3), clean-break scheduler witness structs, encode/decode |
| `exec/src/celltx/mod.rs` | Re-export new types |
| `exec/src/scheduler/dag.rs` | AccessMode, ConflictEntry, conflict_hash-level conflict detection with read/write discrimination |
| `exec/src/lib.rs` | Re-export new types |
| `consensus/src/pipeline/virtual_processor/access_summary.rs` | Replace binding_hash with conflict_hash |

### CellScript compiler (Step 3, separate project)

| File | Change |
|------|--------|
| `src/lib.rs` | Add `TargetProfile::TypedCellL2` variant |
| `src/stdlib/mod.rs` | Add `generate_molecule_typed_cell()` with blake3, 70-byte access records |
| `src/ir/mod.rs` | Typed-cell conflict key inference under TypedCellL2 profile |
| `src/parser/mod.rs` | `#[conflict_key]`, `#[cell_class]`, `#[identity]`, `#[settlement]` attribute parsing |

---

## Acceptance Criteria

- conflict_hash is stable across versioned cell data updates
- typed_data_hash changes across data updates
- CellDAG creates dependency edges for same-conflict WRITE+WRITE transactions
- CellDAG allows independent conflict domains in the same layer
- CellDAG allows READ + READ on same conflict_hash in same layer
- validate_summary() rejects forged or mismatched conflict hashes
- BlockAccessSummary reports shared reads/writes by conflict_hash
- Mutable cells with ConflictKeySpec::None are rejected
- CellScript TargetProfile::TypedCellL2 generates typed-cell witness format

## Out of Scope

- L2 batch structure, BFT committee, checkpoint cells
- Exit model, settlement verification
- Full cell_state_hash covering lock/capacity/type/data (Phase 2 — see §1.12)
- Conflict key coverage validation (compiler-enforced, not runtime-enforced)
- BFT consensus research (HotStuff, Tendermint, etc.)
