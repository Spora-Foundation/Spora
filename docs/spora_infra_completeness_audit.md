# Spora 全链基建完备性审计

## 文档定位

这份文档总结的是当前 Spora 从共识、索引、RPC、钱包到模拟器的全链完备度。

核心判断不是“链能不能跑”，而是“链是否已经具备用户可用性和基础设施闭环”。

结论先行：

> **共识、索引、核心 RPC 和钱包关键路径已经从“断路”进入“可用”，当前主要缺口转向 VM syscall 完整语义、legacy 脚本迁移和更大范围集成测试。**

换句话说，链不再停留在“能跑但用户面基础设施断路”的阶段。`CellIndex -> daemon -> RPC`、`get_headers`、`get_cell_return_address`、`resolve_finality_conflict`、钱包通知、`set_name`、gen1 导入和 `watch-only` 创建链路都已经接通；剩余差距主要在“最后一层产品级收口”。

## 一页结论

### 当前状态

- 共识、virtual state、cell state tree、cell commitment 主路径可运行。
- P2P、出块、accepted path、reorg 相关主逻辑已基本收口。
- `CellIndex` 已接入 daemon 与 RPC service，核心查询不再停留在 stub/空 provider。
- `get_coin_supply`、`get_headers`、`get_cell_return_address`、`resolve_finality_conflict` 已接通。
- 钱包通知、`set_name`、gen1 钱包导入、`watch-only` 创建链路已闭环。
- Cell-native `since` 时间锁校验已进入 DAG validator，但脚本层还没有彻底迁到 `Script + CKB-VM`，legacy 脚本 helper 仍是需要继续收口的接口面。

### 最关键结论

1. **不是共识阻塞了可用性，也不再是查询层彻底断路。**
2. **当前更大的风险是外围接口语义和集成覆盖仍未完全收口。**
3. **如果只做一件事，应该优先补齐 `Script + CKB-VM` 迁移和更大范围端到端测试。**

## 已确认的关键问题

### P0: 历史断路项已修复

以下问题在早期确实属于 P0，但截至 2026-04-13 已不再构成当前阻塞：

- `CellIndex` 已接入 daemon 与 RPC service，不再是“接口存在、数据源缺失”的状态。
- `get_coin_supply` 已返回真实值，不再是硬编码 0。
- `get_cell_return_address` 已可解析返回地址，不再直接报 `not implemented`。
- `get_headers` 与 `resolve_finality_conflict` 已接通，不再只是 RPC 空壳。

这些项应该保留在“修复记录”里，而不应继续放在当前红项列表中。

## P1: 当前仍需继续收口的接口面

### P1-1 legacy 时间锁脚本接口仍不应被视为可用能力

共识层的 Cell-native `since` 已经可用，但公开 helper 和旧脚本接口仍可能把用户带回错误语义的 legacy 路径。

影响：

- 钱包、SDK、工具层仍可能生成“构造成功但实际不可花费”的脚本
- 迁移目标应该是 `Script + CKB-VM`，而不是继续包装 legacy helper

### P1-2 VM syscall 语义仍然是子集实现

`LoadHeader` 等主路径已从 placeholder 进入真实执行，但 syscall/runtime coverage 仍非完整集合。

影响：

- VM 已可用，但离“完全生产级脚本面”仍差最后一层语义补齐

### P1-3 钱包与客户端集成覆盖仍可继续扩大

钱包通知、`set_name`、gen1 导入、`watch-only` 创建都已修复，但 SDK/WASM/transport 的跨层联调覆盖仍值得继续扩大。

影响：

- 当前主路径可用，但仍需要更多端到端验证来降低回归风险

### P1-4 共识层 `since` 已实现，但脚本层还没有彻底迁到 `Script + CKB-VM`

审计后需要修正一个判断：

> **当前问题不是“Cell 时间锁只实现了绝对 DAA 锁”，而是“共识层 `since` 已实现，但脚本层还没有彻底迁到 `Script + CKB-VM`，legacy txscript 时间锁接口仍与 Cell 模型冲突并被上层调用”。**

当前事实如下：

- `CellValidator::validate_in_dag` 已调用 `cell_validation_in_dag::validate_time_locks`
- `validate_time_locks` 已覆盖四类 Cell-native 约束：
- 绝对 DAA 锁
- 相对 DAA 锁
- 绝对时间戳锁
- 相对时间戳锁
- 同文件已有对应单测，说明“Cell-native `since` 主路径完全空缺”这一判断已不成立

文件位置：

- `consensus/src/processes/cell_validator/mod.rs`
- `consensus/src/processes/cell_validator/cell_validation_in_dag.rs`

真正的 P1 风险在“共识已经走向 CKB-VM，但外围还保留 txscript helper”这条分裂路径：

- `OpCheckLockTimeVerify`（CLTV）仍比较栈上值和 `tx.lock_time()`，而 `CellTx::lock_time()` 恒返回 0
- 结果是：
- DAA 型 `lock_time > 0` 会因 `stack_lock_time > tx.lock_time()` 必败
- 时间戳型 `lock_time` 会先在 threshold 类型检查处直接失败
- 这使得 HTLC 超时退款分支不可达，资金存在永久锁死风险
- `OpCheckSequenceVerify`（CSV）仍按 legacy `sequence` 解释 `since`
- 对 Cell 相对锁来说，bit63 表示“相对锁”，但 CSV 把它当成“disabled bit”，会直接报错
- 对 Cell 绝对锁来说，它又只比较低 32 位，忽略 bit62 模式位和高位值域，比较语义仍然错误
- `htlc_script`、`htlc_script_ecdsa`、`pay_to_address_with_lock_time_script`、`pay_to_pub_key_with_lock_time` 等 helper 因此都不应继续被视为“可用的 Cell 时间锁构造器”

受影响调用面不止 txscript 内部，还包括：

- `consensus/client/src/utils.rs`
- `wallet/core/src/tx/generator/generator.rs`
- `wallet/core/src/tx/generator/pending.rs`
- `treasure_boy/src/lib.rs`

影响：

- 共识层时间锁内核不是主要短板，短板是公开 API 仍暴露错误的 legacy 时间锁能力
- 钱包、SDK、工具层仍可能生成“构造成功但实际不可花费”的脚本
- 如果文档继续写成“时间锁只做了绝对 DAA”，会误导后续工作，把重点放错到共识内核补实现，而不是上层接口收口
- 如果文档只写“清理旧 opcode”，又会低估终态要求。这里真正需要的是彻底迁到 `Script + CKB-VM`，和主架构文档、`CELL_MODEL_MIGRATION_ROADMAP.md` 保持一致

Cell-native 替代方向已存在：`exec/src/scripts/` 中已有通过 VM 系统调用读取 `since` / header timestamp 的 fixture，可作为规范迁移路径的起点。终态应当是 wallet / client / SDK 都围绕 `Script + CKB-VM` 暴露能力，而不是继续包装 txscript 时间锁 helper。

详见共识 V2 清单：`V2-P1-04`。

## P2: 需要修正表述的事项

### P2-1 simpa 不是“完全不可用”

`simpa` 的交易构造路径确实仍不完整，`select_transactions` / `build_txs` 仍有 stub 痕迹。

但更准确的表述是：

> **simpa 的交易模拟路径未完成，不应表述为整个模拟器完全不可用。**

原因是 pruning 相关模拟路径已经能够跑通，说明它不是整体瘫痪，而是功能不完整。

### P2-2 VM 问题不应再描述为“空 provider”

较早阶段里，脚本验证曾经存在明显的 placeholder provider 问题。

但当前更准确的描述是：

- VM 校验仍然受 feature gate 控制
- 端到端接线仍未完全统一
- 但已经不是单纯的 `SimpleDataProvider::new()` 空实现问题

因此这条应继续保留为“脚本系统尚未全面完成”，而不是“仍在空跑”。

### P2-3 storage mass 应降级表述

RPC/service 一侧确实仍有 lower-bound / workaround 风格逻辑。

但这更多反映的是：

- 兼容层尚未完全 Cell-native
- RPC 计算口径仍不精确

它不应盖过更直接的阻塞项，例如：

- legacy 脚本迁移未完成
- legacy 时间锁脚本接口仍错误暴露
- VM syscall 语义仍是子集

## 组件完备度评级

| 组件 | 状态 | 评级 |
| --- | --- | --- |
| 共识引擎 | 主路径已闭环 | 90% |
| Cell 状态管理 | 主路径完整 | 90% |
| Mempool / 出块 | 已基本可运行 | 85% |
| P2P | 主路径完整 | 85% |
| RPC 方法定义 | 核心查询已接通，仍有部分语义待继续收口 | 78% |
| CellIndex | 已接入 daemon / RPC，主路径可用 | 82% |
| 钱包 SDK | 主体可用，关键缺口已修复 | 82% |
| CLI / 浏览器支持 | 基础能力可用，仍受集成覆盖限制 | 72% |
| simpa | 可用于部分模拟，交易路径未完成 | 35% |
| 集成测试 | 已明显改善，仍需继续扩展 | 72% |

## 最短修复路径

### 第一优先级

1. 推进 `Script + CKB-VM` 迁移，继续切掉 legacy 时间锁脚本入口。
2. 扩展 VM syscall/runtime 语义覆盖，补齐当前子集实现之外的能力。
3. 扩大 RPC / 钱包 / WASM / transport 的端到端测试覆盖。

### 第二优先级

1. 继续收紧 RPC 质量口径，例如 accepted tx mass 与 conversion API 的上下文一致性。
2. 扩展钱包账户变体和浏览器侧联调覆盖。
3. 清理仓库里仍保留的历史审计/迁移旧口径。

### 第三优先级

1. 继续清理遗留默认 stub / placeholder。
2. 把 storage mass 与更多 RPC 转换口径进一步 Cell-native 化。
3. 补完 simpa 的交易选择和构造路径。

## 建议的落地任务

### 任务 A: 扩展 CellIndex / RPC 集成覆盖

目标：

- 固化 `CellIndex -> daemon -> RPC` 已落地的真实链路
- 继续补齐有上下文要求的 RPC conversion / mass 口径
- 用集成测试锁住已修复查询路径

完成标准：

- `get_headers`、`get_cell_return_address`、`get_coin_supply`、`resolve_finality_conflict` 保持真实可用
- `get_block` / `get_transaction` 的 accepted tx mass 不再回退到无上下文 projection
- 对应 sanity / integration 测试覆盖固定下来

### 任务 B: 完成时间锁脚本面向 `Script + CKB-VM` 的迁移

目标：

- 保持现有 Cell-native `since` 校验路径稳定
- 迁移并清理已失效的 txscript 时间锁 opcode / helper 路径（CLTV / CSV）
- 把 wallet / SDK / 工具层从 `lock_time` helper 切到 `Script + CKB-VM` 方案
- 让文档、公开 API、示例脚本都和 CKB 风格脚本面保持一致
- 最终删除 `spora-txscript` 作为生产依赖

完成标准：

- `validate_time_locks` 继续覆盖绝对/相对 DAA 与绝对/相对时间戳四类语义
- CLTV / CSV 在 CellTx 上改为明确报错或删除，不再静默暴露错误语义
- `pay_to_address_with_lock_time_script`、`pay_to_pub_key_with_lock_time`、`htlc_script`、`htlc_script_ecdsa` 不再作为可用 Cell helper 对外暴露
- wallet / SDK / `treasure_boy` 不再调用 legacy 时间锁构造器
- 对应示例、测试和迁移说明同步更新
- 至少有一条面向上层使用者的标准路径明确落在 `Script + CKB-VM`，而不是继续依赖 txscript builder
- `wallet/core`、`wallet/psst`、`consensus/client`、`mining`、`treasure_boy` 不再把 `spora-txscript` 当作生产依赖
- `spora-txscript` crate 从 workspace 与源码树删除，不再参与生产或测试构建

当前状态（2026-04-12）：

- `spora-txscript` 已从 workspace、`Cargo.lock` 与源码树删除
- `wallet/core`、`wallet/psst`、`consensus/client`、`mining`、`treasure_boy` 已切掉对 `spora-txscript` 的直接生产依赖
- `wallet/psst` 已移除 `TxScriptEngine`
- `mining` / `consensus` 已移除 txscript cache counters 依赖
- `treasure_boy` 已删除 legacy TLC/HTLC CLI、库函数和示例，不再保留“公开但必然失败”的时间锁入口
- 剩余工作主要是继续清理仓库级公开示例、注释和迁移文档，使“Script + CKB-VM”成为唯一推荐路径

### 任务 C: 钱包 SDK 打磨

目标：

- 保持已修复的钱包关键路径稳定
- 扩大 transport / WASM / 浏览器侧联调覆盖

完成标准：

- 通知注册 / 取消注册 / server-client 转发可用
- `set_name` 可用并持久化
- gen1 钱包导入可用
- `watch-only` 创建、descriptor 与地址导出链路保持可用

## 最终结论

当前 Spora 的状态已经不再是“查询层断路、钱包关键接口大量 panic”的阶段。

更准确的判断是：

> **共识主路径、索引接线、核心 RPC 和钱包关键路径已经闭环，项目现在处在“从可用走向生产级”的最后收口阶段。**

因此，下一阶段最值得投入的工作不是继续给共识主路径打补丁，而是：

1. 把脚本面对齐到 `Script + CKB-VM`
2. 补齐 VM syscall/runtime 的剩余语义
3. 用更大范围端到端测试把已修复能力彻底锁住

当这三项完成后，Spora 才能从“已经可用”进一步进入“生产级交付”的阶段。
