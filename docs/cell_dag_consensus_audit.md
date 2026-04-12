# Cell DAG 共识审计

- 日期: 2026-04-11
- 审计范围: `GhostDAG + Cell` 共识设计与当前实现
- 审计方式: 静态代码审计
- 结论摘要: 当前实现下，这个 `cell dag` 方案还不能认为“共识已经成立”

## 结论

按设计方向看，`GhostDAG + Cell` 并非不可行；但按当前仓库中的代码与文档状态，这套方案还没有形成一个自洽、闭环、可验证的共识实现。

当前问题不是“还有些功能没补完”这么简单，而是已经存在落在共识关键路径上的设计缺口和实现缺陷：

- 历史 Cell 查询模型不足以唯一确定 DAG 状态视角
- 虚拟态和重组路径没有从选定父状态正确重建
- 块级验收路径没有真正接入完整的 Cell 上下文校验
- mergeset 内重复消耗同一 Cell 时可能破坏 capacity 守恒
- 状态转移函数对缺失输入存在宽容回退，可能放过无中生有的状态转移
- diff 组合语义本身存在错误，且组合方式前后不一致
- 本地出块模板与本地验证规则互相矛盾

因此，当前更准确的定位是：

- 一个正在进行 Cell 化迁移的原型
- 不是一个已经完成、可安全部署的共识方案

## 关键发现

### [P0] 仅靠 `DAA score` 查询历史 Cell 状态，在 DAG 中语义不足

当前接口将历史状态查询建模为：

- `get_cell_at_daa(out_point, daa)`

但在 DAG 中，“某个 DAA 时刻 Cell 是否 live”并不是一个只由 `daa` 唯一决定的命题；它还依赖一个明确的视角块，例如：

- 当前验证块的 `selected_parent`
- 当前验证块的 `POV`
- 某个具体分支的 past set

也就是说，在 fork / reorg 场景下，同一 `DAA` 下不同分支可能给出不同状态答案。

当前实现与文档都把这个问题简化成了纯 `DAA` 查询：

- `consensus/src/processes/cell_validator/cell_validation_in_dag.rs:11`
- `state/src/index/cell_db.rs:234`
- `docs/spora_ghostdag_cell_architecture.md:178`

这意味着当前状态接口本身还不足以支撑严格共识。

### [P0] 虚拟态和重组路径没有从 selected parent 的 Cell 状态正确重建

当前虚拟态计算路径表面上接收了 `selected_parent_cell_root`，但实际没有从该父状态恢复树，而是直接复用当前 virtual tree：

- `consensus/src/pipeline/virtual_processor/processor.rs:609`
- `consensus/src/pipeline/virtual_processor/processor.rs:617`

同时：

- `CellStateTree::apply_diff_placeholder()` 仍是空实现
- `cell_roots_store` 即使读到了 root，也没有用于真正恢复状态树
- 补算时直接 fallback 到当前 `virtual_state.cell_state_tree`

相关位置：

- `state/src/cell_tree.rs:118`
- `consensus/src/pipeline/virtual_processor/processor.rs:464`

这会导致一个严重后果：

- 验块结果可能依赖“本地当前 virtual tip”
- 而不是依赖“被验证块自己的 selected parent 状态”

这是共识级错误，不是缓存或性能问题。

### [P0] 块验收路径没有真正做完整的 Cell 上下文校验

当前 body/context 校验明确写着：

- 暂时只做 isolation 校验
- 完整 context + DAG 校验没有在这里闭环

相关代码：

- `consensus/src/pipeline/body_processor/body_validation_in_context.rs:24`
- `consensus/src/pipeline/body_processor/body_validation_in_isolation.rs:59`

与此同时，`CellValidator` 虽然写了接口和模块，但没有真正接到关键验块路径中：

- `consensus/src/processes/cell_validator/mod.rs:50`
- `consensus/src/pipeline/virtual_processor/processor.rs:163`

这意味着当前块处理路径并没有严格验证：

- 输入是否真的存在
- 输入是否在该 DAG 视角下未花费
- cellbase maturity 是否成立
- 历史状态是否与目标分支一致

### [P0] mergeset 内重复消耗同一 Cell 时，当前 diff 构造可能只扣一次输入却保留两边输出

这是另一份审计里指出且我认同的关键问题。

`calculate_cell_state()` 在处理 mergeset blues 时，会把每笔交易的输入写入 `ctx.mergeset_cell_diff.remove`，输出写入 `ctx.mergeset_cell_diff.add`。但 `CellDiff` 的底层是 `BTreeMap<TransactionOutpoint, CellMeta>`：

- 同一个输入 outpoint 被两笔交易重复消耗时，`remove_cell()` 的第二次写入会覆盖第一次
- 两笔交易各自创建的 outputs 则都会保留在 `add` 中

相关位置：

- `consensus/src/pipeline/virtual_processor/cell_processing.rs:178`
- `consensus/core/src/cell_diff.rs:44`
- `consensus/core/src/cell_diff.rs:111`

如果两个蓝色块在同一 mergeset 中都消耗了同一个 Cell，那么当前实现没有在 Cell 状态计算层做硬失败，结果可能变成：

- 被消耗的输入只在 diff 中出现一次
- 两边输出都进入新状态

这会直接破坏 capacity 守恒，属于共识级漏洞。

### [P0] 缺失输入时的状态转移是宽容 no-op，可能放过凭空造状态

这是当前最危险的实现问题之一。

在 `calculate_cell_state()` 里，若一个输入对应的 Cell：

- 不在当前 tree 中
- 也不在本 mergeset 已新增集合中

代码不会报错，而是构造一个全零的占位 `CellMeta`，然后继续做 `remove_cell`：

- `consensus/src/pipeline/virtual_processor/cell_processing.rs:184`
- `consensus/src/pipeline/virtual_processor/cell_processing.rs:205`

由于树上本来就没有这个输入，后续 `remove` 基本等价于“不扣任何输入”；而输出仍然会正常加入状态。

结果就是：

- 无效交易可能被计算进 `cell_root`
- 只要出块者按这套错误逻辑构造块头，当前节点就可能接受错误状态

这已经不是“会误报拒绝合法块”，而是“可能接受非法块”。

### [P1] `CellDiff` 组合语义错误，且 `merge()` 与 `with_diff_in_place()` 混用

这是另一份审计里指出的第二组高价值问题，我也认同。

当前 `CellDiff::with_diff_in_place()` 在处理“先 add，后 remove 同一 outpoint”时，语义是错的：

- 正确语义应该是“净变化为无操作”
- 当前实现却会把该 outpoint 放进 `remove`

相关位置：

- `consensus/core/src/cell_diff.rs:156`

同时，调用方对 diff 累积使用了两套不同语义：

- 在一处使用 `with_diff_in_place()`
- 在另一处直接使用 `merge()`

相关位置：

- `consensus/src/pipeline/virtual_processor/processor.rs:423`
- `consensus/src/pipeline/virtual_processor/processor.rs:451`
- `consensus/src/pipeline/virtual_processor/processor.rs:497`

问题在于：

- `merge()` 只是简单扩展两个 map，不处理状态抵消
- `with_diff_in_place()` 试图处理组合，但其实现本身又有 bug

这会让 diff 的可组合性、可逆性和重组稳定性都失去可信性。

### [P1] 本地出块模板和本地验证规则互相矛盾

当前模板构造逻辑中：

- `cell_commitment` 被直接写成 `cell_root`
- `accepted_id_merkle_root` 被写成 `ZERO_HASH`
- coinbase 插入被注释掉，`txs` 甚至可能为空

相关代码：

- `consensus/src/pipeline/virtual_processor/processor.rs:1139`
- `consensus/src/pipeline/virtual_processor/processor.rs:1151`
- `consensus/src/pipeline/virtual_processor/processor.rs:1158`

但验证逻辑要求：

- `cell_commitment = H("spora/cell_commitment/v0" || cell_root)`

相关代码：

- `consensus/src/pipeline/virtual_processor/processor.rs:555`

同时 body isolation 校验还要求：

- 第一笔必须是 coinbase

相关代码：

- `consensus/src/pipeline/body_processor/body_validation_in_isolation.rs:47`

这说明当前节点本地产出的块模板与当前节点自己的验证规则并不一致，系统尚未闭环。

## 设计层判断

### 这个方案“思路上”是否成立

思路上可以成立，但需要满足至少以下条件：

1. Cell 状态查询必须是 `POV-aware`
2. 每个块的 `cell_root` 必须仅由：
   - selected parent 的确定状态
   - 当前块的确定 diff
   推导出来
3. 重组时必须能从 split point 以后做确定性回滚/重放
4. 块级状态校验必须先于状态提交
5. 状态转移对缺失输入必须硬失败，不能 fallback

### 当前代码是否满足这些条件

不满足。

尤其是第 1、2、4、5 条都存在明显缺口。

## 风险评级

### 共识成立性

- 评级: 不成立

理由：

- 状态视角定义不完整
- 关键路径依赖 placeholder / fallback
- 验块与出块规则不一致
- 存在可能接受非法状态转移的路径

### 安全性

- 评级: 高风险

理由：

- 缺失输入可能被静默吞掉
- 重组路径可能基于错误父状态计算 `cell_root`
- 历史查询模型无法严格表达 DAG 状态

### 工程完成度

- 评级: 原型 / 迁移中

理由：

- 大量 `TODO(cell-model)` 仍在关键路径
- 多个测试文件仍是占位
- validator 已存在，但未真正接入主验块流程

## 建议修复顺序

### 第一优先级

1. 把历史查询接口从 `get_cell_at_daa(out_point, daa)` 改成带 `POV` 的接口
2. 删除所有“输入不存在时继续往下算”的宽容分支，改为硬失败
3. 让块级 context validation 真正调用完整的 Cell stateful validator
4. 让虚拟态计算基于 selected parent 的真实 Cell 状态重建，而不是复用当前 virtual tree

### 第二优先级

1. 实现 `CellStateTree` 的真实 diff 应用与重建能力
2. 统一模板构造与验证规则中的 `cell_commitment` 定义
3. 关闭 `accepted_id_merkle_root = ZERO_HASH` 之类的临时占位
4. 完成 coinbase 与 block template 的 Cell 化闭环

### 第三优先级

1. 补齐 reorg / fork / double-spend / maturity 的集成测试
2. 修正 `CellDiff` 的组合语义，并统一所有路径使用同一套 diff 累积规则
3. 在 mergeset 处理阶段对重复消耗同一 outpoint 做硬失败
4. 补齐被消耗 Cell 的完整元数据恢复，避免 `block_daa_score` / `data_bytes` 被错误覆盖
5. 用真实多分支 DAG 用例验证 `cell_root` 的稳定性和可重复性
6. 为状态查询和状态承诺写出正式规范，而不是仅靠实现约定

## 与另一份审计的交叉核对

我对另一份 AI 审计报告做了交叉比对。结论是：它有明显参考价值，但不能直接原样作为最终报告。

### 我确认成立的点

- mergeset 内重复消耗同一 Cell，当前实现确实可能只扣一次输入却保留两边输出
- 引用不存在 Cell 时使用零元数据继续计算，确实是硬 bug
- 虚拟态重建错误地复用了当前 virtual tree，而不是 selected parent 的真实状态
- `CellDiff::with_diff_in_place()` 的 add-then-remove 语义确实有问题
- `merge()` 与 `with_diff_in_place()` 混用，确实会放大 diff 语义不一致

### 我认为需要降级或修正的点

- `CellDB` 只保留单条 spend journal，确实不利于复杂 reorg 历史查询，但它更像历史索引缺陷，不是当前最直接的共识主路径问题
- `CellStateTree::root()` 的 O(n) 重建是明显性能问题，但不直接决定共识是否成立
- 奇数叶子直接提升是树定义选择，除非协议另有规定，否则不能单独定性为共识漏洞
- `ConflictKey` 的浮点精度问题属于 mempool 排序稳定性问题，不应与共识主路径漏洞放在同一优先级
- 时间锁仅实现绝对 DAA 锁，说明功能未完成，但它属于未完成验证能力，不应盖过前面的共识硬缺陷

### 交叉核对后的最终判断

综合两份审计后，结论没有变化，反而更稳固：

- 这个方案在概念上可以成立
- 但当前代码实现下，共识还没有成立
- 且已经存在足以导致错误验块、错误重组、甚至接受非法状态转移的关键路径缺陷

## 测试与验证说明

本次结论主要来自静态代码审计。

我尝试运行以下测试：

```bash
cargo test -p spora-consensus cell_validator --quiet
cargo test -p spora-state cell_db --quiet
```

其中 `spora-state` 测试在当前环境失败，原因不是业务逻辑本身，而是本地构建环境缺少 `libclang.dylib`，导致 `librocksdb-sys` 无法编译。

因此：

- 这份审计结论不依赖测试失败才成立
- 即便不跑测试，关键问题已经能从代码路径直接确认

## 最终判断

最终判断是：

- `GhostDAG + Cell` 作为方向可以成立
- 但当前仓库里的 `cell dag` 共识实现还没有成立
- 且存在共识关键路径上的硬缺陷，不能视为可安全运行的完成态

如果要把这套方案推进到“共识成立”的程度，必须先补齐状态视角定义、父状态重建、完整验块接线，以及对非法输入的硬失败语义。
