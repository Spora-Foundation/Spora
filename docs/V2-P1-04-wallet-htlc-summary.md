# V2-P1-04 Wallet HTLC 集成总结

## 概述

本文档总结了 V2-P1-04 的 Wallet HTLC 集成工作，提供了完整的 Rust 和 JavaScript API 用于创建和支出 HTLC 合约。

## 已完成工作

### 1. Wallet HTLC Core Module

**文件**: `wallet/core/src/tx/htlc.rs` (287 lines)

提供了完整的 HTLC 配置和 witness 构造功能：

#### 1.1 HtlcLockType

支持四种锁类型：

```rust
pub enum HtlcLockType {
    AbsoluteDaa { target: u64 },
    AbsoluteTimestamp { target: u64 },
    RelativeDaa { delta: u64 },
    RelativeTimestamp { delta_seconds: u64 },
}
```

#### 1.2 HtlcConfig

HTLC 配置结构：

```rust
pub struct HtlcConfig {
    pub secret_hash: [u8; 32],
    pub recipient_pubkey: [u8; 32],
    pub sender_pubkey: [u8; 32],
    pub lock: HtlcLockType,
}
```

主要方法：
- `build_script_args()` - 构建 105 字节的脚本参数
- `create_script_ref()` - 创建 ScriptRef
- `encode_since()` - 编码 since 值

#### 1.3 HtlcWitness

Witness 构造器：

```rust
impl HtlcWitness {
    pub fn recipient(signature: [u8; 64], secret: [u8; 32]) -> Vec<u8>;
    pub fn sender_timeout(signature: [u8; 64]) -> Vec<u8>;
}
```

#### 1.4 工具函数

- `compute_secret_hash(secret: &[u8]) -> [u8; 32]`
- `verify_secret(secret: &[u8], expected_hash: &[u8; 32]) -> bool`

### 2. WASM HTLC Module

**文件**: `wallet/core/src/wasm/tx/htlc.rs` (245 lines)

提供了 JavaScript/TypeScript 友好的 API：

#### 2.1 JavaScript API

```javascript
import { HtlcConfig, HtlcLockType, HtlcWitness, computeSecretHash, verifySecret } from 'spora-wallet';

// Create lock type
const lockType = HtlcLockType.absoluteTimestamp(1735689600);

// Create HTLC config
const config = new HtlcConfig(
    secretHash,      // Uint8Array (32 bytes)
    recipientPubkey, // Uint8Array (32 bytes)
    senderPubkey,    // Uint8Array (32 bytes)
    lockType
);

// Build script args
const args = config.buildScriptArgs(); // Uint8Array (105 bytes)

// Encode since value
const since = config.encodeSince(); // u64

// Build witness
const recipientWitness = HtlcWitness.recipient(signature, secret); // Uint8Array (97 bytes)
const senderWitness = HtlcWitness.senderTimeout(signature);        // Uint8Array (65 bytes)

// Compute and verify secret hash
const hash = computeSecretHash(secret);
const isValid = verifySecret(secret, hash);
```

### 3. 测试覆盖

#### 3.1 Core Module Tests (9 tests)

- `test_htlc_lock_type_to_u8` - 锁类型编码
- `test_htlc_lock_type_value` - 锁值获取
- `test_htlc_config_build_script_args` - 脚本参数构建
- `test_htlc_witness_recipient` - 接收方 witness
- `test_htlc_witness_sender_timeout` - 发送方 witness
- `test_compute_secret_hash` - secret hash 计算
- `test_verify_secret` - secret 验证
- `test_htlc_config_encode_since` - since 编码

#### 3.2 WASM Module Tests (4 tests)

- `test_wasm_htlc_lock_type`
- `test_wasm_htlc_config`
- `test_wasm_htlc_witness`
- `test_wasm_compute_secret_hash`

## API 使用示例

### Rust

```rust
use spora_wallet_core::tx::htlc::{HtlcConfig, HtlcLockType, HtlcWitness, compute_secret_hash};
use spora_exec::celltx::{CellOut, CellRef, ScriptRef};
use spora_exec::scripts::htlc_code_hash;

// Generate secret
let secret = [0xABu8; 32];
let secret_hash = compute_secret_hash(&secret);

// Create HTLC config
let config = HtlcConfig::new(
    secret_hash,
    recipient_pubkey,
    sender_pubkey,
    HtlcLockType::AbsoluteTimestamp { target: 1735689600 },
);

// Create output with HTLC lock
let output = CellOut {
    lock: config.create_script_ref(),
    type_: None,
    capacity: amount,
};

// Create input with timelock
let since = config.encode_since();
let input = CellRef::new(outpoint, since);

// Build transaction
let tx = CellTx::new(
    vec![input],
    vec![],
    vec![output],
    vec![vec![]],
    vec![],
)?;

// Later: spend as recipient
let witness = HtlcWitness::recipient(signature, secret);

// Or: spend as sender after timeout
let witness = HtlcWitness::sender_timeout(signature);
```

### JavaScript

```javascript
import { HtlcConfig, HtlcLockType, HtlcWitness, computeSecretHash } from 'spora-wallet';

// Generate random secret
const secret = crypto.getRandomValues(new Uint8Array(32));
const secretHash = computeSecretHash(secret);

// Create HTLC
const lockType = HtlcLockType.absoluteTimestamp(1735689600);
const config = new HtlcConfig(
    secretHash,
    recipientPubkey,
    senderPubkey,
    lockType
);

// Get script args for ScriptRef
const scriptArgs = config.buildScriptArgs();

// Get since value for input
const since = config.encodeSince();

// Create transaction (using other APIs)
// ...

// Spend as recipient
const recipientWitness = HtlcWitness.recipient(signature, secret);

// Or spend as sender after timeout
const senderWitness = HtlcWitness.senderTimeout(signature);
```

## 与 Legacy HTLC 对比

| 特性 | Legacy (txscript) | Cell Model (CKB-VM) |
|------|-------------------|---------------------|
| 创建方式 | `htlc_script()` helper | `HtlcConfig::create_script_ref()` |
| 脚本格式 | txscript opcodes | RISC-V ELF binary |
| Witness 构造 | `htlc_signature_script_with_secret()` | `HtlcWitness::recipient()` |
| 时间锁 | `lock_time` parameter | `since` field + `TimelockConfig` |
| 签名算法 | Schnorr/ECDSA | Schnorr (TODO) |

## 新增文件

| 文件 | 大小 | 描述 |
|------|------|------|
| `wallet/core/src/tx/htlc.rs` | 287 lines | Core HTLC module |
| `wallet/core/src/wasm/tx/htlc.rs` | 245 lines | WASM bindings |
| `docs/V2-P1-04-wallet-htlc-summary.md` | - | 本文档 |

## 修改文件

- `wallet/core/src/tx/mod.rs` - 导出 htlc 模块
- `wallet/core/src/wasm/tx/mod.rs` - 导出 wasm htlc 模块

## 依赖关系

```
wallet/core/src/tx/htlc.rs
    ├── spora_exec::scripts::htlc_code_hash
    ├── spora_exec::celltx::ScriptRef
    └── crate::tx::timelock::TimelockConfig

wallet/core/src/wasm/tx/htlc.rs
    └── crate::tx::htlc (native)
```

## 编译验证

```bash
$ cargo check -p spora-wallet-core
    Finished dev profile [unoptimized + debuginfo] target(s)
```

## 测试运行

```bash
# Core module tests
$ cargo test -p spora-wallet-core htlc

# All tests
$ cargo test -p spora-wallet-core
```

## 已知限制

1. **HTLC 脚本调试中**: CKB-VM 脚本有 2 个测试失败，需要进一步调试系统调用
2. **签名验证未实现**: 当前使用占位符，需要集成 secp256k1
3. **随机 secret 生成**: 需要 `rand` feature

## 下一步工作

1. 完成 HTLC 脚本调试
2. 实现完整的 secp256k1 签名验证
3. 添加更多使用示例
4. 创建端到端集成测试
5. 更新用户文档

## 总结

Wallet HTLC 集成提供了完整的 API 支持：

1. ✅ Core module (Rust) - 完整的配置和 witness 构造
2. ✅ WASM module (JavaScript) - 完整的 JS/TS API
3. ✅ 测试覆盖 - 13 个测试
4. ✅ 文档 - 使用示例和 API 文档

开发者现在可以使用这些 API 创建 HTLC 合约，待 CKB-VM 脚本调试完成后即可进行实际交易。
