# Spora 共识架构 V2 实现差距清单

- 日期: 2026-04-15
- 状态: Audited & Updated
- 目标文档: [spora_consensus_architecture_v2.md](/Users/arthur/RustroverProjects/Spora/docs/spora_consensus_architecture_v2.md)
- 用途: 把 V2 规范要求映射到当前代码实现，区分“已完成”“部分完成”“待收口”

## 1. 如何读这份文档

这份文档是“当前态差距清单”，不是历史快照。

判断标准：

1. 是否已进入主共识路径
2. 是否已在 body/mempool/template/replay 复用
3. 是否已脱离 placeholder/历史桥接语义

## 2. 总体判断（2026-04-15）

当前实现已经从“V2 迁移中”进入“V2 收尾期”：

1. `GhostDAG + Cell state root + accepted_id_merkle_root + cell_commitment` 主链路已闭环
2. `body/mempool/template` 已接入真实 Cell 校验主路径，不再是早期 placeholder 形态
3. 地址锁现代化已接入 `StdSingle/StdSingleECDSA` canonical tuple，legacy inline 锁治理已收口为终态固定拒绝
4. 剩余差距集中在资源模型精化、内部抽象收口与文档一致性

## 3. 状态摘要

| 能力 | 规范要求 | 当前状态 |
|---|---|---|
| GhostDAG 排序 | 必须有 `selected_parent + ordered_mergeset` | 已完成 |
| POV-aware 历史视角 | 必须以 `pov block hash` 为锚点 | 已完成 |
| 共享状态转移 | `body/reorg/mempool/template` 共用语义 | 已完成（仍有内部抽象尾项） |
| Cell 主路径校验 | 缺失输入/双花/capacity/maturity 必须拒绝 | 已完成 |
| 头部承诺校验 | `accepted_id_merkle_root + cell_root + cell_commitment` | 已完成 |
| VM 执行入共识 | 真实 data provider + script verify | 已完成 |
| 地址锁迁移治理 | 终态固定拒绝策略 | 已完成（分阶段参数面已移除） |
| mempool 一致性 | 与正式验块共享校验语义 | 已完成 |
| template 一致性 | 与正式验块共享校验语义 | 已完成 |
| 证明闭环 | model tests / second implementation | 待收口 |

## 4. 已完成项（不再列为 Open）

以下旧条目已完成或过时，不应继续作为“阻塞缺口”：

1. “mempool 仍是简化 placeholder”
2. “template validation 主要停留在简化路径”
3. “verify_scripts 仍使用 `SimpleDataProvider::new()`”
4. “txscript 退役仍是主目标”
5. “地址锁主路径仍是 inline pubkey 默认模型”

## 5. 当前真实 backlog

### 已关闭

#### V2-P0-01 Cell-native mass 模型精化（已关闭）

该项已不再属于 P0 backlog。当前主路径已经以 `effective_compute_mass()`、`selection_mass()` 和真实 `verified_cycles` 为准。  
剩余工作主要是历史文档口径清理与少量辅助命名收尾，而不是 mass 策略主路径整改。

### P0

#### V2-P0-02 metadata/wrapper 分层收敛

`CellMeta` / `CellMetadata` / `MutableTransaction` / `VerifiableTransaction` 等内部抽象仍有收口空间。  
目标是减少“语义重复层”并降低后续维护复杂度。

### P1

#### V2-P1-01 生产级签名锁脚本与 dep 分发闭环

地址与验签规范路径已收口，但生产级 secp256k1/ecdsa 锁脚本 ELF + dep 分发仍需落地。

#### V2-P1-02 地址锁迁移终态策略收口（已完成，保留记录）

该项在 2026-04-15 已完成并不再构成 backlog：

1. `legacy_inline_lock_phase_b_daa_score` / `legacy_inline_lock_phase_c_daa_score` 已从共识参数层移除
2. legacy inline 锁在共识路径固定返回 `LegacyInlineLockDisabled`
3. body / mempool / template / replay 错误语义已统一收口为 `CellValidationFailed`

后续仅保留文档与观测面一致性维护，不再涉及网络阈值治理。

### P2

#### V2-P2-01 历史文档口径对齐

仍有部分历史材料沿用“旧迁移态”叙述，需要继续对齐到当前实现状态。

#### V2-P2-02 Integration 运行时覆盖收尾

编译层已通过，仍需持续扩展运行时回归覆盖（特别是跨模块链路与边界场景）。

## 6. 约定的 issue 状态

| 状态 | 含义 |
|---|---|
| `Open` | 真实存在且尚未完成 |
| `Fixed` | 已在主路径落地 |
| `Stale` | 旧判断因代码演进失效 |

## 7. 一句话结论

V2 的主路径迁移已经完成，当前差距不在“是否接通”，而在“资源模型与长期维护抽象的收尾质量”。
