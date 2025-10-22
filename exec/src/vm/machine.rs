// SPDX-License-Identifier: ISC
// Copyright (C) 2025Tondi developers
//
// VM machine wrapper for CKB-VM
// Adapted from CKB script/src/types.rs

use ckb_vm::{
    ISA_B, ISA_IMC, ISA_MOP,
    machine::{VERSION0, VERSION1, VERSION2},
    DefaultMachineRunner, SupportMachine,
};

/// CKB-VM ISA configuration
pub type VmIsa = u8;
/// CKB-VM version
pub type VmVersion = u32;

/// VM machine type selection based on features
///
/// Note: We use TraceMachine for now to support debugging
/// ASM machine can be enabled later for production performance
pub type Machine = ckb_vm::TraceMachine<
    ckb_vm::DefaultCoreMachine<u64, ckb_vm::WXorXMemory<ckb_vm::SparseMemory<u64>>>,
>;

/// SPORA VM version configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SporaVmVersion {
    /// VM version 0: IMC only
    V0 = 0,
    /// VM version 1: IMC + B + MOP
    V1 = 1,
}

impl SporaVmVersion {
    /// Get the latest VM version
    pub const fn latest() -> Self {
        Self::V1
    }

    /// Get VM ISA configuration
    pub fn vm_isa(self) -> VmIsa {
        match self {
            Self::V0 => ISA_IMC,
            Self::V1 => ISA_IMC | ISA_B | ISA_MOP,
        }
    }

    /// Get VM version number
    pub fn vm_version(self) -> VmVersion {
        match self {
            Self::V0 => VERSION0,
            Self::V1 => VERSION1,
        }
    }
}

impl Default for SporaVmVersion {
    fn default() -> Self {
        Self::latest()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_version() {
        let v1 = SporaVmVersion::V1;
        assert_eq!(v1.vm_isa(), ISA_IMC | ISA_B | ISA_MOP);
        assert_eq!(v1.vm_version(), VERSION1);
    }

    #[test]
    fn test_vm_default() {
        let default_vm = SporaVmVersion::default();
        assert_eq!(default_vm, SporaVmVersion::V1);
    }
}

