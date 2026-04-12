# Spora × GhostDAG × Cell Architecture

**Date**: 2025-10-22  
**Version**: 1.0  
**Status**: Historical Reference Only

> This document is superseded by [spora_consensus_architecture_v2.md](/Users/arthur/RustroverProjects/Spora/docs/spora_consensus_architecture_v2.md).
> It remains useful as design history, but it is not the protocol source of truth anymore.

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Architecture Layers](#2-architecture-layers)
3. [Virtual Block Scope](#3-virtual-block-scope)
4. [Cell Lifecycle](#4-cell-lifecycle)
5. [State Commitment Model](#5-state-commitment-model)
6. [Consensus Flow](#6-consensus-flow)
7. [Reorg Handling](#7-reorg-handling)
8. [Key Design Decisions](#8-key-design-decisions)

---

## 1. Executive Summary

**Spora** is a DAG-based blockchain that combines:
- **GhostDAG** consensus (ordering and selection)
- **CKB Cell model** (programmable state)
- **DAA scores** (global ordering without block height)

### Core Principles

1. **Virtual block exists ONLY in GhostDAG consensus layer**
2. **State layer uses parent set aggregation** (no virtual parent)
3. **Cell lifecycle tracked by DAA scores** (not block numbers)
4. **Deterministic execution** (BTreeMap, sorted iteration)

---

## 2. Architecture Layers

```
┌─────────────────────────────────────────────────────────────┐
│                    Application Layer                         │
│  (Wallet, Mining, RPC - Transaction construction)           │
└──────────────────────┬──────────────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────────────┐
│                    Mempool Layer                             │
│  • CellPool: Transaction memory pool                        │
│  • Deterministic conflict resolution                         │
│  • RBF: fee_density → blue_score → wtxid                     │
│  • CPFP: Child-pays-for-parent chains                        │
└──────────────────────┬──────────────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────────────┐
│                  Consensus Layer                             │
│  ┌────────────────────────────────────────────────────┐     │
│  │           GhostDAG (Ordering)                      │     │
│  │  • Virtual block tip (highest blue score)          │     │
│  │  • Selected parent (highest blue work chain)       │     │
│  │  • Blue set (honest blocks)                        │     │
│  │  • Red set (late/withhold blocks)                  │     │
│  └────────────────────────────────────────────────────┘     │
│  ┌────────────────────────────────────────────────────┐     │
│  │      Cell Validation (3 layers)                    │     │
│  │  1. Isolation: Format, capacity, size              │     │
│  │  2. Context: Availability, conservation, locks     │     │
│  │  3. DAG: Cellbase maturity, reorg awareness        │     │
│  └────────────────────────────────────────────────────┘     │
│  ┌────────────────────────────────────────────────────┐     │
│  │    Virtual Processor (State Transitions)           │     │
│  │  • calculate_cell_state: Process mergeset          │     │
│  │  • verify_cell_root: Merkle root validation        │     │
│  │  • commit_cell_state: Persist to stores            │     │
│  └────────────────────────────────────────────────────┘     │
└──────────────────────┬──────────────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────────────┐
│                    State Layer                               │
│  • CellStateTree: Merkle tree of live cells                 │
│  • CellDB: OutPoint → CellMeta indexing                     │
│  • SpendJournal: Historical state queries                   │
│  • cell_root: State commitment for verification             │
└──────────────────────┬──────────────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────────────┐
│                   Execution Layer                            │
│  • CKB-VM: RISC-V script execution                          │
│  • Syscalls: LoadCell, LoadInput, LoadHeader, etc.          │
│  • Script Groups: Lock scripts, Type scripts                │
│  • Cycles Accounting: DoS protection                        │
└─────────────────────────────────────────────────────────────┘
```

---

## 3. Virtual Block Scope

### ✅ Where Virtual Block EXISTS

**GhostDAG Consensus Layer Only**:

```rust
// Virtual block = tip of GhostDAG selected parent chain
pub struct VirtualState {
    pub ghostdag_data: GhostdagData,  // Virtual block's GhostDAG data
    pub selected_parent: Hash,         // Highest blue work block
    pub blue_set: Vec<Hash>,          // Honest blocks
    pub daa_score: u64,               // Virtual DAA score
    // ...
}
```

**Used for**:
- Transaction ordering (mempool selection)
- Mining (next block selection)
- Block acceptance decisions

### ❌ Where Virtual Block DOES NOT exist

**State Layer** - Uses **parent set aggregation**:

```rust
// NO virtual parent in cell_root calculation!
// cell_root is computed from:
// 1. Selected parent's cell_root
// 2. Block's own cell diff (inputs + outputs)

fn verify_cell_root(block: &Block, selected_parent_root: Hash) -> Result<()> {
    let mut tree = CellStateTree::from_root(selected_parent_root);
    tree.apply_diff(&block.cell_diff);
    let computed_root = tree.root();
    
    if computed_root != block.header.cell_root {
        return Err(BadCellRoot);
    }
    Ok(())
}
```

**Why no virtual parent in state?**
- State is per-block (deterministic)
- Virtual block changes with new tips (dynamic)
- cell_root must be verifiable from block data alone

---

## 4. Cell Lifecycle

### Creation → Live → Spent (DAG-aware)

```
┌─────────────────────────────────────────────────────────────┐
│ 1. CREATION (tx outputs)                                    │
│    • Block at DAA score 50                                  │
│    • Cell added to CellDB with created_daa = 50             │
│    • Cell appears in CellStateTree                          │
│    • cell_root updated                                      │
└────────────────────────┬────────────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────────────┐
│ 2. LIVE (queryable period)                                  │
│    • Cell in CF_CELLS column family                         │
│    • get_cell_at_daa(cell, 75) → Some(cell)                 │
│    • Spendable if maturity met                              │
│    • Multiple blocks may reference (DAG)                    │
└────────────────────────┬────────────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────────────┐
│ 3. SPENT (tx inputs)                                        │
│    • Block at DAA score 150 spends the cell                 │
│    • Cell moved: CF_CELLS → CF_SPENT + CF_SPEND_JOURNAL     │
│    • SpendRecord stored (spent_at_daa=150, meta)            │
│    • get_cell_at_daa(cell, 100) → Some(cell) (historical!)  │
│    • get_cell_at_daa(cell, 200) → None (already spent)      │
└─────────────────────────────────────────────────────────────┘
```

### Historical Queries (GhostDAG-aware)

**Query Logic**:
```rust
fn get_cell_at_daa(outpoint: OutPoint, at_daa: u64) -> Option<CellMeta> {
    // Case 1: Currently live
    if let Some(meta) = CF_CELLS.get(outpoint) {
        if meta.created_daa <= at_daa {
            return Some(meta); // Was live then, still live now
        }
    }
    
    // Case 2: Spent but check history
    if let Some(record) = CF_SPEND_JOURNAL.get(outpoint) {
        if record.cell_meta.created_daa <= at_daa && record.spent_at_daa > at_daa {
            return Some(record.cell_meta); // Was live at that time
        }
    }
    
    None // Not created yet or already spent
}
```

**Use Cases**:
- **Reorg validation**: "Was cell live at DAA 100?"
- **Fork resolution**: "Which chain has valid cell state?"
- **Historical proofs**: "Prove cell existed at block X"

---

## 5. State Commitment Model

### Dual Commitment Design

```rust
pub struct Header {
    // ... other fields
    
    /// cell_root: Merkle root of all live cells (state commitment)
    /// - Verifiable state proof
    /// - Lightweight client can validate
    /// - Deterministically computed from parent + diff
    pub cell_root: Hash,
    
    /// cell_commitment: Versioned commitment (evolution path)
    /// - v0: H("spora/cell_commitment/v0" || cell_root)
    /// - v1: H(cell_root || history_root) [future]
    /// - Allows backward-compatible upgrades
    pub cell_commitment: Hash,
}
```

### cell_root Calculation

```
parent_root = selected_parent.cell_root

FOR each transaction in block:
    FOR each input:
        tree.remove(input.outpoint)  // Spend cell
    FOR each output:
        tree.insert(output.outpoint, output.meta)  // Create cell

block.cell_root = tree.merkle_root()
```

**Properties**:
- ✅ Deterministic (BTreeMap iteration)
- ✅ Incremental (only affected cells)
- ✅ Verifiable (Merkle proofs)
- ✅ Compact (32-byte root)

### cell_commitment Evolution

**Version 0 (Current)**:
```rust
cell_commitment = H("spora/cell_commitment/v0" || cell_root)
```
- Simple wrapper around cell_root
- Allows future extension

**Version 1 (Future)**:
```rust
history_root = merkle_tree(all_historical_states)
cell_commitment = H(cell_root || history_root)
```
- Enables historical state proofs
- Backward compatible (v0 clients validate cell_root)

---

## 6. Consensus Flow

### Block Processing Sequence

```
1. GHOSTDAG Ordering
   ├─> selected_parent = highest blue work
   ├─> mergeset = reachable non-conflicting blocks
   └─> blue_set, red_set determination

2. Cell State Processing
   ├─> Load selected_parent.cell_root
   ├─> Process selected_parent coinbase
   ├─> Process mergeset blues (topological order)
   │   ├─> For each transaction:
   │   │   ├─> Validate in isolation (format, size)
   │   │   ├─> Validate in context (availability, capacity)
   │   │   ├─> Validate in DAG (maturity, conflicts)
   │   │   ├─> Remove input cells
   │   │   └─> Add output cells
   │   └─> Accumulate cell_diff
   └─> Calculate cell_root

3. Verification
   ├─> Verify cell_root matches header
   ├─> Verify cell_commitment
   └─> If valid: Commit to storage

4. Finalization
   ├─> Store cell_diff (block → diff)
   ├─> Store cell_root (block → root)
   ├─> Update virtual state
   └─> Notify subscribers
```

### Mergeset Processing (GHOSTDAG-aware)

**Key Insight**: Transactions may appear in multiple mergeset blocks!

```rust
// Track processed to avoid duplicates
let mut processed_txs = HashSet::new();

for blue_block in ghostdag_data.mergeset_blues {
    for tx in block_txs(blue_block) {
        if processed_txs.contains(&tx.id()) {
            continue; // Skip duplicate
        }
        processed_txs.insert(tx.id());
        
        // Process transaction...
    }
}
```

**Ordering**:
1. Selected parent coinbase (first)
2. Mergeset blues in topological order
3. Within block: transaction order preserved

---

## 7. Reorg Handling

### Scenario: Chain Reorganization

```
Before:
    A --- B --- C --- D (selected chain)
     \
      E --- F

After:
    A --- B --- C --- D
     \
      E --- F --- G --- H (new selected chain, higher blue work)
```

### Reorg Process

```rust
fn calculate_cell_state_relatively(from: D, to: H) -> Hash {
    // Step 1: Find split point
    let split = find_split_point(D, H); // Returns B
    
    // Step 2: Revert D → C → split
    let mut diff = CellDiff::new();
    for block in chain(D → B).reverse() {
        let block_diff = cell_diffs_store.get(block);
        diff.with_diff_in_place(&block_diff.reverse());
    }
    // Now at state B
    
    // Step 3: Apply split → F → H
    for block in chain(B → H) {
        let block_diff = cell_diffs_store.get(block);
        diff.with_diff_in_place(&block_diff);
        
        // Validate cell_root matches
        verify_cell_root(block, diff)?;
    }
    
    H // Return new tip
}
```

### Historical Validation

**Why get_cell_at_daa matters**:

```rust
// Validating block F during reorg:
// Block F was created at DAA 75, spending cell created at DAA 50

// Q: Was the cell live when F was created?
let cell = get_cell_at_daa(input.outpoint, 75)?;

if cell.is_none() {
    return Err("Cell not available at DAA 75");
}

// Even if cell was later spent at DAA 100,
// it was valid when F spent it at DAA 75 ✅
```

---

## 8. Key Design Decisions

### Decision 1: No Virtual Parent in State Layer

**Rationale**:
- Virtual block changes with every new tip
- cell_root must be deterministic and verifiable
- Block header must be self-contained

**Implementation**:
```rust
// ❌ WRONG: Use virtual state
cell_root = virtual_state.cell_tree.root()

// ✅ CORRECT: Use selected parent + block diff
cell_root = apply_diff(selected_parent.cell_root, block.cell_diff)
```

### Decision 2: DAA Score Instead of Block Number

**Block Number Problems** (in DAG):
- Multiple blocks at same height
- No global ordering
- Reorg changes numbering

**DAA Score Benefits**:
- Global total order
- Monotonic (never decreases)
- Reorg-stable
- Natural time lock primitive

**Example**:
```
Block A: height=100, blue_score=95, daa_score=200
Block B: height=100, blue_score=90, daa_score=190

A has higher DAA → comes "later" in consensus order
```

### Decision 3: BTreeMap for All Consensus Maps

**Requirement**: Deterministic iteration order

**HashMap Issue**:
```rust
let diff = cell_diff;
for (outpoint, meta) in diff.add.iter() {
    // HashMap: order = [C, A, B] or [A, C, B] (random!)
    // Different nodes → different Merkle trees → CONSENSUS FAILURE
}
```

**BTreeMap Solution**:
```rust
let diff = cell_diff;  // BTreeMap!
for (outpoint, meta) in diff.add.iter() {
    // BTreeMap: order = [A, B, C] (always sorted by key)
    // All nodes → same Merkle tree → consensus safety ✅
}
```

### Decision 4: Spend Journal for Historical Queries

**Problem**: Need to query Cell state at past DAA scores

**Solution**: Store full metadata when spending

```rust
// When spending cell at DAA 150:
CF_CELLS.delete(outpoint)
CF_SPENT.put(outpoint, 150)
CF_SPEND_JOURNAL.put(outpoint, SpendRecord {
    spent_at_daa: 150,
    cell_meta: full_metadata,  // ✅ Preserved!
})
```

**Benefits**:
- Historical queries work
- Reorg validation efficient
- Light clients can get proofs

**Cost**: ~200 bytes per spent cell (acceptable)

---

## Comparison: Spora vs CKB vs Bitcoin

| Feature | Bitcoin (legacy txout) | CKB (Cell + NC-Max) | Spora (Cell + GhostDAG) |
|---------|----------------|---------------------|-------------------------|
| **State Model** | legacy txout | Cell | Cell ✅ |
| **Consensus** | Longest chain | NC-Max (PoW) | GhostDAG ✅ |
| **Ordering** | Block height | Block number | DAA score ✅ |
| **Time Locks** | Block height / timestamp | Epoch / timestamp | DAA score / timestamp ✅ |
| **Maturity** | 100 blocks | 4 epochs | 100 DAA scores ✅ |
| **Programmability** | Script | CKB-VM scripts | CKB-VM scripts ✅ |
| **State Commitment** | legacy txout set hash | None (transactions only) | cell_root Merkle tree ✅ |
| **Finality** | Probabilistic | Probabilistic | Probabilistic |
| **Reorg Handling** | Rewind & replay | Rewind & replay | Rewind & replay ✅ |

**Key Difference**: Spora = CKB Cell model + GhostDAG consensus

---

## Appendix A: Data Structures

### Header
```rust
pub struct Header {
    pub hash: Hash,                      // Block ID
    pub version: u16,
    pub parents_by_level: Vec<Vec<Hash>>, // DAG parents
    pub hash_merkle_root: Hash,           // Transaction Merkle root
    pub accepted_id_merkle_root: Hash,    // Accepted tx Merkle root
    pub cell_commitment: Hash,            // Versioned cell commitment
    pub cell_root: Hash,                  // State Merkle root ⭐
    pub timestamp: u64,
    pub bits: u32,
    pub nonce: u64,
    pub daa_score: u64,                   // DAA ordering ⭐
    pub blue_work: BlueWorkType,
    pub blue_score: u64,                  // GhostDAG blue score ⭐
    pub pruning_point: Hash,
}
```

### CellDiff
```rust
pub struct CellDiff {
    pub add: BTreeMap<TransactionOutpoint, CellMeta>,    // Created cells
    pub remove: BTreeMap<TransactionOutpoint, CellMeta>, // Spent cells
}

// Deterministic iteration order ✅
```

### CellStateTree
```rust
pub struct CellStateTree {
    pub cells: BTreeMap<Hash, CellEntry>,  // outpoint_hash → cell
    cached_root: Option<Hash>,              // Performance optimization
}

impl CellStateTree {
    pub fn root(&mut self) -> Hash {
        // Binary Merkle tree from sorted cells
        // H("spora-cell/node" || left || right)
    }
}
```

---

## Appendix B: Example Scenarios

### Scenario 1: Simple Block

```
Genesis (DAA 0):
    cell_root = ZERO_HASH (no cells)

Block A (DAA 1):
    Coinbase creates Cell1 (reward)
    Diff: add={Cell1}, remove={}
    cell_root = H(Cell1)

Block B (DAA 2):
    Parents: [A]
    Selected parent: A
    Tx1: spend Cell1, create Cell2
    Diff: add={Cell2}, remove={Cell1}
    cell_root = H(Cell2)
```

### Scenario 2: DAG Mergeset

```
    A (DAA 10)
   / \
  B   C (DAA 11)
   \ /
    D (DAA 12)

Block D processing:
    selected_parent = C (assume higher blue work)
    mergeset = [C, B]
    
    Cell state calculation:
    1. Start from C.cell_root
    2. Process C's coinbase
    3. Process B's transactions (blues in topo order)
    4. Apply accumulated diff
    5. D.cell_root = merkle_root(result)
```

### Scenario 3: Reorg with Historical Query

```
Original chain:
    G --- A --- B (DAA 50) --- C (DAA 100)
                 Creates Cell1   Spends Cell1

New chain:
    G --- A --- D (DAA 60) --- E (DAA 110)
                 Tries to spend Cell1

Reorg validation:
    1. Validate block D at DAA 60:
       - Query: get_cell_at_daa(Cell1, 60)
       - Result: None (Cell1 created at DAA 50 in branch B, not in branch D)
       - Verdict: Invalid (cell doesn't exist)
    
    2. Alternative: If Cell1 was created in A:
       - Query: get_cell_at_daa(Cell1, 60) 
       - Result: Some(Cell1) (created at DAA 50 in A, spent at DAA 100 in C)
       - Check: 50 <= 60 < 100 ✅
       - Verdict: Valid (cell was live at DAA 60)
```

---

## Conclusion

**Spora Architecture** successfully combines:
- ✅ GhostDAG consensus (ordering + selection)
- ✅ CKB Cell model (programmable state)
- ✅ DAA scores (global ordering)
- ✅ Deterministic execution (BTreeMap)
- ✅ Historical queries (SpendJournal)

**Virtual Block Scope**: Consensus layer only, NOT in state computation  
**State Commitment**: Parent aggregation, no virtual parent  
**Cell Lifecycle**: DAA-aware creation → live → spent tracking

**Status**: ✅ **Fully specified and implemented**

---

**Document Version**: 1.0  
**Last Updated**: 2025-10-22  
**Next Review**: Post-mainnet (architecture refinements)
