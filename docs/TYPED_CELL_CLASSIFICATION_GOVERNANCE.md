# Typed Cell 六维分类治理

Branch: `spora-typed`

## 1. 背景

`spora-typed` 分支已完成 typed-cell scheduler witness 的核心重置：旧 `binding_hash` 被移除，新的执行语义明确拆分为 `conflict_hash` 与 `typed_data_hash`。当前分支明确不维护旧 witness 的后向兼容层，也不引入 BFT、settlement、checkpoint、exit 或 CellScript 依赖。

当前 typed cell 采用六维正交分类：

```text
Ownership
Mutability
Accounting
Identity
Settlement
ConflictKeySpec
```

理论组合空间较大，但 Phase 1 的 runtime 直接消费重点是：

```text
Ownership
ConflictKeySpec
```

其余维度主要是编译器、manifest、ProofPlan、settlement 和后续审计层的语义材料。

**本治理文档的目标不是继续增加抽象，而是：**

```text
保留六维内部模型；
收紧明显非法组合；
明确 enforcement level；
避免把六维裸露成用户心智负担；
不在 runtime core 中引入 archetype。
```

---

## 2. 设计原则

### 2.1 六维是内部坐标系，不是用户菜单

六维分类应被视为 runtime/compiler 之间的 normalized metadata，而不是开发者每天手工组合的 public API。

```text
TypedCellDecl = internal semantic coordinate system
```

用户未来在 CellScript 层看到的应该是更自然的声明、lint、模板和 audit view，而不是被迫理解所有组合。

### 2.2 现在不引入 `TypedCellArchetype`

不建议当前在 runtime core 中加入 archetype 枚举。原因：

1. archetype 与六维谁是 source of truth 会变复杂
2. archetype override 会引入新一层约束系统
3. Spora / Hypha / Axone 可能有不同 archetype 语义
4. runtime core 应保持机械、低层、可测
5. archetype 更适合未来 CellScript sugar / docs / audit UI

当前最稳路线：

```text
六维保留；
交叉约束补齐；
文档标注 common patterns；
archetype 暂不进入核心代码。
```

### 2.3 先 hard constraints，后 compiler semantics

Phase 1 只应该拒绝明显非法或危险组合。复杂业务语义——fungible conservation、receipt claim、storage capacity、exit inclusion proof——应留给 CellScript / ProofPlan / Axone / Hypha 后续层处理。

---

## 3. Enforcement Level 分层

| 维度 | Phase 1 enforcement | 说明 |
|------|---------------------|------|
| `Ownership` | runtime-enforced / partially | 决定 immutable、ephemeral、shared 等基础调度边界 |
| `ConflictKeySpec` | runtime-enforced | 直接推导 `conflict_hash`，决定 CellDAG 冲突域 |
| `Mutability` | validator-enforced / advisory | Phase 1 只做非法组合检查，未来进入 compiler / ProofPlan |
| `Accounting` | validator-enforced / advisory | Phase 1 做标签互斥检查，未来做守恒、claim、storage checks |
| `Identity` | manifest / compiler-enforced | 用于 update pairing、settlement、exit、artifact metadata |
| `Settlement` | manifest / future-enforced | Phase 1 是标签，后续由 Axone / Hypha / checkpoint / exit 层消费 |

原则：

```text
runtime 保证调度安全；
compiler 保证语义覆盖；
ProofPlan 解释证明义务；
settlement 层处理外部终局性。
```

---

## 4. 已落地的交叉约束

### 4.1 `Fungible` 与 `NonFungible` 互斥

同一个 cell 不能同时被解释为可分割同质资产和非同质唯一资产。

### 4.2 `Immutable` 与可变迁移模式互斥

`Immutable` ownership 不得搭配 `Versioned` / `AppendOnly` / `Migratable`。仅 `Linear` 合法（且不进入 Write path）。

### 4.3 `Ephemeral` 不得参与外部/根承诺结算

`Ephemeral` ownership 必须使用 `Local` settlement。批内临时状态不应进入长期 root、bridge、checkpoint 或 exit 语义。

### 4.4 可写 cell 不得使用 `ConflictKeySpec::None`

任何能产生 Write 模式的非 ephemeral 访问必须声明 conflict domain。`None` 对可写 cell 是绕过冲突检测的洞。

### 4.5 `Shared` mutable cell 必须显式 conflict key

`ConflictKeySpec::CellId` 对 shared cell 不安全——shared state 的冲突域通常是协议级 key（如 `pool_id`、`market_id`、`oracle_id`）。

---

## 5. 保留但降权的维度

### 5.1 `Party`

`Party` 是 bounded shared-state marker。Phase 1 调度等价于 `Shared`：`conflict_hash` 控制串行化。

未来层可能用 Party metadata 做 participant-set access control、privacy、governance、multi-party workflow 语义。

不建议删除，但必须标注为 advisory。

### 5.2 `Mutability`

`Mutability` 不直接影响 CellDAG 调度，但不应删除。它是后续编译器和 ProofPlan 的语义入口：

```text
Linear      -> consume/create pairing
Versioned   -> version + 1 obligation
AppendOnly  -> append cursor / length obligation
Migratable  -> data layout migration policy
```

Phase 1 只做明显非法组合验证。未来可考虑为 Immutable 新增 `CellMutability::Static`，但当前不扩大变更面。

### 5.3 `Settlement`

维持三值：`Local / Committed / Pending`。

| 值 | 语义 |
|----|------|
| `Local` | 在当前执行环境内完成，不声明外部结算 |
| `Committed` | 参与 root commitment / historical proof / optional anchoring |
| `Pending` | 等待后续 claim / exit / bridge / settlement finalisation |

不要过早引入 `CkbSettled` / `BridgeSettled` / `ConsortiumSettled` / `ValiditySettled`——这些属于 Axone / Hypha / settlement backend 层。

---

## 6. 语义区别

### 6.1 `CellIdentity` vs `ConflictKeySpec`

| 轴 | 回答的问题 | 示例 |
|----|-----------|------|
| `CellIdentity` | 这个 cell 是谁？ | `order_id` |
| `ConflictKeySpec` | 哪些交易必须串行？ | `market_id` |
| `typed_data_hash` | 这个 cell 当前数据是什么？ | `hash(type + data)` |

**Identity 回答"它是谁"；ConflictKeySpec 回答"它和谁不能并行"。** 两者不可隐式等同。

### 6.2 `typed_data_hash` vs future `cell_state_hash`

Phase 1 使用 `typed_data_hash`（仅覆盖 type_script identity + data），不包含 lock/capacity。命名诚实。

Phase 2 可增加 `cell_state_hash = hash(domain || capacity || lock_hash || type_hash || data_hash)`，两者共存：

| Hash | 覆盖范围 | 用途 |
|------|---------|------|
| `typed_data_hash` | type identity + data | typed payload audit / VM-level typed data commitment |
| `cell_state_hash` | capacity + lock + type + data | full state root / settlement commitment |
| `state_root` | collection of live cells | global state commitment |

---

## 7. 不建议当前落地的内容

### 7.1 不引入 runtime `TypedCellArchetype`

暂不新增 archetype 枚举。archetype 更适合未来 CellScript sugar / docs / audit UI，不应进入 runtime core。

### 7.2 不拆分 `TypedCellDecl` struct

`TypedCellDecl` 在 struct 形状上六维同级并列，但语义上不是同权。当前最小改动是：

```text
加 enforcement level 注释
validator 内部分层
```

如果未来误解问题持续，可考虑拆为 `RuntimeCellSemantics { ownership, conflict_key }` + `TypedCellSemanticMetadata { mutability, accounting, identity, settlement }`。但当前不扩大变更面。

---

## 8. 已删除的变体

| 变体 | 删除原因 |
|------|---------|
| `ConflictKeySpec::Owner` | owner-level 串行化不是 conflict-key 原语，应显式用 `Field("owner")` 或 `Composite` |
| `CellSettlement::LocalSettled` | → `Local` |
| `CellSettlement::BridgeSettled` | → `Committed` |
| `CellSettlement::PendingSettlement` | → `Pending` |
| `CELLSCRIPT_SCHEDULER_OP_CLAIM` | 未发布，Phase 1 不需要 |
| `CELLSCRIPT_SCHEDULER_OP_SETTLE` | 未发布，Phase 1 不需要 |
| `CELLSCRIPT_SCHEDULER_OP_MUTATE_INPUT` | 未发布，Phase 1 不需要 |
| `CELLSCRIPT_SCHEDULER_OP_MUTATE_OUTPUT` | 未发布，Phase 1 不需要 |
