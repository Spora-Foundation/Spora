# Spora 全链基建完备性审计

## 文档定位

这份文档总结的是当前 Spora 从共识、索引、RPC、钱包到模拟器的全链完备度。

核心判断不是“链能不能跑”，而是“链是否已经具备用户可用性和基础设施闭环”。

结论先行：

> **共识主路径已经闭环，当前主要断点在 RPC 查询层和索引接线层。**

换句话说，链能出块、能验证、能维护 Cell 状态；但钱包、CLI、浏览器依赖的查询路径还没有完成 Cell 化接线，因此“能跑”和“能用”之间仍有明显落差。

## 一页结论

### 当前状态

- 共识、virtual state、cell state tree、cell commitment 主路径可运行。
- P2P、出块、accepted path、reorg 相关主逻辑已基本收口。
- Cell-native `since` 时间锁校验已进入 DAG validator，但脚本层还没有彻底迁到 `ScriptRef + CKB-VM`，legacy txscript 时间锁入口仍在对外暴露。
- 钱包签名、密钥管理主体框架存在，但部分接口仍有 `todo!()`。
- RPC 查询层仍有多处 stub，直接导致余额、Cell 查询不可用。
- `CellIndex` 实现已经存在，但尚未接入 daemon 与 RPC service。

### 最关键结论

1. **不是共识阻塞了可用性。**
2. **是查询层和索引层还没接上 Cell 模型。**
3. **如果只做一件事，应该优先完成 `CellIndex -> daemon -> RPC` 这条链。**

## 已确认的关键问题

### P0: 查询层断路

#### P0-1 RPC 余额 / Cell 查询仍是 stub

当前 RPC service 中，以下路径仍直接返回空集合或默认值：

- `get_cell_set_by_script_public_key`
- `get_cell_set_by_script_public_keys`
- `get_balance_by_script_public_keys`

文件位置：

- `rpc/service/src/service.rs`

影响：

- 钱包无法查询余额
- 钱包无法发现可花费 Cell
- CLI 和浏览器无法拿到真实账户状态
- 选币和构造交易链路无法从 RPC 层闭环

#### P0-2 CellIndex 尚未接入 daemon

当前 daemon 在构造 `RpcCoreService` 时，传入的仍然是 `None`，并带有 `TODO(cell-model): Replace with CellIndex` 注释。

文件位置：

- `sporad/src/daemon.rs`

影响：

- 即使 `indexes/cellindex` 里已经有实现，RPC 侧也拿不到
- 整个查询层仍停留在“接口存在、数据源缺失”的状态

#### P0-3 Coin supply 仍为硬编码 0

`get_coin_supply` 当前仍返回 `circulating_sompi = 0` 的临时 stub。

文件位置：

- `rpc/service/src/service.rs`

影响：

- 钱包和浏览器中的供应量显示错误
- 生态侧的统计数据不可信

#### P0-4 GetCellReturnAddress 尚未为 Cell 模型实现

当前 RPC 会直接返回：

`GetCellReturnAddress not yet implemented for Cell model`

文件位置：

- `rpc/service/src/service.rs`

影响：

- 依赖旧返回地址语义的工具会直接断路

## P1: 钱包与验证层缺口

### P1-1 钱包通知接口仍有 `todo!()`

以下接口仍会 panic：

- `register_notifications`
- `unregister_notifications`

文件位置：

- `wallet/core/src/wallet/api.rs`
- `wallet/core/src/api/transport.rs`

影响：

- 钱包无法稳定订阅到账、状态更新、交易通知

### P1-2 钱包部分辅助接口仍未补完

当前仍存在会 panic 的路径：

- `PrvKeyDataInfo::set_name`
- `import_gen1_keydata`

文件位置：

- `wallet/core/src/wasm/wallet/keydata.rs`
- `wallet/core/src/wallet/mod.rs`

影响：

- Web/WASM 钱包和历史密钥导入体验不完整

### P1-3 共识层 `since` 已实现，但脚本层还没有彻底迁到 `ScriptRef + CKB-VM`

审计后需要修正一个判断：

> **当前问题不是“Cell 时间锁只实现了绝对 DAA 锁”，而是“共识层 `since` 已实现，但脚本层还没有彻底迁到 `ScriptRef + CKB-VM`，legacy txscript 时间锁接口仍与 Cell 模型冲突并被上层调用”。**

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
- 如果文档只写“清理旧 opcode”，又会低估终态要求。这里真正需要的是彻底迁到 `ScriptRef + CKB-VM`，和主架构文档、`CELL_MODEL_MIGRATION_ROADMAP.md` 保持一致

Cell-native 替代方向已存在：`exec/src/scripts/` 中已有通过 VM 系统调用读取 `since` / header timestamp 的 fixture，可作为规范迁移路径的起点。终态应当是 wallet / client / SDK 都围绕 `ScriptRef + CKB-VM` 暴露能力，而不是继续包装 txscript 时间锁 helper。

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

- 查询层断路
- legacy 时间锁脚本接口仍错误暴露
- 钱包通知接口未实现

## 组件完备度评级

| 组件 | 状态 | 评级 |
| --- | --- | --- |
| 共识引擎 | 主路径已闭环 | 90% |
| Cell 状态管理 | 主路径完整 | 90% |
| Mempool / 出块 | 已基本可运行 | 85% |
| P2P | 主路径完整 | 85% |
| RPC 方法定义 | 接口在，查询数据源未完成 | 60% |
| CellIndex | 代码存在，未接线 | 40% |
| 钱包 SDK | 主体存在，若干接口未补完 | 70% |
| CLI / 浏览器支持 | 受 RPC 查询层阻塞 | 60% |
| simpa | 可用于部分模拟，交易路径未完成 | 35% |
| 集成测试 | 仍有缺口 | 65% |

## 最短修复路径

### 第一优先级

1. 把 `CellIndex` 正式接入 `sporad`。
2. 用 `CellIndex` 替换 RPC 查询 stub。
3. 让余额、Cell 查询、coin supply 先返回真实数据。

### 第二优先级

1. 推进 `ScriptRef + CKB-VM` 迁移，先切掉 legacy 时间锁脚本入口。
2. 清理钱包中的 `todo!()`。
3. 把通知、key rename、旧 key import 收口。

### 第三优先级

1. 清理 RPC trait 默认 `unimplemented!()`。
2. 把 storage mass 口径改成 Cell-native。
3. 补完 simpa 的交易选择和构造路径。

## 建议的落地任务

### 任务 A: CellIndex 接线

目标：

- `sporad` 启动时初始化 `CellIndexer`
- `RpcCoreService` 持有真实 provider
- RPC 查询不再返回 stub

完成标准：

- `get_balance_by_address` 返回真实余额
- `get_cells_by_address` / 对应 Cell 查询返回真实 live cells
- `get_coin_supply` 返回真实值

### 任务 B: 完成时间锁脚本面向 `ScriptRef + CKB-VM` 的迁移

目标：

- 保持现有 Cell-native `since` 校验路径稳定
- 迁移并清理已失效的 txscript 时间锁 opcode / helper 路径（CLTV / CSV）
- 把 wallet / SDK / 工具层从 `lock_time` helper 切到 `ScriptRef + CKB-VM` 方案
- 让文档、公开 API、示例脚本都和 CKB 风格脚本面保持一致

完成标准：

- `validate_time_locks` 继续覆盖绝对/相对 DAA 与绝对/相对时间戳四类语义
- CLTV / CSV 在 CellTx 上改为明确报错或删除，不再静默暴露错误语义
- `pay_to_address_with_lock_time_script`、`pay_to_pub_key_with_lock_time`、`htlc_script`、`htlc_script_ecdsa` 不再作为可用 Cell helper 对外暴露
- wallet / SDK / `treasure_boy` 不再调用 legacy 时间锁构造器
- 对应示例、测试和迁移说明同步更新
- 至少有一条面向上层使用者的标准路径明确落在 `ScriptRef + CKB-VM`，而不是继续依赖 txscript builder

### 任务 C: 钱包 SDK 收口

目标：

- 去掉钱包关键路径中的 `todo!()`
- 保证通知和历史密钥导入不会 panic

完成标准：

- 通知注册 / 取消注册可用
- `set_name` 可用
- `import_gen1_keydata` 不再 panic

## 最终结论

当前 Spora 的状态不是“系统不可用”，也不是“只差小修小补”。

更准确的判断是：

> **共识主路径已经能跑，但用户面基础设施还没有完成 Cell 化闭环。**

因此，下一阶段最值得投入的工作不是继续给共识主路径打补丁，而是：

1. 接上 CellIndex
2. 打通 RPC 查询
3. 清掉钱包和 legacy 时间锁接口中的关键缺口，并把脚本面对齐到 `ScriptRef + CKB-VM`

当这三项完成后，Spora 才能从“协议内核已经成型”进入“整条链真正可用”的阶段。
