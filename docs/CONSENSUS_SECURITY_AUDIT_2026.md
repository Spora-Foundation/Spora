# Spora 共识层安全审计报告 2026

> 审计日期：2026-04-12  
> 审计范围：Spora Cell DAG 共识主路径、CellTx 验证链、状态承诺与对外查询桥接  
> 文档状态：已按代码现状修正  
> 用途：问题池与处置记录，不作为“直接删除清单”

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

| 级别 | 状态 | 说明 |
|---|---|---|
| CRITICAL | 0 | 本轮复核后，没有证据支持“可直接盗花任意 Cell”这类结论 |
| HIGH | 3 | 仍然存在可影响资源模型、桥接一致性、状态承诺语义的高优先级问题 |
| MEDIUM | 5 | 主要是实现债、分层问题、兼容桥未收口 |
| LOW | 若干 | 注释、术语、测试与文档残留 |

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

## 二、已修复项

以下问题在当前代码中已经处理：

### 1. CellStateTree 奇数叶子直接提升

已修复文件：

- [state/src/cell_tree.rs](/Users/arthur/RustroverProjects/Spora/state/src/cell_tree.rs)

当前 odd leaf 会与零右子节点一起哈希，不再直接提升。

### 2. `output_data` 缺少显式大小上限

已修复文件：

- [consensus/src/processes/cell_validator/cell_validation_in_isolation.rs](/Users/arthur/RustroverProjects/Spora/consensus/src/processes/cell_validator/cell_validation_in_isolation.rs)
- [consensus/src/processes/cell_validator/mod.rs](/Users/arthur/RustroverProjects/Spora/consensus/src/processes/cell_validator/mod.rs)

当前已引入 `max_cell_data_size`，并在 isolation validation 中显式拒绝超限输出数据。

### 3. 单笔脚本 cycles 与整块脚本 cycles 未被强制执行

已修复文件：

- [exec/src/vm/verifier.rs](/Users/arthur/RustroverProjects/Spora/exec/src/vm/verifier.rs)
- [consensus/src/processes/cell_validator/mod.rs](/Users/arthur/RustroverProjects/Spora/consensus/src/processes/cell_validator/mod.rs)
- [consensus/src/pipeline/body_processor/body_validation_in_context.rs](/Users/arthur/RustroverProjects/Spora/consensus/src/pipeline/body_processor/body_validation_in_context.rs)

当前：

- VM verifier 可返回总 cycles
- 单笔脚本执行受 `max_block_cycles` 约束
- block body validation 会累计整块脚本 cycles，并在块级做限制

### 4. Virtual diff 组合中的 panic 面

已修复文件：

- [consensus/src/pipeline/virtual_processor/processor.rs](/Users/arthur/RustroverProjects/Spora/consensus/src/pipeline/virtual_processor/processor.rs)

多处 `with_diff_in_place(...).expect(...)` 已改成显式错误处理，不再把 diff 组合失败直接升级为 `panic!`。

### 5. Virtual acceptance-data 缺失

已修复文件：

- [consensus/src/pipeline/virtual_processor/cell_processing.rs](/Users/arthur/RustroverProjects/Spora/consensus/src/pipeline/virtual_processor/cell_processing.rs)

当前会为 mergeset blue / red block 填充 `MergesetBlockAcceptanceData`，不再把 acceptance-data 留空。

### 6. `get_virtual_cells` / `get_pruning_point_cells` 空实现

已修复文件：

- [state/src/cell_tree.rs](/Users/arthur/RustroverProjects/Spora/state/src/cell_tree.rs)
- [consensus/src/consensus/mod.rs](/Users/arthur/RustroverProjects/Spora/consensus/src/consensus/mod.rs)

当前：

- `CellStateTree` 已保留原始 `OutPoint` 双向索引
- `get_virtual_cells(...)` 可从当前 virtual tree 分页返回
- `get_pruning_point_cells(...)` 可通过回滚 virtual diff + chain diff 重建目标视图

### 7. RPC/通知中的真实 stub

已修复文件：

- [rpc/service/src/converter/consensus.rs](/Users/arthur/RustroverProjects/Spora/rpc/service/src/converter/consensus.rs)
- [rpc/service/src/service.rs](/Users/arthur/RustroverProjects/Spora/rpc/service/src/service.rs)
- [rpc/core/src/convert/notification.rs](/Users/arthur/RustroverProjects/Spora/rpc/core/src/convert/notification.rs)

当前已修复：

- block transaction conversion 不再返回空交易列表
- storage mass 计算不再给 outputs 传空迭代器
- `CellsChanged` 通知不再退化成 `Default::default()`

---

## 三、仍然成立的高优先级问题

### HIGH-1. Cell mass 仍是简化实现

核心位置：

- [consensus/src/pipeline/body_processor/body_validation_in_isolation.rs](/Users/arthur/RustroverProjects/Spora/consensus/src/pipeline/body_processor/body_validation_in_isolation.rs)
- [exec/src/celltx/types.rs](/Users/arthur/RustroverProjects/Spora/exec/src/celltx/types.rs)

现状：

- `CellTx` 已拆分出 `compute_mass()`、`transient_mass()`、`storage_mass()`
- `CellTx::mass()` 现在等于 storage-side commitment，而不再冒充通用总质量
- block body isolation 已使用 `compute/transient/storage` 三路分离的简化模型
- `compute_mass()` 已从“纯 serialized-size”提升为“serialized-size + 静态 execution-surface surcharge”
- 尽管已经把脚本 cycles 纳入 block-level effective mass，但这还不是完整的 Cell 三维质量模型

影响：

- 资源模型仍偏乐观
- compute/storage/transient 的职责边界已经初步收口，但 `compute_mass()` 仍只是 pre-VM 静态 hint，而不是完整执行成本

建议：

- 把 block-level mass 的 canonical 定义明确迁到 Cell-native 语义
- 不再依赖 legacy `Transaction` 的 compute/storage mass 兼容含义

### HIGH-2. 共识与外围仍受 legacy `Transaction / ScriptPublicKey / txscript` 牵引

核心位置：

- [consensus/core/src/tx.rs](/Users/arthur/RustroverProjects/Spora/consensus/core/src/tx.rs)
- [consensus/core/src/sign.rs](/Users/arthur/RustroverProjects/Spora/consensus/core/src/sign.rs)
- [consensus/core/src/hashing/sighash.rs](/Users/arthur/RustroverProjects/Spora/consensus/core/src/hashing/sighash.rs)
- [crypto/txscript/src/lib.rs](/Users/arthur/RustroverProjects/Spora/crypto/txscript/src/lib.rs)

现状：

- `CellTx` 已是主对象之一
- 但 `Transaction / TransactionInput / TransactionOutput / ScriptPublicKey` 仍是大面积兼容桥
- `txscript` 仍在驱动外层签名验证与钱包构造路径

影响：

- 新旧模型并存时间越长，语义偏差风险越高
- 误把 bridge 当 canonical 的概率变大

建议：

- 继续把 `MutableTransaction / VerifiableTransaction / sign / sighash / mass` 从 legacy wrapper 推向 Cell-native
- 把 `ScriptPublicKey` 明确降级为兼容桥，不再让它承担 Cell 身份语义

### HIGH-3. `CellMeta` 分层仍有重复定义与边界不清

核心位置：

- [exec/src/celltx/types.rs](/Users/arthur/RustroverProjects/Spora/exec/src/celltx/types.rs)
- [consensus/core/src/cell_diff.rs](/Users/arthur/RustroverProjects/Spora/consensus/core/src/cell_diff.rs)

现状：

- `exec` 和 `consensus-core` 各有一份 `CellMeta`/metadata 语义对象
- 两者并非完全同层：一个偏执行模型，一个偏共识 diff/state 模型

判断：

这是实债，但不是简单“删掉一份”就能解的债。

错误做法：

- 让 `exec -> consensus-core`

因为当前依赖方向恰恰相反。

建议：

- 如果要统一，应下沉到 shared low-level crate，或进一步收敛到 `exec` 侧 canonical 类型
- 不要通过错误的依赖反转来“统一”

---

## 四、中优先级问题

### MEDIUM-1. sighash 仍处于双模型过渡期

核心位置：

- [consensus/core/src/hashing/sighash.rs](/Users/arthur/RustroverProjects/Spora/consensus/core/src/hashing/sighash.rs)
- [exec/src/celltx/sighash.rs](/Users/arthur/RustroverProjects/Spora/exec/src/celltx/sighash.rs)

现状：

- `exec/celltx/sighash.rs` 已有清晰的 `spora-cell/*` 域分离
- `consensus/core` 仍保留 legacy Schnorr sighash 路径
- 已修复“resolved metadata 悄悄改变 legacy sighash 语义”的兼容风险，但两条模型仍并存

建议：

- 继续明确：
  - legacy bridge 的签名语义
  - canonical CellTx 的签名语义
- 最终把共识与钱包主路径收敛到 CellTx/native sighash

### MEDIUM-2. `CellProcessingContext` 不应承担 validator 职责

早期文档把“缺少 `cell_validator` 字段”视为缺陷，这个判断不准确。

更合理的理解是：

- `body_processor` 负责验证
- `virtual_processor/cell_processing` 负责状态应用

建议：

- 保持阶段边界
- 不要把 validator 硬塞进 state-application context

### MEDIUM-3. `ScriptRef.hash()` 可扩展性一般，但不是当前主风险

核心位置：

- [exec/src/celltx/types.rs](/Users/arthur/RustroverProjects/Spora/exec/src/celltx/types.rs)

现状：

- 当前哈希输入为 `code_hash + hash_type + args`
- 未显式加“版本前缀”

判断：

这是一个将来的可扩展性问题，不是当前最硬的安全缺口。

优先级低于：

- mass
- legacy bridge
- output / address / ScriptPublicKey 收口

### MEDIUM-4. `CellOut.verify_capacity()` 的错误类型过于简陋

核心位置：

- [exec/src/celltx/types.rs](/Users/arthur/RustroverProjects/Spora/exec/src/celltx/types.rs)

现状：

- 返回 `Result<(), &'static str>`

这确实不利于调试和上层错误分类，但属于工程质量问题，不是共识级阻塞点。

---

## 五、当前结论

更准确的结论不是：

`Spora 共识层当前不适合主网上线，因为脚本验证完全缺失。`

而是：

`Spora 的 Cell 共识设计已经有可工作的主路径，但仍处于 canonical Cell 模型与 legacy Transaction 兼容桥并存的阶段。主网上线前，仍需完成 Cell mass、legacy bridge 收口、ScriptPublicKey 退役路径等高优先级工作。`

简化评分如下：

| 维度 | 评分 | 说明 |
|---|---|---|
| 概念设计 | A- | Cell / DAG 组合总体正确 |
| 类型分层 | B | 有 metadata / meta 重复与桥接分裂 |
| 共识接线 | C+ | 主路径已接通，但未完全 Cell-native |
| 资源模型 | C | cycles 已接入，mass 仍未完整 |
| 删除准备度 | D | 大量 legacy 对象仍是承重兼容层 |

---

## 六、推荐的后续顺序

1. 完成 Cell-native mass 模型  
2. 继续把 `MutableTransaction / VerifiableTransaction / sign / sighash` 从 legacy wrapper 收口  
3. 把 `ScriptPublicKey` 明确降级为兼容桥，推进 `ScriptRef` / lock-hash 地址路径  
4. 最后再删除 `subnetwork / gas / payload / lock_time / sig_op_count / txscript / legacy Transaction bridge`

对应路线图见：

- [CELL_MODEL_MIGRATION_ROADMAP.md](/Users/arthur/RustroverProjects/Spora/docs/CELL_MODEL_MIGRATION_ROADMAP.md)

---

## 七、维护约定

从本文件开始，今后对审计结论使用三种状态：

- `Open`：仍成立，尚未修复
- `Fixed`：已在代码主路径中落地
- `Stale`：结论已因代码演进失效

避免再把“历史上成立过”的问题继续写成“当前仍是 CRITICAL”。
