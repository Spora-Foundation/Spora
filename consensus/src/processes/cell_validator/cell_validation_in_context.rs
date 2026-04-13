// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Cell transaction validation in DAG context (requires state)

use super::errors::CellValidationError;
use spora_exec::{CellTx, OutPoint};
use spora_hashes::Hash;

/// Cell state provider trait
///
/// Implemented by consensus to provide Cell state queries
pub trait CellStateProvider {
    /// Check if a Cell exists and is unspent
    fn is_cell_available(&self, out_point: &OutPoint, pov: Hash) -> Result<bool, String>;

    /// Get Cell capacity
    fn get_cell_capacity(&self, out_point: &OutPoint, pov: Hash) -> Result<Option<u64>, String>;
}

/// Validate cell transaction in state context
pub fn validate_cell_tx_in_context<P: CellStateProvider>(
    tx: &CellTx,
    pov: Hash,
    _daa_score: u64,
    provider: &P,
) -> Result<(), CellValidationError> {
    // 1. Check all inputs are available
    for input in &tx.inputs {
        let available = provider.is_cell_available(&input.previous_output, pov).map_err(|e| CellValidationError::InvalidFormat(e))?;

        if !available {
            return Err(CellValidationError::CellAlreadySpent(input.previous_output.tx_hash));
        }
    }

    // 2. Verify capacity conservation
    let mut input_capacity = 0u64;
    for input in &tx.inputs {
        let capacity = provider
            .get_cell_capacity(&input.previous_output, pov)
            .map_err(|e| CellValidationError::InvalidFormat(e))?
            .ok_or_else(|| CellValidationError::CellNotFound(input.previous_output.tx_hash))?;

        input_capacity = input_capacity.checked_add(capacity).ok_or(CellValidationError::CapacityOverflow)?;
    }

    let output_capacity = tx.output_capacity();

    if output_capacity > input_capacity {
        return Err(CellValidationError::InsufficientCapacity { required: output_capacity, available: input_capacity });
    }

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
        fn is_cell_available(&self, out_point: &OutPoint, _pov: Hash) -> Result<bool, String> {
            Ok(self.cells.get(out_point).map(|(a, _)| *a).unwrap_or(false))
        }

        fn get_cell_capacity(&self, out_point: &OutPoint, _pov: Hash) -> Result<Option<u64>, String> {
            Ok(self.cells.get(out_point).map(|(_, c)| *c))
        }
    }

    #[test]
    fn test_context_validation() {
        let pov = Hash::from_bytes([1; 32]);
        let provider = MockProvider { cells: HashMap::new() };
        let tx = CellTx::new(vec![], vec![], vec![], vec![], vec![]).unwrap();
        assert!(validate_cell_tx_in_context(&tx, pov, 100, &provider).is_ok());
    }
}
