# Spora Kaspa 遗留代码去除计划

**文档版本**: v2.0  
**更新日期**: 2026-04-12  
**对比基准**: CKB 仓库 (`/Users/arthur/RustroverProjects/ckb`)

---

## 1. 背景与目标

### 1.1 背景

Spora 项目是从 Kaspa 代码库 fork 而来，并逐步迁移到 CKB-VM + Cell 模型架构。目前核心共识层已完成 Cell 模型迁移，但仍有大量 Kaspa 时代的遗留代码散布在 RPC、钱包、共识核心等模块中，产生大量 `#[deprecated]` 警告，阻碍代码清晰度和维护性。

### 1.2 目标

完全移除 Kaspa 遗留类型和桥接代码，实现纯 Cell 模型架构，与 CKB 设计理念对齐。

---

## 2. 遗留代码全景扫描

### 2.1 待清理类型清单（统一口径）

| 类型 | 分类 | 当前状态 | 所在文件 | 处理方向 | 优先级 |
|------|------|---------|---------|---------|--------|
| `SubnetworkId` | 应删除 | `#[deprecated]` | `consensus/core/src/subnets.rs` | 删除，改用 `CellTx::is_coinbase()` | 🔴 P0 |
| `ScriptPublicKey` | 应删除 | 活跃使用 | `consensus/core/src/tx/script_public_key.rs` | 替换为 `ScriptRef` | 🔴 P0 |
| `RpcSubnetworkId` | 应删除 | RPC层残留 | `rpc/core/src/model/subnets.rs` | 随SubnetworkId一起删除 | 🔴 P0 |
| `TransactionOutpoint` | 应合并 | 类型别名 | `consensus/core/src/tx.rs` | 统一使用 `OutPoint` | 🟢 P2 |
| `SignableTransaction` | 应合并 | 活跃使用 | `consensus/core/src/tx.rs` | 并入统一的 resolved transaction 抽象 | 🟡 P1 |
| `MutableTransaction` | 应合并 | 活跃使用 | `consensus/core/src/tx.rs` | 保留能力但不保留独立概念 | 🟡 P1 |
| `PopulatedTransaction` | 应合并 | 活跃使用 | `consensus/core/src/tx.rs` | 并入统一抽象 | 🟢 P2 |
| `ValidatedTransaction` | 应合并 | 活跃使用 | `consensus/core/src/tx.rs` | 将 `calculated_fee` 等能力并入统一抽象 | 🟢 P2 |
| `TransactionInfo` | 保留 | 活跃使用 | `exec/src/celltx/types.rs` | DAG 特有信息，保留且不计入冗余项 | - |

**更新说明**: 经过重新审计，`Transaction`/`TransactionInput`/`TransactionOutput` 已不存在于当前代码库（此前已移除），文档已更新。

### 2.2 遗留桥接函数清单

| 函数名 | 用途 | 所在文件 | 调用次数 | 优先级 |
|-------|------|---------|---------|--------|
| `cell_meta_from_legacy_output()` | ScriptPublicKey → CellMeta | `consensus/core/src/tx.rs` | 12+ | 🔴 P0 |
| `legacy_sequence_to_cell_since()` | sequence → since | `consensus/core/src/tx.rs` | 15+ | 🟡 P1 |
| `cell_out_from_legacy_script_public_key()` | ScriptPublicKey → CellOut | `consensus/core/src/tx.rs` | 3+ | 🟡 P1 |
| `cell_entry_legacy_script_public_key()` | CellEntry → ScriptPublicKey | `consensus/core/src/tx.rs` | 6+ | 🟡 P1 |
| `compute_lock_hash_for_script()` | 为ScriptPublicKey派生lock hash | `consensus/core/src/tx.rs` | 3+ | 🟡 P1 |

**更新说明**: `cell_tx_from_legacy_transaction()` 和 `legacy_compat_transaction_from_cell_tx()` 在当前代码库中已不存在（此前已移除）。

### 2.3 Placeholder / 兼容元数据桥接函数清单

| 函数名 | 用途 | 所在文件 | 最终状态 | 优先级 |
|-------|------|---------|---------|--------|
| `parse_cell_metadata_placeholder_script_public_key()` | 从占位 `ScriptPublicKey` 解码 `CellMetadata` | `consensus/core/src/cell_metadata.rs` | 删除 | 🔴 P0 |
| `cell_metadata_placeholder_script_public_key()` | 将 `lock_hash` 编码到占位 `ScriptPublicKey` | `consensus/core/src/cell_metadata.rs` | 删除 | 🔴 P0 |
| `cell_metadata_placeholder_script_public_key_with_metadata()` | 将 `lock/type/data` 元数据编码到占位 `ScriptPublicKey` | `consensus/core/src/cell_metadata.rs` | 删除 | 🔴 P0 |
| `is_cell_metadata_placeholder_script_public_key()` | 判断脚本是否为 metadata placeholder | `consensus/core/src/cell_metadata.rs` | 删除 | 🟡 P1 |
| `CellMetadata::to_placeholder_cell_entry()` | 将 `CellMetadata` 降级为 placeholder `CellEntry` | `consensus/core/src/cell_metadata.rs` | 删除 | 🟡 P1 |
| `compute_lock_hash_for_script()` | 为 legacy `ScriptPublicKey` 派生合成 lock hash | `consensus/core/src/tx.rs` | 删除 | 🟡 P1 |

**结论**: 这组函数全部属于 Cell 元数据向 legacy `ScriptPublicKey` / `CellEntry` 的过渡桥接。  
在纯 Cell-native 目标下，`CellMetadata`、`ScriptRef`、`CellOut` 应直接在模块边界上传递，因此这组函数**最终都应删除**，不应作为长期 ABI 保留。

---

## 3. 模块级遗留代码分析

### 3.1 Consensus Core (核心共识层)

#### 3.1.1 已完全迁移 ✅

| 组件 | 状态 | 说明 |
|------|------|------|
| `CellDiff` | ✅ Cell 原生 | 完全使用 Cell 模型 |
| `CellStateTree` | ✅ Cell 原生 | MuHash 累加器 |
| `CellValidator` | ✅ Cell 原生 | 验证 CellTx |
| `VirtualProcessor` | ✅ Cell 原生 | 处理 CellTx |

#### 3.1.2 遗留代码热点 🔥

**文件**: `consensus/core/src/tx.rs`

```rust
// 遗留桥接函数 (应删除)
pub fn cell_meta_from_legacy_output(...) -> CellEntry { ... }
pub fn cell_out_from_legacy_script_public_key(...) -> CellOut { ... }
pub fn cell_entry_legacy_script_public_key(...) -> ScriptPublicKey { ... }
pub fn compute_lock_hash_for_script(...) -> [u8; 32] { ... }
pub fn legacy_sequence_to_cell_since(sequence: u64) -> u64 { ... }

// 类型别名 (应统一使用 OutPoint)
pub type TransactionOutpoint = spora_exec::celltx::OutPoint;
```

**更新说明**: `Transaction`/`TransactionInput`/`TransactionOutput` 结构体以及 `cell_tx_from_legacy_transaction`/`legacy_compat_transaction_from_cell_tx` 函数在当前代码库中已不存在。

**文件**: `consensus/core/src/subnets.rs` (全文件)

```rust
// Kaspa 子网概念，Cell 模型不需要
#[deprecated(note = "Use CellTx::is_coinbase() instead")]
pub struct SubnetworkId([u8; SUBNETWORK_ID_SIZE]);

#[deprecated]
pub const SUBNETWORK_ID_NATIVE: SubnetworkId = ...;
#[deprecated]
pub const SUBNETWORK_ID_COINBASE: SubnetworkId = ...;
#[deprecated]
pub const SUBNETWORK_ID_REGISTRY: SubnetworkId = ...;
```

**文件**: `consensus/core/src/tx/script_public_key.rs` (全文件)

```rust
// Kaspa 风格脚本公钥，应替换为 ScriptRef
pub struct ScriptPublicKey {
    pub version: ScriptPublicKeyVersion,
    pub(super) script: ScriptVec,
}
```

**对比 CKB**: 
- CKB 使用 `Script` 结构体 (`util/types/src/core/cell.rs`)
- Spora 应完全使用 `ScriptRef` (`exec/src/celltx/types.rs`)

**文件**: `consensus/core/src/cell_metadata.rs` (placeholder 编解码区段)

```rust
// CellMetadata -> legacy ScriptPublicKey placeholder (应删除)
pub fn cell_metadata_placeholder_script_public_key(...) -> ScriptPublicKey { ... }
pub fn cell_metadata_placeholder_script_public_key_with_metadata(...) -> ScriptPublicKey { ... }

// legacy ScriptPublicKey placeholder -> CellMetadata (应删除)
pub fn parse_cell_metadata_placeholder_script_public_key(...) -> Option<PlaceholderCellMetadata> { ... }
pub fn is_cell_metadata_placeholder_script_public_key(...) -> bool { ... }

// CellMetadata -> legacy CellEntry bridge (应删除)
pub fn to_placeholder_cell_entry(&self) -> CellEntry { ... }
```

**判断**:
- 这些函数不是 Cell 模型必需能力，而是为 RPC / wallet / p2p / legacy signing 兼容层服务
- 一旦 `ScriptPublicKey`、legacy `CellEntry`、placeholder 编码从主路径移除，这组函数应整体删除
- 最终目标应是直接传递 `CellMetadata` / `ScriptRef` / `CellOut`，而不是把 metadata 塞进 `ScriptPublicKey`

### 3.2 RPC 层

#### 3.2.1 遗留代码分布

**文件**: `rpc/core/src/model/tx.rs`

```rust
// 遗留类型别名
pub type RpcScriptPublicKey = ScriptPublicKey;  // 应改为 ScriptRef

// 遗留字段命名
pub struct RpcTransactionInput {
    pub signature_script: Vec<u8>,  // 应改为 witness
    pub sequence: u64,              // 应改为 since
    pub sig_op_count: u8,           // 遗留字段，应删除
    pub since: Option<u64>,         // Cell原生字段已存在
    pub witness: Option<Vec<u8>>,   // Cell原生字段已存在
    ...
}

pub struct RpcTransactionOutput {
    pub value: u64,                 // 应改为 capacity
    pub capacity: Option<u64>,      // Cell原生字段已存在
    pub script_public_key: RpcScriptPublicKey,  // 应改为 lock/type
    pub lock_hash: Option<[u8; 32]>,      // Cell原生字段已存在
    pub type_hash: Option<[u8; 32]>,      // Cell原生字段已存在
    ...
}

pub struct RpcCellEntry {
    pub script_public_key: ScriptPublicKey,  // 应改为 ScriptRef
    ...
}
```

**文件**: `rpc/core/src/model/subnets.rs` (全文件)

```rust
// RPC层的SubnetworkId残留
pub struct RpcSubnetworkId([u8; SUBNETWORK_ID_SIZE]);
```

**文件**: `rpc/core/src/model/message.rs`

```rust
// Subnetwork RPC消息
pub struct GetSubnetworkRequest {
    pub subnetwork_id: RpcSubnetworkId,
}
```

**文件**: `rpc/service/src/service.rs`

```rust
// 遗留导入
use spora_consensus_core::tx::{..., ScriptPublicKey, TransactionOutpoint};

// 遗留函数
fn compute_lock_hash(script_public_key: &ScriptPublicKey) -> [u8; 32] { ... }
fn transaction_outpoint_from_exec(out_point: &OutPoint) -> TransactionOutpoint { ... }
```

**对比 CKB**:
- CKB RPC 使用原生 Cell 类型
- Spora RPC 应保持与 CKB 兼容的命名规范

### 3.3 Mempool 层

#### 3.3.1 状态: 已完全 Cell 化 ✅

| 组件 | 状态 | 说明 |
|------|------|------|
| `CellPool` | ✅ Cell 原生 | 使用 `CellTx` |
| `PoolEntry` | ✅ Cell 原生 | 使用 `CellTx` |
| `TransactionScorer` | ✅ Cell 原生 | 使用 `CellTx` |

**结论**: Mempool 模块无需修改，已是最佳实践。

### 3.4 Wallet 层

#### 3.4.1 遗留代码热点

**文件**: `wallet/psst/src/psst.rs`

```rust
// 遗留导入
use spora_consensus_core::tx::{
    legacy_sequence_to_cell_since, 
    cell_meta_from_legacy_output,
    ...
};

// 遗留调用
CellRef::new(*previous_outpoint, legacy_sequence_to_cell_since(sequence.unwrap_or(u64::MAX)))
```

**文件**: `wallet/core/src/tx/generator/generator.rs`

```rust
// 遗留导入
use spora_consensus_core::tx::{
    cell_out_from_legacy_script_public_key,
    legacy_sequence_to_cell_since,
    ...
};
```

**文件**: `treasure_boy/src/lib.rs`

```rust
// 遗留导入
use spora_consensus_core::tx::{
    legacy_compat_transaction_from_cell_tx,
    legacy_sequence_to_cell_since,
    cell_meta_from_legacy_output,
    ...
};
```

### 3.5 与 CKB 对比总结

| 维度 | CKB | Spora 当前 | Spora 目标 |
|------|-----|-----------|-----------|
| 核心交易类型 | `TransactionView` | `CellTx` ✅ | 保持 |
| 输入类型 | `CellInput` | `CellRef` ✅ | 保持 |
| 输出类型 | `CellOutput` | `CellOut` ✅ | 保持 |
| 脚本类型 | `Script` | `ScriptRef` + `ScriptPublicKey`(遗留) | 纯 `ScriptRef` |
| 子网概念 | 无 | `SubnetworkId`(遗留) | 删除 |
| 交易池 | `TxPool` | `CellPool` ✅ | 保持 |
| VM 验证器 | `TransactionScriptsVerifier` | `TransactionScriptVerifier` ✅ | 保持 |

**更新说明**: `Transaction`/`TransactionInput`/`TransactionOutput` 已不存在于当前代码库，Spora核心交易类型已完全Cell化。

---

## 4. 分阶段迁移计划

### Phase 1: 核心类型删除 (P0) - 2 周

**目标**: 删除 `SubnetworkId`, `RpcSubnetworkId` 及相关常量和RPC接口

**更新说明**: 经过重新审计，`Transaction`/`TransactionInput`/`TransactionOutput` 已不存在于当前代码库，Phase 1 聚焦于 `SubnetworkId` 的清理。

#### Week 1: 准备与依赖分析

1. **标记所有调用点**
   ```bash
   cd /Users/arthur/RustroverProjects/Spora
   rg "SUBNETWORK_ID_" --type rust -n
   rg "SubnetworkId" --type rust -n
   rg "RpcSubnetworkId" --type rust -n
   rg "get_subnetwork" --type rust -n
   ```

2. **创建替代实现**
   - 将 `SubnetworkId::is_coinbase()` 调用替换为 `CellTx::is_coinbase()`
   - 移除 RPC `get_subnetwork` 接口（Cell模型不需要子网概念）

#### Week 2: 删除与验证

1. **删除共识层 subnets.rs**
   ```bash
   rm consensus/core/src/subnets.rs
   # 更新 consensus/core/src/lib.rs 移除模块引用
   ```

2. **删除 RPC 层 subnets.rs**
   ```bash
   rm rpc/core/src/model/subnets.rs
   # 更新 rpc/core/src/model/mod.rs 移除模块引用
   ```

3. **删除 RPC get_subnetwork 接口**
   ```rust
   // rpc/core/src/api/rpc.rs
   // 删除: async fn get_subnetwork(&self, subnetwork_id: RpcSubnetworkId) -> RpcResult<GetSubnetworkResponse>;
   
   // rpc/core/src/model/message.rs
   // 删除: GetSubnetworkRequest, GetSubnetworkResponse
   ```

4. **验证编译**
   ```bash
   cargo check --workspace 2>&1 | grep -E "error|warning" | wc -l
   # 目标: 0 errors, warnings < 50
   ```

### Phase 2: 桥接函数删除 (P1) - 2 周

**目标**: 删除所有 legacy / placeholder 桥接函数

#### Week 1: 钱包层迁移

1. **wallet/psst 迁移**
   - 替换 `legacy_sequence_to_cell_since` 为原生 `since` 处理
   - 替换 `cell_meta_from_legacy_output` 为 `CellMeta` 原生构造
   - 替换 `parse_cell_metadata_placeholder_script_public_key` 为直接消费 `CellMetadata`

2. **wallet/core 迁移**
   - 更新 `generator.rs` 使用 `ScriptRef` 替代 `ScriptPublicKey`
   - 删除对 placeholder `ScriptPublicKey` 的依赖

#### Week 2: 工具层迁移

1. **treasure_boy 迁移**
   - 删除 `legacy_compat_transaction_from_cell_tx` 调用
   - 直接使用 `CellTx` API
   - 删除 `compute_lock_hash_for_script` 路径，直接构造原生 `ScriptRef`

2. **RPC / P2P / client placeholder 迁移**
   - 删除 `cell_metadata_placeholder_script_public_key_with_metadata`
   - 删除 `cell_metadata_placeholder_script_public_key`
   - 删除 `parse_cell_metadata_placeholder_script_public_key`
   - 删除 `is_cell_metadata_placeholder_script_public_key`
   - 删除 `CellMetadata::to_placeholder_cell_entry`
   - 将消息模型改为直接传输 `CellMetadata` / `ScriptRef` / `CellOut`

3. **删除桥接函数**
   ```rust
   // consensus/core/src/tx.rs
   // - cell_meta_from_legacy_output()
   // - legacy_sequence_to_cell_since()
   // - cell_out_from_legacy_script_public_key()
   // - cell_entry_legacy_script_public_key()
   // - compute_lock_hash_for_script()

   // consensus/core/src/cell_metadata.rs
   // - cell_metadata_placeholder_script_public_key()
   // - cell_metadata_placeholder_script_public_key_with_metadata()
   // - parse_cell_metadata_placeholder_script_public_key()
   // - is_cell_metadata_placeholder_script_public_key()
   // - CellMetadata::to_placeholder_cell_entry()
   ```

   **更新说明**: `cell_tx_from_legacy_transaction()` 和 `legacy_compat_transaction_from_cell_tx()` 在当前代码库中已不存在。

**删除判定标准**:
- 主路径不再存在 `ScriptPublicKey` / `TransactionOutput` / legacy `CellEntry`
- RPC / wallet / p2p / client 不再通过 placeholder script 传递 Cell 元数据
- `CellMetadata`、`ScriptRef`、`CellOut` 已成为唯一跨模块数据形态

### Phase 3: ScriptPublicKey 替换 (P1) - 3 周

**目标**: 将 `ScriptPublicKey` 完全替换为 `ScriptRef`

#### Week 1-2: RPC 层迁移

1. **更新 RPC 模型**
   ```rust
   // rpc/core/src/model/tx.rs
   // 修改:
   pub type RpcScriptPublicKey = ScriptRef;  // 替代 ScriptPublicKey
   
   pub struct RpcTransactionOutput {
       pub capacity: u64,           // 替代 value
       pub lock: ScriptRef,         // 替代 script_public_key
       pub type_: Option<ScriptRef>, // 新增
       ...
   }
   ```

2. **更新 Converter**
   ```rust
   // rpc/service/src/converter/consensus.rs
   // 使用 ScriptRef 替代 ScriptPublicKey
   ```

#### Week 3: 验证与测试

1. **API 兼容性测试**
   ```bash
   cargo test --package spora-rpc-core
   cargo test --package spora-rpc-service
   ```

2. **端到端测试**
   ```bash
   cargo test --workspace --test integration
   ```

### Phase 4: 测试与文档 (P2) - 1 周

1. **更新测试用例**
   - 将 `consensus/core/src/tx.rs` 测试从 `Transaction` 迁移到 `CellTx`

2. **更新文档**
   - 更新 `CKB_TO_SPORA_MAPPING.md`
   - 删除遗留类型引用

3. **最终验证**
   ```bash
   cargo test --workspace
   cargo clippy --workspace -- -D warnings
   ```

---

## 5. 风险与缓解

### 5.1 风险矩阵

| 风险 | 可能性 | 影响 | 缓解措施 |
|------|--------|------|---------|
| RPC API 破坏 | 高 | 高 | 保持 RPC 字段兼容，仅修改内部类型 |
| 钱包功能回归 | 中 | 高 | 完整 PSST 测试套件验证 |
| 共识层意外行为 | 低 | 极高 | 渐进式删除，每阶段完整测试 |
| 外部集成破坏 | 中 | 中 | 提前通知，提供迁移指南 |

### 5.2 回滚策略

每个 Phase 完成后创建 tag，便于回滚：
```bash
git tag -a pre-phase-1-$(date +%Y%m%d) -m "Before Phase 1 legacy removal"
git tag -a pre-phase-2-$(date +%Y%m%d) -m "Before Phase 2 bridge removal"
git tag -a pre-phase-3-$(date +%Y%m%d) -m "Before Phase 3 ScriptPublicKey removal"
```

---

## 6. 成功标准

### 6.1 量化指标

| 指标 | 当前值 | 目标值 |
|------|--------|--------|
| `#[deprecated]` 警告数量 | ~15 | 0 |
| `legacy_*` 函数调用 | ~45 | 0 |
| `SubnetworkId` 引用 | ~25 | 0 |
| `RpcSubnetworkId` 引用 | ~10 | 0 |
| `ScriptPublicKey` 引用 (内部) | ~90 | 0 |

**更新说明**: `Transaction` 类型引用已为 0（此前已移除），当前 `#[deprecated]` 警告主要来自 `SubnetworkId` 和 state/index 模块。

### 6.2 质量指标

- [ ] `cargo test --workspace` 全部通过
- [ ] `cargo clippy --workspace` 无警告
- [ ] 基准测试无性能回归
- [ ] 文档完全更新

---

## 7. 附录

### 7.1 遗留代码 grep 命令参考

```bash
# 查找所有 deprecated 属性
rg "#\[deprecated" --type rust -n

# 查找所有 legacy 函数
rg "legacy_" --type rust -n

# 查找 SubnetworkId 使用 (包括RPC层)
rg "SubnetworkId|RpcSubnetworkId" --type rust -n

# 查找 ScriptPublicKey 使用
rg "ScriptPublicKey" --type rust -n | grep -v "ScriptRef"

# 查找 placeholder 函数
rg "placeholder" --type rust -n

# 查找 get_subnetwork RPC接口
rg "get_subnetwork" --type rust -n
```

### 7.2 CKB 参考文件

| CKB 文件 | 说明 |
|---------|------|
| `ckb/util/types/src/core/cell.rs` | Cell 类型定义参考 |
| `ckb/util/types/src/core/views.rs` | TransactionView 定义 |
| `ckb/script/src/verify.rs` | 脚本验证器参考 |
| `ckb/tx-pool/src/pool.rs` | 交易池参考 |

### 7.3 Spora 关键文件

| Spora 文件 | 说明 |
|-----------|------|
| `consensus/core/src/tx.rs` | 遗留桥接函数主要位置 |
| `consensus/core/src/subnets.rs` | SubnetworkId 定义 (应删除) |
| `consensus/core/src/tx/script_public_key.rs` | ScriptPublicKey 定义 (应替换为 ScriptRef) |
| `consensus/core/src/cell_metadata.rs` | Placeholder 桥接函数 (应删除) |
| `rpc/core/src/model/subnets.rs` | RpcSubnetworkId 定义 (应删除) |
| `rpc/core/src/model/tx.rs` | RPC 遗留类型 (ScriptPublicKey 等) |
| `rpc/core/src/model/message.rs` | Subnetwork RPC 消息 (应删除) |
| `exec/src/celltx/types.rs` | 目标 Cell 类型 |

---

## 8. Spora vs CKB 数据结构对比分析

### 8.1 核心类型对比表

| 类型 | CKB | Spora | 差异分析 |
|------|-----|-------|---------|
| **OutPoint** | `tx_hash: Byte32, index: u32` | `tx_hash: [u8; 32], index: u32` | ✅ 本质相同 |
| **Script** | `code_hash: Byte32, hash_type: u8, args: Bytes` | `code_hash: [u8; 32], hash_type: u8, args: Vec<u8>` | ✅ 本质相同 |
| **CellOutput** | `capacity: Capacity, lock: Script, type_: ScriptOpt` | `lock: ScriptRef, type_: Option<ScriptRef>, capacity: u64` | ✅ 本质相同，字段顺序不同 |
| **CellInput** | `since: u64, previous_output: OutPoint` | `out_point: OutPoint, since: u64` | ✅ 本质相同，字段顺序不同 |
| **CellDep** | `out_point: OutPoint, dep_type: u8` | `out_point: OutPoint, dep_type: DepType` | ✅ 本质相同，DepType用enum |
| **Transaction** | `version: u32, cell_deps: [CellDep], inputs: [CellInput], outputs: [CellOutput], outputs_data: [Bytes], header_deps: [Byte32]` | `ver: u16, inputs: [CellRef], deps: [CellDep], header_deps: [[u8; 32]], outputs: [CellOut], outputs_data: [Vec<u8>], witnesses: [Vec<u8>]` | ⚠️ version大小不同，witnesses位置不同 |
| **CellMeta** | `cell_output: CellOutput, out_point: OutPoint, transaction_info: Option<TransactionInfo>, data_bytes: u64, mem_cell_data: Option<Bytes>, mem_cell_data_hash: Option<Byte32>` | `cell_output: CellOut, out_point: OutPoint, transaction_info: Option<TransactionInfo>, data_bytes: u64, mem_cell_data: Option<Vec<u8>>, mem_cell_data_hash: Option<[u8; 32]>` | ✅ 本质相同 |

### 8.2 Spora 待清理数据结构分析（与 2.1 统一）

#### 🔴 应删除

| 数据结构 | 位置 | 原因 | 替代方向 |
|---------|------|------|---------|
| `SubnetworkId` | `consensus/core/src/subnets.rs` | Kaspa 子网概念，Cell 模型不需要 | 直接删除 |
| `RpcSubnetworkId` | `rpc/core/src/model/subnets.rs` | RPC层SubnetworkId残留 | 随SubnetworkId一起删除 |
| `ScriptPublicKey` | `consensus/core/src/tx/script_public_key.rs` | Kaspa 脚本公钥模型 | 统一为 `ScriptRef` |
| `TransactionOutpoint` | `consensus/core/src/tx.rs` | 兼容性别名，增加概念噪音 | 统一为 `OutPoint` |

**更新说明**: `Transaction`/`TransactionInput`/`TransactionOutput` 已不存在于当前代码库（此前已移除）。

#### 🟡 应合并

| 数据结构 | 位置 | 问题 | 建议 |
|---------|------|------|------|
| `SignableTransaction` | `consensus/core/src/tx.rs` | 包装 `CellTx` + entries | 并入统一的 resolved transaction 抽象 |
| `MutableTransaction` | `consensus/core/src/tx.rs` | 同上，支持增量填充 | 保留增量构建能力，但不保留独立概念 |
| `PopulatedTransaction` | `consensus/core/src/tx.rs` | 只读引用包装 | 并入统一抽象 |
| `ValidatedTransaction` | `consensus/core/src/tx.rs` | 仅附加 `calculated_fee` 等派生信息 | 并入统一抽象 |

#### 🟢 保留（不计入冗余项）

| 数据结构 | 位置 | 原因 |
|---------|------|------|
| `TransactionInfo` | `exec/src/celltx/types.rs` | DAG 交易信息，属于 Spora 特有必要扩展 |

#### 🟢 Spora特有（合理存在）

| 数据结构 | 位置 | 说明 | 合理性 |
|---------|------|------|--------|
| `CellStatus::Dead(u64)` | `exec/src/celltx/types.rs` | 包含DAA分数 | ✅ DAG需要 |
| `TransactionInfo.daa_score` | `exec/src/celltx/types.rs` | 使用DAA而非block_number | ✅ DAG特性 |
| `CellTx::header_deps` | `exec/src/celltx/types.rs` | 显式header依赖 | ✅ 与CKB一致 |
| `CellTx::compute_mass()` | `exec/src/celltx/types.rs` | 计算mass | ✅ Spora特有mass模型 |
| `CellTx::storage_mass()` | `exec/src/celltx/types.rs` | 存储mass | ✅ Spora特有mass模型 |

### 8.3 待清理类型数量汇总

| 分类 | 数量 | 项目 |
|------|------|------|
| 应删除 | 4 | `SubnetworkId`, `RpcSubnetworkId`, `ScriptPublicKey`, `TransactionOutpoint` |
| 应合并 | 4 | `SignableTransaction`, `MutableTransaction`, `PopulatedTransaction`, `ValidatedTransaction` |
| 保留 | 1 | `TransactionInfo` |
| **待清理合计** | **8** | 不含 `TransactionInfo` |

**更新说明**: `Transaction`/`TransactionInput`/`TransactionOutput` 已不存在于当前代码库，待清理类型从 10 个减少到 8 个。

### 8.4 建议精简方案

```rust
// 当前 Spora 类型层次（复杂）
VerifiableTransaction (trait)
├── PopulatedTransaction
├── ValidatedTransaction
├── MutableTransaction<T>
│   └── SignableTransaction (type alias)
└── ResolvedCellTransaction

// 建议简化后（接近CKB）
ResolvedCellTx  (合并所有功能)
└── 包含: tx, resolved_inputs, resolved_deps, calculated_fee, etc.
```

### 8.5 与CKB对比结论

| 维度 | 评估 |
|------|------|
| **核心Cell类型** | ✅ 与CKB对齐良好 |
| **交易类型** | 🔴 存在Kaspa遗留，需清理 |
| **脚本类型** | 🟡 ScriptPublicKey多余，应统一为ScriptRef |
| **辅助类型** | 🟡 存在 4 个交易包装类型可合并 |
| **DAG适配** | ✅ TransactionInfo等适配合理 |

**总体评价**: Spora 核心 Cell 类型与 CKB 基本一致，核心交易类型 (`CellTx`/`CellRef`/`CellOut`) 已完全Cell化。当前仍有 **4 个应删除的 Kaspa/兼容遗留类型** (`SubnetworkId`, `RpcSubnetworkId`, `ScriptPublicKey`, `TransactionOutpoint`) 与 **4 个应合并的交易包装类型**。`TransactionInfo` 属于 DAG 特有扩展，应保留且不计入冗余项。

**重要发现**: 经过重新审计，`Transaction`/`TransactionInput`/`TransactionOutput` 已不存在于当前代码库，说明此前已完成核心交易类型的Cell化迁移。当前遗留主要集中在 `SubnetworkId` (共识层+RPC层) 和 `ScriptPublicKey` (脚本层)。

---

## 9. 重新审计总结

### 9.1 与初版文档的差异

| 项目 | 初版审计 | 重新审计 | 差异说明 |
|------|---------|---------|---------|
| `Transaction` 类型 | 🔴 应删除 | ✅ 已不存在 | 此前已完成移除 |
| `TransactionInput` 类型 | 🔴 应删除 | ✅ 已不存在 | 此前已完成移除 |
| `TransactionOutput` 类型 | 🔴 应删除 | ✅ 已不存在 | 此前已完成移除 |
| `RpcSubnetworkId` | 未列出 | 🔴 应删除 | RPC层残留，需补充清理 |
| `get_subnetwork` RPC | 未列出 | 🔴 应删除 | 随SubnetworkId一起移除 |
| 待清理类型总数 | 10 | 8 | 减少2个 |
| `#[deprecated]` 警告 | ~50 | ~15 | 主要剩余SubnetworkId |

### 9.2 当前遗留代码分布热力图

```
consensus/core/src/subnets.rs          ████████░░  高 - SubnetworkId定义
consensus/core/src/tx/script_public_key.rs ██████░░░░  中高 - ScriptPublicKey定义
consensus/core/src/tx.rs               █████░░░░░  中 - 桥接函数
consensus/core/src/cell_metadata.rs    ████░░░░░░  中 - Placeholder函数
rpc/core/src/model/subnets.rs          ████░░░░░░  中 - RpcSubnetworkId
rpc/core/src/model/tx.rs               ███░░░░░░░  中低 - RpcScriptPublicKey
wallet/psst/src/psst.rs                ██░░░░░░░░  低 - 遗留调用
wallet/core/src/tx/generator.rs        ██░░░░░░░░  低 - 遗留调用
treasure_boy/src/lib.rs                ██░░░░░░░░  低 - 遗留调用
```

### 9.3 优先行动建议

1. **立即执行** (本周): 删除 `SubnetworkId` + `RpcSubnetworkId` + `get_subnetwork` RPC
   - 影响范围明确，依赖较少
   - 可快速减少 `#[deprecated]` 警告

2. **短期执行** (2周内): 替换 `ScriptPublicKey` 为 `ScriptRef`
   - 影响范围较大 (RPC + Wallet + Core)
   - 需要协调多模块修改

3. **中期执行** (1月内): 删除 placeholder 桥接函数
   - 依赖前两步完成
   - 清理 `cell_metadata.rs` 中的过渡代码

---

**文档维护**: Spora Team  
**审核状态**: Updated after re-audit  
**下次更新**: Phase 1 完成后
