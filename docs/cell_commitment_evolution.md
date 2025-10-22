# Cell Commitment Evolution Path

**Date**: 2025-10-22  
**Current Version**: v0  
**Status**: ✅ Designed for Evolution

---

## 1. Current Implementation (v0)

### cell_commitment Definition

```rust
// consensus/core/src/header.rs
pub struct Header {
    // ... other fields
    
    /// cell_root: Merkle root of all live cells
    /// - Direct state commitment
    /// - Used for validation
    pub cell_root: Hash,
    
    /// cell_commitment: Versioned commitment wrapper
    /// - v0: Simple hash of cell_root
    /// - Allows future extension
    pub cell_commitment: Hash,
}
```

### v0 Calculation

```rust
// Current formula:
cell_commitment = H("tondi/cell_commitment/v0" || cell_root)

// Where H = blake3 domain-separated hash
let mut hasher = blake3::Hasher::new();
hasher.update(b"tondi/cell_commitment/v0");
hasher.update(&cell_root.as_bytes());
cell_commitment = Hash::from_bytes(*hasher.finalize().as_bytes());
```

### v0 Verification

```rust
// Nodes verify:
1. Compute cell_root from selected_parent + block diff
2. Verify cell_root matches header.cell_root
3. Verify cell_commitment = H("tondi/cell_commitment/v0" || cell_root)
```

### Why Two Fields?

**cell_root**: 
- Primary state commitment
- Used in consensus validation
- Lightweight clients verify against this

**cell_commitment**:
- Versioned wrapper
- Allows adding more commitments later (e.g., history_root)
- Backward compatible upgrades

**Current**: `cell_commitment` is redundant (just wraps cell_root)  
**Future**: `cell_commitment` will aggregate multiple commitments

---

## 2. Future Versions

### v1: State + History Commitment (Planned)

**Motivation**: Enable historical state proofs

**New Field** (not in Header yet):
```rust
pub struct Header {
    // ... existing fields
    pub cell_root: Hash,           // Live cells (same as v0)
    pub history_root: Hash,        // NEW: Historical states Merkle tree
    pub cell_commitment: Hash,     // NEW formula (see below)
}
```

**v1 Calculation**:
```rust
// v1 formula:
cell_commitment = H("tondi/cell_commitment/v1" || cell_root || history_root)

// history_root: Merkle tree of all past cell_roots
// - Allows proving "cell_root was X at DAA Y"
// - Enables stateless validation
```

**history_root Construction**:
```rust
// Merkle tree of historical cell_roots
history_tree = MerkleTree::new();

for daa_score in 0..=current_daa {
    let root_at_daa = cell_roots_store.get(block_at_daa(daa_score));
    history_tree.insert(daa_score, root_at_daa);
}

history_root = history_tree.root();
```

**Benefits**:
- Light clients can verify historical states
- "Prove cell X existed at block Y" without full state
- Enables stateless nodes

**Costs**:
- Additional 32 bytes per header (history_root)
- Incremental Merkle tree overhead
- Backward compatibility complexity

### v2: Full Execution Commitment (Future)

**Motivation**: Commit to execution traces (fraud proofs)

**Formula**:
```rust
cell_commitment = H("tondi/cell_commitment/v2" 
    || cell_root 
    || history_root
    || execution_root)

// execution_root: Merkle tree of script execution traces
// - Cycles consumed
// - Syscalls made
// - Memory accessed
```

**Enables**:
- Fraud proofs for invalid execution
- Challenge-response games
- Layer 2 rollups

---

## 3. Backward Compatibility Strategy

### Version Detection

```rust
// Detect version from domain prefix:
fn detect_version(cell_commitment: Hash, cell_root: Hash, ...) -> u8 {
    // Try v0:
    let v0_commitment = H("tondi/cell_commitment/v0" || cell_root);
    if v0_commitment == cell_commitment {
        return 0;
    }
    
    // Try v1:
    if history_root is available {
        let v1_commitment = H("tondi/cell_commitment/v1" || cell_root || history_root);
        if v1_commitment == cell_commitment {
            return 1;
        }
    }
    
    // Unknown version
    return 0xFF;
}
```

### Soft Fork Activation

**Phase 1: Add history_root field** (consensus change)
```rust
// Old nodes: Ignore history_root, validate cell_root only
// New nodes: Compute and verify both cell_root and history_root
```

**Phase 2: Activate v1** (after majority adoption)
```rust
if current_daa >= V1_ACTIVATION_DAA {
    cell_commitment = v1_formula()
} else {
    cell_commitment = v0_formula()
}
```

**Phase 3: Drop v0** (after 100% adoption)
```rust
// Remove v0 code path
cell_commitment = v1_formula()  // Always
```

### Client Compatibility Matrix

| Client Version | v0 Blocks | v1 Blocks | v2 Blocks |
|----------------|-----------|-----------|-----------|
| v0-only        | ✅ Validate | ❌ Reject | ❌ Reject |
| v0+v1          | ✅ Validate | ✅ Validate | ❌ Reject |
| v0+v1+v2       | ✅ Validate | ✅ Validate | ✅ Validate |

**Light Clients**:
- v0: Can verify cell_root only
- v1: Can verify historical states
- v2: Can verify execution traces

---

## 4. Implementation Roadmap

### Phase 0: v0 (CURRENT) ✅

**Status**: Fully implemented

**Files**:
- `consensus/core/src/header.rs`: Dual fields (cell_root + cell_commitment)
- `consensus/src/pipeline/virtual_processor/cell_processing.rs`: v0 calculation
- Tests: 100% passing

**Formula**: `cell_commitment = H("tondi/cell_commitment/v0" || cell_root)`

### Phase 1: v1 Design (6-12 months)

**Requirements**:
1. **Incremental history tree**: Store cell_roots efficiently
2. **History indexing**: DAA score → cell_root lookup
3. **Light client proofs**: Merkle proof generation
4. **Soft fork mechanism**: Gradual activation

**Estimated Effort**: 4-6 weeks

**Deliverables**:
- `history_tree.rs`: Incremental Merkle tree (e.g., Jellyfish)
- `history_store.rs`: DAA → cell_root index
- `light_client.rs`: Historical proof API
- Tests: Fork scenarios, upgrade paths

### Phase 2: v2 Exploration (12-24 months)

**Requirements**:
1. **Execution traces**: Record syscalls, cycles, memory
2. **Fraud proofs**: Challenge-response protocol
3. **Optimistic rollups**: Layer 2 integration

**Research Needed**:
- Trace compression (execution overhead)
- Challenge period security
- Rollup compatibility

---

## 5. Security Considerations

### v0 Security

**Assumptions**:
- cell_root is honestly computed
- No historical state proofs
- Full nodes must replay all history

**Threats**:
- ❌ Light clients can't verify historical claims
- ❌ No fraud proofs for execution

**Mitigations**:
- ✅ Merkle tree prevents state forgery
- ✅ Deterministic computation (BTreeMap)

### v1 Security

**Additions**:
- ✅ Light clients verify historical states
- ✅ Stateless validation possible

**New Threats**:
- ⚠️ history_root forgery (if not properly anchored)
- ⚠️ Incremental tree bugs

**Mitigations**:
- Use well-tested tree (Jellyfish, Sparse Merkle)
- Comprehensive test coverage
- Gradual rollout

---

## 6. Code Examples

### v0 Implementation (Current)

```rust
// consensus/src/pipeline/virtual_processor/processor.rs

fn verify_expected_cell_state(
    &self,
    ctx: &mut CellProcessingContext,
    header: &Header
) -> Result<(), RuleError> {
    // Calculate cell_root from state tree
    let calculated_cell_root = ctx.get_cell_root();
    
    // v0: cell_commitment should equal cell_root (after domain hash)
    let expected_commitment = Self::compute_cell_commitment_v0(calculated_cell_root);
    
    if expected_commitment != header.cell_commitment {
        return Err(RuleError::BadCellCommitment);
    }
    
    Ok(())
}

fn compute_cell_commitment_v0(cell_root: Hash) -> Hash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"tondi/cell_commitment/v0");
    hasher.update(&cell_root.as_bytes());
    Hash::from_bytes(*hasher.finalize().as_bytes())
}
```

### v1 Implementation (Future)

```rust
// Future v1 implementation

fn verify_expected_cell_state_v1(
    &self,
    ctx: &mut CellProcessingContext,
    header: &Header
) -> Result<(), RuleError> {
    // Calculate cell_root (same as v0)
    let calculated_cell_root = ctx.get_cell_root();
    
    // Calculate history_root (new!)
    let calculated_history_root = self.compute_history_root(header.daa_score);
    
    // v1: cell_commitment = H(cell_root || history_root)
    let expected_commitment = Self::compute_cell_commitment_v1(
        calculated_cell_root,
        calculated_history_root
    );
    
    if expected_commitment != header.cell_commitment {
        return Err(RuleError::BadCellCommitment);
    }
    
    Ok(())
}

fn compute_cell_commitment_v1(cell_root: Hash, history_root: Hash) -> Hash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"tondi/cell_commitment/v1");
    hasher.update(&cell_root.as_bytes());
    hasher.update(&history_root.as_bytes());
    Hash::from_bytes(*hasher.finalize().as_bytes())
}

fn compute_history_root(&self, current_daa: u64) -> Hash {
    // Incremental Merkle tree of all cell_roots up to current_daa
    let mut tree = HistoryTree::new();
    
    for daa in 0..=current_daa {
        if let Some(root) = self.cell_roots_store.get_by_daa(daa) {
            tree.insert(daa, root);
        }
    }
    
    tree.root()
}
```

---

## 7. Migration Path

### Step 1: Add history_root field (Soft Fork)

```rust
// consensus/core/src/header.rs

pub struct Header {
    // ... existing fields
    pub cell_root: Hash,           // Existing
    pub history_root: Option<Hash>, // NEW! Optional for backward compat
    pub cell_commitment: Hash,     // Existing (formula changes based on version)
}
```

### Step 2: Dual validation period

```rust
const V1_ACTIVATION_DAA: u64 = 1_000_000; // Example

fn verify_cell_commitment(&self, header: &Header) -> Result<()> {
    if header.daa_score < V1_ACTIVATION_DAA {
        // v0 validation
        self.verify_v0(header)?;
    } else {
        // v1 validation
        if header.history_root.is_none() {
            return Err("v1 activated but history_root missing");
        }
        self.verify_v1(header)?;
    }
    Ok(())
}
```

### Step 3: Remove v0 code

```rust
// After 100% network adoption:
// - Remove v0 validation code
// - Make history_root required (not optional)
// - Simplify to v1-only
```

---

## 8. Performance Impact

### v0 (Current)

**Computation**:
```
cell_root calculation: O(n log n)  [n = live cells]
cell_commitment: O(1)  [single hash]

Total: O(n log n)  [dominated by Merkle tree]
```

**Storage**:
```
Header size: 217 bytes (no change from UTXO version)
Per-block storage: ~200 bytes (cell_diff + cell_root)
```

### v1 (Future)

**Computation**:
```
cell_root: O(n log n)  [unchanged]
history_root: O(log h)  [h = historical blocks, incremental tree]
cell_commitment: O(1)

Total: O(n log n + log h) ≈ O(n log n)  [still dominated by cell_root]
```

**Storage**:
```
Header size: 249 bytes (+32 for history_root)
Per-block storage: ~264 bytes (+32 for history_root, +32 for journal)
```

**Verdict**: Minimal overhead (~15% increase)

---

## 9. Testing Strategy

### v0 Tests (Current) ✅

```bash
cargo test --package tondi-consensus-core cell_diff
cargo test --package tondi-consensus virtual_processor
```

**Coverage**:
- ✅ cell_root calculation
- ✅ cell_commitment verification
- ✅ Reorg handling
- ✅ Merkle tree correctness

### v1 Tests (Future)

**Required**:
- history_root calculation
- Incremental tree updates
- Fork activation scenarios
- Light client proof generation
- Backward compatibility (v0 → v1 upgrade)

### v2 Tests (Research)

**Required**:
- Execution trace recording
- Fraud proof generation  
- Challenge-response games
- Rollup integration

---

## 10. Conclusion

### Current State (v0)

✅ **Fully implemented and tested**  
✅ **Backward compatible design**  
✅ **Clear evolution path**

### Recommendation

**Short-term** (0-6 months):
- ✅ Keep v0 as-is
- ✅ Monitor for issues
- ⏳ Research v1 design

**Medium-term** (6-12 months):
- ⏳ Implement v1 (history_root)
- ⏳ Test on testnet
- ⏳ Soft fork activation

**Long-term** (12-24+ months):
- 🔮 Research v2 (execution commitment)
- 🔮 Explore rollup integration
- 🔮 Advanced fraud proofs

### Design Quality

**Strengths**:
- ✅ Clear versioning scheme
- ✅ Domain separation
- ✅ Backward compatibility  
- ✅ Minimal overhead

**Weaknesses**:
- ⚠️ v0 doesn't provide historical proofs
- ⚠️ Incremental history tree needs careful design
- ⚠️ Soft fork activation complexity

**Overall**: ✅ **Excellent foundation for evolution**

---

**Version**: 1.0  
**Author**: Spora Team  
**Last Updated**: 2025-10-22  
**Next Review**: v1 design phase (6 months)

