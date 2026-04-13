# CellMeta Naming Conflict Resolution Plan

**Date**: 2025-10-22
**Status**: Planning Phase

---

## 📊 Current State: 6+ CellMeta Variants

### 1. **consensus/core/src/cell_diff.rs::CellMeta** ⭐ (Keep)
```rust
pub struct CellMeta {
    pub capacity: u64,
    pub lock_hash: [u8; 32],
    pub type_hash: Option<[u8; 32]>,
    pub data_hash: [u8; 32],
    pub block_daa_score: u64,
}
```
**用途**: Lightweight, used in CellDiff and merkle trees  
**大小**: 5 fields, ~100 bytes  
**决策**: **保持名称** - 这是最核心的Cell元数据

---

### 2. **consensus/core/src/cell_metadata.rs::CellMetadata** ✅ (Keep)
```rust
pub struct CellMetadata {
    // ... 11 fields including CellMeta fields + DAG info
    pub is_cellbase: bool,
    pub block_hash: Hash,
    // ... optional data fields
}
```
**用途**: Complete metadata for validators and queries  
**大小**: 11 fields, ~200 bytes  
**决策**: **保持名称** - 已经是独特的命名

---

### 3. **exec/src/celltx/types.rs::CellMeta** ⚠️ (Rename)
```rust
pub struct CellMeta {
    pub cell_output: CellOutput,
    pub out_point: OutPoint,
    pub transaction_info: Option<TransactionInfo>,
    pub data_bytes: u64,
    pub mem_cell_data: Option<Vec<u8>>,
}
```
**用途**: Exec layer general Cell info  
**大小**: 5 fields  
**决策**: **重命名为 `CellInfo`** (已部分完成)

---

### 4. **exec/src/vm/syscalls/load_cell.rs::CellMeta** ⚠️ (Rename)
```rust
pub struct CellMeta {
    pub cell_output: CellOutput,
    pub out_point: OutPoint,
    pub data: Option<Vec<u8>>,
}
```
**用途**: VM syscall - load_cell specific  
**大小**: 3 fields  
**决策**: **重命名为 `VmCellInfo` 或保持局部作用域**

---

### 5. **exec/src/vm/syscalls/load_cell_data.rs::CellMeta** ⚠️ (Rename)
```rust
pub struct CellMeta {
    pub out_point: OutPoint,
    pub data: Vec<u8>,
}
```
**用途**: VM syscall - load_cell_data specific  
**大小**: 2 fields  
**决策**: **重命名为 `CellDataInfo` 或保持局部作用域**

---

### 6. **state/src/index/cell_db.rs::CellMeta** ⚠️ (Rename)
```rust
pub struct CellMeta {
    pub cell_output: CellOutput,
    pub cell_data: Vec<u8>,
    pub daa_score: u64,
    pub block_hash: [u8; 32],
    pub is_cellbase: bool,
    pub segment_info: Option<SegmentInfo>,
}
```
**用途**: Database/state storage  
**大小**: 6 fields  
**决策**: **重命名为 `StoredCellMeta` 或 `DbCellMeta`**

---

## 🎯 Renaming Strategy

### Priority 1: Consensus Layer (Core) ✅
- ✅ **Keep**: `consensus/core/src/cell_diff.rs::CellMeta`
- ✅ **Keep**: `consensus/core/src/cell_metadata.rs::CellMetadata`

### Priority 2: Exec Layer
**Option A: Unified Approach**
```rust
// exec/src/celltx/types.rs
pub struct CellInfo {  // General purpose
    pub cell_output: CellOutput,
    pub out_point: OutPoint,
    // ...
}

// Type alias for compatibility
#[deprecated]
pub type CellMeta = CellInfo;
```

**Option B: Keep VM Syscalls Local**
```rust
// load_cell.rs, load_cell_data.rs - keep as local structs
// They are only used within their syscall modules
// No renaming needed - just ensure they're not pub exported
```

### Priority 3: State Layer
```rust
// state/src/index/cell_db.rs
pub struct StoredCellMeta {  // Clear: stored in database
    pub cell_output: CellOutput,
    pub cell_data: Vec<u8>,
    // ...
}
```

---

## 📋 Implementation Plan

### Step 1: Exec Layer (exec/src/celltx/types.rs)
- [x] Rename `CellMeta` → `CellInfo`
- [ ] Add deprecated type alias
- [ ] Update all usages in exec/

### Step 2: State Layer (state/src/index/cell_db.rs)
- [ ] Rename `CellMeta` → `StoredCellMeta`
- [ ] Update all usages in state/
- [ ] Update cellindex/ usages

### Step 3: VM Syscalls (Optional)
- [ ] Option A: Rename to `VmCellInfo`, `VmCellDataInfo`
- [ ] Option B: Keep as module-local, unexported structs

### Step 4: Update Imports
- [ ] Fix all import statements
- [ ] Run full test suite
- [ ] Verify no regressions

---

## ✅ Already Done

1. ✅ Added `CellMetadata` to consensus/core (new type, no conflicts)
2. ✅ Fixed all validator tests to use `CellMetadata`
3. ✅ Added type alias in `exec/src/celltx/types.rs`:
   ```rust
   pub type CellMeta = CellInfo;  // Deprecated
   ```

---

## 🚧 Remaining Work

1. **State Layer Renaming** (~30 minutes)
   - Rename in `state/src/index/cell_db.rs`
   - Update `indexes/cellindex/` imports

2. **VM Syscalls** (~15 minutes)
   - Decision: Keep as module-local or rename

3. **Testing** (~30 minutes)
   - Run full test suite
   - Fix any broken tests
   - Verify compilation

---

## 📊 Impact Analysis

### Files to Change (est.)
- `state/src/index/cell_db.rs` (1 file)
- `state/src/index/script_index.rs` (possibly)
- `indexes/cellindex/src/*.rs` (3-4 files)
- VM syscalls (3 files, if renaming)

### Tests to Update
- State layer tests (~5 tests)
- CellIndex tests (~3 tests)

### Estimated Time: 1-2 hours

---

## 🎯 Recommendation

**Immediate Action**: 
1. Rename `state::CellMeta` → `StoredCellMeta` (highest impact)
2. Keep VM syscall `CellMeta` as module-local (lowest risk)
3. Mark `exec::CellMeta` as deprecated (already done)

**Post-Refactor**:
- Document each variant's purpose clearly
- Add module-level comments explaining the distinction
- Consider future: unified Cell type hierarchy?


