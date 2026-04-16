// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Spora 执行层序列化架构
//
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

use borsh::{BorshDeserialize, BorshSerialize};

/// VM ABI 序列化辅助函数
pub mod vm_abi;

/// Molecule 兼容性层（预留）
pub mod molecule_compat;

/// 序列化工具函数
pub mod utils;

/// 序列化缓存
pub mod cache;

/// 序列化宏
pub mod macros;

/// 序列化验证
pub mod validation;

/// 流式序列化
pub mod streaming;

/// 序列化安全
pub mod security;

/// 序列化压缩
pub mod compression;

/// 序列化错误类型
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum SerializationError {
    /// 不支持的版本
    #[error("unsupported version: {0}")]
    UnsupportedVersion(u8),
    /// 反序列化失败
    #[error("deserialization failed: {0}")]
    DeserializationFailed(String),
    /// 升级路径不可用
    #[error("upgrade path not available: from {from} to {to}")]
    UpgradePathNotAvailable { from: u8, to: u8 },
    /// IO 错误
    #[error("io error: {0}")]
    IoError(String),
}

impl From<std::io::Error> for SerializationError {
    fn from(e: std::io::Error) -> Self {
        SerializationError::IoError(e.to_string())
    }
}

/// 版本化序列化接口
///
/// 所有 VM-facing 和 storage-facing 类型必须实现此 trait，
/// 以确保 schema 演进时的向后兼容性。
pub trait VersionedSerializable: Sized + BorshSerialize + BorshDeserialize {
    /// 当前版本号
    const CURRENT_VERSION: u8;

    /// 获取实例的版本号
    fn version(&self) -> u8 {
        Self::CURRENT_VERSION
    }

    /// 从指定版本的二进制数据升级解析
    ///
    /// # Arguments
    /// * `version` - 数据存储时的版本号
    /// * `bytes` - 原始二进制数据
    ///
    /// # Returns
    /// * `Ok(Self)` - 成功解析并升级
    /// * `Err(Error)` - 解析失败或不支持的版本
    fn upgrade_from(version: u8, bytes: &[u8]) -> Result<Self, SerializationError> {
        if version == Self::CURRENT_VERSION {
            BorshDeserialize::try_from_slice(bytes)
                .map_err(|e| SerializationError::DeserializationFailed(e.to_string()))
        } else {
            Err(SerializationError::UpgradePathNotAvailable {
                from: version,
                to: Self::CURRENT_VERSION,
            })
        }
    }
}

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

impl<T: VersionedSerializable> VersionedEnvelope<T> {
    /// Borsh 格式版本标识
    pub const FORMAT_VERSION_BORSH: u8 = 0x00;
    /// Molecule 格式版本标识 (预留)
    pub const FORMAT_VERSION_MOLECULE: u8 = 0x80;

    /// 创建新的版本化信封 (使用当前版本)
    pub fn new(value: &T) -> Result<Self, SerializationError> {
        let payload = borsh::to_vec(value)?;
        Ok(Self {
            format_version: Self::FORMAT_VERSION_BORSH,
            schema_version: T::CURRENT_VERSION,
            payload,
            _phantom: std::marker::PhantomData,
        })
    }

    /// 解析信封内容
    pub fn parse(&self) -> Result<T, SerializationError> {
        match self.format_version {
            Self::FORMAT_VERSION_BORSH => {
                // Borsh 格式
                T::upgrade_from(self.schema_version, &self.payload)
            }
            Self::FORMAT_VERSION_MOLECULE..=0xFF => {
                // 未来: Molecule 格式
                Err(SerializationError::UnsupportedVersion(self.format_version))
            }
            _ => Err(SerializationError::UnsupportedVersion(self.format_version)),
        }
    }

    /// 获取格式版本
    pub fn format_version(&self) -> u8 {
        self.format_version
    }

    /// 获取 schema 版本
    pub fn schema_version(&self) -> u8 {
        self.schema_version
    }

    /// 获取 payload 大小
    pub fn payload_size(&self) -> usize {
        self.payload.len()
    }
}

impl<T> Default for VersionedEnvelope<T> {
    fn default() -> Self {
        Self {
            format_version: 0,
            schema_version: 0,
            payload: Vec::new(),
            _phantom: std::marker::PhantomData,
        }
    }
}

/// VM ABI 错误
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum VmAbiError {
    /// 序列化失败
    #[error("serialization failed: {0}")]
    SerializationFailed(String),
    /// 反序列化失败
    #[error("deserialization failed: {0}")]
    DeserializationFailed(String),
    /// ABI 版本不匹配
    #[error("ABI version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: u16, actual: u16 },
    /// 不支持的 ABI 版本
    #[error("unsupported ABI version: {0}")]
    UnsupportedAbiVersion(u16),
}

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

    /// 检查 ABI 版本是否兼容
    fn is_abi_compatible(version: u16) -> bool {
        version == Self::abi_version()
    }
}

/// VM ABI 版本协商器
pub struct VmAbiNegotiator;

impl VmAbiNegotiator {
    /// Borsh-based ABI v1 版本号
    pub const ABI_VERSION_BORSH_V1: u16 = 0x0001;
    /// Molecule-based ABI v1 版本号 (预留)
    pub const ABI_VERSION_MOLECULE_V1: u16 = 0x8001;

    /// 协商脚本和 VM 之间的 ABI 版本
    ///
    /// # Arguments
    /// * `script_version` - 脚本要求的 ABI 版本
    /// * `vm_capabilities` - VM 支持的 ABI 版本列表
    ///
    /// # Returns
    /// * `Ok(u16)` - 协商成功的 ABI 版本
    /// * `Err(VmAbiError)` - 协商失败
    pub fn negotiate(script_version: u16, vm_capabilities: &[u16]) -> Result<u16, VmAbiError> {
        // 优先使用脚本要求的版本
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
            actual: vm_capabilities.first().copied().unwrap_or(0),
        })
    }

    /// 获取 VM 默认支持的 ABI 版本列表
    pub fn default_capabilities() -> Vec<u16> {
        vec![Self::ABI_VERSION_BORSH_V1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    struct TestData {
        value: u64,
        name: String,
    }

    impl VersionedSerializable for TestData {
        const CURRENT_VERSION: u8 = 1;
    }

    #[test]
    fn test_versioned_envelope_roundtrip() {
        let data = TestData {
            value: 42,
            name: "test".to_string(),
        };

        let envelope = VersionedEnvelope::new(&data).unwrap();
        assert_eq!(envelope.format_version, VersionedEnvelope::<TestData>::FORMAT_VERSION_BORSH);
        assert_eq!(envelope.schema_version, TestData::CURRENT_VERSION);

        let parsed: TestData = envelope.parse().unwrap();
        assert_eq!(parsed, data);
    }

    #[test]
    fn test_versioned_envelope_unsupported_format() {
        let envelope = VersionedEnvelope::<TestData> {
            format_version: 0x80, // Molecule format (not supported yet)
            schema_version: 1,
            payload: vec![1, 2, 3],
            _phantom: std::marker::PhantomData,
        };

        let result = envelope.parse();
        assert!(matches!(result, Err(SerializationError::UnsupportedVersion(0x80))));
    }

    #[test]
    fn test_version_upgrade_path_not_available() {
        let envelope = VersionedEnvelope::<TestData> {
            format_version: VersionedEnvelope::<TestData>::FORMAT_VERSION_BORSH,
            schema_version: 0, // Old version
            payload: borsh::to_vec(&TestData { value: 42, name: "test".to_string() }).unwrap(),
            _phantom: std::marker::PhantomData,
        };

        let result = envelope.parse();
        assert!(matches!(
            result,
            Err(SerializationError::UpgradePathNotAvailable { from: 0, to: 1 })
        ));
    }

    #[test]
    fn test_vm_abi_negotiation_success() {
        let caps = vec![VmAbiNegotiator::ABI_VERSION_BORSH_V1];
        let result = VmAbiNegotiator::negotiate(VmAbiNegotiator::ABI_VERSION_BORSH_V1, &caps);
        assert_eq!(result.unwrap(), VmAbiNegotiator::ABI_VERSION_BORSH_V1);
    }

    #[test]
    fn test_vm_abi_negotiation_fallback() {
        // VM 只支持 Borsh，但脚本要求 Molecule
        let caps = vec![VmAbiNegotiator::ABI_VERSION_BORSH_V1];
        let result = VmAbiNegotiator::negotiate(VmAbiNegotiator::ABI_VERSION_MOLECULE_V1, &caps);
        // 应该回退到 Borsh v1
        assert_eq!(result.unwrap(), VmAbiNegotiator::ABI_VERSION_BORSH_V1);
    }

    #[test]
    fn test_vm_abi_negotiation_failure() {
        let caps = vec![0x0002]; // 只支持 v2
        let result = VmAbiNegotiator::negotiate(0x0001, &caps);
        assert!(matches!(
            result,
            Err(VmAbiError::VersionMismatch { expected: 0x0001, actual: 0x0002 })
        ));
    }

    #[test]
    fn test_default_capabilities() {
        let caps = VmAbiNegotiator::default_capabilities();
        assert!(caps.contains(&VmAbiNegotiator::ABI_VERSION_BORSH_V1));
    }

    #[test]
    fn test_versioned_envelope_default() {
        let envelope: VersionedEnvelope<TestData> = VersionedEnvelope::default();
        assert_eq!(envelope.format_version, 0);
        assert_eq!(envelope.schema_version, 0);
        assert!(envelope.payload.is_empty());
    }

    #[test]
    fn test_versioned_envelope_size_methods() {
        let data = TestData {
            value: 42,
            name: "test".to_string(),
        };
        let envelope = VersionedEnvelope::new(&data).unwrap();
        
        assert_eq!(envelope.format_version(), VersionedEnvelope::<TestData>::FORMAT_VERSION_BORSH);
        assert_eq!(envelope.schema_version(), TestData::CURRENT_VERSION);
        assert!(envelope.payload_size() > 0);
    }

    #[test]
    fn test_serialization_error_conversions() {
        let io_err = std::io::Error::new(std::io::ErrorKind::Other, "test error");
        let ser_err: SerializationError = io_err.into();
        assert!(matches!(ser_err, SerializationError::IoError(_)));
    }

    #[test]
    fn test_vm_serializable_abi_compatibility() {
        // Test that abi_version matches is_abi_compatible
        let version = TestData::CURRENT_VERSION as u16;
        assert!(TestData::is_abi_compatible(version));
        assert!(!TestData::is_abi_compatible(version + 1));
    }

    // Test struct for VmSerializable
    #[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
    struct TestVmData {
        id: u32,
        data: Vec<u8>,
    }

    impl VmSerializable for TestVmData {
        fn to_vm_bytes(&self) -> Vec<u8> {
            borsh::to_vec(self).unwrap_or_default()
        }

        fn from_vm_bytes(bytes: &[u8]) -> Result<Self, VmAbiError> {
            BorshDeserialize::try_from_slice(bytes)
                .map_err(|e| VmAbiError::DeserializationFailed(e.to_string()))
        }

        fn abi_version() -> u16 {
            0x0001
        }
    }

    #[test]
    fn test_vm_serializable_roundtrip() {
        let data = TestVmData {
            id: 123,
            data: vec![1, 2, 3, 4, 5],
        };

        let bytes = data.to_vm_bytes();
        let restored = TestVmData::from_vm_bytes(&bytes).unwrap();
        
        assert_eq!(data, restored);
        assert_eq!(TestVmData::abi_version(), 0x0001);
    }

    #[test]
    fn test_vm_serializable_deserialization_error() {
        let invalid_bytes = vec![0xFF, 0xFF, 0xFF]; // Invalid borsh data
        let result = TestVmData::from_vm_bytes(&invalid_bytes);
        assert!(matches!(result, Err(VmAbiError::DeserializationFailed(_))));
    }

    #[test]
    fn test_vm_abi_error_display() {
        let err = VmAbiError::SerializationFailed("test".to_string());
        assert!(err.to_string().contains("test"));

        let err = VmAbiError::DeserializationFailed("test".to_string());
        assert!(err.to_string().contains("test"));

        let err = VmAbiError::VersionMismatch { expected: 1, actual: 2 };
        assert!(err.to_string().contains("1") && err.to_string().contains("2"));

        let err = VmAbiError::UnsupportedAbiVersion(0x8001);
        assert!(err.to_string().contains("8001"));
    }

    #[test]
    fn test_versioned_envelope_with_empty_data() {
        #[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
        struct MinimalData;

        impl VersionedSerializable for MinimalData {
            const CURRENT_VERSION: u8 = 1;
        }

        let data = MinimalData;
        let envelope = VersionedEnvelope::new(&data).unwrap();
        let restored: MinimalData = envelope.parse().unwrap();
        assert_eq!(data, restored);
    }

    #[test]
    fn test_versioned_envelope_with_very_large_version() {
        #[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
        struct TestData;

        impl VersionedSerializable for TestData {
            const CURRENT_VERSION: u8 = 255; // Max u8
        }

        let data = TestData;
        let envelope = VersionedEnvelope::new(&data).unwrap();
        assert_eq!(envelope.schema_version(), 255);
        let restored: TestData = envelope.parse().unwrap();
        assert_eq!(data, restored);
    }

    #[test]
    fn test_version_upgrade_with_multiple_versions() {
        #[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
        struct MultiVersionData {
            value: u32,
        }

        impl VersionedSerializable for MultiVersionData {
            const CURRENT_VERSION: u8 = 3;

            fn upgrade_from(version: u8, bytes: &[u8]) -> Result<Self, SerializationError> {
                match version {
                    1 => {
                        // v1 had a single byte value
                        if bytes.is_empty() {
                            return Err(SerializationError::DeserializationFailed("empty bytes".to_string()));
                        }
                        Ok(Self { value: bytes[0] as u32 })
                    }
                    2 => {
                        // v2 had a u16 value
                        if bytes.len() < 2 {
                            return Err(SerializationError::DeserializationFailed("insufficient bytes".to_string()));
                        }
                        let value = u16::from_le_bytes([bytes[0], bytes[1]]) as u32;
                        Ok(Self { value })
                    }
                    3 => BorshDeserialize::try_from_slice(bytes)
                        .map_err(|e| SerializationError::DeserializationFailed(e.to_string())),
                    _ => Err(SerializationError::UpgradePathNotAvailable { from: version, to: 3 }),
                }
            }
        }

        // Test v1 → v3 migration
        let v1_envelope = VersionedEnvelope {
            format_version: 0x00,
            schema_version: 1,
            payload: vec![42],
            _phantom: std::marker::PhantomData::<MultiVersionData>,
        };
        let result = v1_envelope.parse().unwrap();
        assert_eq!(result.value, 42);

        // Test v2 → v3 migration
        let v2_envelope = VersionedEnvelope {
            format_version: 0x00,
            schema_version: 2,
            payload: vec![0x39, 0x05], // 1337 in little-endian
            _phantom: std::marker::PhantomData::<MultiVersionData>,
        };
        let result = v2_envelope.parse().unwrap();
        assert_eq!(result.value, 1337);
    }

    #[test]
    fn test_vm_abi_negotiation_with_empty_capabilities() {
        let result = VmAbiNegotiator::negotiate(0x0001, &[]);
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(VmAbiError::VersionMismatch { expected: 0x0001, actual: 0 })
        ));
    }

    #[test]
    fn test_vm_abi_negotiation_with_multiple_capabilities() {
        let caps = vec![0x0001, 0x0002, 0x8001];
        
        // Should find exact match
        let result = VmAbiNegotiator::negotiate(0x0002, &caps).unwrap();
        assert_eq!(result, 0x0002);
        
        // Should fall back from Molecule to Borsh
        let result = VmAbiNegotiator::negotiate(0x8002, &caps).unwrap();
        assert_eq!(result, 0x0002);
    }
}
