// SPDX-License-Identifier: ISC
// Copyright (C) 2025Tondi developers
//
// Cell transaction validation in DAG context (requires state)

use super::errors::CellValidationError;
use tondi_exec::{CellTx, OutPoint};

/// Cell state provider trait
///
/// Implemented by consensus to provide Cell state queries
pub trait CellStateProvider {
    /// Check if a Cell exists and is unspent
    fn is_cell_available(&self, out_point: &OutPoint, daa_score: u64) -> Result<bool, String>;
    
    /// Get Cell capacity
    fn get_cell_capacity(&self, out_point: &OutPoint) -> Result<Option<u64>, String>;
}

/// Validate cell transaction in DAG context
pub fn validate_cell_tx_in_context<P: CellStateProvider>(
    tx: &CellTx,
    daa_score: u64,
    provider: &P,
) -> Result<(), CellValidationError> {
    // 1. Check all inputs are available
    for input in &tx.inputs {
        let available = provider.is_cell_available(&input.out_point, daa_score)
            .map_err(|e| CellValidationError::InvalidFormat(e))?;
        
        if !available {
            return Err(CellValidationError::CellAlreadySpent([0; 32])); // TODO: proper hash
        }
        
        // Check time locks
        if input.since != 0 {
            validate_time_lock(input.since, daa_score)?;
        }
    }
    
    // 2. Verify capacity conservation
    let mut input_capacity = 0u64;
    for input in &tx.inputs {
        let capacity = provider.get_cell_capacity(&input.out_point)
            .map_err(|e| CellValidationError::InvalidFormat(e))?
            .ok_or_else(|| CellValidationError::CellNotFound([0; 32]))?;
        
        input_capacity = input_capacity.checked_add(capacity)
            .ok_or(CellValidationError::CapacityOverflow)?;
    }
    
    let output_capacity = tx.output_capacity();
    
    if output_capacity > input_capacity {
        return Err(CellValidationError::InsufficientCapacity {
            required: output_capacity,
            available: input_capacity,
        });
    }
    
    Ok(())
}

/// Validate time lock
fn validate_time_lock(since: u64, current_daa: u64) -> Result<(), CellValidationError> {
    let is_relative = (since & 0x8000_0000_0000_0000) != 0;
    let is_daa = (since & 0x4000_0000_0000_0000) != 0;
    let lock_value = since & 0x3FFF_FFFF_FFFF_FFFF;
    
    if is_daa && !is_relative {
        // Absolute DAA lock
        if current_daa < lock_value {
            return Err(CellValidationError::TimeLockNotSatisfied {
                lock_value,
                current: current_daa,
            });
        }
    }
    
    // TODO: Implement relative locks and timestamp locks
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct MockProvider {
        cells: HashMap<OutPoint, (bool, u64)>, // (available, capacity)
    }

    impl CellStateProvider for MockProvider {
        fn is_cell_available(&self, out_point: &OutPoint, _daa: u64) -> Result<bool, String> {
            Ok(self.cells.get(out_point).map(|(a, _)| *a).unwrap_or(false))
        }
        
        fn get_cell_capacity(&self, out_point: &OutPoint) -> Result<Option<u64>, String> {
            Ok(self.cells.get(out_point).map(|(_, c)| *c))
        }
    }

    #[test]
    fn test_time_lock_validation() {
        // Absolute DAA lock
        let since = 0x4000_0000_0000_0064; // DAA lock at 100
        assert!(validate_time_lock(since, 50).is_err());
        assert!(validate_time_lock(since, 100).is_ok());
        assert!(validate_time_lock(since, 150).is_ok());
    }
}

