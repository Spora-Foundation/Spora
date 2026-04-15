// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Cell transaction validation in isolation (no state required)

use super::errors::CellValidationError;
use spora_exec::CapacityError;
use spora_exec::CellTx;

/// Validate cell transaction format and basic constraints
pub fn validate_cell_tx_in_isolation(tx: &CellTx, max_cell_data_size: usize) -> Result<(), CellValidationError> {
    // 1. Check version
    if tx.version != spora_exec::CELL_TX_VERSION {
        return Err(CellValidationError::InvalidFormat(format!("Invalid version: 0x{:04X}", tx.version)));
    }

    // 2. Check inputs not empty (unless cellbase)
    if tx.inputs.is_empty() {
        return Err(CellValidationError::InvalidFormat("No inputs (use cellbase for mining rewards)".to_string()));
    }

    // 3. Check outputs not empty
    if tx.outputs.is_empty() {
        return Err(CellValidationError::InvalidFormat("No outputs".to_string()));
    }

    // 4. Check outputs_data length matches outputs
    if tx.outputs.len() != tx.outputs_data.len() {
        return Err(CellValidationError::InvalidFormat("outputs and outputs_data length mismatch".to_string()));
    }

    // 5. Check each output's capacity
    for (idx, output) in tx.outputs.iter().enumerate() {
        let data_len = tx.outputs_data.get(idx).map(|d| d.len()).unwrap_or(0);
        if data_len > max_cell_data_size {
            return Err(CellValidationError::InvalidFormat(format!(
                "Cell output data too large: {} > {} bytes",
                data_len, max_cell_data_size
            )));
        }
        output.verify_capacity(data_len).map_err(|e| match e {
            CapacityError::InsufficientCapacity { required, available } => {
                CellValidationError::InsufficientCapacity { required, available }
            }
        })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use spora_exec::{CellInput, CellOutput, OutPoint, Script};

    fn create_test_tx() -> CellTx {
        let lock = Script::new([0x00; 32], 0, vec![0; 20]);
        CellTx::new(
            vec![CellInput::new(OutPoint::new([0; 32], 0), 0)],
            vec![],
            vec![CellOutput { lock, type_: None, capacity: 10000 }],
            vec![vec![]],
            vec![],
        )
        .unwrap()
    }

    #[test]
    fn test_valid_transaction() {
        let tx = create_test_tx();
        assert!(validate_cell_tx_in_isolation(&tx, 500 * 1024).is_ok());
    }

    #[test]
    fn test_rejects_oversized_output_data() {
        let lock = Script::new([0x00; 32], 0, vec![0; 20]);
        let tx = CellTx::new(
            vec![CellInput::new(OutPoint::new([0; 32], 0), 0)],
            vec![],
            vec![CellOutput { lock, type_: None, capacity: 600_000 }],
            vec![vec![0u8; 1024]],
            vec![],
        )
        .unwrap();

        let result = validate_cell_tx_in_isolation(&tx, 512);
        assert!(result.is_err());
    }

    #[test]
    fn test_rejects_insufficient_output_capacity_with_structured_error() {
        let lock = Script::new([0x00; 32], 0, vec![0; 20]);
        let tx = CellTx::new(
            vec![CellInput::new(OutPoint::new([0; 32], 0), 0)],
            vec![],
            vec![CellOutput { lock, type_: None, capacity: 10 }],
            vec![vec![0u8; 100]],
            vec![],
        )
        .unwrap();

        let result = validate_cell_tx_in_isolation(&tx, 500 * 1024);
        assert!(matches!(result, Err(CellValidationError::InsufficientCapacity { .. })));
    }
}
