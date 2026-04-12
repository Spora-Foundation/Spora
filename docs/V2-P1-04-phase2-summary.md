# V2-P1-04 Phase 2 实施总结

## 概述

本文档总结了 V2-P1-04 "把 legacy 时间锁脚本面彻底迁移到 ScriptRef + CKB-VM" 的 Phase 2 实施工作。

## 已完成工作

### 1. CKB-VM 时间锁脚本基础结构

**文件**: `exec/src/scripts/timelock.rs`

提供了四种时间锁脚本的 ScriptRef 构造 helper：

- `absolute_timestamp_lock(target: u64)` - 绝对时间戳锁
- `relative_daa_lock(delta: u64)` - 相对 DAA 锁（区块数）
- `absolute_daa_lock(target: u64)` - 绝对 DAA 锁
- `relative_timestamp_lock(delta_seconds: u64)` - 相对时间戳锁

以及 since 编码/解码函数：
- `encode_absolute_timestamp_since(timestamp: u64) -> u64`
- `encode_relative_daa_since(delta: u64) -> u64`
- `encode_absolute_daa_since(daa_score: u64) -> u64`
- `encode_relative_timestamp_since(delta_seconds: u64) -> u64`
- `decode_since(since: u64) -> (bool, bool, u64)`

### 2. Wallet 时间锁配置模块

**文件**: `wallet/core/src/tx/timelock.rs`

提供了高级的 `TimelockConfig` 类型，支持：

```rust
pub enum TimelockConfig {
    AbsoluteTimestamp { target: u64 },
    RelativeDaa { delta: u64 },
    AbsoluteDaa { target: u64 },
    RelativeTimestamp { delta_seconds: u64 },
    None,
}
```

使用方法：
```rust
let config = TimelockConfig::absolute_timestamp(1735689600);
let since = config.encode_since();
let input = CellRef::new(outpoint, since);
```

### 3. WASM SDK 支持

**文件**: `wallet/core/src/wasm/tx/timelock.rs`

为 JavaScript/TypeScript 提供了完整的 WASM 绑定：

```javascript
// 时间锁配置
const config = TimeLockConfig.absoluteTimestamp(1735689600);
const since = config.encodeSince();

// Since 编码
const since = SinceEncoding.absoluteTimestamp(1735689600);
const decoded = SinceEncoding.decode(since);
// decoded = { isRelative: false, isTimestamp: true, value: 1735689600 }
```

### 4. C 语言脚本模板

**文件**: 
- `exec/src/scripts/timelock_absolute.c` - 绝对时间戳锁脚本模板
- `exec/src/scripts/timelock_relative.c` - 相对 DAA 锁脚本模板

这些模板展示了如何在 CKB-VM 中使用系统调用读取 `since` 字段并验证时间锁条件。

### 5. 文档更新

**文件**: `docs/cell_model_timelock_migration.md`

更新了迁移指南，包含：
- 新的 helper 函数使用示例
- Rust 和 JavaScript 代码示例
- C 语言脚本模板说明
- 迁移时间线更新

## API 参考

### Rust API

#### exec::scripts::timelock

```rust
use spora_exec::scripts::timelock;

// 创建时间锁脚本
let script = timelock::absolute_timestamp_lock(1735689600);

// 编码 since 值
let since = timelock::encode_absolute_timestamp_since(1735689600);

// 解码 since 值
let (is_relative, is_timestamp, value) = timelock::decode_since(since);
```

#### wallet::tx::timelock

```rust
use spora_wallet_core::tx::timelock::{TimelockConfig, since_encoding};

// 使用配置
let config = TimelockConfig::absolute_timestamp(1735689600);
let since = config.encode_since();

// 直接使用编码函数
let since = since_encoding::encode_absolute_timestamp_since(1735689600);
```

### JavaScript/WASM API

```javascript
import { TimeLockConfig, SinceEncoding, SinceFlags } from 'spora-wallet';

// 时间锁配置
const config = TimeLockConfig.absoluteTimestamp(1735689600);
console.log(config.isLocked()); // true
console.log(config.value());    // 1735689600
console.log(config.encodeSince()); // encoded value

// Since 编码
const since = SinceEncoding.absoluteTimestamp(1735689600);
const since = SinceEncoding.relativeDaa(100);
const since = SinceEncoding.absoluteDaa(1000000);
const since = SinceEncoding.relativeTimestamp(86400);

// Since 解码
const decoded = SinceEncoding.decode(since);
// { isRelative: false, isTimestamp: true, value: 1735689600 }

// 常量
console.log(SinceFlags.relative);     // 9223372036854775808
console.log(SinceFlags.timestamp);    // 4611686018427387904
console.log(SinceFlags.valueMask);    // 72057594037927935
```

## Since 编码规范

| 锁类型 | bit63 | bit62 | 值范围 (bits 0-55) |
|--------|-------|-------|-------------------|
| 绝对 DAA | 0 | 0 | DAA score |
| 绝对时间戳 | 0 | 1 | Unix timestamp |
| 相对 DAA | 1 | 0 | 区块数 delta |
| 相对时间戳 | 1 | 1 | 秒数 delta |

## 测试

所有时间锁模块测试通过：

```bash
$ cargo test -p spora-exec timelock
running 9 tests
test scripts::timelock::tests::test_decode_since ... ok
test scripts::timelock::tests::test_encode_absolute_timestamp_since ... ok
test scripts::timelock::tests::test_encode_relative_daa_since ... ok
test scripts::timelock::tests::test_encode_relative_timestamp_since ... ok
test scripts::timelock::tests::test_absolute_timestamp_lock ... ok
test scripts::timelock::tests::test_relative_timestamp_lock ... ok
test scripts::timelock::tests::test_relative_daa_lock ... ok
test scripts::timelock::tests::test_secp256k1_with_timelock ... ok
test scripts::timelock::tests::test_absolute_daa_lock ... ok

test result: ok. 9 passed; 0 failed; 0 ignored
```

## 下一步工作 (Phase 3)

1. **实现完整的 CKB-VM 时间锁脚本**
   - 编译 `timelock_absolute.c` 和 `timelock_relative.c` 为 RISC-V 二进制
   - 添加测试用的 ELF fixtures
   - 实现完整的 HTLC 脚本

2. **迁移 Wallet 代码**
   - 更新 `GeneratorSettings` 支持新的时间锁配置
   - 删除 `GeneratorSettings.final_transaction_lock_time` / WASM `lockTime` 兼容参数，统一改用 Cell-native `since`
   - 删除 legacy `try_sign_with_lock_time` / `signWithLockTime` 入口

3. **移除 Legacy API**
   - 继续收紧公开 API、示例和文档，统一只推荐 `ScriptRef + CKB-VM`
   - 清理残余 legacy 术语与迁移期兼容注释

## 相关文件

### 新增文件
- `exec/src/scripts/timelock.rs` (351 lines)
- `exec/src/scripts/timelock_absolute.c` (156 lines)
- `exec/src/scripts/timelock_relative.c` (147 lines)
- `wallet/core/src/tx/timelock.rs` (218 lines)
- `wallet/core/src/wasm/tx/timelock.rs` (210 lines)

### 修改文件
- `exec/src/scripts/mod.rs` - 导出 timelock 模块
- `exec/src/scripts/README.md` - 更新时间锁文档
- `wallet/core/src/tx/mod.rs` - 导出 timelock 模块
- `wallet/core/src/wasm/tx/mod.rs` - 导出 WASM timelock 模块
- `wallet/core/Cargo.toml` - 添加 spora-exec 依赖
- `docs/cell_model_timelock_migration.md` - 更新迁移指南

## 兼容性

- 所有新 API 与现有代码兼容
- Legacy API 继续保留并标记为 deprecated
- 使用 `#[allow(deprecated)]` 可继续编译现有代码
- 新代码推荐使用 `TimelockConfig` 或 `timelock` 模块

## 总结

Phase 2 成功实现了 CKB-VM 时间锁脚本的完整基础设施，包括：

1. ✅ ScriptRef 构造 helper
2. ✅ Since 编码/解码工具
3. ✅ Wallet 配置抽象
4. ✅ WASM SDK 支持
5. ✅ C 语言脚本模板
6. ✅ 完整文档和测试

开发者现在可以使用新的 API 创建时间锁交易，而不再依赖已废弃的 CLTV/CSV opcode。
