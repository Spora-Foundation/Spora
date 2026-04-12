# Spora Header 字段使用审计报告

**日期**: 2026-04-12  
**审计范围**: `consensus/core/src/header.rs` 及相关哈希、验证、RPC/P2P 转换路径  
**目的**: 识别 Header 中不再需要的字段，消除架构债务

---

## Header 结构概览

```rust
pub struct Header {
    pub hash: Hash,                          // 缓存的区块哈希
    pub version: u16,                        // 版本号
    pub parents_by_level: Vec<Vec<Hash>>,    // 分层父块列表
    pub hash_merkle_root: Hash,              // 交易哈希默克尔根
    pub accepted_id_merkle_root: Hash,       // 接受ID默克尔根
    pub cell_commitment: Hash,               // Cell状态承诺
    pub cell_root: Hash,                     // Cell状态默克尔根
    pub timestamp: u64,                      // 毫秒时间戳
    pub bits: u32,                           // 难度目标
    pub nonce: u64,                          // 随机数
    pub daa_score: u64,                      // DAA分数
    pub blue_work: BlueWorkType,             // 蓝色工作量
    pub blue_score: u64,                     // 蓝色分数
    pub pruning_point: Hash,                 // 修剪点哈希
}
```

---

## 字段使用审计

### 1. 核心字段（必需）

| 字段 | 使用位置 | 状态 | 说明 |
|------|----------|------|------|
| `hash` | 哈希缓存 | 必需 | 区块唯一标识 |
| `version` | `hashing::header::hash` | 必需 | 协议版本 |
| `parents_by_level` | 共识、哈希、GhostDAG | 必需 | DAG 多父块结构 |
| `timestamp` | 哈希、时间验证 | 必需 | 区块时间 |
| `bits` | 哈希、PoW 验证 | 必需 | 难度目标 |
| `nonce` | 哈希、PoW 挖矿 | 必需 | 随机数 |
| `daa_score` | 哈希、共识、时间锁 | 必需 | 全局时间线 |
| `blue_work` | 哈希、GhostDAG | 必需 | 链选择依据 |
| `blue_score` | 哈希、GhostDAG | 必需 | 链选择依据 |
| `pruning_point` | 哈希、修剪 | 必需 | 状态修剪 |

### 2. 交易相关字段

#### 2.1 `hash_merkle_root`

**用途**: 区块内所有交易的默克尔根

**使用位置**:
- `consensus/core/src/hashing/header.rs:18` - 包含在区块哈希计算中
- `consensus/src/pipeline/virtual_processor/processor.rs:2048` - 计算并验证

**状态**: 必需

**说明**: 用于验证区块内交易的完整性。

---

#### 2.2 `accepted_id_merkle_root`

**用途**: 被接受交易ID的默克尔根

**使用位置**:
- `consensus/core/src/hashing/header.rs:19` - 包含在区块哈希计算中
- `consensus/src/pipeline/virtual_processor/processor.rs:965-972` - 验证逻辑
- `consensus/src/pipeline/virtual_processor/processor.rs:2050` - 计算逻辑

**验证代码**:
```rust
let calculated_accepted_id_merkle_root = self.accepted_id_merkle_root(&ctx.accepted_tx_ids);
if calculated_accepted_id_merkle_root != header.accepted_id_merkle_root {
    return Err(RuleError::BadAcceptedIDMerkleRoot(...));
}
```

**状态**: 必需

**说明**: GhostDAG 特有字段，用于验证哪些交易被当前区块接受（考虑 DAG 的并发特性）。

---

### 3. Cell 相关字段

#### 3.1 `cell_commitment`

**用途**: Cell 状态的版本化承诺

**使用位置**:
- `consensus/core/src/hashing/header.rs` - 包含在区块哈希计算中
- `consensus/src/pipeline/virtual_processor/processor.rs` - 验证逻辑与计算逻辑

**计算方式**:
```rust
/// V0 format: H("spora/cell_commitment/v0" || cell_root)
fn compute_cell_commitment_v0(cell_root: Hash) -> Hash {
    let mut hasher = Hasher::new();
    hasher.update(b"spora/cell_commitment/v0");
    hasher.update(cell_root.as_bytes().as_ref());
    Hash::from_bytes(*hasher.finalize().as_bytes())
}
```

**状态**: 必需

**说明**: 提供版本化的 Cell 状态承诺，允许未来升级承诺格式而不破坏兼容性。

---

#### 3.2 `cell_root`

**用途**: 所有活跃 Cell 的默克尔根

**使用位置**:
- `consensus/core/src/hashing/header.rs` - 包含在区块哈希计算中
- `consensus/src/pipeline/virtual_processor/processor.rs` - 计算逻辑与校验逻辑
- `consensus/src/consensus/cell_set_override.rs` - 创世块设置

**状态**: 必需

**说明**: 用于状态证明，轻客户端可以通过此根验证 Cell 状态。

**跨边界暴露缺口**:
- `rpc/core/src/model/header.rs` - `RpcHeader -> Header` / `RpcRawHeader -> Header` 时将 `cell_root` 置为 `Default::default()`
- `rpc/grpc/core/proto/rpc.proto` - `RpcBlockHeader` 当前没有 `cellRoot`
- `rpc/grpc/core/src/convert/header.rs` - 反序列化 `RpcBlockHeader` 时把 `cell_root` 硬编码为零哈希
- `protocol/p2p/proto/p2p.proto` - `BlockHeader` 当前没有 `cellRoot`
- `protocol/p2p/src/convert/header.rs` - P2P 反序列化时把 `cell_root` 硬编码为 `ZERO_HASH`
- `consensus/client/src/header.rs` - JS/wasm bridge 在缺失 `cellRoot` 时为兼容旧对象默认填零哈希

**建议**: 完成 `cell_root` 在 RPC、P2P 和客户端桥接层的显式暴露，避免共识头跨边界后丢失状态根。

---

## 审计结论

### 字段必要性总结

| 类别 | 字段数 | 必需 | 可选/待评估 |
|------|--------|------|-------------|
| 核心共识 | 10 | 10 | 0 |
| 交易相关 | 2 | 2 | 0 |
| Cell 状态 | 2 | 2 | 0 |
| **总计** | **14** | **14** | **0** |

### 关键发现

1. **所有字段均为必需**: 经过审计，Header 中的 14 个字段都有明确用途，没有冗余字段。

2. **GhostDAG 特有字段合理**:
   - `accepted_id_merkle_root`: DAG 并发交易的接受证明
   - `blue_work`/`blue_score`: GhostDAG 链选择算法必需
   - `parents_by_level`: DAG 多父块结构

3. **Cell 模型字段合理**:
   - `cell_commitment`: 版本化状态承诺
   - `cell_root`: 状态证明根

4. **唯一实质问题不在 Header 设计本身，而在跨边界暴露**: `cell_root` 在共识内部是必需字段，但在 RPC、P2P 和部分客户端桥接层仍会丢失或被回填为零哈希。

### 无删除建议

当前 Header 设计紧凑，没有可以删除的字段。所有字段都服务于：
- 共识安全性（PoW、GhostDAG）
- 状态验证（Cell 模型）
- 轻客户端支持（默克尔证明）

### 改进建议

1. **完成 RPC/P2P 暴露**: 将 `cell_root` 添加到 `RpcBlockHeader`、`BlockHeader` 及其双向转换
2. **收敛兼容分支**: 待协议层补齐后，移除 JS/wasm bridge 对缺失 `cellRoot` 的零哈希回填
3. **文档完善**: 为每个字段添加更详细的注释说明其用途
4. **版本规划**: `cell_commitment` 的版本化设计为未来升级预留空间

---

## 附录: 字段使用详细映射

```
hash                          -> 通用 (缓存标识)
version                       -> hashing::header::hash
parents_by_level              -> hashing::header::hash, GhostDAG, 共识
hash_merkle_root              -> hashing::header::hash, virtual_processor
accepted_id_merkle_root       -> hashing::header::hash, virtual_processor (验证+计算)
cell_commitment               -> hashing::header::hash, virtual_processor (验证+计算)
cell_root                     -> hashing::header::hash, virtual_processor (计算), cell_set_override
timestamp                     -> hashing::header::hash, 时间验证
bits                          -> hashing::header::hash, PoW
nonce                         -> hashing::header::hash, PoW
daa_score                     -> hashing::header::hash, 共识, 时间锁
blue_work                     -> hashing::header::hash, GhostDAG
blue_score                    -> hashing::header::hash, GhostDAG
pruning_point                 -> hashing::header::hash, 状态修剪
```

---

## 参考文件

- `consensus/core/src/header.rs` - Header 定义
- `consensus/core/src/hashing/header.rs` - 哈希计算
- `consensus/src/pipeline/virtual_processor/processor.rs` - 验证逻辑
- `rpc/grpc/core/proto/rpc.proto` - RPC 头部协议定义
- `rpc/core/src/model/header.rs` - RPC 模型转换
- `rpc/grpc/core/src/convert/header.rs` - RPC gRPC 转换
- `protocol/p2p/proto/p2p.proto` - P2P 头部协议定义
- `protocol/p2p/src/convert/header.rs` - P2P 头部转换
