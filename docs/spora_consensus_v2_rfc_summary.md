# Spora 共识架构 V2 RFC 摘要

- 日期: 2026-04-13
- 文档版本: v2.0
- 状态: 已按当前实现审计更新
- 正文: [spora_consensus_architecture_v2.md](/Users/arthur/RustroverProjects/Spora/docs/spora_consensus_architecture_v2.md)
- 差距清单: [spora_consensus_v2_gap_analysis.md](/Users/arthur/RustroverProjects/Spora/docs/spora_consensus_v2_gap_analysis.md)
- Issue 清单: [spora_consensus_v2_issue_list.md](/Users/arthur/RustroverProjects/Spora/docs/spora_consensus_v2_issue_list.md)

## 1. 这份摘要现在的定位

这份文档不再是 “V2 draft review briefing”。

截至 2026-04-13，更准确的定位是：

- 说明 V2 RFC 的协议核心仍然是什么
- 标明哪些协议判断已经被代码主路径实现
- 把剩余工作收敛到真实还没完成的事项

换句话说，V2 已经不再是“是否要做”的提案，而是“哪些部分已经落地、哪些部分还要继续收尾”的协议摘要。

## 2. V2 现在到底解决了什么

V2 仍然定义 Spora 为：

- `GhostDAG` 负责排序
- `Cell` 负责状态表达
- `CKB-VM` 负责脚本执行
- `accepted_id_merkle_root + cell_root + cell_commitment` 把 accepted 结果与 live Cell 状态绑定到 header

这条协议主线在当前代码中已经进入主路径，不再只是架构口号。

## 3. 仍然有效的三条核心决策

### 决策 1: 状态视角必须是 POV-aware

任何 Cell live / spent / mature / 可花费性判断，都必须相对于：

- `pov block hash`

而不能只相对于：

- `daa score`

这条原则仍然有效，而且当前实现已经按这条原则组织主路径查询与 DAG 校验。

### 决策 2: `selected_parent` 的已提交状态是合法起点

每个块的状态计算都必须从：

- `selected_parent committed state`

开始，而不是从：

- 当前 `virtual_state`
- 某个缓存树
- 某个“接近正确”的 root 恢复结果

这条原则仍然是 V2 的关键约束，当前实现也已经朝这个方向收口。

### 决策 3: 入口应尽量共享状态转移语义

以下入口应尽量共享同一套状态转移语义：

- `body validation in context`
- `reorg replay`
- `mempool admission`
- `block template validation`

这条原则仍然成立，不过它现在更适合作为“后续持续收敛目标”，而不是再描述成“主路径仍完全未接入”。

## 4. 现在已经落地的部分

### 已经落地

- `selected_parent + ordered_mergeset` 主路径
- `cell_root` / `cell_commitment` / `accepted_id_merkle_root` 校验
- `POV-aware` Cell provider
- `get_virtual_cells` / `get_pruning_point_cells`
- block body validation 中的 `CellValidator::validate_full_with_scripts_and_cycles(...)`
- mempool 主路径上的真实输入解析、POV 视角校验、fee 计算与 contextual mass 回填
- template 构造中的 canonical commitment / acceptance / coinbase 主路径
- `txscript` 删除、`ScriptPublicKey` 主路径退役、wallet legacy account / compat / gen0 删除

### 仍然需要继续收尾

- Cell-native 资源模型仍是简化实现，`compute_mass()` 仍是静态 hint
- `CellMeta` / metadata / wrapper 分层仍有继续收敛空间
- `mempool` / `template` 虽然已经不是 placeholder，但仍有内部 helper 和命名可继续收口
- 历史文档与说明材料仍有一部分口径落后于代码现状

## 5. 对旧版摘要里几条判断的修正

下列旧判断已经不再准确：

- “`mempool` 还是简化实现”  
  更准确地说：mempool 已接入真实 POV / Cell 校验主路径，但内部仍有 wrapper 和命名遗留可继续收口。

- “`template validation` 仍部分简化”  
  更准确地说：template 已进入 canonical Cell 校验路径，但 fee / admission / helper 分层仍可继续统一。

- “`verify_scripts` 还没有用真实 consensus-backed data provider”  
  这条已过时。当前 `CellValidator` 已准备 VM data provider，而不是 `SimpleDataProvider::new()` placeholder。

## 6. 现在应该如何理解 V2 完成度

当前更准确的结论不是：

> “V2 还只是方向正确，协议实现尚未闭环。”

而是：

> “V2 的协议骨架和代码主路径已经对齐；剩余工作主要在资源模型精化、内部抽象收缩和历史文档口径对齐，而不是继续打通主路径。”

## 7. 当前最值得继续推进的事项

如果继续沿 V2 方向推进，优先级建议是：

1. 完成 Cell-native mass 模型的进一步精化
2. 收敛 `CellMeta` / metadata / wrapper 的分层边界
3. 继续统一 mempool / template / signing 的内部抽象与命名
4. 对齐历史审计文档、README 和仓库说明材料

## 8. 一句话结论

V2 现在不再是 “要不要做” 的 RFC，而已经是当前代码主路径的协议解释框架。  
后续工作不是再把主路径切成 Cell，而是继续把已经完成的 V2 实现收紧、验证和文档化。
