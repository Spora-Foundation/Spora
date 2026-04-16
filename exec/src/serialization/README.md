# Spora 序列化分层治理

本文档描述 Spora 执行层的序列化架构。

## 架构概述

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

## 实现状态

### Phase 1: 存储层治理 ✅ 已完成

- [x] `VersionedSerializable` trait 定义
- [x] `VersionedEnvelope<T>` 实现
- [x] 所有 CellTx 类型实现 `VersionedSerializable`
- [x] 架构声明文档

### Phase 2: VM ABI 治理 ✅ 已完成

- [x] `VmSerializable` trait 定义
- [x] `VmAbiNegotiator` 版本协商
- [x] `ResolvedHeader` 实现 `VmSerializable`
- [x] `ResolvedCell` 实现 `VmSerializable`
- [x] VM syscall 使用新抽象
- [x] `vm_abi` 模块提供标准化序列化

### Phase 3: Molecule 迁移 🔮 预留

- [ ] Molecule schema 定义
- [ ] Molecule 代码生成
- [ ] Molecule 版本 `VmSerializable` 实现
- [ ] ABI 版本协商完整支持

## 核心组件

### VersionedSerializable

为存储层类型提供版本化序列化支持：

```rust
use spora_exec::{VersionedSerializable, VersionedEnvelope};

// 类型自动实现 VersionedSerializable
let tx = CellTx::new(...);

// 包装到 VersionedEnvelope
let envelope = VersionedEnvelope::new(&tx)?;

// 存储到 RocksDB
db.put(key, borsh::to_vec(&envelope)?)?;

// 从 RocksDB 读取并解析
let envelope: VersionedEnvelope<CellTx> = borsh::from_slice(&bytes)?;
let tx = envelope.parse()?;
```

### VmSerializable

为 VM-facing 类型提供 ABI 抽象：

```rust
use spora_exec::{VmSerializable, ResolvedHeader};

// 序列化传递给 VM
let header = ResolvedHeader { ... };
let bytes = header.to_vm_bytes();

// VM 内反序列化
let header = ResolvedHeader::from_vm_bytes(&bytes)?;
```

## 版本策略

### Schema 版本 (VersionedSerializable)

- 每个类型有 `CURRENT_VERSION` 常量
- 版本变更时实现 `upgrade_from` 方法
- 支持从旧版本平滑升级

### ABI 版本 (VmSerializable)

- `0x0001`: Borsh-based ABI v1 (当前)
- `0x8001`: Molecule-based ABI v1 (未来)
- 使用 `VmAbiNegotiator` 协商版本

## 模块结构

```
serialization/
├── mod.rs              # 核心 trait 和类型
├── vm_abi.rs           # VM ABI 序列化辅助函数
├── molecule_compat.rs  # Molecule 迁移预留接口
└── README.md           # 本文档
```

## 迁移路径

### Phase 1 (当前) ✅
- ✅ 所有 CellTx 类型实现 `VersionedSerializable`
- ✅ `ResolvedHeader` / `ResolvedCell` 实现 `VmSerializable`
- ✅ VM syscall 使用 `VmSerializable` 抽象
- ✅ `vm_abi` 模块标准化序列化格式

### Phase 2 (未来 3-6 个月)
- 完善 ABI 版本协商机制
- 支持脚本指定 ABI 版本
- 添加更多 VM-facing 类型的 `VmSerializable` 实现

### Phase 3 (按需 6-12 个月)
- 实现 `molecule_compat` 模块
- 添加 Molecule 版本的 `VmSerializable`
- 评估是否全栈切换到 Molecule
- 保持分层架构，独立迁移各层
