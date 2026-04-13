// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Cell validator tests

#[cfg(test)]
mod tests {
    use crate::processes::cell_validator::{
        cell_validation_in_context, cell_validation_in_dag, cell_validation_in_isolation, CellConsensusParams, CellStateProvider,
        CellValidator, DagCellProvider,
    };
    #[cfg(feature = "vm")]
    use crate::processes::cell_validator::{CellScriptDataProvider, CellValidationError};
    use spora_consensus_core::{cell_metadata::CellMetadata, tx::TransactionOutpoint};
    #[cfg(feature = "vm")]
    use spora_exec::scripts::{always_success_code_hash, ALWAYS_SUCCESS_SCRIPT};
    #[cfg(feature = "vm")]
    use spora_exec::{CellDep, DepType};
    use spora_exec::{CellOut, CellRef, CellTx, OutPoint, ScriptRef};
    use spora_hashes::Hash;
    use std::collections::HashMap;
    use std::sync::Arc;

    struct MockProvider {
        cells: HashMap<(Hash, OutPoint), CellMetadata>,
        block_timestamps: HashMap<Hash, u64>,
    }

    impl CellStateProvider for MockProvider {
        fn is_cell_available(&self, out_point: &OutPoint, pov: Hash) -> Result<bool, String> {
            Ok(self.cells.contains_key(&(pov, out_point.clone())))
        }

        fn get_cell_capacity(&self, out_point: &OutPoint, pov: Hash) -> Result<Option<u64>, String> {
            Ok(self.cells.get(&(pov, out_point.clone())).map(|meta| meta.capacity))
        }
    }

    impl DagCellProvider for MockProvider {
        fn get_cell_at_pov(&self, out_point: &OutPoint, pov: Hash) -> Result<Option<CellMetadata>, String> {
            Ok(self.cells.get(&(pov, out_point.clone())).cloned())
        }

        fn get_block_timestamp(&self, block_hash: Hash) -> Result<u64, String> {
            self.block_timestamps.get(&block_hash).copied().ok_or_else(|| format!("missing timestamp for {}", block_hash))
        }
    }

    #[cfg(feature = "vm")]
    impl CellScriptDataProvider for MockProvider {
        fn get_cell_data(&self, out_point: &OutPoint, pov: Hash) -> Result<Option<Vec<u8>>, String> {
            Ok(self.cells.get(&(pov, out_point.clone())).and_then(|meta| meta.data.clone()))
        }

        fn get_header(&self, block_hash: Hash) -> Result<Option<spora_exec::vm::ResolvedHeader>, String> {
            Ok(self.block_timestamps.get(&block_hash).copied().map(|timestamp| spora_exec::vm::ResolvedHeader {
                hash: block_hash.as_bytes(),
                timestamp,
                daa_score: 0,
                parents: vec![],
            }))
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
        )
        .unwrap()
    }

    fn tx_outpoint(out_point: &OutPoint) -> TransactionOutpoint {
        TransactionOutpoint { tx_hash: out_point.tx_hash, index: out_point.index }
    }

    #[test]
    fn test_isolation_validation() {
        let tx = create_test_tx();
        assert!(cell_validation_in_isolation::validate_cell_tx_in_isolation(&tx, CellConsensusParams::default().max_cell_data_size)
            .is_ok());
    }

    #[test]
    fn test_context_validation() {
        let tx = create_test_tx();
        let pov = Hash::from_bytes([1; 32]);
        let mut provider = MockProvider { cells: HashMap::new(), block_timestamps: HashMap::new() };

        // Add a Cell
        let out_point = OutPoint::new([0; 32], 0);
        provider.cells.insert(
            (pov, out_point.clone()),
            CellMetadata {
                out_point: tx_outpoint(&out_point),
                capacity: 100000,
                data_bytes: 0,
                lock_hash: [0; 32],
                type_hash: None,
                data_hash: [0; 32],
                block_daa_score: 50,
                is_cellbase: false,
                block_hash: spora_hashes::Hash::from_bytes([0; 32]),
                lock_code_hash: None,
                type_code_hash: None,
                lock_script: None,
                type_script: None,
                data: None,
            },
        );

        assert!(cell_validation_in_context::validate_cell_tx_in_context(&tx, pov, 100, &provider).is_ok());
    }

    #[test]
    fn test_dag_validation() {
        let tx = create_test_tx();
        let pov = Hash::from_bytes([2; 32]);
        let mut provider = MockProvider { cells: HashMap::new(), block_timestamps: HashMap::new() };

        // Add a cellbase Cell
        let out_point = OutPoint::new([0; 32], 0);
        provider.cells.insert(
            (pov, out_point.clone()),
            CellMetadata {
                out_point: tx_outpoint(&out_point),
                capacity: 100000,
                data_bytes: 0,
                lock_hash: [0; 32],
                type_hash: None,
                data_hash: [0; 32],
                block_daa_score: 50,
                is_cellbase: true,
                block_hash: spora_hashes::Hash::from_bytes([0; 32]),
                lock_code_hash: None,
                type_code_hash: None,
                lock_script: None,
                type_script: None,
                data: None,
            },
        );

        // Should fail: cellbase not mature (created at 50, current 100, maturity 100)
        assert!(cell_validation_in_dag::validate_cellbase_maturity(&tx, pov, 100, 100, &provider).is_err());

        // Should succeed: cellbase mature (created at 50, current 200, maturity 100)
        assert!(cell_validation_in_dag::validate_cellbase_maturity(&tx, pov, 200, 100, &provider).is_ok());
    }

    #[test]
    fn test_cell_validator_integration() {
        let params = Arc::new(CellConsensusParams::default());
        let provider = Arc::new(MockProvider { cells: HashMap::new(), block_timestamps: HashMap::new() });

        let validator = CellValidator::new(params, provider);
        let tx = create_test_tx();

        // Test isolation validation
        assert!(validator.validate_in_isolation(&tx).is_ok());
    }

    #[test]
    fn test_validate_in_dag_accepts_dep_group() {
        use spora_exec::{encode_dep_group_data, CellDep, DepType};

        let pov = Hash::from_bytes([3; 32]);
        let input_out_point = OutPoint::new([1; 32], 0);
        let dep_group_out_point = OutPoint::new([2; 32], 0);
        let expanded_dep_out_point = OutPoint::new([5; 32], 0);
        let block_hash = Hash::from_bytes([4; 32]);
        let lock = ScriptRef::new([0; 32], 0, vec![]);

        // Encode a DepGroup that references one expanded outpoint
        let dep_group_data = encode_dep_group_data(&[expanded_dep_out_point]);

        let mut provider = MockProvider { cells: HashMap::new(), block_timestamps: HashMap::new() };
        provider.cells.insert(
            (pov, input_out_point.clone()),
            CellMetadata {
                out_point: tx_outpoint(&input_out_point),
                capacity: 1_000,
                data_bytes: 0,
                lock_hash: [0; 32],
                type_hash: None,
                data_hash: [0; 32],
                block_daa_score: 0,
                is_cellbase: false,
                block_hash,
                lock_code_hash: None,
                type_code_hash: None,
                lock_script: None,
                type_script: None,
                data: None,
            },
        );
        // The DepGroup cell itself with its encoded data
        provider.cells.insert(
            (pov, dep_group_out_point.clone()),
            CellMetadata {
                out_point: tx_outpoint(&dep_group_out_point),
                capacity: 1_000,
                data_bytes: dep_group_data.len() as u64,
                lock_hash: [0; 32],
                type_hash: None,
                data_hash: [0; 32],
                block_daa_score: 0,
                is_cellbase: false,
                block_hash,
                lock_code_hash: None,
                type_code_hash: None,
                lock_script: None,
                type_script: None,
                data: Some(dep_group_data),
            },
        );
        // The expanded dep cell referenced by the DepGroup
        provider.cells.insert(
            (pov, expanded_dep_out_point.clone()),
            CellMetadata {
                out_point: tx_outpoint(&expanded_dep_out_point),
                capacity: 1_000,
                data_bytes: 0,
                lock_hash: [0; 32],
                type_hash: None,
                data_hash: [0; 32],
                block_daa_score: 0,
                is_cellbase: false,
                block_hash,
                lock_code_hash: None,
                type_code_hash: None,
                lock_script: None,
                type_script: None,
                data: None,
            },
        );
        provider.block_timestamps.insert(block_hash, 0);

        let tx = CellTx::new(
            vec![CellRef::new(input_out_point, 0)],
            vec![CellDep { out_point: dep_group_out_point, dep_type: DepType::DepGroup }],
            vec![CellOut { lock, type_: None, capacity: 1_000 }],
            vec![vec![]],
            vec![],
        )
        .unwrap();

        let validator = CellValidator::new(Arc::new(CellConsensusParams::default()), Arc::new(provider));
        let result = validator.validate_in_dag(&tx, pov, 0, 0);
        assert!(result.is_ok(), "DepGroup should be accepted: {result:?}");
    }

    #[test]
    fn test_validate_in_dag_rejects_dep_group_with_missing_expanded_dep() {
        use crate::processes::cell_validator::CellValidationError;
        use spora_exec::{encode_dep_group_data, CellDep, DepType};

        let pov = Hash::from_bytes([3; 32]);
        let input_out_point = OutPoint::new([1; 32], 0);
        let dep_group_out_point = OutPoint::new([2; 32], 0);
        let missing_dep = OutPoint::new([0xAA; 32], 0);
        let block_hash = Hash::from_bytes([4; 32]);
        let lock = ScriptRef::new([0; 32], 0, vec![]);

        // Encode a DepGroup referencing a cell that does NOT exist
        let dep_group_data = encode_dep_group_data(&[missing_dep]);

        let mut provider = MockProvider { cells: HashMap::new(), block_timestamps: HashMap::new() };
        provider.cells.insert(
            (pov, input_out_point.clone()),
            CellMetadata {
                out_point: tx_outpoint(&input_out_point),
                capacity: 1_000,
                data_bytes: 0,
                lock_hash: [0; 32],
                type_hash: None,
                data_hash: [0; 32],
                block_daa_score: 0,
                is_cellbase: false,
                block_hash,
                lock_code_hash: None,
                type_code_hash: None,
                lock_script: None,
                type_script: None,
                data: None,
            },
        );
        provider.cells.insert(
            (pov, dep_group_out_point.clone()),
            CellMetadata {
                out_point: tx_outpoint(&dep_group_out_point),
                capacity: 1_000,
                data_bytes: dep_group_data.len() as u64,
                lock_hash: [0; 32],
                type_hash: None,
                data_hash: [0; 32],
                block_daa_score: 0,
                is_cellbase: false,
                block_hash,
                lock_code_hash: None,
                type_code_hash: None,
                lock_script: None,
                type_script: None,
                data: Some(dep_group_data),
            },
        );
        provider.block_timestamps.insert(block_hash, 0);

        let tx = CellTx::new(
            vec![CellRef::new(input_out_point, 0)],
            vec![CellDep { out_point: dep_group_out_point, dep_type: DepType::DepGroup }],
            vec![CellOut { lock, type_: None, capacity: 1_000 }],
            vec![vec![]],
            vec![],
        )
        .unwrap();

        let validator = CellValidator::new(Arc::new(CellConsensusParams::default()), Arc::new(provider));
        let result = validator.validate_in_dag(&tx, pov, 0, 0);
        assert!(matches!(result, Err(CellValidationError::DepCellNotFound(_))), "Missing expanded dep should fail: {result:?}");
    }

    #[cfg(feature = "vm")]
    #[test]
    fn test_full_validation_with_real_dep_script_provider() {
        let pov = Hash::from_bytes([7; 32]);
        let input_out_point = OutPoint::new([6; 32], 0);
        let dep_out_point = OutPoint::new([9; 32], 0);
        let code_hash = always_success_code_hash();
        let always_success_lock = ScriptRef::new(code_hash, 0, vec![]);

        let mut provider = MockProvider { cells: HashMap::new(), block_timestamps: HashMap::new() };
        provider.cells.insert(
            (pov, input_out_point.clone()),
            CellMetadata {
                out_point: tx_outpoint(&input_out_point),
                capacity: 1_000,
                data_bytes: 0,
                lock_hash: [0; 32],
                type_hash: None,
                data_hash: [0; 32],
                block_daa_score: 0,
                is_cellbase: false,
                block_hash: Hash::from_bytes([5; 32]),
                lock_code_hash: None,
                type_code_hash: None,
                lock_script: Some(always_success_lock.clone()),
                type_script: None,
                data: Some(vec![]),
            },
        );
        provider.block_timestamps.insert(Hash::from_bytes([5; 32]), 0);
        provider.cells.insert(
            (pov, dep_out_point.clone()),
            CellMetadata {
                out_point: tx_outpoint(&dep_out_point),
                capacity: 1_000,
                data_bytes: ALWAYS_SUCCESS_SCRIPT.len() as u64,
                lock_hash: [0; 32],
                type_hash: None,
                data_hash: [0; 32],
                block_daa_score: 0,
                is_cellbase: false,
                block_hash: Hash::from_bytes([8; 32]),
                lock_code_hash: None,
                type_code_hash: None,
                lock_script: Some(always_success_lock.clone()),
                type_script: None,
                data: Some(ALWAYS_SUCCESS_SCRIPT.to_vec()),
            },
        );
        provider.block_timestamps.insert(Hash::from_bytes([8; 32]), 0);

        let tx = CellTx::new(
            vec![CellRef::new(input_out_point, 0)],
            vec![CellDep { out_point: dep_out_point.clone(), dep_type: DepType::Code }],
            vec![CellOut { lock: ScriptRef::new(code_hash, 0, vec![]), type_: None, capacity: 1_000 }],
            vec![vec![]],
            vec![],
        )
        .unwrap();

        let validator = CellValidator::new(Arc::new(CellConsensusParams::default()), Arc::new(provider));
        let result = validator.validate_full_with_scripts(&tx, pov, 0, 0);
        assert!(result.is_ok(), "{result:?}");
    }

    #[cfg(feature = "vm")]
    #[test]
    fn test_full_validation_with_missing_dep_script_provider_fails() {
        let pov = Hash::from_bytes([7; 32]);
        let input_out_point = OutPoint::new([6; 32], 0);
        let dep_out_point = OutPoint::new([9; 32], 0);
        let code_hash = always_success_code_hash();

        let mut provider = MockProvider { cells: HashMap::new(), block_timestamps: HashMap::new() };
        provider.cells.insert(
            (pov, input_out_point.clone()),
            CellMetadata {
                out_point: tx_outpoint(&input_out_point),
                capacity: 1_000,
                data_bytes: 0,
                lock_hash: [0; 32],
                type_hash: None,
                data_hash: [0; 32],
                block_daa_score: 0,
                is_cellbase: false,
                block_hash: Hash::from_bytes([5; 32]),
                lock_code_hash: None,
                type_code_hash: None,
                lock_script: None,
                type_script: None,
                data: Some(vec![]),
            },
        );
        provider.block_timestamps.insert(Hash::from_bytes([5; 32]), 0);

        let tx = CellTx::new(
            vec![CellRef::new(input_out_point, 0)],
            vec![CellDep { out_point: dep_out_point.clone(), dep_type: DepType::Code }],
            vec![CellOut { lock: ScriptRef::new(code_hash, 0, vec![]), type_: None, capacity: 1_000 }],
            vec![vec![]],
            vec![],
        )
        .unwrap();

        let validator = CellValidator::new(Arc::new(CellConsensusParams::default()), Arc::new(provider));
        let result = validator.validate_full_with_scripts(&tx, pov, 0, 0);
        assert!(matches!(result, Err(CellValidationError::DepCellNotFound(hash)) if hash == dep_out_point.tx_hash), "{result:?}");
    }

    #[cfg(feature = "vm")]
    #[test]
    fn test_verify_scripts_enforces_per_tx_cycles_limit() {
        let pov = Hash::from_bytes([0x21; 32]);
        let input_out_point = OutPoint::new([0x22; 32], 0);
        let dep_out_point = OutPoint::new([0x23; 32], 0);
        let input_block_hash = Hash::from_bytes([0x24; 32]);
        let dep_block_hash = Hash::from_bytes([0x25; 32]);
        let code_hash = always_success_code_hash();
        let always_success_lock = ScriptRef::new(code_hash, 0, vec![]);

        let mut provider = MockProvider { cells: HashMap::new(), block_timestamps: HashMap::new() };
        provider.cells.insert(
            (pov, input_out_point.clone()),
            CellMetadata {
                out_point: tx_outpoint(&input_out_point),
                capacity: 1_000,
                data_bytes: 0,
                lock_hash: [0; 32],
                type_hash: None,
                data_hash: [0; 32],
                block_daa_score: 0,
                is_cellbase: false,
                block_hash: input_block_hash,
                lock_code_hash: None,
                type_code_hash: None,
                lock_script: Some(always_success_lock.clone()),
                type_script: None,
                data: Some(vec![]),
            },
        );
        provider.cells.insert(
            (pov, dep_out_point.clone()),
            CellMetadata {
                out_point: tx_outpoint(&dep_out_point),
                capacity: 1_000,
                data_bytes: ALWAYS_SUCCESS_SCRIPT.len() as u64,
                lock_hash: [0; 32],
                type_hash: None,
                data_hash: [0; 32],
                block_daa_score: 0,
                is_cellbase: false,
                block_hash: dep_block_hash,
                lock_code_hash: None,
                type_code_hash: None,
                lock_script: Some(always_success_lock.clone()),
                type_script: None,
                data: Some(ALWAYS_SUCCESS_SCRIPT.to_vec()),
            },
        );
        provider.block_timestamps.insert(input_block_hash, 0);
        provider.block_timestamps.insert(dep_block_hash, 0);

        let tx = CellTx::new(
            vec![CellRef::new(input_out_point, 0)],
            vec![CellDep { out_point: dep_out_point, dep_type: DepType::Code }],
            vec![CellOut { lock: always_success_lock, type_: None, capacity: 1_000 }],
            vec![vec![]],
            vec![],
        )
        .unwrap();

        let provider = Arc::new(provider);
        let baseline_validator = CellValidator::new(Arc::new(CellConsensusParams::default()), provider.clone());
        let consumed_cycles = baseline_validator.verify_scripts_with_cycles(&tx, pov).expect("always-success should verify");
        assert!(consumed_cycles > 0, "baseline run should consume some cycles");

        let strict_limit = consumed_cycles.saturating_sub(1);
        let strict_params = Arc::new(CellConsensusParams { max_tx_cycles: strict_limit, ..CellConsensusParams::default() });
        let strict_validator = CellValidator::new(strict_params, provider);
        let result = strict_validator.verify_scripts_with_cycles(&tx, pov);

        assert!(
            matches!(result, Err(CellValidationError::ExceededMaxCycles { total, limit }) if total == consumed_cycles && limit == strict_limit),
            "{result:?}"
        );
    }
}
