// SPDX-License-Identifier: ISC
// Copyright (C) 2025Tondi developers
//
// VM integration for Cell script execution

#[cfg(feature = "vm")]
pub mod machine;
#[cfg(feature = "vm")]
pub mod syscalls;
#[cfg(feature = "vm")]
pub mod cost_model;
#[cfg(feature = "vm")]
pub mod scheduler;

#[cfg(feature = "vm")]
pub use machine::*;
#[cfg(feature = "vm")]
pub use syscalls::*;
#[cfg(feature = "vm")]
pub use cost_model::*;
#[cfg(feature = "vm")]
pub use scheduler::*;

/// VM integration status
pub const VM_ENABLED: bool = cfg!(feature = "vm");

/// VM version for SPORA
pub const SPORA_VM_VERSION: u32 = 0x0001_0000; // 1.0.0

/// VM ISA support
pub const SPORA_VM_ISA: u8 = 0x07; // IMC + B + MOP

/// Maximum cycles per block
pub const MAX_BLOCK_CYCLES: u64 = 70_000_000; // Same as CKB

/// Maximum cycles per transaction
pub const MAX_TX_CYCLES: u64 = 10_000_000;
