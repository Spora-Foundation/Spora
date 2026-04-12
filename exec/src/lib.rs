// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// This file is part of Spora, a DAG-based blockchain with Cell model.
// Portions adapted from Nervos CKB (MIT License).

//! Cell Execution Layer
//!
//! This crate implements the execution layer for Cell transactions, including:
//! - Cell transaction types (CellTx, CellRef, CellOut, ScriptRef)
//! - Parallel scheduler with RW-Set DAG
//! - VM integration (CKB-VM for script verification)
//! - Standard scripts (secp256k1 lock, capacity type)

#![warn(missing_docs)]

/// Cell transaction types and operations
pub mod celltx;
/// Parallel transaction scheduler
pub mod scheduler;
/// Standard scripts (secp256k1 lock, capacity type)
pub mod scripts;
/// VM integration for script execution (CKB-VM based)
#[cfg(feature = "vm")]
pub mod vm;

pub use celltx::{encode_dep_group_data, parse_dep_group_data, CellDep, CellOut, CellRef, CellTx, DepType, OutPoint, ScriptRef};

/// Cell transaction version
pub const CELL_TX_VERSION: u16 = 0xC001;

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
