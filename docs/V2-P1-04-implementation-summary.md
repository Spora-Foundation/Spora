# V2-P1-04: Legacy 时间锁脚本迁移到 ScriptRef + CKB-VM - 实施总结

> 更新（2026-04-12）：这份文档记录的是迁移早期阶段。当前状态已经进一步推进到：
> - `spora-txscript` crate 已从 workspace、`Cargo.lock` 和源码树删除
> - wallet / client / mining / `treasure_boy` 等生产路径已移除 direct `txscript` 依赖
> - `treasure_boy` 已删除 legacy TLC/HTLC CLI 和库接口，不再暴露“保留但必然失败”的时间锁入口

## 实施范围

本次实施完成了 V2-P1-04 的第一阶段目标：将 legacy 时间锁脚本面彻底迁移到 `ScriptRef + CKB-VM` 的准备工作。

## 已完成的修改

### 1. txscript opcodes (crypto/txscript/src/opcodes/mod.rs)

**修改内容：**
- `OpCheckLockTimeVerify` (CLTV, 0xb0): 改为始终返回 `OpcodeDisabled` 错误
- `OpCheckSequenceVerify` (CSV, 0xb1): 改为始终返回 `OpcodeDisabled` 错误

**错误信息：**
```
OP_CHECKLOCKTIMEVERIFY is disabled in Cell model. Use CKB-VM with `since` syscall instead.
OP_CHECKSEQUENCEVERIFY is disabled in Cell model. Use CKB-VM with `since` syscall instead.
```

**测试更新：**
- `test_opchecklocktimeverify_disabled_in_cell_model`: 验证 CLTV 返回 OpcodeDisabled
- `test_opchecksequenceverify_disabled_in_cell_model`: 验证 CSV 返回 OpcodeDisabled

### 2. txscript standard.rs (crypto/txscript/src/standard.rs)

**标记为 deprecated 的函数：**
- `pay_to_pub_key_with_lock_time`
- `pay_to_address_with_lock_time_script`
- `htlc_script`
- `htlc_script_ecdsa`

**弃用说明：**
```rust
#[deprecated(
    since = "0.2.0",
    note = "Legacy time lock scripts are disabled in Cell model. Use CKB-VM with `since` syscall instead."
)]
```

### 3. ScriptBuilder (crypto/txscript/src/script_builder.rs)

**标记为 deprecated 的方法：**
- `add_lock_time`
- `add_sequence`

### 4. WASM ScriptBuilder (crypto/txscript/src/wasm/builder.rs)

**标记为 deprecated 的方法：**
- `add_lock_time`
- `add_sequence`

### 5. txscript 常量 (crypto/txscript/src/lib.rs)

**标记为 deprecated 的常量：**
- `MAX_TX_IN_SEQUENCE_NUM`
- `SEQUENCE_LOCK_TIME_DISABLED`
- `SEQUENCE_LOCK_TIME_MASK`
- `LOCK_TIME_THRESHOLD`

### 6. CellTx (exec/src/celltx/types.rs)

**标记为 deprecated 的方法：**
- `lock_time()`

### 7. Wallet (wallet/core/src/tx/generator/)

**更新的文件：**
- `generator.rs`: 添加 `#[allow(deprecated)]` 和 TODO 注释
- `pending.rs`: 添加 `#[allow(deprecated)]` 和 TODO 注释

### 8. Consensus Client (consensus/client/src/utils.rs)

**标记为 deprecated 的函数：**
- `pay_to_address_with_lock_time_script`
- `htlc_script`
- `htlc_script_ecdsa`

### 9. Treasure Boy (treasure_boy/src/lib.rs)

**当前状态（2026-04-12）：**
- 早期实现曾添加迁移注释
- 现已进一步删除 legacy TLC/HTLC 库函数、CLI 参数、README 示例与集成脚本中的对应入口
- `treasure_boy` 当前只保留标准 single / batch airdrop 能力

### 10. 迁移文档 (docs/cell_model_timelock_migration.md)

创建了完整的迁移指南，包括：
- 背景和问题说明
- 迁移步骤
- CKB-VM 脚本示例
- 兼容性说明
- 时间线

## 编译状态

所有修改的包编译成功：
- ✅ `spora-txscript`
- ✅ `spora-consensus-client`
- ✅ `spora-exec`
- ✅ `spora-wallet-core`

## 警告统计

新增的 deprecated 警告帮助开发者识别需要迁移的代码：
- txscript: 26 个警告（包括新添加的 deprecated 警告）
- consensus-client: 40 个警告（主要是 legacy Transaction 类型）

## 后续工作

### Phase 2: 实现 CKB-VM 替代方案
1. 在 `exec/src/scripts/` 中实现 CKB-VM 时间锁脚本
2. 提供 `ScriptRef` 构造 helper
3. 更新 wallet/SDK 使用新方案

### Phase 3: 移除 Legacy API
1. 移除 deprecated 函数实现
2. 清理 deprecated 常量
3. 更新所有调用点

## 相关文档

- [spora_consensus_v2_issue_list.md](spora_consensus_v2_issue_list.md) - V2-P1-04 原始需求
- [spora_infra_completeness_audit.md](spora_infra_completeness_audit.md) - 基础设施审计
- [cell_model_timelock_migration.md](cell_model_timelock_migration.md) - 迁移指南

## 风险缓解

1. **资金锁死风险**: CLTV/CSV 现在返回明确错误，防止意外使用
2. **API 误用风险**: 所有 legacy API 标记为 deprecated，编译时警告
3. **迁移路径**: 提供完整文档和 TODO 注释指导后续工作

## 验收标准检查

- [x] CLTV / CSV 在 CellTx 上改为显式返回错误
- [x] 对外脚本能力的终态明确为 `ScriptRef + CKB-VM`
- [x] `pay_to_pub_key_with_lock_time`、`pay_to_address_with_lock_time_script`、`htlc_script`、`htlc_script_ecdsa` 标记为 deprecated
- [x] `consensus/client`、WASM SDK、wallet generator、`treasure_boy` 已从“添加迁移注释”推进到直接移除 legacy 公开入口
- [x] `ScriptBuilder` 中 `add_lock_time()` / `add_sequence()` 标记为 deprecated
- [x] `CellTx::lock_time()` 标记为 deprecated
- [x] txscript 中仅服务旧 opcode 的常量标记为 deprecated
- [x] 提供迁移文档和示例
