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
    #[cfg(feature = "vm")]
    use crate::test_helpers::{always_success_cell_metadata, always_success_lock_script};
    #[cfg(feature = "vm")]
    use secp256k1::Keypair;
    #[cfg(feature = "vm")]
    use spora_addresses::{Address, Prefix};
    use spora_consensus_core::{cell_metadata::CellMetadata, tx::TransactionOutpoint};
    #[cfg(feature = "vm")]
    use spora_consensus_core::{
        sign::{sign, sign_with_multiple_v2},
        tx::{pay_to_address_lock_script, MutableTransaction},
    };
    #[cfg(feature = "vm")]
    use spora_exec::{CellDep, DepType};
    use spora_exec::{CellInput, CellOutput, CellTx, OutPoint, Script};
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
                version: 1,
                parents_by_level: vec![],
                hash_merkle_root: [0; 32],
                accepted_id_merkle_root: [0; 32],
                cell_commitment: [0; 32],
                cell_root: [0; 32],
                segment_root: [0; 32],
                timestamp,
                bits: 0,
                nonce: 0,
                daa_score: 0,
                blue_work: [0; 24],
                blue_score: 0,
                pruning_point: [0; 32],
            }))
        }
    }

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

    fn tx_outpoint(out_point: &OutPoint) -> TransactionOutpoint {
        TransactionOutpoint { tx_hash: out_point.tx_hash, index: out_point.index }
    }

    #[cfg(feature = "vm")]
    fn sub_be(lhs: &[u8; 32], rhs: &[u8; 32]) -> [u8; 32] {
        let mut out = [0u8; 32];
        let mut borrow = 0i16;
        for i in (0..32).rev() {
            let diff = lhs[i] as i16 - rhs[i] as i16 - borrow;
            if diff < 0 {
                out[i] = (diff + 256) as u8;
                borrow = 1;
            } else {
                out[i] = diff as u8;
                borrow = 0;
            }
        }
        out
    }

    #[cfg(feature = "vm")]
    fn ecdsa_high_s_compact(signature: [u8; 64]) -> [u8; 64] {
        // secp256k1 curve order (big-endian)
        const CURVE_ORDER: [u8; 32] = [
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFE, 0xBA, 0xAE, 0xDC, 0xE6,
            0xAF, 0x48, 0xA0, 0x3B, 0xBF, 0xD2, 0x5E, 0x8C, 0xD0, 0x36, 0x41, 0x41,
        ];

        let mut out = signature;
        let mut s = [0u8; 32];
        s.copy_from_slice(&signature[32..64]);
        let high_s = sub_be(&CURVE_ORDER, &s);
        out[32..64].copy_from_slice(&high_s);
        out
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
        let lock = Script::new([0; 32], 0, vec![]);

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
            vec![CellInput::new(input_out_point, 0)],
            vec![CellDep { out_point: dep_group_out_point, dep_type: DepType::DepGroup }],
            vec![CellOutput { lock, type_: None, capacity: 1_000 }],
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
        let lock = Script::new([0; 32], 0, vec![]);

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
            vec![CellInput::new(input_out_point, 0)],
            vec![CellDep { out_point: dep_group_out_point, dep_type: DepType::DepGroup }],
            vec![CellOutput { lock, type_: None, capacity: 1_000 }],
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
        let always_success_lock = always_success_lock_script();

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
        provider.cells.insert((pov, dep_out_point.clone()), always_success_cell_metadata(&dep_out_point, Hash::from_bytes([8; 32])));
        provider.block_timestamps.insert(Hash::from_bytes([8; 32]), 0);

        let tx = CellTx::new(
            vec![CellInput::new(input_out_point, 0)],
            vec![CellDep { out_point: dep_out_point.clone(), dep_type: DepType::Code }],
            vec![CellOutput { lock: always_success_lock.clone(), type_: None, capacity: 1_000 }],
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
        let code_hash = always_success_lock_script().code_hash;

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
            vec![CellInput::new(input_out_point, 0)],
            vec![CellDep { out_point: dep_out_point.clone(), dep_type: DepType::Code }],
            vec![CellOutput { lock: Script::new(code_hash, 0, vec![]), type_: None, capacity: 1_000 }],
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
        let always_success_lock = always_success_lock_script();

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
        provider.cells.insert((pov, dep_out_point.clone()), always_success_cell_metadata(&dep_out_point, dep_block_hash));
        provider.block_timestamps.insert(input_block_hash, 0);
        provider.block_timestamps.insert(dep_block_hash, 0);

        let tx = CellTx::new(
            vec![CellInput::new(input_out_point, 0)],
            vec![CellDep { out_point: dep_out_point, dep_type: DepType::Code }],
            vec![CellOutput { lock: always_success_lock.clone(), type_: None, capacity: 1_000 }],
            vec![vec![]],
            vec![],
        )
        .unwrap();

        let provider = Arc::new(provider);
        let baseline_validator = CellValidator::new(Arc::new(CellConsensusParams::default()), provider.clone());
        let consumed_cycles = baseline_validator.verify_scripts_with_cycles(&tx, pov, 0).expect("always-success should verify");
        assert!(consumed_cycles > 0, "baseline run should consume some cycles");

        let strict_limit = consumed_cycles.saturating_sub(1);
        let strict_params = Arc::new(CellConsensusParams { max_tx_cycles: strict_limit, ..CellConsensusParams::default() });
        let strict_validator = CellValidator::new(strict_params, provider);
        let result = strict_validator.verify_scripts_with_cycles(&tx, pov, 0);

        assert!(
            matches!(result, Err(CellValidationError::ExceededMaxCycles { total, limit }) if total == consumed_cycles && limit == strict_limit),
            "{result:?}"
        );
    }

    #[cfg(feature = "vm")]
    #[test]
    fn test_full_validation_with_native_pubkey_lock_and_no_code_dep() {
        let pov = Hash::from_bytes([0x31; 32]);
        let input_out_point = OutPoint::new([0x32; 32], 0);
        let block_hash = Hash::from_bytes([0x33; 32]);
        let secret_key = [0x44; 32];
        let keypair = Keypair::from_seckey_slice(secp256k1::SECP256K1, &secret_key).expect("valid secret key");
        let pubkey = keypair.public_key().x_only_public_key().0.serialize();
        let address = Address::new_std_single(Prefix::Testnet, &pubkey).expect("valid address");
        let lock_script = pay_to_address_lock_script(&address);

        let resolved_input = CellMetadata {
            out_point: tx_outpoint(&input_out_point),
            capacity: 10_000,
            data_bytes: 0,
            lock_hash: lock_script.hash(),
            type_hash: None,
            data_hash: [0; 32],
            block_daa_score: 0,
            is_cellbase: false,
            block_hash,
            lock_code_hash: None,
            type_code_hash: None,
            lock_script: Some(lock_script.clone()),
            type_script: None,
            data: Some(vec![]),
        };

        let unsigned_tx = CellTx::new(
            vec![CellInput::new(input_out_point, 0)],
            vec![],
            vec![CellOutput { lock: lock_script.clone(), type_: None, capacity: 9_000 }],
            vec![vec![]],
            vec![vec![]],
        )
        .unwrap();
        let signed_tx = sign(MutableTransaction::with_resolved_metadata(unsigned_tx, vec![resolved_input.clone()]), keypair).tx;

        let mut provider = MockProvider { cells: HashMap::new(), block_timestamps: HashMap::new() };
        provider.cells.insert((pov, input_out_point), resolved_input);
        provider.block_timestamps.insert(block_hash, 0);

        let validator = CellValidator::new(Arc::new(CellConsensusParams::default()), Arc::new(provider));
        let result = validator.validate_full_with_scripts_and_cycles(&signed_tx, pov, 0, 0).expect("native pubkey verification");
        assert!(result > 0, "native verification should be charged cycles");
    }

    #[cfg(feature = "vm")]
    #[test]
    fn test_native_pubkey_lock_enforces_per_tx_cycles_limit() {
        let pov = Hash::from_bytes([0x41; 32]);
        let input_out_point = OutPoint::new([0x42; 32], 0);
        let block_hash = Hash::from_bytes([0x43; 32]);
        let secret_key = [0x44; 32];
        let keypair = Keypair::from_seckey_slice(secp256k1::SECP256K1, &secret_key).expect("valid secret key");
        let pubkey = keypair.public_key().x_only_public_key().0.serialize();
        let address = Address::new_std_single(Prefix::Testnet, &pubkey).expect("valid address");
        let lock_script = pay_to_address_lock_script(&address);

        let resolved_input = CellMetadata {
            out_point: tx_outpoint(&input_out_point),
            capacity: 10_000,
            data_bytes: 0,
            lock_hash: lock_script.hash(),
            type_hash: None,
            data_hash: [0; 32],
            block_daa_score: 0,
            is_cellbase: false,
            block_hash,
            lock_code_hash: None,
            type_code_hash: None,
            lock_script: Some(lock_script.clone()),
            type_script: None,
            data: Some(vec![]),
        };

        let unsigned_tx = CellTx::new(
            vec![CellInput::new(input_out_point, 0)],
            vec![],
            vec![CellOutput { lock: lock_script.clone(), type_: None, capacity: 9_000 }],
            vec![vec![]],
            vec![vec![]],
        )
        .unwrap();
        let signed_tx = sign(MutableTransaction::with_resolved_metadata(unsigned_tx, vec![resolved_input.clone()]), keypair).tx;

        let mut provider = MockProvider { cells: HashMap::new(), block_timestamps: HashMap::new() };
        provider.cells.insert((pov, input_out_point), resolved_input);
        provider.block_timestamps.insert(block_hash, 0);
        let provider = Arc::new(provider);

        let baseline_validator = CellValidator::new(Arc::new(CellConsensusParams::default()), provider.clone());
        let baseline_cycles =
            baseline_validator.validate_full_with_scripts_and_cycles(&signed_tx, pov, 0, 0).expect("native pubkey verification");
        assert!(baseline_cycles > 0, "native path should consume cycles");

        let strict_limit = baseline_cycles.saturating_sub(1);
        let strict_params = Arc::new(CellConsensusParams { max_tx_cycles: strict_limit, ..CellConsensusParams::default() });
        let strict_validator = CellValidator::new(strict_params, provider);
        let result = strict_validator.validate_full_with_scripts_and_cycles(&signed_tx, pov, 0, 0);
        assert!(
            matches!(result, Err(CellValidationError::ExceededMaxCycles { total, limit }) if total == baseline_cycles && limit == strict_limit),
            "{result:?}"
        );
    }

    #[cfg(feature = "vm")]
    #[test]
    fn test_full_validation_with_native_pubkey_lock_and_type_script_runs_vm_type_only() {
        let pov = Hash::from_bytes([0x34; 32]);
        let input_out_point = OutPoint::new([0x35; 32], 0);
        let dep_out_point = OutPoint::new([0x36; 32], 0);
        let input_block_hash = Hash::from_bytes([0x37; 32]);
        let dep_block_hash = Hash::from_bytes([0x38; 32]);
        let secret_key = [0x39; 32];
        let keypair = Keypair::from_seckey_slice(secp256k1::SECP256K1, &secret_key).expect("valid secret key");
        let pubkey = keypair.public_key().x_only_public_key().0.serialize();
        let address = Address::new_std_single(Prefix::Testnet, &pubkey).expect("valid address");
        let lock_script = pay_to_address_lock_script(&address);
        let type_script = always_success_lock_script();

        let resolved_input = CellMetadata {
            out_point: tx_outpoint(&input_out_point),
            capacity: 10_000,
            data_bytes: 0,
            lock_hash: lock_script.hash(),
            type_hash: None,
            data_hash: [0; 32],
            block_daa_score: 0,
            is_cellbase: false,
            block_hash: input_block_hash,
            lock_code_hash: None,
            type_code_hash: None,
            lock_script: Some(lock_script.clone()),
            type_script: None,
            data: Some(vec![]),
        };

        let unsigned_tx = CellTx::new(
            vec![CellInput::new(input_out_point, 0)],
            vec![CellDep { out_point: dep_out_point, dep_type: DepType::Code }],
            vec![CellOutput { lock: lock_script.clone(), type_: Some(type_script), capacity: 9_000 }],
            vec![vec![]],
            vec![vec![]],
        )
        .unwrap();
        let signed_tx = sign(MutableTransaction::with_resolved_metadata(unsigned_tx, vec![resolved_input.clone()]), keypair).tx;

        let mut provider = MockProvider { cells: HashMap::new(), block_timestamps: HashMap::new() };
        provider.cells.insert((pov, input_out_point), resolved_input);
        provider.cells.insert((pov, dep_out_point.clone()), always_success_cell_metadata(&dep_out_point, dep_block_hash));
        provider.block_timestamps.insert(input_block_hash, 0);
        provider.block_timestamps.insert(dep_block_hash, 0);

        let validator = CellValidator::new(Arc::new(CellConsensusParams::default()), Arc::new(provider));
        let consumed = validator
            .validate_full_with_scripts_and_cycles(&signed_tx, pov, 0, 0)
            .expect("native lock should verify and VM should validate type script");
        assert!(consumed > 0, "type script path should consume VM cycles");
    }

    #[cfg(feature = "vm")]
    #[test]
    fn test_mixed_native_and_vm_path_enforces_total_per_tx_cycles_limit() {
        let pov = Hash::from_bytes([0x3A; 32]);
        let input_out_point = OutPoint::new([0x3B; 32], 0);
        let dep_out_point = OutPoint::new([0x3C; 32], 0);
        let input_block_hash = Hash::from_bytes([0x3D; 32]);
        let dep_block_hash = Hash::from_bytes([0x3E; 32]);
        let secret_key = [0x3F; 32];
        let keypair = Keypair::from_seckey_slice(secp256k1::SECP256K1, &secret_key).expect("valid secret key");
        let pubkey = keypair.public_key().x_only_public_key().0.serialize();
        let address = Address::new_std_single(Prefix::Testnet, &pubkey).expect("valid address");
        let lock_script = pay_to_address_lock_script(&address);
        let type_script = always_success_lock_script();

        let resolved_input = CellMetadata {
            out_point: tx_outpoint(&input_out_point),
            capacity: 10_000,
            data_bytes: 0,
            lock_hash: lock_script.hash(),
            type_hash: None,
            data_hash: [0; 32],
            block_daa_score: 0,
            is_cellbase: false,
            block_hash: input_block_hash,
            lock_code_hash: None,
            type_code_hash: None,
            lock_script: Some(lock_script.clone()),
            type_script: None,
            data: Some(vec![]),
        };

        let unsigned_tx = CellTx::new(
            vec![CellInput::new(input_out_point, 0)],
            vec![CellDep { out_point: dep_out_point.clone(), dep_type: DepType::Code }],
            vec![CellOutput { lock: lock_script.clone(), type_: Some(type_script), capacity: 9_000 }],
            vec![vec![]],
            vec![vec![]],
        )
        .unwrap();
        let signed_tx = sign(MutableTransaction::with_resolved_metadata(unsigned_tx, vec![resolved_input.clone()]), keypair).tx;

        let mut provider = MockProvider { cells: HashMap::new(), block_timestamps: HashMap::new() };
        provider.cells.insert((pov, input_out_point), resolved_input);
        provider.cells.insert((pov, dep_out_point.clone()), always_success_cell_metadata(&dep_out_point, dep_block_hash));
        provider.block_timestamps.insert(input_block_hash, 0);
        provider.block_timestamps.insert(dep_block_hash, 0);
        let provider = Arc::new(provider);

        let baseline_validator = CellValidator::new(Arc::new(CellConsensusParams::default()), provider.clone());
        let baseline_cycles =
            baseline_validator.validate_full_with_scripts_and_cycles(&signed_tx, pov, 0, 0).expect("mixed native+vm verification");
        assert!(baseline_cycles > 0, "mixed path should consume cycles");

        let strict_limit = baseline_cycles.saturating_sub(1);
        let strict_params = Arc::new(CellConsensusParams { max_tx_cycles: strict_limit, ..CellConsensusParams::default() });
        let strict_validator = CellValidator::new(strict_params, provider);
        let result = strict_validator.validate_full_with_scripts_and_cycles(&signed_tx, pov, 0, 0);
        assert!(
            matches!(result, Err(CellValidationError::ExceededMaxCycles { total, limit }) if total == baseline_cycles && limit == strict_limit),
            "{result:?}"
        );
    }

    #[cfg(feature = "vm")]
    #[test]
    fn test_full_validation_with_mixed_native_and_vm_locks_skips_only_native_groups() {
        let pov = Hash::from_bytes([0x81; 32]);
        let std_input_out_point = OutPoint::new([0x82; 32], 0);
        let vm_input_out_point = OutPoint::new([0x83; 32], 0);
        let dep_out_point = OutPoint::new([0x84; 32], 0);
        let std_block_hash = Hash::from_bytes([0x85; 32]);
        let vm_block_hash = Hash::from_bytes([0x86; 32]);
        let dep_block_hash = Hash::from_bytes([0x87; 32]);
        let secret_key = [0x88; 32];
        let keypair = Keypair::from_seckey_slice(secp256k1::SECP256K1, &secret_key).expect("valid secret key");
        let pubkey = keypair.public_key().x_only_public_key().0.serialize();
        let native_lock = pay_to_address_lock_script(&Address::new_std_single(Prefix::Testnet, &pubkey).expect("valid address"));
        let vm_lock = always_success_lock_script();

        let native_resolved_input = CellMetadata {
            out_point: tx_outpoint(&std_input_out_point),
            capacity: 10_000,
            data_bytes: 0,
            lock_hash: native_lock.hash(),
            type_hash: None,
            data_hash: [0; 32],
            block_daa_score: 0,
            is_cellbase: false,
            block_hash: std_block_hash,
            lock_code_hash: None,
            type_code_hash: None,
            lock_script: Some(native_lock.clone()),
            type_script: None,
            data: Some(vec![]),
        };
        let vm_resolved_input = CellMetadata {
            out_point: tx_outpoint(&vm_input_out_point),
            capacity: 6_000,
            data_bytes: 0,
            lock_hash: vm_lock.hash(),
            type_hash: None,
            data_hash: [0; 32],
            block_daa_score: 0,
            is_cellbase: false,
            block_hash: vm_block_hash,
            lock_code_hash: None,
            type_code_hash: None,
            lock_script: Some(vm_lock.clone()),
            type_script: None,
            data: Some(vec![]),
        };

        let unsigned_tx = CellTx::new(
            vec![CellInput::new(std_input_out_point, 0), CellInput::new(vm_input_out_point, 0)],
            vec![CellDep { out_point: dep_out_point.clone(), dep_type: DepType::Code }],
            vec![
                CellOutput { lock: native_lock.clone(), type_: None, capacity: 9_000 },
                CellOutput { lock: vm_lock.clone(), type_: None, capacity: 7_000 },
            ],
            vec![vec![], vec![]],
            vec![vec![], vec![]],
        )
        .unwrap();
        let signed_tx = sign(
            MutableTransaction::with_resolved_metadata(unsigned_tx, vec![native_resolved_input.clone(), vm_resolved_input.clone()]),
            keypair,
        )
        .tx;

        let mut provider = MockProvider { cells: HashMap::new(), block_timestamps: HashMap::new() };
        provider.cells.insert((pov, std_input_out_point), native_resolved_input);
        provider.cells.insert((pov, vm_input_out_point), vm_resolved_input);
        provider.cells.insert((pov, dep_out_point.clone()), always_success_cell_metadata(&dep_out_point, dep_block_hash));
        provider.block_timestamps.insert(std_block_hash, 0);
        provider.block_timestamps.insert(vm_block_hash, 0);
        provider.block_timestamps.insert(dep_block_hash, 0);

        let validator = CellValidator::new(Arc::new(CellConsensusParams::default()), Arc::new(provider));
        let consumed = validator
            .validate_full_with_scripts_and_cycles(&signed_tx, pov, 0, 0)
            .expect("native standard lock should be verified natively while VM lock still runs in VM");
        assert!(consumed > 0, "non-standard VM lock should still consume cycles");
    }

    #[cfg(feature = "vm")]
    #[test]
    fn test_full_validation_with_mixed_native_and_vm_locks_rejects_bad_native_signature() {
        let pov = Hash::from_bytes([0x91; 32]);
        let std_input_out_point = OutPoint::new([0x92; 32], 0);
        let vm_input_out_point = OutPoint::new([0x93; 32], 0);
        let dep_out_point = OutPoint::new([0x94; 32], 0);
        let std_block_hash = Hash::from_bytes([0x95; 32]);
        let vm_block_hash = Hash::from_bytes([0x96; 32]);
        let dep_block_hash = Hash::from_bytes([0x97; 32]);
        let secret_key = [0x98; 32];
        let keypair = Keypair::from_seckey_slice(secp256k1::SECP256K1, &secret_key).expect("valid secret key");
        let pubkey = keypair.public_key().x_only_public_key().0.serialize();
        let native_lock = pay_to_address_lock_script(&Address::new_std_single(Prefix::Testnet, &pubkey).expect("valid address"));
        let vm_lock = always_success_lock_script();

        let native_resolved_input = CellMetadata {
            out_point: tx_outpoint(&std_input_out_point),
            capacity: 10_000,
            data_bytes: 0,
            lock_hash: native_lock.hash(),
            type_hash: None,
            data_hash: [0; 32],
            block_daa_score: 0,
            is_cellbase: false,
            block_hash: std_block_hash,
            lock_code_hash: None,
            type_code_hash: None,
            lock_script: Some(native_lock.clone()),
            type_script: None,
            data: Some(vec![]),
        };
        let vm_resolved_input = CellMetadata {
            out_point: tx_outpoint(&vm_input_out_point),
            capacity: 6_000,
            data_bytes: 0,
            lock_hash: vm_lock.hash(),
            type_hash: None,
            data_hash: [0; 32],
            block_daa_score: 0,
            is_cellbase: false,
            block_hash: vm_block_hash,
            lock_code_hash: None,
            type_code_hash: None,
            lock_script: Some(vm_lock.clone()),
            type_script: None,
            data: Some(vec![]),
        };

        let unsigned_tx = CellTx::new(
            vec![CellInput::new(std_input_out_point, 0), CellInput::new(vm_input_out_point, 0)],
            vec![CellDep { out_point: dep_out_point.clone(), dep_type: DepType::Code }],
            vec![
                CellOutput { lock: native_lock.clone(), type_: None, capacity: 9_000 },
                CellOutput { lock: vm_lock.clone(), type_: None, capacity: 7_000 },
            ],
            vec![vec![], vec![]],
            vec![vec![], vec![]],
        )
        .unwrap();
        let mut signed_tx = sign(
            MutableTransaction::with_resolved_metadata(unsigned_tx, vec![native_resolved_input.clone(), vm_resolved_input.clone()]),
            keypair,
        )
        .tx;
        signed_tx.witnesses[0] = vec![0u8; 65];

        let mut provider = MockProvider { cells: HashMap::new(), block_timestamps: HashMap::new() };
        provider.cells.insert((pov, std_input_out_point), native_resolved_input);
        provider.cells.insert((pov, vm_input_out_point), vm_resolved_input);
        provider.cells.insert((pov, dep_out_point.clone()), always_success_cell_metadata(&dep_out_point, dep_block_hash));
        provider.block_timestamps.insert(std_block_hash, 0);
        provider.block_timestamps.insert(vm_block_hash, 0);
        provider.block_timestamps.insert(dep_block_hash, 0);

        let validator = CellValidator::new(Arc::new(CellConsensusParams::default()), Arc::new(provider));
        let result = validator.validate_full_with_scripts_and_cycles(&signed_tx, pov, 0, 0);
        assert!(
            matches!(result, Err(CellValidationError::InvalidSignature) | Err(CellValidationError::ScriptVerificationFailed(_))),
            "{result:?}"
        );
    }

    #[cfg(feature = "vm")]
    #[test]
    fn test_full_validation_rejects_invalid_native_pubkey_signature() {
        let pov = Hash::from_bytes([0x41; 32]);
        let input_out_point = OutPoint::new([0x42; 32], 0);
        let block_hash = Hash::from_bytes([0x43; 32]);
        let address = Address::new_std_single(Prefix::Testnet, &[0x55; 32]).expect("valid address");
        let lock_script = pay_to_address_lock_script(&address);

        let resolved_input = CellMetadata {
            out_point: tx_outpoint(&input_out_point),
            capacity: 10_000,
            data_bytes: 0,
            lock_hash: lock_script.hash(),
            type_hash: None,
            data_hash: [0; 32],
            block_daa_score: 0,
            is_cellbase: false,
            block_hash,
            lock_code_hash: None,
            type_code_hash: None,
            lock_script: Some(lock_script.clone()),
            type_script: None,
            data: Some(vec![]),
        };

        let tx = CellTx::new(
            vec![CellInput::new(input_out_point, 0)],
            vec![],
            vec![CellOutput { lock: lock_script.clone(), type_: None, capacity: 9_000 }],
            vec![vec![]],
            vec![vec![0u8; 65]],
        )
        .unwrap();

        let mut provider = MockProvider { cells: HashMap::new(), block_timestamps: HashMap::new() };
        provider.cells.insert((pov, input_out_point), resolved_input);
        provider.block_timestamps.insert(block_hash, 0);

        let validator = CellValidator::new(Arc::new(CellConsensusParams::default()), Arc::new(provider));
        let result = validator.validate_full_with_scripts_and_cycles(&tx, pov, 0, 0);
        assert!(
            matches!(result, Err(CellValidationError::InvalidSignature) | Err(CellValidationError::ScriptVerificationFailed(_))),
            "{result:?}"
        );
    }

    #[cfg(feature = "vm")]
    #[test]
    fn test_full_validation_rejects_unversioned_standard_witness_envelope() {
        let pov = Hash::from_bytes([0x44; 32]);
        let input_out_point = OutPoint::new([0x45; 32], 0);
        let block_hash = Hash::from_bytes([0x46; 32]);
        let secret_key = [0x47; 32];
        let keypair = Keypair::from_seckey_slice(secp256k1::SECP256K1, &secret_key).expect("valid secret key");
        let pubkey = keypair.public_key().x_only_public_key().0.serialize();
        let lock_script = pay_to_address_lock_script(&Address::new_std_single(Prefix::Testnet, &pubkey).expect("valid address"));

        let resolved_input = CellMetadata {
            out_point: tx_outpoint(&input_out_point),
            capacity: 10_000,
            data_bytes: 0,
            lock_hash: lock_script.hash(),
            type_hash: None,
            data_hash: [0; 32],
            block_daa_score: 0,
            is_cellbase: false,
            block_hash,
            lock_code_hash: None,
            type_code_hash: None,
            lock_script: Some(lock_script.clone()),
            type_script: None,
            data: Some(vec![]),
        };

        let unsigned_tx = CellTx::new(
            vec![CellInput::new(input_out_point, 0)],
            vec![],
            vec![CellOutput { lock: lock_script.clone(), type_: None, capacity: 9_000 }],
            vec![vec![]],
            vec![vec![]],
        )
        .unwrap();
        let mut signed_tx = sign_with_multiple_v2(
            MutableTransaction::with_resolved_metadata(unsigned_tx, vec![resolved_input.clone()]),
            &[secret_key],
        )
        .fully_signed()
        .expect("native signing should succeed")
        .tx;
        assert_eq!(signed_tx.witnesses[0].first().copied(), Some(1), "signed witness should be versioned envelope");
        signed_tx.witnesses[0][0] = 0;

        let mut provider = MockProvider { cells: HashMap::new(), block_timestamps: HashMap::new() };
        provider.cells.insert((pov, input_out_point), resolved_input);
        provider.block_timestamps.insert(block_hash, 0);

        let validator = CellValidator::new(Arc::new(CellConsensusParams::default()), Arc::new(provider));
        let result = validator.validate_full_with_scripts_and_cycles(&signed_tx, pov, 0, 0);
        assert!(
            matches!(result, Err(CellValidationError::ScriptVerificationFailed(ref msg)) if msg.contains("expected versioned envelope")),
            "{result:?}"
        );
    }

    #[cfg(feature = "vm")]
    #[test]
    fn test_full_validation_rejects_invalid_sighash_type_in_standard_witness_envelope() {
        let pov = Hash::from_bytes([0x48; 32]);
        let input_out_point = OutPoint::new([0x49; 32], 0);
        let block_hash = Hash::from_bytes([0x4A; 32]);
        let secret_key = [0x4B; 32];
        let keypair = Keypair::from_seckey_slice(secp256k1::SECP256K1, &secret_key).expect("valid secret key");
        let pubkey = keypair.public_key().x_only_public_key().0.serialize();
        let lock_script = pay_to_address_lock_script(&Address::new_std_single(Prefix::Testnet, &pubkey).expect("valid address"));

        let resolved_input = CellMetadata {
            out_point: tx_outpoint(&input_out_point),
            capacity: 10_000,
            data_bytes: 0,
            lock_hash: lock_script.hash(),
            type_hash: None,
            data_hash: [0; 32],
            block_daa_score: 0,
            is_cellbase: false,
            block_hash,
            lock_code_hash: None,
            type_code_hash: None,
            lock_script: Some(lock_script.clone()),
            type_script: None,
            data: Some(vec![]),
        };

        let unsigned_tx = CellTx::new(
            vec![CellInput::new(input_out_point, 0)],
            vec![],
            vec![CellOutput { lock: lock_script.clone(), type_: None, capacity: 9_000 }],
            vec![vec![]],
            vec![vec![]],
        )
        .unwrap();
        let mut signed_tx = sign_with_multiple_v2(
            MutableTransaction::with_resolved_metadata(unsigned_tx, vec![resolved_input.clone()]),
            &[secret_key],
        )
        .fully_signed()
        .expect("native signing should succeed")
        .tx;
        signed_tx.witnesses[0][1] = 0xff;

        let mut provider = MockProvider { cells: HashMap::new(), block_timestamps: HashMap::new() };
        provider.cells.insert((pov, input_out_point), resolved_input);
        provider.block_timestamps.insert(block_hash, 0);

        let validator = CellValidator::new(Arc::new(CellConsensusParams::default()), Arc::new(provider));
        let result = validator.validate_full_with_scripts_and_cycles(&signed_tx, pov, 0, 0);
        assert!(
            matches!(result, Err(CellValidationError::ScriptVerificationFailed(ref msg)) if msg.contains("invalid sighash type")),
            "{result:?}"
        );
    }

    #[cfg(feature = "vm")]
    #[test]
    fn test_full_validation_rejects_unversioned_standard_ecdsa_witness_envelope() {
        let pov = Hash::from_bytes([0x4C; 32]);
        let input_out_point = OutPoint::new([0x4D; 32], 0);
        let block_hash = Hash::from_bytes([0x4E; 32]);
        let secret_key = [0x4F; 32];
        let keypair = Keypair::from_seckey_slice(secp256k1::SECP256K1, &secret_key).expect("valid secret key");
        let pubkey = keypair.public_key().serialize();
        let lock_script = pay_to_address_lock_script(&Address::new_std_single_ecdsa(Prefix::Testnet, &pubkey).expect("valid address"));

        let resolved_input = CellMetadata {
            out_point: tx_outpoint(&input_out_point),
            capacity: 10_000,
            data_bytes: 0,
            lock_hash: lock_script.hash(),
            type_hash: None,
            data_hash: [0; 32],
            block_daa_score: 0,
            is_cellbase: false,
            block_hash,
            lock_code_hash: None,
            type_code_hash: None,
            lock_script: Some(lock_script.clone()),
            type_script: None,
            data: Some(vec![]),
        };

        let unsigned_tx = CellTx::new(
            vec![CellInput::new(input_out_point, 0)],
            vec![],
            vec![CellOutput { lock: lock_script.clone(), type_: None, capacity: 9_000 }],
            vec![vec![]],
            vec![vec![]],
        )
        .unwrap();
        let mut signed_tx = sign_with_multiple_v2(
            MutableTransaction::with_resolved_metadata(unsigned_tx, vec![resolved_input.clone()]),
            &[secret_key],
        )
        .fully_signed()
        .expect("native ecdsa signing should succeed")
        .tx;
        assert_eq!(signed_tx.witnesses[0].first().copied(), Some(1), "signed witness should be versioned envelope");
        signed_tx.witnesses[0][0] = 0;

        let mut provider = MockProvider { cells: HashMap::new(), block_timestamps: HashMap::new() };
        provider.cells.insert((pov, input_out_point), resolved_input);
        provider.block_timestamps.insert(block_hash, 0);

        let validator = CellValidator::new(Arc::new(CellConsensusParams::default()), Arc::new(provider));
        let result = validator.validate_full_with_scripts_and_cycles(&signed_tx, pov, 0, 0);
        assert!(
            matches!(result, Err(CellValidationError::ScriptVerificationFailed(ref msg)) if msg.contains("expected versioned envelope")),
            "{result:?}"
        );
    }

    #[cfg(feature = "vm")]
    #[test]
    fn test_full_validation_with_native_pubkey_ecdsa_lock_and_no_code_dep() {
        let pov = Hash::from_bytes([0x51; 32]);
        let input_out_point = OutPoint::new([0x52; 32], 0);
        let block_hash = Hash::from_bytes([0x53; 32]);
        let secret_key = [0x54; 32];
        let keypair = Keypair::from_seckey_slice(secp256k1::SECP256K1, &secret_key).expect("valid secret key");
        let pubkey = keypair.public_key().serialize();
        let address = Address::new_std_single_ecdsa(Prefix::Testnet, &pubkey).expect("valid address");
        let lock_script = pay_to_address_lock_script(&address);

        let resolved_input = CellMetadata {
            out_point: tx_outpoint(&input_out_point),
            capacity: 10_000,
            data_bytes: 0,
            lock_hash: lock_script.hash(),
            type_hash: None,
            data_hash: [0; 32],
            block_daa_score: 0,
            is_cellbase: false,
            block_hash,
            lock_code_hash: None,
            type_code_hash: None,
            lock_script: Some(lock_script.clone()),
            type_script: None,
            data: Some(vec![]),
        };

        let unsigned_tx = CellTx::new(
            vec![CellInput::new(input_out_point, 0)],
            vec![],
            vec![CellOutput { lock: lock_script.clone(), type_: None, capacity: 9_000 }],
            vec![vec![]],
            vec![vec![]],
        )
        .unwrap();
        let signed_tx = sign_with_multiple_v2(
            MutableTransaction::with_resolved_metadata(unsigned_tx, vec![resolved_input.clone()]),
            &[secret_key],
        )
        .fully_signed()
        .expect("ecdsa native signing should succeed")
        .tx;

        let mut provider = MockProvider { cells: HashMap::new(), block_timestamps: HashMap::new() };
        provider.cells.insert((pov, input_out_point), resolved_input);
        provider.block_timestamps.insert(block_hash, 0);

        let validator = CellValidator::new(Arc::new(CellConsensusParams::default()), Arc::new(provider));
        let result = validator.validate_full_with_scripts_and_cycles(&signed_tx, pov, 0, 0).expect("native ecdsa verification");
        assert!(result > 0, "native verification should be charged cycles");
    }

    #[cfg(feature = "vm")]
    #[test]
    fn test_full_validation_rejects_high_s_standard_ecdsa_signature() {
        let pov = Hash::from_bytes([0x81; 32]);
        let input_out_point = OutPoint::new([0x82; 32], 0);
        let block_hash = Hash::from_bytes([0x83; 32]);
        let secret_key = [0x84; 32];
        let keypair = Keypair::from_seckey_slice(secp256k1::SECP256K1, &secret_key).expect("valid secret key");
        let pubkey = keypair.public_key().serialize();
        let address = Address::new_std_single_ecdsa(Prefix::Testnet, &pubkey).expect("valid address");
        let lock_script = pay_to_address_lock_script(&address);

        let resolved_input = CellMetadata {
            out_point: tx_outpoint(&input_out_point),
            capacity: 10_000,
            data_bytes: 0,
            lock_hash: lock_script.hash(),
            type_hash: None,
            data_hash: [0; 32],
            block_daa_score: 0,
            is_cellbase: false,
            block_hash,
            lock_code_hash: None,
            type_code_hash: None,
            lock_script: Some(lock_script.clone()),
            type_script: None,
            data: Some(vec![]),
        };

        let unsigned_tx = CellTx::new(
            vec![CellInput::new(input_out_point, 0)],
            vec![],
            vec![CellOutput { lock: lock_script.clone(), type_: None, capacity: 9_000 }],
            vec![vec![]],
            vec![vec![]],
        )
        .unwrap();
        let mut signed_tx = sign_with_multiple_v2(
            MutableTransaction::with_resolved_metadata(unsigned_tx, vec![resolved_input.clone()]),
            &[secret_key],
        )
        .fully_signed()
        .expect("ecdsa native signing should succeed")
        .tx;

        let witness = signed_tx.witnesses.get_mut(0).expect("witness");
        assert_eq!(witness.first().copied(), Some(1), "standard witness envelope must be versioned");
        let pubkey_len = usize::from(witness[2]);
        let sig_len_offset = 3 + pubkey_len;
        assert_eq!(witness[sig_len_offset], 64, "ecdsa envelope should carry compact 64-byte signature");
        let sig_start = sig_len_offset + 1;
        let sig_end = sig_start + 64;
        let mut compact = [0u8; 64];
        compact.copy_from_slice(&witness[sig_start..sig_end]);
        let high_s = ecdsa_high_s_compact(compact);
        witness[sig_start..sig_end].copy_from_slice(&high_s);

        let mut provider = MockProvider { cells: HashMap::new(), block_timestamps: HashMap::new() };
        provider.cells.insert((pov, input_out_point), resolved_input);
        provider.block_timestamps.insert(block_hash, 0);

        let validator = CellValidator::new(Arc::new(CellConsensusParams::default()), Arc::new(provider));
        let result = validator.validate_full_with_scripts_and_cycles(&signed_tx, pov, 0, 0);
        assert!(matches!(result, Err(CellValidationError::ScriptVerificationFailed(ref msg)) if msg.contains("high-S")), "{result:?}");
    }
}
