// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Cell transaction validation specific to DAG consensus

use super::cell_validation_in_context::CellStateProvider;
use super::errors::CellValidationError;
use spora_consensus_core::cell_metadata::CellMetadata;
use spora_exec::{parse_dep_group_data_for_abi, CellTx, DepGroupDataAbi, DepType, OutPoint};
use spora_hashes::Hash;

/// Extended state provider for DAG validation
pub trait DagCellProvider: CellStateProvider {
    /// Query Cell state at an explicit point-of-view block.
    fn get_cell_at_pov(&self, out_point: &OutPoint, pov: Hash) -> Result<Option<CellMetadata>, String>;

    /// Return the timestamp of a specific block hash.
    fn get_block_timestamp(&self, block_hash: Hash) -> Result<u64, String>;
}

/// Validate cellbase maturity
///
/// Cellbase outputs cannot be spent until they mature (DAA score threshold)
/// This prevents miners from spending rewards in case of reorg
pub fn validate_cellbase_maturity<P: DagCellProvider>(
    tx: &CellTx,
    pov: Hash,
    current_daa: u64,
    maturity: u64,
    provider: &P,
) -> Result<(), CellValidationError> {
    for input in &tx.inputs {
        let meta = provider
            .get_cell_at_pov(&input.previous_output, pov)
            .map_err(|e| CellValidationError::InvalidFormat(e))?
            .ok_or_else(|| CellValidationError::CellNotFound(input.previous_output.tx_hash))?;

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
/// Ensures all referenced Cells exist in the state snapshot defined by `pov`.
pub fn validate_cell_existence<P: DagCellProvider>(tx: &CellTx, pov: Hash, provider: &P) -> Result<(), CellValidationError> {
    validate_cell_existence_for_dep_group_abi(tx, pov, provider, DepGroupDataAbi::Spora)
}

/// Validate Cell existence using an explicit DepGroup cell-data ABI.
pub fn validate_cell_existence_for_dep_group_abi<P: DagCellProvider>(
    tx: &CellTx,
    pov: Hash,
    provider: &P,
    dep_group_data_abi: DepGroupDataAbi,
) -> Result<(), CellValidationError> {
    // Check all inputs exist
    for input in &tx.inputs {
        let available = provider.is_cell_available(&input.previous_output, pov).map_err(|e| CellValidationError::InvalidFormat(e))?;

        if !available {
            return Err(CellValidationError::CellNotFound(input.previous_output.tx_hash));
        }
    }

    // Check all deps exist
    for dep in &tx.cell_deps {
        // The dep cell itself must exist regardless of type
        let available = provider.is_cell_available(&dep.out_point, pov).map_err(|e| CellValidationError::InvalidFormat(e))?;
        if !available {
            return Err(CellValidationError::DepCellNotFound(dep.out_point.tx_hash));
        }

        if dep.dep_type == DepType::DepGroup {
            // Expand: read the DepGroup cell's data and verify every referenced OutPoint
            let meta = provider
                .get_cell_at_pov(&dep.out_point, pov)
                .map_err(CellValidationError::InvalidFormat)?
                .ok_or(CellValidationError::DepCellNotFound(dep.out_point.tx_hash))?;
            let data = meta.data.ok_or_else(|| {
                CellValidationError::InvalidFormat(format!("DepGroup cell data not available for {}", dep.out_point))
            })?;
            let outpoints = parse_dep_group_data_for_abi(&data, dep_group_data_abi).map_err(CellValidationError::InvalidFormat)?;
            for op in &outpoints {
                let ok = provider.is_cell_available(op, pov).map_err(|e| CellValidationError::InvalidFormat(e))?;
                if !ok {
                    return Err(CellValidationError::DepCellNotFound(op.tx_hash));
                }
            }
        }
    }

    Ok(())
}

/// Validate all input `since` constraints against the explicit POV state snapshot.
pub fn validate_time_locks<P: DagCellProvider>(
    tx: &CellTx,
    pov: Hash,
    current_daa: u64,
    current_timestamp: u64,
    provider: &P,
) -> Result<(), CellValidationError> {
    for input in &tx.inputs {
        if input.since == 0 {
            continue;
        }

        let meta = provider
            .get_cell_at_pov(&input.previous_output, pov)
            .map_err(|e| CellValidationError::InvalidFormat(e))?
            .ok_or_else(|| CellValidationError::CellNotFound(input.previous_output.tx_hash))?;

        let is_relative = (input.since & 0x8000_0000_0000_0000) != 0;
        let is_daa = (input.since & 0x4000_0000_0000_0000) != 0;
        let lock_value = input.since & 0x3FFF_FFFF_FFFF_FFFF;

        let current_value = if is_daa { current_daa } else { current_timestamp };

        let required_value = if is_relative {
            let base_value = if is_daa {
                meta.block_daa_score
            } else {
                provider.get_block_timestamp(meta.block_hash).map_err(CellValidationError::InvalidFormat)?
            };

            base_value.checked_add(lock_value).ok_or_else(|| CellValidationError::InvalidFormat("time lock overflow".to_string()))?
        } else {
            lock_value
        };

        if current_value < required_value {
            return Err(CellValidationError::TimeLockNotSatisfied { lock_value: required_value, current: current_value });
        }
    }

    Ok(())
}

/// Validate transaction in reorg context
///
/// When applying a block after reorg, check that:
/// - All inputs were unspent in the explicit POV state snapshot
/// - No conflicts with the new chain tip
pub fn validate_in_reorg_context<P: DagCellProvider>(
    tx: &CellTx,
    pov: Hash,
    block_daa: u64,
    provider: &P,
) -> Result<(), CellValidationError> {
    validate_in_reorg_context_for_dep_group_abi(tx, pov, block_daa, provider, DepGroupDataAbi::Spora)
}

/// Validate transaction in reorg context using an explicit DepGroup cell-data ABI.
pub fn validate_in_reorg_context_for_dep_group_abi<P: DagCellProvider>(
    tx: &CellTx,
    pov: Hash,
    block_daa: u64,
    provider: &P,
    dep_group_data_abi: DepGroupDataAbi,
) -> Result<(), CellValidationError> {
    validate_cell_existence_for_dep_group_abi(tx, pov, provider, dep_group_data_abi)?;

    for input in &tx.inputs {
        let meta = provider
            .get_cell_at_pov(&input.previous_output, pov)
            .map_err(|e| CellValidationError::InvalidFormat(e))?
            .ok_or_else(|| CellValidationError::CellNotFound(input.previous_output.tx_hash))?;

        // Cell must have been created before or at this block
        if meta.block_daa_score > block_daa {
            return Err(CellValidationError::CellNotYetCreated { created_daa: meta.block_daa_score, spent_at_daa: block_daa });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use spora_consensus_core::tx::TransactionOutpoint;
    use spora_exec::{CellInput, CellOutput, Script};
    use std::collections::HashMap;

    struct MockDagProvider {
        cells: HashMap<(Hash, OutPoint), CellMetadata>,
    }

    impl CellStateProvider for MockDagProvider {
        fn is_cell_available(&self, out_point: &OutPoint, pov: Hash) -> Result<bool, String> {
            Ok(self.cells.contains_key(&(pov, out_point.clone())))
        }

        fn get_cell_capacity(&self, out_point: &OutPoint, pov: Hash) -> Result<Option<u64>, String> {
            Ok(self.cells.get(&(pov, out_point.clone())).map(|meta| meta.capacity))
        }
    }

    impl DagCellProvider for MockDagProvider {
        fn get_cell_at_pov(&self, out_point: &OutPoint, pov: Hash) -> Result<Option<CellMetadata>, String> {
            Ok(self.cells.get(&(pov, out_point.clone())).cloned())
        }

        fn get_block_timestamp(&self, _block_hash: Hash) -> Result<u64, String> {
            Ok(1_000)
        }
    }

    fn tx_outpoint(out_point: &OutPoint) -> TransactionOutpoint {
        TransactionOutpoint { tx_hash: out_point.tx_hash, index: out_point.index }
    }

    #[test]
    fn test_cellbase_maturity() {
        let out_point = OutPoint::new([1; 32], 0);
        let pov = Hash::from_bytes([9; 32]);
        let mut provider = MockDagProvider { cells: HashMap::new() };

        // Add a cellbase Cell created at DAA 50
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

        let lock = Script::new([0; 32], 0, vec![]);
        let tx = CellTx::new(
            vec![CellInput::new(out_point, 0)],
            vec![],
            vec![CellOutput { lock, type_: None, capacity: 90000 }],
            vec![vec![]],
            vec![],
        )
        .unwrap();

        // Should fail: current DAA = 100, required = 150
        assert!(validate_cellbase_maturity(&tx, pov, 100, 100, &provider).is_err());

        // Should succeed: current DAA = 150
        assert!(validate_cellbase_maturity(&tx, pov, 150, 100, &provider).is_ok());

        // Should succeed: current DAA = 200
        assert!(validate_cellbase_maturity(&tx, pov, 200, 100, &provider).is_ok());
    }

    #[test]
    fn test_non_cellbase_no_maturity() {
        let out_point = OutPoint::new([2; 32], 0);
        let pov = Hash::from_bytes([8; 32]);
        let mut provider = MockDagProvider { cells: HashMap::new() };

        // Add a regular Cell (not cellbase)
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

        let lock = Script::new([0; 32], 0, vec![]);
        let tx = CellTx::new(
            vec![CellInput::new(out_point, 0)],
            vec![],
            vec![CellOutput { lock, type_: None, capacity: 90000 }],
            vec![vec![]],
            vec![],
        )
        .unwrap();

        // Should succeed immediately (no maturity for regular Cells)
        assert!(validate_cellbase_maturity(&tx, pov, 51, 100, &provider).is_ok());
    }

    #[test]
    fn test_reorg_validation() {
        let out_point = OutPoint::new([3; 32], 0);
        let pov = Hash::from_bytes([7; 32]);
        let mut provider = MockDagProvider { cells: HashMap::new() };

        // Cell created at DAA 100
        provider.cells.insert(
            (pov, out_point.clone()),
            CellMetadata {
                out_point: tx_outpoint(&out_point),
                capacity: 100000,
                data_bytes: 0,
                lock_hash: [0; 32],
                type_hash: None,
                data_hash: [0; 32],
                block_daa_score: 100,
                is_cellbase: false,
                block_hash: spora_hashes::Hash::from_bytes([0; 32]),
                lock_code_hash: None,
                type_code_hash: None,
                lock_script: None,
                type_script: None,
                data: None,
            },
        );

        let lock = Script::new([0; 32], 0, vec![]);
        let tx = CellTx::new(
            vec![CellInput::new(out_point, 0)],
            vec![],
            vec![CellOutput { lock, type_: None, capacity: 90000 }],
            vec![vec![]],
            vec![],
        )
        .unwrap();

        // Tx in block at DAA 150: should succeed
        assert!(validate_in_reorg_context(&tx, pov, 150, &provider).is_ok());

        // Tx in block at DAA 99: should fail (Cell not yet created)
        assert!(validate_in_reorg_context(&tx, pov, 99, &provider).is_err());
    }

    #[test]
    fn test_time_lock_validation_variants() {
        let out_point = OutPoint::new([4; 32], 0);
        let created_block = Hash::from_bytes([5; 32]);
        let pov = Hash::from_bytes([6; 32]);
        let mut provider = MockDagProvider { cells: HashMap::new() };

        provider.cells.insert(
            (pov, out_point.clone()),
            CellMetadata {
                out_point: tx_outpoint(&out_point),
                capacity: 100000,
                data_bytes: 0,
                lock_hash: [0; 32],
                type_hash: None,
                data_hash: [0; 32],
                block_daa_score: 100,
                is_cellbase: false,
                block_hash: created_block,
                lock_code_hash: None,
                type_code_hash: None,
                lock_script: None,
                type_script: None,
                data: None,
            },
        );

        let lock = Script::new([0; 32], 0, vec![]);
        let absolute_daa = CellTx::new(
            vec![CellInput::new(out_point.clone(), 0x4000_0000_0000_0096)],
            vec![],
            vec![CellOutput { lock: lock.clone(), type_: None, capacity: 90000 }],
            vec![vec![]],
            vec![],
        )
        .unwrap();
        assert!(validate_time_locks(&absolute_daa, pov, 149, 1_100, &provider).is_err());
        assert!(validate_time_locks(&absolute_daa, pov, 150, 1_100, &provider).is_ok());

        let relative_daa = CellTx::new(
            vec![CellInput::new(out_point.clone(), 0xC000_0000_0000_000A)],
            vec![],
            vec![CellOutput { lock: lock.clone(), type_: None, capacity: 90000 }],
            vec![vec![]],
            vec![],
        )
        .unwrap();
        assert!(validate_time_locks(&relative_daa, pov, 109, 1_100, &provider).is_err());
        assert!(validate_time_locks(&relative_daa, pov, 110, 1_100, &provider).is_ok());

        let absolute_timestamp = CellTx::new(
            vec![CellInput::new(out_point.clone(), 0x0000_0000_0000_044C)],
            vec![],
            vec![CellOutput { lock: lock.clone(), type_: None, capacity: 90000 }],
            vec![vec![]],
            vec![],
        )
        .unwrap();
        assert!(validate_time_locks(&absolute_timestamp, pov, 150, 1_099, &provider).is_err());
        assert!(validate_time_locks(&absolute_timestamp, pov, 150, 1_100, &provider).is_ok());

        let relative_timestamp = CellTx::new(
            vec![CellInput::new(out_point, 0x8000_0000_0000_0032)],
            vec![],
            vec![CellOutput { lock, type_: None, capacity: 90000 }],
            vec![vec![]],
            vec![],
        )
        .unwrap();
        assert!(validate_time_locks(&relative_timestamp, pov, 150, 1_049, &provider).is_err());
        assert!(validate_time_locks(&relative_timestamp, pov, 150, 1_050, &provider).is_ok());
    }
}
