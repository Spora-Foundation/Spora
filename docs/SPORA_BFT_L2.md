# Typed Cell + BFT L2: A CKB-Settled Parallel Execution Layer

## 1. Core Idea

The core idea is to build a high-throughput L2 execution layer whose state model
remains faithful to the Cell philosophy.

Instead of turning CKB into an account-model chain, the L2 introduces **typed
cells** as the primary state units. Each typed cell has a declared ownership
model, conflict key, accounting role, identity policy, and settlement behaviour.

Transactions do not modify global contract storage in place. They consume
old typed cells, propose new typed cells, and prove that the transformation
is valid. ("Mutate" appears only as scheduler-level classification
terminology; typed cells remain consume/create at the semantic layer.)

**The core thesis: this is not converting CKB into an account chain, but
upgrading the Cell model into a parallel-schedulable, auditable, settleable
typed ledger.**

The BFT committee provides fast ordering and soft/final confirmation for L2
batches, while CKB L1 provides settlement, checkpointing, exit security, and
long-term neutrality.

### Relationship to Existing SporaBFT Architecture

This document specifies the L2 layer. It builds on decisions already
established in the SporaBFT architecture document:

- **Six-layer invariant hierarchy** (Cell-local / Type-group / Transaction /
  Shared-cell / Registry / Global) for verification scope and failure
  consequences.
- **ProofPlan three-layer architecture**: CellScript compiler produces the
  expected plan, the executor produces the actual receipt, and on-chain
  commitment of the receipt root enables third-party audit comparison.
- **Owned-cell optimized path** (replacing the earlier "Fast Path" term) with
  explicit soft-finality semantics and rollback responsibility.
- **Shared-cell congestion awareness** as a first-class product feature
  (`estimate-parallelism`, `detect-hot-shared-cells`).
- **CKB compatibility** at the execution-model level is "mental model
  compatible." For the CKB-settled configuration described in this document,
  CKB anchoring is **required**. Direct L1 contract equivalence remains
  best-effort / non-goal. Generic SporaBFT deployments without CKB
  settlement may treat anchoring as optional.
- **MVP convergence** on Invoice Financing Demo, validating duplicate
  financing detection, signature attribution, cell consumption/creation,
  constraint checking, oracle dependency, and audit replay — audit completeness
  over raw TPS.

Where this document introduces new classification dimensions (ownership,
mutability, accounting, identity, settlement), they are designed to compose
with the existing invariant layers and ProofPlan architecture, not replace them.

### Design Thesis and Boundary

| Layer          | Phase 1 guarantees                                  | Phase 1 does **not** guarantee yet          |
| -------------- | --------------------------------------------------- | -------------------------------------------- |
| Typed cells    | conflict-aware parallelism via `conflict_hash`      | automatic composability across typed cells   |
| Committee      | BFT-attested soft finality (≥2/3 signatures)        | fully decentralised ordering                 |
| CKB checkpoint | root continuity + committee auth on-chain           | full L2 execution validity                   |
| ProofPlan root | audit commitment (compiler-plan vs runtime-receipt) | L1-verified validity                         |
| Exit           | inclusion proof + timelock + ownership              | instant trustless exit before challenge window |
| DA             | committee-retained batch data                       | public DA guarantee                          |

---

## 2. Why Typed Cells?

A normal Cell already has a powerful structure:

```text
CellOutput {
    capacity    // storage-space quota (shannons)
    lock        // ownership script (who can spend this cell)
    type        // validation script (what transitions are valid)
    data        // arbitrary state bytes
}
```

Note that **ownership in the Cell model is determined by the lock script**, not
by a type declaration. Typed-cell ownership class is a *scheduling hint* that
tells the executor how to parallelise, not a replacement for lock-script
authority.

For high-performance L2 execution, the executor and scheduler need more
semantic information:

```text
Who owns this state?                    -> ownership class (scheduler hint)
Can it be updated?                       -> mutability class
What is its conflict key?               -> for parallel scheduling
Is it fungible, non-fungible, receipt-like, or shared pool state?
                                        -> accounting class
Does it settle back to CKB L1?          -> settlement class
How is its identity preserved across updates?
                                        -> identity class
```

Typed cells answer these questions explicitly.

A typed cell is not just a byte blob with scripts. It is a classified state unit:

```text
Typed Cell =
    Cell data schema
  + ownership class       -> maps to CellScriptSchedulerWitness.effect_class + touches_shared
  + mutability class      -> maps to scheduler operation (CONSUME/CREATE; "MUTATE" is scheduler terminology only)
  + accounting class      -> domain constraint on data layout
  + identity class        -> determines OutPoint / TYPE_ID / field key
  + settlement class      -> determines L1 commitment participation
  + conflict key          -> maps to conflict_hash in scheduler access witness
                          (stable across data updates; distinct from cell_state_hash)
                          note: owner is an authority dimension, not necessarily a conflict dimension
                          — see §3.1 for owned/fungible conflict_hash rules
```

**The core proposition: a Cell is not merely a storage unit, but a
schedulable, auditable, settleable state credential.**

---

## 3. Typed Cell Classification

Typed cells should not be classified by a single enum. A serious design uses
multiple orthogonal dimensions.

### Mapping to CellScriptSchedulerWitness

The Spora codebase already implements `CellScriptSchedulerWitness` (see
`exec/src/celltx/types.rs`), which carries scheduler-consumable metadata
attached to each transaction:

```text
CellScriptSchedulerWitness {
    magic             // 0xCE11
    version           // 1
    effect_class      // PURE | READ_ONLY | MUTATING | CREATING | DESTROYING
    parallelizable    // bool
    touches_shared    // Vec<[u8; 32]> — hashes of shared cells touched
    estimated_cycles  // u64
    accesses          // Vec<CellScriptSchedulerAccessWitness>
}

CellScriptSchedulerAccessWitness {
    operation         // CONSUME | TRANSFER | DESTROY | CLAIM | SETTLE
                      // | READ_REF | CREATE | MUTATE_INPUT | MUTATE_OUTPUT
    source            // INPUT | CELL_DEP | OUTPUT
    index             // u32 — position within the source list
    binding_hash      // [u8; 32] — current: type+data hash
                      // L2 extension: split into conflict_hash + cell_state_hash
}
```

### Conflict Hash vs State Hash

The current `binding_hash` field conflates two distinct roles:

- **conflict_hash**: a stable identifier for conflict detection. Derived from
  `hash(type_script || conflict_key_value)`, it does **not** change when cell
  data is updated. A shared Pool cell retains the same `conflict_hash` across
  reserve changes because its `pool_id` is stable.

- **cell_state_hash**: a content commitment for state integrity. Derived from
  `hash(type_script || data)`, it changes with every data update. Used for
  state root computation and audit, not for scheduling.

```text
conflict_hash = hash(type_script || declared_conflict_key_value)
    -> stable across updates
    -> scheduler conflict detection
    -> replaces binding_hash for scheduling purposes

cell_state_hash = hash(type_script || data)
    -> changes with data
    -> state root commitment / content integrity
    -> replaces binding_hash for commitment purposes
```

**Conflict keys must be stable; state hashes may change.** Without this
separation, a shared Pool cell would appear as a different conflict domain
after every reserve update, breaking the scheduler's ability to detect that
"Pool A before" and "Pool A after" are the same serialisation point.
```

The typed-cell classification dimensions map to these fields as follows:

| Classification | Scheduler Witness Mapping |
|----------------|---------------------------|
| ownership: owned | `resource` keyword; no shared conflict key; stable owner or cell-identity `conflict_hash`; authority derived from lock / owner field / witness proof |
| ownership: shared | `shared` keyword; `touches_shared` contains this cell's `conflict_hash` |
| ownership: party | `resource` + `#[cell_class(party)]` (L2 extension); `touches_shared` contains session/party `conflict_hash` |
| ownership: immutable | `resource` + `#[cell_class(immutable)]` (L2 extension); `effect_class = READ_ONLY` |
| ownership: ephemeral | `resource` + `#[cell_class(ephemeral)]` (L2 extension); not admitted to scheduler |
| accounting: receipt | `receipt` keyword; operation `CLAIM` or `SETTLE` |
| mutability: linear | operations: CONSUME input + CREATE output |
| mutability: versioned | operations: CONSUME + CREATE, `version` field in data |
| mutability: append_only | operation: MUTATE_OUTPUT (scheduler-level classification for a successor output derived from a previous cell; lowered as consume + create at Cell semantics level), data only appends |
| accounting: * | domain constraint, not scheduler-level |
| identity: * | determines OutPoint indexing and TYPE_ID presence |
| settlement: * | determines batch commitment participation |

This mapping is critical: the CellScript compiler must emit a
`CellScriptSchedulerWitness` consistent with the typed-cell declarations, and
the scheduler validates it via `validate_summary()` against a trusted
compiler-produced summary before execution.

**Default, not absolute**: under the `typed-cell-l2` profile, these mappings
are defaults. A `resource` may still declare `#[cell_class(party)]`,
`#[cell_class(immutable)]`, or `#[cell_class(ephemeral)]` where the L2
profile permits it. The mapping row shows the *default* ownership for each
keyword, not a fixed semantic equivalence.

**Trusted summary provenance**: the compiler-produced summary must be derived
from the compiled CellScript artifact and committed in the artifact manifest
or code hash. A transaction-provided scheduler witness is **never** trusted
by itself — it must match a summary whose provenance is the authoritative
compiler output. This prevents an attacker from forging scheduler witnesses
to falsely claim parallel safety.

### 3.1 Ownership / Access Class

This determines parallel execution and access rules.

```text
owned
shared
party
immutable
ephemeral
```

### `owned`

A cell controlled by one owner or one authority.

Examples:

```text
user vault position
NFT
personal order
receipt
single-user asset cell
```

Owned cells are easy to parallelise. Transactions touching different owned cells do not conflict.

**Owned/fungible conflict_hash rule**: for owned fungible cells, the default
conflict key should be the concrete cell identity or
`(asset_id, owner, shard_id)`, not merely `owner`. Using `owner` alone as
conflict key would serialise all token operations for a single large holder.
`owner` is an authority dimension (who can spend), not a concurrency dimension
(who blocks whom). The protocol should default to fine-grained conflict
scope unless a developer explicitly declares owner-level serialisation.

### `shared`

A public mutable cell accessed by many users.

Examples:

```text
AMM pool
global orderbook
oracle state
shared vault
auction book
```

Shared cells require ordering by a declared conflict key, such as `pool_id`.

### `party`

A bounded multi-party cell.

Examples:

```text
payment channel
bilateral OTC agreement
game room
escrow
committee state
```

This is more precise than treating everything as globally shared. A party cell is shared only among a known set or session.

### `immutable`

Read-only reference data.

Examples:

```text
protocol config
schema cell
fee table
curve parameters
historical oracle snapshot
```

Immutable cells introduce no write conflict and can be read by many transactions in parallel.

### `ephemeral`

L2-only transient state.

Examples:

```text
temporary execution receipt
batch-local proof state
intent trace
intermediate settlement record
```

**Ephemeral cell lifecycle rules:**

- Created within a batch by an L2 action.
- Valid only within the batch or a bounded cross-batch window.
- Must not be referenced by any `l1_settled` or `rollup_committed` cell.
- Folded into batch metadata upon batch finalisation; not persisted to L1.
- If a downstream cell depends on an ephemeral cell, the dependency must be
  resolved within the same batch, otherwise the transaction is rejected.

Ephemeral cells are not admitted to the `CellScriptSchedulerWitness` access
set — they exist below the scheduler's horizon.

> **Hard boundary**: ephemeral cells may not affect admission, ordering, or
> conflict resolution unless materialised into a committed typed cell within
> the same batch. This prevents the use of ephemeral cells to bypass
> scheduling constraints.

---

### 3.2 Mutability Class

This describes how a cell evolves.

```text
linear
versioned
append_only
migratable
```

### `linear`

The normal Cell pattern:

```text
old cell consumed (CONSUME)
new cell created (CREATE)
```

### `versioned`

Each update increments a version.

```text
next.version = current.version + 1
```

Useful for shared cells, party cells, and optimistic concurrency.

### `append_only`

State can only append, never rewrite history.

Examples:

```text
settlement journal
MMR log
execution trace
batch receipt log
```

This is very useful for rollup-style commitment and audit.

**Append-only concurrency rule**: append-only logs with different `log_id`
are parallel. Logs sharing the same `log_id` are batchable but must be
ordered by append index — the scheduler must guarantee a deterministic
append sequence, even though no write conflict exists in the traditional
sense.

### `migratable`

The cell has explicit data layout migration rules.

```text
layout v1 -> layout v2
```

This connects naturally with later CellScript data layout policies.

> **Note on `burnable`:** earlier drafts listed `burnable` as a mutability
> class. In the Cell model, burning is simply "consume without create" — it is
> an accounting-level constraint (the output side is empty), not a separate
> mutability dimension. It is therefore covered by the `linear` pattern with
> a zero-output accounting rule.

---

### 3.3 Accounting Class

This is where the design becomes strongly Cell-native.

```text
fungible
non_fungible
share
debt
receipt
claim
storage_claim
```

### `fungible`

Token-like cells with `asset_id` and `amount`.

They support split, merge, transfer, and conservation checks.

### `share`

Vault or LP share cells.

They represent proportional claim over underlying state.

### `debt`

Debt positions or obligations.

Fields may include:

```text
principal
interest_index
collateral_ref
maturity
liquidation_threshold
```

### `receipt`

A proof that something happened and may later be redeemed.

Examples:

```text
bridge receipt
vesting receipt
withdrawal receipt
DAO deposit receipt
order fill receipt
```

### `claim`

A cell representing an exit or redemption right.

Very important for L2 withdrawal and settlement.

### `storage_claim`

A cell that represents a claim on CKB L1 `capacity` (storage space measured
in shannons). Unlike `fungible` tokens, a storage claim is denominated in
`capacity` bytes and must satisfy CKB's `occupied_capacity` rule: the cell's
`capacity` field must be at least `8 + lock_serialized_size + type_serialized_size
+ data_len`.

This replaces the earlier `capacity` entry in the accounting class. The CKB
`capacity` field is a storage-space unit, not a financial asset; calling it
`storage_claim` avoids confusion with fungible token accounting.

**`storage_claim` is not a token class.** It is a claim over
occupied-capacity-backed L1 storage space, subject to CKB's
`occupied_capacity` constraint.

---

### 3.4 Identity Class

This determines whether a cell is anonymous, unique, singleton, or keyed.

```text
anonymous
type_id
singleton
field_identity
composite_identity
```

### `anonymous`

No durable identity. Common for token shards.

### `type_id`

CKB TYPE_ID-style identity.

### `singleton`

Only one live instance exists.

Examples:

```text
global config
registry
rollup checkpoint cell
```

### `field_identity`

Identity is a field:

```text
order_id
position_id
channel_id
```

### `composite_identity`

Identity is a tuple:

```text
(pool_id, owner)
(asset_id, owner)
(channel_id, participant)
```

Composite identity is very important for scalable L2 indexing and conflict detection.

---

### 3.5 Settlement Class

This is where the L2 keeps its CKB character.

```text
l1_settled
l2_only
rollup_committed
exit_claim
fraud_proof
validity_proof
```

### `l1_settled`

The typed cell has a canonical L1 Cell image.

### `l2_only`

The cell exists only inside the L2 state.

### `rollup_committed`

The cell participates in a state root or commitment root periodically posted to CKB.

### `exit_claim`

The cell can be transformed into an L1 withdrawal or claim.

### `fraud_proof` / `validity_proof`

The cell participates in optimistic or validity proof workflows.

L2 state **must** have clear L1 settlement semantics. Otherwise this is
merely an appchain, not a CKB-settled typed cell layer.

---

## 4. Example Typed Cell Declarations

The following examples show **target CellScript syntax** for typed-cell
declarations. CellScript already provides first-class `resource`, `shared`, and
`receipt` keywords that map directly to ownership classes:

- `resource` → owned (single-owner linear cell)
- `shared` → shared (multi-user mutable cell)
- `receipt` → receipt-like (proof-of-event, redeemable)

The `#[cell_class(...)]`, `#[identity(...)]`, `#[settlement(...)]`,
and `#[conflict_key(...)]` attributes are **L2 profile extensions** that do not
yet exist in the current CellScript compiler. They would be added under the
`typed-cell-l2` target profile (see §11).

Current CellScript capabilities (`#[capability(store)]`, `#[capability(transfer)]`,
`#[capability(destroy)]`) and action-level hints (`#[effect(...)]`,
`#[scheduler_hint(...)]`) already exist and carry through to the L2 profile.

A personal offer (existing `resource` keyword):

```cellscript
#[identity(field(order_id))]
#[settlement(rollup_committed)]
#[conflict_key(order_id)]
resource Offer has store {
    order_id: Hash
    owner: Address
    state: OfferState
    price: u128
    payment_asset: AssetId
}
```

An AMM pool (existing `shared` keyword):

```cellscript
#[identity(field(pool_id))]
#[settlement(rollup_committed)]
#[conflict_key(pool_id)]
shared Pool has store {
    pool_id: Hash
    version: u64
    asset_a: AssetId
    asset_b: AssetId
    reserve_a: u128
    reserve_b: u128
    fee_bps: u16
}
```

A payment channel (existing `resource` keyword, L2 `party` extension):

```cellscript
#[cell_class(party)]
#[identity(field(channel_id))]
#[settlement(exit_claim)]
#[conflict_key(channel_id)]
resource Channel has store {
    channel_id: Hash
    version: u64
    alice: Address
    bob: Address
    balance_alice: u128
    balance_bob: u128
}
```

A protocol config (existing `resource`, `immutable` via L2 attribute):

```cellscript
#[cell_class(immutable)]
#[identity(singleton)]
#[settlement(l1_settled)]
resource DexConfig has store {
    fee_admin: Address
    max_fee_bps: u16
}
```

> **Note**: `shared` and `receipt` are already first-class CellScript keywords.
> The L2 profile extends them with `#[identity(...)]`, `#[settlement(...)]`, and
> `#[conflict_key(...)]` attributes, plus new ownership classes (`party`,
> `immutable`, `ephemeral`) via the `#[cell_class(...)]` attribute on `resource`
> declarations.

---

## 5. Execution Model

The execution model remains Cell-native:

```text
consume input typed cells     -> scheduler op: CONSUME
read reference typed cells    -> scheduler op: READ_REF
create output typed cells     -> scheduler op: CREATE
prove transformation validity -> CKB-VM script verification
emit state root delta         -> CellDiff { add, remove }
```

The CellScript action below illustrates the target syntax. The L2 executor
extracts the scheduler witness from the compiled action:

```cellscript
action fill(input: Offer, payment: Token, buyer: Address)
    -> (output: Offer, seller_payment: Token)
    move input.state: Live -> output.state: Filled
where
    preserve output from input {
        order_id
        owner
        price
        payment_asset
    }

    require {
        payment.amount == input.price
        payment.asset_id == input.payment_asset
        output.owner == buyer
    }

    consume payment

    create seller_payment = Token {
        asset_id: payment.asset_id,
        amount: payment.amount,
        owner: input.owner
    } with_lock(input.owner)
```

The L2 executor extracts:

```text
read set                 -> from accesses where operation = READ_REF
write set                -> from accesses where operation ∈ {CREATE, MUTATE_INPUT, MUTATE_OUTPUT}
                          // MUTATE_* are scheduler terminology; lowered as consume + create at Cell level
conflict keys            -> conflict_hash from each access record
state transitions        -> cell_state_hash from each access record
state transition         -> CellDiff { add: created cells, remove: consumed cells }
accounting constraints   -> domain rules (conservation, debt ratios, etc.)
created cells            -> CellDiff.add
consumed cells           -> CellDiff.remove
settlement obligations   -> from settlement class of each output cell
```

So a transaction is not merely code execution. It is a typed accounting
transformation, and its `CellDiff` is the primitive that both the scheduler
and the settlement layer consume.

---

## 6. Parallel Execution

Typed cells allow semantic conflict detection before execution.

### Existing Infrastructure: CellDAG + BlockAccessSummary

The Spora codebase already implements a parallel execution pipeline:

- **`CellDAG`** (`exec/src/scheduler/dag.rs`): builds a dependency DAG from
  scheduler witnesses. Transactions are organised into topological layers;
  each layer can execute in parallel.
- **`ParallelExecutor`** (`exec/src/scheduler/executor.rs`): executes
  transactions layer by layer using Rayon, with deterministic result
  ordering by `NodeId`.
- **`BlockAccessSummary`** (`consensus/src/pipeline/virtual_processor/access_summary.rs`):
  aggregates per-transaction scheduler witnesses into a block-level read/write
  summary, including `cellscript_shared_reads` and `cellscript_shared_writes`
  sets derived from `touches_shared` hashes.
- **Trusted access set validation**: the `validate_summary()` path ensures
  that the runtime scheduler witness matches a compiler-produced trusted
  summary before the transaction is admitted to the block.

### Conflict Detection Granularity

The scheduler detects conflicts at the **conflict_hash** level — a stable
hash derived from `hash(type_script || conflict_key_value)`. This is distinct
from `cell_state_hash` (derived from `hash(type_script || data)`) which changes
on every update.

For a shared Pool cell, `conflict_hash` remains constant across reserve
changes because it is derived from the stable `pool_id`, while `cell_state_hash`
changes with each swap. This ensures that "Pool A before" and "Pool A after"
are correctly recognised as the same serialisation point.

The typed-cell `conflict_key` declaration serves two purposes:

1. **Compiler enforcement**: the CellScript compiler derives `conflict_hash`
   from `hash(type_script || declared_conflict_key_value)`, ensuring stable
   conflict domains regardless of data mutations.
2. **Documentation**: developers can reason about parallelism at the
   business-logic level (`pool_id`) while the scheduler operates at the
   cryptographic level (`conflict_hash`).

### Scheduling Rules

```text
owned cells with different conflict_hashes      -> parallel
immutable reads                                 -> parallel
shared cells with different conflict_hashes     -> parallel
same shared conflict_hash                       -> serial (within same topological layer)
party cells with different session hashes       -> parallel
append-only logs with different log_id     -> parallel
same log_id append-only                      -> batchable but ordered by append index
```

Example:

```text
Swap on Pool A  -> conflict_hash = hash(Pool type_script || pool_id=A)
Swap on Pool B  -> conflict_hash = hash(Pool type_script || pool_id=B)
Transfer Token X from Alice -> conflict_hash = hash(Token type_script || owner=alice)
Transfer Token Y from Bob   -> conflict_hash = hash(Token type_script || owner=bob)
```

These can execute in parallel unless they touch the same conflict_hash.

**Parallelism is not guessed — it is derived from the conflict domains that
typed cells expose via their scheduler witnesses.**

---

## 7. BFT Committee Layer

The L2 uses a limited validator committee for fast ordering and finality.

### Phase 1 Default: Single Sequencer + Committee Attestation

For the first usable version, the committee operates as a **single sequencer
with committee attestation**:

- One designated sequencer orders transactions and produces batch proposals.
- A committee of N validators attests to each batch by signing the batch root.
- A batch is considered soft-final when ≥ ⌈2N/3⌉+1 validators have signed.
- The sequencer can be replaced by a committee vote (liveness safety).

This provides **BFT-attested batch finality**, not full decentralised
ordering. Censorship resistance is handled through sequencer replacement
and later forced-inclusion paths (see §13). It minimises coordination
overhead and allows sub-second soft finality.

### Future Configurations

```text
BFT committee with rotating leader   -> better censorship resistance
HotStuff-style pipelined consensus    -> lower confirmation latency at scale
Tendermint-style consensus           -> simpler liveness analysis
```

These are not Phase 1 blockers. The single-sequencer model is sufficient
to validate the typed-cell execution model and CKB settlement path.

### Committee Responsibilities

```text
ordering transactions
executing or verifying typed cell transitions
signing batch roots
publishing checkpoints to CKB
maintaining data availability policy
processing exits and disputes
```

### Performance Characteristics

With a limited validator set, confirmation can be very fast:

```text
sub-second to few-second soft finality
high throughput under low-conflict workloads
periodic CKB settlement
```

The trade-off is explicit:

```text
smaller committee = faster finality, weaker censorship resistance
larger committee = more decentralised, higher coordination cost
CKB settlement = long-term security anchor independent of committee size
```

---

## 8. Batch Structure

Each L2 batch should contain:

```text
previous_state_root
ordered transaction list or tx commitment
new_state_root
cell_diff_add                          // created typed cells (aligned with CellDiff.add)
cell_diff_remove                       // consumed typed cells (aligned with CellDiff.remove)
read_set_commitment                    // from BlockAccessSummary
write_set_commitment                   // from BlockAccessSummary
conflict_key_schedule                  // conflict_hash ordering for shared cells
receipt_root
exit_root
ProofPlan / audit metadata root
committee_signatures
```

> **Note on `proofplan_root`**: in Phase 1 this is an **audit commitment**,
> not the canonical validity root. CKB L1 does not verify ProofPlan content
> in Phase 1. In later fraud/validity modes, parts of it may become
> challengeable or proof-linked.

A simplified batch commitment:

```text
Batch {
    prev_root
    tx_root
    cell_diff_root           // covers both add and remove
    read_write_root
    receipt_root
    exit_root
    proofplan_root
    next_root
    committee_sigs
}
```

This batch root is periodically committed to a CKB L1 checkpoint cell.

> **Alignment with CellDiff**: the `cell_diff_add` and `cell_diff_remove`
> fields correspond directly to `CellDiff { add: CellCollection, remove:
> CellCollection }` already implemented in `consensus/core/src/cell_diff.rs`.

---

## 9. CKB Settlement

CKB L1 does not execute every L2 transaction. It verifies settlement
commitments and exit/fraud/validity rules.

### Rollup Configuration Cell

Global rollup parameters live in a dedicated **rollup config cell** (singleton,
`l1_settled`, `identity(singleton)`):

```text
rollup_id
committee_public_keys
challenge_period_blocks
max_batch_interval
settlement_type              // committee | optimistic | validity
```

The `challenge_period` is a global parameter, not per-batch. It belongs in
the config cell, not repeated in each checkpoint.

### Checkpoint Cell

A checkpoint cell may contain:

```text
rollup_id
batch_number
prev_state_root
new_state_root
batch_data_hash
committee_signature_aggregate
exit_root
```

> **DA boundary**: in Phase 1, `batch_data_hash` commits to committee-retained
> batch data and is not by itself a public DA guarantee. A hash only commits
> to *what the data is*, not *that the data remains retrievable*. In later
> phases, `batch_data_hash` must bind to an explicit DA availability proof
> or external DA commitment.

### Checkpoint Type Script Verification (Feasibility)

The checkpoint cell's type script must verify on CKB L1. This raises a
critical feasibility question: **can the verification fit within CKB's cycle
limits?**

Estimated cycle budget for Phase 1 checkpoint verification
(**preliminary targets, subject to on-chain benchmark**):

```text
BLS signature aggregation verify (N keys, 1 aggregate)
    ~5M cycles for ~20 validators (secp256k1 multi-sig alternative: ~3M)
State root continuity check (hash comparison)
    ~0.1M cycles
Checkpoint cell structure parsing (Molecule decode)
    ~0.2M cycles
Total Phase 1 estimate: ~5–6M cycles
```

CKB's per-block cycle limit is 70M (`MAX_BLOCK_CYCLES`), and per-transaction
is 10M (`MAX_TX_CYCLES`). A checkpoint transaction at ~5–6M cycles is within
the per-transaction limit but consumes a significant portion. This is
feasible for periodic (not per-block) settlement, but requires careful gas
pricing and settlement frequency tuning.

**Phase 1 preferred signature scheme**: threshold M-of-N secp256k1
signatures verified on-chain, because this aligns better with existing CKB
cryptographic tooling and has a more predictable cycle cost.

BLS signature aggregation remains an optional optimisation after cycle
benchmarks confirm its feasibility on CKB-VM.

**Fallback if on-chain signature verification exceeds cycle budget**:

- **Fallback A**: reduce committee size N until M-of-N secp256k1 fits within
  `MAX_TX_CYCLES` (10M). This is the safest option — L1 always verifies
  committee authorisation.
- **Fallback B**: optimistic checkpoint acceptance, where the checkpoint
  cell includes only the committee public key set root and the batch root;
  committee signature validity becomes challengeable during the challenge
  window. This requires a complete challenge path and is not a Phase 1
  goal.

Do **not** accept a checkpoint on L1 based solely on off-chain-verified
signatures. L1 must either verify the signature itself (Fallback A) or
enter a declared optimistic mode with challenge semantics (Fallback B).

### Settlement Stages

In the simplest committee-based version:

```text
CKB verifies committee signatures and checkpoint continuity.
```

In a more advanced optimistic version:

```text
CKB supports fraud challenges against invalid transitions.
```

In a validity version:

```text
CKB verifies succinct validity proof or proof commitment.
```

Recommended staging:

```text
Phase 1: BFT committee + CKB checkpoint settlement
Phase 2: exit claims + DA commitments
Phase 3: fraud proof / validity proof roadmap
```

Do not make ZK validity proof a blocker for the first usable version.

---

## 10. Exit Model

Exit is critical. Users must understand how L2 state becomes L1 state.

An exit claim can be represented as a typed cell:

```cellscript
#[cell_class(owned)]
#[identity(field(exit_id))]
#[settlement(l1_settled)]
resource ExitClaim has store {
    exit_id: Hash
    owner: Address
    asset_id: AssetId
    amount: u128
    source_batch: u64
    inclusion_proof_root: Hash
}
```

Exit action:

```cellscript
action exit(claim: ExitClaim, witness proof: InclusionProof)
    -> l1_token: L1Token
where
    require verify_inclusion(claim, proof)
    require claim.owner == proof.owner

    consume claim

    create l1_token = L1Token {
        asset_id: claim.asset_id,
        amount: claim.amount
    } with_lock(claim.owner)
```

**Exit membership root**: `verify_inclusion` checks membership against a
checkpoint root. Phase 2 must choose one canonical exit membership root:

- **Option A: state-root based exit** — `ExitClaim` is a committed typed cell
  inside `new_state_root`. This preserves typed cell purity but requires
  L1 verification against the full state root.
- **Option B: exit-root based exit** — exits are separately accumulated in
  `exit_root`. This is more specialised for L1 verification and is the
  recommended default for Phase 2.

Either choice must be declared upfront; the two roots must not be mixed for
the same exit claim.

This keeps exit as an action, not hidden magic.

**Exit is not a bridge's black-box operation — it is an auditable Cell
transformation.**

### L1 Exit Verification Path

For the exit to work on CKB L1, the exit claim's type script must verify:

1. **Inclusion proof**: the claim cell exists in a committed L2 state root.
   This requires a Merkle proof verification inside a CKB type script.
   Estimated cycle cost: ~2–3M cycles for a standard Merkle branch verify
   (32-byte hashes, depth ~20–30 levels).

2. **Challenge window**: the claim must not be spendable until
   `challenge_period` blocks have passed on L1. This uses CKB's `since`
   field on the exit claim cell's `CellInput`, with the relative lock set
   to the challenge period length. The `since` field already supports
   block-number-based relative locks in the CKB VM (`LoadHeader` syscall).

3. **Ownership**: the exit claim's lock script must match the original L2
   owner. Since the exit claim cell is `l1_settled` with the owner's lock
   script, CKB's standard lock verification handles this natively.

This three-part verification path (inclusion + timeliness + ownership) is
the minimum viable exit verification for Phase 2.

---

## 11. Role of CellScript

CellScript remains the language for writing typed cell transformations.

It should not fork into a separate language. Instead:

```text
CellScript core
  + ckb target profile (existing: TargetProfile::Ckb)
  + typed-cell-l2 target profile (future: TargetProfile::TypedCellL2)
```

This aligns with the existing **profile-gated** design: CellScript already
supports `TargetProfile` (currently only `Ckb`) which controls stdlib
generation, syscall availability, and scheduler ABI. The `Ckb` profile
preserves strict CKB-VM compatibility; the future `TypedCellL2` profile
adds L2-specific constructs.

### Current CellScript Constructs (both profiles)

```text
Keywords:      resource | shared | receipt | action | flow | lock
Capabilities:  #[capability(store)] | #[capability(transfer)] | #[capability(destroy)]
Effects:       #[effect(pure|readonly|mutating|creating|destroying)]  (on actions)
Scheduler:     #[scheduler_hint(parallel|sequential, estimated_cycles=N)]  (on actions)
Operations:    create | consume | transfer | destroy | claim | settle | read_ref
Constraints:   require | preserve | move
Ownership:     linear type system (compile-time consumption checks)
```

### L2 Profile Extensions

The `typed-cell-l2` profile would add:

```text
New attributes on resource/shared/receipt declarations:
  #[identity(field(name) | singleton | type_id | composite(...))]  -> OutPoint indexing
  #[settlement(l1_settled | l2_only | rollup_committed | exit_claim)]  -> batch participation
  #[conflict_key(field_name)]  -> conflict_hash derivation rule
  #[cell_class(party | immutable | ephemeral)]  -> new ownership on resource

New target profile:
  TargetProfile::TypedCellL2  -> extends stdlib, scheduler ABI, settlement metadata
```

The compiler must emit a `CellScriptSchedulerWitness` consistent with these
declarations, and the `validate_summary()` path on the scheduler side ensures
the runtime witness matches the compiler-produced trusted summary.

### Existing Scheduler Tooling

CellScript already provides `cellc scheduler-plan` for policy consumption:

```bash
cellc scheduler-plan contract.cell --target-profile ckb
```

This emits per-action parallelism decisions, shared touch-set conflicts,
estimated cycles, and total/max summaries. The L2 profile would extend this
report with settlement participation and conflict key schedule metadata.

This keeps L1 and L2 semantically aligned while allowing the L2 profile to
express scheduling and settlement metadata that the L1 profile does not need.

---

## 12. How This Differs from Other L2 Approaches

### Compared with Sui

Sui has a powerful object ownership model: owned, shared, immutable, party-like
ownership patterns.

Typed Cell L2 should learn from this, but not become an object chain.

Difference:

```text
Sui:
    object-centric Move execution

Typed Cell L2:
    Cell transformation + accounting + CKB settlement
```

### Compared with Fuel/Sway

Fuel is strong in UTXO-style parallel execution and access lists.

Typed Cell L2 should learn from this, but remain more settlement-aware and
audit-oriented.

Difference:

```text
Fuel/Sway:
    Rust-like contract engineering over FuelVM

Typed Cell L2:
    verifier-style Cell transformations with ProofPlan and L1 settlement images
```

### Compared with Rollup Ecosystem (Arbitrum, Optimism, StarkNet, zkSync)

The broader rollup ecosystem offers various L2 approaches:

```text
Arbitrum/Optimism:  EVM-compatible optimistic rollups
StarkNet:           Cairo VM + STARK validity proofs
zkSync:             EVM-compatible validity rollups
```

None of these are Cell-native. They inherit the account model and contract
storage from their respective L1 chains.

Typed Cell L2 differs fundamentally:

```text
All of the above:
    account-model or contract-storage L2, L1-VM-compatible

Typed Cell L2:
    Cell-model L2, accounting-first, parallel by typed conflict keys,
    CKB-settled, auditable through ProofPlan
```

### Unique Position

```text
CKB-settled
typed-cell native
accounting-first
parallel by semantic conflict keys (conflict_hash)
auditable through CellScript/ProofPlan
```

---

## 13. Security Model

The security model should be explicit.

### Phase 1 Trust Model

Phase 1 assumes:

```text
at least one honest committee member for audit / fraud evidence preservation
≥2/3 committee signatures for soft-final batches
committee DA availability until batch data is externally committed
CKB L1 enforces checkpoint continuity and exit timelocks
users treat L2 soft finality as rollback-capable until L1 checkpoint finality
```

Phase 1 does **not** assume:

```text
trustless execution validity
permissionless ordering
public DA
automatic fraud-proof enforcement
```

### Fast Path Security

```text
BFT committee signs valid batches.
Users get fast L2 finality.
Soft finality: owned-cell optimized path with explicit rollback responsibility.
Hard finality: after CKB checkpoint confirmation.
```

### Settlement Security

```text
CKB stores checkpoint commitments.
CKB enforces exit and challenge rules.
Checkpoint type script verifies committee signature aggregate and state continuity.
```

### Data Availability

Options:

```text
Phase 1: committee DA (simplest, trusted committee)
Phase 2: external DA (e.g. EigenDA, Celestia) with L1 DA commitment
Phase 3: CKB-posted compressed data + hybrid DA with fraud/exit fallback
```

Data availability is the liveness assumption. If DA is lost, users cannot
construct exit proofs. The roadmap must show how each DA upgrade reduces
this risk.

### User Protection

A mature design should eventually provide:

```text
forced exit                    -> user can exit even if sequencer is down
checkpoint verification        -> light clients can verify L2 state independently
challenge window               -> time for fraud proofs after checkpoint
fraud proof or validity proof  -> cryptographic assurance of correct execution
committee slashing             -> economic penalty for misbehaviour
committee rotation             -> governance mechanism for validator replacement
```

Early stage can start with committee trust, but the roadmap must clearly
show how it hardens.

> **Fraud/validity proof warning**: fraud/validity proof design is
> intentionally out of Phase 1. Until Phase 4, security is committee-trust
> plus CKB checkpoint/exit enforcement, not fully trust-minimised rollup
> security. Do not conflate BFT-attested finality with trustless execution.

### Committee Misbehaviour

```text
Sequencer censorship:
    -> users submit forced-inclusion transactions via CKB L1
    -> after timeout, sequencer must include or be replaced

Committee signing invalid batch:
    -> any validator can produce a fraud proof
    -> misbehaving validators are slashed (stake loss)
    -> batch is reverted, affected exits are prioritised

DA withholding:
    -> committee must post DA commitments to CKB
    -> if DA is unavailable for > challenge_period, forced exit window opens
```

---

## 14. Performance Expectations

If validator count is limited and conflict keys are well-designed, performance
can be very high.

The system benefits from:

```text
small BFT committee (Phase 1)
parallel execution of non-conflicting typed cells (via CellDAG + ParallelExecutor)
immutable read sharing
party-cell localised state
batch settlement
state root commitments
```

Expected characteristics:

```text
low latency L2 confirmation
high TPS under low-conflict workloads
shared hot-cell bottlenecks isolated by conflict_hash
periodic CKB settlement
```

Bottlenecks:

```text
hot shared pools (same conflict_hash = serialisation point)
data availability bandwidth
state database writes
signature aggregation
batch proof generation
settlement frequency
```

### Shared-Cell Congestion Awareness

The SporaBFT architecture already identifies shared-cell congestion as a
first-class concern. The typed-cell design should expose congestion signals
that the scheduler can consume:

```text
estimate_parallelism(conflict_hash)    -> how many txs can touch this key concurrently
detect_hot_shared_cells(batch)       -> flag cells that serialise too many transactions
shard_shared_cell(cell, key_fn)      -> split a hot shared cell into partitioned owned cells
```

The design should therefore encourage:

```text
owned cells by default
shared cells only when needed
party cells for bounded multi-party state
append-only logs for batch accounting
pool/order sharding for high-traffic shared state
```

---

## 15. Development Roadmap

### Phase 1: Minimal Typed Cell Appchain

```text
single sequencer + committee attestation (see §7)
typed cell store
owned/shared/immutable classification -> CellScriptSchedulerWitness mapping
conflict-key scheduler (CellDAG + ParallelExecutor)
CellScript action execution with scheduler witness validation
state root
committee-signed batches
CKB checkpoint cell with type script
MVP: Invoice Financing Demo (audit completeness over TPS)
```

Goal:

```text
Two acceptance criteria:
1. Parallel execution evidence: same batch contains non-conflicting typed cell
   transactions executed in parallel, producing a deterministic final state root.
2. Settlement feasibility evidence: one checkpoint transaction verifies on
   CKB within the cycle target (≤10M), with committee attestation and root
   continuity.
```

### Phase 1 Non-Goals

Phase 1 explicitly does **not** target:

```text
Not a fully trustless rollup                 -> committee trust required
Not a ZK validity rollup                     -> validity proof is Phase 4+
Not full decentralised ordering               -> single sequencer orders
Not automatic protocol composability          -> typed cells are manually declared
Not direct equivalence to CKB L1 type scripts -> best-effort compatibility
Not a replacement for CellScript L1 profile   -> both profiles coexist
```

### Phase 2: Accounting and Exit Layer

```text
fungible / receipt / claim / storage_claim cell classes
exit claim cells with L1 inclusion proof verification (see §10)
Merkle proof verification in CKB type script (~2–3M cycles)
CKB withdrawal verifier using `since` field for challenge window
batch receipt root
ProofPlan audit expansion
```

Goal:

```text
prove users can safely exit and audit accounting with L1-enforced guarantees.
```

### Phase 3: Advanced Parallelism

```text
party cells with bounded multi-party state
append-only journals
shared-cell sharding (shard_shared_cell)
congestion-aware scheduling (estimate_parallelism, detect_hot_shared_cells)
intent batching
route netting
parallel execution benchmarks
```

Goal:

```text
prove it scales beyond simple transfers and handles shared-cell hot spots.
```

### Phase 4: Security Hardening

```text
DA policy upgrade (committee -> external -> hybrid)
fraud proof or validity proof design
committee rotation and slashing governance
forced exit via L1 inclusion proof
standard compatibility fixtures
six-layer invariant enforcement across all paths
```

Goal:

```text
reduce trust in the committee and harden settlement.
```

### Phase 5: Production Tooling

```text
transaction solver
L2 explorer
ProofPlan audit bundle
batch trace viewer
deployment governance
wallet integration
SDK
```

Goal:

```text
make it usable by real applications.
```

---

## 16. One-Sentence Positioning

> **Typed Cell + BFT L2 is a CKB-settled parallel execution layer where typed
cells expose ownership, conflict keys (via conflict_hash), accounting roles,
identity, and settlement semantics, allowing a small BFT committee to execute
transactions quickly while preserving CellScript's explicit verifier and
ProofPlan audit model.**

---

## 17. Shorter Strategic Framing

```text
Sui makes objects parallel.
Fuel makes UTXOs parallel.
Rollups make accounts parallel and L1-settled.
Typed Cell L2 makes accounting cells parallel and CKB-settled.
```
