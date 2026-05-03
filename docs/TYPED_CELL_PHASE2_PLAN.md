# Typed Cell Phase 2 计划

Branch: `spora-typed`
前置：Phase 1 已完成（typed-cell scheduler 语义已落地，`cargo test --workspace` 零失败）
状态：**等待 CellScript 稳定后执行**

## Phase 1 回顾

Phase 1 做了什么：

- `conflict_hash + typed_data_hash` 替代 `binding_hash`
- 六维 typed cell 分类（Ownership × Mutability × Accounting × Identity × Settlement × ConflictKeySpec）
- CellDAG 读写感知调度（`build_from_typed()`）
- BlockAccessSummary 按 conflict_hash 组织 shared read/write 域
- 5 个操作码：CONSUME / TRANSFER / DESTROY / READ_REF / CREATE
- 全 workspace 测试零失败

Phase 1 没做什么：

- 没有编译器产出 typed-cell scheduler witness（CellScript 未稳定）
- 没有把 CellDAG 接入 virtual_processor 生产线
- 没有端到端 demo
- 没有 cell_state_hash

## Phase 2 目标

**让 typed-cell scheduler 从"runtime 可解码验证"升级为"端到端可运行"。**

具体来说，Phase 2 结束时应该能做到：

```text
CellScript 编译 action
  → 产出 typed-cell scheduler witness（Molecule 格式，70 字节访问记录）
  → 运行时解码验证
  → BlockAccessSummary 提取 conflict_hash 读写域
  → ExecutionDAG 分层
  → 并行 analyze_blue_block
  → 顺序 commit_block_effect
  → 确定性 cell_root
```

---

## 前置条件

Phase 2 的执行前提：

1. **CellScript 稳定**：CellScript 编译器可以正确生成 Molecule 格式的 scheduler witness
2. **CellScript 重新加入 workspace**：`cellscript` crate 可构建、可测试

**如果 CellScript 尚未稳定，Phase 2A（runtime 侧）仍可先行。**

---

## Phase 2A：Runtime 消费侧闭合

目标：让已有的 typed-cell 调度能力真正跑起来。不依赖 CellScript。

### 2A-1：~~BlockAccessSummary access 级 conflict_hash 反向提取~~ ✅ 已完成

`touches_shared` 字段已从 `CellScriptSchedulerWitness` 中彻底删除。`merge_cellscript_scheduler_witness` 现在直接从 access 记录按 operation 精确分类：

```text
access.operation ∈ {READ_REF}                           → cellscript_shared_reads
access.operation ∈ {CONSUME, CREATE, DESTROY, TRANSFER}  → cellscript_shared_writes
```

这比原来的 `touches_shared` + `effect_class` 粗粒度分类更精确——一个 MUTATING action 可能同时 READ 一个 conflict_hash 和 WRITE 另一个，而旧模型只能一股脑归入 writes。

### 2A-2：ExecutionDAG 接入 conflict_hash 维度

`ExecutionDAG::build()` 只依赖 `has_dependency_on()` 的 bool 结果，已包含 conflict_hash 维度。但测试尚未覆盖 conflict_hash 场景。

| 文件 | 改动 |
|------|------|
| `consensus/src/pipeline/virtual_processor/execution_dag.rs` | 补 conflict_hash 依赖测试：两个 block 的 shared write 域相同 → 同层不可并行 |
| `consensus/src/pipeline/virtual_processor/execution_dag.rs` | 补测试：两个 block 的 shared read 域相同 → 同层可并行 |

**验证**：`cargo test -p spora-consensus -- execution_dag`

### 2A-3：CellDAG.build_from_typed() 接入 block template 路径

当前 `CellDAG::build_from_typed()` 只在单元测试中使用。需接入 block template selector。

| 文件 | 改动 |
|------|------|
| `mining/src/block_template/selector.rs` | 在 `select_transactions()` 中，如果 tx 携带 typed-cell scheduler witness，使用 `CellDAG::build_from_typed()` 替代纯 outpoint 冲突检测 |
| `minempool/src/pool.rs` | mempool 冲突检测加入 conflict_hash 维度（可选，Phase 2 不阻塞） |

**验证**：`cargo test -p spora-mining`

### 2A-4：验证 trusted summary 路径可用

当前 `BlockAccessSummary::try_from_block_txs_with_cellscript_scheduler()` 支持 `TrustedCellScriptSchedulerAccessSets`，但需要验证端到端路径。

| 文件 | 改动 |
|------|------|
| `consensus/src/pipeline/virtual_processor/access_summary.rs` | 补集成测试：trusted summary → validate_summary → merge → ExecutionDAG |
| `wallet/core/src/tx/generator/generator.rs` | 确认 `attach_cellscript_compiled_scheduler_witness()` 产出的 trusted summary 可被 virtual_processor 消费 |

**验证**：`cargo test -p spora-consensus -- trusted_access_set`

### 2A-5：执行等价性测试

MPE 文档 §9 要求：串行 reference runner 与并行 runner 给出完全相同的 `cell_root`、`accepted_tx_ids`、`acceptance_data`、`reward_data`。

| 文件 | 改动 |
|------|------|
| `consensus/src/pipeline/virtual_processor/cell_processing.rs` | 补 reference 测试：对同一 mergeset，串行 `analyze_blue_block` + `commit_block_effect` 与当前实现给出相同结果 |
| `consensus/src/pipeline/virtual_processor/parallel_tests.rs` | 跨 blue block 重复 tx / double spend / cell_dep read-after-spend 场景测试 |

**验证**：`cargo test -p spora-consensus -- parallel`

---

## Phase 2B：CellScript 生产侧闭合

前置：CellScript 稳定，重新加入 workspace。

### 2B-1：TargetProfile::TypedCell

| 文件 | 改动 |
|------|------|
| CellScript compiler | 新增 `TargetProfile::TypedCell` 枚举变体 |
| `wallet/core/src/tx/generator/settings.rs` | 新增 `CELLSCRIPT_TARGET_PROFILE_TYPED_CELL` 常量，在 `cellscript_action_generator_plan_from_metadata_json` 中处理 |

### 2B-2：conflict_key canonical encoding

| 文件 | 改动 |
|------|------|
| CellScript compiler | `#[conflict_key(composite(...))]` 降级为 `encode_conflict_key_value_composite` 字节 |
| CellScript compiler | `#[conflict_key(field(...))]` 降级为单字段 UTF-8 字节 |
| CellScript compiler | `#[identity(...)]` 降级为 `CellIdentity` 枚举值 |

### 2B-3：generate_molecule_typed_cell()

| 文件 | 改动 |
|------|------|
| CellScript compiler | 新增 `generate_molecule_typed_cell()` 函数：blake3 域分离，70 字节访问记录 |
| CellScript compiler | 每个 typed cell access 生成 `CellScriptSchedulerAccessWitness`，operation/source 从 action 语义推导 |

关键约束：

```text
- conflict_hash = blake3("spora-typed-cell/conflict-hash/v1" || code_hash || hash_type || args || conflict_key_value)
- typed_data_hash = blake3("spora-typed-cell/typed-data-hash/v1" || code_hash || hash_type || args || data)
- effect_class = 从 action 操作集推导（pure/read_only/mutating/creating/destroying）
- 零值 typed_data_hash 非法
```

### 2B-4：trusted summary / scheduler-plan 生成

| 文件 | 改动 |
|------|------|
| CellScript compiler | 编译输出中包含 `scheduler_witness_hex`（Molecule 编码的完整 witness） |
| CellScript compiler | 编译输出中包含 `create_set` / `consume_set` / `destroy_set` / `read_set` 元数据 |
| CellScript compiler | 编译输出 JSON 符合 `wallet/core/src/tx/generator/settings.rs` 已有的 `cellscript_action_generator_plan_from_metadata_json` 解析格式 |

### 2B-5：identity/conflict_key 语义检查

| 文件 | 改动 |
|------|------|
| CellScript compiler | `#[conflict_key]` 覆盖率检查：shared/party mutable cell 必须声明 conflict_key |
| CellScript compiler | `ConflictKeySpec::None` 拒绝检查：non-ephemeral write-capable cell 不得使用 None |
| CellScript compiler | conflict_key 域完整性：同一 type_script 的所有实例必须使用相同的 conflict_key 模式 |

---

## Phase 2C：端到端 Demo

### 2C-1：CellScript → runtime 集成测试

| 文件 | 改动 |
|------|------|
| `testing/integration/` | 新增集成测试：CellScript 编译 → wallet generator 产出 tx → virtual_processor 验证 → cell_root 一致 |

### 2C-2：Invoice Financing Demo

这是 SporaBFT 架构文档定义的 MVP demo。Phase 2 应能演示：

```text
发票 cell（owned, linear, fungible）
  → 发票融资 action
  → CellScript 编译产出 typed-cell scheduler witness
  → conflict_hash = invoice_id（按发票去重，防重复融资）
  → typed_data_hash = blake3(... || invoice_data)
  → runtime 验证：同一 invoice_id 的两个融资 tx 在同一 block 中被 serial 化
  → 审计：typed_data_hash 提供发票数据承诺
```

### 2C-3：AMM Pool Demo（可选）

```text
Pool cell（shared, versioned, non-fungible）
  → swap action
  → conflict_hash = pool_id（按池分组）
  → 不同 pool_id 的 swap 可并行
  → 同 pool_id 的 swap serial 化
```

---

## Phase 2 不做的事

- **不做 BFT**：共识机制选择取决于部署形态，Phase 2 不碰
- **不做 checkpoint/exit**：L2 语义待定
- **不做 settlement 编译器强制**：`#[settlement]` 仅 parse 不 enforce
- **不做 CLAIM/SETTLE/MUTATE 操作码**：5 个操作码足够 Phase 2，按需引入
- **不做 cell_state_hash**：Phase 3 内容
- **不做 MPE 跨 blue block 并行执行**：那需要 virtual_processor 的 effect 模型重构，是独立工程

---

## 执行顺序

```text
Phase 2A（不依赖 CellScript，可立即开始）
  2A-1 BlockAccessSummary access 级 conflict_hash 反向提取
  2A-2 ExecutionDAG conflict_hash 测试
  2A-3 CellDAG 接入 block template
  2A-4 trusted summary 端到端验证
  2A-5 执行等价性测试

Phase 2B（依赖 CellScript 稳定）
  2B-1 TargetProfile::TypedCell
  2B-2 conflict_key canonical encoding
  2B-3 generate_molecule_typed_cell()
  2B-4 trusted summary / scheduler-plan 生成
  2B-5 identity/conflict_key 语义检查

Phase 2C（依赖 2A + 2B）
  2C-1 CellScript → runtime 集成测试
  2C-2 Invoice Financing Demo
  2C-3 AMM Pool Demo（可选）
```

---

## 完成标准

Phase 2 完成时必须满足：

1. CellScript 编译器可以为 typed-cell action 生成合法的 Molecule scheduler witness
2. 生成的 witness 能通过 `decode_cellscript_scheduler_witness_molecule()` 全语义验证
3. `BlockAccessSummary` 正确提取 conflict_hash 读写域
4. `ExecutionDAG` 正确分层（conflict_hash 维度）
5. Block template selector 使用 conflict_hash 做冲突预过滤
6. Invoice Financing Demo 端到端跑通
7. 同一 invoice_id 的两个融资 tx 在同一 block 中被 serial 化
8. 不同 conflict_hash 的 tx 可并行
9. 串行 runner 与并行 runner 给出相同 cell_root
10. `cargo test --workspace` 零失败

---

## 文件影响预估

| 模块 | 改动类型 |
|------|---------|
| `consensus/src/pipeline/virtual_processor/execution_dag.rs` | 补测试 |
| `consensus/src/pipeline/virtual_processor/access_summary.rs` | 补测试 |
| `consensus/src/pipeline/virtual_processor/cell_processing.rs` | 补 reference 测试 |
| `mining/src/block_template/selector.rs` | 接入 CellDAG::build_from_typed() |
| `wallet/core/src/tx/generator/settings.rs` | 新增 TypedCell profile 处理 |
| CellScript compiler | TargetProfile, conflict_key encoding, witness 生成 |
| `testing/integration/` | 端到端集成测试 |
