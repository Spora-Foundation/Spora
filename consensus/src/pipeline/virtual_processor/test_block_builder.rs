use std::{ops::Deref, sync::Arc};

use crate::{
    model::stores::{
        block_transactions::BlockTransactionsStoreReader, cell_roots::CellRootsStoreReader, headers::HeaderStoreReader,
        pruning::PruningStoreReader, virtual_state::VirtualStateStoreReader,
    },
    processes::window::WindowManager,
};
use spora_consensus_core::{
    block::BlockTemplate,
    blockhash::ORIGIN,
    coinbase::{BlockRewardData, MinerData},
    errors::block::RuleError,
    merkle::calc_hash_merkle_root_cell,
    BlockHashMap,
};
use spora_exec::{CellTx, ScriptRef};
use spora_hashes::Hash;

use super::VirtualStateProcessor;

/// Wrapper for virtual processor with util methods for building a block with any parent context
pub struct TestBlockBuilder {
    processor: Arc<VirtualStateProcessor>,
}

impl Deref for TestBlockBuilder {
    type Target = VirtualStateProcessor;

    fn deref(&self) -> &Self::Target {
        &self.processor
    }
}

impl TestBlockBuilder {
    fn rebuild_coinbase_for_header_context(
        &self,
        template: &mut BlockTemplate,
        miner_data: &MinerData,
        parents: &[Hash],
    ) -> Result<(), RuleError> {
        let ghostdag_data = self.ghostdag_manager.ghostdag(parents);
        let mut mergeset_rewards = BlockHashMap::default();

        for block_hash in ghostdag_data.mergeset_blues.iter().chain(ghostdag_data.mergeset_reds.iter()).copied() {
            if mergeset_rewards.contains_key(&block_hash) {
                continue;
            }

            let block_daa_score = self.headers_store.get_daa_score(block_hash).unwrap();
            let reward_data = self
                .block_transactions_store
                .get(block_hash)
                .ok()
                .and_then(|txs| {
                    txs.first()
                        .and_then(|tx| tx.payload())
                        .and_then(|payload| self.coinbase_manager.deserialize_coinbase_payload(payload).ok())
                        .map(|payload| BlockRewardData::new(payload.subsidy, 0, payload.miner_data.lock_script.clone()))
                })
                .unwrap_or_else(|| {
                    BlockRewardData::new(
                        self.coinbase_manager.calc_block_subsidy(block_daa_score),
                        0,
                        ScriptRef::new([0; 32], 0, vec![]),
                    )
                });
            mergeset_rewards.insert(block_hash, reward_data);
        }

        let coinbase = self
            .coinbase_manager
            .expected_coinbase_transaction(
                template.block.header.daa_score,
                miner_data.clone(),
                &ghostdag_data,
                &mergeset_rewards,
                &Default::default(),
            )
            .expect("coinbase transaction creation must succeed")
            .tx;
        if template.block.transactions.is_empty() {
            template.block.transactions.push(coinbase);
        } else {
            template.block.transactions[0] = coinbase;
        }

        let storage_mass_activated = true;
        template.block.header.hash_merkle_root =
            calc_hash_merkle_root_cell(template.block.transactions.iter(), storage_mass_activated);
        template.selected_parent_hash = ghostdag_data.selected_parent;
        template.selected_parent_timestamp = self.headers_store.get_timestamp(template.selected_parent_hash).unwrap();
        template.selected_parent_daa_score = self.headers_store.get_daa_score(template.selected_parent_hash).unwrap();

        Ok(())
    }

    pub fn new(processor: Arc<VirtualStateProcessor>) -> Self {
        Self { processor }
    }

    fn build_block_template_with_parents_impl(
        &self,
        parents: Vec<Hash>,
        miner_data: MinerData,
        txs: Vec<CellTx>,
        validate_transactions: bool,
    ) -> Result<BlockTemplate, RuleError> {
        //
        // In the context of this method "pov virtual" is the virtual block which has `parents` as tips and not the actual virtual
        //
        let pruning_point = self.pruning_point_store.read().pruning_point().unwrap();
        let virtual_read = self.virtual_stores.read();
        let virtual_state = virtual_read.state.get().unwrap();
        let finality_point = ORIGIN; // No real finality point since we are not actually building virtual here
        let sink = virtual_state.ghostdag_data.selected_parent;
        let mut accumulated_diff = virtual_state.cell_diff.clone().reverse();
        // Search for the sink block from the PoV of this virtual
        let (pov_sink, virtual_parent_candidates) =
            self.sink_search_algorithm(&virtual_read, &mut accumulated_diff, sink, parents.clone(), finality_point, pruning_point);
        let virtual_parent_candidates =
            self.filter_virtual_parent_candidates(&virtual_read, &accumulated_diff, pov_sink, virtual_parent_candidates);
        let (pov_virtual_parents, pov_virtual_ghostdag_data) =
            self.pick_virtual_parents(pov_sink, virtual_parent_candidates, pruning_point);
        let pov_sink_cell_root = self.cell_roots_store.get(pov_sink).unwrap();
        let pov_virtual_state = match self.calculate_virtual_state(
            &virtual_read,
            pov_virtual_parents.clone(),
            pov_virtual_ghostdag_data,
            pov_sink_cell_root,
            &mut accumulated_diff,
        ) {
            Ok(state) => state,
            Err(_) => self.calculate_virtual_state(
                &virtual_read,
                vec![pov_sink],
                self.ghostdag_manager.ghostdag(&[pov_sink]),
                pov_sink_cell_root,
                &mut accumulated_diff,
            )?,
        };
        if validate_transactions {
            self.validate_block_template_cell_transactions(&txs, &pov_virtual_state)?;
        }
        drop(virtual_read);
        let mut template = self.build_block_template_from_virtual_state_cell(pov_virtual_state, miner_data.clone(), txs, vec![])?;
        let ghostdag_data = self.ghostdag_manager.ghostdag(&parents);
        let pruning_info = self.pruning_point_store.read().get().unwrap();
        let daa_window = self.window_manager.block_daa_window(&ghostdag_data).unwrap();
        let timestamp = self.window_manager.calc_past_median_time(&ghostdag_data).unwrap().0 + 1;

        template.block.header.parents_by_level = vec![parents.clone()];
        template.block.header.pruning_point =
            self.pruning_point_manager.expected_header_pruning_point_v1(ghostdag_data.to_compact(), pruning_info);
        template.block.header.bits = self.window_manager.calculate_difficulty_bits(&ghostdag_data, &daa_window);
        template.block.header.daa_score = daa_window.daa_score;
        template.block.header.timestamp = timestamp;
        template.block.header.blue_score = ghostdag_data.blue_score;
        template.block.header.blue_work = ghostdag_data.blue_work;
        self.rebuild_coinbase_for_header_context(&mut template, &miner_data, &parents)?;
        template.block.header.finalize();

        Ok(template)
    }

    /// Test-only helper method for building a block template with specific parents
    pub(crate) fn build_block_template_with_parents(
        &self,
        parents: Vec<Hash>,
        miner_data: MinerData,
        txs: Vec<CellTx>,
    ) -> Result<BlockTemplate, RuleError> {
        self.build_block_template_with_parents_impl(parents, miner_data, txs, true)
    }

    pub(crate) fn build_block_template_with_parents_unchecked(
        &self,
        parents: Vec<Hash>,
        miner_data: MinerData,
        txs: Vec<CellTx>,
    ) -> Result<BlockTemplate, RuleError> {
        self.build_block_template_with_parents_impl(parents, miner_data, txs, false)
    }
}
