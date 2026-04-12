// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// VM system calls
// Adapted from CKB script/src/syscalls/

pub mod blake3;
pub mod current_cycles;
pub mod debugger;
pub mod load_cell;
pub mod load_cell_data;
pub mod load_header;
pub mod load_input;
pub mod load_script;
pub mod load_tx;
pub mod load_witness;
pub mod utils; // Spora-specific: blake3 hash syscall

pub use blake3::Blake3Hash;
pub use current_cycles::CurrentCycles;
pub use debugger::Debugger;
pub use load_cell::LoadCell;
pub use load_cell_data::LoadCellData;
pub use load_header::LoadHeader;
pub use load_input::LoadInput;
pub use load_script::LoadScript;
pub use load_tx::LoadTx;
pub use load_witness::LoadWitness;
pub use utils::*;

/// System call numbers (aligned with CKB)
pub const LOAD_TX_HASH_SYSCALL_NUMBER: u64 = 2061;
pub const LOAD_SCRIPT_HASH_SYSCALL_NUMBER: u64 = 2062;
pub const LOAD_CELL_SYSCALL_NUMBER: u64 = 2071;
pub const LOAD_HEADER_SYSCALL_NUMBER: u64 = 2072;
pub const LOAD_INPUT_SYSCALL_NUMBER: u64 = 2073;
pub const LOAD_WITNESS_SYSCALL_NUMBER: u64 = 2074;
pub const LOAD_SCRIPT_SYSCALL_NUMBER: u64 = 2075;
pub const LOAD_CELL_BY_FIELD_SYSCALL_NUMBER: u64 = 2081;
pub const LOAD_HEADER_BY_FIELD_SYSCALL_NUMBER: u64 = 2082;
pub const LOAD_INPUT_BY_FIELD_SYSCALL_NUMBER: u64 = 2083;
pub const LOAD_CELL_DATA_SYSCALL_NUMBER: u64 = 2092;
pub const CURRENT_CYCLES_SYSCALL_NUMBER: u64 = 2042;
pub const DEBUG_PRINT_SYSCALL_NUMBER: u64 = 2177;

/// Spora-specific syscall numbers (3000+ range to avoid conflicts)
pub const BLAKE3_HASH_SYSCALL_NUMBER: u64 = 3001;
pub const EXEC_SYSCALL_NUMBER: u64 = 2043;

/// System call return codes
pub const SUCCESS: u8 = 0;
pub const INDEX_OUT_OF_BOUND: u8 = 1;
pub const ITEM_MISSING: u8 = 2;
pub const LENGTH_NOT_ENOUGH: u8 = 3;
pub const SLICE_OUT_OF_BOUND: u8 = 4;
pub const WAIT_FAILURE: u8 = 5;
pub const INVALID_FD: u8 = 6;
pub const OTHER_END_CLOSED: u8 = 7;
pub const MAX_VMS_SPAWNED: u8 = 8;
pub const MAX_FDS_CREATED: u8 = 9;

/// Source type for loading data
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Load from transaction inputs
    Input = 0x01,
    /// Load from transaction outputs
    Output = 0x02,
    /// Load from cell dependencies
    CellDep = 0x03,
    /// Load from header dependencies
    HeaderDep = 0x04,
    /// Load from current script group
    GroupInput = 0x0100,
    /// Load from current script group outputs
    GroupOutput = 0x0200,
}

impl Source {
    /// Parse source from u64
    pub fn parse(source: u64) -> Option<Self> {
        match source {
            0x01 => Some(Self::Input),
            0x02 => Some(Self::Output),
            0x03 => Some(Self::CellDep),
            0x04 => Some(Self::HeaderDep),
            0x0100 => Some(Self::GroupInput),
            0x0200 => Some(Self::GroupOutput),
            _ => None,
        }
    }
}

/// Cell field selector
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellField {
    /// Capacity field
    Capacity = 0,
    /// Data hash field
    DataHash = 1,
    /// Lock field
    Lock = 2,
    /// Lock hash field
    LockHash = 3,
    /// Type field
    Type = 4,
    /// Type hash field
    TypeHash = 5,
    /// Occupied capacity field
    OccupiedCapacity = 6,
}

impl CellField {
    /// Parse field from u64
    pub fn parse(field: u64) -> Option<Self> {
        match field {
            0 => Some(Self::Capacity),
            1 => Some(Self::DataHash),
            2 => Some(Self::Lock),
            3 => Some(Self::LockHash),
            4 => Some(Self::Type),
            5 => Some(Self::TypeHash),
            6 => Some(Self::OccupiedCapacity),
            _ => None,
        }
    }
}

/// Input field selector
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputField {
    /// OutPoint (tx_hash + index)
    OutPoint = 0,
    /// Since (time-lock value)
    Since = 1,
}

impl InputField {
    /// Parse field from u64
    pub fn parse(field: u64) -> Option<Self> {
        match field {
            0 => Some(Self::OutPoint),
            1 => Some(Self::Since),
            _ => None,
        }
    }
}

/// Header field selector (for future LoadHeader implementation)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderField {
    /// DAA score
    DaaScore = 0,
    /// Timestamp
    Timestamp = 1,
    /// Block hash
    Hash = 2,
    /// Parent hashes
    Parents = 3,
}

impl HeaderField {
    /// Parse field from u64
    pub fn parse(field: u64) -> Option<Self> {
        match field {
            0 => Some(Self::DaaScore),
            1 => Some(Self::Timestamp),
            2 => Some(Self::Hash),
            3 => Some(Self::Parents),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_parse() {
        assert_eq!(Source::parse(0x01), Some(Source::Input));
        assert_eq!(Source::parse(0x02), Some(Source::Output));
        assert_eq!(Source::parse(0x03), Some(Source::CellDep));
        assert_eq!(Source::parse(0x99), None);
    }

    #[test]
    fn test_cell_field_parse() {
        assert_eq!(CellField::parse(0), Some(CellField::Capacity));
        assert_eq!(CellField::parse(1), Some(CellField::DataHash));
        assert_eq!(CellField::parse(99), None);
    }
}
