# Spora 共识层安全审计报告 2026

> 审计日期：2026-04-12  
> 最后更新：2026-04-12  
> 审计范围：Spora Cell DAG 共识主路径、CellTx 验证链、状态承诺与对外查询桥接  
> 文档状态：已按代码现状修正  
> 用途：问题池与处置记录，不作为"直接删除清单"

---

## 执行摘要

这份文档修正了早期版本里几条已经失真的结论。

当前更准确的判断是：

- Spora 的 Cell / CellTx 概念设计总体成立，问题主要在迁移桥接和部分共识实现未收口。
- “脚本验证完全缺失”“CellValidator 未接入共识”这两条结论已不成立。
- 仍然成立并值得优先处理的，是：
  - Cell mass 仍是简化实现
  - 仍存在 legacy `Transaction / ScriptPublicKey / txscript` 对共识外围的牵引
  - 一些查询/RPC/桥接路径之前存在 stub，目前已部分修复，但还未完全摆脱兼容层

### 当前风险评级

| 级别 | 数量 | 状态 | 说明 |
|---|---|---|---|
| CRITICAL | 0 | ✅ 已清零 | 本轮复核后，没有证据支持"可直接盗花任意 Cell"这类结论 |
| HIGH | 3 | ⚠️ 持续跟踪 | 资源模型、桥接一致性、状态承诺语义 |
| MEDIUM | 4 | 📋 按计划处理 | 实现债、分层问题、兼容桥未收口 |
| LOW | 若干 | 📝 文档/术语 | 注释、术语、测试与文档残留 |

### 关键指标速览

| 指标 | 状态 | 说明 |
|---|---|---|
| Cell验证器 | ✅ 已接入 | `CellValidator` 已接入 block body validation 主路径 |
| 脚本验证 | ✅ 已启用 | 非 coinbase 交易默认执行 CKB-VM 脚本验证 |
| 并行验证 | ✅ 已实现 | 区块内交易并行验证，确定性收敛 |
| Cycles限制 | ✅ 已启用 | 单笔/区块级 cycles 限制已生效 |
| Mass模型 | ⚠️ 简化版 | compute/transient/storage 三维模型已定义，但 compute 仍为静态 hint |

---

## 一、已证伪或已过时的结论

### 1. “脚本验证完全缺失”

该结论已经过时。

当前代码中：

- [body_validation_in_context.rs](/Users/arthur/RustroverProjects/Spora/consensus/src/pipeline/body_processor/body_validation_in_context.rs) 会对每笔非 coinbase `CellTx` 调用 `CellValidator::validate_full_with_scripts_and_cycles(...)`
- [cell_validator/mod.rs](/Users/arthur/RustroverProjects/Spora/consensus/src/processes/cell_validator/mod.rs) 已包含：
  - `validate_in_isolation`
  - `validate_in_context`
  - `validate_in_dag`
  - `verify_scripts`
  - `validate_full_with_scripts_and_cycles`

因此更准确的表述应为：

`脚本验证已接入 block body validation 主路径，但仍受 legacy Transaction / txscript / bridge 牵引，尚未完成 Cell-native 收口。`

### 2. “CellValidator 未被调用”

该结论也已过时。

`CellValidator` 当前确实被 body processor 调用；问题不在“未接入”，而在：

- 它尚未成为所有相关路径的唯一验证骨架
- mass / sighash / wallet / rpc 仍保留部分桥接逻辑

### 3. “红块交易完全未处理，导致共识失效”

这条描述过度了。

当前更准确的情况是：

- 红块不会像蓝块那样进入 selected virtual state
- 红块奖励与 acceptance-data 语义需要谨慎区分
- 之前 `acceptance_data` 存在缺口，现已补齐 mergeset acceptance-data 填充

因此它是：

`状态语义与奖励/acceptance-data 边界问题`

而不是：

`任何红块都会直接破坏 Cell 状态正确性`

---

## 二、已修复项 (Fixed)

以下问题在当前代码中已经处理：

### ✅ 1. CellStateTree 奇数叶子直接提升

**状态**: Fixed  
**已修复文件**:
- [state/src/cell_tree.rs](/Users/arthur/RustroverProjects/Spora/state/src/cell_tree.rs)

当前 odd leaf 会与零右子节点一起哈希，不再直接提升。

### ✅ 2. `output_data` 缺少显式大小上限

**状态**: Fixed  
**已修复文件**:
- [consensus/src/processes/cell_validator/cell_validation_in_isolation.rs](/Users/arthur/RustroverProjects/Spora/consensus/src/processes/cell_validator/cell_validation_in_isolation.rs)
- [consensus/src/processes/cell_validator/mod.rs](/Users/arthur/RustroverProjects/Spora/consensus/src/processes/cell_validator/mod.rs)

当前已引入 `max_cell_data_size` (默认 500KB)，并在 isolation validation 中显式拒绝超限输出数据。

### ✅ 3. 单笔脚本 cycles 与整块脚本 cycles 未被强制执行

**状态**: Fixed  
**已修复文件**:
- [exec/src/vm/verifier.rs](/Users/arthur/RustroverProjects/Spora/exec/src/vm/verifier.rs)
- [consensus/src/processes/cell_validator/mod.rs](/Users/arthur/RustroverProjects/Spora/consensus/src/processes/cell_validator/mod.rs)
- [consensus/src/pipeline/body_processor/body_validation_in_context.rs](/Users/arthur/RustroverProjects/Spora/consensus/src/pipeline/body_processor/body_validation_in_context.rs)

当前：
- VM verifier 返回总 cycles
- 单笔脚本执行受 `max_block_cycles` (70M) 约束
- block body validation 会累计整块脚本 cycles，并在块级做限制
- 交易并行验证，结果按区块原始顺序确定性收敛

### ✅ 4. Virtual diff 组合中的 panic 面

**状态**: Fixed  
**已修复文件**:
- [consensus/src/pipeline/virtual_processor/processor.rs](/Users/arthur/RustroverProjects/Spora/consensus/src/pipeline/virtual_processor/processor.rs)

多处 `with_diff_in_place(...).expect(...)` 已改成显式错误处理，不再把 diff 组合失败直接升级为 `panic!`。

### ✅ 5. Virtual acceptance-data 缺失

**状态**: Fixed  
**已修复文件**:
- [consensus/src/pipeline/virtual_processor/cell_processing.rs](/Users/arthur/RustroverProjects/Spora/consensus/src/pipeline/virtual_processor/cell_processing.rs)

当前会为 mergeset blue / red block 填充 `MergesetBlockAcceptanceData`，不再把 acceptance-data 留空。

### ✅ 6. `get_virtual_cells` / `get_pruning_point_cells` 空实现

**状态**: Fixed  
**已修复文件**:
- [state/src/cell_tree.rs](/Users/arthur/RustroverProjects/Spora/state/src/cell_tree.rs)
- [consensus/src/consensus/mod.rs](/Users/arthur/RustroverProjects/Spora/consensus/src/consensus/mod.rs)

当前：
- `CellStateTree` 已保留原始 `OutPoint` 双向索引
- `get_virtual_cells(...)` 可从当前 virtual tree 分页返回
- `get_pruning_point_cells(...)` 可通过回滚 virtual diff + chain diff 重建目标视图

### ✅ 7. RPC/通知中的真实 stub

**状态**: Fixed  
**已修复文件**:
- [rpc/service/src/converter/consensus.rs](/Users/arthur/RustroverProjects/Spora/rpc/service/src/converter/consensus.rs)
- [rpc/service/src/service.rs](/Users/arthur/RustroverProjects/Spora/rpc/service/src/service.rs)
- [rpc/core/src/convert/notification.rs](/Users/arthur/RustroverProjects/Spora/rpc/core/src/convert/notification.rs)

当前已修复：
- block transaction conversion 不再返回空交易列表
- storage mass 计算不再给 outputs 传空迭代器
- `CellsChanged` 通知不再退化成 `Default::default()`

---

## 三、仍然成立的高优先级问题 (Open)

### HIGH-1. Cell mass 仍是简化实现

**状态**: Open  
**优先级**: HIGH  
**标签**: `resource-model`, `mass`, `cell-native`

**核心位置**:
- [consensus/src/pipeline/body_processor/body_validation_in_isolation.rs](/Users/arthur/RustroverProjects/Spora/consensus/src/pipeline/body_processor/body_validation_in_isolation.rs)
- [exec/src/celltx/types.rs](/Users/arthur/RustroverProjects/Spora/exec/src/celltx/types.rs)

**现状**:
- `CellTx` 已拆分出 `compute_mass()`、`transient_mass()`、`storage_mass()`
- `CellTx::mass()` 现在等于 storage-side commitment，而不再冒充通用总质量
- block body isolation 已使用 `compute/transient/storage` 三路分离的简化模型
- `compute_mass()` 已从"纯 serialized-size"提升为"serialized-size + 静态 execution-surface surcharge"
- 脚本 cycles 已纳入 block-level effective mass 计算

**问题**:
- `compute_mass()` 仍只是 pre-VM 静态 hint，不是完整执行成本
- 资源模型仍偏乐观，可能低估实际执行成本

**建议**:
- [ ] 把 block-level mass 的 canonical 定义明确迁到 Cell-native 语义
- [ ] 不再依赖 legacy `Transaction` 的 compute/storage mass 兼容含义
- [ ] 考虑引入 post-VM 实际 cycles 反馈到 mass 计算

### HIGH-2. 共识与外围仍受 legacy `Transaction / ScriptPublicKey / txscript` 牵引

**状态**: Open  
**优先级**: HIGH  
**标签**: `legacy-bridge`, `migration`, `technical-debt`

**核心位置**:
- [consensus/core/src/tx.rs](/Users/arthur/RustroverProjects/Spora/consensus/core/src/tx.rs)
- [consensus/core/src/sign.rs](/Users/arthur/RustroverProjects/Spora/consensus/core/src/sign.rs)
- [consensus/core/src/hashing/sighash.rs](/Users/arthur/RustroverProjects/Spora/consensus/core/src/hashing/sighash.rs)
- [crypto/txscript/src/lib.rs](/Users/arthur/RustroverProjects/Spora/crypto/txscript/src/lib.rs)

**现状**:
- `CellTx` 已是主对象之一，验证主路径已切换
- 但 `Transaction / TransactionInput / TransactionOutput / ScriptPublicKey` 仍是大面积兼容桥
- `txscript` 仍在驱动外层签名验证与钱包构造路径

**影响**:
- 新旧模型并存时间越长，语义偏差风险越高
- 误把 bridge 当 canonical 的概率变大
- 维护成本增加，开发者容易混淆

**建议**:
- [ ] 继续把 `MutableTransaction / VerifiableTransaction / sign / sighash / mass` 从 legacy wrapper 推向 Cell-native
- [ ] 把 `ScriptPublicKey` 明确降级为兼容桥，不再让它承担 Cell 身份语义
- [ ] 制定明确的 legacy 组件退役路线图

### HIGH-3. `CellMeta` 分层仍有重复定义与边界不清

**状态**: Open  
**优先级**: HIGH  
**标签**: `type-duplication`, `layering`, `refactoring`

**核心位置**:
- [exec/src/celltx/types.rs](/Users/arthur/RustroverProjects/Spora/exec/src/celltx/types.rs)
- [consensus/core/src/cell_diff.rs](/Users/arthur/RustroverProjects/Spora/consensus/core/src/cell_diff.rs)

**现状**:
- `exec` 和 `consensus-core` 各有一份 `CellMeta`/metadata 语义对象
- 两者并非完全同层：一个偏执行模型，一个偏共识 diff/state 模型

**判断**:
这是实债，但不是简单"删掉一份"就能解的债。

**错误做法**:
- 让 `exec -> consensus-core` (依赖方向错误)

**建议**:
- [ ] 如果要统一，应下沉到 shared low-level crate
- [ ] 或进一步收敛到 `exec` 侧 canonical 类型
- [ ] 不要通过错误的依赖反转来"统一"

---

## 四、中优先级问题 (Open)

### MEDIUM-1. sighash 仍处于双模型过渡期

**状态**: Open  
**优先级**: MEDIUM  
**标签**: `sighash`, `dual-model`, `signature`

**核心位置**:
- [consensus/core/src/hashing/sighash.rs](/Users/arthur/RustroverProjects/Spora/consensus/core/src/hashing/sighash.rs)
- [exec/src/celltx/sighash.rs](/Users/arthur/RustroverProjects/Spora/exec/src/celltx/sighash.rs)

**现状**:
- `exec/celltx/sighash.rs` 已有清晰的 `spora-cell/*` 域分离
- `consensus/core` 仍保留 legacy Schnorr sighash 路径
- 已修复"resolved metadata 悄悄改变 legacy sighash 语义"的兼容风险

**建议**:
- [ ] 明确 legacy bridge 的签名语义
- [ ] 明确 canonical CellTx 的签名语义
- [ ] 最终把共识与钱包主路径收敛到 CellTx/native sighash

### MEDIUM-2. `CellProcessingContext` 不应承担 validator 职责

**状态**: Open → 重新评估  
**优先级**: MEDIUM  
**标签**: `architecture`, `separation-of-concerns`

**说明**:
早期文档把"缺少 `cell_validator` 字段"视为缺陷，这个判断不准确。

**更合理的理解**:
- `body_processor` 负责验证
- `virtual_processor/cell_processing` 负责状态应用

**建议**:
- [x] 保持阶段边界 (已落实)
- [ ] 不要把 validator 硬塞进 state-application context

### MEDIUM-3. `ScriptRef.hash()` 可扩展性一般

**状态**: Open  
**优先级**: MEDIUM  
**标签**: `extensibility`, `hash`, `future-proofing`

**核心位置**:
- [exec/src/celltx/types.rs](/Users/arthur/RustroverProjects/Spora/exec/src/celltx/types.rs)

**现状**:
- 当前哈希输入为 `code_hash + hash_type + args`
- 未显式加"版本前缀"

**判断**:
这是一个将来的可扩展性问题，不是当前最硬的安全缺口。

**优先级低于**:
- mass
- legacy bridge
- output / address / ScriptPublicKey 收口

### MEDIUM-4. `CellOut.verify_capacity()` 的错误类型过于简陋

**状态**: Open  
**优先级**: MEDIUM  
**标签**: `error-handling`, `engineering-quality`

**核心位置**:
- [exec/src/celltx/types.rs](/Users/arthur/RustroverProjects/Spora/exec/src/celltx/types.rs)

**现状**:
- 返回 `Result<(), &'static str>`

**影响**:
不利于调试和上层错误分类，但属于工程质量问题，不是共识级阻塞点。

**建议**:
- [ ] 使用结构化错误类型替代字符串错误

---

## 五、当前结论

### 5.1 核心判断

**过时的结论**:
> "Spora 共识层当前不适合主网上线，因为脚本验证完全缺失。"

**当前更准确的结论**:
> "Spora 的 Cell 共识设计已经有可工作的主路径，但仍处于 canonical Cell 模型与 legacy Transaction 兼容桥并存的阶段。主网上线前，仍需完成 Cell mass、legacy bridge 收口、ScriptPublicKey 退役路径等高优先级工作。"

### 5.2 维度评分

| 维度 | 评分 | 趋势 | 说明 |
|---|---|---|---|
| 概念设计 | A- | → | Cell / DAG 组合总体正确 |
| 类型分层 | B | → | 有 metadata / meta 重复与桥接分裂 |
| 共识接线 | B+ | ↑ | 主路径已接通，脚本验证已启用 |
| 资源模型 | C+ | ↑ | cycles 已接入，mass 仍需完善 |
| 删除准备度 | C | ↑ | legacy 对象正在逐步退役 |

### 5.3 与 CKB 的兼容性状态

| 项目 | 状态 | 说明 |
|---|---|---|
| Cell 结构 | ✅ 对齐 | OutPoint/CellOutput/CellDep/Script 与 CKB 同构 |
| hash_type | ✅ 对齐 | Data=0, Type=1, Data1=2, Data2=4 |
| 系统调用 | ✅ 兼容 | 核心 syscall 编号与 CKB 一致 |
| VM 版本 | ✅ 兼容 | 支持 CKB VM V0/V1/V2 |
| 时间锁 | ⚠️ 适配 | 使用 DAA score 替代 Epoch |

详见: [CKB_TO_SPORA_MAPPING.md](/Users/arthur/RustroverProjects/Spora/docs/CKB_TO_SPORA_MAPPING.md)

---

## 六、推荐的后续顺序

### 阶段一：资源模型完善 (P0)
1. [ ] 完成 Cell-native mass 模型定义
2. [ ] 实现 post-VM cycles 反馈到 mass 计算
3. [ ] 移除 legacy `Transaction` 的 mass 兼容层

### 阶段二：Legacy Bridge 收口 (P1)
4. [ ] 把 `MutableTransaction / VerifiableTransaction` 从 legacy wrapper 收口
5. [ ] 把 `sign / sighash` 收敛到 CellTx/native sighash
6. [ ] 把 `ScriptPublicKey` 明确降级为兼容桥
7. [ ] 推进 `ScriptRef` / lock-hash 地址路径

### 阶段三：清理退役 (P2)
8. [ ] 删除 `subnetwork / gas / payload / lock_time / sig_op_count`
9. [ ] 删除 `txscript` 依赖
10. [ ] 删除 legacy `Transaction` bridge

### 参考文档
- [CELL_MODEL_MIGRATION_ROADMAP.md](/Users/arthur/RustroverProjects/Spora/docs/CELL_MODEL_MIGRATION_ROADMAP.md)
- [CKB_TO_SPORA_MAPPING.md](/Users/arthur/RustroverProjects/Spora/docs/CKB_TO_SPORA_MAPPING.md)

---

## 七、维护约定

### 7.1 问题状态定义

从本文件开始，今后对审计结论使用三种状态：

| 状态 | 含义 | 示例 |
|---|---|---|
| `Open` | 仍成立，尚未修复 | HIGH-1, HIGH-2, HIGH-3 |
| `Fixed` | 已在代码主路径中落地 | 第2节所有问题 |
| `Stale` | 结论已因代码演进失效 | "脚本验证完全缺失" |

**原则**: 避免再把"历史上成立过"的问题继续写成"当前仍是 CRITICAL"。

### 7.2 文档更新流程

1. **代码变更后**: 如果修复了某个问题，更新对应条目状态为 `Fixed`
2. **新发现问题**: 按优先级添加到对应章节，标记为 `Open`
3. **结论过时**: 如果某个结论不再成立，标记为 `Stale` 并说明原因
4. **定期复核**: 每月复核一次，确保文档与代码现状一致

### 7.3 标签系统

| 标签 | 用途 |
|---|---|
| `resource-model` | 资源模型相关问题 |
| `legacy-bridge` | 兼容桥相关问题 |
| `type-duplication` | 类型重复定义问题 |
| `security` | 安全相关 |
| `performance` | 性能相关 |
| `engineering-quality` | 工程质量 |
