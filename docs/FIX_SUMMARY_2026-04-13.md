# Spora 修复总结报告

**日期**: 2026-04-13  
**修复范围**: 钱包通知系统

---

## 修复内容

### 1. WalletNotification 结构体扩展 ✅

**文件**: `wallet/core/src/api/message.rs`

**修复前**:
```rust
pub struct WalletNotification {}
```

**修复后**:
```rust
pub enum WalletNotification {
    WalletPing,
    CellIndexNotEnabled { url },
    WalletOpen { wallet_descriptor, account_descriptors },
    WalletCreate { wallet_descriptor, storage_descriptor },
    WalletError { message },
    WalletClose,
    WalletReload { wallet_descriptor, account_descriptors },
    PrvKeyDataCreate { prv_key_data_info },
    AccountSelection { id },
    AccountActivation { ids },
    AccountDeactivation { ids },
    AccountCreate { account_descriptor },
    AccountUpdate { account_descriptor },
    ServerStatus { network_id, server_version, is_synced, url },
    CellProcStart,
    CellProcStop,
    CellProcError { message },
    Discovery { record },
    Pending { record },
    Maturity { record },
    Reorg { record },
    Stasis { record },
    Balance { id, balance },
    Metrics { network_id, metrics },
    FeeRate { priority, normal, low },
    SyncState { sync_state },
    Connect { network_id, url },
    Disconnect { network_id, url },
    DaaScoreChange { current_daa_score },
    Error { message },
}
```

**状态**: ✅ 已修复，代码编译通过，测试通过

---

### 2. 事件转换实现 ✅

**文件**: `wallet/core/src/events.rs`

添加了 `From<&Events>` for `WalletNotification` 实现，将内部 `Events` 枚举无损转换为 `WalletNotification`。

**状态**: ✅ 已实现

---

### 3. 通知链路修复 ✅

**修复前**:
```rust
async fn register_notifications(self: Arc<Self>, channel: Receiver<WalletNotification>) -> Result<u64>;
notification_channels: HashMap<u64, Receiver<WalletNotification>>
```

**修复后**:
```rust
async fn register_notifications(self: Arc<Self>) -> Result<(u64, Receiver<WalletNotification>)>;
notification_channels: HashMap<u64, Sender<WalletNotification>>
```

`Wallet::notify()` 现在会把 `Events` 同时广播到 multiplexer 和已注册的 `WalletNotification` channel。`WalletClient` 也实现了 `EventHandler`，会把服务端转发过来的 `Events` 转成 `WalletNotification` 发给本地监听者。

---

### 4. WASM / TypeScript 事件面校准 ✅

**文件**:

- `wallet/core/src/events.rs`
- `wallet/core/src/wasm/notify.rs`
- `wallet/core/src/wasm/wallet/wallet.rs`

本轮补齐了 WASM / TypeScript 侧与 Rust 真实事件集的偏差：

- `WalletEventType` / `WalletEventMap` 新增并对齐 `wallet-ping`、`wallet-list`、`metrics`
- `CellProcessorEventType` / `CellProcessorEventMap` 新增并对齐 `server-status`、`metrics`、`fee-rate`
- `Wallet.addEventListener()` 的 TypeScript 重载改为返回真实的完整事件对象，而不是错误地声明为仅返回 `eventData`
- `EventKind` 统一使用 `wallet-ping` 作为标准字符串，同时保留对历史别名 `wallet-start` 的兼容解析
- `EventKind::All` 现在同时接受 `"all"` 和 `"*"`，避免手写字符串订阅时出现不必要的歧义
- `wallet-open` / `wallet-reload` 的 TS payload 改为可选字段，对齐 Rust 侧 `Option<T>`
- `FeeRateEstimateBucket` 真实序列化键已对齐为 `feeRate`，通知侧 `IFeeRateEvent` 也同步改成 `feeRate: number`
- `Hint` 现在对外按透明字符串序列化，`wallet-hint` 与 `userHint` 不再在 `{ text }` 和 `string` 之间来回漂移
- `sync-state` 的 TS 类型改为和 Rust 一致的 tagged union，不再错误声明为 `{ event, data }`
- `CellProcessorEvent` / `IWalletEvent` 对 unit 事件不再强制要求 `data` 字段，和真实 `{ type }` 事件对象对齐

**状态**: ✅ 已修复

---

## 测试状态

```
running 63 tests
test result: ok. 59 passed; 0 failed; 4 ignored
```

所有钱包核心测试通过。

---

## 修复后评级

| 组件 | 修复前 | 修复后 | 状态 |
|------|--------|--------|------|
| WalletNotification 数据 | ❌ 空结构体 | ✅ 完整枚举 | 已修复 |
| 事件转换 | ❌ 未实现 | ✅ 已实现 | 已修复 |
| 通知链路 | ❌ 断裂 | ✅ 已闭环 | 已修复 |
| WASM / TS 事件类型 | ⚠️ 与真实事件集存在漂移 | ✅ 已对齐 | 已修复 |
| WASM/JS 事件 | ✅ 正常工作 | ✅ 正常工作 | 无变化 |

**总体状态**: 钱包通知系统已闭环，原生 Rust receiver、`Wallet::notify()` 本地转发、`WalletServer -> WalletClient` transport 转发与 WASM/JS multiplexer 均可工作。

---

## 建议的后续工作

### 短期（本周）
1. 评估是否需要为原生 Rust 通知增加过滤或订阅范围
2. 扩大更多事件类型的 server/client 集成覆盖

### 中期（2-4 周）
1. 统一 `Events` / `WalletNotification` 的对外文档
2. 补更多事件类型的行为测试

---

## 相关文件

- `wallet/core/src/api/message.rs` - WalletNotification 定义
- `wallet/core/src/events.rs` - Events 枚举和转换实现
- `wallet/core/src/wallet/mod.rs` - 通知分发逻辑
- `wallet/core/src/wallet/api.rs` - register_notifications 实现
- `wallet/core/src/api/transport.rs` - WalletServer / WalletClient 通知转发
- `wallet/core/src/wasm/notify.rs` - WASM / TypeScript 事件类型声明
- `wallet/core/src/wasm/wallet/wallet.rs` - Wallet.addEventListener 类型声明
