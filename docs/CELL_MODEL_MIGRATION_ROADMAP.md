# Spora Cell 模型迁移路线图

- 日期: 2026-04-13
- 文档版本: v2.0
- 状态: 已审计更新
- 定位: 迁移完成后的审计记录与后续收尾路线
- 关联文档:
  - [KASPA_LEGACY_REMOVAL_PLAN.md](/Users/arthur/RustroverProjects/Spora/docs/KASPA_LEGACY_REMOVAL_PLAN.md)
  - [spora_consensus_architecture_v2.md](/Users/arthur/RustroverProjects/Spora/docs/spora_consensus_architecture_v2.md)
  - [spora_cell_architecture_review.md](/Users/arthur/RustroverProjects/Spora/docs/spora_cell_architecture_review.md)

## 1. 审计结论

这份文档原本把仓库描述为：

> “Cell 共识内核 + legacy Transaction 适配外壳”

这个判断已经不再准确。

截至 2026-04-13，Spora 的代码主路径已经完成以下迁移：

- 共识、RPC、钱包、P2P、index、mining、notify 主路径都已切到 Cell-native / Script-native / Address-native
- `ScriptPublicKey`、`SubnetworkId`、`get_subnetwork`、legacy bridge helper 已从主路径删除
- `wallet` legacy account / compat / gen0 派生子系统已删除
- `testing/integration` 的 feature 名也已从 `legacy-*` 改为中性命名
- 在 `testing/`、`consensus/`、`protocol/`、`rpc/`、`wallet/`、`mining/` 这些核心代码范围内，`legacy` / `Legacy` 文本命中已清零

因此，这份文档不再描述“主路径怎么迁移”，而是记录：

- 哪些迁移已经完成
- 哪些旧判断已经失效
- 还剩哪些收尾项值得另立任务处理

## 2. 与旧版本相比，哪些判断已失效

下列旧判断已经被代码现状否定：

- “`Transaction` 仍是大面积兼容桥（client/wallet 层仍在使用）”
- “`ScriptPublicKey` 仍深度渗透在 RPC、钱包、地址、脚本、签名路径”
- “`get_virtual_cells` / `get_pruning_point_cells` 仍是空壳”
- “txscript 仍是外围重要承重件”
- “迁移还应继续按 Phase 1~6 打通主路径”

这些表述在当前仓库中都不成立。

## 3. 已完成迁移的主范围

### 3.1 Consensus / Consensus Core

- `consensus/core` 的 legacy tx bridge 已删除
- placeholder `ScriptPublicKey` 编解码桥已删除
- `sign`、`sighash`、`mass`、`standard_script` 主路径已改为 canonical Cell / `Script`
- coinbase / reward / miner data 已切到 canonical lock script
- `get_virtual_cells` / `get_pruning_point_cells` 已实现

### 3.2 Consensus Client / WASM

- `consensus/client` 的 `output` / `transaction` / `serializable` / `sign` / `utils` 已切到 Cell-native
- WASM / JS 接口已使用 `lockScript`、`payToAddressLockScript` 等 canonical 命名
- 不再从 client output / cell 路径回退构造 `ScriptPublicKey`

### 3.3 Wallet

- `wallet/psst` 已改为纯 Cell 输入输出模型
- `wallet/core` 的 generator / payment / mass / address query 主链已切到 `CellOutput` / `Script`
- legacy account、compat、gen0 派生子系统已删除
- CLI 不再暴露 legacy account / legacy-data / default_with_legacy_accounts 等入口

### 3.4 RPC / gRPC / wRPC

- `RpcTransactionOutput` / `RpcCellEntry` 已切为 canonical Cell schema
- `get_subnetwork` runtime API、proto、router、client、server 链路已删除
- `rpc/core`、`rpc/grpc/core`、`rpc/service` 主路径不再依赖 `ScriptPublicKey` / `RpcSubnetworkId`

### 3.5 Protocol / Index / Mining / Notify

- `protocol/p2p` wire schema 已切到 canonical Cell 结构
- `indexes/core` / `rpc/service` 地址索引主链已按 `Address` / lock hash 工作
- `mining` 的 mempool / manager / owner grouping 已切到 Cell-native / Address-native
- `notify` 地址跟踪已切到按 `Address` 工作

### 3.6 Tests / Benches / Examples

- `spora-consensus-core` lib tests 通过
- `spora-mining` lib tests 通过
- `consensus/core` benches 与 `consensus` benches 已切到 Cell-native
- integration / wasm / node 示例中的主路径 legacy 构造已清理
- `testing/integration` feature 名已改为 `integration-tests` / `cellindex-tests`

## 4. 已完成删除的对象

以下对象已经不再属于待迁移项：

- `cell_meta_from_legacy_output()`
- `legacy_sequence_to_cell_since()`
- `cell_out_from_legacy_script_public_key()`
- `cell_entry_legacy_script_public_key()`
- `compute_lock_hash_for_script()`
- placeholder `ScriptPublicKey` 元数据桥
- `get_subnetwork`
- `SubnetworkId` / `RpcSubnetworkId` 主路径依赖
- wallet legacy account
- wallet compat
- wallet gen0 derivation
- deprecated wrapper API 与若干 deprecated alias

## 5. 当前真正剩余的工作

这些工作已经不属于“Cell 主路径迁移未完成”，而属于收尾或独立重构：

### 5.1 文档与说明材料

这一类问题现在主要是“历史文档口径落后于代码现状”，而不是实现层仍有兼容桥。

当前已确认仍需继续对齐的材料包括：

- `spora.md`
- 部分 README
- 历史审计文档与设计评审文档
- `docs/CONSENSUS_SECURITY_AUDIT_2026.md`
- `docs/V2-P1-04-phase2-summary.md`

这些材料中的典型过时表述包括：

- 把仓库描述为“Cell 内核 + legacy 外壳”
- 声称 `ScriptPublicKey`、`txscript`、`get_subnetwork`、wallet legacy account 仍在主路径活跃
- 把已删除的 bridge helper 继续列为“待迁移项”
- 把 `testing/integration` 的旧 feature 名继续写成 `legacy-*`

后续处理原则：

- 文档可以保留历史背景，但必须明确“这是过去状态”
- 不再把历史分析文档里的旧判断当成当前架构事实
- 若文档仍需保留旧术语，应显式标注其语境是“历史/审计背景”

### 5.2 非本路线图范围的内部结构收缩

以下对象仍在代码中存在，但它们已经不属于“legacy 兼容桥未删除”，而是更偏向内部抽象收敛或命名治理：

- `EmbeddedCellMetadata`
- `MutableTransaction` / `SignableTransaction` / `TransactionOutpoint` 等内部抽象
- 仅为历史兼容说明而存在的注释文本

更具体地说：

- `EmbeddedCellMetadata`
  - 仍存在于 `consensus/core` 与 `consensus/client` 的少量内部元数据流中
  - 当前用途是“紧凑型嵌入元数据视图”，用于在不携带完整脚本对象时表达 lock/type/data 哈希与长度
  - 是否继续收缩，应单独评估它在 client / serialization / mempool 视图中的必要性

- `MutableTransaction` / `SignableTransaction` / `TransactionOutpoint`
  - 这些类型仍是仓库中广泛使用的内部 wrapper
  - 它们已经不再承载 legacy `Transaction / ScriptPublicKey` 语义
  - 但它们和 `CellTx` / `CellInput` / exec `OutPoint` 的边界仍可继续压缩
  - 如果继续推进，建议以“transaction wrapper simplification”单独立项，而不是挂在 legacy migration 名下

- 历史说明性注释
  - 目前仍零散存在于 README、审计文档、设计评审、少量算法说明和外部工具说明中
  - 这些文本应继续收口到“historical / previous / reserved / compatibility note”等中性表述
  - 但不应为了注释清理而重新引入任何兼容实现

### 5.3 Integration 验证状态

`cargo check -p spora-testing-integration --tests` 已通过。

这意味着当前主路径迁移不仅在核心 crate 上完成编译验证，也已经越过 integration crate 的编译层面。

## 6. 更新后的 Definition of Done

对这条迁移线，当前可以采用的完成口径是：

- `CellTx` 已成为代码主路径中的规范交易对象
- `Script` 已成为代码主路径中的规范脚本对象
- RPC / wallet / index / mining / notify 主路径不再依赖 legacy `Transaction` / `ScriptPublicKey`
- legacy bridge / adapter / compatibility layer 已删除
- 剩余任务属于文档清理、内部抽象整合和独立重构，而不是主路径迁移

## 7. 后续建议

如果要继续推进，不建议再沿用“Phase 1~6 主路径迁移”这套叙述。更合适的是拆成独立的后续工作：

1. 文档与 README 历史表述对齐
2. `EmbeddedCellMetadata` 与内部 transaction wrapper 的独立重构
3. 继续补齐 integration crate 的运行时测试与仓库级文档收尾

## 8. 最终口径

这份路线图现在应被理解为：

- 主路径迁移已经完成
- 旧版 roadmap 中大量“尚未完成”的判断已经过期
- 后续不再接受重新引入 legacy bridge / adapter / compatibility layer
- 如果还有后续工作，应以“收尾重构”而不是“主路径迁移”名义单独立项
