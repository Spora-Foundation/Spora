# Typed Cell 执行总结

Branch: `spora-typed`

## 核心设计

本分支重置 scheduler witness 语义，以 typed cell 为中心。

**部署形态未定**：`spora-typed` 可能部署为联盟链、独立链、或 L2 rollup。本分支只做 typed-cell scheduler 语义，不预设任何特定部署形态。所有设计选择应同时兼容这三种路径。

旧 `binding_hash` 模型被移除，因为它混淆了两个不同概念：

- 稳定的冲突身份（用于调度）
- 可变的状态承诺（用于审计和状态根）

Typed cell 执行使用两个显式哈希：

```text
conflict_hash
    跨数据更新稳定
    blake3("spora-typed-cell/conflict-hash/v1" || code_hash || hash_type || args || conflict_key_value)
    用于 CellDAG 冲突检测

typed_data_hash
    数据变更时改变
    blake3("spora-typed-cell/typed-data-hash/v1" || code_hash || hash_type || args || data)
    用于审计和 typed-data 承诺
```

本分支不维护任何后向兼容层。

## Non-Goals

- 不与旧 `binding_hash` scheduler witness 后向兼容
- 不做 v1/v2 桥接
- 不将 `binding_hash` 别名为 `conflict_hash`
- 不保留 deprecated 字段
- 不保留 reserved-but-unsupported 枚举变体或操作码
- 不依赖 CellScript（runtime-first）
- 不预设部署形态（联盟链 / 独立链 / L2 rollup 均可）
- 不涉及 BFT、settlement、checkpoint、exit 模型

**Phase 2 计划**：见 [TYPED_CELL_PHASE2_PLAN.md](TYPED_CELL_PHASE2_PLAN.md)

---

## 已落地改动面

### 1. Typed Cell 分类类型

文件：`exec/src/celltx/types.rs`

六维分类体系：

| 维度 | 类型 | 值域 |
|------|------|------|
| Ownership | `CellOwnership` | Owned, Shared, Party, Immutable, Ephemeral |
| Mutability | `CellMutability` | Linear, Versioned, AppendOnly, Migratable |
| Accounting | `CellAccounting` | Fungible, NonFungible, Receipt, StorageClaim（多标签 Vec）——Phase 1 仅为分类元数据，不强制 CKB occupied capacity 验证 |
| Identity | `CellIdentity` | OutPoint, TypeId, Singleton, Field(String), Composite(Vec\<String\>) |
| Settlement | `CellSettlement` | LocalSettled, BridgeSettled, PendingSettlement |
  （命名不预设 L2：`LocalSettled` = 本链结算，`BridgeSettled` = 跨链结算，`PendingSettlement` = 待结算。不再使用 L2Only/RollupCommitted/ExitClaim） |
| ConflictKeySpec | `ConflictKeySpec` | CellId, Field(String), Composite(Vec\<String\>), Owner, None |

声明结构：`TypedCellDecl { ownership, mutability, accounting, identity, settlement, conflict_key }`

验证规则：mutable cell 使用 `ConflictKeySpec::None` 被拒绝。

### 2. 哈希计算

文件：`exec/src/celltx/types.rs`

- `compute_conflict_hash(type_script, conflict_key_value) → [u8; 32]`
  - blake3 域分离：`spora-typed-cell/conflict-hash/v1`
  - 跨数据更新稳定

- `compute_typed_data_hash(type_script, data) → [u8; 32]`
  - blake3 域分离：`spora-typed-cell/typed-data-hash/v1`
  - 数据变更时改变
  - 不包含 lock/capacity（非 cell_state_hash，Phase 2 再加）

- `encode_conflict_key_value_composite(fields: &[&[u8]]) → Vec<u8>`
  - 规范长度前缀编码：`len(field1_le_u32) || field1 || len(field2_le_u32) || field2 || ...`
  - 消除 `["ab","c"]` vs `["a","bc"]` 歧义

### 3. Scheduler Witness（clean break）

文件：`exec/src/celltx/types.rs`

**删除** `binding_hash`，替换为 `conflict_hash + typed_data_hash`。

- `TYPED_CELL_SCHEDULER_WITNESS_VERSION = 1`（非 v2，是版本重置）
- Access 记录从 38 字节扩展到 70 字节：`operation(1) + source(1) + index(4) + conflict_hash(32) + typed_data_hash(32)`
- `CellScriptSchedulerAccessWitness` 字段：`operation, source, index, conflict_hash, typed_data_hash`
- `CellScriptSchedulerWitness` 不再包含 `touches_shared` 字段（已删除）。冲突域由 access 级 `conflict_hash` 按 operation 精确分类：READ_REF→reads，其他→writes
- Molecule encode/decode 更新为 70 字节记录格式
- `SchedulerAccessKey` 从 `binding_hash` 更新为 `conflict_hash`
- `AccessSetMismatch` 错误变体从 `binding_hash` 更新为 `conflict_hash`

**Admission guard 语义边界**：
- `is_cellscript_scheduler_witness_bytes()` 仅做结构级验证（magic、version、counts）
- Operation/source 语义约束在 `validate_cellscript_scheduler_access_envelope()` 中强制执行
- 这确保非法 operation/source 组合的 witness 可以被推入交易，但在 `BlockAccessSummary` 阶段被拒绝

**Operation/source 合法组合**（Phase 1 仅保留 5 个操作码：CONSUME/TRANSFER/DESTROY/READ_REF/CREATE）：

```text
CONSUME / DESTROY             → source 必须是 INPUT
CREATE                        → source 必须是 OUTPUT
TRANSFER                      → source 必须是 INPUT 或 OUTPUT
READ_REF                      → source 必须是 CELL_DEP 或 INPUT
```

已删除的未发布操作码：CLAIM、SETTLE、MUTATE_INPUT、MUTATE_OUTPUT。尚未发布，无需保留。

### 4. CellDAG — conflict_hash + AccessMode 感知调度

文件：`exec/src/scheduler/dag.rs`

**AccessMode 设计**：

```rust
enum AccessMode {
    Read,   // READ_REF — 只读访问，不修改 cell
    Write,  // CONSUME / CREATE / DESTROY / TRANSFER — 修改 cell
}
```

映射规则：`READ_REF` → `Read`，其他 4 个操作码 → `Write`。

**ConflictEntry 设计**：

```rust
struct ConflictEntry {
    node_id: u32,       // DAG 中的交易节点索引
    mode: AccessMode,   // 该交易的访问模式
}
```

**调度数据结构**：

```rust
conflict_hash_conflicts: BTreeMap<[u8; 32], Vec<ConflictEntry>>
```

每个 conflict_hash 对应一组 `ConflictEntry`，记录哪些交易访问了该冲突域以及访问模式。

**API**：
- `build_from_typed(txs)` — 基于 conflict_hash 的调度方法
- `extract_conflict_accesses(tx)` — 提取 conflict_hash/AccessMode 对
- `can_parallel(accesses_a, accesses_b)` — 判断两个 tx 是否可并行

**冲突规则**：

```text
READ  + READ  同 conflict_hash → 同层（无依赖边）
READ  + WRITE 同 conflict_hash → 依赖边
WRITE + WRITE 同 conflict_hash → 依赖边（不同拓扑层）
不同 conflict_hash → 并行
```

### 5. TypedCellStore

文件：`exec/src/celltx/types.rs`

- `ScriptId`：`code_hash + hash_type + args_hash`（blake3），而非仅 code_hash
- `TypedCellStore` trait：`get_decl` / `insert_decl`
- `InMemoryTypedCellStore`：BTreeMap\<ScriptId, TypedCellDecl\>

### 6. BlockAccessSummary

文件：`consensus/src/pipeline/virtual_processor/access_summary.rs`

- `cellscript_shared_reads` / `cellscript_shared_writes` 从 `binding_hash` 键改为 `conflict_hash` 键
- `merge_cellscript_scheduler_witness` 不再从 `touches_shared` 提取冲突域，改为从 access 记录按 operation 精确分类：
  - `READ_REF` → `cellscript_shared_reads`
  - `CONSUME / CREATE / DESTROY / TRANSFER` → `cellscript_shared_writes`
- 所有测试代码更新

### 7. Re-exports

| 文件 | 新增导出 |
|------|---------|
| `exec/src/celltx/mod.rs` | `CellOwnership, CellMutability, CellAccounting, CellIdentity, CellSettlement, ConflictKeySpec, TypedCellDecl, TypedCellDeclError, TypedCellStore, InMemoryTypedCellStore, ScriptId, compute_conflict_hash, compute_typed_data_hash, encode_conflict_key_value_composite, validate_typed_cell_decl, TYPED_CELL_SCHEDULER_WITNESS_VERSION` |
| `exec/src/lib.rs` | 对应顶层 re-export |
| `exec/src/scheduler/mod.rs` | `AccessMode, ConflictEntry` |

### 8. Workspace 全量 binding_hash → conflict_hash 迁移

| 文件 | 改动 |
|------|------|
| `mining/src/block_template/selector.rs` | 测试：`binding_hash` → `conflict_hash + typed_data_hash` |
| `mining/src/manager_tests.rs` | 测试：`binding_hash` → `conflict_hash + typed_data_hash` |
| `wallet/core/src/tx/generator/generator.rs` | 测试：`binding_hash` → `conflict_hash + typed_data_hash`（2 处） |
| `consensus/src/pipeline/virtual_processor/processor.rs` | 测试 helper：`binding_hash` → `conflict_hash` |

### 9. Workspace 构建修复

| 文件 | 改动 |
|------|------|
| `Cargo.toml`（workspace root） | 移除 `cellscript` workspace member 和 dependency |
| `testing/integration/Cargo.toml` | 移除 `cellscript` dependency |

### 10. 预存测试修复

| 文件 | 改动 |
|------|------|
| `exec/tests/serialization_integration.rs` | Scenario 2: negotiate 不支持降级，改为 `assert!(is_err())` |
| `exec/tests/vm_abi_integration.rs` | ResolvedHeader/Cell ABI 版本从 `BORSH_V1` 更新为 `MOLECULE_V1` |
| `consensus/src/pipeline/body_processor/body_validation_in_isolation.rs` | mass 超限测试：witness 大小从 600KB 更新为 2MB（适配 `max_block_mass = 2_000_000`） |

---

## 语义规范

### conflict_key_value 规范编码

`conflict_key_value` 必须使用规范编码。复合键禁止原始拼接（避免 `["ab","c"]` vs `["a","bc"]` 歧义）。

```text
conflict_key_value = len(field1_le_u32) || field1 || len(field2_le_u32) || field2 || ...
```

单字段键可用原始字节；复合键必须使用此规范形式。

CellScript 将在 Step 3 中将 `#[conflict_key(composite(...))]` 降级为规范 conflict_key_value 字节。

### CellIdentity 与 ConflictKeySpec 独立性

`CellIdentity` 和 `ConflictKeySpec` 是独立轴，不可隐式等同。示例：orderbook cell：

```text
identity     = order_id    （每订单唯一）
conflict_key = market_id   （按市场分组，实现并行执行）
```

默认规则：

```text
owned mutable:   默认 conflict_key = CellId（一个 cell，无共享冲突）
shared mutable:  conflict_key 必须显式声明
immutable:       conflict_key = None（无写冲突）
ephemeral:       conflict_key = None（不进入调度器）
```

### typed_data_hash 与 cell_state_hash 的区别

Phase 1 使用 `typed_data_hash`（仅覆盖 type_script identity + data），不包含 lock/capacity。

Phase 2 将添加 `cell_state_hash = blake3(domain || capacity || lock_hash || type_hash || data_hash)`，两者共存。

它们是层级关系，非替代关系：

```text
typed_data_hash = typed data 层（type_script identity + data）
cell_state_hash = full cell state 层（capacity + lock + type + data）
state_root      = collection 层（多 cell state 聚合）
```

typed_data_hash 用于类型化审计和 VM 级 typed payload 检查；cell_state_hash 用于 full state-root / settlement 承诺。

### Scheduler Witness 校验规则

- `magic` 必须等于 `0xCE11`
- `version` 必须等于 `1`（`TYPED_CELL_SCHEDULER_WITNESS_VERSION`）
- `access_count == accesses.len()`
- `access_count <= MAX_CELLSCRIPT_ACCESS_COUNT`（当前为 256，防内存溢出攻击）
- 全零 `conflict_hash` 在 typed-cell 模式下非法——`validate_cellscript_scheduler_access_envelope` 直接拒绝，`merge_cellscript_scheduler_witness` 中有 debug_assert 防御
- 所有 typed-cell 访问必须携带非零 `typed_data_hash`。全零 `typed_data_hash` 在 typed-cell 模式下默认非法——本 branch 只做 typed cell，没有理由允许模糊空间
- Operation/source 组合必须合法
- 任何能产生 Write 模式的非 ephemeral 访问不得使用 `ConflictKeySpec::None`
  （比按 `CellMutability` 枚举更准确：Linear burn 也是 mutable）
- Runtime witness 必须与 trusted summary 完全匹配

### Conflict Key 语义完整性

`conflict_hash` 正确性依赖于 `conflict_key_value` 从 typed cell 的协议语义中正确推导。运行时无法验证这一点——它只对接收到的数据做哈希。因此：

- 共享可变 cell 必须声明显式 `ConflictKeySpec`（非 None）
- `ConflictKeySpec::Field` / `Composite` 值必须由编译器验证覆盖所有写冲突状态
- 运行时强制：可变 cell 使用 `ConflictKeySpec::None` 被拒绝
- 编译器强制：`#[conflict_key]` 覆盖率检查（Step 3）

---

## 测试覆盖

### exec/src/celltx/types.rs（22 个 typed cell 测试）

- compute_conflict_hash 确定性
- compute_typed_data_hash 确定性
- conflict_hash 跨数据更新稳定性
- conflict_hash 不同 conflict_key_value 产生不同哈希
- typed_data_hash 不同 data 产生不同哈希
- conflict_hash 不同 script 产生不同哈希
- encode_conflict_key_value_composite 规范编码
- encode_conflict_key_value_composite 消歧
- encode_conflict_key_value_composite 空输入
- validate_typed_cell_decl 拒绝 mutable+None
- validate_typed_cell_decl 拒绝 shared mutable+None
- validate_typed_cell_decl 接受 immutable+None
- validate_typed_cell_decl 接受 ephemeral+None
- validate_typed_cell_decl 接受 owned+CellId
- TypedCellDecl Borsh 序列化 round-trip
- ScriptId 派生和不同 args 区分
- InMemoryTypedCellStore 插入/查询 round-trip
- scheduler witness encode/decode round-trip
- conflict_hash 稳定性（witness 级别）
- validate_summary 拒绝伪造 conflict_hash
- shared cell 必须声明显式 conflict_key
- composite conflict_key 与 field conflict_key 产生不同哈希
- operation/source 验证组合测试

### exec/src/scheduler/dag.rs（6 个 typed cell DAG 测试）

- WRITE+WRITE 同 conflict_hash → 依赖边
- READ+READ 同 conflict_hash → 同层
- READ+WRITE 同 conflict_hash → 依赖边
- 不同 conflict_hash → 并行
- can_parallel 工具函数
- 混合冲突域（AMM pool A/B 场景）

### 全 workspace 测试状态

`cargo test --workspace` — **零失败**

---

## 待执行（CellScript 侧）

CellScript 目前不在 `spora-typed` 分支的 workspace 中。待 CellScript 稳定后，**最小接入顺序**：

1. `TargetProfile::TypedCell` 枚举变体
2. conflict_key canonical encoding
3. `generate_molecule_typed_cell()` — blake3 域分离、70 字节访问记录

4. trusted summary / scheduler-plan 生成
5. identity/conflict_key 语义检查

**延后项**（不在 CellScript 接入首批）：

- `#[settlement]` 仅 parse 不 enforce，或直接延后 — settlement 语义取决于部署形态
- BFT committee / checkpoint cells
- Exit model / bridge verification
- CLAIM / SETTLE / MUTATE 操作码 — 按需重新引入

接入原则：先让 CellScript 正确、优雅、可审计地产生 runtime 事实，再扩展元数据维度。

---

## Out of Scope

- 共识机制选择（BFT / PoW / PoS / 联盟）——取决于部署形态
- 跨链结算 / bridge 语义
- 完整 cell_state_hash（Phase 2 — 见 typed_data_hash 与 cell_state_hash 的区别）
- Conflict key 覆盖率验证（编译器强制，非运行时强制）
- CLAIM / SETTLE / MUTATE 操作码（按需重新引入）
