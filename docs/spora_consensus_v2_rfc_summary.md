# Spora 共识架构 V2 RFC 摘要

- 日期: 2026-04-11
- 状态: Draft for Review
- 正文: [spora_consensus_architecture_v2.md](/Users/arthur/RustroverProjects/Spora/docs/spora_consensus_architecture_v2.md)
- 差距清单: [spora_consensus_v2_gap_analysis.md](/Users/arthur/RustroverProjects/Spora/docs/spora_consensus_v2_gap_analysis.md)
- Issue 清单: [spora_consensus_v2_issue_list.md](/Users/arthur/RustroverProjects/Spora/docs/spora_consensus_v2_issue_list.md)

## 1. 这份 RFC 要解决什么问题

Spora 当前已经从“Cell on DAG 的原型想法”走到了“主状态承诺路径基本成立”的阶段，但协议定义和实现入口还没有完全收口。

V2 RFC 的目标只有一个：

- 让 Spora 的共识定义从“实现驱动”切换到“协议驱动”

换句话说，先定义什么是协议真相，再要求代码逐步贴齐。

## 2. 提案核心

V2 提案把 Spora 定义为：

- `Kaspa / GhostDAG` 负责排序
- `CKB / Cell` 负责状态表达
- `CKB-VM` 负责脚本执行
- `accepted_id_merkle_root + cell_root + cell_commitment` 负责把 accepted 结果和 live Cell 状态绑定到 header

这意味着 Spora 既不是：

- 单纯“把 CKB 放进 DAG”

也不是：

- 单纯“给 Kaspa 加一个脚本字段”

它是一个新协议，必须把三者在协议层焊死：

- 排序
- 状态
- 执行

## 3. V2 的三条核心决策

### 决策 1: 状态视角必须是 POV-aware

任何 Cell live / spent / mature / 可花费性判断，都必须相对于：

- `pov block hash`

而不能只相对于：

- `daa score`

原因：

- DAG 中同一个 `DAA` 不能唯一决定状态视角
- `POV` 才能把状态判断锚定到具体的 selected-parent 上下文

### 决策 2: `selected_parent` 的已提交状态是唯一合法起点

每个块的状态计算都必须从：

- `selected_parent committed state`

开始，而不是从：

- 当前 `virtual_state`
- 某个本地缓存树
- 某个“接近正确”的 root 恢复结果

原因：

- `virtual` 只是缓存
- 只有 `selected_parent` 能成为协议层的单一状态起点

### 决策 3: 必须有单一 State Transition Engine

以下入口必须共享同一套状态转移语义：

- `body validation in context`
- `reorg replay`
- `mempool admission`
- `block template validation`

原因：

- 如果不同入口跑不同版本的规则，协议就没有闭环
- “最终在某个更晚阶段才拒绝”不等于协议设计正确

## 4. 和旧体系相比，到底变了什么

### 相比 CKB

新增负担：

- `GhostDAG`
- `selected_parent`
- `ordered_mergeset`
- red / blue reward 语义
- `POV-aware` 状态查询

保留核心：

- Cell 生命周期
- `lock/type script`
- `since`
- VM 执行模型

### 相比 Kaspa

新增负担：

- Cell diff 不再只是 legacy txout diff
- 输入检查不再只是 spend/create
- 需要 VM / script group / data provider
- 历史状态和 time lock 更复杂

保留核心：

- `GhostDAG`
- `selected_parent`
- mergeset / accepted / pruning / virtual

## 5. V2 成立的最低标准

V2 不是写了文档就算成立。至少要满足这四条：

1. 正式协议只承认 `POV-aware` 历史状态语义。
2. `body/reorg/mempool/template` 使用同一套状态转移引擎。
3. `CKB-VM` 和真实 data provider 属于共识，而不是可选插件。
4. `accepted_id_merkle_root`、`cell_root`、`cell_commitment` 都由同一 working state 导出。

只要这四条有一条没做到，V2 就仍然只是“方向正确”，不是“闭环协议”。

## 6. 当前实现进度

### 已基本具备

- `selected_parent + ordered_mergeset` 主路径
- `cell_root` / `cell_commitment` / `accepted_id_merkle_root` 校验
- 缺失输入、双花、capacity 不守恒等核心拒绝逻辑
- `POV-aware` Cell provider

### 仍需补完

- `body_validation_in_context` 还没有接到完整 Cell 状态机
- `mempool` 还是简化实现
- `template validation` 仍部分简化
- `verify_scripts` 还没有用真实 consensus-backed data provider

## 7. 评审时应重点讨论的不是“代码细节”，而是这些协议问题

1. `POV` 的正式定义是否足够明确，能否支撑第二实现。
2. `accepted_tx_order` 是否要被明确定义为规范对象，而不是实现副产物。
3. `cell_commitment` 的 v1 / v2 升级路径是否要在现在就预留。
4. `VM` 的版本激活、cycles 计费、syscall 兼容规则是否要纳入本 RFC。
5. `CellDB/get_cell_at_daa` 是否正式降级为非共识接口。

## 8. 推荐决策

如果团队要推进 V2，我建议在评审会上直接做三项决策：

1. 接受 V2 作为新的协议主文档。
2. 正式把旧 `spora_ghostdag_cell_architecture.md` 降级为历史参考。
3. 同意后续所有 Cell / mempool / template / VM 改动都以“统一 State Transition Engine”为收敛目标。

## 9. 一句话结论

V2 的意义不是“把现在的实现写漂亮”，而是给 Spora 一个可以真正长期演进的协议骨架。  
如果 CKB 负责告诉我们“状态怎么表达”，Kaspa 负责告诉我们“DAG 怎么排序”，那 V2 负责回答最后那个最难的问题：

- 这两件事怎样在同一个共识状态机里成立。
