use std::{collections::HashSet, sync::Arc};

use super::BlockBodyProcessor;
use crate::errors::{BlockProcessResult, RuleError};
use crate::processes::cell_validator::{cell_validation_in_isolation::validate_cell_tx_in_isolation, CellConsensusParams};
use spora_consensus_core::{
    block::Block,
    errors::coinbase::CoinbaseError,
    mass::{ContextualMasses, Mass, NonContextualMasses},
    tx::TransactionOutpoint,
};

impl BlockBodyProcessor {
    /// Validate the block body in isolation (non-contextual checks).
    ///
    /// This is the **sole entry point** for all isolation-level validation:
    /// merkle root, coinbase structure, per-tx format/capacity/data-size checks,
    /// block mass limits, duplicate detection, double-spend detection, and
    /// chained-transaction detection.
    ///
    /// **Contract**: `validate_body_in_context` relies on this function having
    /// already been called and does **not** repeat any isolation checks.
    /// Callers (i.e. `validate_body`) must always invoke this function before
    /// `validate_body_in_context`.
    pub fn validate_body_in_isolation(self: &Arc<Self>, block: &Block) -> BlockProcessResult<Mass> {
        let crescendo_activated = true; // always active

        Self::check_has_transactions(block)?;
        Self::check_hash_merkle_root(block, crescendo_activated)?;
        Self::check_only_one_coinbase(block)?;
        self.check_transactions_in_isolation(block)?;
        self.check_coinbase_has_zero_mass(block, crescendo_activated)?;
        let mass = self.check_block_mass(block, crescendo_activated)?;
        self.check_duplicate_transactions(block)?;
        self.check_block_double_spends(block)?;
        self.check_no_chained_transactions(block)?;
        Ok(mass)
    }

    fn check_has_transactions(block: &Block) -> BlockProcessResult<()> {
        // We expect the outer flow to not queue blocks with no transactions for body validation,
        // but we still check it in case the outer flow changes.
        if block.transactions.is_empty() {
            return Err(RuleError::NoTransactions);
        }
        Ok(())
    }

    fn check_hash_merkle_root(block: &Block, crescendo_activated: bool) -> BlockProcessResult<()> {
        // CellTx merkle root calculation using Cell-specific function
        use spora_consensus_core::merkle::calc_hash_merkle_root_cell;
        let calculated = calc_hash_merkle_root_cell(block.transactions.iter(), crescendo_activated);
        if calculated != block.header.hash_merkle_root {
            return Err(RuleError::BadMerkleRoot(block.header.hash_merkle_root, calculated));
        }
        Ok(())
    }

    fn check_only_one_coinbase(block: &Block) -> BlockProcessResult<()> {
        if !block.transactions[0].is_coinbase() {
            return Err(RuleError::FirstTxNotCoinbase);
        }

        if let Some(i) = block.transactions[1..].iter().position(|tx| tx.is_coinbase()) {
            return Err(RuleError::MultipleCoinbases(i));
        }

        Ok(())
    }

    fn check_transactions_in_isolation(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        for tx in block.transactions.iter() {
            if tx.is_coinbase() {
                continue;
            }

            validate_cell_tx_in_isolation(tx, CellConsensusParams::default().max_cell_data_size)
                .map_err(|e| RuleError::CellValidationError(format!("Isolation validation failed for tx {:?}: {e}", tx.id())))?;
        }
        Ok(())
    }

    fn check_coinbase_has_zero_mass(&self, block: &Block, crescendo_activated: bool) -> BlockProcessResult<()> {
        if !crescendo_activated {
            return Ok(());
        }

        let payload = block.transactions[0].payload().unwrap_or(&[]);
        let coinbase_data = self.coinbase_manager.deserialize_coinbase_payload(payload).map_err(|err| match err {
            CoinbaseError::PayloadLenBelowMin(..)
            | CoinbaseError::PayloadLenAboveMax(..)
            | CoinbaseError::PayloadLockScriptLenAboveMax(..)
            | CoinbaseError::PayloadCantContainLockScript(..) => RuleError::BadCoinbasePayload(err),
        })?;
        if coinbase_data.mass_commitment != 0 {
            return Err(RuleError::CoinbaseNonZeroMassCommitment);
        }
        Ok(())
    }

    fn check_block_mass(self: &Arc<Self>, block: &Block, crescendo_activated: bool) -> BlockProcessResult<Mass> {
        if crescendo_activated {
            let mut total_compute_mass: u64 = 0;
            let mut total_transient_mass: u64 = 0;
            for tx in block.transactions.iter() {
                let non_contextual_masses = self.mass_calculator.calc_non_contextual_masses_cell(tx);
                let compute_mass = non_contextual_masses.compute_mass;
                let transient_mass = non_contextual_masses.transient_mass;

                // Sum over the various masses separately
                total_compute_mass = total_compute_mass.saturating_add(compute_mass);
                total_transient_mass = total_transient_mass.saturating_add(transient_mass);

                // Verify all limits
                if total_compute_mass > self.max_block_mass {
                    return Err(RuleError::ExceedsComputeMassLimit(total_compute_mass, self.max_block_mass));
                }
                if total_transient_mass > self.max_block_mass {
                    return Err(RuleError::ExceedsTransientMassLimit(total_transient_mass, self.max_block_mass));
                }
            }
            Ok((NonContextualMasses::new(total_compute_mass, total_transient_mass), ContextualMasses::new(0)))
        } else {
            let mut total_mass: u64 = 0;
            for tx in block.transactions.iter() {
                let compute_mass = self.mass_calculator.calc_non_contextual_masses_cell(tx).compute_mass;
                total_mass = total_mass.saturating_add(compute_mass);
                if total_mass > self.max_block_mass {
                    return Err(RuleError::ExceedsComputeMassLimit(total_mass, self.max_block_mass));
                }
            }
            Ok((NonContextualMasses::new(total_mass, 0), ContextualMasses::new(0)))
        }
    }

    fn check_block_double_spends(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        let mut existing = HashSet::new();
        for input in block.transactions.iter().flat_map(|tx| &tx.inputs) {
            // Convert OutPoint to TransactionOutpoint
            let txout = TransactionOutpoint { tx_hash: input.previous_output.tx_hash, index: input.previous_output.index };
            if !existing.insert(txout.clone()) {
                return Err(RuleError::DoubleSpendInSameBlock(txout));
            }
        }
        Ok(())
    }

    fn check_no_chained_transactions(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        let mut block_created_outpoints = HashSet::new();
        for tx in block.transactions.iter() {
            for index in 0..tx.outputs.len() {
                block_created_outpoints.insert(TransactionOutpoint { tx_hash: tx.id(), index: index as u32 });
            }
        }

        for input in block.transactions.iter().flat_map(|tx| &tx.inputs) {
            // Convert OutPoint to TransactionOutpoint
            let txout = TransactionOutpoint { tx_hash: input.previous_output.tx_hash, index: input.previous_output.index };
            if block_created_outpoints.contains(&txout) {
                return Err(RuleError::ChainedTransaction(txout));
            }
        }
        Ok(())
    }

    fn check_duplicate_transactions(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        let mut ids = HashSet::new();
        for tx in block.transactions.iter() {
            if !ids.insert(tx.id()) {
                return Err(RuleError::DuplicateTransactions(tx.id().into()));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        config::{Config, ConfigBuilder},
        consensus::test_consensus::TestConsensus,
        errors::RuleError,
        params::MAINNET_PARAMS,
    };
    use spora_consensus_core::{
        api::{BlockValidationFutures, ConsensusApi},
        block::MutableBlock,
        header::Header,
        merkle::calc_hash_merkle_root_cell as calc_hash_merkle_root_with_options,
    };
    use spora_core::assert_match;
    use spora_exec::{CellInput, CellOutput, CellTx, OutPoint, Script};
    use spora_hashes::Hash;

    fn calc_hash_merkle_root<'a>(txs: impl ExactSizeIterator<Item = &'a CellTx>) -> Hash {
        calc_hash_merkle_root_with_options(txs, false)
    }

    fn make_lock(byte: u8) -> Script {
        Script::new([byte; 32], 0, vec![])
    }

    #[test]
    fn validate_body_in_isolation_test() {
        let consensus = TestConsensus::new(&Config::new(MAINNET_PARAMS));
        let wait_handles = consensus.init();

        let body_processor = consensus.block_body_processor();

        // Build CellTx transactions directly (no Transaction conversion)
        let lock = make_lock(0);
        let coinbase_payload = consensus
            .services
            .coinbase_manager
            .serialize_coinbase_payload(&spora_consensus_core::coinbase::CoinbaseData {
                blue_score: 9,
                subsidy: 0x12a05f200,
                mass_commitment: 0,
                miner_data: spora_consensus_core::coinbase::MinerData { lock_script: lock.clone(), extra_data: &[] },
            })
            .unwrap();
        let coinbase = CellTx::new(
            vec![],
            vec![],
            vec![CellOutput { lock: lock.clone(), type_: None, capacity: 0x12a05f200 }],
            vec![coinbase_payload],
            vec![],
        )
        .unwrap();
        let tx1 = CellTx::new(
            vec![CellInput::new(OutPoint::new([0x16; 32], 0xffffffff), 0), CellInput::new(OutPoint::new([0x4b; 32], 0xffffffff), 0)],
            vec![],
            vec![CellOutput { lock: lock.clone(), type_: None, capacity: 1000 }],
            vec![vec![]],
            vec![vec![], vec![]],
        )
        .unwrap();
        let tx2 = CellTx::new(
            vec![CellInput::new(OutPoint::new([0x03; 32], 0), 0)],
            vec![],
            vec![
                CellOutput { lock: lock.clone(), type_: None, capacity: 0x2123e300 },
                CellOutput { lock: lock.clone(), type_: None, capacity: 0x108e20f00 },
            ],
            vec![vec![], vec![]],
            vec![vec![0x49, 0x30]],
        )
        .unwrap();
        let tx3 = CellTx::new(
            vec![CellInput::new(OutPoint::new([0xc3; 32], 1), 0)],
            vec![],
            vec![
                CellOutput { lock: lock.clone(), type_: None, capacity: 0xf4240 },
                CellOutput { lock: lock.clone(), type_: None, capacity: 0x11d260c0 },
            ],
            vec![vec![], vec![]],
            vec![vec![0x47, 0x30]],
        )
        .unwrap();
        let tx4 = CellTx::new(
            vec![CellInput::new(OutPoint::new([0x0b; 32], 0), 0)],
            vec![],
            vec![CellOutput { lock: lock.clone(), type_: None, capacity: 0xf4240 }],
            vec![vec![]],
            vec![vec![0x49, 0x30]],
        )
        .unwrap();

        let mut example_block = MutableBlock::new(
            Header::new_finalized(
                0,
                vec![vec![
                    Hash::from_slice(&[
                        0x16, 0x5e, 0x38, 0xe8, 0xb3, 0x91, 0x45, 0x95, 0xd9, 0xc6, 0x41, 0xf3, 0xb8, 0xee, 0xc2, 0xf3, 0x46, 0x11,
                        0x89, 0x6b, 0x82, 0x1a, 0x68, 0x3b, 0x7a, 0x4e, 0xde, 0xfe, 0x2c, 0x00, 0x00, 0x00,
                    ]),
                    Hash::from_slice(&[
                        0x4b, 0xb0, 0x75, 0x35, 0xdf, 0xd5, 0x8e, 0x0b, 0x3c, 0xd6, 0x4f, 0xd7, 0x15, 0x52, 0x80, 0x87, 0x2a, 0x04,
                        0x71, 0xbc, 0xf8, 0x30, 0x95, 0x52, 0x6a, 0xce, 0x0e, 0x38, 0xc6, 0x00, 0x00, 0x00,
                    ]),
                ]],
                // Hard-coded manually modified
                Hash::from_slice(&[
                    0x04, 0xa0, 0x6c, 0x77, 0xe6, 0x8a, 0xb1, 0xb1, 0xd9, 0xde, 0xe5, 0xec, 0xd8, 0x83, 0x48, 0x78, 0x96, 0x01, 0x94,
                    0x14, 0x4e, 0x66, 0x03, 0xde, 0xfb, 0xea, 0x73, 0xf0, 0x40, 0x16, 0xfe, 0x5f,
                ]),
                Default::default(),
                Default::default(),
                Default::default(), // cell_root
                Default::default(), // segment_root
                0x17305aa654a,
                0x207fffff,
                1,
                0,
                0.into(),
                9,
                Default::default(),
            ),
            vec![coinbase, tx1, tx2, tx3, tx4],
        );
        example_block.header.hash_merkle_root = calc_hash_merkle_root(example_block.transactions.iter());

        body_processor.validate_body_in_isolation(&example_block.clone().to_immutable()).unwrap();

        let mut block = example_block.clone();
        let txs = &mut block.transactions;
        txs[1].version += 1;
        assert_match!(body_processor.validate_body_in_isolation(&block.to_immutable()), Err(RuleError::BadMerkleRoot(_, _)));

        let mut block = example_block.clone();
        let txs = &mut block.transactions;
        txs[1].witnesses.push(vec![0; 2_000_000]);
        block.header.hash_merkle_root = calc_hash_merkle_root(txs.iter());
        assert_match!(body_processor.validate_body_in_isolation(&block.to_immutable()), Err(RuleError::ExceedsComputeMassLimit(_, _)));

        let mut block = example_block.clone();
        let txs = &mut block.transactions;
        txs.push(txs[1].clone());
        block.header.hash_merkle_root = calc_hash_merkle_root(txs.iter());
        assert_match!(body_processor.validate_body_in_isolation(&block.to_immutable()), Err(RuleError::DuplicateTransactions(_)));

        let mut block = example_block.clone();
        let txs = &mut block.transactions;
        txs[1].inputs.clear();
        block.header.hash_merkle_root = calc_hash_merkle_root(txs.iter());
        assert_match!(body_processor.validate_body_in_isolation(&block.to_immutable()), Err(RuleError::MultipleCoinbases(_)));

        let mut block = example_block.clone();
        let txs = &mut block.transactions;
        txs[2].inputs[0].previous_output = txs[1].inputs[0].previous_output.clone();
        block.header.hash_merkle_root = calc_hash_merkle_root(txs.iter());
        assert_match!(body_processor.validate_body_in_isolation(&block.to_immutable()), Err(RuleError::DoubleSpendInSameBlock(_)));

        let mut block = example_block.clone();
        let txs = &mut block.transactions;
        txs[0].inputs.push(CellInput::new(OutPoint::new([1; 32], 0), 0));
        block.header.hash_merkle_root = calc_hash_merkle_root(txs.iter());
        assert_match!(body_processor.validate_body_in_isolation(&block.to_immutable()), Err(RuleError::FirstTxNotCoinbase));

        let mut block = example_block.clone();
        let txs = &mut block.transactions;
        txs[1].outputs_data.clear();
        block.header.hash_merkle_root = calc_hash_merkle_root(txs.iter());
        assert_match!(body_processor.validate_body_in_isolation(&block.to_immutable()), Err(RuleError::CellValidationError(_)));

        let mut block = example_block.clone();
        let txs = &mut block.transactions;
        txs[0].outputs_data[0] = consensus
            .services
            .coinbase_manager
            .serialize_coinbase_payload(&spora_consensus_core::coinbase::CoinbaseData {
                blue_score: 9,
                subsidy: 0x12a05f200,
                mass_commitment: 1,
                miner_data: spora_consensus_core::coinbase::MinerData { lock_script: lock.clone(), extra_data: &[] },
            })
            .unwrap();
        block.header.hash_merkle_root = calc_hash_merkle_root(txs.iter());
        assert_match!(body_processor.validate_body_in_isolation(&block.to_immutable()), Err(RuleError::CoinbaseNonZeroMassCommitment));

        let mut block = example_block;
        let txs = &mut block.transactions;
        txs[3].inputs[0].previous_output = OutPoint::new(txs[2].id(), 0);
        block.header.hash_merkle_root = calc_hash_merkle_root(txs.iter());
        assert_match!(body_processor.validate_body_in_isolation(&block.to_immutable()), Err(RuleError::ChainedTransaction(_)));

        consensus.shutdown(wait_handles);
    }

    #[tokio::test]
    async fn merkle_root_missing_parents_known_invalid_test() {
        let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
        let consensus = TestConsensus::new(&config);
        let wait_handles = consensus.init();

        let mut block = consensus.build_block_with_parents_and_transactions(1.into(), vec![config.genesis.hash], vec![]);
        block.transactions[0].version += 1;

        let BlockValidationFutures { block_task, virtual_state_task } =
            consensus.validate_and_insert_block(block.clone().to_immutable());

        assert_match!(block_task.await, Err(RuleError::BadMerkleRoot(_, _)));
        // Assert that both tasks return the same error
        assert_match!(virtual_state_task.await, Err(RuleError::BadMerkleRoot(_, _)));

        // BadMerkleRoot shouldn't mark the block as known invalid
        assert_match!(
            consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await,
            Err(RuleError::BadMerkleRoot(_, _))
        );

        let mut block = consensus.build_block_with_parents_and_transactions(1.into(), vec![config.genesis.hash], vec![]);
        block.header.parents_by_level[0][0] = 0.into();

        assert_match!(
            consensus.validate_and_insert_block(block.clone().to_immutable()).virtual_state_task.await,
            Err(RuleError::MissingParents(_))
        );

        // MissingParents shouldn't mark the block as known invalid
        assert_match!(
            consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await,
            Err(RuleError::MissingParents(_))
        );

        consensus.shutdown(wait_handles);
    }
}
