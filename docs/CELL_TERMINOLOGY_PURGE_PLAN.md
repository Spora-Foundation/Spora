# legacy txout 术语彻底清除方案（激进版）

> **目标**：代码库中不再存在任何 `legacy_txout` / `legacy txout` / `LegacyTxout` 字样，**包括 proto 字段号、数据库存储键、CLI 参数**，彻底完成 Cell 模型的语义统一。
>
> **原则**：**不保留向后兼容**，所有历史包袱一次性清理。

## 现状概览

| 指标 | 数值 |
|---|---|
| 含 `legacy_txout` 的 `.rs` 文件 | **249 个** |
| `legacy_txout` 总命中次数 | **3915 处** |
| 含 `legacy_txout` 的 `.proto` 文件 | **3 个** |
| 需要重命名的文件（路径含 legacy_txout） | **23 个** |

### 按模块命中分布

| 优先级 | 模块 | 命中 | 文件 | 说明 |
|---|---|---|---|---|
| P0 | `consensus/core` | 155 | 20 | 核心类型定义：CellEntry, Statuslegacy txoutValid |
| P0 | `consensus/src` | 107 | 22 | 共识引擎内部引用 |
| P0 | `consensus/notify` | 20 | 2 | CellsChanged 通知源头 |
| P1 | `notify/src` | 241 | 14 | 通知框架：事件/订阅/广播 |
| P1 | `indexes/` | 110 | 5 | 索引处理器 |
| P1 | `rpc/core` | 255 | 15 | RPC 类型定义 |
| P1 | `rpc/service` | 81 | 3 | RPC 实现 |
| P1 | `rpc/grpc` | 156 | 13 | gRPC 转换层 + proto |
| P1 | `rpc/wrpc` | 55 | 6 | wRPC 客户端 |
| P2 | `wallet/core` | 1367 | 54 | **最大工作量**：CellContext/Processor/Balance |
| P2 | `wallet/psst` | 47 | 7 | PSST 签名中的 cell_entry |
| P2 | `consensus/client` | 295 | 11 | 客户端 SDK 层 |
| P3 | `mining/src` | 67 | 13 | Mempool 旧 cell_set |
| P3 | `crypto/txscript` | 156 | 6 | 脚本引擎 CellEntry 参数 |
| P3 | `protocol/` | 85 | 14 | P2P 协议流 |
| P3 | `testing/` | 235 | 11 | 集成测试 |
| P3 | `treasure_boy` | 96 | 2 | 空投工具 |
| P4 | `cli/src` | 89 | 10 | 命令行界面 |
| P4 | `sporad/src` | 36 | 2 | daemon 入口 |
| P4 | `simpa/src` | 14 | 2 | 模拟器 |
| P4 | 其余 | ~35 | ~8 | database/daemon/utils 等 |

---

## 清除方案（6 个阶段）

### Phase 0：核心类型消灭（共识层基础）

**影响范围**：~280 处，~40 文件
**前置条件**：无
**预计工作量**：2-3 天

#### T0.1 消灭 `CellEntry` 类型

**当前定义**：`consensus/core/src/tx.rs`

```
CellEntry { amount, script_public_key, block_daa_score, is_coinbase }
```

**操作**：
1. 在 `consensus/core/src/tx.rs` 中将 `CellEntry` 重命名为 `CellEntry`（或废弃，改用 `cell_diff::CellMeta`）
2. **不保留 alias**，直接删除 `CellEntry` 类型定义
3. 全局替换所有 `CellEntry` → `CellEntry`
4. 更新所有序列化/反序列化逻辑，旧数据无法读取（预期行为）

**受影响文件**（直接引用）：
- `consensus/core/src/tx.rs` — 定义
- `consensus/core/src/hashing/sighash.rs` — 签名哈希
- `consensus/core/src/mass/mod.rs` — 质量计算
- `consensus/core/src/sign.rs` — 签名
- `consensus/core/src/muhash.rs` — MuHash 兼容
- `consensus/core/src/api/mod.rs` — API trait
- `consensus/client/src/legacy_txout.rs` — 客户端包装
- `crypto/txscript/src/lib.rs` — 脚本引擎
- `crypto/txscript/src/opcodes/mod.rs` — 操作码
- `mining/src/mempool/model/cell_set.rs` — Mempool 集
- `wallet/psst/src/input.rs` — PSST 输入
- `protocol/p2p/src/convert/legacy_txout.rs` — P2P 转换
- `rpc/core/src/model/tx.rs` — RPC 模型

#### T0.2 消灭 `Statuslegacy txoutValid`

**当前定义**：`consensus/core/src/blockstatus.rs`

**操作**：
1. 重命名为 `StatusCellValid`
2. **不保留 alias**，直接替换所有引用
3. 旧数据库中的序列化状态值将无法读取（需要清空数据库重新同步）

**受影响文件**：
- `consensus/core/src/blockstatus.rs` — 定义（6 处）
- `consensus/src/pipeline/virtual_processor/processor.rs` — 2 处
- `consensus/src/pipeline/virtual_processor/cell_processing.rs` — 2 处
- `consensus/src/pipeline/virtual_processor/tests.rs` — 2 处
- `consensus/src/consensus/mod.rs` — 1 处
- `consensus/src/consensus/cell_provider.rs` — 1 处
- `consensus/src/consensus/test_consensus.rs` — 4 处
- `simpa/src/main.rs` — 3 处
- `simpa/src/simulator/miner.rs` — 1 处
- `testing/integration/src/consensus_integration_tests.rs` — ~20 处

#### T0.3 清除 `cell_commitment` header alias

**当前位置**：`consensus/core/src/header.rs`

**操作**：
1. 删除 `#[serde(alias = "cell_commitment")]` 注解
2. 字段名从 `cell_commitment` 改为 `state_commitment`（可选，进一步抽象）
3. **旧数据库完全无法读取**，必须清空重新同步

#### T0.4 清除 `consensus/core/src/muhash.rs` 中的 legacy txout 引用

**操作**：检查是否还有活跃调用。如共识层已完全使用 CellDiff，可将此文件标记 `#[deprecated]` 或删除。

---

### Phase 1：通知管道统一（事件系统）

**影响范围**：~350 处，~35 文件
**前置条件**：Phase 0 完成
**预计工作量**：2-3 天

#### T1.1 消灭 `CellsChanged` 事件

**当前状态**：`CellsChanged` 已存在并行使用，`CellsChanged` 仍在订阅链路中。

**操作**：
1. `notify/src/events.rs` — 删除 `CellsChanged` 枚举值
2. `notify/src/scope.rs` — 删除 `CellsChangedScope`
3. `notify/src/subscription/` — 删除所有 `CellsChanged` 分支（single.rs, array.rs, compounded.rs, context.rs, mod.rs）
4. `notify/src/notification.rs` — 删除 `CellsChanged` 通知变体
5. `notify/src/broadcaster.rs` — 删除相关路由
6. `notify/src/collector.rs` — 删除相关收集逻辑
7. `notify/src/root.rs` — 删除相关注册
8. `consensus/notify/src/notification.rs` — 删除 `CellsChanged` 变体
9. `consensus/notify/src/service.rs` — 删除相关订阅

#### T1.2 消灭 `PruningPointCellSetOverride`

**操作**：重命名为 `PruningPointCellSetOverride`，全局替换。

**受影响文件**：
- `notify/src/scope.rs`
- `notify/src/events.rs`
- `indexes/core/src/notification.rs`
- `indexes/processor/src/processor.rs`
- `indexes/processor/src/service.rs`
- `rpc/core/src/api/notifications.rs`
- `rpc/service/src/service.rs`

#### T1.3 清除 `indexes/core/src/notification.rs` 中的旧类型

**操作**：删除 `CellsChangedNotification`，只保留 `CellsChangedNotification`。

#### T1.4 清除 `indexes/core/src/indexed_legacy_txouts.rs`

**操作**：文件重命名为 `indexed_cells.rs`，内部类型全部替换。

---

### Phase 2：RPC 层重命名

**影响范围**：~550 处，~37 文件
**前置条件**：Phase 1 完成
**预计工作量**：3-4 天

#### T2.1 RPC 核心类型

**文件**：`rpc/core/src/model/`

| 旧名称 | 新名称 |
|---|---|
| `CompactCellEntry` | `CompactCellEntry` |
| `CompactCellCollection` | `CompactCellCollection` |
| `CellSetByScriptPublicKey` | `CellSetByScriptPublicKey` |
| `BalanceByScriptPublicKey` | 保持（语义正确） |
| `RpcCellsByAddressesEntry` | `RpcCellsByAddressesEntry` |

#### T2.2 RPC 方法名

**文件**：`rpc/core/src/api/ops.rs`、`rpc/service/src/service.rs`

| 旧方法 | 新方法 |
|---|---|
| `get_cells_by_addresses` | `get_cells_by_addresses` |
| `get_cell_return_address` | `get_cell_return_address` |
| `notify_cells_changed` | `notify_cells_changed` |
| `stop_notifying_cells_changed` | `stop_notifying_cells_changed` |

#### T2.3 RPC 转换层

**文件**：
- `rpc/core/src/convert/legacy_txout.rs` → 重命名为 `cell.rs`
- `rpc/service/src/converter/index.rs` — 替换内部引用
- `rpc/grpc/core/src/convert/tx.rs` — 替换 proto 映射
- `rpc/grpc/core/src/convert/message.rs` — 替换消息转换
- `rpc/grpc/core/src/convert/notification.rs` — 替换通知转换

#### T2.4 gRPC Proto 定义（彻底替换）

**文件**：`rpc/grpc/core/proto/rpc.proto`、`rpc/grpc/core/proto/messages.proto`

**策略**：**proto 字段号和字段名全部替换**，不保留兼容。

**操作**：
1. 所有 `cellEntry` → `cellEntry`
2. 所有 `cellCommitment` → `cellCommitment`
3. 所有 `cellsChanged` → `cellsChanged`
4. **字段号重新分配**（可选，如果字段顺序不变可保持原号）
5. 生成新的 proto 代码（`cargo build` 自动触发）

**注意**：这会导致新旧节点无法互通。预期行为——全网升级。

#### T2.5 P2P Proto 定义（彻底替换）

**文件**：`protocol/p2p/proto/p2p.proto`

**操作**：
1. `CellEntry cellEntry` → `CellEntry cellEntry`
2. 相关消息名 `*LegacyTxout*` → `*Cell*`
3. **不保留旧字段**，全网节点必须同步升级

#### T2.6 wRPC 客户端

**文件**：
- `rpc/wrpc/wasm/src/client.rs`
- `rpc/wrpc/wasm/src/notify.rs`
- `rpc/wrpc/client/src/client.rs`
- `rpc/wrpc/server/src/server.rs`

---

### Phase 3：钱包层重构（最大工作量）

**影响范围**：~1414 处，~61 文件
**前置条件**：Phase 0、Phase 2 完成
**预计工作量**：5-7 天

#### T3.1 文件重命名

```
wallet/core/src/legacy_txout/           → wallet/core/src/cell/
  balance.rs                    →   balance.rs
  binding.rs                    →   binding.rs
  context.rs                    →   context.rs
  iterator.rs                   →   iterator.rs
  mod.rs                        →   mod.rs
  outgoing.rs                   →   outgoing.rs
  pending.rs                    →   pending.rs
  processor.rs                  →   processor.rs
  reference.rs                  →   reference.rs
  scan.rs                       →   scan.rs
  settings.rs                   →   settings.rs
  stream.rs                     →   stream.rs
  sync.rs                       →   sync.rs
  test.rs                       →   test.rs

wallet/core/src/wasm/legacy_txout/      → wallet/core/src/wasm/cell/
  context.rs                    →   context.rs
  mod.rs                        →   mod.rs
  processor.rs                  →   processor.rs

wallet/core/src/storage/transaction/legacy_txout.rs → cell.rs
```

#### T3.2 核心类型重命名

| 旧名称 | 新名称 | 定义文件 |
|---|---|---|
| `CellContext` | `CellContext` | `wallet/core/src/legacy_txout/context.rs` |
| `CellProcessor` | `CellProcessor` | `wallet/core/src/legacy_txout/processor.rs` |
| `CellBalance` | `CellBalance` | `wallet/core/src/legacy_txout/balance.rs` |
| `CellScan` | `CellScan` | `wallet/core/src/legacy_txout/scan.rs` |
| `CellStream` | `CellStream` | `wallet/core/src/legacy_txout/stream.rs` |
| `CellIterator` | `CellIterator` | `wallet/core/src/legacy_txout/iterator.rs` |
| `CellEntryReference` | `CellEntryReference` | `wallet/core/src/legacy_txout/reference.rs` |
| `OutgoingCell` | `OutgoingCell` | `wallet/core/src/legacy_txout/outgoing.rs` |
| `PendingCellEntry` | `PendingCellEntry` | `wallet/core/src/legacy_txout/pending.rs` |

#### T3.3 字段/变量名替换

在整个 `wallet/` 目录下全局替换：
- `cell_entry` → `cell_entry`
- `cell_entries` → `cell_entries`
- `cell_context` → `cell_context`
- `cell_processor` → `cell_processor`
- `cell_balance` → `cell_balance`
- `cell_scan` → `cell_scan`

#### T3.4 PSST 模块

**文件**：`wallet/psst/src/`

| 旧 | 新 |
|---|---|
| `input.cell_entry` | `input.cell_entry` |
| `Error::MissingCellEntry` | `Error::MissingCellEntry` |
| `Error::NotCompatibleLegacyTxouts` | `Error::NotCompatibleCells` |
| `Error::MultipleUnlockLegacyTxoutError` | `Error::MultipleUnlockCellError` |

#### T3.5 事件名替换

**文件**：`wallet/core/src/events.rs`、`wallet/core/src/wasm/notify.rs`

将所有 `CellsChanged` 相关事件映射替换为 `CellsChanged`。

#### T3.6 consensus/client 包装层

**文件**：`consensus/client/src/legacy_txout.rs` → 重命名为 `cell.rs`

内部 `ClientCellEntry` → `ClientCellEntry`，更新 `consensus/client/src/lib.rs` 中的 mod 声明和 re-export。

---

### Phase 4：执行/挖矿/脚本层

**影响范围**：~380 处，~35 文件
**前置条件**：Phase 0 完成
**预计工作量**：2-3 天

#### T4.1 crypto/txscript

**文件**：`crypto/txscript/src/lib.rs`、`crypto/txscript/src/opcodes/mod.rs` 等

将函数签名中的 `CellEntry` 参数替换为 Phase 0 中定义的新类型。

#### T4.2 mining mempool

**文件重命名**：`mining/src/mempool/model/cell_set.rs` → `cell_set.rs`

**类型替换**：所有 Mempool 中的 CellEntry 引用改为 CellMeta。

#### T4.3 protocol P2P

**文件重命名**：`protocol/p2p/src/convert/legacy_txout.rs` → `cell.rs`

更新 `protocol/p2p/src/convert/mod.rs` 中的 mod 声明。

#### T4.4 treasure_boy

**全局替换**：`treasure_boy/src/lib.rs` 中所有 legacy_txout 引用。

---

### Phase 5：外围清理 + 编译验证

**影响范围**：~200 处，~30 文件
**前置条件**：Phase 0-4 完成
**预计工作量**：2 天

#### T5.1 CLI 参数（彻底替换）

**文件**：`sporad/src/args.rs`、`cli/src/cli.rs`

```
--cellindex  →  --cellindex
```

**操作**：
1. **不保留 `--cellindex` alias**
2. 启动参数解析失败时给出明确错误："`--cellindex` has been removed, use `--cellindex`"
3. 环境变量 `CELLINDEX_*` → `CELLINDEX_*`

#### T5.2 目录/路径名

```
CELLINDEX_CELLS_DB / CELLINDEX_SCRIPTS_DB  ← 已是新名称，无需改
cellindex_db_dir  →  cellindex_db_dir（sporad/src/daemon.rs）
```

#### T5.3 集成测试

**文件**：`testing/integration/src/` 下所有文件

全局替换所有 legacy_txout 引用，更新测试用例名称。

#### T5.4 simpa 模拟器

**文件**：`simpa/src/main.rs`、`simpa/src/simulator/miner.rs`

#### T5.5 数据库前缀（彻底替换）

**文件**：`database/src/registry.rs`

**操作**：
1. 所有 `DatabaseStorePrefixes::LegacyTxout*` → `DatabaseStorePrefixes::Cell*`
2. 存储键字符串从 `"legacy_txout_*"` → `"cell_*"`
3. **旧数据库完全无法识别**，必须清空重新同步

**需要修改的前缀**（示例）：
```rust
// 旧
LegacyTxoutDiffs = b"legacy_txout_diffs",
LegacyTxoutMultisets = b"legacy_txout_multisets",

// 新
CellDiffs = b"cell_diffs",
CellRoots = b"cell_roots",
```

#### T5.6 文件级重命名汇总

以下 23 个文件路径含 `legacy_txout`，需要重命名：

```
consensus/client/src/legacy_txout.rs             → cell.rs
consensus/src/consensus/cell_set_override.rs → cell_set_override.rs
mining/src/mempool/model/cell_set.rs     → cell_set.rs
protocol/p2p/src/convert/legacy_txout.rs         → cell.rs
rpc/core/src/convert/legacy_txout.rs             → cell.rs
indexes/core/src/indexed_legacy_txouts.rs        → indexed_cells.rs
wallet/core/src/storage/transaction/legacy_txout.rs → cell.rs
wallet/core/src/legacy_txout/*.rs (14 files)     → wallet/core/src/cell/*.rs
wallet/core/src/wasm/legacy_txout/*.rs (3 files) → wallet/core/src/wasm/cell/*.rs
```

#### T5.7 最终验证

```bash
# 1. 全量编译
cargo build --all-targets --all-features 2>&1 | head -100

# 2. 全量测试
cargo test --all-targets 2>&1 | tail -20

# 3. 零残留检查
grep -rci 'legacy_txout' --include='*.rs' | grep -v '/target/' | awk -F: '$2>0'
# 期望输出：仅 proto 生成文件 + deprecated alias 注释

# 4. Proto 兼容性检查（确保旧字段号保留）
grep -n 'legacy_txout\|legacy txout' rpc/grpc/core/proto/*.proto protocol/p2p/proto/*.proto
# 期望输出：仅 deprecated 注释和兼容别名
```

---

## 语义陷阱：不能简单重命名的地方

以下位置的 `legacy_txout` 不仅是名字问题，而是**语义/格式/API 契约**的一部分，需要特殊处理：

### Trap 1: Proto 消息名（P2P 协议版本绑定）

**位置**：`protocol/p2p/proto/p2p.proto`

```protobuf
// 第 156-180 行 - 这些是 P2P 协议消息名
message RequestPruningPointCellSetMessage { ... }
message PruningPointCellSetChunkMessage { ... }
message OutpointAndCellEntryPair { ... }
message CellEntry { ... }  // 字段号 2
message RequestNextPruningPointCellSetChunkMessage { ... }
message DonePruningPointCellSetChunksMessage { ... }
```

**为什么不能简单换名**：
- P2P 协议版本 `PROTOCOL_VERSION = 7` 隐含了这些消息的存在
- 消息名变更 = 协议版本必须升级到 8
- 旧节点收到不认识的消息名会直接断开

**正确处理**：
1. 在 `protocol/flows/src/flow_context.rs` 升级 `PROTOCOL_VERSION = 8`
2. 同时重命名所有消息
3. 全网节点必须同步升级（无兼容层）

---

### Trap 2: Serde 序列化键（钱包数据格式）

**位置**：`wallet/core/src/storage/transaction/data.rs`（多处）

```rust
// 第 18-19, 24-25, 30-31, 36-37, 54-56, 71-73, 88-89, 104-105, 119-120 行
#[serde(rename = "cellEntries")]
cell_entries: Vec<LegacyTxoutRecord>,
```

**为什么不能简单换名**：
- 这是用户钱包文件的 JSON 键名
- 直接改名会导致用户无法加载旧钱包
- 但既然不保留兼容，这是**预期行为**

**正确处理**：
1. 改为 `#[serde(rename = "cellEntries")]`
2. 用户必须重新导入私钥/助记词
3. 在 CHANGELOG 中明确声明：**钱包数据不兼容，需重新导入**

---

### Trap 3: RPC Trait 方法名（API 契约）

**位置**：`rpc/core/src/api/rpc.rs`

```rust
// 第 375-383, 388-395, 480-490 行
fn get_cells_by_address(&self, ...) -> ...;
fn get_cells_by_addresses(&self, ...) -> ...;
fn get_cell_return_address(&self, ...) -> ...;
```

**为什么不能简单换名**：
- 这些是 trait 方法签名，客户端通过名称调用
- 但既然不保留兼容，直接改名即可

**正确处理**：
- 直接改为 `get_cells_by_address` 等
- 客户端代码必须同步更新

---

### Trap 4: CLI 参数（用户脚本依赖）

**位置**：`sporad/src/args.rs`

```rust
// 第 55 行
pub cellindex: bool,

// 第 81-85 行（devnet-prealloc 特性）
#[cfg(feature = "devnet-prealloc")]
pub num_prealloc_legacy_txouts: Option<u64>,
```

**为什么不能简单换名**：
- 用户脚本、systemd 服务、docker-compose 都依赖 `--cellindex`
- 直接改名会导致启动失败

**正确处理**：
1. `--cellindex` → `--cellindex`
2. `--num-prealloc-legacy_txouts` → `--num-prealloc-cells`
3. 启动时如果检测到旧参数，给出明确错误：
   ```
   Error: --cellindex has been removed. Use --cellindex instead.
   ```

---

### Trap 5: JavaScript/TypeScript API（wasm 绑定）

**位置**：`rpc/core/src/wasm/message.rs`、`rpc/wrpc/wasm/src/notify.rs`

```typescript
// rpc/core/src/wasm/message.rs 第 244, 474 行
isCellIndexed : boolean;
hasCellIndex : boolean;

// rpc/wrpc/wasm/src/notify.rs 第 23, 26, 57, 60 行
CellsChanged = "legacy_txouts-changed",
PruningPointCellSetOverride = "pruning-point-legacy_txout-set-override",
```

**为什么不能简单换名**：
- 这些会编译成 wasm 的 JS API
- 前端应用直接依赖这些字段名

**正确处理**：
1. 改为 `isCellIndexed`, `hasCellIndex`
2. 改为 `CellsChanged`, `PruningPointCellSetOverride`
3. 前端必须同步更新

**额外文件**（需要检查）：
- `wasm/examples/nodejs/javascript/refactoring/tx-script-sign.js`
- `wasm/examples/nodejs/javascript/refactoring/tx-create.js`
- `wasm/examples/nodejs/javascript/transactions/cell-context-listener.js`
- `wasm/examples/nodejs/javascript/wallet/wallet.js`

---

### Trap 6: 事件类型字符串（运行时匹配）

**位置**：`wallet/core/src/events.rs`、`wallet/core/src/wasm/notify.rs`

```rust
// wallet/core/src/events.rs 第 77, 287, 326, 366, 416 行
CellIndexNotEnabled { ... }

// wallet/core/src/wasm/notify.rs 第 19, 42, 108, 145, 249 行
CellIndexNotEnabled = "legacy_txout-index-not-enabled",
```

**为什么不能简单换名**：
- 这些是运行时事件类型标识符
- 字符串匹配失败会导致事件丢失

**正确处理**：
- 改为 `CellIndexNotEnabled`, `"cell-index-not-enabled"`
- 确保所有订阅方同步更新

---

### Trap 7: 特性标志（Cargo feature）

**位置**：`testing/integration/Cargo.toml` 第 77 行

```toml
legacy-cellindex-tests = []
```

**位置**：`testing/integration/src/lib.rs` 第 11 行

```rust
#[cfg(feature = "legacy-cellindex-tests")]
```

**正确处理**：
- 改为 `legacy-cellindex-tests`
- 更新所有 `#[cfg(feature = ...)]` 引用

---

### Trap 8: 错误消息字符串（用户可见）

**位置**：`wallet/psst/src/error.rs` 第 15, 29 行

```rust
#[error("Missing legacy txout entry")]
#[error("Unlock legacy_txout error")]
```

**正确处理**：
- 改为 `"Missing Cell entry"`, `"Unlock cell error"`
- 这些会显示给用户，需要同步更新文档

---

### Trap 9: Shell 脚本中的 CLI 参数

**位置**：`treasure_boy/airdrop_integration_test.sh` 第 62, 289, 375 行

```bash
# 第 62 行
cargo run --bin sporad --release -- --testnet --netsuffix=10 --cellindex --rpclisten=127.0.0.1:16210

# 第 289 行（注释）
# Such as querying RPC for transaction status, checking legacy txout changes, etc.

# 第 375 行
echo "  - Private key must have sufficient legacy txout"
```

**正确处理**：
- `--cellindex` → `--cellindex`
- 注释和错误消息中的 `legacy txout` → `Cell`

---

### Trap 10: JavaScript/TypeScript 示例代码

**位置**：`wasm/examples/nodejs/` 目录下 12 个文件

```
wasm/examples/nodejs/javascript/transactions/cell-context-listener.js
wasm/examples/nodejs/javascript/transactions/cell-context-generator.js
wasm/examples/nodejs/javascript/wallet/wallet.js
wasm/examples/nodejs/javascript/general/mining-header.js
wasm/examples/nodejs/javascript/general/mining-pow.js
wasm/examples/nodejs/javascript/transactions/simple-transaction.js
wasm/examples/nodejs/javascript/transactions/single-transaction-demo.js
wasm/examples/nodejs/javascript/transactions/generator.js
wasm/examples/nodejs/javascript/transactions/estimate.js
wasm/examples/nodejs/javascript/refactoring/tx-script-sign.js
wasm/examples/nodejs/javascript/refactoring/tx-create.js
wasm/examples/nodejs/typescript/src/cell-context-listener.ts
```

**关键代码模式**：

```javascript
// cell-context-listener.js 第 10-11 行
const { CellProcessor, CellContext } = require('@spora/wasm');

// 第 40, 44, 46 行
let processor = new CellProcessor({ rpc, networkId });
let context = new CellContext({ processor });

// 第 73 行 - 事件名
processor.addEventListener("legacy_txout-proc-start", async (event) => { ... });

// wallet.js 第 138-140 行 - 字段名
MatureLegacyTxout: data.balance.matureLegacyTxoutCount,
PendingLegacyTxout: data.balance.pendingLegacyTxoutCount,
StasisLegacyTxout: data.balance.stasisLegacyTxoutCount

// 第 152, 155 行 - 事件 case
case "legacy_txout-proc-start":
case "legacy_txout-proc-stop":
```

**正确处理**：
- `CellProcessor` → `CellProcessor`
- `CellContext` → `CellContext`
- `"legacy_txout-proc-start"` → `"cell-proc-start"`
- `matureLegacyTxoutCount` → `matureCellCount`
- 文件名 `cell-context-listener.js` → `cell-context-listener.js`

---

### Trap 11: HTML 示例页面

**位置**：`wasm/examples/web/`

```
wasm/examples/web/index.html:50
  <li><a href="cell-context.html">CellContext</a></li>

wasm/examples/web/cell-context.html:9
  let { Resolver, RpcClient, Encoding, CellProcessor, CellContext } = spora;

wasm/examples/web/cell-context.html:33, 35
  monitor.processor = new CellProcessor({ rpc, networkId : network });
  let context = new CellContext({ processor : monitor.processor });

wasm/examples/web/cell-context.html:48, 53（用户可见文本）
  log("Please note that some addresses may have thousands of legacy txouts...");
  log("CellProcessor, your browser may have difficulty rendering large sets of legacy txouts.");
```

**正确处理**：
- HTML 文件名 `cell-context.html` → `cell-context.html`
- 链接文本 `CellContext` → `CellContext`
- JS 类名 `CellProcessor/CellContext` → `CellProcessor/CellContext`
- 用户提示文本中的 `legacy txouts` → `Cells`

---

### Trap 12: 文档中的历史引用

**位置**：`docs/*.md`（非方案文档本身）

```
docs/spora_cell_architecture_review.md - 解释 Cell 与 legacy txout 的区别
  第 40, 46, 72, 121, 160, 174, 216, 218 行

docs/spora_infra_completeness_audit.md
  第 22, 35, 39-40, 231 行
```

**正确处理**：
- 这些文档**保留原样**——它们是历史记录，解释为什么从 legacy txout 迁移到 Cell
- 只更新当前状态描述，历史对比部分不修改

---

## 执行顺序与依赖关系

```
Phase 0 (核心类型)
  │
  ├──→ Phase 1 (通知管道) ──→ Phase 2 (RPC层)
  │                                │
  └──→ Phase 4 (执行/挖矿/脚本) ──┘
                                   │
                                   ├──→ Phase 3 (钱包层)
                                   │
                                   └──→ Phase 5 (外围清理 + 验证)
```

**关键依赖**：
- Phase 1 依赖 Phase 0（CellEntry 消灭后通知类型才能改）
- Phase 2 依赖 Phase 1（通知事件名改完后 RPC 层才能对齐）
- Phase 3 依赖 Phase 0 + Phase 2（核心类型 + RPC 接口都改完后钱包才能跟）
- Phase 5 在所有其他 Phase 完成后执行

**语义陷阱的特殊处理**：
- Trap 1 (Proto 版本) 必须在 Phase 2 开始时处理
- Trap 2 (Serde 键) 在 Phase 3 钱包层处理
- Trap 3 (RPC 方法) 在 Phase 2 处理
- Trap 4 (CLI 参数) 在 Phase 5 处理
- Trap 5 (JS API) 在 Phase 2 wasm 部分处理
- Trap 6 (事件字符串) 在 Phase 1 处理
- Trap 7 (Feature 标志) 在 Phase 5 处理
- Trap 8 (错误消息) 在对应模块的 Phase 中处理
- Trap 9 (Shell 脚本) 在 Phase 5 处理
- Trap 10 (JS/TS 示例) 在 Phase 2 wasm 之后处理
- Trap 11 (HTML 示例) 在 Phase 2 wasm 之后处理
- Trap 12 (历史文档) **不处理**——保留作为迁移记录


## 完成标志

```bash
# 1. Rust 代码零残留
$ grep -rci 'legacy_txout' --include='*.rs' . \
    | grep -v '/target/' \
    | awk -F: '$2>0 {print}'
# 期望输出：空

# 2. Proto 定义零残留
$ grep -ri 'legacy_txout\|LegacyTxout\|legacy txout' --include='*.proto' .
# 期望输出：空

# 3. 配置文件零残留
$ grep -ri 'legacy_txout\|LegacyTxout\|legacy txout' --include='*.toml' --include='*.yaml' --include='*.json' .
# 期望输出：空

# 4. 文档中仅历史说明保留
$ grep -ri 'legacy_txout\|LegacyTxout\|legacy txout' --include='*.md' . \
    | grep -v 'CHANGELOG' \
    | grep -v 'MIGRATION' \
    | grep -v 'HISTORY'
# 期望输出：空

# 5. 编译通过
cargo build --all-targets --all-features

# 6. 测试通过
cargo test --all-targets
```

**最终状态**：代码库中 `legacy_txout` / `LegacyTxout` / `legacy txout` 字样**完全消失**，如同从未存在过。
