// SPDX-License-Identifier: ISC
// Copyright (C) 2025Tondi developers
//
// Cell validator tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::processes::cell_validator::{
        CellValidator, CellConsensusParams, CellValidationError,
        cell_validation_in_isolation, cell_validation_in_context, cell_validation_in_dag,
        CellStateProvider, DagCellProvider
    };
    use tondi_consensus_core::cell_metadata::CellMetadata;
    use tondi_exec::{CellTx, CellRef, CellOut, ScriptRef, OutPoint};
    use std::collections::HashMap;
    use std::sync::Arc;

    struct MockProvider {
        cells: HashMap<OutPoint, (bool, u64, CellMetadata)>, // (available, capacity, meta)
    }

    impl CellStateProvider for MockProvider {
        fn is_cell_available(&self, out_point: &OutPoint, _daa: u64) -> Result<bool, String> {
            Ok(self.cells.get(out_point).map(|(a, _, _)| *a).unwrap_or(false))
        }
        
        fn get_cell_capacity(&self, out_point: &OutPoint) -> Result<Option<u64>, String> {
            Ok(self.cells.get(out_point).map(|(_, c, _)| *c))
        }
    }

    impl DagCellProvider for MockProvider {
        fn get_cell_metadata(&self, out_point: &OutPoint) -> Result<Option<CellMetadata>, String> {
            Ok(self.cells.get(out_point).map(|(_, _, m)| m.clone()))
        }
        
        fn get_cell_at_daa(&self, out_point: &OutPoint, _daa: u64) -> Result<Option<CellMetadata>, String> {
            Ok(self.cells.get(out_point).map(|(_, _, m)| m.clone()))
        }
    }

    fn create_test_tx() -> CellTx {
        let lock = ScriptRef::new([0x00; 32], 0, vec![0; 20]);
        CellTx::new(
            vec![CellRef::new(OutPoint::new([0; 32], 0), 0)],
            vec![],
            vec![CellOut { lock, type_: None, capacity: 10000 }],
            vec![vec![]],
            vec![],
        ).unwrap()
    }

    #[test]
    fn test_isolation_validation() {
        let tx = create_test_tx();
        assert!(cell_validation_in_isolation::validate_cell_tx_in_isolation(&tx).is_ok());
    }

    #[test]
    fn test_context_validation() {
        let tx = create_test_tx();
        let mut provider = MockProvider {
            cells: HashMap::new(),
        };
        
        // Add a Cell
        let out_point = OutPoint::new([0; 32], 0);
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
                block_hash: tondi_hashes::Hash::from_bytes([0; 32]),
                lock_code_hash: None,
                type_code_hash: None,
                data: None,
            },
        ));
        
        assert!(cell_validation_in_context::validate_cell_tx_in_context(
            &tx, 100, &provider
        ).is_ok());
    }

    #[test]
    fn test_dag_validation() {
        let tx = create_test_tx();
        let mut provider = MockProvider {
            cells: HashMap::new(),
        };
        
        // Add a cellbase Cell
        let out_point = OutPoint::new([0; 32], 0);
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
                block_hash: tondi_hashes::Hash::from_bytes([0; 32]),
                lock_code_hash: None,
                type_code_hash: None,
                data: None,
            },
        ));
        
        // Should fail: cellbase not mature (created at 50, current 100, maturity 100)
        assert!(cell_validation_in_dag::validate_cellbase_maturity(
            &tx, 100, 100, &provider
        ).is_err());
        
        // Should succeed: cellbase mature (created at 50, current 200, maturity 100)
        assert!(cell_validation_in_dag::validate_cellbase_maturity(
            &tx, 200, 100, &provider
        ).is_ok());
    }

    #[test]
    fn test_cell_validator_integration() {
        let params = Arc::new(CellConsensusParams::default());
        let provider = Arc::new(MockProvider {
            cells: HashMap::new(),
        });
        
        let validator = CellValidator::new(params, provider);
        let tx = create_test_tx();
        
        // Test isolation validation
        assert!(validator.validate_in_isolation(&tx).is_ok());
    }
}
