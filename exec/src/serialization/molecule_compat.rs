// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Molecule 序列化兼容性层（预留实现）
//
//! # Molecule 序列化兼容性层
//!
//! 本模块为未来从 Borsh 迁移到 Molecule 预留接口。
//! 当 VM ABI 定型后，可以实现此模块以支持 Molecule 格式。
//!
//! ## 迁移计划
//!
//! Phase 3 (未来 6-12 个月):
//! 1. 定义 Molecule schema 文件 (.mol)
//! 2. 使用 moleculec 生成 Rust 代码
//! 3. 在此模块实现 Molecule 版本的序列化
//! 4. 更新 VmSerializable 实现以支持 Molecule
//!
//! ## 版本协商
//!
//! - ABI 版本 0x0001: Borsh-based (当前)
//! - ABI 版本 0x8001: Molecule-based (未来)
//!
//! 脚本可以通过 ABI 版本协商机制请求特定格式。

use crate::serialization::{SerializationError, VmAbiError};

/// Molecule 序列化器（预留接口）
///
/// 当 Molecule 支持添加时，此结构体将实现实际的序列化逻辑。
pub struct MoleculeSerializer;

impl MoleculeSerializer {
    /// 检查 Molecule 支持是否可用
    ///
    /// 当前始终返回 false，表示 Molecule 尚未实现。
    pub const fn is_available() -> bool {
        false
    }

    /// 获取 Molecule ABI 版本
    pub const fn abi_version() -> u16 {
        0x8001 // Molecule-based ABI v1
    }
}

/// Molecule 序列化错误
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum MoleculeError {
    /// Molecule 支持尚未实现
    #[error("Molecule serialization not yet implemented")]
    NotImplemented,
    /// Schema 不匹配
    #[error("schema mismatch: {0}")]
    SchemaMismatch(String),
    /// 验证失败
    #[error("validation failed: {0}")]
    ValidationFailed(String),
}

impl From<MoleculeError> for SerializationError {
    fn from(e: MoleculeError) -> Self {
        SerializationError::DeserializationFailed(e.to_string())
    }
}

impl From<MoleculeError> for VmAbiError {
    fn from(e: MoleculeError) -> Self {
        VmAbiError::SerializationFailed(e.to_string())
    }
}

/// 预留：Molecule 版本的 ResolvedHeader 序列化
///
/// 当实现时，将使用 molecule::serialize() 替代 borsh::to_vec()
pub fn serialize_resolved_header_molecule(_header: &crate::vm::ResolvedHeader) -> Result<Vec<u8>, MoleculeError> {
    Err(MoleculeError::NotImplemented)
}

/// 预留：Molecule 版本的 ResolvedHeader 反序列化
pub fn deserialize_resolved_header_molecule(_bytes: &[u8]) -> Result<crate::vm::ResolvedHeader, MoleculeError> {
    Err(MoleculeError::NotImplemented)
}

/// 预留：Molecule 版本的 ResolvedCell 序列化
pub fn serialize_resolved_cell_molecule(_cell: &crate::vm::ResolvedCell) -> Result<Vec<u8>, MoleculeError> {
    Err(MoleculeError::NotImplemented)
}

/// 预留：Molecule 版本的 ResolvedCell 反序列化
pub fn deserialize_resolved_cell_molecule(_bytes: &[u8]) -> Result<crate::vm::ResolvedCell, MoleculeError> {
    Err(MoleculeError::NotImplemented)
}

/// Schema 定义（预留）
///
/// 当迁移到 Molecule 时，需要定义以下 schema：
///
/// ```molecule
/// // resolved_header.mol
/// table ResolvedHeader {
///     hash: Byte32,
///     version: Uint32,
///     parents_by_level: ParentsByLevel,
///     hash_merkle_root: Byte32,
///     accepted_id_merkle_root: Byte32,
///     cell_commitment: Byte32,
///     cell_root: Byte32,
///     segment_root: Byte32,
///     timestamp: Uint64,
///     bits: Uint32,
///     nonce: Uint64,
///     daa_score: Uint64,
///     blue_work: Byte24,
///     blue_score: Uint64,
///     pruning_point: Byte32,
/// }
///
/// vector ParentsByLevel <ParentLevel>;
/// vector ParentLevel <Byte32>;
/// ```
pub mod schema {
    //! 预留的 Molecule schema 定义
    //!
    //! 这些定义将在 Phase 3 迁移时实现。

    /// ResolvedHeader schema 版本
    pub const RESOLVED_HEADER_SCHEMA_VERSION: u8 = 1;

    /// ResolvedCell schema 版本
    pub const RESOLVED_CELL_SCHEMA_VERSION: u8 = 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_molecule_not_available() {
        assert!(!MoleculeSerializer::is_available());
    }

    #[test]
    fn test_molecule_abi_version() {
        assert_eq!(MoleculeSerializer::abi_version(), 0x8001);
    }

    #[test]
    fn test_serialize_resolved_header_not_implemented() {
        // 创建一个空的 header 用于测试
        let header = crate::vm::ResolvedHeader {
            hash: [0; 32],
            version: 0,
            parents_by_level: vec![],
            hash_merkle_root: [0; 32],
            accepted_id_merkle_root: [0; 32],
            cell_commitment: [0; 32],
            cell_root: [0; 32],
            segment_root: [0; 32],
            timestamp: 0,
            bits: 0,
            nonce: 0,
            daa_score: 0,
            blue_work: [0; 24],
            blue_score: 0,
            pruning_point: [0; 32],
        };
        
        let result = serialize_resolved_header_molecule(&header);
        assert!(matches!(result, Err(MoleculeError::NotImplemented)));
    }
}
