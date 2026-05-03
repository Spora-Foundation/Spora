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

对 Immutable cell，`Linear` 只表示创建时物化（creation-time materialisation）；创建后它是只读对象，不得出现在 Write-classified access 中。

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
| `Committed` | 参与 root commitment / historical proof / optional anchoring。不等于已外部终局化，只表示该 cell 进入承诺根；外部终局性由 Axone/Hypha settlement 层处理 |
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

### 7.2 TypedCellDecl struct 已拆分

`TypedCellDecl` 已拆分为两个子 struct：

```rust
pub struct RuntimeCellSemantics {
    pub ownership: CellOwnership,
    pub conflict_key: ConflictKeySpec,
}

pub struct TypedCellSemanticMetadata {
    pub mutability: CellMutability,
    pub accounting: Vec<CellAccounting>,
    pub identity: CellIdentity,
    pub settlement: CellSettlement,
}

pub struct TypedCellDecl {
    pub runtime: RuntimeCellSemantics,
    pub semantic: TypedCellSemanticMetadata,
}
```

这使两个 enforcement tier 在类型层面就物理隔离，防止 runtime 层意外消费 advisory 维度。

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

---

## 9. VM / CellScript / ProofPlan 边界规则

`TypedCellDecl` 是 runtime/compiler 之间的 normalized metadata，不是独立语义权威。
为防止它越权与 VM、CellScript、ProofPlan 冲突，定义三条硬规则和一条 anti-override 规则。

### 9.1 三条硬规则

```text
Rule 1: VM never consumes typed-cell semantic axes.
        VM only executes.

Rule 2: Runtime consumes only scheduling-critical metadata:
        ownership + conflict_key + witness envelope.

Rule 3: CellScript/ProofPlan are the semantic source of truth.
        TypedCellDecl is generated/normalised metadata,
        not an independent language.
```

中文：

```text
VM 只执行，scheduler 才调度，CellScript 才声明，ProofPlan 才解释。
```

#### Rule 1: VM 边界

VM 层只知道：

```text
load cell
load witness
run script
return success/failure
consume cycles
```

VM 不应该理解 `Ownership`、`Mutability`、`Accounting`、`Identity`、`Settlement`、`ConflictKeySpec`。
这些属于 execution scheduler / metadata / compiler semantic layer，不是 VM opcode 或 syscall 语义。

如果 VM 开始理解 `Fungible`、`Receipt`、`Settlement`，那就越权了。

#### Rule 2: Runtime 边界

Runtime 直接消费的只有 `RuntimeCellSemantics`：

```text
ownership  → conflict_hash → CellDAG
conflict_key → conflict_hash → CellDAG
witness envelope → access records → BlockAccessSummary
```

`TypedCellSemanticMetadata` 中的四个 advisory 维度（mutability, accounting, identity, settlement）在 Phase 1
只做交叉约束验证，不被 runtime scheduler 消费。

#### Rule 3: CellScript / ProofPlan 边界

`TypedCellDecl` 不应成为和 CellScript 平级的第二套语义系统。正确路径：

```text
CellScript source
    ↓
semantic checker
    ↓
ProofPlan
    ↓
TypedCellDecl / scheduler witness / manifest
    ↓
runtime validation
```

错误路径：

```text
CellScript source
TypedCellDecl hand-written config
runtime tries to reconcile both
```

CellScript 是 source of intent；TypedCellDecl 是 lowered metadata。
用户不应该手写两套互相可能矛盾的东西。

### 9.2 Anti-override 规则

```text
TypedCellDecl must not introduce verifier semantics
that are not derivable from CellScript source,
ProofPlan obligations, or runtime scheduler requirements.
```

中文：

**TypedCellDecl 不得发明 CellScript 源码、ProofPlan 或 runtime scheduler 要求之外的新验证语义。**

这条规则防止 TypedCellDecl 越权。例如：

- TypedCellDecl 不能自己发明新的 accounting 约束（如守恒规则）——守恒由 ProofPlan 表达
- TypedCellDecl 不能自己发明新的 settlement 终局性——终局性由 Axone/Hypha 层处理
- TypedCellDecl 不能自己发明新的 ownership 语义——ownership 由 CellScript 声明 + runtime 调度消费

### 9.3 最容易越权的维度

| 维度 | 风险 | 制约
|------|------|------|
| `Mutability` | 容易和 CellScript 的 `action input → output`、`move`、`consume/create` 冲突 | 只能是 "expected transition pattern"，真正证明来自 action signature
| `Accounting` | 容易和 `receipt`、`claim`、`settle` 冲突 | 只能是 tag，不能自动生成会计规则；守恒、claim、redeem 必须由 ProofPlan/CellScript 约束
| `Settlement` | 最容易和 Axone/CKB 语义冲突 | Phase 1 只做 metadata，不 runtime enforce；真正 settlement 由 Axone checkpoint/exit/CKB scripts 处理

### 9.4 CellScript 未来接入方式

CellScript 未来不要直接暴露六维全量配置。
它应该从现有语言概念推导：

```cellscript
#[conflict_key(pool_id)]
#[identity(field(pool_id))]
#[settlement(committed)]
shared Pool has store {
    pool_id: Hash
    reserve_a: u128
    reserve_b: u128
}
```

然后编译器生成：

```text
ownership = Shared
conflict_key = Field(pool_id)
identity = Field(pool_id)
settlement = Committed
mutability = Versioned / Linear, inferred or explicit
accounting = inferred/tagged
```

这样 `TypedCellDecl` 是 **CellScript lowering artifact**，不是和 CellScript 平级的第二套语言。

---

## 10. Runtime Scheduling Metadata vs TypedCellDecl

### 10.1 精确定义

```text
Runtime scheduling metadata 是调度器消费的最小交易级信息：
operation、source、index、conflict_hash、typed_data_hash 和 read/write access mode。

TypedCellDecl 是每类 typed cell 的归一化语义声明，
用来推导 runtime scheduling metadata，并支持 manifest 校验。
Phase 1 中，只有 ownership 和 conflict_key 是 runtime 调度关键轴；
mutability、accounting、identity、settlement 作为 compiler、
ProofPlan、audit 和未来 settlement 层的语义元数据保留。
```

一句话：

**Runtime scheduling metadata 是“这笔交易怎么排队”；TypedCellDecl 是“这种 Cell 应该怎么被理解”。**

前者是执行调度输入，后者是语义声明和编译产物。

### 10.2 推导链

```text
TypedCellDecl (per-cell-type semantic declaration)
    ↓ derive / validate / lower
Runtime scheduling metadata (per-transaction access witness)
    ↓ consume
CellDAG / scheduler / BlockAccessSummary
```

**TypedCellDecl 是完整说明书；runtime scheduling metadata 是从说明书里抽出来的调度卡片。**

### 10.3 Runtime scheduling metadata 回答的问题

```text
这笔交易读写了哪些 typed cell？
哪些读写会冲突？
哪些可以并行？
哪些必须排序？
```

它不关心：

```text
这个 cell 是不是 receipt？
是不是 debt？
是不是以后要 bridge settlement？
是不是有商业含义？
是不是 invoice？
```

它只关心：

```text
这个 access 的 conflict_hash 是什么？
是 Read 还是 Write？
operation/source 合不合法？
witness 是否和 trusted summary 匹配？
```

### 10.4 Runtime scheduling metadata 的职责边界

**应该做：**

```text
1. 检查 witness 结构合法
2. 检查 operation/source 合法
3. 检查 conflict_hash 是否匹配 trusted summary
4. 构造 read/write access set
5. 构造 CellDAG dependency edge
6. 生成 BlockAccessSummary
```

**不应该做：**

```text
1. 解释 invoice_id 是什么
2. 判断 AMM pool 公式是否正确
3. 判断 ReceiptCell 是否可 claim
4. 判断 StorageClaim 是否满足 CKB occupied capacity
5. 判断 settlement 是否真的完成
6. 替代 CellScript / ProofPlan 做业务验证
```

**Runtime scheduling metadata 是调度索引，不是业务规则。**

### 10.5 实例：AMM Pool

CellScript 声明：

```cellscript
#[conflict_key(pool_id)]
#[identity(field(pool_id))]
#[settlement(committed)]
shared Pool has store {
    pool_id: Hash
    version: u64
    reserve_a: u128
    reserve_b: u128
}
```

编译器归一化为 TypedCellDecl：

```rust
TypedCellDecl {
    runtime: RuntimeCellSemantics {
        ownership: Shared,
        conflict_key: Field("pool_id"),
    },
    semantic: TypedCellSemanticMetadata {
        mutability: Versioned,
        accounting: vec![],
        identity: Field("pool_id"),
        settlement: Committed,
    },
}
```

某笔 swap 交易访问 Pool A 时的 runtime scheduling metadata：

```rust
CellScriptSchedulerAccessWitness {
    operation: Consume,       // write-classified operation
    source: Input,
    index: 0,
    conflict_hash: H(Pool type_script || pool_id=A),
    typed_data_hash: H(Pool type_script || data_before),
}
```

Scheduler 只看：

```text
operation = Write
conflict_hash = X
```

调度结果：

```text
Tx1 swap Pool A, Tx2 swap Pool A
=> same conflict_hash, Write+Write, dependency edge

Tx1 swap Pool A, Tx2 swap Pool B
=> different conflict_hash, parallel
```

**Scheduler 不需要知道它是 AMM、reserve、fee、pricing curve。**

### 10.6 实例：Invoice Receipt

CellScript 声明：

```cellscript
receipt FinancingReceipt has store {
    receipt_id: Hash
    invoice_id: Hash
    funder: Address
    amount: u128
}
```

TypedCellDecl：

```rust
TypedCellDecl {
    runtime: RuntimeCellSemantics {
        ownership: Owned,
        conflict_key: Field("receipt_id"),
    },
    semantic: TypedCellSemanticMetadata {
        mutability: Linear,
        accounting: vec![Receipt],
        identity: Field("receipt_id"),
        settlement: Committed,
    },
}
```

Runtime scheduling metadata 只关心：

```text
receipt_id 对应 conflict_hash
CREATE receipt 是 Write
```

ProofPlan 会关心：

```text
invoice 是否重复融资？
funder 是否签名？
amount 是否匹配？
oracle proof 是否存在？
```

```text
TypedCellDecl 告诉系统这是什么类型的 cell；
ProofPlan 告诉系统这笔 action 应该检查什么；
runtime metadata 告诉 scheduler 怎么调度。
```

### 10.7 Validator 分层实现

`validate_typed_cell_decl` 已拆为两层：

```rust
pub fn validate_typed_cell_decl(decl: &TypedCellDecl) -> Result<(), TypedCellDeclError> {
    check_runtime_scheduling_rules(decl)?;    // 只检查 decl.runtime.*
    check_semantic_consistency_rules(decl)?;  // 检查 decl.runtime.* + decl.semantic.* 交叉约束
    Ok(())
}
```

```rust
fn check_runtime_scheduling_rules(decl: &TypedCellDecl) {
    // 可写 cell 不得使用 ConflictKeySpec::None
    // 只访问 decl.runtime.ownership 和 decl.runtime.conflict_key
}

fn check_semantic_consistency_rules(decl: &TypedCellDecl) {
    // Immutable + mutable mutability 拒绝
    // Fungible + NonFungible 互斥
    // Ephemeral + non-Local settlement 拒绝
    // 访问 decl.runtime.* 和 decl.semantic.*
}
```

这样不会让人以为六维同级 runtime enforced。

---

## 11. TypedCellDecl 所有权与生成链

### 11.1 核心原则

```text
TypedCellDecl 的“语义来源”在 CellScript 端；
它的“执行格式”在 Spora runtime 端。
```

**TypedCellDecl 是 Spora protocol 的 metadata contract；CellScript 是它的 authoring frontend。**

### 11.2 三层架构

```text
┌─────────────────────────────────────────────────────────────────┐
│  Layer A: Spora protocol crate / runtime crate                  │
│  - canonical TypedCellDecl schema (Rust definition)             │
│  - compute_conflict_hash / compute_typed_data_hash              │
│  - validate_typed_cell_decl                                      │
│  - encode_typed_cell_manifest                                    │
│  = truth of wire / manifest format                               │
├─────────────────────────────────────────────────────────────────┤
│  Layer B: CellScript Spora / TypedCell profile                   │
│  - parse attributes (#[conflict_key], #[identity], #[settlement])│
│  - infer ownership from resource/shared/receipt                  │
│  - check conflict_key field exists                               │
│  - canonical encode composite conflict keys                      │
│  - emit TypedCellDecl manifest                                   │
│  - emit scheduler witness template                               │
│  - emit ProofPlan / artifact_set metadata                        │
│  = truth of source semantics                                     │
├─────────────────────────────────────────────────────────────────┤
│  Layer C: Runtime scheduler                                      │
│  - consumes: operation, source, index, conflict_hash,            │
│    typed_data_hash, AccessMode                                    │
│  - does NOT re-interpret: Fungible, Receipt, Migratable,         │
│    Pending, Settlement, Identity                                  │
│  - does NOT persist settlement/finality decisions from metadata  │
│  (may carry fields through manifests/receipts, but must not make │
│   external finality or business-validity decisions from them)    │
│  = truth of execution ordering                                   │
└─────────────────────────────────────────────────────────────────┘
```

流程：

```text
CellScript declares.
Compiler normalises.
Runtime verifies.
Scheduler executes.
```

### 11.3 为什么 TypedCellDecl 不能只放在 CellScript repo

Spora runtime 必须有一份自己的 Rust definition，因为 runtime 要：

```text
验证 manifest / trusted summary / scheduler witness
推导 conflict_hash
校验交叉约束
构造 CellDAG
```

spora-typed 已经在 runtime 里落地了 TypedCellDecl、conflict_hash、typed_data_hash、
TypedCellStore、BlockAccessSummary 和 CellDAG 调度，明确 runtime-first、不依赖 CellScript。

正确结构：

```text
TypedCellDecl schema/spec lives in Spora protocol.
CellScript Spora profile emits it.
Runtime consumes and validates it.
```

**类型定义属于协议边界；生成逻辑属于 CellScript；校验逻辑属于 runtime。**

### 11.4 CellScript profile 架构

CellScript core 不应该内置太多 Spora-specific 东西。Core 只保留：

```text
resource
shared
receipt
action
where
require
preserve
consume
create
move
flow
```

Spora profile 才启用：

```cellscript
#[conflict_key(...)]
#[identity(...)]
#[settlement(...)]
#[cell_class(...)]
```

这样 CKB L1 profile 不会被污染。

### 11.5 Profile 家族

TypedCellDecl 是共同中间层；Spora / Hypha / Axone 只是不同部署后端：

```text
TypedCell profile family
    ├── Spora: open chain header roots
    ├── Hypha: federation/audit roots
    └── Axone: CKB checkpoint cell roots
```

CellScript may eventually expose deployment profiles：

```text
Ckb
TypedCell(Spora)
TypedCell(Hypha)
TypedCell(Axone)
```

CellScript core remains profile-gated。第一个 typed-cell profile应该是共享的，
Spora / Hypha / Axone 作为不同部署后端，共享同一个 typed-cell lowering backend。

### 11.6 当前 spora-typed 的定位

spora-typed 分支实现的是 Layer A（protocol schema + validation）和 Layer C（scheduler consumption）。

Layer B（CellScript Spora profile 的代码生成）属于 Phase 2+，当前分支不依赖 CellScript。

当 CellScript Spora profile 实现时，它输出的 TypedCellDecl 必须与此 runtime 的 Rust definition
在 wire format 上兼容——Layer A 是 canonical contract。

---

## 12. Scope 收尾

**当前 `spora-typed` 只完成 typed-cell execution core；Spora、Hypha、Axone 是同一 typed-cell stack 的未来部署方向，不进入当前 runtime core scope。**

具体边界：

```text
当前 scope:     typed-cell execution core
                = conflict_hash + typed_data_hash + CellDAG scheduling
                + TypedCellDecl schema + validation
                + scheduler witness envelope

不在 scope:     CellScript Spora profile 代码生成 (Phase 2+)
                Spora/Hypha/Axone 部署后端差异
                BFT / settlement / checkpoint / exit
                ProofPlan / artifact_set / audit layer
                VM typed-cell semantic awareness
```

TypedCellDecl 在当前 scope 内是 normalized metadata——不是 VM primitive，不是独立语义权威，不是用户 API。
