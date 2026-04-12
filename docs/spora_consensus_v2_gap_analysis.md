# Spora 共识架构 V2 实现差距清单

- 日期: 2026-04-11
- 状态: Draft
- 目标文档: [spora_consensus_architecture_v2.md](/Users/arthur/RustroverProjects/Spora/docs/spora_consensus_architecture_v2.md)
- 用途: 把 V2 规范要求直接映射到当前代码实现，区分“已完成”“部分完成”“未完成”

## 1. 如何读这份文档

主架构文档回答的是：

- 协议应该长什么样
- 共识流程应该如何定义
- Spora 和 CKB / Kaspa 的边界是什么

这份差距清单回答的是：

- 当前代码已经实现了哪些协议要点
- 还有哪些地方仍然停留在迁移态
- 下一步应该优先改哪里

这里的判断标准不是“测试是否通过”，而是：

- 是否已经进入主共识路径
- 是否已经成为共享状态机的一部分
- 是否已经脱离 placeholder / TODO / simplified 路径

## 2. 总体判断

当前实现的状态可以概括为：

- `GhostDAG + Cell state root + accepted_id_merkle_root + cell_commitment` 的主链路已经基本成型
- 若干致命实现 bug 已修复，主验证路径已经能拒绝缺失输入、重复花费、capacity 不守恒等情况
- 但协议设计要求的“单一状态转移引擎”还没有完全落地
- `body validation`、`mempool`、`template`、`VM data provider` 仍然存在迁移态实现

因此，当前结论应当是：

- 主共识状态机已经具备 V2 雏形
- 但还不能说协议实现已经完全闭环

## 3. 状态摘要

| 能力 | 规范要求 | 当前状态 |
|---|---|---|
| GhostDAG 排序 | 必须有 `selected_parent + ordered_mergeset` | 已完成 |
| POV-aware 历史视角 | 必须以 `pov block hash` 为锚点 | 已完成 |
| 共享状态转移 | `body/reorg/mempool/template` 共用一套逻辑 | 部分完成 |
| Cell 主路径校验 | 缺失输入、双花、capacity、maturity 必须拒绝 | 已完成 |
| 头部承诺校验 | `accepted_id_merkle_root + cell_root + cell_commitment` | 已完成 |
| VM 执行入共识 | 真实 data provider + script verify | 部分完成 |
| mempool 一致性 | 与正式验块共用状态机 | 未完成 |
| template 一致性 | 与正式验块共用状态机 | 部分完成 |
| 旧 `DAA-only` 接口降级 | 只能作为索引，不是协议定义 | 部分完成 |
| 证明闭环 | model tests / second implementation | 未完成 |

## 4. 已经完成的部分

### 4.1 POV-aware Cell 查询已经落地

V2 要求协议级历史查询只能写成：

- `get_cell_at_pov(outpoint, pov_block)`

当前实现已经有对应主路径：

- `consensus/src/consensus/cell_provider.rs`
- `consensus/src/processes/cell_validator/cell_validation_in_dag.rs`

现状判断：

- `ConsensusCellProvider` 会沿 `selected_parent` 链和 `cell_diff` 回溯状态
- 共识侧已经不再以 `DAA-only` 作为唯一状态语义

结论：

- 这一条与 V2 一致

### 4.2 主状态机已经能拒绝关键非法交易

当前 `virtual_processor` 的 Cell 状态计算已经具备这些能力：

- 缺失输入硬失败
- mergeset 内重复花费硬失败
- coinbase maturity 检查
- 非 coinbase 交易 capacity 守恒检查
- 重复 outpoint 创建检查

关键代码路径：

- `consensus/src/pipeline/virtual_processor/cell_processing.rs`

结论：

- 这一条已经进入主链路，不再是“文档设计”

### 4.3 头部承诺校验已经闭环

当前主路径会校验：

- `accepted_id_merkle_root`
- `cell_root`
- `cell_commitment = H("spora/cell_commitment/v0" || cell_root)`

关键代码路径：

- `consensus/src/pipeline/virtual_processor/processor.rs`
- `consensus/src/pipeline/pruning_processor/processor.rs`

结论：

- 这一条已经和 V2 定义一致

### 4.4 selected_parent 状态重建已经走对方向

当前实现已经不再直接拿 `virtual_state` 冒充父状态，而是：

- 从 `virtual_state + diff` 重建 `selected_parent` 状态
- 对重建出来的 root 做一致性校验

关键代码路径：

- `consensus/src/pipeline/virtual_processor/processor.rs`

结论：

- 这修掉了早期最危险的一类共识错误

## 5. 部分完成的部分

### 5.1 `body validation in context` 还没有接到完整 Cell 状态机

V2 要求：

- `body validation in context` 必须调用真实的 Cell context / DAG / script 校验

当前代码：

- `consensus/src/pipeline/body_processor/body_validation_in_context.rs`

现状：

- 这里只做了 isolation 检查
- coinbase payload / subsidy 等上下文检查是有的
- 但非 coinbase 交易没有在这里跑完整的 Cell context / DAG / VM 逻辑

影响：

- 最终非法块仍会在后续状态处理中被拒绝
- 但协议实现还没有达到“单一状态机，多入口复用”的目标

结论：

- 部分完成，不是最终形态

### 5.2 `CellValidator` 已存在，但还没有成为唯一真相实现

V2 要求：

- isolation / context / DAG / script 应该由共享的状态转移引擎统一实现

当前代码：

- `consensus/src/processes/cell_validator/mod.rs`

现状：

- `validate_in_isolation`
- `validate_in_context`
- `validate_in_dag`
- `validate_full_with_scripts`

这些接口都已经存在，但还没有全面接入：

- `body validation`
- `mempool admission`
- `template validation`
- `reorg replay`

影响：

- 有正确方向
- 但还没有完成“统一状态机收敛”

结论：

- 部分完成

### 5.3 `VM script verification` 还停留在 placeholder data provider

V2 要求：

- VM 必须属于共识
- data provider 必须来自真实共识存储

当前代码：

- `consensus/src/processes/cell_validator/mod.rs`

现状：

- `verify_scripts` 已存在
- 但使用的是 `SimpleDataProvider::new()`
- 注释里明确写着 `TODO: Use real data provider from consensus storage`

影响：

- 接口存在，不代表已经进入正式共识
- 没有真实 data provider，就无法说“CKB-VM 已被协议焊死”

结论：

- 部分完成

### 5.4 旧 `get_cell_at_daa` 还保留在状态索引层

V2 要求：

- `DAA-only` 接口只能作为索引或调试接口存在

当前代码：

- `state/src/index/cell_db.rs`

现状：

- 接口仍存在
- 相关测试仍存在
- 旧文档和一些辅助文档里还有残留引用

影响：

- 只要协议正文和主路径不再依赖它，就不是致命问题
- 但语义上仍需要继续降级，避免团队误用

结论：

- 部分完成

## 6. 尚未完成的部分

### 6.1 mempool 还没有接入真实 Cell 共识校验

V2 要求：

- mempool 准入必须与正式验块共享状态机

当前代码：

- `consensus/src/pipeline/virtual_processor/processor.rs`

现状：

- `validate_mempool_transaction` 仍然是简化实现
- `validate_mempool_transactions_in_parallel` 直接返回 `vec![Ok(()); ...]`
- `populate_mempool_transaction` 基本还是 placeholder
- 文件里保留了多处 `TODO(cell-model)` 和 `unimplemented!` 注释

影响：

- 这不是纯 policy 问题
- 说明 mempool 还没有真正切到 V2 状态机

结论：

- 未完成

### 6.2 block template 还没有使用完整的 Cell 校验与 fee 语义

V2 要求：

- template builder 必须和正式验块生成同一组承诺和相同的交易可接受性结论

当前代码：

- `consensus/src/pipeline/virtual_processor/processor.rs`

现状：

- `validate_block_template_transaction` 仍返回简化 fee
- `validate_block_template_transactions` 仍写着 `Simplified - full Cell validation pending`
- template 构造已经会生成 `cell_root / cell_commitment / accepted_id_merkle_root / coinbase`
- 但交易筛选和准入还没有完全共享正式状态机

影响：

- 比 mempool 更接近 V2
- 但还不是协议闭环要求下的最终版本

结论：

- 部分完成

### 6.3 证明闭环还没开始

V2 要求：

- 顺序无关性测试
- split/reorg replay 测试
- second implementation / model checker 对拍

当前代码：

- 包内测试已经能覆盖若干主路径 bug
- 但还没有形成正式的协议证明层

影响：

- “测试通过” 只能说明当前实现没有显式炸掉
- 不能证明协议在复杂 DAG 导入顺序下必然等价

结论：

- 未完成

## 7. 与 CKB / Kaspa 的实现差距怎么理解

对照旧体系时，最容易误判的地方有两个。

### 7.1 相比 CKB

CKB 的难点在：

- Cell 语义
- script group
- `since`
- VM 执行

Spora 已经补上了部分 Cell 状态与承诺，但还没把完整 VM data provider 和全入口共享状态机收口。  
所以当前更像：

- `DAG ordering` 已经成型
- `Cell state` 主路径已成型
- `CKB-VM as consensus` 还在收尾

### 7.2 相比 Kaspa

Kaspa 的难点在：

- `selected_parent`
- `ordered_mergeset`
- accepted 集
- virtual / pruning

Spora 已经把其中的排序层和状态承诺层结合起来了，但比 Kaspa 多出来的 Cell / VM 复杂度还没有完全落到 mempool 和 template。  
所以当前更像：

- 主共识路径已经比纯 Kaspa 更重
- 外围入口还没有全部共识化

## 8. 下一阶段的精确任务

### P0

1. 把 `body_validation_in_context` 接到共享 `CellValidator / State Transition Engine`
2. 用真实 consensus-backed data provider 替换 `SimpleDataProvider`
3. 把 `mempool admission` 改成调用正式 Cell 状态机

### P1

1. 把 `template validation` 从简化路径切到正式状态机
2. 明确废弃 `get_cell_at_daa` 的协议语义，只保留索引定位
3. 清理旧文档、旧测试、旧注释里的 `DAA-only` 表述

### P2

1. 建立 DAG 顺序无关性 / reorg replay / accepted root 一致性测试集
2. 设计 second implementation 或 model checker 对拍方案
3. 评估增量状态树与历史证明扩展

## 9. 一句话结论

Spora V2 现在已经不是“概念草图”了，但也还不是“协议闭环成品”。  
最准确的判断是：

- 排序层和主状态承诺层已基本成型
- 共享状态机和 VM 共识化还差最后一段路
