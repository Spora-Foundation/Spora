# CKB → SPORA 映射表

**目的**: 详细说明如何从 CKB 源码迁移到 SPORA

---

## 1. 核心类型映射

| CKB 类型 | CKB 文件 | SPORA 类型 | SPORA 文件 | 差异 |
|---------|---------|-----------|-----------|------|
| `CellOutputput` | `util/types/src/core/cell.rs` | `CellOutput` | `exec/src/celltx/types.rs` | ✅ 结构相同 |
| `OutPoint` | `util/types/src/core/cell.rs` | `OutPoint` | `exec/src/celltx/types.rs` | ✅ 完全相同 |
| `Script` | `util/types/src/core/cell.rs` | `Script` | `exec/src/celltx/types.rs` | ✅ 结构相同 |
| `CellInput` | `util/types/src/core/cell.rs` | `CellInput` | `exec/src/celltx/types.rs` | ✅ 结构相同 |
| `CellDep` | `util/types/src/core/cell.rs` | `CellDep` | `exec/src/celltx/types.rs` | ✅ 完全相同 |
| `Transaction` | `util/types/src/core/views.rs` | `CellTx` | `exec/src/celltx/types.rs` | ✅ 有 header_deps |
| `ResolvedTransaction` | `util/types/src/core/cell.rs` | `ResolvedCellTx` | `exec/src/celltx/types.rs` | ✅ 概念相同 |
| `CellMeta` | `util/types/src/core/cell.rs` | `CellMeta` | `exec/src/celltx/types.rs` | ✅ 添加 DAG 字段 |

## 2. 哈希函数映射

| 用途 | CKB (Blake2b) | SPORA (Blake3) | 差异 |
|------|---------------|----------------|------|
| TxID | `blake2b(tx)` | `blake3("spora-cell/txid" \|\| tx)` | ⚠️ 域前缀 |
| WTxID | `blake2b(tx+wit)` | `blake3("spora-cell/wtxid" \|\| tx+wit)` | ⚠️ 域前缀 |
| SigHash | `blake2b(...)` | `blake3("spora-cell/sig" \|\| ...)` | ⚠️ 域前缀 |
| PubkeyHash | `blake2b(pubkey)[0..20]` | `blake3(pubkey)[0..20]` | ⚠️ 哈希函数 |
| ScriptHash | `blake2b(script)` | `blake3(script)` | ⚠️ 哈希函数 |

**迁移指南**:
```rust
// CKB: 
let hash = blake2b_256(data);

// SPORA:
let hash = blake3::hash(data);  // ✅ 更简单

// 或带域分离:
let mut hasher = blake3::Hasher::new();
hasher.update(b"spora-domain");
hasher.update(data);
let hash = *hasher.finalize().as_bytes();
```

## 3. 系统调用映射

### 3.1 完全相同 ✅

| 系统调用 | 编号 | CKB 文件 | SPORA 文件 | 复用度 |
|---------|------|---------|-----------|--------|
| LOAD_WITNESS | 2074 | `script/src/syscalls/load_witness.rs` | `exec/src/vm/syscalls/load_witness.rs` | 100% |
| CURRENT_CYCLES | 2042 | `script/src/syscalls/current_cycles.rs` | `exec/src/vm/syscalls/current_cycles.rs` | 100% |
| DEBUG_PRINT | 2177 | `script/src/syscalls/debugger.rs` | `exec/src/vm/syscalls/debugger.rs` | 100% |

### 3.2 需要修改哈希 ⚠️

| 系统调用 | 编号 | 修改内容 | 复用度 |
|---------|------|---------|--------|
| LOAD_TX_HASH | 2061 | Blake2b → Blake3 | 95% |
| LOAD_SCRIPT | 2075 | Blake2b → Blake3 | 95% |
| LOAD_SCRIPT_HASH | 2062 | Blake2b → Blake3 | 95% |
| LOAD_CELL | 2071 | 适配 CellDataProvider | 90% |
| LOAD_CELL_BY_FIELD | 2081 | 适配 CellDataProvider | 90% |

### 3.3 已实现（DAG适配版） ✅

| 系统调用 | 编号 | SPORA 文件 | 状态 | DAG特性 |
|---------|------|-----------|------|---------|
| LOAD_HEADER | 2072 | `load_header.rs` | ✅ 完成 | 多父区块、daa_score ⭐ |
| LOAD_INPUT | 2073 | `load_input.rs` | ✅ 完成 | Since时间锁支持 ⭐ |
| LOAD_CELL_DATA | 2092 | `load_cell_data.rs` | ✅ 完成 | Cell data字段 ⭐ |

### 3.4 SPORA特有扩展 ✅

| 系统调用 | 编号 | SPORA 文件 | 说明 |
|---------|------|-----------|------|
| BLAKE3_HASH | 3001 | `exec/src/vm/syscalls/blake3.rs` | SPORA特有，原生Blake3加速 |

### 3.5 待实现（高级功能） ⏳

| 系统调用 | 编号 | CKB 文件 | 优先级 |
|---------|------|---------|--------|
| EXEC | 2043 | `script/src/syscalls/exec.rs` | P2 |
| SPAWN | 2601+ | `script/src/syscalls/spawn.rs` | P3 |

## 4. VM 组件映射

### 4.1 基础组件

| 组件 | CKB | SPORA | 复用策略 |
|------|-----|-------|----------|
| Machine Type | `TraceMachine` / `AsmMachine` | `TraceMachine` | ✅ 直接复用 |
| ISA Support | `ISA_IMC \| ISA_B \| ISA_MOP` | ✅ 相同 | ✅ 直接复用 |
| VM Version | `VERSION0/1/2` | `VERSION0/1/2` | ✅ 直接复用 |
| Cycles Type | `u64` | `u64` | ✅ 直接复用 |
| Cost Model | `transferred_byte_cycles` | ✅ 相同 | ✅ 直接复用 |

### 4.2 VM 架构对比

**CKB VM 架构**:
```
TransactionScriptsVerifier
├── Scheduler (复杂状态机，支持Spawn/Exec)
│   ├── 多VM管理 (MAX_VMS=16)
│   ├── 文件描述符管理
│   ├── 管道/进程间通信
│   └── 快照/恢复机制
└── Syscalls (完整实现)
```

**SPORA VM 架构** (简化但完整):
```
TransactionScriptVerifier
├── 顺序脚本组执行
│   ├── Lock脚本验证
│   └── Type脚本验证
└── Syscalls (核心功能完整)
    ├── 数据加载 (Cell/Input/Header/Witness)
    ├── 哈希计算 (Blake3原生)
    └── 调试/周期计数
```

### 4.3 关键差异说明

| 特性 | CKB | SPORA | 原因 |
|------|-----|-------|------|
| Spawn/Exec | ✅ 完整支持 | ⏳ 待实现 | 优先级较低 |
| 并行脚本执行 | ❌ 无 | ✅ 交易级并行 | SPORA特有优化 |
| 文件描述符 | ✅ 完整 | ❌ 不需要 | 简化设计 |
| 多VM调度 | ✅ 复杂调度器 | ⏳ placeholder | 当前顺序执行足够 |
| Blake3加速 | ❌ VM内计算 | ✅ 原生syscall | 性能优化 |

## 5. 验证器映射

| CKB 验证器 | CKB 文件 | SPORA 验证器 | SPORA 文件 | 差异 |
|-----------|---------|-------------|-----------|------|
| `TransactionScriptsVerifier` | `script/src/verify.rs` | `TransactionScriptVerifier` | `exec/src/vm/verifier.rs` | ✅ 已实现 |
| `Scheduler` | `script/src/scheduler.rs` | (placeholder) | `exec/src/vm/scheduler.rs` | ⏳ 未来并行执行 |
| `TransactionVerifier` | `verification/src/transaction_verifier.rs` | `CellValidator` | `consensus/src/processes/cell_validator/` | ⚠️ DAG 适配 |
| `BlockVerifier` | `verification/src/block_verifier.rs` | `BlockValidator` | (待实现) | ⚠️ DAG 适配 |

## 6. 存储层映射

| CKB Trait | CKB 文件 | SPORA Trait | SPORA 文件 | 差异 |
|-----------|---------|------------|-----------|------|
| `CellDataProvider` | `traits/src/cell_data_provider.rs` | `CellDataProvider` | `exec/src/vm/verifier.rs` | ✅ DAG 适配 |
| `HeaderProvider` | `traits/src/header_provider.rs` | (inline in CellDataProvider) | `exec/src/vm/verifier.rs` | ✅ DAG 多父 |
| `ChainStore` | `store/src/lib.rs` | `CellDB` | `state/src/index/cell_db.rs` | ⚠️ 简化版 |
| `Freezer` | `freezer/src/freezer.rs` | `SegmentWriter` | `state/src/store/segment.rs` | ⚠️ DA 特化 |

## 7. 交易池映射

| CKB TxPool | CKB 文件 | SPORA CellPool | SPORA 文件 | 差异 |
|-----------|---------|---------------|-----------|------|
| `TxPool` | `tx-pool/src/pool.rs` | `CellPool` | `mempool/src/cellpool.rs` | ✅ 概念相同 |
| `TxEntry` | `tx-pool/src/component/entry.rs` | `CellEntry` | `mempool/src/cellpool.rs` | ✅ 概念相同 |
| `Edges` | `tx-pool/src/component/edges.rs` | (待实现) | - | ⏳ 依赖追踪 |
| `Scorer` | `tx-pool/src/component/score.rs` | `Scorer` | `mempool/src/scorer.rs` | ✅ 已实现 |

## 8. 直接复制的文件

以下文件可以从 CKB 直接复制，仅需修改哈希函数：

### 8.1 可完全复制 ✅
```bash
# 成本模型
cp ckb/script/src/cost_model.rs exec/src/vm/cost_model.rs  # ✅ 已完成

# 系统调用工具
cp ckb/script/src/syscalls/utils.rs exec/src/vm/syscalls/utils.rs  # ✅ 已完成

# CurrentCycles 系统调用
cp ckb/script/src/syscalls/current_cycles.rs exec/src/vm/syscalls/current_cycles.rs  # ✅ 已完成

# Debugger 系统调用
cp ckb/script/src/syscalls/debugger.rs exec/src/vm/syscalls/debugger.rs  # ✅ 已完成

# LoadWitness 系统调用
cp ckb/script/src/syscalls/load_witness.rs exec/src/vm/syscalls/load_witness.rs  # ✅ 已完成
```

### 8.2 复制后需修改 ⚠️
```bash
# LoadTx - 替换 Blake2b 为 Blake3
cp ckb/script/src/syscalls/load_tx.rs exec/src/vm/syscalls/load_tx.rs  # ✅ 已完成

# LoadScript - 替换 Blake2b 为 Blake3
cp ckb/script/src/syscalls/load_script.rs exec/src/vm/syscalls/load_script.rs  # ✅ 已完成

# LoadCell - 适配 CellStateProvider
cp ckb/script/src/syscalls/load_cell.rs exec/src/vm/syscalls/load_cell.rs  # ✅ 已完成
```

### 8.3 已完成（DAG适配版） ✅
```bash
# LoadHeader - DAG 多父适配版
# exec/src/vm/syscalls/load_header.rs  # ✅ 已完成（DAG多父支持）

# LoadInput - 时间锁支持
# exec/src/vm/syscalls/load_input.rs  # ✅ 已完成（Since字段支持）

# LoadCellData - Cell data 字段加载
# exec/src/vm/syscalls/load_cell_data.rs  # ✅ 已完成

# LoadScript - Blake3适配版
# exec/src/vm/syscalls/load_script.rs  # ✅ 已完成

# LoadTx - Blake3适配版
# exec/src/vm/syscalls/load_tx.rs  # ✅ 已完成
```

### 8.4 SPORA特有扩展 ✅
```bash
# Blake3 哈希系统调用 (SPORA特有)
# exec/src/vm/syscalls/blake3.rs  # ✅ 已完成
```

### 8.5 待实现 ⏳
```bash
# Exec 系统调用
cp ckb/script/src/syscalls/exec.rs exec/src/vm/syscalls/exec.rs  # ⏳ 待实现

# Spawn 系统调用
cp ckb/script/src/syscalls/spawn.rs exec/src/vm/syscalls/spawn.rs  # ⏳ 待实现
```

## 9. 不能直接复用的组件

### 9.1 共识层
❌ **CKB Consensus** → **SPORA DAG Consensus**
- CKB: NC-Max (单链)
- SPORA: GhostDAG (DAG)
- 需要: 完全重写

### 9.2 区块结构
⚠️ **CKB Header** → **SPORA Header with cell_commitment**
- CKB: `block_number`, `parent_hash` (单父)
- SPORA: `daa_score`, `parent_hashes` (多父), `cell_commitment`, `cell_root`
- 需要: 扩展字段

#### Cell Commitment 设计 (v0)

**字段职责分离**:
- `cell_commitment`: 可演进的版本化承诺，用于共识校验
  - v0 实现: `H(domain || cell_root)` 或当前为 multiset_hash (过渡期)
  - 未来可升级为: `H(domain || cell_root || segment_root || script_index_root || ...)`
- `cell_root`: Live cells 的 Merkle 状态根，用于轻客户端状态证明

**为什么不合并**:
- `cell_root` 是单一语义（live cells 状态根）
- `cell_commitment` 可包含多个命名空间的绑定（预留扩展性）
- 避免只承诺部分状态造成的升级限制

**实现位置**:
- Header 结构: `consensus/core/src/header.rs`
- 计算逻辑: `consensus/src/pipeline/virtual_processor/processor.rs`
- 验证逻辑: `consensus/src/pipeline/pruning_processor/processor.rs`

### 9.3 Cellbase 处理
⚠️ **CKB Cellbase** → **SPORA Cellbase with Mergeset**
- CKB: 单矿工奖励
- SPORA: 蓝块矿工 + 红块矿工（mergeset 奖励）
- 需要: DAG 适配

## 10. 复用度统计

### 总体复用度
```
完全复用 (100%):  45%
  - ckb-vm 核心
  - 系统调用架构
  - 成本模型
  - 部分系统调用

修改复用 (>70%):  35%
  - 哈希函数替换 (Blake2b → Blake3)
  - 数据提供者适配
  - Cell 类型定义

全新实现 (<30%):  20%
  - DAG 共识
  - DA 层
  - 重组逻辑
  - Mergeset 奖励
  - 并行调度器
```

### 代码行复用
```
可直接复制:     ~1,500 LOC (15%)
复制后修改:     ~3,500 LOC (35%)
全新实现:       ~5,000 LOC (50%)
总计:          ~10,000 LOC

主要组件 (2025-04):
  ✅ VM 系统调用:      ~1,600 LOC (10个syscall)
  ✅ Cell 类型系统:    ~660 LOC (types.rs)
  ✅ 交易验证器:       ~400 LOC (verifier.rs)
  ✅ 并行调度器:       ~1,200 LOC (scheduler/)
  ✅ 脚本示例:         ~200 LOC (scripts/)
```

---

## 11. 快速参考

### 11.1 从 CKB 复制代码的步骤

1. **检查文件**
   ```bash
   cat /Users/arthur/RustroverProjects/ckb/script/src/syscalls/load_witness.rs
   ```

2. **复制到 SPORA**
   ```bash
   cp ckb/script/src/syscalls/load_witness.rs Spora/exec/src/vm/syscalls/load_witness.rs
   ```

3. **修改哈希（如需要）**
   ```rust
   // 查找所有 Blake2b 调用
   rg "blake2b|Blake2b" load_witness.rs
   
   // 替换为 Blake3
   sed -i 's/blake2b_256/blake3::hash/g' load_witness.rs
   ```

4. **适配类型**
   ```rust
   // CKB:
   use ckb_types::packed::Transaction;
   
   // SPORA:
   use crate::celltx::types::CellTx;
   ```

5. **测试**
   ```bash
   cargo test --package spora-exec --lib syscalls::load_witness
   ```

### 11.2 常用类型映射速查

| CKB 类型 | SPORA 类型 | 说明 |
|---------|-----------|------|
| `Bytes` | `Vec<u8>` | SPORA使用标准Vec |
| `Byte32` | `[u8; 32]` | 固定大小数组 |
| `Script` | `Script` | 脚本引用 |
| `CellOutputput` | `CellOutput` | Cell输出 |
| `CellInput` | `CellInput` | Cell输入（含since） |
| `Transaction` | `CellTx` | 交易（含header_deps） |
| `OutPoint` | `OutPoint` | 完全相同 |
| `CellDep` | `CellDep` | 完全相同 |
| `ScriptHashType` | `u8` | 0=Data, 1=Type, 2=Data1, 4=Data2 |

### 11.3 脚本验证流程

```rust
// 1. 创建交易
let tx = CellTx::new(inputs, deps, outputs, outputs_data, witnesses)?;

// 2. 创建数据提供者
let provider = MyCellDataProvider::new();

// 3. 创建验证器
let verifier = TransactionScriptVerifier::new(Arc::new(tx), Arc::new(provider));

// 4. 执行验证
let result = verifier.verify()?;
```

---

## 12. CKB 依赖对应

| CKB Crate | SPORA Crate/Module | 说明 |
|-----------|-------------------|------|
| `ckb-vm` | `ckb-vm` (依赖) | ✅ 完全相同，直接依赖 |
| `ckb-types` | `spora-exec::celltx` | ✅ 自己实现 (CellTx/CellOutput等) |
| `ckb-hash` | `blake3` (依赖) | ✅ 使用Blake3替代Blake2b |
| `ckb-traits` | `spora-exec::vm::verifier` | ✅ CellDataProvider trait |
| `ckb-script` | `spora-exec::vm` | ✅ 基于CKB修改，适配Cell模型 |
| `ckb-script::scheduler` | `spora-exec::scheduler` | ⚠️ 不同实现，交易级并行 |
| `ckb-store` | `spora-state` | ⚠️ DAG 适配 |
| `ckb-tx-pool` | `spora-mempool` | ⚠️ DAG 适配 |

### 12.1 Cargo.toml 依赖对比

**CKB script/Cargo.toml**:
```toml
[dependencies]
ckb-vm = "0.24"
ckb-types = { path = "../util/types" }
ckb-hash = { path = "../util/hash" }
ckb-traits = { path = "../traits" }
```

**SPORA exec/Cargo.toml**:
```toml
[dependencies]
ckb-vm = "0.24"
blake3 = "1.5"
# 无ckb-types, 无ckb-hash, 无ckb-traits
# 类型定义在本地 celltx/types.rs
# traits定义在 vm/verifier.rs
```

---

---

## 13. 实施状态总结（2025-04-12）

### ✅ 100% 完成

**核心系统调用**: 10/12 (83%)
- ✅ LoadCell, LoadCellData
- ✅ LoadInput, LoadHeader (DAG适配)
- ✅ LoadTx, LoadWitness, LoadScript
- ✅ CurrentCycles, Debugger
- ✅ Blake3Hash (SPORA特有扩展)

**VM 执行层**: 100%
- ✅ VM Machine 类型定义
- ✅ ScriptVersion (V0/V1/V2)
- ✅ TransactionScriptVerifier
- ✅ CellDataProvider trait
- ✅ 真实 CKB-VM 执行循环
- ✅ 已解析输入/依赖的运行时环境

**Cell 模型架构**: 100%
- ✅ CellOutput, CellInput, CellDep, CellTx
- ✅ Script (Lock/Type脚本)
- ✅ OutPoint, CellMeta
- ✅ ResolvedCellTx
- ✅ DepGroup 解析

**并行调度器**: 100%
- ✅ CellDAG (RW-Set依赖图)
- ✅ 冲突解决 (ConflictResolver)
- ✅ 并行执行器 (ParallelExecutor)

### ⏳ 待实现（可选）

**高级系统调用**: 2/12 (17%)
- ⏳ Exec (2043) - 动态脚本执行
- ⏳ Spawn (2601+) - 脚本生成

**并行VM调度**: 
- ⏳ 多脚本组并行执行 (当前为顺序执行)

---

**最后更新**: 2025-04-12  
**实施状态**: ✅ 核心100%完成，VM执行层生产就绪  
**维护者**: SPORA Team
