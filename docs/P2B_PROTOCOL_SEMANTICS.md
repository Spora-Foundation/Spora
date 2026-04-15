# P2B 协议语义规范

- 日期: 2026-04-15
- 状态: Normative（规范性）
- 相关文档: [P2B_VIRTUAL_PROCESSOR_PARALLELIZATION_DESIGN.md](./P2B_VIRTUAL_PROCESSOR_PARALLELIZATION_DESIGN.md)

---

## 0. 本文档的定位

本文档是 P2B Virtual Processor 并行化重构的**前置规范**。在动代码前，必须先明确 5 个协议语义问题。后续所有实现（包括串行 reference runner 和并行 runner）**不允许偏离**本文档定义的规则。

本文档中的规则具有规范性（normative）效力。当实现与本文档产生歧义时，以本文档为准。

---

## 1. 核心规则

### 规则 1: Duplicate Transaction Across Blue Blocks

**场景**：同一笔交易（相同 tx_id）出现在同一 mergeset 中的多个 blue blocks 里。

**规范处理**：

1. 按 GhostDAG canonical order 遍历 mergeset 中的 blue blocks。
2. 该交易**仅在 canonical order 中最早出现的那个 blue block 内被接受（accepted）**。
3. 在后续 blue blocks 中再次出现的同一笔交易，视为 **skipped**。

**约束**：

- skipped 的重复交易**不进入** accepted set。
- skipped 的重复交易**不进入** cell_diff。
- skipped 的重复交易**不影响** cell_root。
- "最早出现"的判定依据是 GhostDAG canonical order，不是网络到达顺序或本地处理顺序。

### 规则 2: Double Spend Across Blue Blocks

**场景**：不同 blue blocks 中的**不同交易**消费了同一个 outpoint。

**规范处理**：

1. 按 GhostDAG canonical order 遍历 mergeset 中的 blue blocks。
2. **仅接受 canonical order 中第一个成功消费该 outpoint 的交易**。
3. 后续 blue blocks 中消费同一 outpoint 的冲突交易，视为 **skipped**。

**约束**：

- 冲突检测基于 **outpoint 级别**，不是 tx 级别。即：如果一笔交易的某个输入 outpoint 已被先前 blue block 中的另一笔交易消费，则该笔交易整体视为 skipped。
- **不允许"部分应用"**：不存在"跳过冲突输入，保留非冲突输入"的处理方式。一笔交易要么整体 accepted，要么整体 skipped。
- skipped 的冲突交易不进入 accepted set，不进入 cell_diff，不影响 cell_root。

### 规则 3: Effect Invalidation at Commit Time

**场景**：analyze 阶段基于某个 snapshot 生成了一个 block 的 effect，但在提交（commit）到 canonical state 时，发现该 effect 的前提已失效（例如：该 effect 依赖的 outpoint 已被先前提交的 effect 消费）。

**规范处理**：

1. 该 block 的**整块 effect 变空**（empty effect）。
2. **不允许部分应用**：不存在"跳过失效交易，保留其余交易"的处理方式。

**约束**：

- 该 block 的所有交易均视为 **skipped**，其 `accepted_tx_ids` 为空。
- 该 block **仍保留在 mergeset 中**，不从 blue set 中移除。
- 该 block **不产生 cell state 变更**：cell_diff 为空，不影响 cell_root。
- 该 block 的 reward 按空 acceptance 计算（即：该 block 内无 accepted 交易，但 block 本身仍作为 blue block 存在于 mergeset 中）。

### 规则 4: Acceptance Data Recording Scope

**场景**：确定 acceptance data 中记录哪些交易。

**规范处理**：

1. acceptance data 记录的是**最终真正进入 canonical virtual state 的交易**。
2. **不是**"被分析过的候选交易"。

**约束**：

- 只有通过了 commit 阶段冲突检查的交易，才出现在 acceptance data 中。
- 在 analyze 阶段被分析但在 commit 阶段因冲突而 skipped 的交易，**不出现在** acceptance data 中。
- acceptance data 的排列顺序遵循 GhostDAG canonical order：先按 blue block 的 canonical order 排列，block 内按交易在 block 中的原始顺序排列。

### 规则 5: Reward and Acceptance Definition Alignment

**场景**：确定 reward 计算与 acceptance data 之间的关系。

**规范处理**：

1. reward 计算和 acceptance data **共享同一套 "accepted" 定义**。
2. 一个 blue block 的 reward 基于其**最终被 accepted 的交易**（经过 commit 阶段确认的）。

**约束**：

- 如果一个 blue block 因规则 3 导致整块 effect 为空，则该 block 的 reward 按零 accepted 交易计算。
- red blocks 仍然只参与 reward/acceptance 语义，**不进入** selected virtual state 的 tx 应用。red blocks 的交易不会被执行，也不会产生 cell state 变更。
- `accepted_tx_ids` 是 acceptance_data 和 reward_data 的**唯一来源**，不存在独立于 accepted_tx_ids 的其他 acceptance 或 reward 数据路径。

---

## 2. 不变量（Invariants）

以上 5 条规则共同保证以下不变量。任何实现必须满足这些不变量，否则视为违反协议语义。

### 不变量 1: 确定性一致

对同一 mergeset，无论分析是串行还是并行，最终 `cell_root` 完全一致。

即：串行 reference runner 和并行 runner 对相同的 GhostDAG 视角，必须产出完全相同的 `cell_root`、`accepted_tx_ids`、`mergeset_acceptance_data` 和 `reward_data`。

### 不变量 2: 顺序无关性

对同一 mergeset，不同的 block import 顺序不影响最终结果。

即：本地节点以何种物理顺序接收到 mergeset 中的 blocks，不改变最终的 canonical virtual state。唯一决定处理顺序的是 GhostDAG canonical order。

### 不变量 3: 单一数据来源

`accepted_tx_ids` 是 `acceptance_data` 和 `reward_data` 的唯一来源。

即：不存在绕过 `accepted_tx_ids` 的独立 acceptance 或 reward 计算路径。所有消费 acceptance 或 reward 信息的下游逻辑，必须且只能基于 `accepted_tx_ids`。

### 不变量 4: 唯一仲裁依据

GhostDAG 的 canonical order 是唯一的冲突仲裁依据。

即：当发生 duplicate tx 或 double spend 时，赢家的选择完全且唯一地由 GhostDAG canonical order 决定。不存在基于手续费、tx 大小、本地到达时间或其他因素的仲裁规则。

---

## 3. 术语表

| 术语 | 定义 |
|---|---|
| **canonical order** | GhostDAG 对 mergeset 中 blue blocks 给出的确定性处理顺序。该顺序由 GhostDAG 的 blue-work 和共识排序规则决定，与本地节点的 block 到达顺序无关。 |
| **mergeset** | 在某个 POV（point of view）block 下，由 GhostDAG 确定的需要合并进 virtual state 的一组 blocks。包含 blue blocks 和 red blocks。 |
| **blue block** | mergeset 中被 GhostDAG 分类为 blue 的 block。blue blocks 的交易会被尝试执行并应用到 virtual state 中（可能因冲突而 skipped）。 |
| **red block** | mergeset 中被 GhostDAG 分类为 red 的 block。red blocks 的交易不进入 virtual state 的 tx 应用，仅参与 reward/acceptance 语义。 |
| **effect** | 对单个 blue block 进行分析后产生的局部状态变更描述（`BlockExecutionEffect`），包含 cell_diff、accepted_tx_ids、reward_data 等。effect 是纯数据，不直接修改全局状态。 |
| **snapshot** | 某一时刻 canonical state 的只读冻结视图（`ExecutionSnapshot`）。analyze 阶段基于 snapshot 进行分析，snapshot 在分析期间不可变。 |
| **accepted** | 一笔交易最终通过了 commit 阶段的冲突检查，真正进入了 canonical virtual state。只有 accepted 的交易才出现在 acceptance_data 和 reward_data 中。 |
| **skipped** | 一笔交易因 duplicate（规则 1）、double spend（规则 2）或 effect invalidation（规则 3）而未进入 canonical virtual state。skipped 的交易不产生任何 cell state 变更。 |

---

## 4. 规则间关系

- 规则 1 和规则 2 定义了 **analyze 阶段**的冲突处理语义（串行场景下直接适用）。
- 规则 3 定义了 **commit 阶段**的冲突处理语义（并行场景下，analyze 基于 snapshot 可能乐观通过，但 commit 时需要二次检查）。
- 规则 4 和规则 5 定义了冲突处理结果的**下游传播语义**，确保 acceptance_data 和 reward_data 与实际 accepted 交易严格一致。

五条规则共同确保：无论执行模型是串行还是并行，协议语义完全等价。
