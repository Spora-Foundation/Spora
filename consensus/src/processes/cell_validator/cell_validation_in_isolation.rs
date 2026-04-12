// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Cell transaction validation in isolation (no state required)

use super::errors::CellValidationError;
use spora_exec::CellTx;

/// Validate cell transaction format and basic constraints
pub fn validate_cell_tx_in_isolation(tx: &CellTx, max_cell_data_size: usize) -> Result<(), CellValidationError> {
    // 1. Check version
    if tx.ver != spora_exec::CELL_TX_VERSION {
        return Err(CellValidationError::InvalidFormat(format!("Invalid version: 0x{:04X}", tx.ver)));
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
        output.verify_capacity(data_len).map_err(|e| CellValidationError::InvalidFormat(e.to_string()))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use spora_exec::{CellOut, CellRef, OutPoint, ScriptRef};

    fn create_test_tx() -> CellTx {
        let lock = ScriptRef::new([0x00; 32], 0, vec![0; 20]);
        CellTx::new(
            vec![CellRef::new(OutPoint::new([0; 32], 0), 0)],
            vec![],
            vec![CellOut { lock, type_: None, capacity: 10000 }],
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
        let lock = ScriptRef::new([0x00; 32], 0, vec![0; 20]);
        let tx = CellTx::new(
            vec![CellRef::new(OutPoint::new([0; 32], 0), 0)],
            vec![],
            vec![CellOut { lock, type_: None, capacity: 600_000 }],
            vec![vec![0u8; 1024]],
            vec![],
        )
        .unwrap();

        let result = validate_cell_tx_in_isolation(&tx, 512);
        assert!(result.is_err());
    }
}
