# Spora VM 完备性审计

## 文档定位

这份文档总结的是当前 `Spora` 虚拟机实现的真实完备度。

重点不是判断“目录里有没有 VM 代码”，而是判断以下问题：

- 是否已经存在真实可执行的 VM 主路径
- 是否已经进入共识校验主流程
- syscall 是否只是定义了接口，还是已经具备可用语义
- cycles、脚本大小、内存等资源限制是否真正落地
- 测试是否覆盖了真实 ELF 执行，而不只是单元级空壳验证

结论先行：

> **Spora VM 已经具备真实执行和真实集成闭环，但还不能称为资源约束与 syscall 语义都已生产级收口。**

换句话说，当前状态不是“VM 还只是骨架”，而是“执行主链路已经打通，限制和兼容性细节仍有缺口”。

## 一页结论

### 当前状态

- `run_script()` 已使用真实 `ckb-vm` 执行 ELF，而不是 placeholder。
- `TransactionScriptVerifier` 已完成脚本分组、syscall 装配、并行验证和 cycles 汇总。
- `CellValidator` 已接入 VM 校验路径，虚拟处理器和区块正文验证也会调用该路径。
- 区块正文验证会累计所有交易的 `verified_cycles` 并执行整块上限检查。
- `spora-exec` 的 VM 相关测试不是空架子，当前本地 `cargo test -p spora-exec --features vm` 结果为 `113 passed, 0 failed`，另有 `4` 个 doctest 通过。
- 当前最主要的缺口不在“能不能跑脚本”，而在“资源限制是否真实 enforce”以及“syscall 是否完整符合 CKB 风格语义”。

### 最关键结论

1. **真实 VM 执行已经落地。**
2. **共识主路径已经接入 VM。**
3. **测试强度高于表面印象，已覆盖多个真实 ELF fixture。**
4. **真正的高优先级问题是资源限制落地不完整，而不是执行框架缺失。**

## 本次复核证据

以下结论来自代码复核与本地测试，不是仅根据文件名推断。

### 真实执行路径

- `exec/src/vm/machine.rs`
- `run_script()` 会：
- 创建 `ckb-vm` machine
- 注册 syscalls
- 加载 ELF program
- 执行到退出
- 返回 machine cycles

这说明当前 VM 已经具备真实执行后端。

### 脚本验证器

- `exec/src/vm/verifier.rs`
- 已实现：
- lock/type script grouping
- resolved input cell 驱动的 lock script 分组
- Rayon 并行校验
- group 级 cycles 汇总
- syscall 运行时装配

### 共识层接入

- `consensus/src/processes/cell_validator/mod.rs`
- `verify_scripts_with_cycles()` 会准备 VM data provider，并调用 `TransactionScriptVerifier`

- `consensus/src/pipeline/virtual_processor/cell_processing.rs`
- 非 coinbase 交易会进入 VM 脚本校验

- `consensus/src/pipeline/body_processor/body_validation_in_context.rs`
- 区块正文验证会收集每笔交易的 `verified_cycles`
- 会在 block 级别累计并检查 `max_block_cycles`

### 本地测试结果

执行命令：

```bash
cargo test -p spora-exec --features vm
```

结果：

- `91` 个测试通过
- `0` 个测试失败
- `4` 个 doctest 通过

已确认覆盖的真实 ELF / VM 执行测试包括：

- `always_success`
- `load_input_since`
- `load_header_timestamp`
- `load_dep_cell_data`
- `timelock_absolute`
- `timelock_relative`
- `htlc`
- `htlc_minimal`

因此，“测试主要还是框架，端到端不足”的表述需要修正为：

> **单 crate 范围内的 VM 执行覆盖已经较强，但跨共识层、mempool、区块模板和 reorg 路径的联动回归仍有限。**

## 需要修正的判断

### 修正 1: VM 不是 skeleton-only

较早阶段可以把 VM 描述为“框架已搭好，真实执行待接入”。

但当前代码事实已经不是这样：

- 真实 `ckb-vm` 已接入
- 实际 ELF 已执行
- 共识验证路径已调用

因此不能再把当前实现描述成“框架 95%，执行待落地”。

更准确的说法是：

> **执行闭环已完成，完备性缺口主要集中在资源约束和 syscall 语义完整性。**

### 修正 2: 测试不是“只有框架”

当前测试已经包含多个真实脚本和正反用例，不应再写成“多数还只是框架”。

更准确的说法是：

> **VM 执行级测试已经成型，但系统级回归覆盖仍需继续向 consensus / mempool / template 侧扩展。**

### 修正 3: block cycles 有累计校验

不能写成“只有单笔交易 cycles 限制，没有 block 级预算”。

当前事实是：

- `CellValidator` 返回单笔交易 `verified_cycles`
- block 正文验证会累计所有交易 cycles
- 超过上限会直接拒绝区块

真正的问题曾经不是“block 预算不存在”，而是 tx-level cycles cap 没有独立 enforce。

该问题现已修复：`CellValidator` 会在 block-level 累计预算之外，额外对单笔交易脚本 cycles 执行独立上限检查。

## 已确认的关键问题

### 已修复: `CurrentCycles` syscall 已返回真实 cycles

文件位置：

- `exec/src/vm/syscalls/current_cycles.rs`

当前状态：

- syscall 编号 `2042` 已注册并接到 `machine.cycles()`
- 已有 syscall 级回归测试覆盖

这项问题在当前分支已不再构成缺口。

### 已修复: `Debugger` syscall 不再破坏 guest memory

文件位置：

- `exec/src/vm/syscalls/debugger.rs`

当前状态：

- 运行时现在通过 `load_bytes()` 读取 guest message
- 不再误用 `store_bytes()` 把零值写回 VM 内存
- 已有 syscall 级回归测试覆盖“读取消息但不篡改内存”

这项问题原本属于真实行为缺陷，而不是代码风格问题；当前分支已修复。

### 已修复: `LoadInput` 的 field 错误码语义已对齐

文件位置：

- `exec/src/vm/syscalls/load_input.rs`

当前状态：

- `LOAD_INPUT_BY_FIELD` 现在使用共享的 `Source` / `InputField` 解析
- 未知 field 会返回 `ITEM_MISSING`
- 不再把 field 不存在误报成 `INDEX_OUT_OF_BOUND`

这项问题不影响现有 fixture 通过率，但会影响 syscall 语义一致性；当前分支已修复。

### 已修复: `LoadCell` 已对齐到共享枚举解析

文件位置：

- `exec/src/vm/syscalls/load_cell.rs`

当前状态：

- `LoadCell` 不再维护本地 `Source` / `CellField` 副本
- 运行时改为直接使用共享枚举解析 source 和 field
- 已补未知 field 返回 `ITEM_MISSING` 的 syscall 级回归测试

这项改动主要用于降低未来编号漂移和 syscall 语义分叉的风险；当前分支已修复。

### 已修复: `LoadCellData` / `LoadWitness` 已对齐到共享 `Source` 解析

文件位置：

- `exec/src/vm/syscalls/load_cell_data.rs`
- `exec/src/vm/syscalls/load_witness.rs`

当前状态：

- 两个 syscall 都不再使用裸 source 常量分支
- 运行时统一改为使用共享 `Source` 枚举解析
- 已补非法 source 返回 `INDEX_OUT_OF_BOUND` 的 syscall 级回归测试

这项改动继续降低了 syscall 之间的语义漂移风险；当前分支已修复。

### 已修复: tx/script/memory 约束已进入实际运行路径

文件位置：

- `exec/src/vm/mod.rs`
- `consensus/src/processes/cell_validator/mod.rs`

当前事实：

- `MAX_TX_CYCLES = 10_000_000`
- `MAX_SCRIPT_SIZE = 1024 * 1024`
- `MAX_VM_MEMORY = 4 * 1024 * 1024`

当前状态：

- tx-level cycles cap 已在 `CellValidator` 中单独 enforce
- script size 已在 `run_script()` 加载 ELF 前主动校验
- VM memory limit 已通过 `new_with_memory()` 进入实际 machine 初始化路径

剩余风险不再是“限制没有落地”，而是：

- 默认值是否最终作为协议/实现常量冻结
- mempool / template 路径是否需要进一步做更激进的预算前置裁剪

### P1: 多个 syscall 仍是“语义子集实现”

文件位置：

- `exec/src/vm/syscalls/load_cell.rs`
- `exec/src/vm/syscalls/load_input.rs`
- `exec/src/vm/syscalls/load_witness.rs`
- `exec/src/vm/syscalls/load_script.rs`
- `exec/src/vm/syscalls/load_header.rs`

当前现状：

- `store_data()` 本身支持 CKB 风格 offset 读取
- `LoadInput`、`LoadWitness`、`LoadScript`、`LoadCell`、`LoadCellData`、`LoadHeader` 已接上 partial read 语义
- 但 syscall 整体仍未覆盖完整 CKB 运行时语义与所有 source/layout 组合

影响：

- 现有 fixtures 可以运行，基础 partial read 已不再是主要缺口
- 但更复杂脚本一旦依赖更完整的 source/field/layout 语义，仍可能暴露兼容差异

更准确的结论不是“syscall 缺失”，而是：

> **syscall 大多已存在且可运行，但若以 CKB 兼容为目标，当前仍只覆盖了一个可用子集。**

### P1: Header/runtime 语义仍偏最小实现

文件位置：

- `exec/src/vm/syscalls/load_header.rs`
- `exec/src/vm/syscalls/load_cell.rs`
- `exec/src/vm/syscalls/load_cell_data.rs`

当前现状：

- `LoadHeader` 已具备 `HeaderDep` source、field 读取和 partial read 行为
- 但 `LoadHeader` 仍只暴露当前 `ResolvedHeader` 提供的最小字段子集
- `LoadCell` / `LoadCellData` 也仍未覆盖完整 CKB source/layout 语义

影响：

- 当前足以支撑现有 fixtures
- 但不足以支撑更复杂的 header-aware scripts

### P2: 调度器文件仍是占位层

文件位置：

- `exec/src/vm/scheduler.rs`

说明：

- 真正的并行执行目前在 `verifier.rs` 中完成
- `scheduler.rs` 仍是未来扩展占位

这不是执行正确性的核心风险，但应避免在文档里把它描述为“完整调度子系统已实现”。

### P2: mempool / template 路径对 block 预算的前置裁剪仍有限

文件位置：

- `consensus/src/pipeline/virtual_processor/processor.rs`

当前现状：

- mempool 和模板校验会拿到单笔交易 `verified_cycles`
- 但本次复核中没有看到像 block 正文验证那样的整块 cycles 预累计裁剪逻辑

影响：

- 不属于共识正确性断点
- 但会影响构块质量、预算提前筛除能力和工程一致性

## 完备度评级

| 维度 | 评级 | 说明 |
|------|------|------|
| VM 执行主路径 | A- | 真实 `ckb-vm` 已接入并执行 ELF |
| Script verifier | A- | 分组、并行、cycles 汇总已具备 |
| 共识接入 | A- | CellValidator 与 block validation 已接通 |
| Syscall 覆盖率 | B | 核心 syscall 存在且可用，但多为语义子集 |
| Header/runtime 完整性 | B | `LoadHeader` 已可用，但仍是最小字段子集 |
| 资源限制落地 | B+ | tx/block cycles、script size、VM memory 已接入运行路径 |
| 测试证据强度 | A- | 多个真实 ELF 回归已通过 |
| 总体评级 | B+ | 已真实可用，但距离生产级完备仍有关键收口项 |

## 建议的整改顺序

### P1

1. 继续补齐 syscall 的剩余 CKB 语义子集，尤其是 source/field/layout 组合。
2. 如果要追求更强兼容性，再扩展 `ResolvedHeader` 与 `LoadHeader` 的字段面。
3. 增加跨 crate 的集成测试，覆盖：
- `CellValidator`
- `virtual_processor`
- `body_processor`
- block/template 路径的 cycles 行为

### P2

1. 统一 mempool / block template / block validation 的 cycles 预算口径。
2. 明确 `scheduler.rs` 的归属，要么实现，要么继续作为占位并在文档中降级表述。

## 最终结论

当前 Spora VM 的真实状态应描述为：

> **执行闭环已完成，测试强度高于表面印象，共识主路径已接入；但资源限制 enforce 和 syscall 语义完整性仍未完全收口。**

因此，当前项目不应再把 VM 视为“待接线 skeleton”，也不应把它直接描述为“生产级 fully complete”。

更准确的工程判断是：

> **这是一个已经进入真实运行阶段的 VM 实现，下一阶段工作的重点应从“把它跑起来”切换为“把约束、兼容性和系统级一致性补齐”。**
