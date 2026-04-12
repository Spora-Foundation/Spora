# Spora Cell 模型迁移路线图

- 日期: 2026-04-12
- 状态: Draft
- 目的: 把“术语清理”升级为“协议与实现迁移计划”
- 关联文档:
  - [spora_consensus_architecture_v2.md](/Users/arthur/RustroverProjects/Spora/docs/spora_consensus_architecture_v2.md)
  - [spora_cell_architecture_review.md](/Users/arthur/RustroverProjects/Spora/docs/spora_cell_architecture_review.md)
  - [CELL_TERMINOLOGY_PURGE_PLAN.md](/Users/arthur/RustroverProjects/Spora/docs/CELL_TERMINOLOGY_PURGE_PLAN.md)

## 1. 文档定位

这份文档不是“还能把哪些词从仓库里删掉”的清单。

它定义的是：

- 哪些模块只是术语残留
- 哪些模块是过渡桥接层
- 哪些概念最终应该删除
- 删除它们之前必须先打通哪些 canonical Cell 路径

一句话总结：

> Spora 当前的真实状态不是“已经完成 Cell 化”，而是“Cell 共识内核 + legacy Transaction 适配外壳”。

因此接下来的工作不能再按“大范围 rename / delete checklist”推进，而要按迁移阶段推进。

## 2. 当前判断

### 2.1 已经完成的部分

- 代码层的 legacy 术语清理已经基本完成
- `CellDiff`、`CellStateTree`、`cell_root`、`cell_commitment` 这些内核对象已经是主路径
- `CellEntry`、`RpcCellEntry`、`CompactCellEntry`、client/wallet wrapper 已经开始承载 canonical cell metadata
- `sighash`、`mass`、`MutableTransaction`、`PopulatedTransaction` 已经开始优先消费 resolved metadata
- `RpcTransactionOutput`、`RpcCellEntry`、部分 block/RPC bridge 已经不再是纯 legacy 壳

### 2.2 还没有完成的部分

- `Transaction` 仍是大面积兼容桥
- `VerifiableTransaction`、`MutableTransaction` 仍然围绕 legacy `TransactionInput/TransactionOutput` 组织
- `ScriptPublicKey` 仍深度渗透在 RPC、钱包、地址、脚本、签名路径
- `txscript` 仍是钱包与验证外围的重要承重件
- `subnetwork / gas / sig_op_count / payload / lock_time / sequence` 仍然是结构字段而不是已退役兼容字段
- `get_virtual_cells` / `get_pruning_point_cells` 仍是空壳，说明状态查询层还没有彻底 Cell-native

### 2.3 禁止继续做的事

以下做法会让代码库更乱，而不是更接近目标状态：

- 先删 `Transaction` 的 5 个 legacy 字段，再等编译器报错后到处补
- 在 `ScriptPublicKey` 仍是主入口时，直接删除 `txscript`
- 在 RPC / wallet / index 仍依赖 legacy bridge 时，把 `CellEntry` 直接替换成另一个最终结构
- 用“语义改名”掩盖真实的桥接层和 stub

## 3. 迁移原则

### 3.1 先打通 canonical path，再删除 legacy path

删除不是起点，删除是结束条件。

任何 legacy 概念只在满足以下条件后才能删：

- 已有 canonical Cell 对应物
- 共识主路径已经在用 canonical 对象
- RPC / wallet / index 至少有一条完整可用链路
- 测试和同步路径不再依赖 legacy bridge

### 3.2 优先清理“假 Cell 壳”

危险度最高的不是旧名字，而是：

- 名字叫 `Cell*`
- 结构和语义仍是 legacy `CellEntry/Transaction` 兼容桥
- 或者干脆返回空列表 / 空 iter / stub

因此优先级应是：

1. 消灭 fake Cell 壳
2. 打通 metadata-aware / CellTx-aware 主路径
3. 再删 legacy 字段和模块

### 3.3 不做 Big Bang

不采用“一天删掉 Transaction 的 5 个字段、全仓一起炸”的方案。

采用阶段式迁移：

- 先让新旧路径共存，但 canonical path 优先
- 然后逐层切调用者
- 最后移除 legacy path

## 4. 阶段路线

## Phase 0: 停止“术语驱动”重构

### 目标

- 不再把 rename 当成完成迁移
- 所有后续工作都按“协议对象是否真的替换”评估

### 工作项

- 冻结 `CELL_TERMINOLOGY_PURGE_PLAN` 为历史术语清理文档
- 后续协议迁移统一记录在本路线图
- 对外沟通时明确区分：
  - `renamed`
  - `bridged`
  - `canonical`
  - `stub`

### 完成条件

- 团队不再把“零术语残留”当成“Cell 迁移完成”

## Phase 1: 打通 canonical cell metadata 主路径

### 目标

让 `CellEntry`、RPC、client、wallet 至少能携带和传递真实 Cell 元信息，而不是只有：

- `amount`
- `script_public_key`
- `block_daa_score`
- `is_coinbase`

### 当前状态

这一步已经部分完成。

已落地的方向：

- `CellEntry` 可嵌入 canonical metadata
- placeholder script 不再只有 lock hash
- `RpcCellEntry` / `CompactCellEntry` / client `CellEntry` 已开始带 metadata
- `RpcTransactionOutput` 也已经补入 optional metadata

### 剩余工作

- 把 `CellEntry` 从“legacy bridge + metadata extension”继续推进到更明确的 canonical 载体
- 收敛 `CellMetadata <-> CellEntry <-> RPC/client` 的单一定义
- 减少 placeholder script 在非桥接路径里的可见性

### 完成条件

- canonical metadata 在 consensus -> rpc -> client -> wallet 一条链上可无损传递
- 新代码不需要依赖 fake `script_public_key` 才能恢复 cell identity

## Phase 2: 替换验证与计量骨架

### 目标

把以下核心路径从 legacy bridge 推到 metadata-aware / Cell-aware：

- mempool validate
- contextual mass
- signing / sighash
- transaction wrappers

### 当前状态

这一步也已部分完成。

已落地：

- `MutableTransaction` 有 `resolved_cell_metadata`
- `VerifiableTransaction::cell_metadata()` 已存在
- contextual mass 已优先消费 metadata
- sighash 已开始区分 canonical metadata path 和 legacy path

### 剩余工作

- 让 `MutableTransaction` 不再把 `entries: Vec<Option<CellEntry>>` 作为唯一真实来源
- 让 `SignableTransaction / PopulatedTransaction / ValidatedTransaction` 优先面向 metadata
- 把 `consensus/core/src/sign.rs` 和 `consensus/client/src/signing.rs` 从 “script_public_key + amount” 逻辑继续收口
- 把 `ConsensusApi` 的验证入口逐步从 `MutableTransaction<Transaction>` 推向 metadata-aware / CellTx-aware 输入

### 完成条件

- mass 和 sighash 不再要求调用方先伪装成 legacy `Transaction`
- mempool validate 能明确区分：
  - legacy bridge path
  - canonical Cell path

## Phase 3: 补齐查询、索引、RPC 和块体桥接

### 目标

消灭“名字是 Cell，返回值却是空/stub”的对外接口。

### 当前高优先级问题

- `get_virtual_cells` 仍返回空
- `get_pruning_point_cells` 仍返回空
- 部分 block / raw block / notification conversion 仍在桥接或 stub

### 当前已完成的一部分

- `RpcTransactionOutput` 已不再只剩 `value + scriptPublicKey`
- `rpc/core/src/convert/block.rs` 已不再把 block 交易直接丢空
- `ConsensusApi` 已新增 `calculate_verifiable_transaction_contextual_masses`
- `ConsensusApi` 已新增 `calc_cell_tx_hash_merkle_root`

### 剩余工作

- 为 live / pruning cell 查询补足可迭代且可恢复 `TransactionOutpoint` 的状态索引
- 消除 notification conversion 里的 `Default::default()` 占位符
- 收敛 `CellTx <-> RpcTransaction` 的桥接语义，明确哪些字段是 canonical，哪些仅是兼容镜像

### 前置条件

`CellStateTree` 当前只保存 `outpoint_hash -> CellEntry`，这一层无法无损恢复 outpoint。

因此必须先做其中之一：

- 让状态树节点携带原始 outpoint
- 或增加 `outpoint_hash -> TransactionOutpoint` 索引
- 或提供独立的 live-cell 枚举索引

### 完成条件

- `get_virtual_cells` / `get_pruning_point_cells` 不再是空壳
- block / raw block / notification 不再靠 `vec![]` 或 `Default::default()` 应付

## Phase 4: 用 ScriptRef 替换 ScriptPublicKey

### 目标

这是最关键的一次结构替换。

从：

- `ScriptPublicKey { version, script }`

走向：

- `ScriptRef { code_hash, hash_type, args }`

### 为什么必须单列一阶段

因为 `ScriptPublicKey` 不只是一个字段，它渗透在：

- 地址系统
- RPC 输出
- 钱包构造
- 签名验证
- `txscript`
- `CellEntry`
- 索引键

### 迁移步骤

1. 先在 canonical `CellMetadata/CellOut` 主路径中明确 `ScriptRef`
2. 再让 RPC / client / wallet 支持 `ScriptRef` 暴露
3. 再重写地址模型，按 `lock script` 或 `lock hash` 编址
4. 最后把 `ScriptPublicKey` 降级为兼容桥

### 完成条件

- 新代码不再直接以 `ScriptPublicKey` 作为 Cell 身份的一部分
- 地址系统不再由 script bytes 直接编码

## Phase 5: 替换脚本与签名体系

### 目标

从 Kaspa/Bitcoin 风格脚本系统迁移到 CKB-VM/Cell 风格脚本系统。

### 需要最终退役的模块

- `crypto/txscript/`
- `consensus/core/src/hashing/sighash.rs`
- `consensus/core/src/sign.rs`

### 但前置条件非常严格

只有在以下条件都成立后才能删：

- `ScriptRef` 已成为主路径
- 钱包签名路径已切到 canonical Cell 模型
- `exec/src/vm/` 和新的 sighash 路径能覆盖钱包、mempool、验块需求
- RPC / wallet / PSTT 已不再依赖 txscript 生成和验证标准脚本

### 完成条件

- 钱包和共识都不再调用 txscript 解释器
- sighash 只有一条 canonical Cell 路径

## Phase 6: 删除 legacy Transaction 概念字段

### 目标

这是最终删除阶段，不是起始阶段。

要删除的对象包括：

- `Transaction.subnetwork_id`
- `Transaction.gas`
- `Transaction.payload`
- `Transaction.lock_time`
- `TransactionInput.sig_op_count`
- legacy `sequence lock` 语义

### 为什么要一次性收口

这些字段散落在 RPC schema、钱包构造器、legacy sighash 和历史 bridge 上。继续保留“薄兼容层”只会让 `Transaction` 继续成为事实标准对象，拖长双轨期。

当前迁移策略改为：

- 不再新增 `CellTx <-> Transaction` 桥接层
- 不再以“先保留兼容壳、后续再清”为默认路线
- 删除 legacy 字段时，同一轮把主调用方迁到 `CellTx` 或 metadata-aware wrapper
- 编译断点属于迁移清单的一部分，不是引入新适配器的理由

### 完成条件

- `Transaction` 不再是协议主对象
- `CellTx` 成为唯一规范交易对象

## 5. 模块处理矩阵

说明：下表中的“现在能不能删 = 否”仅表示这些项不能在不迁移调用方的情况下被孤立删除；它不意味着可以继续通过兼容层长期保留这些字段或桥接逻辑。

| 模块/概念 | 当前判断 | 现在能不能删 | 删除前置条件 |
|---|---|---:|---|
| `subnets.rs` | 最终应删除 | 否 | `Transaction` 不再是主对象 |
| `Transaction.gas` | 最终应删除 | 否 | cycles 计量进入共识主路径 |
| `sig_op_count` | 最终应删除 | 否 | txscript 路径退役 |
| `Transaction.payload` | 最终应删除 | 否 | coinbase / payload 完全切到 CellTx |
| `Transaction.lock_time` | 最终应删除 | 否 | `since` 成为唯一锁定语义 |
| sequence lock 常量 | 最终应删除 | 否 | legacy input bridge 退役 |
| `crypto/txscript` | 最终应删除 | 否 | `ScriptRef + CKB-VM` 主路径完成 |
| legacy sighash | 最终应删除 | 否 | canonical Cell sighash 覆盖钱包与共识 |
| `ScriptPublicKey` | 必须重写/降级 | 否 | `ScriptRef` 进入 RPC/wallet/index |
| 地址系统 | 必须重写 | 否 | lock-based address model 定稿 |
| `CellEntry` 当前 4 字段壳 | 必须继续收敛 | 否 | canonical metadata 全链打通 |
| `get_virtual_cells` 空壳 | 必须补 | 否 | 先补可枚举状态索引 |

## 6. 近期执行顺序

按当前代码状态，下一轮工作建议按这个顺序推进：

1. 完成 `CellEntry / CellMetadata / RPC / client / wallet` 的 canonical metadata 收口
2. 继续推进 `MutableTransaction / VerifiableTransaction / sign / mass`
3. 补 `get_virtual_cells` / `get_pruning_point_cells` 的真实实现
4. 清 notification / block conversion 里的剩余 stub
5. 设计 `ScriptRef` 替换路径
6. 再讨论删 `subnetwork/gas/payload/lock_time/sig_op_count`

## 7. Definition of Done

只有同时满足以下条件，才能说“Cell 迁移完成”：

- 共识、mempool、模板构造、重组 replay 共用同一套 Cell 状态转移语义
- `CellTx` 是唯一规范交易对象
- `ScriptRef` 是唯一规范脚本引用对象
- RPC / wallet / index 不再依赖 legacy `Transaction/ScriptPublicKey`
- txscript / legacy sighash / subnetwork / gas / sig_op_count 等只剩历史文档，不再是活跃代码

在此之前，系统都应被准确描述为：

> 当前实现仍残留 legacy 痕迹，但迁移策略已经确定为一次性收口：不再接受新增兼容桥，主路径直接切到 CellTx / ScriptRef
