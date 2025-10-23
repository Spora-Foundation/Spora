// SPDX-License-Identifier: ISC
// Copyright (C) 2025 Spora developers
//
// Cell transaction validation specific to DAG consensus

use super::errors::CellValidationError;
use super::cell_validation_in_context::CellStateProvider;
use spora_exec::{CellTx, OutPoint};
use spora_consensus_core::cell_metadata::CellMetadata;

/// Extended state provider for DAG validation
pub trait DagCellProvider: CellStateProvider {
    /// Get Cell metadata
    fn get_cell_metadata(&self, out_point: &OutPoint) -> Result<Option<CellMetadata>, String>;
    
    /// GHOSTDAG-aware: query Cell state at a specific DAA score
    fn get_cell_at_daa(&self, out_point: &OutPoint, daa: u64) -> Result<Option<CellMetadata>, String>;
}

/// Validate cellbase maturity
///
/// Cellbase outputs cannot be spent until they mature (DAA score threshold)
/// This prevents miners from spending rewards in case of reorg
pub fn validate_cellbase_maturity<P: DagCellProvider>(
    tx: &CellTx,
    current_daa: u64,
    maturity: u64,
    provider: &P,
) -> Result<(), CellValidationError> {
    for input in &tx.inputs {
        // Get Cell metadata
        let meta = provider.get_cell_metadata(&input.out_point)
            .map_err(|e| CellValidationError::InvalidFormat(e))?
            .ok_or_else(|| CellValidationError::CellNotFound([0; 32]))?;
        
        // Check cellbase maturity
        if meta.is_cellbase {
            let maturity_daa = meta.block_daa_score + maturity;
            if current_daa < maturity_daa {
                return Err(CellValidationError::CellbaseNotMature {
                    created_daa: meta.block_daa_score,
                    current_daa,
                    required_daa: maturity_daa,
                });
            }
        }
    }
    
    Ok(())
}

/// Validate Cell existence in DAG context
///
/// Ensures all referenced Cells exist and are in the DAG history
pub fn validate_cell_existence<P: DagCellProvider>(
    tx: &CellTx,
    current_daa: u64,
    provider: &P,
) -> Result<(), CellValidationError> {
    // Check all inputs exist
    for input in &tx.inputs {
        let available = provider.is_cell_available(&input.out_point, current_daa)
            .map_err(|e| CellValidationError::InvalidFormat(e))?;
        
        if !available {
            return Err(CellValidationError::CellNotFound([0; 32]));
        }
    }
    
    // Check all deps exist
    for dep in &tx.deps {
        let available = provider.is_cell_available(&dep.out_point, current_daa)
            .map_err(|e| CellValidationError::InvalidFormat(e))?;
        
        if !available {
            return Err(CellValidationError::DepCellNotFound([0; 32]));
        }
    }
    
    Ok(())
}

/// Validate transaction in reorg context
///
/// When applying a block after reorg, check that:
/// - All inputs were unspent at this DAA score
/// - No conflicts with the new chain tip
pub fn validate_in_reorg_context<P: DagCellProvider>(
    tx: &CellTx,
    block_daa: u64,
    provider: &P,
) -> Result<(), CellValidationError> {
    // Validate at the block's DAA score, not current tip
    validate_cell_existence(tx, block_daa, provider)?;
    
    // Check each input was valid at that time
    for input in &tx.inputs {
        let meta = provider.get_cell_metadata(&input.out_point)
            .map_err(|e| CellValidationError::InvalidFormat(e))?
            .ok_or_else(|| CellValidationError::CellNotFound([0; 32]))?;
        
        // Cell must have been created before or at this block
        if meta.block_daa_score > block_daa {
            return Err(CellValidationError::CellNotYetCreated {
                created_daa: meta.block_daa_score,
                spent_at_daa: block_daa,
            });
        }
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use spora_exec::{CellRef, ScriptRef, CellOut};
    use std::collections::HashMap;

    struct MockDagProvider {
        cells: HashMap<OutPoint, (bool, u64, CellMetadata)>, // (available, capacity, meta)
    }

    impl CellStateProvider for MockDagProvider {
        fn is_cell_available(&self, out_point: &OutPoint, _daa: u64) -> Result<bool, String> {
            Ok(self.cells.get(out_point).map(|(a, _, _)| *a).unwrap_or(false))
        }
        
        fn get_cell_capacity(&self, out_point: &OutPoint) -> Result<Option<u64>, String> {
            Ok(self.cells.get(out_point).map(|(_, c, _)| *c))
        }
    }

    impl DagCellProvider for MockDagProvider {
        fn get_cell_metadata(&self, out_point: &OutPoint) -> Result<Option<CellMetadata>, String> {
            Ok(self.cells.get(out_point).map(|(_, _, m)| m.clone()))
        }
        
        fn get_cell_at_daa(&self, out_point: &OutPoint, _daa: u64) -> Result<Option<CellMetadata>, String> {
            // Simple implementation: just return the cell if it exists
            Ok(self.cells.get(out_point).map(|(_, _, m)| m.clone()))
        }
    }

    #[test]
    fn test_cellbase_maturity() {
        let out_point = OutPoint::new([1; 32], 0);
        let mut provider = MockDagProvider {
            cells: HashMap::new(),
        };
        
        // Add a cellbase Cell created at DAA 50
        provider.cells.insert(out_point.clone(), (
            true,
            100000,
            CellMetadata {
                capacity: 100000,
                lock_hash: [0; 32],
                type_hash: None,
                data_hash: [0; 32],
                block_daa_score: 50,
                is_cellbase: true,
                block_hash: spora_hashes::Hash::from_bytes([0; 32]),
                lock_code_hash: None,
                type_code_hash: None,
                data: None,
            },
        ));
        
        let lock = ScriptRef::new([0; 32], 0, vec![]);
        let tx = CellTx::new(
            vec![CellRef::new(out_point, 0)],
            vec![],
            vec![CellOut { lock, type_: None, capacity: 90000 }],
            vec![vec![]],
            vec![],
        ).unwrap();
        
        // Should fail: current DAA = 100, required = 150
        assert!(validate_cellbase_maturity(&tx, 100, 100, &provider).is_err());
        
        // Should succeed: current DAA = 150
        assert!(validate_cellbase_maturity(&tx, 150, 100, &provider).is_ok());
        
        // Should succeed: current DAA = 200
        assert!(validate_cellbase_maturity(&tx, 200, 100, &provider).is_ok());
    }

    #[test]
    fn test_non_cellbase_no_maturity() {
        let out_point = OutPoint::new([2; 32], 0);
        let mut provider = MockDagProvider {
            cells: HashMap::new(),
        };
        
        // Add a regular Cell (not cellbase)
        provider.cells.insert(out_point.clone(), (
            true,
            100000,
            CellMetadata {
                capacity: 100000,
                lock_hash: [0; 32],
                type_hash: None,
                data_hash: [0; 32],
                block_daa_score: 50,
                is_cellbase: false,
                block_hash: spora_hashes::Hash::from_bytes([0; 32]),
                lock_code_hash: None,
                type_code_hash: None,
                data: None,
            },
        ));
        
        let lock = ScriptRef::new([0; 32], 0, vec![]);
        let tx = CellTx::new(
            vec![CellRef::new(out_point, 0)],
            vec![],
            vec![CellOut { lock, type_: None, capacity: 90000 }],
            vec![vec![]],
            vec![],
        ).unwrap();
        
        // Should succeed immediately (no maturity for regular Cells)
        assert!(validate_cellbase_maturity(&tx, 51, 100, &provider).is_ok());
    }

    #[test]
    fn test_reorg_validation() {
        let out_point = OutPoint::new([3; 32], 0);
        let mut provider = MockDagProvider {
            cells: HashMap::new(),
        };
        
        // Cell created at DAA 100
        provider.cells.insert(out_point.clone(), (
            true,
            100000,
            CellMetadata {
                capacity: 100000,
                lock_hash: [0; 32],
                type_hash: None,
                data_hash: [0; 32],
                block_daa_score: 100,
                is_cellbase: false,
                block_hash: spora_hashes::Hash::from_bytes([0; 32]),
                lock_code_hash: None,
                type_code_hash: None,
                data: None,
            },
        ));
        
        let lock = ScriptRef::new([0; 32], 0, vec![]);
        let tx = CellTx::new(
            vec![CellRef::new(out_point, 0)],
            vec![],
            vec![CellOut { lock, type_: None, capacity: 90000 }],
            vec![vec![]],
            vec![],
        ).unwrap();
        
        // Tx in block at DAA 150: should succeed
        assert!(validate_in_reorg_context(&tx, 150, &provider).is_ok());
        
        // Tx in block at DAA 99: should fail (Cell not yet created)
        assert!(validate_in_reorg_context(&tx, 99, &provider).is_err());
    }
}

