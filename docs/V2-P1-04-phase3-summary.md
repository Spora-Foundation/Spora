# V2-P1-04 Phase 3 实施总结

## 概述

本文档总结了 V2-P1-04 "把 legacy 时间锁脚本面彻底迁移到 ScriptRef + CKB-VM" 的 Phase 3 实施工作。

## 已完成工作

### 1. RISC-V ELF Fixtures

成功编译了两个 CKB-VM 时间锁脚本为 RISC-V ELF 二进制文件：

#### 1.1 Absolute Timestamp Lock

**文件**: 
- Source: `exec/src/scripts/fixtures/timelock_absolute.rs`
- Binary: `exec/src/scripts/fixtures/timelock_absolute.elf` (1472 bytes)

**功能**:
- 验证输入的 `since` 字段（作为绝对时间戳）>= 目标时间戳
- 目标时间戳: 2025-01-01 00:00:00 UTC (1735689600)
- 检查 since 格式: bit63=0 (绝对), bit62=1 (时间戳)

**系统调用**:
- `LOAD_INPUT_BY_FIELD` (2083) - 读取输入的 since 字段

#### 1.2 Relative DAA Lock

**文件**:
- Source: `exec/src/scripts/fixtures/timelock_relative.rs`
- Binary: `exec/src/scripts/fixtures/timelock_relative.elf` (1440 bytes)

**功能**:
- 验证输入的 `since` 字段（作为相对 DAA）>= 目标 delta
- 目标 delta: 100 个区块
- 检查 since 格式: bit63=1 (相对), bit62=0 (DAA)

**系统调用**:
- `LOAD_INPUT_BY_FIELD` (2083) - 读取输入的 since 字段

### 2. 测试覆盖

创建了完整的测试套件，共 21 个测试全部通过：

#### 2.1 基础测试 (11 tests)
- `test_absolute_timestamp_lock`
- `test_relative_daa_lock`
- `test_absolute_daa_lock`
- `test_relative_timestamp_lock`
- `test_encode_absolute_timestamp_since`
- `test_encode_relative_daa_since`
- `test_encode_relative_timestamp_since`
- `test_decode_since`
- `test_secp256k1_with_timelock`
- `test_timelock_absolute_script_size`
- `test_timelock_relative_script_size`

#### 2.2 CKB-VM 集成测试 (10 tests)

**Absolute Lock Tests** (5 tests):
- `test_timelock_absolute_accepts_valid_timestamp` - 接受有效时间戳
- `test_timelock_absolute_accepts_future_timestamp` - 接受未来时间戳
- `test_timelock_absolute_rejects_past_timestamp` - 拒绝过去时间戳
- `test_timelock_absolute_rejects_relative_lock` - 拒绝相对锁
- `test_timelock_absolute_rejects_daa_lock` - 拒绝 DAA 锁

**Relative Lock Tests** (5 tests):
- `test_timelock_relative_accepts_valid_delta` - 接受有效 delta
- `test_timelock_relative_accepts_larger_delta` - 接受更大的 delta
- `test_timelock_relative_rejects_small_delta` - 拒绝过小的 delta
- `test_timelock_relative_rejects_absolute_lock` - 拒绝绝对锁
- `test_timelock_relative_rejects_timestamp_lock` - 拒绝时间戳锁

### 3. 构建工具

创建了构建脚本 `exec/src/scripts/fixtures/build_fixtures.sh`：

```bash
# 编译所有 fixtures
./build_fixtures.sh

# 输出 code hashes
blake3sum *.elf
```

### 4. API 导出

在 `exec/src/scripts/mod.rs` 中导出了新的 fixtures：

```rust
pub const TIMELOCK_ABSOLUTE_SCRIPT: &[u8] = include_bytes!("fixtures/timelock_absolute.elf");
pub fn timelock_absolute_code_hash() -> [u8; 32];

pub const TIMELOCK_RELATIVE_SCRIPT: &[u8] = include_bytes!("fixtures/timelock_relative.elf");
pub fn timelock_relative_code_hash() -> [u8; 32];
```

## 测试结果

```bash
$ cargo test -p spora-exec timelock
running 21 tests
test scripts::tests::test_timelock_relative_script_size ... ok
test scripts::timelock::tests::test_encode_absolute_timestamp_since ... ok
test scripts::timelock::tests::test_decode_since ... ok
test scripts::tests::test_timelock_absolute_script_size ... ok
...
test scripts::timelock_absolute_test::tests::test_timelock_absolute_accepts_valid_timestamp ... ok
test scripts::timelock_relative_test::tests::test_timelock_relative_accepts_valid_delta ... ok
...

test result: ok. 21 passed; 0 failed; 0 ignored
```

## 技术细节

### RISC-V 编译

使用 Rust 编译为 RISC-V 目标：

```bash
rustc --edition 2021 \
    --target riscv64imac-unknown-none-elf \
    -C opt-level=3 \
    -C link-arg=--image-base=0x0 \
    -o timelock_absolute.elf \
    timelock_absolute.rs
```

### 系统调用接口

脚本使用 CKB-VM 系统调用 2083 (`LOAD_INPUT_BY_FIELD`)：

```rust
// 读取 since 字段
asm!(
    "ecall",
    inlateout("a0") buf.as_mut_ptr() as usize => ret,
    in("a1") (&mut size as *mut u64) as usize,
    in("a2") 0usize,      // offset
    in("a3") 0usize,      // index
    in("a4") 0x0100usize, // source: GROUP_INPUT
    in("a5") 0x01usize,   // field: SINCE
    in("a7") 2083usize,   // syscall: LOAD_INPUT_BY_FIELD
);
```

### Since 编码验证

脚本验证 since 值的标志位：

```rust
// Absolute timestamp lock
let is_relative = (since & (1u64 << 63)) != 0;  // must be 0
let is_timestamp = (since & (1u64 << 62)) != 0; // must be 1

// Relative DAA lock
let is_relative = (since & (1u64 << 63)) != 0;  // must be 1
let is_timestamp = (since & (1u64 << 62)) != 0; // must be 0
```

## 新增文件

### Source Files
- `exec/src/scripts/fixtures/timelock_absolute.rs` (98 lines)
- `exec/src/scripts/fixtures/timelock_relative.rs` (94 lines)
- `exec/src/scripts/fixtures/build_fixtures.sh` (104 lines)
- `exec/src/scripts/timelock_absolute_test.rs` (155 lines)
- `exec/src/scripts/timelock_relative_test.rs` (155 lines)

### Binary Files
- `exec/src/scripts/fixtures/timelock_absolute.elf` (1472 bytes)
- `exec/src/scripts/fixtures/timelock_relative.elf` (1440 bytes)

### Documentation
- `docs/V2-P1-04-phase3-summary.md` (本文件)

## 修改文件

- `exec/src/scripts/mod.rs` - 导出新的 fixtures 和测试模块
- `exec/src/scripts/README.md` - 更新时间锁 fixtures 文档

## 下一步工作 (Phase 4)

1. **实现完整的 HTLC 脚本**
   - 结合签名验证和时间锁验证
   - 支持 preimage 揭示路径
   - 支持 refund 路径

2. **优化时间锁脚本**
   - 支持通过 script args 传递目标值（当前是硬编码的）
   - 支持 header deps 验证

3. **Wallet 集成**
   - 更新 `GeneratorSettings` 支持新的时间锁配置
   - 删除 `GeneratorSettings.final_transaction_lock_time` / WASM `lockTime` 兼容参数，统一改用 Cell-native `since`

4. **文档完善**
   - 添加更多使用示例
   - 创建 JavaScript/TypeScript 示例

## 兼容性

- 所有新 fixtures 与现有 CKB-VM 测试框架兼容
- 使用 `TransactionScriptVerifier` 进行验证
- 支持 `ScriptVersion::V2`

## 总结

Phase 3 成功实现了完整的 CKB-VM 时间锁脚本基础设施：

1. ✅ 编译了 RISC-V ELF fixtures
2. ✅ 实现了完整的时间锁验证逻辑
3. ✅ 添加了全面的测试覆盖（21个测试）
4. ✅ 更新了文档

这些 fixtures 证明了 CKB-VM 时间锁方案的可行性，为 Phase 4 的完整 HTLC 实现奠定了基础。
