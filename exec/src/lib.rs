// SPDX-License-Identifier: ISC
// Copyright (C) 2024 Tondi developers
//
// This file is part of Tondi, a DAG-based blockchain with Cell model.
// Portions adapted from Nervos CKB (MIT License).

//! Cell Execution Layer
//!
//! This crate implements the execution layer for Cell transactions, including:
//! - Cell transaction types (CellTx, CellRef, CellOut, ScriptRef)
//! - Parallel scheduler with RW-Set DAG
//! - VM integration (CKB-VM for script verification)
//! - Standard scripts (secp256k1 lock, capacity type)

#![warn(missing_docs)]

pub mod celltx;
// pub mod scheduler;  // Phase 2
// pub mod vm;         // Phase 3
// pub mod scripts;    // Phase 3

pub use celltx::{CellTx, CellRef, CellOut, ScriptRef, OutPoint, CellDep, DepType};

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

