use crate::{
    block_template::{builder::BlockTemplateBuilder, errors::BuilderError},
    cache::BlockTemplateCache,
    errors::MiningManagerResult,
    feerate::{FeeEstimateVerbose, FeerateEstimations, FeerateEstimatorArgs},
    mempool::{
        config::Config,
        model::tx::{MempoolTransaction, TransactionPostValidation, TransactionPreValidation, TxRemovalReason},
        populate_entries_and_try_validate::{validate_mempool_cell_transaction, validate_mempool_mempool_transactions_in_parallel},
        tx::{Orphan, Priority, RbfPolicy},
        Mempool,
    },
    model::{
        owner_txs::{AddressSet, GroupedOwnerTransactions},
        topological_sort::IntoIterTopologically,
        tx_insert::TransactionInsertion,
        tx_query::TransactionQuery,
    },
    MempoolCountersSnapshot, MiningCounters, P2pTxCountSample,
};
use itertools::Itertools;
use parking_lot::RwLock;
use spora_consensus_client::pay_to_address_lock_script;
use spora_consensus_core::{
    api::{
        args::{TransactionValidationArgs, TransactionValidationBatchArgs},
        ConsensusApi,
    },
    block::{BlockTemplate, TemplateBuildMode, TemplateTransactionSelector},
    coinbase::MinerData,
    errors::{block::RuleError as BlockRuleError, tx::TxRuleError},
    tx::{CellTx, MutableTransaction, OutPointCompat, TransactionId},
};
use spora_consensusmanager::{spawn_blocking, ConsensusProxy};
use spora_core::{debug, error, info, time::Stopwatch, warn};
use spora_mining_errors::{manager::MiningManagerError, mempool::RuleError};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

pub struct MiningManager {
    config: Arc<Config>,
    block_template_cache: BlockTemplateCache,
    mempool: RwLock<Mempool>,
    counters: Arc<MiningCounters>,
}

impl MiningManager {
    // Used for tests only so we can pass a single value target_time_per_block
    pub fn new(
        target_time_per_block: u64,
        relay_non_std_transactions: bool,
        max_block_mass: u64,
        cache_lifetime: Option<u64>,
        counters: Arc<MiningCounters>,
    ) -> Self {
        let config = Config::build_default(target_time_per_block, relay_non_std_transactions, max_block_mass);
        Self::with_config(config, cache_lifetime, counters)
    }

    pub fn new_with_extended_config(
        target_time_per_block: u64,
        relay_non_std_transactions: bool,
        max_block_mass: u64,
        ram_scale: f64,
        cache_lifetime: Option<u64>,
        counters: Arc<MiningCounters>,
    ) -> Self {
        let config =
            Config::build_default(target_time_per_block, relay_non_std_transactions, max_block_mass).apply_ram_scale(ram_scale);
        Self::with_config(config, cache_lifetime, counters)
    }

    pub(crate) fn with_config(config: Config, cache_lifetime: Option<u64>, counters: Arc<MiningCounters>) -> Self {
        let config = Arc::new(config);
        let mempool = RwLock::new(Mempool::new(config.clone(), counters.clone()));
        let block_template_cache = BlockTemplateCache::new(cache_lifetime);
        Self { config, block_template_cache, mempool, counters }
    }

    pub fn get_block_template(&self, consensus: &dyn ConsensusApi, miner_data: &MinerData) -> MiningManagerResult<BlockTemplate> {
        let virtual_state_approx_id = consensus.get_virtual_state_approx_id();
        let mut cache_lock = self.block_template_cache.lock(virtual_state_approx_id);
        let immutable_template = cache_lock.get_immutable_cached_template();

        // We first try and use a cached template if not expired
        if let Some(immutable_template) = immutable_template {
            drop(cache_lock);
            if immutable_template.miner_data == *miner_data {
                return Ok(immutable_template.as_ref().clone());
            }
            // Miner data is new -- make the minimum changes required
            // Note the call returns a modified clone of the cached block template
            let block_template = BlockTemplateBuilder::modify_block_template(consensus, miner_data, &immutable_template)?;

            // No point in updating cache since we have no reason to believe this coinbase will be used more
            // than the previous one, and we want to maintain the original template caching time
            return Ok(block_template);
        }

        // Rust rewrite:
        // We avoid passing a mempool ref to blockTemplateBuilder by calling
        // mempool.BlockCandidateTransactions and mempool.RemoveTransactions here.
        // We remove recursion seen in blockTemplateBuilder.BuildBlockTemplate here.
        debug!("Building a new block template...");
        let _swo = Stopwatch::<22>::with_threshold("build_block_template full loop");
        let mut attempts: u64 = 0;
        loop {
            attempts += 1;

            let selector = self.build_selector();
            let block_template_builder = BlockTemplateBuilder::new();
            let build_mode = if attempts < self.config.maximum_build_block_template_attempts {
                TemplateBuildMode::Standard
            } else {
                TemplateBuildMode::Infallible
            };
            match block_template_builder.build_block_template(consensus, miner_data, selector, build_mode) {
                Ok(block_template) => {
                    let block_template = cache_lock.set_immutable_cached_template(block_template);
                    match attempts {
                        1 => {
                            debug!(
                                "Built a new block template with {} transactions in {:#?}",
                                block_template.block.transactions.len(),
                                _swo.elapsed()
                            );
                        }
                        2 => {
                            debug!(
                                "Built a new block template with {} transactions at second attempt in {:#?}",
                                block_template.block.transactions.len(),
                                _swo.elapsed()
                            );
                        }
                        n => {
                            debug!(
                                "Built a new block template with {} transactions in {} attempts totaling {:#?}",
                                block_template.block.transactions.len(),
                                n,
                                _swo.elapsed()
                            );
                        }
                    }
                    return Ok(block_template.as_ref().clone());
                }
                Err(BuilderError::ConsensusError(BlockRuleError::InvalidTransactionsInNewBlock(invalid_transactions))) => {
                    let mut missing_outpoint: usize = 0;
                    let mut invalid: usize = 0;

                    let mut mempool_write = self.mempool.write();
                    invalid_transactions.iter().for_each(|(x, err)| {
                        // On missing outpoints, the most likely is that the tx was already in a block accepted by
                        // the consensus but not yet processed by handle_new_block_transactions(). Another possibility
                        // is a double spend. In both cases, we simply remove the transaction but keep its redeemers.
                        // Those will either be valid in a next block template or invalidated if it's a double spend.
                        //
                        // If the redeemers of a transaction accepted in consensus but not yet handled in mempool were
                        // removed, it would lead to having subsequently submitted children transactions of the removed
                        // redeemers being unexpectedly either orphaned or rejected in case orphans are disallowed.
                        //
                        // For all other errors, we do remove the redeemers.

                        let removal_result = if *err == TxRuleError::MissingTxOutpoints {
                            missing_outpoint += 1;
                            mempool_write.remove_transaction(x, false, TxRemovalReason::Muted, "")
                        } else {
                            invalid += 1;
                            warn!("Remove per BBT invalid transaction and descendants");
                            mempool_write.remove_transaction(
                                x,
                                true,
                                TxRemovalReason::InvalidInBlockTemplate,
                                format!(" error: {}", err).as_str(),
                            )
                        };
                        if let Err(err) = removal_result {
                            // Original golang comment:
                            // mempool.remove_transactions might return errors in situations that are perfectly fine in this context.
                            // TODO: Once the mempool invariants are clear, this might return an error:
                            // https://github.com/sporanet/sporad/issues/1553
                            // NOTE: unlike golang, here we continue removing also if an error was found
                            error!("Error from mempool.remove_transactions: {:?}", err);
                        }
                    });
                    drop(mempool_write);

                    debug!(
                        "Building a new block template failed for {} txs missing outpoint and {} invalid txs",
                        missing_outpoint, invalid
                    );
                }
                Err(err) => {
                    warn!("Building a new block template failed: {}", err);
                    return Err(err)?;
                }
            }
        }
    }

    /// Dynamically builds a transaction selector based on the specific state of the ready transactions frontier
    pub(crate) fn build_selector(&self) -> Box<dyn TemplateTransactionSelector> {
        self.mempool.read().build_selector()
    }

    /// Returns realtime feerate estimations based on internal mempool state
    pub(crate) fn get_realtime_feerate_estimations(&self, _virtual_daa_score: u64) -> FeerateEstimations {
        let args = FeerateEstimatorArgs::new(self.config.network_blocks_per_second, self.config.maximum_mass_per_block);
        let estimator = self.mempool.read().build_feerate_estimator(args);
        estimator.calc_estimations(self.config.minimum_feerate())
    }

    /// Returns realtime feerate estimations based on internal mempool state with additional verbose data
    pub(crate) fn get_realtime_feerate_estimations_verbose(
        &self,
        consensus: &dyn ConsensusApi,
        prefix: spora_addresses::Prefix,
    ) -> MiningManagerResult<FeeEstimateVerbose> {
        let args = FeerateEstimatorArgs::new(self.config.network_blocks_per_second, self.config.maximum_mass_per_block);
        let network_mass_per_second = args.network_mass_per_second();
        let mempool_read = self.mempool.read();
        let estimator = mempool_read.build_feerate_estimator(args);
        let ready_transactions_count = mempool_read.ready_transaction_count();
        let ready_transaction_total_mass = mempool_read.ready_transaction_total_mass();
        drop(mempool_read);
        let mut resp = FeeEstimateVerbose {
            estimations: estimator.calc_estimations(self.config.minimum_feerate()),
            network_mass_per_second,
            mempool_ready_transactions_count: ready_transactions_count as u64,
            mempool_ready_transactions_total_mass: ready_transaction_total_mass,

            next_block_template_feerate_min: -1.0,
            next_block_template_feerate_median: -1.0,
            next_block_template_feerate_max: -1.0,
        };
        // calculate next_block_template_feerate_xxx
        {
            let lock_script = pay_to_address_lock_script(
                &spora_addresses::Address::new(prefix, spora_addresses::Version::PubKey, &[0u8; 32]).expect("Valid test address"),
            );
            let miner_data: MinerData = MinerData::new(lock_script, vec![]);

            let BlockTemplate { block: spora_consensus_core::block::MutableBlock { transactions, .. }, calculated_fees, .. } =
                self.get_block_template(consensus, &miner_data)?;

            let Some(Stats { max, median, min }) = feerate_stats(transactions, calculated_fees) else {
                return Ok(resp);
            };

            resp.next_block_template_feerate_max = max;
            resp.next_block_template_feerate_min = min;
            resp.next_block_template_feerate_median = median;
        }
        Ok(resp)
    }

    /// Clears the block template cache, forcing the next call to get_block_template to build a new block template.
    #[cfg(test)]
    pub(crate) fn clear_block_template(&self) {
        self.block_template_cache.clear();
    }

    #[cfg(test)]
    pub(crate) fn block_template_builder(&self) -> BlockTemplateBuilder {
        BlockTemplateBuilder::new()
    }

    pub(crate) fn validate_and_insert_cell_transaction(
        &self,
        consensus: &dyn ConsensusApi,
        cell_tx: CellTx,
        priority: Priority,
        orphan: Orphan,
        rbf_policy: RbfPolicy,
    ) -> MiningManagerResult<TransactionInsertion> {
        let TransactionPreValidation { mut transaction, cell_tx, feerate_threshold } =
            self.mempool.read().pre_validate_and_populate_cell_transaction(consensus, cell_tx, rbf_policy)?;
        let args = TransactionValidationArgs::new(feerate_threshold);
        let canonical_cell_tx = cell_tx.expect("cell transaction pre-validation must preserve the canonical CellTx");
        let validation_result = validate_mempool_cell_transaction(consensus, &mut transaction, canonical_cell_tx.as_ref(), &args);
        let mut mempool = self.mempool.write();
        match mempool.post_validate_and_insert_transaction(
            consensus,
            validation_result,
            transaction,
            Some(canonical_cell_tx),
            priority,
            orphan,
            rbf_policy,
        )? {
            TransactionPostValidation { removed, accepted: Some(accepted_transaction), accepted_cell_tx: _ } => {
                let unorphaned_transactions = mempool.get_unorphaned_transactions_after_accepted_cell_transaction(
                    accepted_transaction.as_ref(),
                    Some(accepted_transaction.id().into()),
                    spora_consensus_core::constants::UNACCEPTED_DAA_SCORE,
                );
                drop(mempool);

                let mut accepted_transactions = Vec::with_capacity(unorphaned_transactions.len() + 1);
                accepted_transactions.push(accepted_transaction);
                accepted_transactions.extend(self.validate_and_insert_unorphaned_transactions(consensus, unorphaned_transactions));
                self.counters.increase_tx_counts(1, priority);

                Ok(TransactionInsertion::new(removed, accepted_transactions))
            }
            TransactionPostValidation { removed, accepted: None, accepted_cell_tx: _ } => {
                Ok(TransactionInsertion::new(removed, vec![]))
            }
        }
    }

    pub fn validate_and_insert_cell_transaction_batch(
        &self,
        consensus: &dyn ConsensusApi,
        transactions: Vec<CellTx>,
        priority: Priority,
        orphan: Orphan,
        rbf_policy: RbfPolicy,
    ) -> Vec<MiningManagerResult<Arc<CellTx>>> {
        const TRANSACTION_CHUNK_SIZE: usize = 250;

        let mut insert_results: Vec<MiningManagerResult<Arc<CellTx>>> = Vec::with_capacity(transactions.len());
        let mut unorphaned_transactions = vec![];

        let _swo = Stopwatch::<81>::with_threshold("validate_and_insert_cell_transaction_batch topological_sort op");
        let sorted_transactions = transactions.into_iter().map(Arc::new).topological_into_iter();
        drop(_swo);

        let mut transactions = Vec::with_capacity(sorted_transactions.len());
        let mut canonical_cell_txs = Vec::with_capacity(sorted_transactions.len());
        let mut validation_args = Vec::with_capacity(sorted_transactions.len());

        for chunk in &sorted_transactions.chunks(TRANSACTION_CHUNK_SIZE) {
            let mempool = self.mempool.read();
            for cell_tx in chunk {
                let transaction_id: TransactionId = cell_tx.id().into();
                match mempool.pre_validate_and_populate_cell_transaction(consensus, cell_tx.as_ref().clone(), rbf_policy) {
                    Ok(TransactionPreValidation { transaction, cell_tx: Some(canonical_cell_tx), feerate_threshold }) => {
                        validation_args.push(TransactionValidationArgs::new(feerate_threshold));
                        transactions.push(transaction);
                        canonical_cell_txs.push(canonical_cell_tx);
                    }
                    Ok(TransactionPreValidation { cell_tx: None, .. }) => {
                        unreachable!("cell transaction pre-validation must preserve the canonical CellTx")
                    }
                    Err(RuleError::RejectAlreadyAccepted(transaction_id)) => {
                        debug!("Ignoring already accepted cell transaction {}", transaction_id);
                    }
                    Err(RuleError::RejectDuplicate(transaction_id)) => {
                        debug!("Ignoring cell transaction already in the mempool {}", transaction_id);
                    }
                    Err(RuleError::RejectDuplicateOrphan(transaction_id)) => {
                        debug!("Ignoring cell transaction already in the orphan pool {}", transaction_id);
                    }
                    Err(err) => {
                        debug!("Failed to pre validate cell transaction {0} due to rule error: {1}", transaction_id, err);
                        insert_results.push(Err(MiningManagerError::MempoolError(err)));
                    }
                }
            }
        }

        let mut lower_bound: usize = 0;
        let mut validation_results = Vec::with_capacity(transactions.len());
        while let Some(upper_bound) = self.next_transaction_chunk_upper_bound(&transactions, lower_bound) {
            assert!(lower_bound < upper_bound, "the chunk is never empty");
            for index in lower_bound..upper_bound {
                validation_results.push(validate_mempool_cell_transaction(
                    consensus,
                    &mut transactions[index],
                    canonical_cell_txs[index].as_ref(),
                    &validation_args[index],
                ));
            }
            lower_bound = upper_bound;
        }
        assert_eq!(transactions.len(), validation_results.len(), "every transaction should have a matching validation result");

        for chunk in &transactions.into_iter().zip(canonical_cell_txs).zip(validation_results).chunks(TRANSACTION_CHUNK_SIZE) {
            let mut mempool = self.mempool.write();
            let txs = chunk.flat_map(|((transaction, cell_tx), validation_result)| {
                let transaction_id = transaction.id();
                match mempool.post_validate_and_insert_transaction(
                    consensus,
                    validation_result,
                    transaction,
                    Some(cell_tx),
                    priority,
                    orphan,
                    rbf_policy,
                ) {
                    Ok(TransactionPostValidation { removed: _, accepted: Some(accepted_transaction), accepted_cell_tx: _ }) => {
                        insert_results.push(Ok(accepted_transaction.clone()));
                        self.counters.increase_tx_counts(1, priority);
                        mempool.get_unorphaned_transactions_after_accepted_cell_transaction(
                            accepted_transaction.as_ref(),
                            Some(accepted_transaction.id().into()),
                            spora_consensus_core::constants::UNACCEPTED_DAA_SCORE,
                        )
                    }
                    Ok(TransactionPostValidation { removed: _, accepted: None, accepted_cell_tx: _ })
                    | Err(RuleError::RejectDuplicate(_)) => {
                        vec![]
                    }
                    Err(err) => {
                        debug!("Failed to post validate cell transaction {0} due to rule error: {1}", transaction_id, err);
                        insert_results.push(Err(MiningManagerError::MempoolError(err)));
                        vec![]
                    }
                }
            });
            unorphaned_transactions.extend(txs);
        }

        insert_results
            .extend(self.validate_and_insert_unorphaned_transactions(consensus, unorphaned_transactions).into_iter().map(Ok));
        insert_results
    }

    fn validate_and_insert_unorphaned_transactions(
        &self,
        consensus: &dyn ConsensusApi,
        mut incoming_transactions: Vec<MempoolTransaction>,
    ) -> Vec<Arc<CellTx>> {
        // The capacity used here may be exceeded (see next comment).
        let mut accepted_transactions = Vec::with_capacity(incoming_transactions.len());
        // The validation args map is immutably empty since unorphaned transactions do not require pre processing so there
        // are no feerate thresholds to use. Instead, we rely on this being checked during post processing.
        let args = TransactionValidationBatchArgs::new();
        // We loop as long as incoming unorphaned transactions do unorphan other transactions when they
        // get validated and inserted into the mempool.
        while !incoming_transactions.is_empty() {
            let mut transactions = incoming_transactions;

            // no lock on mempool
            // We process the transactions by chunks of max block mass to prevent locking the virtual processor for too long.
            let mut lower_bound: usize = 0;
            let mut validation_results = Vec::with_capacity(transactions.len());
            while let Some(upper_bound) = self.next_mempool_transaction_chunk_upper_bound(&transactions, lower_bound) {
                assert!(lower_bound < upper_bound, "the chunk is never empty");
                validation_results.extend(validate_mempool_mempool_transactions_in_parallel(
                    consensus,
                    &mut transactions[lower_bound..upper_bound],
                    &args,
                ));
                lower_bound = upper_bound;
            }
            assert_eq!(transactions.len(), validation_results.len(), "every transaction should have a matching validation result");

            // write lock on mempool
            let mut mempool = self.mempool.write();
            incoming_transactions = transactions
                .into_iter()
                .zip(validation_results)
                .flat_map(|(transaction, validation_result)| {
                    let orphan_id = transaction.id();
                    let priority = transaction.priority;
                    let rbf_policy = Mempool::get_orphan_transaction_rbf_policy(priority);
                    let cell_tx = transaction.cell_tx();
                    let mtx = transaction.mtx;
                    match mempool.post_validate_and_insert_transaction(
                        consensus,
                        validation_result,
                        mtx,
                        cell_tx,
                        priority,
                        Orphan::Allowed,
                        rbf_policy,
                    ) {
                        Ok(TransactionPostValidation { removed: _, accepted: Some(accepted_transaction), accepted_cell_tx: _ }) => {
                            accepted_transactions.push(accepted_transaction.clone());
                            self.counters.increase_tx_counts(1, priority);
                            mempool.get_unorphaned_transactions_after_accepted_cell_transaction(
                                accepted_transaction.as_ref(),
                                Some(accepted_transaction.id().into()),
                                spora_consensus_core::constants::UNACCEPTED_DAA_SCORE,
                            )
                        }
                        Ok(TransactionPostValidation { removed: _, accepted: None, accepted_cell_tx: _ }) => vec![],
                        Err(err) => {
                            debug!("Failed to unorphan transaction {0} due to rule error: {1}", orphan_id, err);
                            vec![]
                        }
                    }
                })
                .collect::<Vec<_>>();
            drop(mempool);
        }
        accepted_transactions
    }

    fn next_transaction_chunk_upper_bound(&self, transactions: &[MutableTransaction], lower_bound: usize) -> Option<usize> {
        if lower_bound >= transactions.len() {
            return None;
        }
        let mut mass = 0;
        transactions[lower_bound..]
            .iter()
            .position(|tx| {
                mass += tx.calculated_non_contextual_masses.unwrap().max();
                mass >= self.config.maximum_mass_per_block
            })
            // Make sure the upper bound is greater than the lower bound, allowing to handle a very unlikely,
            // (if not impossible) case where the mass of a single transaction is greater than the maximum
            // chunk mass.
            .map(|relative_index| relative_index.max(1) + lower_bound)
            .or(Some(transactions.len()))
    }

    fn next_mempool_transaction_chunk_upper_bound(&self, transactions: &[MempoolTransaction], lower_bound: usize) -> Option<usize> {
        if lower_bound >= transactions.len() {
            return None;
        }
        let mut mass = 0;
        transactions[lower_bound..]
            .iter()
            .position(|tx| {
                mass += tx.mtx.calculated_non_contextual_masses.unwrap().max();
                mass >= self.config.maximum_mass_per_block
            })
            .map(|relative_index| relative_index.max(1) + lower_bound)
            .or(Some(transactions.len()))
    }

    /// Try to return a mempool transaction by its id.
    ///
    /// Note: the transaction is an orphan if tx.is_fully_populated() returns false.
    pub fn get_transaction(&self, transaction_id: &TransactionId, query: TransactionQuery) -> Option<MutableTransaction> {
        self.mempool.read().get_transaction(transaction_id, query)
    }

    /// Returns whether the mempool holds this transaction in any form.
    pub fn has_transaction(&self, transaction_id: &TransactionId, query: TransactionQuery) -> bool {
        self.mempool.read().has_transaction(transaction_id, query)
    }

    pub fn get_all_transactions(&self, query: TransactionQuery) -> (Vec<MutableTransaction>, Vec<MutableTransaction>) {
        const TRANSACTION_CHUNK_SIZE: usize = 1000;
        // read lock on mempool by transaction chunks
        let transactions = if query.include_transaction_pool() {
            let transaction_ids = self.mempool.read().get_all_transaction_ids(TransactionQuery::TransactionsOnly).0;
            let mut transactions = Vec::with_capacity(self.mempool.read().transaction_count(TransactionQuery::TransactionsOnly));
            for chunks in transaction_ids.chunks(TRANSACTION_CHUNK_SIZE) {
                let mempool = self.mempool.read();
                transactions.extend(chunks.iter().filter_map(|x| mempool.get_transaction(x, TransactionQuery::TransactionsOnly)));
            }
            transactions
        } else {
            vec![]
        };
        // read lock on mempool
        let orphans = if query.include_orphan_pool() {
            self.mempool.read().get_all_transactions(TransactionQuery::OrphansOnly).1
        } else {
            vec![]
        };
        (transactions, orphans)
    }

    /// get_transactions_by_addresses returns the sending and receiving transactions for
    /// a set of addresses.
    ///
    /// Note: a transaction is an orphan if tx.is_fully_populated() returns false.
    pub fn get_transactions_by_addresses(&self, addresses: &AddressSet, query: TransactionQuery) -> GroupedOwnerTransactions {
        // TODO: break the monolithic lock
        self.mempool.read().get_transactions_by_addresses(addresses, query)
    }

    pub fn transaction_count(&self, query: TransactionQuery) -> usize {
        self.mempool.read().transaction_count(query)
    }

    pub fn handle_new_block_transactions(
        &self,
        consensus: &dyn ConsensusApi,
        block_daa_score: u64,
        block_transactions: &[CellTx], // Updated to CellTx
    ) -> MiningManagerResult<Vec<Arc<CellTx>>> {
        // TODO: should use tx acceptance data to verify that new block txs are actually accepted into virtual state.
        // TODO: avoid returning a result from this function (and the underlying function). Any possible error is a
        // problem of the internal implementation and unrelated to the caller

        // write lock on mempool
        let unorphaned_transactions =
            self.mempool.write().handle_new_block_transactions(consensus, block_daa_score, block_transactions)?;

        // alternate no & write lock on mempool
        let accepted_transactions = self.validate_and_insert_unorphaned_transactions(consensus, unorphaned_transactions);

        Ok(accepted_transactions)
    }

    pub fn expire_low_priority_transactions(&self, consensus: &dyn ConsensusApi) {
        // very fine-grained write locks on mempool
        debug!("<> Expiring low priority transactions...");

        // orphan pool
        if let Err(err) = self.mempool.write().expire_orphan_low_priority_transactions(consensus) {
            warn!("Failed to expire transactions from orphan pool: {}", err);
        }

        // accepted transaction cache
        self.mempool.write().expire_accepted_transactions(consensus);

        // mempool
        let expired_low_priority_transactions = self.mempool.write().collect_expired_low_priority_transactions(consensus);
        for chunk in &expired_low_priority_transactions.iter().chunks(24) {
            let mut mempool = self.mempool.write();
            chunk.into_iter().for_each(|tx| {
                if let Err(err) = mempool.remove_transaction(tx, true, TxRemovalReason::Muted, "") {
                    warn!("Failed to remove transaction {} from mempool: {}", tx, err);
                }
            });
        }
        match expired_low_priority_transactions.len() {
            0 => {}
            1 => debug!("Removed transaction ({}) {}", TxRemovalReason::Expired, expired_low_priority_transactions[0]),
            n => debug!("Removed {} transactions ({}): {}...", n, TxRemovalReason::Expired, expired_low_priority_transactions[0]),
        }
    }

    pub fn revalidate_high_priority_transactions(
        &self,
        consensus: &dyn ConsensusApi,
        transaction_ids_sender: UnboundedSender<Vec<TransactionId>>,
    ) {
        const TRANSACTION_CHUNK_SIZE: usize = 1000;

        // read lock on mempool
        // Prepare a vector with clones of high priority transactions found in the mempool
        let mempool = self.mempool.read();
        let transaction_ids = mempool.all_transaction_ids_with_priority(Priority::High);
        if transaction_ids.is_empty() {
            debug!("<> Revalidating high priority transactions found no transactions");
            return;
        } else {
            debug!("<> Revalidating {} high priority transactions...", transaction_ids.len());
        }
        drop(mempool);
        // read lock on mempool by transaction chunks
        let mut transactions: Vec<MempoolTransaction> = Vec::with_capacity(transaction_ids.len());
        for chunk in &transaction_ids.iter().chunks(TRANSACTION_CHUNK_SIZE) {
            let mempool = self.mempool.read();
            transactions.extend(chunk.filter_map(|x| mempool.get_mempool_transaction(x, TransactionQuery::TransactionsOnly)));
        }

        let mut valid: usize = 0;
        let mut accepted: usize = 0;
        let mut other: usize = 0;
        let mut missing_outpoint: usize = 0;
        let mut invalid: usize = 0;

        // We process the transactions by level of dependency inside the batch.
        // Doing so allows to remove all chained dependencies of rejected transactions.
        let _swo = Stopwatch::<800>::with_threshold("revalidate topological_sort op");
        let sorted_transactions = transactions.topological_into_iter();
        drop(_swo);

        // read lock on mempool by transaction chunks
        // As the revalidation process is no longer atomic, we filter the transactions ready for revalidation,
        // keeping only the ones actually present in the mempool (see comment above).
        let _swo = Stopwatch::<900>::with_threshold("revalidate populate_mempool_entries op");
        let mut transactions = Vec::with_capacity(sorted_transactions.len());
        for chunk in &sorted_transactions.chunks(TRANSACTION_CHUNK_SIZE) {
            let mempool = self.mempool.read();
            let txs = chunk.filter_map(|mut x| {
                let transaction_id = x.id();
                if mempool.has_accepted_transaction(&transaction_id) {
                    accepted += 1;
                    None
                } else if mempool.has_transaction(&transaction_id, TransactionQuery::TransactionsOnly) {
                    x.mtx.clear_entries();
                    mempool.populate_mempool_entries(&mut x.mtx);
                    match x.mtx.is_fully_populated() && !x.has_canonical_cell_tx() {
                        false => Some(x),
                        true => {
                            // If all entries are populated with mempool cells, we already know the transaction is valid
                            valid += 1;
                            None
                        }
                    }
                } else {
                    other += 1;
                    None
                }
            });
            transactions.extend(txs);
        }
        drop(_swo);

        // no lock on mempool
        // We process the transactions by chunks of max block mass to prevent locking the virtual processor for too long.
        let mut lower_bound: usize = 0;
        let mut validation_results = Vec::with_capacity(transactions.len());
        while let Some(upper_bound) = self.next_mempool_transaction_chunk_upper_bound(&transactions, lower_bound) {
            assert!(lower_bound < upper_bound, "the chunk is never empty");
            let _swo = Stopwatch::<60>::with_threshold("revalidate validate_mempool_transactions_in_parallel op");
            validation_results.extend(validate_mempool_mempool_transactions_in_parallel(
                consensus,
                &mut transactions[lower_bound..upper_bound],
                &TransactionValidationBatchArgs::new(),
            ));
            drop(_swo);
            lower_bound = upper_bound;
        }
        assert_eq!(transactions.len(), validation_results.len(), "every transaction should have a matching validation result");

        // write lock on mempool
        // Depending on the validation result, transactions are either accepted or removed
        for chunk in &transactions.into_iter().zip(validation_results).chunks(TRANSACTION_CHUNK_SIZE) {
            let mut valid_ids = Vec::with_capacity(TRANSACTION_CHUNK_SIZE);
            let mut mempool = self.mempool.write();
            let _swo = Stopwatch::<60>::with_threshold("revalidate update_revalidated_transaction op");
            for (transaction, validation_result) in chunk {
                let transaction_id = transaction.id();
                match validation_result {
                    Ok(()) => {
                        // Only consider transactions still being in the mempool since during the validation some might have been removed.
                        if mempool.update_revalidated_transaction(transaction) {
                            // A following transaction should not remove this one from the pool since we process in a topological order.
                            // Still, considering the (very unlikely) scenario of two high priority txs sandwiching a low one, where
                            // in this case topological order is not guaranteed since we only considered chained dependencies of
                            // high-priority transactions, we might wrongfully return as valid the id of a removed transaction.
                            // However, as only consequence, said transaction would then be advertised to registered peers and not be
                            // provided upon request.
                            valid_ids.push(transaction_id);
                            valid += 1;
                        } else {
                            other += 1;
                        }
                    }
                    Err(RuleError::RejectMissingOutpoint) => {
                        let missing_txs = transaction
                            .mtx
                            .entries
                            .iter()
                            .zip(transaction.mtx.tx.inputs.iter())
                            .filter_map(|(entry, input)| entry.is_none().then_some(input.out_point.transaction_id()))
                            .collect::<Vec<_>>();

                        // A transaction may have missing outpoints for legitimate reasons related to concurrency, like a race condition between
                        // an accepted block having not started yet or unfinished call to handle_new_block_transactions but already processed by
                        // the consensus and this ongoing call to revalidate.
                        //
                        // So we only remove the transaction and keep its redeemers in the mempool because we cannot be sure they are invalid, in
                        // fact in the race condition case they are valid regarding outpoints.
                        let extra_info = match missing_txs.len() {
                            0 => " but no missing tx!".to_string(), // this is never supposed to happen
                            1 => format!(" missing tx {}", missing_txs[0]),
                            n => format!(" with {} missing txs {}..{}", n, missing_txs[0], missing_txs.last().unwrap()),
                        };

                        // This call cleanly removes the invalid transaction.
                        _ = mempool
                            .remove_transaction(
                                &transaction_id,
                                false,
                                TxRemovalReason::RevalidationWithMissingOutpoints,
                                extra_info.as_str(),
                            )
                            .inspect_err(|err| warn!("Failed to remove transaction {} from mempool: {}", transaction_id, err));
                        missing_outpoint += 1;
                    }
                    Err(err) => {
                        // Rust rewrite note:
                        // The behavior changes here compared to the golang version.
                        // The failed revalidation is simply logged and the process continues.
                        warn!(
                            "Removing high priority transaction {0} and its redeemers, it failed revalidation with {1}",
                            transaction_id, err
                        );
                        // This call cleanly removes the invalid transaction and its redeemers.
                        _ = mempool
                            .remove_transaction(&transaction_id, true, TxRemovalReason::Muted, "")
                            .inspect_err(|err| warn!("Failed to remove transaction {} from mempool: {}", transaction_id, err));
                        invalid += 1;
                    }
                }
            }
            if !valid_ids.is_empty() {
                let _ = transaction_ids_sender.send(valid_ids);
            }
            drop(_swo);
            drop(mempool);
        }
        match accepted + missing_outpoint + invalid {
            0 => {
                info!("Revalidated {} high priority transactions", valid);
            }
            _ => {
                info!(
                    "Revalidated {} and removed {} high priority transactions (removals: {} accepted, {} missing outpoint, {} invalid)",
                    valid,
                    accepted + missing_outpoint + invalid,
                    accepted,
                    missing_outpoint,
                    invalid,
                );
                if other > 0 {
                    debug!(
                        "During revalidation of high priority transactions {} txs were removed from the mempool by concurrent flows",
                        other
                    )
                }
            }
        }
    }

    /// is_transaction_output_dust returns whether or not the passed transaction output
    /// amount is considered dust or not based on the configured minimum transaction
    /// relay fee.
    ///
    /// Dust is defined in terms of the minimum transaction relay fee. In particular,
    /// if the cost to the network to spend coins is more than 1/3 of the minimum
    /// transaction relay fee, it is considered dust.
    ///
    #[cfg(test)]
    pub fn is_transaction_output_dust(&self, transaction_output: &spora_consensus_core::tx::CellOut) -> bool {
        self.mempool.read().is_transaction_output_dust(transaction_output)
    }

    pub fn has_accepted_transaction(&self, transaction_id: &TransactionId) -> bool {
        self.mempool.read().has_accepted_transaction(transaction_id)
    }

    pub fn unaccepted_transactions(&self, transactions: Vec<TransactionId>) -> Vec<TransactionId> {
        self.mempool.read().unaccepted_transactions(transactions)
    }

    pub fn unknown_transactions(&self, transactions: Vec<TransactionId>) -> Vec<TransactionId> {
        self.mempool.read().unknown_transactions(transactions)
    }

    #[cfg(test)]
    pub(crate) fn get_estimated_size(&self) -> usize {
        self.mempool.read().get_estimated_size()
    }
}

/// Async proxy for the mining manager
#[derive(Clone)]
pub struct MiningManagerProxy {
    inner: Arc<MiningManager>,
}

impl MiningManagerProxy {
    pub fn new(inner: Arc<MiningManager>) -> Self {
        Self { inner }
    }

    pub async fn get_block_template(self, consensus: &ConsensusProxy, miner_data: MinerData) -> MiningManagerResult<BlockTemplate> {
        consensus.clone().spawn_blocking(move |c| self.inner.get_block_template(c, &miner_data)).await
    }

    /// Returns realtime feerate estimations based on internal mempool state
    pub async fn get_realtime_feerate_estimations(self, virtual_daa_score: u64) -> FeerateEstimations {
        spawn_blocking(move || self.inner.get_realtime_feerate_estimations(virtual_daa_score)).await.unwrap()
    }

    /// Returns realtime feerate estimations based on internal mempool state with additional verbose data
    pub async fn get_realtime_feerate_estimations_verbose(
        self,
        consensus: &ConsensusProxy,
        prefix: spora_addresses::Prefix,
    ) -> MiningManagerResult<FeeEstimateVerbose> {
        consensus.clone().spawn_blocking(move |c| self.inner.get_realtime_feerate_estimations_verbose(c, prefix)).await
    }

    pub async fn validate_and_insert_cell_transaction(
        self,
        consensus: &ConsensusProxy,
        transaction: CellTx,
        priority: Priority,
        orphan: Orphan,
        rbf_policy: RbfPolicy,
    ) -> MiningManagerResult<TransactionInsertion> {
        consensus
            .clone()
            .spawn_blocking(move |c| self.inner.validate_and_insert_cell_transaction(c, transaction, priority, orphan, rbf_policy))
            .await
    }

    pub async fn validate_and_insert_cell_transaction_batch(
        self,
        consensus: &ConsensusProxy,
        transactions: Vec<CellTx>,
        priority: Priority,
        orphan: Orphan,
        rbf_policy: RbfPolicy,
    ) -> Vec<MiningManagerResult<Arc<CellTx>>> {
        consensus
            .clone()
            .spawn_blocking(move |c| {
                self.inner.validate_and_insert_cell_transaction_batch(c, transactions, priority, orphan, rbf_policy)
            })
            .await
    }

    pub async fn handle_new_block_transactions(
        self,
        consensus: &ConsensusProxy,
        block_daa_score: u64,
        block_transactions: Arc<Vec<CellTx>>, // Updated to CellTx
    ) -> MiningManagerResult<Vec<Arc<CellTx>>> {
        consensus
            .clone()
            .spawn_blocking(move |c| self.inner.handle_new_block_transactions(c, block_daa_score, &block_transactions))
            .await
    }

    pub async fn expire_low_priority_transactions(self, consensus: &ConsensusProxy) {
        consensus.clone().spawn_blocking(move |c| self.inner.expire_low_priority_transactions(c)).await;
    }

    pub async fn revalidate_high_priority_transactions(
        self,
        consensus: &ConsensusProxy,
        transaction_ids_sender: UnboundedSender<Vec<TransactionId>>,
    ) {
        consensus.clone().spawn_blocking(move |c| self.inner.revalidate_high_priority_transactions(c, transaction_ids_sender)).await;
    }

    /// Try to return a mempool transaction by its id.
    ///
    /// Note: the transaction is an orphan if tx.is_fully_populated() returns false.
    pub async fn get_transaction(self, transaction_id: TransactionId, query: TransactionQuery) -> Option<MutableTransaction> {
        spawn_blocking(move || self.inner.get_transaction(&transaction_id, query)).await.unwrap()
    }

    /// Returns whether the mempool holds this transaction in any form.
    pub async fn has_transaction(self, transaction_id: TransactionId, query: TransactionQuery) -> bool {
        spawn_blocking(move || self.inner.has_transaction(&transaction_id, query)).await.unwrap()
    }

    pub async fn transaction_count(self, query: TransactionQuery) -> usize {
        spawn_blocking(move || self.inner.transaction_count(query)).await.unwrap()
    }

    pub async fn get_all_transactions(self, query: TransactionQuery) -> (Vec<MutableTransaction>, Vec<MutableTransaction>) {
        spawn_blocking(move || self.inner.get_all_transactions(query)).await.unwrap()
    }

    /// get_transactions_by_addresses returns the sending and receiving transactions for
    /// a set of addresses.
    ///
    /// Note: a transaction is an orphan if tx.is_fully_populated() returns false.
    pub async fn get_transactions_by_addresses(self, addresses: AddressSet, query: TransactionQuery) -> GroupedOwnerTransactions {
        spawn_blocking(move || self.inner.get_transactions_by_addresses(&addresses, query)).await.unwrap()
    }

    /// Returns whether a transaction id was registered as accepted in the mempool, meaning
    /// that the consensus accepted a block containing it and said block was handled by the
    /// mempool.
    ///
    /// Registered transaction ids expire after a delay and are unregistered from the mempool.
    /// So a returned value of true means with certitude that the transaction was accepted and
    /// a false means either the transaction was never accepted or it was but beyond the expiration
    /// delay.
    pub async fn has_accepted_transaction(self, transaction_id: TransactionId) -> bool {
        spawn_blocking(move || self.inner.has_accepted_transaction(&transaction_id)).await.unwrap()
    }

    /// Returns a vector of unaccepted transactions.
    /// For more details, see [`Self::has_accepted_transaction()`].
    pub async fn unaccepted_transactions(self, transactions: Vec<TransactionId>) -> Vec<TransactionId> {
        spawn_blocking(move || self.inner.unaccepted_transactions(transactions)).await.unwrap()
    }

    /// Returns a vector with all transaction ids that are neither in the mempool, nor in the orphan pool
    /// nor accepted.
    pub async fn unknown_transactions(self, transactions: Vec<TransactionId>) -> Vec<TransactionId> {
        spawn_blocking(move || self.inner.unknown_transactions(transactions)).await.unwrap()
    }

    pub fn snapshot(&self) -> MempoolCountersSnapshot {
        self.inner.counters.snapshot()
    }

    pub fn p2p_tx_count_sample(&self) -> P2pTxCountSample {
        self.inner.counters.p2p_tx_count_sample()
    }

    /// Returns a recent sample of transaction count which is not necessarily accurate
    /// but is updated enough for being used as a stats/metric
    pub fn transaction_count_sample(&self, query: TransactionQuery) -> u64 {
        let mut count = 0;
        if query.include_transaction_pool() {
            count += self.inner.counters.txs_sample.load(std::sync::atomic::Ordering::Relaxed)
        }
        if query.include_orphan_pool() {
            count += self.inner.counters.orphans_sample.load(std::sync::atomic::Ordering::Relaxed)
        }
        count
    }
}

/// Represents statistical information about fee rates of transactions.
struct Stats {
    /// The maximum fee rate observed.
    max: f64,
    /// The median fee rate observed.
    median: f64,
    /// The minimum fee rate observed.
    min: f64,
}
/// Calculates the maximum, median, and minimum fee rates (fee per unit mass)
/// for a set of transactions, excluding the first transaction which is assumed
/// to be the coinbase transaction.
///
/// # Arguments
///
/// * `transactions` - A vector of `Transaction` objects. The first transaction
///   is assumed to be the coinbase transaction and is excluded from fee rate
///   calculations.
/// * `calculated_fees` - A vector of fees associated with the transactions.
///   This vector should have one less element than the `transactions` vector
///   since the first transaction (coinbase) does not have a fee.
///
/// # Returns
///
/// Returns an `Option<Stats>` containing the maximum, median, and minimum fee
/// rates if the input vectors are valid. Returns `None` if the vectors are
/// empty or if the lengths are inconsistent.
fn feerate_stats(transactions: Vec<CellTx>, calculated_fees: Vec<u64>) -> Option<Stats> {
    if calculated_fees.is_empty() {
        return None;
    }
    if transactions.len() != calculated_fees.len() + 1 {
        error!(
            "[feerate_stats] block template transactions length ({}) is expected to be one more than `calculated_fees` length ({})",
            transactions.len(),
            calculated_fees.len()
        );
        return None;
    }
    debug_assert!(transactions[0].is_coinbase());
    let mut feerates = calculated_fees
        .into_iter()
        .zip(transactions
            .iter()
            // skip coinbase tx
            .skip(1)
            .map(|tx| tx.serialized_size().max(1) as u64))
        .map(|(fee, mass)| fee as f64 / mass as f64)
        .collect_vec();
    feerates.sort_unstable_by(f64::total_cmp);

    let max = feerates[feerates.len() - 1];
    let min = feerates[0];
    let median = feerates[feerates.len() / 2];

    Some(Stats { max, median, min })
}

#[cfg(test)]
mod tests {
    use super::*;
    use spora_consensus_core::tx::{CellRef, CellTx, OutPoint};

    fn transactions(length: usize) -> Vec<CellTx> {
        let coinbase = CellTx::new(vec![], vec![], vec![], vec![], vec![]).expect("coinbase test tx must be constructible");
        let regular = || {
            CellTx::new(vec![CellRef::new(OutPoint::new([0u8; 32], 0), 0)], vec![], vec![], vec![], vec![vec![]])
                .expect("regular test tx must be constructible")
        };

        let mut txs = Vec::with_capacity(length.max(1));
        txs.push(coinbase);
        while txs.len() < length {
            txs.push(regular());
        }
        txs
    }

    #[test]
    fn feerate_stats_test() {
        let calculated_fees = vec![100u64, 200, 300, 400];
        let txs = transactions(calculated_fees.len() + 1);
        let mass = txs[1].serialized_size().max(1) as f64;
        let Stats { max, median, min } = feerate_stats(txs, calculated_fees).unwrap();
        assert_eq!(max, 400.0 / mass);
        assert_eq!(median, 300.0 / mass);
        assert_eq!(min, 100.0 / mass);
    }

    #[test]
    fn feerate_stats_empty_test() {
        let calculated_fees = vec![];
        let txs = transactions(calculated_fees.len() + 1);
        assert!(feerate_stats(txs, calculated_fees).is_none());
    }

    #[test]
    fn feerate_stats_inconsistent_test() {
        let calculated_fees = vec![100u64, 200, 300, 400];
        let txs = transactions(calculated_fees.len());
        assert!(feerate_stats(txs, calculated_fees).is_none());
    }
}
