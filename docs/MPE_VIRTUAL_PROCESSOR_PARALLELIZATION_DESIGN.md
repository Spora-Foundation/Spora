# MPE Virtual Processor 并行化设计

- 日期: 2026-04-15
- 状态: Draft (已部分实现基础结构)
- 结论级别: 设计评审结论
- 相关文档:
  - [CONSENSUS_SECURITY_AUDIT_2026.md](/Users/arthur/RustroverProjects/Spora/docs/CONSENSUS_SECURITY_AUDIT_2026.md)
  - [spora_consensus_v2_issue_list.md](/Users/arthur/RustroverProjects/Spora/docs/spora_consensus_v2_issue_list.md)
  - [cell_dag_consensus_audit.md](/Users/arthur/RustroverProjects/Spora/docs/cell_dag_consensus_audit.md)

## 0. 这份文档在讨论什么

这份文档讨论的不是“Spora 要不要并行”，而是一个更具体的问题：

- `P1`：单个 block 内，交易验证能不能并行
- `P2a`：模板构建时，能不能提前把明显冲突的交易过滤掉
- `MPE`：virtual processor 在处理一个 mergeset 的多个 blue blocks 时，能不能并行推进 Cell 状态

前两件事和后面这件事，看起来都叫“并发化”，但本质完全不同。

用人话说：

- `P1` 是“一个 block 里有很多交易，验证它们时能不能多核一起跑”
- `P2a` 是“矿工本地挑交易时，能不能提前把互相打架的交易挑掉”
- `MPE` 是“一个 DAG 视角下有很多 blue blocks 要一起算进虚拟状态，能不能同时算”

真正麻烦的是 `MPE`。

原因不是它更耗 CPU，而是它会碰到：

- 哪个 blue block 先消费了某个 cell
- 哪些交易已经被别的 blue block 接受过
- reward 和 acceptance data 应该记在哪个 block 上
- 最终 `cell_root` 必须和串行规范结果完全一致

所以这份文档本质上是在回答：

`MPE` 是不是一个值得做的吞吐优化，以及它在不改变共识语义的前提下应该怎么做。

### 0.1 快速判断表

| 项目 | 在并行什么 | 是否已落地 | 是否触及共识语义 | 现在是否建议继续推进 |
|---|---|---|---|---|
| `P1` | 单个 block 内的 tx 验证 | 是 | 否 | 已完成 |
| `P2a` | 模板构建时的冲突预过滤 | 是 | 否，属于本地策略 | 已完成 |
| `MPE` | mergeset 内多个 blue blocks 的 virtual-state 推进 | 否 | 是 | 只能先做前置重构 |

## 1. 结论摘要

这份文档回答三个问题：

1. `MPE` 是否值得做
2. `MPE` 如何安全地做
3. 是否需要修改 GhostDAG 模型

当前结论是：

- `MPE` 值得做，但它是吞吐优化工程，不是短平快补丁
- `MPE` 现在还不应该直接开工做“跨 blue block 并行执行”
- 不需要修改 GhostDAG 模型
- 需要先重构 `virtual_processor`，把“逐块重放并直接改共享状态”改成“生成局部 effect，再按确定性顺序合并”

一句话总结：

MPE 的前提不是更多线程，而是先把 virtual-state 执行语义变成可组合、可验证、可顺序合并的 effect 模型。

## 2. 当前状态

### 2.1 已经完成的部分

- `P1` 已完成
  - `body_validation_in_context` 中的块内交易验证已经并行化
  - `exec/src/vm/verifier.rs` 中脚本组验证已经使用确定性顺序和 `par_iter`
- `P2a` 已完成
  - block template 路径已经接入 `CellDAG` 做冲突预过滤
  - 只用于模板选择和预过滤，不改变共识规则

### 2.2 还没有完成的部分

- `virtual_processor` 的 mergeset blue block 处理仍是串行
- `exec/scheduler/*` 还没有进入 production virtual-state 路径
- `calculate_cell_state()` 仍然通过可变共享状态逐块重放

### 2.3 已实现的基础结构

当前代码中已存在部分 effect 模型的基础结构，但尚未完全实现文档设计的并行化方案：

- **已存在的结构**（`consensus/src/pipeline/virtual_processor/cell_processing.rs`）：
  - `BlockCellProcessingEffect`（第 108-115 行）：类似文档中的 `BlockExecutionEffect`，包含 `block_hash`、`accepted_tx_ids`、`cell_diff`、`reward_data` 等字段
  - `SelectedParentCoinbaseEffect`（第 101-106 行）：处理选中父块 coinbase 的 effect
  - `commit_block_effect()`（第 426-443 行）：将 effect 提交到上下文

- **与设计的差距**：
  - `process_blue_block()` 仍直接修改 `CellStateTree` 和 `processed_txs`，不是纯分析函数
  - 不存在 `BlockAccessSummary` 和 `ExecutionSnapshot`
  - 尚未实现 "并行生成 effect + 顺序提交" 的完整模型

**当前状态总结**：已有初步的 effect 分离结构，但分析和提交阶段尚未完全解耦，距离 MPE 并行化还有重构工作要做。

核心代码路径：

- `consensus/src/pipeline/virtual_processor/cell_processing.rs`
- `consensus/src/pipeline/virtual_processor/processor.rs`
- `consensus/src/model/stores/ghostdag.rs`
- `exec/src/scheduler/dag.rs`
- `exec/src/scheduler/executor.rs`

### 2.3 它和 CKB 是什么关系

这个问题很容易被误解成：

- “CKB 早就有这个并行化了，我们只是补课”
- 或者
- “CKB 没做，所以我们也不用做”

这两种理解都不准确。

更准确的比较是：

- CKB 是链式共识
- Spora 是 GhostDAG + Cell

CKB 需要解决的是：

- 单笔交易怎么验证
- 单个 block 怎么验证
- VM 脚本怎么执行

Spora 在这些问题之外，还多了一层：

- 同一个 virtual state 计算里，要处理一组 blue blocks 对 Cell 状态的合成影响

这就是 `MPE` 的来源。

换句话说：

- `P1` 这种并行，CKB 也能做，也值得做
- `MPE` 这种并行，CKB 天然没有同等问题空间，因为它没有 mergeset blue blocks 这层执行语义

所以：

- `MPE` 不是“落后于 CKB 的缺功能”
- `MPE` 是 GhostDAG 架构自己带来的额外复杂度

截至 2026-04-12，我对照了 CKB 官方主线实现，至少在主验证路径上可以看到：

- 块内交易验证仍以串行遍历为主
- 脚本组验证在官方实现里也不是“像 MPE 那样”的共识层跨任务并行

这进一步说明：

- 我们现在讨论的不是“照搬 CKB 的现成方案”
- 而是在 GhostDAG 语义之上，设计 CKB 不需要的一层执行并行化

## 3. 为什么现在不能直接做 MPE

### 3.1 `calculate_cell_state()` 不是纯函数

当前 blue block 处理不是“输入 block，输出结果”，而是：

- 读取当前 `cell_state_tree`
- 读取并修改 `replay_validation`
- 读取并修改 `processed_txs`
- 修改 `accepted_tx_ids`
- 修改 `mergeset_acceptance_data`
- 修改 `mergeset_rewards`
- 修改 `block_cell_diffs`

也就是说，现在的 blue block 处理是“共享状态上的顺序重放”，不是“独立任务的结果收集”。

直接把 blue blocks 扔进 Rayon，会产生两个问题：

- 数据竞争问题
- 更严重的共识语义漂移问题

当前真正被写入的对象，至少包括：

- `ctx.cell_state_tree`
- `ctx.mergeset_cell_diff`
- `ctx.block_cell_diffs`
- `ctx.accepted_tx_ids`
- `ctx.mergeset_acceptance_data`
- `ctx.mergeset_rewards`
- `processed_txs`
- `replay_validation`

所以现在的 `calculate_cell_state()` 更像一个“顺序状态机”，而不是一个“可拆分任务的调度器入口”。

### 3.2 后一个 blue block 是否可接受，依赖前一个 blue block 的结果

当前状态下，blue block B 的交易验证可能依赖：

- blue block A 已经消费了哪些输入
- blue block A 已经创建了哪些输出
- blue block A 已经把哪些交易记入 `processed_txs`
- replay overlay 中已经有哪些 spend/add

因此 blue blocks 之间不是天然独立的。

这意味着：

- 不能假设“同一 mergeset 的 blue block 都可以并行”
- 也不能用当前代码结构证明这种并行是安全的

### 3.3 当前跨 blue block 冲突没有被抽象成可合并规则

当前路径对冲突输入的处理，仍然更接近：

- 顺序重放
- 谁先消费，后面谁失败

这在串行实现下是明确的。

但一旦并行化，就必须先定义：

- 冲突 blue blocks 是否允许同时进入候选分析
- 谁是规范赢家
- 失败 block 的 accepted tx / reward / journal 怎么处理
- 是否允许“本块部分接受，部分跳过”

这些目前都没有被抽象成一个可并行、可确定性合并的规范。

### 3.4 `ConflictResolver` 不能直接带入共识

`exec/src/scheduler/conflict.rs` 当前适合：

- 模板选择
- mempool 策略
- 本地优化排序

它不适合未经协议设计就直接进入共识路径。

原因很简单：

- 模板侧可以“冲突时选一个赢家”
- 共识侧必须先有明确协议规则，再决定是否允许这种赢家选择

因此：

MPE 不能等同于“把 scheduler 直接接进 virtual_processor”。

### 3.5 当前问题更像“执行语义未分层”，不是“缺线程池”

这点需要说清楚。

现在仓里并不缺并行基础设施：

- 有 `CellDAG`
- 有 `ParallelExecutor`
- 有 `ConflictResolver`
- 有已经落地的 `P1`

但 `MPE` 还是不能直接做。

原因不是“还没 import rayon”，而是：

- 目前没有纯粹的 `BlockExecutionEffect`
- 没有 block-level 访问摘要
- 没有 effect 冲突的规范合并规则
- 没有 reference runner 与 parallel runner 的等价性测试

所以 `MPE` 的核心工作量，不在并发 API，而在执行模型本身。

## 4. MPE 值不值得做

### 4.1 值得做的理由

`MPE` 是值得做的，理由如下：

- DAG 高 BPS 场景下，virtual-state 推进会越来越吃掉吞吐红利
- 当前块内验证并行已经做完，下一层明显瓶颈会逐步转移到 mergeset 状态应用
- `CellDAG`、`ParallelExecutor`、block effect 拆分这些基础工作已经开始出现，工程方向是对的

### 4.2 不值得立刻硬做的理由

它不是当前最安全的“直接提速按钮”，因为：

- 当前语义层还没拆干净
- 共享可变状态太多
- blue block 冲突规则还没有被提升成 effect 合并规则

因此：

- 从长期看，值得做
- 从当前实现状态看，不值得直接并行化

更准确的定位是：

MPE 是一个需要前置重构的 P2 项，不是一个可以立刻落地的 P1 优化项。

## 5. 是否需要改 GhostDAG 模型

不需要。

### 5.1 不需要改的部分

GhostDAG 当前已经提供了 MPE 所需的核心共识语义：

- selected parent
- blue / red 分类
- mergeset
- blue-work/consensus order 迭代

相关代码：

- `consensus/src/model/stores/ghostdag.rs`

这些已经足够表达：

- 哪些 block 在当前 POV 下属于 mergeset
- 它们的规范顺序是什么
- 哪些奖励/acceptance 应归于 blue 或 red

### 5.2 真正该改的部分

要改的是执行层，不是 DAG 模型：

- `virtual_processor/cell_processing.rs`
- `virtual_processor/processor.rs`
- replay overlay 接口
- block effect 的生成与合并方式

所以 MPE 的正确表述不是：

- “修改 GhostDAG 支持并行执行”

而是：

- “在 GhostDAG 已给定的规范顺序之上，重构 virtual-state 执行层，使其可以安全并行地生成局部 effect”

换句话说：

- GhostDAG 继续负责“谁是蓝、谁是红、顺序是什么”
- 执行层负责“在这个顺序之上，怎样更高效但仍然确定地算出同一个结果”

这两层不要混。

### 5.3 哪些东西明确不需要改

为了减少误解，这里把“不需要改”的东西单独列出来：

- 不需要改 GhostDAG 的打分模型
- 不需要改 blue/red/selected-parent/mergeset 的定义
- 不需要改 block/header 的承诺字段
- 不需要改 RPC 对外协议才能开始做 `MPE`

`MPE` 首先是 internal execution refactor，不是协议格式升级。

## 6. 安全实现原则

MPE 若要进入实现，必须满足以下原则。

### 6.1 Canonical order 不变

无论内部是否并行，外部可观察的规范顺序必须仍由 GhostDAG 决定。

并行化不能改变：

- blue block 的规范处理顺序
- acceptance data 的顺序语义
- reward 归属
- 最终 `cell_root`

### 6.2 并行阶段只生成 effect，不直接改全局状态

并行阶段只能做：

- 读取冻结快照
- 分析 block / tx 的读写集
- 生成局部 `BlockExecutionEffect`

并行阶段不能直接做：

- `tree.remove(...)`
- `tree.insert(...)`
- 修改 `processed_txs`
- 修改 `accepted_tx_ids`
- 修改 `mergeset_acceptance_data`

### 6.3 合并必须顺序且确定性

即使 effect 是并行准备的，最终合并也必须：

- 按 GhostDAG 规范顺序
- 使用确定性 map / list 排序
- 在 effect 冲突时给出明确、稳定、一致的失败/跳过语义

这里最重要的一点是：

`可以并行准备，不等于可以并行提交。`

MPE 真正安全的模型，通常是：

- 并行准备局部 effect
- 顺序提交 effect

而不是：

- 多个线程同时去改 `cell_state_tree`

### 6.4 先定义冲突语义，再做并行

必须先明确：

- cross-blue-block duplicate tx 的规范处理
- cross-blue-block double spend 的规范处理
- “分析成功但合并时前提失效”时的处理

如果这些语义先不写清楚，任何并行化都只是在移动 bug。

## 6.5 开工前必须回答的协议问题

下面这些问题，如果答案还没定，就不应该开始写 `MPE` 的并行代码：

1. 同一笔 tx 同时出现在两个 blue blocks 里，规范结果是什么
2. 两个 blue blocks 里的不同 tx 花同一个 outpoint，规范结果是什么
3. “分析阶段看起来可接受，但提交到 canonical state 时前提失效”的 effect，应该：
   - 整块失败
   - 仅跳过该 tx
   - 整块变空 effect
4. acceptance data 记录的是：
   - 被分析过的 tx
   - 还是最终真正进入 canonical virtual state 的 tx
5. reward 计算和 acceptance data 是否共享同一套“accepted”定义

这些问题都不是代码细节，而是协议语义。

## 7. 推荐的安全路线

### Phase 0: 保持 GhostDAG 不动

目标：

- 不修改 GhostDAG 模型
- 不修改 blue/red/selected-parent/mergeset 定义

完成标准：

- 所有后续设计文档都把 GhostDAG 视为既定输入

### Phase 1: 把 blue block 处理拆成纯 effect 生成 + 顺序提交

新增明确的内部对象，例如：

```rust
pub struct BlockExecutionEffect {
    pub block_hash: Hash,
    pub block_daa_score: u64,
    pub cell_diff: CellDiff,
    pub accepted_tx_ids: Vec<Hash>,
    pub acceptance_data: Vec<Hash>,
    pub reward_data: BlockRewardData,
}
```

要求：

- `analyze_blue_block(...)` 不接收 `&mut CellStateTree`
- `analyze_blue_block(...)` 不直接改 `processed_txs`
- `commit_block_effect(...)` 才能改全局状态

这一步做完后，串行语义必须与当前实现等价。

### Phase 2: 引入 block access summary

为每个 blue block 生成纯访问摘要，例如：

```rust
pub struct BlockAccessSummary {
    pub tx_ids: BTreeSet<Hash>,
    pub spent_outpoints: BTreeSet<OutPoint>,
    pub created_outpoints: BTreeSet<OutPoint>,
    pub read_deps: BTreeSet<OutPoint>,
}
```

目的：

- 判断哪些 blue blocks 真正独立
- 构建 block-level dependency graph
- 在不改 GhostDAG 的前提下推导并行层

### Phase 3: 建立 block-level execution DAG

输入：

- GhostDAG 给定的 mergeset order
- `BlockAccessSummary`

输出：

- 只包含“可并行 effect 生成”的层

注意：

- 这里的 DAG 是执行层 DAG，不是新的 GhostDAG
- 它不能改变最终 canonical order，只能表示“哪些任务可以在同一前置状态下分析”

### Phase 4: 层内并行生成 effect，层间顺序合并

算法：

1. 按 GhostDAG 规范顺序建立 block-level execution DAG
2. 逐层取出互不冲突的 blocks
3. 层内并行调用 `analyze_blue_block(...)`
4. 层结束后按 GhostDAG 规范顺序逐个 `commit_block_effect(...)`

这样可以保证：

- 层内无共享写
- 层间有 barrier
- 最终提交顺序仍是规范顺序

### Phase 5: 最后才考虑更细粒度并行

只有前四步稳定后，才值得继续探索：

- block 内 tx-level `CellDAG`
- 跨 block 的更细粒度 effect 切分
- replay overlay 的不可变 delta 化

## 8. 明确不该做的事

### 8.1 不要直接把当前 `process_blue_block()` 放进 `par_iter`

这会把共享可变状态竞争直接带进共识路径。

### 8.2 不要把 `ConflictResolver` 直接升级成共识规则

它目前适合模板/mempool，不适合直接决定共识赢家。

### 8.3 不要为了并行化而修改 GhostDAG 的 blue/red/selected-parent 语义

MPE 的问题不是 DAG 模型不够，而是执行层没被正确分层。

### 8.4 不要把“并行分析结果”直接当成最终 accepted 结果

并行分析阶段最多只能得到“候选 effect”。

最终 accepted 结果只能在 canonical order 下，通过顺序提交得到。

否则会出现：

- 本地线程调度不同
- 最终 accepted 集不同
- 进而 `cell_root` 不同

## 9. 完成标准

`MPE` 只有满足以下条件，才算真正安全落地：

1. 串行 reference runner 与并行 runner 对同一 mergeset 给出完全相同的：
   - `cell_root`
   - `accepted_tx_ids`
   - `mergeset_acceptance_data`
   - `reward_data`
2. 不同导入顺序、相同 GhostDAG 视角下，结果完全一致
3. cross-blue-block duplicate tx / double spend / cell_dep read-after-spend 场景有明确规范测试
4. 并行实现中不存在直接共享写 `cell_state_tree` / `processed_txs` / replay overlay 的路径

建议再加一条工程验收门槛：

5. 旧的“边分析边写共享状态”路径要被删除或明确降级为测试 reference runner，不能与新路径并存为双主实现

## 10. Go / No-Go 建议

当前建议是：

- `Go`：为 `MPE` 做前置重构
- `No-Go`：直接对现有 `calculate_cell_state()` 做跨 blue block 并行化

也就是说，下一步正确动作不是：

- “把 blue blocks 并行起来”

而是：

- “先把 blue block 执行收敛成纯 effect 生成 + 顺序提交”

如果只看一句话的管理结论，那就是：

- `Go`：为 MPE 做前置重构
- `No-Go`：直接做跨 blue block 并行执行

## 11. 直接行动项

建议按以下顺序推进：

1. **重构 `process_blue_block(...)`**（`consensus/src/pipeline/virtual_processor/cell_processing.rs`）：
   - 当前已实现 `BlockCellProcessingEffect` 和 `commit_block_effect()`
   - 需要进一步将 `process_blue_block()` 改为纯分析函数 `analyze_blue_block()`，不接收 `&mut CellStateTree`
   - 引入 `ExecutionSnapshot` 作为只读快照

2. **为 blue block 增加 `BlockAccessSummary`**
   - 新增文件 `consensus/src/pipeline/virtual_processor/access_summary.rs`
   - 包含 `spent_outpoints`、`created_outpoints`、`read_deps`、`tx_ids` 等字段

3. **补一组 reference 测试**，锁住：
   - same mergeset, same result
   - duplicate tx across blues
   - double spend across blues

4. **在 reference 语义稳定后，再引入 block-level execution DAG**
   - 保留 tx-level `CellDAG` 给模板/块内分析
   - 为 MPE 新增 block-level `ExecutionDAG`

## 12. 一次性落地方案

这一节讨论的是：

- 开一个新分支
- 明确接受内部接口破坏
- 不考虑后向兼容
- 用一次性重构把 `MPE` 真正落地

这不是“平滑迁移方案”，而是“趁现在把旧执行骨架直接换掉”的方案。

### 12.1 适用前提

只有满足下面三条，才适合走一次性落地：

- 团队接受 `virtual_processor`、相关测试、部分内部接口同时破坏
- 当前没有对旧 `calculate_cell_state()` 内部行为做稳定 API 承诺
- 目标是尽快得到更干净的执行模型，而不是最小改动上线

如果这三条不成立，就应该继续走前面那套渐进式路线。

### 12.2 一次性方案的总原则

一次性方案里，最重要的不是“并行”，而是先把旧模型删掉。

具体来说：

- 不再保留“分析 blue block 时直接改全局状态”的写法
- 不再保留 `process_blue_block(...)` 既分析又提交的双重职责
- 不再保留“依赖当前 tree/overlay 的隐式顺序副作用”作为规范来源

新的规范来源应当变成：

1. GhostDAG 给出的 canonical order
2. 纯分析阶段生成的 `BlockExecutionEffect`
3. 顺序提交阶段对 effect 的确定性合并

### 12.3 一次性落地步骤

#### Step 0: 明确协议语义

在动代码前，先把下面三条写死，不允许实现时临场发挥：

- duplicate tx across blue blocks：
  - 只接受 GhostDAG 规范顺序里最早出现的一次
  - 后续重复出现视为 skipped，不进入 accepted set
- double spend across blue blocks：
  - 只接受规范顺序里第一个成功消费该 outpoint 的交易
  - 后续冲突交易视为 skipped，不允许“部分应用”
- red blocks：
  - 仍然只参与 reward/acceptance 语义，不进入 selected virtual state 的 tx 应用

如果这三条不先定，后面所有并行化都是在实现不明确的协议。

#### Step 1: 删除旧的 blue-block 直接写状态模型

目标文件：

- `consensus/src/pipeline/virtual_processor/cell_processing.rs`

动作：

- 把 `process_selected_parent_coinbase(...)` 改成只返回 effect
- 把 `process_blue_block(...)` 改成只返回 effect
- 不允许它们接收：
  - `&mut CellStateTree`
  - `&mut processed_txs`
  - `&mut ReplayValidationContext`

新的函数签名应该更接近：

```rust
fn analyze_blue_block(
    snapshot: &ExecutionSnapshot,
    block_hash: Hash,
    block: &Block,
) -> Result<BlockExecutionEffect, RuleError>
```

这一刀做完后，旧的“边分析边写全局状态”路径应该直接删除，而不是保留成 fallback。

#### Step 2: 引入冻结快照和纯 effect

目标文件：

- `consensus/src/pipeline/virtual_processor/cell_processing.rs`
- `consensus/src/pipeline/virtual_processor/processor.rs`

新增核心结构：

```rust
pub struct ExecutionSnapshot {
    pub cell_state_view: Arc<CellStateView>,
    pub replay_view: Arc<ReplayValidationView>,
    pub processed_txs: Arc<BTreeSet<Hash>>,
}

pub struct BlockExecutionEffect {
    pub block_hash: Hash,
    pub block_daa_score: u64,
    pub cell_diff: CellDiff,
    pub accepted_tx_ids: Vec<Hash>,
    pub acceptance_entries: Vec<Hash>,
    pub reward_data: BlockRewardData,
}
```

要求：

- `ExecutionSnapshot` 只能读
- `BlockExecutionEffect` 必须完整表达提交所需副作用
- effect 本身不能藏隐式外部依赖

#### Step 3: 直接建立 block-level access summary

目标文件：

- `consensus/src/pipeline/virtual_processor/cell_processing.rs`
- 需要时新增 `consensus/src/pipeline/virtual_processor/access_summary.rs`

为每个 blue block 生成：

```rust
pub struct BlockAccessSummary {
    pub block_hash: Hash,
    pub spent_outpoints: BTreeSet<OutPoint>,
    pub created_outpoints: BTreeSet<OutPoint>,
    pub read_deps: BTreeSet<OutPoint>,
    pub tx_ids: BTreeSet<Hash>,
}
```

这一步不需要后向兼容，所以不要试图复用旧的 `processed_txs + replay_validation` 隐式语义。

直接让 access summary 成为是否可并行的唯一依据。

#### Step 4: 建立 block-level execution DAG

目标文件：

- `exec/src/scheduler/dag.rs`
- 或新增一个更贴近 `virtual_processor` 语义的 block-level DAG 构造器

注意，这一步最好不要强行复用 tx-level `CellDAG` 原类型。

更干净的方案是：

- 保留 tx-level `CellDAG` 给模板/块内分析
- 为 `MPE` 新增 block-level `ExecutionDAG`

原因是这两者虽然都叫 DAG，但对象不同：

- 一个节点是 tx
- 一个节点是 blue block effect

一次性方案下，建议直接新建 block-level DAG，避免把 tx-level 代码扭成双用途。

#### Step 5: 逐层并行分析，逐层顺序提交

目标文件：

- `consensus/src/pipeline/virtual_processor/cell_processing.rs`
- `exec/src/scheduler/executor.rs` 或新增 block-level executor

执行模型直接改成：

1. GhostDAG 先给出 canonical order
2. 用 `BlockAccessSummary` 构造 block-level execution DAG
3. 每一层 `par_iter()` 跑 `analyze_blue_block(...)`
4. 每一层完成后，按 canonical order 顺序 `commit_block_effect(...)`

这一步是一刀切方案的核心。

如果做不到这一步，就说明还没有真正完成 `MPE`。

#### Step 6: 删掉旧状态机残件

一次性方案里，这一步不要犹豫保留旧桥。

应该删除或降级为内部 helper 的对象包括：

- 旧的 `process_blue_block(...)` 共享写路径
- 旧的 `processed_txs` 外层累积逻辑
- 旧的 `ReplayValidationContext` 可变重放路径
- 所有“先从当前 tree 试一下，不行再宽容 fallback”的执行残件

保留旧代码只会让后续维护同时面对两套语义。

#### Step 7: 一次性补齐 reference tests

目标文件：

- `consensus/src/pipeline/virtual_processor/tests.rs`
- `consensus/src/pipeline/virtual_processor/cell_tests.rs`
- 必要时新增 `consensus/tests/virtual_processor_parallel.rs`

必须一次性补上的测试：

1. same mergeset, different import order, same `cell_root`
2. duplicate tx across blue blocks
3. double spend across blue blocks
4. created-then-spent across different blue blocks
5. red/blue reward 与 acceptance data 稳定性
6. 串行 runner 与并行 runner 结果完全一致

如果没有这组 reference tests，一次性改完也不算安全落地。

### 12.4 一次性方案会破坏什么

这条路线默认接受下面这些破坏：

- `virtual_processor` 内部函数签名大改
- 现有局部 helper 大量删除
- 依赖旧 effect/旧 replay 语义的测试整体重写
- `exec/scheduler` 可能新增 block-level DAG/executor，而不是继续保持只有 tx-level 版本

但它不应该破坏：

- GhostDAG 模型
- block/header 承诺语义
- RPC 对外协议
- 已有 `P1`/`P2a` 的完成结果

### 12.5 一次性方案最大的风险

一次性重构最大的风险，不是编译不过，而是：

- 你以为自己只在“重构实现”
- 实际上悄悄改了 duplicate tx / double spend / acceptance / reward 的协议语义

所以一次性方案最需要的不是更多开发人手，而是：

- 一份先写好的规范结论
- 一套 reference tests
- 一个可以和旧串行 runner 对拍的阶段

### 12.6 一次性方案的 Go / No-Go

只有在团队接受下面这句话时，才该走这条路：

`我们愿意把 virtual-state 执行层当作一个可以整体替换的内部实现，而不是继续给旧路径打补丁。`

如果团队还没准备好接受这个前提，就不要走一次性方案。

---

最终结论：

MPE 值得做，但先重构执行语义；GhostDAG 不需要改。

## 13. 外部对比说明

本文关于 CKB 的对比，参考的是 CKB 官方主线仓库截至 2026-04-12 的公开代码：

- `script/src/verify.rs`
- `verification/src/block_verifier.rs`
- `verification/src/transaction_verifier.rs`

这些参考的用途只有一个：

- 说明 CKB 没有与 Spora `MPE` 完全同型的问题空间

它们不是这份设计文档的规范来源；这份文档的规范来源仍然是 Spora 自己当前的 GhostDAG 和 virtual processor 代码。
