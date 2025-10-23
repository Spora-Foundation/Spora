# Next Steps: VM Compilation + CellValidator Integration

**Current Status**: 90% Complete  
**Remaining**: 10% (VM compilation + integration + tests)  
**ETA**: 4-6 hours

---

## 🚧 Task 1: Fix Remaining VM Compilation Errors (1-2 hours)

### Current Status
- ✅ utils.rs - Fixed (CKB-style implementation)
- ✅ load_tx.rs - Fixed
- ✅ load_witness.rs - Fixed
- 🚧 Remaining: 7 syscalls need同样的更新

### Pattern to Apply

All syscalls need这three changes:

#### Change 1: Update imports
```rust
// Add SUCCESS to imports
use super::utils::{store_data, SUCCESS, INDEX_OUT_OF_BOUND};

// Simplify register imports (only what's needed)
use ckb_vm::registers::{A0, A2, A3, A4, A7};  // Example
```

#### Change 2: Update store_data calls
```rust
// OLD (our initial implementation):
let ret = store_data(machine, data, addr, len_ptr)?;
machine.set_register(A0, M::REG::from_u8(ret));

// NEW (CKB style):
store_data(machine, data)?;  // Reads A0,A1,A2 internally
machine.set_register(A0, M::REG::from_u8(SUCCESS));
```

#### Change 3: Fix register access
```rust
// OLD:
let index = machine.registers()[A3].to_usize();

// NEW:
let index = machine.registers()[A3].to_u64() as usize;
```

### Files to Update

1. **exec/src/vm/syscalls/load_cell.rs** (140 lines)
   - 3 places with store_data calls
   - Multiple register accesses

2. **exec/src/vm/syscalls/load_input.rs** (100 lines)
   - 1 store_data call
   - register accesses

3. **exec/src/vm/syscalls/load_script.rs** (60 lines)
   - 1 store_data call
   - register access

4. **exec/src/vm/syscalls/load_cell_data.rs** (70 lines)
   - 1 store_data call
   - register access

5. **exec/src/vm/syscalls/blake3.rs** (90 lines)
   - Special case: needs to read input data first
   - Then store output

6. **exec/src/vm/syscalls/current_cycles.rs** (40 lines)
   - No store_data, just set register

7. **exec/src/vm/syscalls/debugger.rs** (50 lines)
   - No store_data, just read + print

8. **exec/src/vm/syscalls/load_header.rs** (50 lines)
   - Placeholder, just return error

---

## 🔄 Task 2: Replace TransactionValidator with CellValidator (1 hour)

### File: `consensus/src/pipeline/virtual_processor/processor.rs`

#### Step 1: Remove TransactionValidator field (line ~158)
```rust
// DELETE:
pub(super) transaction_validator: TransactionValidator,

// Currently keeping it for compilation, will remove after replacement
```

#### Step 2: Add CellValidator field
```rust
// ADD after other managers:
pub(super) cell_validator: Arc<CellValidator<impl CellStateProvider>>,
```

#### Step 3: Update initialization in `new()` method
```rust
// In VirtualStateProcessor::new():
let cell_validator = Arc::new(CellValidator::new(
    Arc::new(CellConsensusParams::default()),
    // Provider will be implemented
));
```

#### Step 4: Replace validation calls
Search for all uses of `self.transaction_validator` and replace with `self.cell_validator`.

**Note**: This requires defining a proper CellStateProvider that can query the consensus stores.

---

## 🧪 Task 3: Add Comprehensive Tests (2-3 hours)

### File: `consensus/src/pipeline/virtual_processor/cell_tests.rs`

Currently ~50 lines, needs expansion to ~500+ lines.

### Test Scenarios

#### 1. Basic Cell Transaction
```rust
#[test]
fn test_simple_cell_transaction() {
    // Create genesis
    // Mine block A with coinbase
    // Mine block B spending A's coinbase
    // Verify cell_root updates correctly
}
```

#### 2. Multi-Parent DAG Block
```rust
#[test]
fn test_multi_parent_mergeset() {
    //     A
    //    / \
    //   B   C
    //    \ /
    //     D
    // D merges B and C
    // Verify mergeset processing
    // Verify no double-spend
}
```

#### 3. Cellbase Maturity
```rust
#[test]
fn test_cellbase_maturity_enforcement() {
    // Mine cellbase at DAA 100
    // Try spend at DAA 150 (should fail, maturity=100)
    // Try spend at DAA 201 (should succeed)
}
```

#### 4. Reorg with Cell State
```rust
#[test]
fn test_reorg_cell_state_consistency() {
    // Chain A: 1→2→3 (cells created)
    // Chain B: 1→2'→3'→4' (higher blue work)
    // Reorg to B
    // Verify cell state consistent
}
```

#### 5. Cell Root Verification
```rust
#[test]
fn test_cell_root_verification() {
    // Create block with known cell state
    // Calculate expected cell_root
    // Verify matches header
    // Test mismatch rejection
}
```

#### 6. Cell Commitment
```rust
#[test]
fn test_cell_commitment_v0() {
    // Verify cell_commitment = H("spora/cell_commitment/v0" || cell_root)
    // Test with known vectors
}
```

---

## 📋 Detailed Checklist

### VM Compilation Fix
- [ ] Update load_cell.rs (store_data calls)
- [ ] Update load_input.rs (store_data call)
- [ ] Update load_script.rs (store_data call)
- [ ] Update load_cell_data.rs (store_data call)
- [ ] Update blake3.rs (special handling)
- [ ] Update current_cycles.rs (register only)
- [ ] Update debugger.rs (no store_data)
- [ ] Update load_header.rs (placeholder)
- [ ] Run `cargo check --package spora-exec --features vm`
- [ ] Verify 0 errors

### CellValidator Integration
- [ ] Define CellStateProvider implementation
- [ ] Add cell_validator field to VirtualStateProcessor
- [ ] Initialize cell_validator in new()
- [ ] Replace transaction_validator calls
- [ ] Remove old transaction_validator field
- [ ] Run `cargo check --package spora-consensus`
- [ ] Verify compilation

### Testing
- [ ] Implement 6 test scenarios above
- [ ] Add edge case tests
- [ ] Run `cargo test --package spora-consensus`
- [ ] Verify all pass

### Final Verification
- [ ] cargo check (all packages)
- [ ] cargo test (all packages)
- [ ] Update spora.md with final status
- [ ] Mark ready for review

---

## 🎯 Success Criteria

### Completion (100%)
- ✅ All compilation errors fixed
- ✅ All tests passing
- ✅ No unimplemented! remaining
- ✅ No TODO(spora-critical) remaining
- ✅ CellValidator fully integrated
- ✅ Comprehensive test coverage

### Quality
- ✅ No shortcuts taken
- ✅ Full CKB compatibility maintained
- ✅ GhostDAG aware (DAA scores, multi-parent)
- ✅ Complete documentation
- ✅ Clean code (no deprecated remnants)

---

**Priority**: HIGH  
**Complexity**: MEDIUM  
**Risk**: LOW (all hard problems solved)  
**Confidence**: HIGH

**Ready to complete in next session!**

