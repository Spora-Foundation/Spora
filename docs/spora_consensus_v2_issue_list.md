# Spora 共识架构 V2 Issue 清单

- 日期: 2026-04-13
- 文档版本: v2.0
- 状态: 已按当前实现审计更新
- 主文档: [spora_consensus_architecture_v2.md](/Users/arthur/RustroverProjects/Spora/docs/spora_consensus_architecture_v2.md)
- 差距清单: [spora_consensus_v2_gap_analysis.md](/Users/arthur/RustroverProjects/Spora/docs/spora_consensus_v2_gap_analysis.md)
- 评审摘要: [spora_consensus_v2_rfc_summary.md](/Users/arthur/RustroverProjects/Spora/docs/spora_consensus_v2_rfc_summary.md)

## 1. 文档定位

这份文档不再是 2026-04-11 时点上的原始执行清单。

当前它的作用是：

- 把仍然真实存在的 V2 收尾事项列成 backlog
- 明确哪些 issue 已经完成或已经过时
- 避免继续把已删除的 bridge / txscript / placeholder 路径写成“待实施项”

## 2. 已完成或已过时的旧 issue

下列 issue 已不应继续视为 `Open`：

### 已完成

- 旧 `get_virtual_cells` / `get_pruning_point_cells` 空实现问题
- `txscript` 依赖删除与公开时间锁脚本面下线
- `ScriptPublicKey` / `get_subnetwork` / wallet legacy account / compat / gen0 主路径退役
- mempool 主路径中“直接返回全 `Ok`”的 placeholder 校验
- `verify_scripts` 使用 `SimpleDataProvider::new()` placeholder 的旧判断

### 已过时

- “mempool 还没有接入真实 Cell 共识校验”
- “template validation 仍主要停留在简化路径”
- “V2-P1-04 仍以 txscript 退役为主要目标”

这些条目之所以过时，不是因为“不重要”，而是因为它们已经被当前代码主路径完成或跨过。

## 3. 当前 backlog

## P0

### V2-P0-01 完成 Cell-native mass 模型精化

**为什么要做**

当前 `compute_mass()` 仍主要是 pre-VM 静态 hint。资源模型已经可用，但仍偏保守/简化。

**主要代码路径**

- `consensus/src/pipeline/body_processor/body_validation_in_isolation.rs`
- `exec/src/celltx/types.rs`
- `consensus/core/src/mass/mod.rs`

**完成标准**

- block-level mass 定义进一步明确
- `compute/transient/storage` 三路语义与使用边界清晰
- post-VM cycles 是否反馈到 mass 形成明确策略

**不做的风险**

- 实际执行成本与 admission / block mass 判断仍可能存在偏差

### V2-P0-02 收敛 `CellMeta` / metadata / wrapper 分层

**为什么要做**

`exec` 与 `consensus-core` 仍存在并行的 metadata 语义层；`MutableTransaction` / `SignableTransaction` / `CellEntry` 周边 wrapper 也还有继续压缩空间。

**主要代码路径**

- `exec/src/celltx/types.rs`
- `consensus/core/src/cell_diff.rs`
- `consensus/core/src/cell_metadata.rs`
- `consensus/core/src/tx.rs`

**完成标准**

- 哪一层是 canonical metadata 定义更加明确
- wrapper 与 exec 类型的边界进一步清晰
- 不再依赖模糊的“双层 metadata 语义”

**不做的风险**

- 后续继续演进时，类型边界仍容易漂移

## P1

### V2-P1-01 继续统一 mempool / template / signing 的内部抽象

**为什么要做**

mempool 与 template 已经接入真实主路径，但内部仍有一些历史 helper、命名和 wrapper 没完全收紧。

**主要代码路径**

- `consensus/src/pipeline/virtual_processor/processor.rs`
- `consensus/core/src/tx.rs`
- `wallet/core/src/tx/generator/*`
- `consensus/core/src/hashing/sighash.rs`

**完成标准**

- fee / contextual mass / resolved-input 回填路径命名更加一致
- `MutableTransaction` / `VerifiableTransaction` / `SignableTransaction` 的职责边界更清楚
- signing / sighash / mempool / template 不再保留误导性的旧命名或历史桥接语义

**不做的风险**

- 主路径虽然正确，但内部维护成本继续偏高

### V2-P1-02 继续统一 lock-hash / Script / address 表达

**为什么要做**

主路径已经切到 `Script` / `Address` / lock-hash，但仓库内部仍有一些命名、说明和包装类型可继续收口。

**主要代码路径**

- `consensus/client`
- `wallet/core`
- `rpc/core`
- `notify`

**完成标准**

- 对外和对内的脚本/地址命名更加稳定
- 不再出现把历史脚本模型误写成当前正式接口的情况

**不做的风险**

- 文档、SDK 和内部实现之间继续出现语义错位

## P2

### V2-P2-01 对齐历史文档与说明材料

**为什么要做**

当前最大的“gap”之一已经不是实现，而是历史文档仍把仓库描述成“Cell 内核 + legacy 外壳”。

**主要材料**

- `spora.md`
- README
- 历史审计文档
- 设计评审文档

**完成标准**

- 文档明确区分“过去状态”和“当前状态”
- 不再把已删除的 bridge / txscript / ScriptPublicKey 主路径依赖写成待办

### V2-P2-02 Integration 验证收尾

**为什么要做**

`cargo check -p spora-testing-integration --tests` 已通过。

**完成标准**

- 持续补齐 integration 的运行时测试覆盖
- 验证当前主路径迁移与文档口径一致

## 4. Issue 状态约定

从现在起，这份 issue 清单使用三类状态：

| 状态 | 含义 |
|---|---|
| `Open` | 仍然真实存在的 backlog |
| `Fixed` | 已在代码主路径落地 |
| `Stale` | 旧 issue 判断已因代码演进失效 |

## 5. 一句话结论

V2 的 issue backlog 现在已经从“主路径迁移清单”收缩成“资源模型、内部抽象和文档收尾清单”。  
后续不应再把已删除的 legacy bridge / txscript / placeholder 路径重新写回 `Open`。
