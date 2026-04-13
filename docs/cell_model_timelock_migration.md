# Cell Model 时间锁迁移指南

## 概述

本文档描述了从 legacy txscript 时间锁（CLTV/CSV）迁移到 Cell 模型的 `Script + CKB-VM + since + header_deps` 架构的过程。

## 背景

### Legacy 模型的问题

在 legacy 模型中，时间锁通过以下方式实现：
- `OP_CHECKLOCKTIMEVERIFY` (CLTV): 比较栈上值与 `tx.lock_time()`
- `OP_CHECKSEQUENCEVERIFY` (CSV): 比较栈上值与输入的 `sequence`

**Cell 模型下的问题：**
1. `CellTx::lock_time()` 恒返回 0，导致 CLTV 检查必然失败
2. CSV 对 Cell `since` 的解释错误（bit63 语义不匹配）
3. 使用这些 opcode 的脚本会导致资金永久锁死

### Cell 模型解决方案

Cell 模型使用以下机制实现时间锁：
1. **per-input `since`**: 每个输入独立的时间锁字段
2. **CKB-VM**: 通过系统调用读取 `since` 和 header timestamp
3. **`header_deps`**: 依赖区块头获取时间信息
4. **`Script`**: 与 CKB 对齐的脚本引用格式

## 迁移决策

当前仓库对时间锁迁移的执行策略已经确定为“一步到位切换”，不再接受新增兼容层：

1. 新代码一律使用 `CellInput.since + Script + CKB-VM + header_deps`
2. 不再为 `Transaction.lock_time`、legacy `sequence`、CLTV/CSV helper 增加 Cell 兼容包装
3. 旧 helper 如果暂时还没删除，其行为也只能是确定性报错，不能继续伪装成可用功能
4. 因删除旧 API 产生的编译错误，应在调用方完成迁移，而不是再补一层 `Transaction -> CellTx` 或脚本 builder shim

## 迁移步骤

### 1. 识别受影响的代码

搜索以下 legacy 函数和常量的使用：

```rust
// Legacy 函数（已标记为 deprecated）
spora_txscript::pay_to_address_with_lock_time_script
spora_txscript::pay_to_pub_key_with_lock_time
spora_txscript::htlc_script
spora_txscript::htlc_script_ecdsa
spora_txscript::ScriptBuilder::add_lock_time
spora_txscript::ScriptBuilder::add_sequence

// Legacy 常量（已标记为 deprecated）
spora_txscript::LOCK_TIME_THRESHOLD
spora_txscript::MAX_TX_IN_SEQUENCE_NUM
spora_txscript::SEQUENCE_LOCK_TIME_DISABLED
spora_txscript::SEQUENCE_LOCK_TIME_MASK
```

### 2. 迁移到 CKB-VM 方案

#### 2.1 时间锁脚本结构

**Legacy HTLC 脚本：**
```text
OP_IF
  OP_BLAKE3 <hash(secret)> OP_EQUALVERIFY
  <recipient_pubkey> OP_CHECKSIG
OP_ELSE
  <lock_time> OP_CHECKLOCKTIMEVERIFY OP_DROP
  <sender_pubkey> OP_CHECKSIG
OP_ENDIF
```

**Cell 模型 HTLC 脚本（CKB-VM）：**
```rust
// 使用 Script 定义 lock script
let lock_script = Script::new(
    code_hash,  // CKB-VM 脚本的 code hash
    hash_type,  // 哈希类型
    args,       // 参数：recipient_pubkey, sender_pubkey, secret_hash, lock_since
);
```

#### 2.2 输入构造

**Legacy:**
```rust
let input = TransactionInput {
    previous_outpoint: outpoint,
    signature_script: sig_script,
    sequence: lock_time as u64,  // Legacy sequence
    sig_op_count: 0,
};
```

**Cell 模型:**
```rust
let input = CellInput::new(
    outpoint,
    since,  // Cell 模型的 since 字段
);

// since 编码：
// - bit63 = 0: 绝对锁（DAA 或 timestamp）
// - bit63 = 1: 相对锁（DAA 或 timestamp）
// - bit62 = 0: DAA 锁
// - bit62 = 1: timestamp 锁
```

#### 2.3 使用 Helper 函数（推荐）

**Rust:**
```rust
use spora_exec::scripts::timelock;
use spora_wallet_core::tx::timelock::{TimelockConfig, since_encoding};

// 方法 1: 使用 TimelockConfig (wallet)
let config = TimelockConfig::absolute_timestamp(1735689600);
let since = config.encode_since();
let input = CellInput::new(outpoint, since);

// 方法 2: 使用 timelock 模块 (exec)
let since = timelock::encode_absolute_timestamp_since(1735689600);
let input = CellInput::new(outpoint, since);

// 创建时间锁脚本
let lock_script = timelock::absolute_timestamp_lock(1735689600);
```

**JavaScript/WASM:**
```javascript
import { TimeLockConfig, SinceEncoding } from 'spora-wallet';

// 创建时间锁配置
const config = TimeLockConfig.absoluteTimestamp(1735689600);
const since = config.encodeSince();

// 或者直接编码
const since = SinceEncoding.absoluteTimestamp(1735689600);
```

#### 2.4 交易构造

**Cell 模型完整示例：**
```rust
use spora_exec::{
    CellDep, CellOutput, CellInput, CellTx, DepType, OutPoint, Script,
};
use spora_exec::scripts::timelock;

// 创建带时间锁的输入
let target_timestamp = 1735689600u64; // 2025-01-01 00:00:00 UTC
let since = timelock::encode_absolute_timestamp_since(target_timestamp);
let input = CellInput::new(outpoint, since);

// 创建时间锁脚本
let lock_script = timelock::absolute_timestamp_lock(target_timestamp);

// 创建输出
let output = CellOutput {
    lock: lock_script,
    type_: None,
    capacity: amount,
};

// 创建交易
let tx = CellTx::new(
    vec![input],
    vec![], // deps
    vec![output],
    vec![vec![]], // outputs_data
    vec![witness], // witnesses
)?;
```

### 3. CKB-VM 脚本示例

参考 `exec/src/scripts/` 目录下的示例：

#### 3.1 时间锁脚本 Helper

```rust
use spora_exec::scripts::timelock;

// 绝对时间戳锁（Unix 时间戳）
let script = timelock::absolute_timestamp_lock(1735689600);

// 相对 DAA 锁（区块数）
let script = timelock::relative_daa_lock(100);

// 绝对 DAA 锁
let script = timelock::absolute_daa_lock(1_000_000);

// 相对时间戳锁（秒）
let script = timelock::relative_timestamp_lock(24 * 60 * 60);
```

#### 3.2 Since 编码

```rust
use spora_exec::scripts::timelock;

// 编码 since 值
let since = timelock::encode_absolute_timestamp_since(1735689600);
let since = timelock::encode_relative_daa_since(100);

// 解码 since 值
let (is_relative, is_timestamp, value) = timelock::decode_since(since);
```

#### 3.3 C 语言脚本模板

参考 `exec/src/scripts/timelock_absolute.c` 和 `timelock_relative.c`：

```c
// 系统调用示例：
// - 读取 since: LOAD_INPUT_BY_FIELD_SYSCALL with FIELD_SINCE
// - 读取 header timestamp: LOAD_HEADER_BY_FIELD_SYSCALL

int main() {
    // 1. 加载 since 字段
    uint8_t since_buf[8];
    load_input_by_field(since_buf, &since_len, 0, 0, SOURCE_GROUP_INPUT, FIELD_SINCE);
    
    // 2. 解析 since 值
    uint64_t since = parse_le64(since_buf);
    
    // 3. 验证时间锁条件
    if (!verify_timelock(since, target_value)) {
        return 1; // 验证失败
    }
    
    return 0; // 成功
}
```

### 4. 验证时间锁

Cell 模型的时间锁验证在共识层完成：

```rust
// consensus/src/processes/cell_validator/cell_validation_in_dag.rs
// validate_time_locks 函数验证四类时间锁：
// - 绝对 DAA 锁
// - 相对 DAA 锁
// - 绝对 timestamp 锁
// - 相对 timestamp 锁
```

## 删除策略

### 待删除 API

以下 API 已标记为 deprecated。它们现在的定位是“待删除入口”，而不是“长期兼容能力”：

| 函数/常量 | 替代方案 |
|-----------|----------|
| `pay_to_address_with_lock_time_script` | CKB-VM 脚本 + `since` |
| `pay_to_pub_key_with_lock_time` | CKB-VM 脚本 + `since` |
| `htlc_script` | CKB-VM HTLC 脚本 |
| `htlc_script_ecdsa` | CKB-VM HTLC 脚本 |
| `ScriptBuilder::add_lock_time` | 不再使用 |
| `ScriptBuilder::add_sequence` | 不再使用 |
| `LOCK_TIME_THRESHOLD` | Cell `since` 编码 |
| `CellTx::lock_time()` | Cell `since` 字段 |

### 执行要求

1. 钱包、SDK、client、脚本 builder、示例代码不再新增对这些旧 API 的依赖
2. 旧 API 的删除不以“先保留薄兼容层”为前提；需要迁移时，直接修调用方
3. 若某个公开入口暂时保留，只允许：
   - 明确标记 deprecated
   - 文档写明不可用于 Cell 模型正式时间锁
   - 运行时稳定返回“Cell 模型不支持”的错误
4. 不允许继续提供“看起来成功、实际上会锁死资金”的 helper

## 测试

### 回归测试

确保以下测试通过：
1. `validate_time_locks` 继续覆盖四类 `since` 语义
2. 旧 helper 若仍保留，会稳定报错而不是生成"看似成功、实则不可花费"的脚本
3. wallet / SDK 不再能构造 CLTV/CSV 风格的 Cell 时间锁输出

### 新测试

添加 CKB-VM 时间锁脚本的测试：
```rust
#[test]
fn test_ckb_vm_timelock_script() {
    // 测试 CKB-VM 脚本能正确验证 since
}
```

## 参考文档

- [spora_consensus_v2_issue_list.md](spora_consensus_v2_issue_list.md) - V2-P1-04 详细说明
- [spora_infra_completeness_audit.md](spora_infra_completeness_audit.md) - 基础设施完备性审计
- `consensus/src/processes/cell_validator/cell_validation_in_dag.rs` - 时间锁验证实现
- `exec/src/scripts/` - CKB-VM 脚本示例

## 执行顺序

1. 统一文档与公开接口：明确 Cell 模型的规范时间锁只有 `since + Script + CKB-VM`
2. 迁移 wallet / SDK / client / 示例代码到 Cell-native 路径
3. 删除 CLTV / CSV helper、builder 包装层和遗留常量
4. 删除仍暴露错误语义的 `lock_time` / legacy `sequence` 相关入口

## 问题反馈

如有迁移问题，请参考：
- 技术文档：`docs/consensus/`
- 代码示例：`exec/src/scripts/`
- 测试用例：`consensus/src/processes/cell_validator/`
