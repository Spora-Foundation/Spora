// SPDX-License-Identifier: ISC
// Copyright (C) 2025 Spora developers
//
// VM Machine implementation (adapted from CKB-VM)
// Reference: ckb/script/src/types.rs

use super::error::VMError;
use ckb_vm::{
    DefaultMachineRunner, SupportMachine, Syscalls,
    ISA_B, ISA_IMC, ISA_MOP,
    machine::{VERSION0, VERSION1, VERSION2},
};

/// CKB-VM ISA type
pub type VmIsa = u8;
/// CKB-VM version type
pub type VmVersion = u32;
/// Cycles type
pub type Cycle = u64;

/// Script version (simplified from CKB)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScriptVersion {
    /// VM 0 with basic syscalls
    V0 = 0,
    /// VM 1 with extended syscalls (B + MOP extensions)
    V1 = 1,
    /// VM 2 (latest, full feature set)
    V2 = 2,
}

impl ScriptVersion {
    /// Returns the latest version
    pub const fn latest() -> Self {
        Self::V2
    }

    /// Returns the ISA set for this version
    pub fn vm_isa(self) -> VmIsa {
        match self {
            Self::V0 => ISA_IMC,
            Self::V1 => ISA_IMC | ISA_B | ISA_MOP,
            Self::V2 => ISA_IMC | ISA_B | ISA_MOP,
        }
    }

    /// Returns the VM version
    pub fn vm_version(self) -> VmVersion {
        match self {
            Self::V0 => VERSION0,
            Self::V1 => VERSION1,
            Self::V2 => VERSION2,
        }
    }

    /// Creates a VM core machine without cycles limit
    pub fn init_core_machine_without_limit(self) -> <Machine as DefaultMachineRunner>::Inner {
        self.init_core_machine(u64::MAX)
    }

    /// Creates a VM core machine with cycles limit
    pub fn init_core_machine(self, max_cycles: Cycle) -> <Machine as DefaultMachineRunner>::Inner {
        let isa = self.vm_isa();
        let version = self.vm_version();
        <<Machine as DefaultMachineRunner>::Inner as SupportMachine>::new(isa, version, max_cycles)
    }
}

/// Default machine type
/// For simplicity, we use TraceMachine with SparseMemory
/// ASM optimization can be added later via feature flags
pub type Machine = ckb_vm::TraceMachine<
    ckb_vm::DefaultCoreMachine<u64, ckb_vm::WXorXMemory<ckb_vm::SparseMemory<u64>>>,
>;

/// VM context for execution
pub struct VmContext {
    /// Script version
    pub version: ScriptVersion,
    /// Maximum cycles
    pub max_cycles: Cycle,
}

impl VmContext {
    /// Create a new VM context
    pub fn new(version: ScriptVersion, max_cycles: Cycle) -> Self {
        Self { version, max_cycles }
    }

    /// Create a VM context with default cycles limit (10M)
    pub fn with_default_cycles(version: ScriptVersion) -> Self {
        Self::new(version, 10_000_000)
    }
}

/// Run a script with given program and syscalls
///
/// NOTE: This is a placeholder implementation
/// Full CKB-VM integration requires more complex scheduler setup
/// See: /home/arthur/RustRoverProjects/ckb/script/src/verify.rs for complete implementation
pub fn run_script(
    _program: &[u8],
    _args: &[Vec<u8>],
    _syscalls: Vec<Box<dyn Syscalls<<Machine as DefaultMachineRunner>::Inner>>>,
    _context: &VmContext,
) -> Result<Cycle, VMError> {
    // TODO: Implement full CKB-VM execution
    // For now, return success with placeholder cycles
    // This allows compilation and testing of other components
    
    // The proper implementation requires:
    // 1. Create Scheduler (see ckb/script/src/scheduler.rs)
    // 2. Load program into machine
    // 3. Register syscalls
    // 4. Run with cycle limits
    // 5. Handle suspension/resumption
    
    Ok(1000) // Placeholder cycles
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_script_version_isa() {
        assert_eq!(ScriptVersion::V0.vm_isa(), ISA_IMC);
        assert_eq!(ScriptVersion::V1.vm_isa(), ISA_IMC | ISA_B | ISA_MOP);
        assert_eq!(ScriptVersion::V2.vm_isa(), ISA_IMC | ISA_B | ISA_MOP);
    }

    #[test]
    fn test_script_version_latest() {
        assert_eq!(ScriptVersion::latest(), ScriptVersion::V2);
    }

    #[test]
    fn test_vm_context_creation() {
        let ctx = VmContext::new(ScriptVersion::V2, 1_000_000);
        assert_eq!(ctx.version, ScriptVersion::V2);
        assert_eq!(ctx.max_cycles, 1_000_000);
    }
}
