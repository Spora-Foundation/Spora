// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// This file is part of Spora, a DAG-based blockchain with Cell model.
// Portions adapted from Nervos CKB (MIT License).

//! # Spora 执行层 (Cell Execution Layer)
//!
//! This crate implements the execution layer for Cell transactions, including:
//! - Cell transaction types (CellTx, CellInput, CellOutput, Script)
//! - Parallel scheduler with RW-Set DAG
//! - VM integration (CKB-VM for script verification)
//! - Standard scripts (secp256k1 lock, capacity type)
//!
//! ## 序列化分层治理架构
//!
//! 本 crate 采用分层序列化策略，以平衡性能、兼容性和未来演进需求：
//!
//! ### 三层架构
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │  Layer 3: VM/Script ABI 层 (Borsh v1 + Molecule v1)             │
//! │  - ResolvedHeader, ResolvedCell, Witness Payload                │
//! │  - 脚本可见的所有数据结构                                        │
//! │  - 需要: canonical, partial read, version兼容                  │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  Layer 2: 内部通信/存储层 (保持 Borsh + Version Envelope)       │
//! │  - P2P消息 (Protobuf 已覆盖)                                    │
//! │  - 钱包/节点 RPC (已有 JSON/Borsh 双轨)                         │
//! │  - RocksDB 存储 (加 version envelope)                          │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  Layer 1: 共识关键路径 (已绕过 Borsh，保持现状)                 │
//! │  - Block Hash, TxID, SigHash (自定义流式哈希)                   │
//! │  - 完全不受影响，继续用 domain-separated Blake3                 │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ### 重要保证
//!
//! 1. **Borsh bytes 不是 canonical consensus bytes**
//!    - 所有共识关键哈希 (block hash, txid, sighash) 使用自定义流式哈希
//!    - Borsh 仅用于内部通信和存储，不参与共识
//!
//! 2. **VM-facing ABI 必须经过显式格式边界**
//!    - legacy default 仍保留 Borsh/custom v1，避免破坏现有脚本
//!    - Molecule v1 (`0x8001`) 已作为 canonical VM ABI 可用
//!
//! 3. **VM ABI 是独立抽象层**
//!    - 通过 [`VmSerializable`](serialization::VmSerializable) trait 抽象序列化实现
//!    - VM 层可以独立切到 Molecule，不影响存储层和共识哈希层
//!
//! ### 相关模块
//!
//! - [`serialization`](crate::serialization) - 版本化序列化 trait 和 envelope
//! - [`celltx`](crate::celltx) - Cell 交易类型
//! - [`vm`](crate::vm) - VM 集成和 ABI 层
//!
//! 详细设计文档: `docs/SERIALIZATION_LAYER_GOVERNANCE_MIGRATION_PLAN.md`

#![warn(missing_docs)]

/// Cell transaction types and operations
pub mod celltx;
/// Parallel transaction scheduler
pub mod scheduler;
/// Standard scripts (secp256k1 lock, capacity type)
pub mod scripts;
/// Serialization framework with version governance
pub mod serialization;
/// VM integration for script execution (CKB-VM based)
#[cfg(feature = "vm")]
pub mod vm;
#[cfg(feature = "vm")]
pub use vm::{ResolvedCell, ResolvedHeader};

pub use celltx::{
    encode_dep_group_data, parse_dep_group_data, CapacityError, CellDep, CellInput, CellOutput, CellTx, DepType, OutPoint, Script,
    ScriptHashVersion, CELLTX_SCHEMA_VERSION,
};

// Re-export serialization framework
pub use serialization::{
    append_vm_abi_trailer, split_vm_abi_trailer, SerializationError, VersionedEnvelope, VersionedSerializable, VmAbiError,
    VmAbiFormat, VmAbiNegotiator, VmSerializable,
};

// Re-export vm_abi helpers
pub use serialization::vm_abi::{
    serialize_cell_input, serialize_cell_output, serialize_outpoint, serialize_script, serialized_cell_output_size,
    serialized_script_size,
};

// Re-export utils
pub use serialization::utils::{
    deserialize_from_bytes, deserialize_many, estimate_serialized_size, is_valid_versioned_envelope, peek_format_version,
    peek_schema_version, serialize_many, serialize_to_bytes, SerializeResult,
};

// Re-export cache
pub use serialization::cache::{CacheStats, SerializationCache, ThreadSafeSerializationCache};

// Re-export validation
pub use serialization::validation::{is_valid_envelope, validate_envelope, SerializerValidator, ValidationConfig, ValidationResult};

// Re-export streaming
pub use serialization::streaming::{deserialize_streaming, serialize_streaming, StreamingDeserializer, StreamingSerializer};

// Re-export security
pub use serialization::security::{
    compute_hash, deserialize_with_integrity, serialize_with_integrity, verify_integrity, SecureEnvelope, SecurityConfig,
    SecurityGuard,
};

// Re-export compression
pub use serialization::compression::{
    compress, decompress, estimate_compressed_size, select_algorithm, CompressedEnvelope, CompressionAlgorithm, CompressionConfig,
    CompressionResult, CompressionStats,
};

// Re-export molecule compatibility layer
pub use serialization::molecule_compat::{
    deserialize_cell_input_molecule, deserialize_cell_output_molecule, deserialize_outpoint_molecule,
    deserialize_resolved_cell_molecule, deserialize_resolved_header_molecule, deserialize_script_molecule,
    serialize_cell_input_molecule, serialize_cell_output_molecule, serialize_outpoint_molecule, serialize_resolved_cell_molecule,
    serialize_resolved_header_molecule, serialize_script_molecule, MoleculeError, MoleculeSerializer,
};

/// Cell transaction version
pub const CELL_TX_VERSION: u32 = 0xC001;

/// Network ID (u32, little-endian)
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkId {
    /// Reserved (invalid)
    Reserved = 0x00000000,
    /// Mainnet
    Mainnet = 0x00000001,
    /// Testnet
    Testnet = 0x00000002,
    /// Devnet
    Devnet = 0x00000003,
    /// Regtest
    Regtest = 0xFFFFFFFF,
}

impl NetworkId {
    /// Convert to u32
    pub fn to_u32(self) -> u32 {
        self as u32
    }

    /// Parse from u32
    pub fn from_u32(val: u32) -> Option<Self> {
        match val {
            0x00000000 => Some(Self::Reserved),
            0x00000001 => Some(Self::Mainnet),
            0x00000002 => Some(Self::Testnet),
            0x00000003 => Some(Self::Devnet),
            0xFFFFFFFF => Some(Self::Regtest),
            _ => None,
        }
    }
}
