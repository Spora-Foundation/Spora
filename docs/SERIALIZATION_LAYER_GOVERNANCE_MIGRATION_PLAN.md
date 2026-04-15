# Spora 序列化分层治理迁移计划

- **日期**: 2026-04-15
- **状态**: 提案阶段
- **文档版本**: v1.0
- **关联文档**:
  - [cell_diff_audit.md](/Users/arthur/RustroverProjects/Spora/docs/cell_diff_audit.md)
  - [spora_consensus_architecture_v2.md](/Users/arthur/RustroverProjects/Spora/docs/spora_consensus_architecture_v2.md)
  - [CELL_MODEL_MIGRATION_ROADMAP.md](/Users/arthur/RustroverProjects/Spora/docs/CELL_MODEL_MIGRATION_ROADMAP.md)

---

## 1. 概述

### 1.1 背景与动机

Spora 项目当前采用 **Borsh** 作为默认序列化方案，与 CKB 使用的 **Molecule** 形成差异。随着项目进入 VM/Script ABI 定型阶段，需要重新评估序列化策略：

- **Borsh 的优势**: 简单、高性能、Rust 集成友好、代码占用小 (~2KB)
- **Borsh 的局限**: 非自描述、字段顺序依赖、enum ordinal 固定为 u8、不支持 partial reading
- **Molecule 的优势**: Canonical bytes、partial reading、版本兼容性、自包含子结构、零拷贝

### 1.2 核心洞察

**不需要"全部切换到 Molecule"，而是建立"分层序列化治理"架构。**

两种格式各有最佳适用场景。盲目全栈切换会带来：
- 巨大的代码改动量（所有 `#[derive(BorshSerialize)]`）
- 钱包/节点 RPC 的双格式兼容复杂度
- 零实际收益（内部通信/存储场景不需要 partial read）

### 1.3 设计目标

1. **现在不阻塞**: 不换 Molecule 不阻塞 Spora 主现代化议程
2. **未来可迁移**: VM ABI 定型时可平滑切换到 Molecule
3. **风险可控制**: 通过 version envelope 和 trait 抽象隔离变更影响

---

## 2. 三层架构设计

### 2.1 架构总览

```
┌─────────────────────────────────────────────────────────────────┐
│  Layer 3: VM/Script ABI 层 (长期需 Molecule)                    │
│  - ResolvedHeader, ResolvedCell, Witness Payload                │
│  - 脚本可见的所有数据结构                                        │
│  - 需要: canonical, partial read, version兼容                  │
├─────────────────────────────────────────────────────────────────┤
│  Layer 2: 内部通信/存储层 (保持 Borsh + Version Envelope)       │
│  - P2P消息 (Protobuf 已覆盖)                                    │
│  - 钱包/节点 RPC (已有 JSON/Borsh 双轨)                         │
│  - RocksDB 存储 (加 version envelope)                          │
├─────────────────────────────────────────────────────────────────┤
│  Layer 1: 共识关键路径 (已绕过 Borsh，保持现状)                 │
│  - Block Hash, TxID, SigHash (自定义流式哈希)                   │
│  - 完全不受影响，继续用 domain-separated Blake3                 │
└─────────────────────────────────────────────────────────────────┘
```

### 2.2 各层详细说明

#### Layer 1: 共识关键路径 (现状 ✓)

| 用途 | 当前实现 | 状态 |
|------|----------|------|
| Block Hash | 自定义流式哈希 + domain prefix | ✅ 已绕过 Borsh |
| TxID | 自定义流式哈希 + domain prefix | ✅ 已绕过 Borsh |
| SigHash | 自定义流式哈希 + domain prefix | ✅ 已绕过 Borsh |
| Script Hash | Blake3(code_hash \|\| hash_type \|\| args) | ✅ 已绕过 Borsh |

**关键保证**: 共识哈希完全不依赖 Borsh，避免跨协议攻击和哈希碰撞。

#### Layer 2: 内部通信/存储层 (需治理 ⚠️)

| 用途 | 当前实现 | 治理需求 |
|------|----------|----------|
| P2P 网络消息 | Protobuf | ✅ 无需改动 |
| 钱包 RPC | JSON / Borsh 双轨 | ✅ 无需改动 |
| RocksDB 存储 | `borsh::to_vec()` 裸写 | ⚠️ **需加 version envelope** |
| 内部类型序列化 | `#[derive(BorshSerialize)]` | ⚠️ **需明确 trait 边界** |

**风险点**: 当前存储层直接使用 `borsh::to_vec()` 写入 RocksDB，缺乏 versioning，schema 演进困难。

#### Layer 3: VM/Script ABI 层 (需长期规划 🔴)

| 类型 | 当前实现 | 问题 | 未来需求 |
|------|----------|------|----------|
| `ResolvedHeader` | `#[derive(BorshSerialize)]` | 字段布局变更影响脚本 | Canonical + Partial Read |
| `ResolvedCell` | 未序列化传递 | - | 可能需要 |
| Witness Payload | 应用自定义 | 无标准格式 | Canonical + Version |
| Proof Blob | 应用自定义 | 无标准格式 | Canonical + Version |

**关键问题**: `ResolvedHeader` 通过 Borsh 传递给 VM，其字段布局变更将直接影响脚本可见数据。

---

## 3. 序列化格式对比分析

### 3.1 Borsh vs Molecule 技术对比

| 特性 | Borsh | Molecule | 影响场景 |
|------|-------|----------|----------|
| **编码方式** | 长度前缀 (动态类型) | 偏移量表 | 可变类型序列化 |
| **零拷贝** | ❌ 不支持 | ✅ 支持 | VM 内频繁访问 |
| **Partial Read** | ❌ O(N) 遍历 | ✅ O(1) 偏移量 | 脚本跳读字段 |
| **代码大小** | ~2KB | ~10KB | 合约二进制大小 |
| **确定性** | ✅ 是 | ✅ 是 | 共识哈希 |
| **Schema 演进** | ❌ 需手动处理 | ✅ 内置版本支持 | 长期兼容性 |
| **跨语言支持** | 中等 | 优秀 (官方多语言) | SDK 生态 |
| **Rust 集成** | 优秀 (derive 宏) | 良好 (codegen) | 开发效率 |

### 3.2 Spora 现状适用性评估

| 场景 | 推荐格式 | 理由 |
|------|----------|------|
| **VM ABI 定型后** | Molecule | 脚本需要 partial read、canonical bytes |
| **存储层** | Borsh + Version Envelope | 简单、性能好，versioning 解决演进 |
| **P2P 网络** | Protobuf (现状) | 无需改动 |
| **共识哈希** | 自定义流式 (现状) | 已正确绕过 Borsh |
| **钱包 RPC** | JSON/Borsh 双轨 (现状) | 无需改动 |

---

## 4. 迁移计划

### 4.1 Phase 1: 立即执行 (本周)

#### 任务 1.1: 定义版本化序列化 trait

```rust
// 新增: exec/src/serialization/mod.rs

/// 版本化序列化接口
/// 
/// 所有 VM-facing 和 storage-facing 类型必须实现此 trait，
/// 以确保 schema 演进时的向后兼容性。
pub trait VersionedSerializable: Sized {
    /// 当前版本号
    const CURRENT_VERSION: u8;
    
    /// 获取实例的版本号
    fn version(&self) -> u8;
    
    /// 从指定版本的二进制数据升级解析
    /// 
    /// # Arguments
    /// * `version` - 数据存储时的版本号
    /// * `bytes` - 原始二进制数据
    /// 
    /// # Returns
    /// * `Ok(Self)` - 成功解析并升级
    /// * `Err(Error)` - 解析失败或不支持的版本
    fn upgrade_from(version: u8, bytes: &[u8]) -> Result<Self, SerializationError>;
}

/// 序列化错误类型
#[derive(Debug, thiserror::Error)]
pub enum SerializationError {
    #[error("unsupported version: {0}")]
    UnsupportedVersion(u8),
    #[error("deserialization failed: {0}")]
    DeserializationFailed(String),
    #[error("upgrade path not available: from {from} to {to}")]
    UpgradePathNotAvailable { from: u8, to: u8 },
}
```

#### 任务 1.2: 实现 VersionedEnvelope 包装器

```rust
// 新增: exec/src/serialization/envelope.rs

/// 版本化序列化信封
/// 
/// 所有存储到 RocksDB 的类型必须使用此包装器，
/// 以确保未来可以平滑迁移序列化格式。
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct VersionedEnvelope<T> {
    /// 序列化格式版本
    /// 
    /// - 0x00-0x7F: Borsh 格式 (当前)
    /// - 0x80-0xFF: 保留给未来格式 (如 Molecule)
    pub format_version: u8,
    
    /// 数据 schema 版本
    pub schema_version: u8,
    
    /// 实际序列化数据
    pub payload: Vec<u8>,
    
    /// 类型标记 (编译时优化)
    _phantom: std::marker::PhantomData<T>,
}

impl<T: BorshSerialize + BorshDeserialize> VersionedEnvelope<T> {
    /// 创建新的版本化信封 (使用当前版本)
    pub fn new(value: &T) -> Result<Self, std::io::Error> {
        let payload = borsh::to_vec(value)?;
        Ok(Self {
            format_version: 0x00, // Borsh
            schema_version: T::CURRENT_VERSION,
            payload,
            _phantom: std::marker::PhantomData,
        })
    }
    
    /// 解析信封内容
    pub fn parse(&self) -> Result<T, SerializationError> {
        match self.format_version {
            0x00 => {
                // Borsh 格式
                if self.schema_version == T::CURRENT_VERSION {
                    BorshDeserialize::try_from_slice(&self.payload)
                        .map_err(|e| SerializationError::DeserializationFailed(e.to_string()))
                } else {
                    T::upgrade_from(self.schema_version, &self.payload)
                }
            }
            0x80..=0xFF => {
                // 未来: Molecule 格式
                Err(SerializationError::UnsupportedVersion(self.format_version))
            }
            _ => Err(SerializationError::UnsupportedVersion(self.format_version)),
        }
    }
}
```

#### 任务 1.3: 添加架构声明文档

```rust
// 在 exec/src/lib.rs 顶部添加:

//! # Spora 执行层序列化架构声明
//!
//! ## 重要保证
//!
//! 1. **Borsh bytes 不是 canonical consensus bytes**
//!    - 所有共识关键哈希 (block hash, txid, sighash) 使用自定义流式哈希
//!    - Borsh 仅用于内部通信和存储，不参与共识
//!
//! 2. **所有 VM-facing 类型必须使用 VersionedEnvelope**
//!    - 确保脚本可见数据可以平滑演进
//!    - 为未来切换到 Molecule 预留路径
//!
//! 3. **VM ABI 是独立抽象层**
//!    - 通过 `VmSerializable` trait 抽象序列化实现
//!    - 未来可单独切换 VM 层到 Molecule，不影响其他层
//!
//! ## 分层责任
//!
//! - Layer 1 (共识): 自定义流式哈希，完全绕过 Borsh
//! - Layer 2 (存储): Borsh + VersionedEnvelope
//! - Layer 3 (VM ABI): Borsh (当前) → Molecule (未来)
```

### 4.2 Phase 2: VM ABI 治理 (未来 3-6 个月)

#### 任务 2.1: 定义 VmSerializable trait

```rust
// 新增: exec/src/vm/serialization.rs

/// VM 可见数据的序列化抽象
/// 
/// 此 trait 隔离 VM ABI 与具体序列化实现，
/// 允许未来从 Borsh 切换到 Molecule 而不影响业务逻辑。
pub trait VmSerializable: Sized {
    /// 序列化为 VM 可见字节
    fn to_vm_bytes(&self) -> Vec<u8>;
    
    /// 从 VM 可见字节解析
    fn from_vm_bytes(bytes: &[u8]) -> Result<Self, VmAbiError>;
    
    /// 获取 ABI 版本
    fn abi_version() -> u16;
}

/// VM ABI 错误
#[derive(Debug, thiserror::Error)]
pub enum VmAbiError {
    #[error("serialization failed: {0}")]
    SerializationFailed(String),
    #[error("deserialization failed: {0}")]
    DeserializationFailed(String),
    #[error("ABI version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: u16, actual: u16 },
}

// 为 ResolvedHeader 实现 Borsh 版本
impl VmSerializable for ResolvedHeader {
    fn to_vm_bytes(&self) -> Vec<u8> {
        // 当前: Borsh 实现
        borsh::to_vec(self).expect("Borsh serialization should not fail")
    }
    
    fn from_vm_bytes(bytes: &[u8]) -> Result<Self, VmAbiError> {
        BorshDeserialize::try_from_slice(bytes)
            .map_err(|e| VmAbiError::DeserializationFailed(e.to_string()))
    }
    
    fn abi_version() -> u16 {
        0x0001 // Borsh-based ABI v1
    }
}
```

#### 任务 2.2: 重构 VM syscall 使用新抽象

```rust
// 修改: exec/src/vm/syscalls/load_header.rs

pub struct LoadHeader {
    // ... 现有字段 ...
}

impl LoadHeader {
    fn load_header(&self, hash: &[u8; 32]) -> Result<Vec<u8>, VMError> {
        let header = self.data_provider
            .load_header(hash)
            .ok_or(VMError::ItemMissing("header".to_string()))?;
        
        // 使用 VmSerializable 而非直接 Borsh
        Ok(header.to_vm_bytes())
    }
}
```

### 4.3 Phase 3: VM ABI 定型时 (未来 6-12 个月)

#### 任务 3.1: 实现 Molecule 版本的 VmSerializable

```rust
// 新增: exec/src/vm/serialization_molecule.rs

// 当 VM ABI 定型后，为 ResolvedHeader 实现 Molecule 版本

pub struct MoleculeVmSerializer;

impl VmSerializable for ResolvedHeader {
    fn to_vm_bytes(&self) -> Vec<u8> {
        // 切换到 Molecule 实现
        molecule::serialize(self).expect("Molecule serialization should not fail")
    }
    
    fn from_vm_bytes(bytes: &[u8]) -> Result<Self, VmAbiError> {
        molecule::deserialize(bytes)
            .map_err(|e| VmAbiError::DeserializationFailed(e.to_string()))
    }
    
    fn abi_version() -> u16 {
        0x8001 // Molecule-based ABI v1
    }
}
```

#### 任务 3.2: 版本协商机制

```rust
// 在 VM 启动时进行 ABI 版本协商

pub struct VmAbiNegotiator;

impl VmAbiNegotiator {
    /// 协商脚本和 VM 之间的 ABI 版本
    pub fn negotiate(script_version: u16, vm_capabilities: &[u16]) -> Result<u16, VmAbiError> {
        // 优先使用 Molecule 版本 (0x80xx)
        // 回退到 Borsh 版本 (0x00xx)
        for cap in vm_capabilities {
            if *cap == script_version {
                return Ok(*cap);
            }
        }
        
        // 尝试版本回退
        if script_version >= 0x8000 {
            // 脚本要求 Molecule，但 VM 不支持，尝试 Borsh
            let borsh_fallback = script_version & 0x00FF;
            if vm_capabilities.contains(&borsh_fallback) {
                return Ok(borsh_fallback);
            }
        }
        
        Err(VmAbiError::VersionMismatch {
            expected: script_version,
            actual: vm_capabilities[0],
        })
    }
}
```

---

## 5. 风险分析与缓解

### 5.1 风险评估矩阵

| 风险 | 可能性 | 影响 | 缓解措施 |
|------|--------|------|----------|
| Borsh schema 变更导致存储不兼容 | 中 | 高 | VersionedEnvelope 强制使用 |
| VM ABI 变更破坏现有脚本 | 中 | 极高 | VmSerializable trait 抽象 |
| 全栈切换 Molecule 成本过高 | 低 | 中 | 分层架构避免全栈切换 |
| 多格式并存增加复杂度 | 中 | 低 | 清晰的 trait 边界和文档 |

### 5.2 关键决策检查点

| 检查点 | 触发条件 | 决策内容 |
|--------|----------|----------|
| CP-1 | Phase 1 完成 | 确认所有 VM-facing 类型已加 version envelope |
| CP-2 | Phase 2 完成 | 确认 VmSerializable trait 已覆盖所有 VM ABI |
| CP-3 | VM ABI 定型前 1 个月 | 评估是否启动 Molecule 实现 |
| CP-4 | VM ABI 定型时 | 决定是否切换 VM 层到 Molecule |

---

## 6. 实施时间表

```
Week 1-2:  Phase 1.1 - 1.3
    ├── 定义 VersionedSerializable trait
    ├── 实现 VersionedEnvelope
    ├── 添加架构声明文档
    └── 代码审查

Week 3-4:  Phase 1.4 (存储层迁移)
    ├── 识别所有 RocksDB 存储类型
    ├── 添加 VersionedEnvelope 包装
    ├── 迁移测试
    └── 性能基准测试

Month 2-3: Phase 2 (VM ABI 治理)
    ├── 定义 VmSerializable trait
    ├── 重构所有 VM syscall
    ├── 单元测试覆盖
    └── 集成测试

Month 6+:  Phase 3 (按需执行)
    ├── 评估 VM ABI 定型状态
    ├── 实现 Molecule 版本 (如需要)
    ├── 版本协商机制
    └── 主网升级协调
```

---

## 7. 验证清单

### 7.1 Phase 1 完成标准

- [ ] `VersionedSerializable` trait 定义完成
- [ ] `VersionedEnvelope<T>` 实现完成并通过测试
- [ ] 所有 RocksDB 存储类型使用 `VersionedEnvelope`
- [ ] 架构声明文档已添加到 `exec/src/lib.rs`
- [ ] CI 通过，无新增警告
- [ ] 性能基准测试无显著退化 (<5%)

### 7.2 Phase 2 完成标准

- [ ] `VmSerializable` trait 定义完成
- [ ] 所有 VM-facing 类型实现 `VmSerializable`
- [ ] 所有 VM syscall 使用 `to_vm_bytes()` / `from_vm_bytes()`
- [ ] 单元测试覆盖率 >90%
- [ ] 集成测试通过 (包括脚本执行)

### 7.3 Phase 3 完成标准 (按需)

- [ ] Molecule 版本的 `VmSerializable` 实现完成
- [ ] 版本协商机制实现并测试
- [ ] 多语言 SDK 兼容性验证
- [ ] 主网升级计划制定

---

## 8. 附录

### 8.1 术语表

| 术语 | 定义 |
|------|------|
| **Canonical bytes** | 确定性字节表示，相同逻辑值始终产生相同字节序列 |
| **Partial reading** | 无需解析整个对象即可读取特定字段的能力 |
| **Version envelope** | 包装序列化数据的元数据头，包含版本信息 |
| **VM ABI** | 虚拟机与脚本之间的应用程序二进制接口 |
| **Schema evolution** | 数据结构随时间演进而保持兼容性的能力 |

### 8.2 参考文档

- [Borsh 规范](https://borsh.io/)
- [Molecule 规范](https://docs.nervos.org/docs/serialization/serialization-molecule-in-ckb)
- [Spora 序列化分层架构现状](memory://bb6979d6-2150-464d-a08a-49a00732081a)

### 8.3 决策记录

| 日期 | 决策 | 理由 |
|------|------|------|
| 2026-04-15 | 不立即切换到 Molecule | 当前工作不依赖 partial reading，Borsh 足够 |
| 2026-04-15 | 采用分层治理架构 | 避免全栈切换成本，保留未来灵活性 |
| 2026-04-15 | 强制使用 VersionedEnvelope | 解决 schema 演进风险，预留格式切换路径 |

---

**文档维护者**: Spora 核心团队  
**下次审查日期**: 2026-05-15 (或 VM ABI 定型时，以先到为准)
