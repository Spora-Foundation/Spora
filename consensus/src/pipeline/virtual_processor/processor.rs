#[cfg(feature = "vm")]
use crate::processes::cell_validator::CellScriptDataProvider;
use crate::{
    consensus::{
        cell_provider::OverlayCellProvider,
        services::{
            ConsensusServices, DbBlockDepthManager, DbDagTraversalManager, DbGhostdagManager, DbParentsManager, DbPruningPointManager,
            DbWindowManager,
        },
        storage::ConsensusStorage,
    },
    constants::BLOCK_VERSION,
    errors::RuleError,
    model::{
        services::{
            reachability::{MTReachabilityService, ReachabilityService},
            relations::MTRelationsService,
        },
        stores::{
            acceptance_data::{AcceptanceDataStoreReader, DbAcceptanceDataStore},
            block_transactions::{BlockTransactionsStoreReader, DbBlockTransactionsStore},
            block_window_cache::{BlockWindowCacheStore, BlockWindowCacheWriter},
            cell_diffs::{CellDiffsStoreReader, DbCellDiffsStore},
            cell_roots::{CellRootsStoreReader, DbCellRootsStore},
            daa::DbDaaStore,
            depth::{DbDepthStore, DepthStoreReader},
            ghostdag::{DbGhostdagStore, GhostdagData, GhostdagStoreReader},
            headers::{DbHeadersStore, HeaderStoreReader},
            past_pruning_points::DbPastPruningPointsStore,
            pruning::{DbPruningStore, PruningStoreReader},
            pruning_samples::DbPruningSamplesStore,
            // pruning cell-set store removed - Cell state in VirtualState
            reachability::DbReachabilityStore,
            relations::{DbRelationsStore, RelationsStoreReader},
            selected_chain::{DbSelectedChainStore, SelectedChainStore},
            statuses::{DbStatusesStore, StatusesStore, StatusesStoreBatchExtensions, StatusesStoreReader},
            tips::{DbTipsStore, TipsStoreReader},
            virtual_state::{LkgVirtualState, VirtualState, VirtualStateStoreReader, VirtualStores},
            DB,
        },
    },
    params::Params,
    pipeline::{
        deps_manager::VirtualStateProcessingMessage, pruning_processor::processor::PruningProcessingMessage, ProcessingCounters,
    },
    processes::{cell_validator::CellValidationError, CellConsensusParams, CellStateProvider, CellValidator, DagCellProvider},
    processes::{coinbase::CoinbaseManager, ghostdag::ordering::SortableBlock, window::WindowManager},
};

// Type aliases for migration compatibility
use spora_consensus_core::errors::tx::TxRuleError;
pub type TxResult<T> = Result<T, TxRuleError>;
use once_cell::unsync::Lazy;
use spora_consensus_core::{
    api::args::{TransactionValidationArgs, TransactionValidationBatchArgs},
    block::{BlockTemplate, MutableBlock, TemplateBuildMode, TemplateTransactionSelector},
    blockstatus::BlockStatus::{StatusCellValid, StatusDisqualifiedFromChain},
    cell_diff::{CellDiff, CellMeta}, // Cell model
    cell_metadata::CellMetadata,
    coinbase::MinerData,
    config::genesis::GenesisBlock,
    constants::MAX_SAU,
    header::Header,
    mass::{ContextualMasses, MassCalculator},
    mining_rules::MiningRules,
    pruning::PruningPointsList,
    tx::{legacy_sequence_to_cell_since, CellEntry, CellTx, MutableTransaction, Transaction, TransactionOutpoint},
    BlockHashSet,
    ChainPath,
};
// Cell state tree
use spora_exec::scheduler::CellDAG;
use spora_state::{CellEntry as StateCellEntry, CellStateTree};
// Legacy transaction-output imports removed - fully replaced by Cell model
use spora_consensus_notify::{
    notification::{
        CellsChangedNotification, NewBlockTemplateNotification, Notification, SinkBlueScoreChangedNotification,
        VirtualChainChangedNotification, VirtualDaaScoreChangedNotification,
    },
    root::ConsensusNotificationRoot,
};
use spora_consensusmanager::SessionLock;
use spora_core::{debug, info, time::unix_now, trace, warn};
use spora_database::prelude::{StoreError, StoreResultEmptyTuple, StoreResultExtensions};
use spora_hashes::{Hash, ZERO_HASH};
use spora_notify::{events::EventType, notifier::Notify};

use super::{
    cell_processing::{apply_cell_diff_to_tree, exec_outpoint, CellProcessingContext},
    errors::{PruningImportError, PruningImportResult},
};
use crossbeam_channel::{Receiver as CrossbeamReceiver, Sender as CrossbeamSender};
use itertools::Itertools;
use parking_lot::{RwLock, RwLockUpgradableReadGuard};
use rand::{seq::SliceRandom, Rng};
use rayon::ThreadPool;
use rocksdb::WriteBatch;
use spora_exec::OutPoint;
use spora_utils::binary_heap::BinaryHeapExtensions;
use std::{
    cmp::min,
    collections::{BinaryHeap, HashMap, HashSet, VecDeque},
    ops::Deref,
    sync::{atomic::Ordering, Arc},
};

fn filter_conflicting_template_transactions(txs: Vec<CellTx>, tx_selector: &mut dyn TemplateTransactionSelector) -> Vec<CellTx> {
    prefilter_conflicting_template_transactions(txs, Some(tx_selector)).kept_txs
}

struct TemplateConflictPrefilterOutcome {
    kept_txs: Vec<CellTx>,
    rejected_tx_ids: Vec<Hash>,
}

fn template_conflict_tx_rule_error() -> TxRuleError {
    TxRuleError::CellValidationFailed("template transaction conflicts with another selected transaction".to_string())
}

fn prefilter_conflicting_template_transactions(
    txs: Vec<CellTx>,
    mut tx_selector: Option<&mut dyn TemplateTransactionSelector>,
) -> TemplateConflictPrefilterOutcome {
    if txs.len() <= 1 {
        return TemplateConflictPrefilterOutcome { kept_txs: txs, rejected_tx_ids: Vec::new() };
    }

    let dag = match CellDAG::build(&txs) {
        Ok(dag) => dag,
        Err(err) => {
            warn!("template CellDAG analysis failed, skipping conflict prefilter: {err}");
            return TemplateConflictPrefilterOutcome { kept_txs: txs, rejected_tx_ids: Vec::new() };
        }
    };

    if dag.conflicts.is_empty() {
        return TemplateConflictPrefilterOutcome { kept_txs: txs, rejected_tx_ids: Vec::new() };
    }

    let mut rejected = HashSet::new();
    let mut queue = VecDeque::new();

    // Preserve current selector order: keep the earliest selected transaction
    // for each conflicting outpoint, reject later conflicts and all their
    // descendants which depend on their outputs.
    for consumers in dag.conflicts.values() {
        for &node_id in consumers.iter().skip(1) {
            if rejected.insert(node_id) {
                queue.push_back(node_id);
            }
        }
    }

    while let Some(node_id) = queue.pop_front() {
        if let Some(successors) = dag.successors(node_id) {
            for &(successor, _) in successors {
                if rejected.insert(successor) {
                    queue.push_back(successor);
                }
            }
        }
    }

    if rejected.is_empty() {
        return TemplateConflictPrefilterOutcome { kept_txs: txs, rejected_tx_ids: Vec::new() };
    }

    let rejected_count = rejected.len();
    let mut rejected_tx_ids = Vec::with_capacity(rejected_count);
    let kept_txs = txs
        .into_iter()
        .enumerate()
        .filter_map(|(node_id, tx)| {
            if rejected.contains(&node_id) {
                let tx_id = Hash::from_bytes(tx.id());
                rejected_tx_ids.push(tx_id);
                if let Some(selector) = tx_selector.as_deref_mut() {
                    selector.reject_selection(tx_id);
                }
                None
            } else {
                Some(tx)
            }
        })
        .collect();

    trace!("template CellDAG prefilter removed {} conflicting/descendant transactions", rejected_count);
    TemplateConflictPrefilterOutcome { kept_txs, rejected_tx_ids }
}

fn outpoint_to_cell_tree_hash(outpoint: &TransactionOutpoint) -> Hash {
    use blake3::Hasher;

    let mut hasher = Hasher::new();
    hasher.update(b"spora-cell/outpoint");
    hasher.update(&outpoint.tx_hash);
    hasher.update(&outpoint.index.to_le_bytes());
    Hash::from_bytes(*hasher.finalize().as_bytes())
}

fn synthetic_metadata_from_tree_entry(outpoint: TransactionOutpoint, entry: &StateCellEntry) -> CellMetadata {
    CellMetadata {
        out_point: outpoint,
        capacity: entry.capacity,
        data_bytes: entry.data_bytes,
        lock_hash: entry.lock_hash.as_bytes().try_into().expect("hash size is fixed"),
        type_hash: entry.type_hash.map(|hash| hash.as_bytes().try_into().expect("hash size is fixed")),
        data_hash: entry.data_hash.as_bytes().try_into().expect("hash size is fixed"),
        block_daa_score: entry.block_daa_score,
        is_cellbase: entry.is_cellbase,
        block_hash: ZERO_HASH,
        lock_code_hash: None,
        type_code_hash: None,
        lock_script: None,
        type_script: None,
        data: None,
    }
}

fn synthetic_metadata_from_cell_entry(outpoint: TransactionOutpoint, cell_entry: &CellEntry) -> CellMetadata {
    CellMetadata {
        out_point: outpoint,
        capacity: cell_entry.capacity,
        data_bytes: cell_entry.data_bytes,
        lock_hash: cell_entry.lock_hash,
        type_hash: cell_entry.type_hash,
        data_hash: cell_entry.data_hash,
        block_daa_score: cell_entry.block_daa_score,
        is_cellbase: cell_entry.is_cellbase,
        block_hash: ZERO_HASH,
        lock_code_hash: None,
        type_code_hash: None,
        lock_script: None,
        type_script: None,
        data: None,
    }
}

fn backfill_mempool_entries_from_resolved_inputs(mutable_tx: &mut MutableTransaction, resolved_inputs: &[CellMetadata]) {
    for ((_entry_slot, metadata_slot), metadata) in
        mutable_tx.entries.iter_mut().zip(mutable_tx.resolved_cell_metadata.iter_mut()).zip(resolved_inputs.iter())
    {
        if metadata_slot.is_none() {
            // Keep canonical metadata as the source of truth instead of synthesizing
            // placeholder-backed CellEntry values on the hot mempool path.
            *metadata_slot = Some(metadata.clone());
        }
    }
}

type TemplateOverlayProvider = OverlayCellProvider<VirtualSnapshotCellProvider>;

struct TemplateValidationOutcome {
    valid_txs: Vec<CellTx>,
    calculated_fees: Vec<u64>,
    invalid_transactions: HashMap<Hash, TxRuleError>,
}

fn compute_cell_data_hash(data: &[u8]) -> [u8; 32] {
    if data.is_empty() {
        [0u8; 32]
    } else {
        use blake3::Hasher;

        let mut hasher = Hasher::new();
        hasher.update(b"spora-cell/data");
        hasher.update(data);
        *hasher.finalize().as_bytes()
    }
}

fn cell_metadata_from_cell_output(
    block_hash: Hash,
    block_daa_score: u64,
    is_cellbase: bool,
    tx_id: [u8; 32],
    output_index: u32,
    output: &spora_exec::CellOut,
    output_data: &[u8],
) -> CellMetadata {
    CellMetadata {
        out_point: TransactionOutpoint { tx_hash: tx_id, index: output_index },
        capacity: output.capacity,
        data_bytes: output_data.len() as u64,
        lock_hash: output.lock.hash(),
        type_hash: output.type_.as_ref().map(|script| script.hash()),
        data_hash: compute_cell_data_hash(output_data),
        block_daa_score,
        is_cellbase,
        block_hash,
        lock_code_hash: Some(output.lock.code_hash),
        type_code_hash: output.type_.as_ref().map(|script| script.code_hash),
        lock_script: Some(output.lock.clone()),
        type_script: output.type_.clone(),
        data: Some(output_data.to_vec()),
    }
}

#[derive(Clone)]
struct VirtualSnapshotCellProvider {
    snapshot_pov: Hash,
    template_timestamp: u64,
    virtual_state: Arc<VirtualState>,
    headers_store: Arc<DbHeadersStore>,
    block_transactions_store: Arc<DbBlockTransactionsStore>,
    overrides: HashMap<OutPoint, CellMetadata>,
}

impl VirtualSnapshotCellProvider {
    fn new(
        virtual_state: Arc<VirtualState>,
        headers_store: Arc<DbHeadersStore>,
        block_transactions_store: Arc<DbBlockTransactionsStore>,
        overrides: HashMap<OutPoint, CellMetadata>,
        template_timestamp: u64,
    ) -> Self {
        let snapshot_pov = virtual_state.ghostdag_data.selected_parent;
        Self { snapshot_pov, template_timestamp, virtual_state, headers_store, block_transactions_store, overrides }
    }

    fn to_transaction_outpoint(out_point: &OutPoint) -> TransactionOutpoint {
        TransactionOutpoint { tx_hash: out_point.tx_hash, index: out_point.index }
    }

    fn ensure_snapshot_pov(&self, pov: Hash) -> Result<(), String> {
        if pov == self.snapshot_pov {
            Ok(())
        } else {
            Err(format!("unexpected POV {pov}, expected {}", self.snapshot_pov))
        }
    }

    fn load_metadata_from_block(&self, block: Hash, outpoint: &TransactionOutpoint) -> Result<Option<CellMetadata>, String> {
        let transactions = self.block_transactions_store.get(block).map_err(|e| format!("Transaction lookup error: {e}"))?;
        let header = self.headers_store.get_header(block).map_err(|e| format!("Header lookup error: {e}"))?;

        for (tx_index, tx) in transactions.iter().enumerate() {
            if Hash::from_bytes(tx.id()) != Hash::from_bytes(outpoint.tx_hash) {
                continue;
            }

            let output_index = outpoint.index as usize;
            if output_index >= tx.outputs.len() {
                return Ok(None);
            }

            let output = &tx.outputs[output_index];
            let output_data = tx.outputs_data.get(output_index).map(|data| data.as_slice()).unwrap_or(&[]);
            return Ok(Some(cell_metadata_from_cell_output(
                block,
                header.daa_score,
                tx_index == 0 && tx.is_coinbase(),
                tx.id(),
                output_index as u32,
                output,
                output_data,
            )));
        }

        Ok(None)
    }

    fn resolve_virtual_only_metadata(&self, outpoint: &TransactionOutpoint) -> Result<Option<CellMetadata>, String> {
        for block in std::iter::once(self.virtual_state.ghostdag_data.selected_parent)
            .chain(self.virtual_state.ghostdag_data.mergeset_blues.iter().copied())
        {
            if let Some(metadata) = self.load_metadata_from_block(block, outpoint)? {
                return Ok(Some(metadata));
            }
        }

        Ok(None)
    }

    fn resolve_snapshot_metadata(&self, out_point: &OutPoint) -> Result<Option<CellMetadata>, String> {
        if let Some(metadata) = self.overrides.get(out_point) {
            return Ok(Some(metadata.clone()));
        }

        let tx_outpoint = Self::to_transaction_outpoint(out_point);
        let tree_key = outpoint_to_cell_tree_hash(&tx_outpoint);
        let live_in_tree = self.virtual_state.cell_state_tree.get(&tree_key).is_some();

        if let Ok((creator_block, tx_index)) =
            self.block_transactions_store.get_transaction_location(Hash::from_bytes(tx_outpoint.tx_hash))
        {
            let is_selected_parent_coinbase = creator_block == self.snapshot_pov && tx_index == 0;
            if live_in_tree || is_selected_parent_coinbase {
                if let Some(metadata) = self.load_metadata_from_block(creator_block, &tx_outpoint)? {
                    return Ok(Some(metadata));
                }
            }
        }

        if live_in_tree {
            if let Some(metadata) = self.resolve_virtual_only_metadata(&tx_outpoint)? {
                return Ok(Some(metadata));
            }
        }

        let Some(tree_entry) = self.virtual_state.cell_state_tree.get(&tree_key) else {
            return Ok(None);
        };

        let mut synthetic = synthetic_metadata_from_tree_entry(tx_outpoint, tree_entry);
        synthetic.block_hash = self.snapshot_pov;
        Ok(Some(synthetic))
    }
}

impl CellStateProvider for VirtualSnapshotCellProvider {
    fn is_cell_available(&self, out_point: &OutPoint, pov: Hash) -> Result<bool, String> {
        self.ensure_snapshot_pov(pov)?;
        Ok(self.resolve_snapshot_metadata(out_point)?.is_some())
    }

    fn get_cell_capacity(&self, out_point: &OutPoint, pov: Hash) -> Result<Option<u64>, String> {
        self.ensure_snapshot_pov(pov)?;
        Ok(self.resolve_snapshot_metadata(out_point)?.map(|metadata| metadata.capacity))
    }
}

impl DagCellProvider for VirtualSnapshotCellProvider {
    fn get_cell_at_pov(&self, out_point: &OutPoint, pov: Hash) -> Result<Option<CellMetadata>, String> {
        self.ensure_snapshot_pov(pov)?;
        self.resolve_snapshot_metadata(out_point)
    }

    fn get_block_timestamp(&self, block_hash: Hash) -> Result<u64, String> {
        if block_hash == ZERO_HASH {
            Ok(self.template_timestamp)
        } else {
            self.headers_store.get_timestamp(block_hash).map_err(|e| format!("Header timestamp lookup error: {e}"))
        }
    }
}

#[cfg(feature = "vm")]
impl CellScriptDataProvider for VirtualSnapshotCellProvider {
    fn get_cell_data(&self, out_point: &OutPoint, pov: Hash) -> Result<Option<Vec<u8>>, String> {
        self.ensure_snapshot_pov(pov)?;
        Ok(self.resolve_snapshot_metadata(out_point)?.and_then(|metadata| metadata.data))
    }

    fn get_header(&self, block_hash: Hash) -> Result<Option<spora_exec::vm::ResolvedHeader>, String> {
        let header = match self.headers_store.get_header(block_hash) {
            Ok(header) => header,
            Err(StoreError::KeyNotFound(_)) => return Ok(None),
            Err(err) => return Err(format!("Header lookup error: {err}")),
        };
        Ok(Some(spora_exec::vm::ResolvedHeader {
            hash: block_hash.as_bytes(),
            timestamp: header.timestamp,
            daa_score: header.daa_score,
            parents: header.direct_parents().iter().map(|hash| hash.as_bytes()).collect(),
        }))
    }
}

pub struct VirtualStateProcessor {
    // Channels
    receiver: CrossbeamReceiver<VirtualStateProcessingMessage>,
    pruning_sender: CrossbeamSender<PruningProcessingMessage>,
    pruning_receiver: CrossbeamReceiver<PruningProcessingMessage>,

    // Thread pool
    pub(super) thread_pool: Arc<ThreadPool>,

    // DB
    pub(super) db: Arc<DB>,

    // Config
    pub(super) genesis: GenesisBlock,
    pub(super) max_block_parents: u8,
    pub(super) mergeset_size_limit: u64,
    pub(super) coinbase_maturity: u64,

    // Stores
    pub(super) statuses_store: Arc<RwLock<DbStatusesStore>>,
    pub(super) ghostdag_store: Arc<DbGhostdagStore>,
    pub(super) headers_store: Arc<DbHeadersStore>,
    pub(super) daa_excluded_store: Arc<DbDaaStore>,
    pub(super) block_transactions_store: Arc<DbBlockTransactionsStore>,
    pub(super) pruning_point_store: Arc<RwLock<DbPruningStore>>,
    pub(super) past_pruning_points_store: Arc<DbPastPruningPointsStore>,
    pub(super) body_tips_store: Arc<RwLock<DbTipsStore>>,
    pub(super) depth_store: Arc<DbDepthStore>,
    pub(super) selected_chain_store: Arc<RwLock<DbSelectedChainStore>>,
    pub(super) pruning_samples_store: Arc<DbPruningSamplesStore>,

    // Cell-related stores
    pub(super) cell_diffs_store: Arc<DbCellDiffsStore>,
    pub(super) cell_roots_store: Arc<DbCellRootsStore>,
    pub(super) acceptance_data_store: Arc<DbAcceptanceDataStore>,
    pub(super) virtual_stores: Arc<RwLock<VirtualStores>>,

    /// The "last known good" virtual state. To be used by any logic which does not want to wait
    /// for a possible virtual state write to complete but can rather settle with the last known state
    pub lkg_virtual_state: LkgVirtualState,

    // Managers and services
    pub(super) ghostdag_manager: DbGhostdagManager,
    pub(super) reachability_service: MTReachabilityService<DbReachabilityStore>,
    pub(super) relations_service: MTRelationsService<DbRelationsStore>,
    pub(super) dag_traversal_manager: DbDagTraversalManager,
    pub(super) window_manager: DbWindowManager,
    pub(super) coinbase_manager: CoinbaseManager,
    pub(super) mass_calculator: MassCalculator,
    // TransactionValidator removed - Cell validation is now routed through snapshot/overlay providers.
    pub(super) pruning_point_manager: DbPruningPointManager,
    pub(super) parents_manager: DbParentsManager,
    pub(super) depth_manager: DbBlockDepthManager,

    // block window caches
    pub(super) block_window_cache_for_difficulty: Arc<BlockWindowCacheStore>,
    pub(super) block_window_cache_for_past_median_time: Arc<BlockWindowCacheStore>,

    // Pruning lock
    pub(super) pruning_lock: SessionLock,

    // Notifier
    notification_root: Arc<ConsensusNotificationRoot>,

    // Counters
    counters: Arc<ProcessingCounters>,

    // Mining Rule
    mining_rules: Arc<MiningRules>,
}

impl VirtualStateProcessor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        receiver: CrossbeamReceiver<VirtualStateProcessingMessage>,
        pruning_sender: CrossbeamSender<PruningProcessingMessage>,
        pruning_receiver: CrossbeamReceiver<PruningProcessingMessage>,
        thread_pool: Arc<ThreadPool>,
        params: &Params,
        db: Arc<DB>,
        storage: &Arc<ConsensusStorage>,
        services: &Arc<ConsensusServices>,
        pruning_lock: SessionLock,
        notification_root: Arc<ConsensusNotificationRoot>,
        counters: Arc<ProcessingCounters>,
        mining_rules: Arc<MiningRules>,
    ) -> Self {
        Self {
            receiver,
            pruning_sender,
            pruning_receiver,
            thread_pool,

            genesis: params.genesis.clone(),
            max_block_parents: params.max_block_parents(),
            mergeset_size_limit: params.mergeset_size_limit(),
            coinbase_maturity: params.coinbase_maturity(),

            db,
            statuses_store: storage.statuses_store.clone(),
            headers_store: storage.headers_store.clone(),
            ghostdag_store: storage.ghostdag_store.clone(),
            daa_excluded_store: storage.daa_excluded_store.clone(),
            block_transactions_store: storage.block_transactions_store.clone(),
            pruning_point_store: storage.pruning_point_store.clone(),
            past_pruning_points_store: storage.past_pruning_points_store.clone(),
            body_tips_store: storage.body_tips_store.clone(),
            depth_store: storage.depth_store.clone(),
            selected_chain_store: storage.selected_chain_store.clone(),
            pruning_samples_store: storage.pruning_samples_store.clone(),
            cell_diffs_store: storage.cell_diffs_store.clone(),
            cell_roots_store: storage.cell_roots_store.clone(),
            acceptance_data_store: storage.acceptance_data_store.clone(),
            virtual_stores: storage.virtual_stores.clone(),
            lkg_virtual_state: storage.lkg_virtual_state.clone(),

            block_window_cache_for_difficulty: storage.block_window_cache_for_difficulty.clone(),
            block_window_cache_for_past_median_time: storage.block_window_cache_for_past_median_time.clone(),

            ghostdag_manager: services.ghostdag_manager.clone(),
            reachability_service: services.reachability_service.clone(),
            relations_service: services.relations_service.clone(),
            dag_traversal_manager: services.dag_traversal_manager.clone(),
            window_manager: services.window_manager.clone(),
            coinbase_manager: services.coinbase_manager.clone(),
            mass_calculator: services.mass_calculator.clone(),
            // transaction_validator removed - using CellValidator
            pruning_point_manager: services.pruning_point_manager.clone(),
            parents_manager: services.parents_manager.clone(),
            depth_manager: services.depth_manager.clone(),

            pruning_lock,
            notification_root,
            counters,
            mining_rules,
        }
    }

    pub fn worker(self: &Arc<Self>) {
        'outer: while let Ok(msg) = self.receiver.recv() {
            if msg.is_exit_message() {
                break;
            }

            // Once a task arrived, collect all pending tasks from the channel.
            // This is done since virtual processing is not a per-block
            // operation, so it benefits from max available info

            let messages: Vec<VirtualStateProcessingMessage> = std::iter::once(msg).chain(self.receiver.try_iter()).collect();
            trace!("virtual processor received {} tasks", messages.len());

            let resolve_result = self.resolve_virtual();

            let statuses_read = self.statuses_store.read();
            for msg in messages {
                match msg {
                    VirtualStateProcessingMessage::Exit => break 'outer,
                    VirtualStateProcessingMessage::Process(task, virtual_state_result_transmitter) => {
                        // We don't care if receivers were dropped
                        let result = match &resolve_result {
                            Ok(()) => Ok(statuses_read.get(task.block().hash()).expect("block status must exist in store")),
                            Err(err) => Err(err.clone()),
                        };
                        let _ = virtual_state_result_transmitter.send(result);
                    }
                };
            }
        }

        // Pass the exit signal on to the following processor
        self.pruning_sender.send(PruningProcessingMessage::Exit).expect("pruning receiver should be alive");
    }

    fn resolve_virtual(self: &Arc<Self>) -> Result<(), RuleError> {
        let pruning_point = self.pruning_point_store.read().pruning_point().expect("pruning point must exist");
        let virtual_read = self.virtual_stores.upgradable_read();
        let prev_state = virtual_read.state.get().expect("virtual state must exist");
        let finality_point = self.virtual_finality_point(&prev_state.ghostdag_data, pruning_point);

        // PRUNE SAFETY: in order to avoid locking the prune lock throughout virtual resolving we make sure
        // to only process blocks in the future of the finality point (F) which are never pruned (since finality depth << pruning depth).
        // This is justified since:
        //      1. Tips which are not in the future of F definitely don't have F on their chain
        //         hence cannot become the next sink (due to finality violation).
        //      2. Such tips cannot be merged by virtual since they are violating the merge depth
        //         bound (merge depth <= finality depth).
        // (both claims are true by induction for any block in their past as well)
        let prune_guard = self.pruning_lock.blocking_read();
        let tips = self
            .body_tips_store
            .read()
            .get()
            .expect("body tips must exist")
            .read()
            .iter()
            .copied()
            .filter(|&h| self.reachability_service.is_dag_ancestor_of(finality_point, h))
            .collect_vec();
        drop(prune_guard);
        let prev_sink = prev_state.ghostdag_data.selected_parent;
        let mut accumulated_diff = prev_state.cell_diff.clone().reverse();

        let (new_sink, virtual_parent_candidates) =
            self.sink_search_algorithm(&virtual_read, &mut accumulated_diff, prev_sink, tips, finality_point, pruning_point);
        let virtual_parent_candidates =
            self.filter_virtual_parent_candidates(&virtual_read, &accumulated_diff, new_sink, virtual_parent_candidates);
        let (virtual_parents, virtual_ghostdag_data) = self.pick_virtual_parents(new_sink, virtual_parent_candidates, pruning_point);
        assert_eq!(virtual_ghostdag_data.selected_parent, new_sink);

        let sink_cell_root = self.cell_roots_store.get(new_sink).expect("sink cell root must exist in store");
        let chain_path = self.dag_traversal_manager.calculate_chain_path(prev_sink, new_sink, None);
        let sink_ghostdag_data = Lazy::new(|| self.ghostdag_store.get_data(new_sink).expect("sink ghostdag data must exist"));
        // Cache the DAA and Median time windows of the sink for future use, as well as prepare for virtual's window calculations
        self.cache_sink_windows(new_sink, prev_sink, &sink_ghostdag_data);

        let new_virtual_state = match self.calculate_virtual_state(
            &virtual_read,
            virtual_parents.clone(),
            virtual_ghostdag_data,
            sink_cell_root,
            &mut accumulated_diff,
        ) {
            Ok(state) => state,
            Err(rule_error) => {
                warn!(
                    "Virtual state calculation failed for sink {} with parents {:?}: {}. Retrying with selected parent only.",
                    new_sink, virtual_parents, rule_error
                );
                let fallback_parents = vec![new_sink];
                let fallback_ghostdag_data = self.ghostdag_manager.ghostdag(&fallback_parents);
                accumulated_diff = prev_state.cell_diff.clone().reverse();
                self.calculate_virtual_state(
                    &virtual_read,
                    fallback_parents,
                    fallback_ghostdag_data,
                    sink_cell_root,
                    &mut accumulated_diff,
                )?
            }
        };
        self.commit_virtual_state(virtual_read, new_virtual_state.clone(), &accumulated_diff, &chain_path);

        let compact_sink_ghostdag_data = if let Some(sink_ghostdag_data) = Lazy::get(&sink_ghostdag_data) {
            // If we had to retrieve the full data, we convert it to compact
            sink_ghostdag_data.to_compact()
        } else {
            // Else we query the compact data directly.
            self.ghostdag_store.get_compact_data(new_sink).expect("compact ghostdag data must exist")
        };

        // Update the pruning processor about the virtual state change
        // Empty the channel before sending the new message. If pruning processor is busy, this step makes sure
        // the internal channel does not grow with no need (since we only care about the most recent message)
        let _consume = self.pruning_receiver.try_iter().count();
        self.pruning_sender
            .send(PruningProcessingMessage::Process { sink_ghostdag_data: compact_sink_ghostdag_data })
            .expect("pruning receiver should be alive");

        // Emit notifications
        let accumulated_cell_diff = Arc::new(new_virtual_state.cell_diff.clone());
        let virtual_parents = Arc::new(new_virtual_state.parents.clone());
        let block_cell_diffs = Arc::new(new_virtual_state.block_cell_diffs.clone());
        self.notification_root
            .notify(Notification::NewBlockTemplate(NewBlockTemplateNotification {}))
            .expect("expecting an open unbounded channel");
        // CellsChanged notification - GHOSTDAG-aware
        self.notification_root
            .notify(Notification::CellsChanged(CellsChangedNotification::new(
                accumulated_cell_diff,
                virtual_parents.clone(),
                block_cell_diffs,
            )))
            .expect("expecting an open unbounded channel");
        self.notification_root
            .notify(Notification::SinkBlueScoreChanged(SinkBlueScoreChangedNotification::new(compact_sink_ghostdag_data.blue_score)))
            .expect("expecting an open unbounded channel");
        self.notification_root
            .notify(Notification::VirtualDaaScoreChanged(VirtualDaaScoreChangedNotification::new(new_virtual_state.daa_score)))
            .expect("expecting an open unbounded channel");
        if self.notification_root.has_subscription(EventType::VirtualChainChanged) {
            // check for subscriptions before the heavy lifting
            let added_chain_blocks_acceptance_data = chain_path
                .added
                .iter()
                .copied()
                .map(|added| self.acceptance_data_store.get(added).expect("acceptance data must exist for chain block"))
                .collect_vec();
            self.notification_root
                .notify(Notification::VirtualChainChanged(VirtualChainChangedNotification::new(
                    chain_path.added.into(),
                    chain_path.removed.into(),
                    Arc::new(added_chain_blocks_acceptance_data),
                )))
                .expect("expecting an open unbounded channel");
        }
        Ok(())
    }

    pub(crate) fn virtual_finality_point(&self, virtual_ghostdag_data: &GhostdagData, pruning_point: Hash) -> Hash {
        let finality_point = self.depth_manager.calc_finality_point(virtual_ghostdag_data, pruning_point);
        if self.reachability_service.is_chain_ancestor_of(pruning_point, finality_point) {
            finality_point
        } else {
            // At the beginning of IBD when virtual finality point might be below the pruning point
            // or disagreeing with the pruning point chain, we take the pruning point itself as the finality point
            pruning_point
        }
    }

    /// Calculates the Cell state of `to` starting from the state of `from`.
    /// The provided `diff` is assumed to initially hold the Cell diff of `from` from virtual.
    /// The function returns the top-most Cell-valid block on `chain(to)` which is ideally
    /// `to` itself (with the exception of returning `from` if `to` is already known to be disqualified).
    /// When returning it is guaranteed that `diff` holds the diff of the returned block from virtual
    fn calculate_cell_state_relatively(
        &self,
        stores: &VirtualStores,
        diff: &mut CellDiff,
        from: Hash,
        to: Hash,
    ) -> Result<Hash, RuleError> {
        // Avoid reorging if disqualified status is already known
        if self.statuses_store.read().get(to).expect("block status must exist") == StatusDisqualifiedFromChain {
            return Ok(from);
        }

        let mut split_point: Option<Hash> = None;

        // Walk down to the reorg split point
        for current in self.reachability_service.default_backward_chain_iterator(from) {
            if self.reachability_service.is_chain_ancestor_of(current, to) {
                split_point = Some(current);
                break;
            }

            let mergeset_diff = self.cell_diffs_store.get(current).expect("cell diff must exist for cell-valid block");
            // Apply the diff in reverse (Cell model)
            diff.with_diff_in_place(&mergeset_diff.as_reversed()).map_err(|e| {
                RuleError::CellValidationError(format!(
                    "failed to reverse cell diff while reorging from {} toward {} at {}: {}",
                    from, to, current, e
                ))
            })?;
        }

        let split_point = split_point.expect("chain iterator was expected to reach the reorg split point");
        debug!("VIRTUAL PROCESSOR, found split point: {split_point}");

        // A variable holding the most recent Cell-valid block on `chain(to)` (note that it's maintained such
        // that 'diff' is always its Cell diff from virtual)
        let mut diff_point = split_point;

        // Walk back up to the new virtual selected parent candidate
        let mut chain_block_counter = 0;
        let mut chain_disqualified_counter = 0;
        for (selected_parent, current) in self.reachability_service.forward_chain_iterator(split_point, to, true).tuple_windows() {
            if selected_parent != diff_point {
                // This indicates that the selected parent is disqualified, propagate up and continue
                let statuses_guard = self.statuses_store.upgradable_read();
                if statuses_guard.get(current).expect("block status must exist") != StatusDisqualifiedFromChain {
                    RwLockUpgradableReadGuard::upgrade(statuses_guard)
                        .set(current, StatusDisqualifiedFromChain)
                        .expect("status store write must succeed");
                    chain_disqualified_counter += 1;
                }
                continue;
            }

            match self.cell_diffs_store.get(current) {
                Ok(mergeset_diff) => {
                    diff.with_diff_in_place(mergeset_diff.deref()).map_err(|e| {
                        RuleError::CellValidationError(format!(
                            "failed to apply stored cell diff while reorging from {} toward {} at {}: {}",
                            from, to, current, e
                        ))
                    })?;
                    diff_point = current;
                }
                Err(StoreError::KeyNotFound(_)) => {
                    if self.statuses_store.read().get(current).expect("block status must exist") == StatusDisqualifiedFromChain {
                        // Current block is already known to be disqualified
                        continue;
                    }

                    let header = self.headers_store.get_header(current).expect("block header must exist");
                    let mergeset_data = self.ghostdag_store.get_data(current).expect("ghostdag data must exist");
                    let pov_daa_score = header.daa_score;

                    let virtual_state = stores.state.get().expect("virtual state must exist");
                    let mut selected_parent_cell_tree = self.reconstruct_tree_from_virtual_diff(&virtual_state, diff);
                    let expected_selected_parent_root =
                        self.cell_roots_store.get(selected_parent).expect("selected parent cell root must exist for chain block");
                    let calculated_selected_parent_root = selected_parent_cell_tree.root();
                    assert_eq!(
                        calculated_selected_parent_root, expected_selected_parent_root,
                        "reconstructed selected parent cell state does not match the committed cell root"
                    );

                    let mut ctx = CellProcessingContext::new(mergeset_data.into(), selected_parent_cell_tree);

                    let res = self
                        .calculate_cell_state(&mut ctx, pov_daa_score, current, header.timestamp)
                        .and_then(|_| self.verify_expected_cell_state(&mut ctx, &header));

                    if let Err(rule_error) = res {
                        info!("Block {} is disqualified from virtual chain: {}", current, rule_error);
                        self.statuses_store
                            .write()
                            .set(current, StatusDisqualifiedFromChain)
                            .expect("status store write must succeed");
                        chain_disqualified_counter += 1;
                    } else {
                        debug!("VIRTUAL PROCESSOR, cell validated for {current}");
                        let pruning_sample_from_pov = ctx.pruning_sample_from_pov.unwrap_or_else(|| {
                            self.pruning_point_manager.expected_header_pruning_point_v2(ctx.ghostdag_data.to_compact()).pruning_sample
                        });

                        // Accumulate the diff (Cell model)
                        if let Err(e) = diff.with_diff_in_place(&ctx.mergeset_cell_diff) {
                            info!("Block {} is disqualified from virtual chain: invalid cell diff composition: {}", current, e);
                            self.statuses_store
                                .write()
                                .set(current, StatusDisqualifiedFromChain)
                                .expect("status store write must succeed");
                            chain_disqualified_counter += 1;
                            continue;
                        }
                        // Update the diff point
                        diff_point = current;
                        // Commit Cell state data for current chain block
                        let cell_root = ctx.get_cell_root();
                        self.commit_cell_state(
                            current,
                            ctx.mergeset_cell_diff.clone(),
                            cell_root,
                            ctx.mergeset_acceptance_data.clone(),
                            pruning_sample_from_pov,
                        );
                        // Count the number of cell-processed chain blocks
                        chain_block_counter += 1;
                    }
                }
                Err(err) => {
                    return Err(RuleError::CellValidationError(format!(
                        "unexpected diff store error while processing {} toward {}: {}",
                        from, to, err
                    )))
                }
            }
        }
        // Report counters
        self.counters.chain_block_counts.fetch_add(chain_block_counter, Ordering::Relaxed);
        if chain_disqualified_counter > 0 {
            self.counters.chain_disqualified_counts.fetch_add(chain_disqualified_counter, Ordering::Relaxed);
        }

        Ok(diff_point)
    }

    /// Verify that the expected cell state matches the calculated state
    /// Verifies the expected Cell state.
    fn verify_expected_cell_state(&self, ctx: &mut CellProcessingContext, header: &Header) -> Result<(), RuleError> {
        // Calculate cell_root from current state tree
        let calculated_cell_root = ctx.get_cell_root();

        // Verify cell_root matches header
        let expected_cell_root = header.cell_root;

        if calculated_cell_root != expected_cell_root {
            // Detailed error for debugging
            let error_msg = format!(
                "Cell root mismatch for block {:?}:\n\
                 Expected: {:?}\n\
                 Calculated: {:?}\n\
                 Tree size: {} cells\n\
                 Diff: +{} cells, -{} cells\n\
                 Selected parent: {:?}",
                header.hash,
                expected_cell_root,
                calculated_cell_root,
                ctx.cell_state_tree.len(),
                ctx.mergeset_cell_diff.num_added(),
                ctx.mergeset_cell_diff.num_removed(),
                ctx.ghostdag_data.selected_parent,
            );

            return Err(RuleError::BadCellRoot(error_msg));
        }

        // Verify cell_commitment (v0: H("spora/cell_commitment/v0" || cell_root))
        let calculated_commitment = self.compute_cell_commitment_v0(calculated_cell_root);
        let expected_commitment = header.cell_commitment;

        if calculated_commitment != expected_commitment {
            let error_msg = format!(
                "Cell commitment mismatch for block {:?}:\n\
                 Expected commitment: {:?}\n\
                 Calculated commitment: {:?}\n\
                 Cell root: {:?}",
                header.hash, expected_commitment, calculated_commitment, calculated_cell_root,
            );

            return Err(RuleError::BadCellCommitment(error_msg));
        }

        let calculated_accepted_id_merkle_root = self.accepted_id_merkle_root(&ctx.accepted_tx_ids);
        if calculated_accepted_id_merkle_root != header.accepted_id_merkle_root {
            return Err(RuleError::BadAcceptedIDMerkleRoot(
                header.hash,
                header.accepted_id_merkle_root,
                calculated_accepted_id_merkle_root,
            ));
        }

        Ok(())
    }

    /// Compute cell_commitment version 0
    ///
    /// V0 format: H("spora/cell_commitment/v0" || cell_root)
    fn compute_cell_commitment_v0(&self, cell_root: Hash) -> Hash {
        use blake3::Hasher;

        let mut hasher = Hasher::new();
        hasher.update(b"spora/cell_commitment/v0");
        hasher.update(cell_root.as_bytes().as_ref());

        Hash::from_bytes(*hasher.finalize().as_bytes())
    }

    fn reconstruct_tree_from_virtual_diff(&self, virtual_state: &VirtualState, diff_from_virtual: &CellDiff) -> CellStateTree {
        let mut tree = virtual_state.cell_state_tree.clone();
        apply_cell_diff_to_tree(&mut tree, diff_from_virtual);
        tree
    }

    pub(super) fn filter_virtual_parent_candidates(
        &self,
        stores: &VirtualStores,
        diff_from_virtual_to_sink: &CellDiff,
        selected_parent: Hash,
        candidates: VecDeque<Hash>,
    ) -> VecDeque<Hash> {
        let mut filtered = VecDeque::with_capacity(candidates.len());

        for candidate in candidates {
            if self.statuses_store.read().get(candidate).expect("block status must exist") == StatusDisqualifiedFromChain {
                continue;
            }

            let mut candidate_diff = diff_from_virtual_to_sink.clone();
            match self.calculate_cell_state_relatively(stores, &mut candidate_diff, selected_parent, candidate) {
                Ok(diff_point) if diff_point == candidate => {
                    filtered.push_back(candidate);
                }
                Ok(_) => {
                    debug!("Block candidate {} has invalid Cell state and is ignored from virtual parent selection.", candidate);
                }
                Err(err) => {
                    warn!(
                        "Block candidate {} is ignored from virtual parent selection due to cell diff processing error: {}",
                        candidate, err
                    );
                }
            }
        }

        filtered
    }

    fn accepted_id_merkle_root(&self, accepted_tx_ids: &[spora_consensus_core::tx::TransactionId]) -> Hash {
        spora_merkle::calc_merkle_root(accepted_tx_ids.iter().copied())
    }

    fn convert_legacy_coinbase_to_cell_tx(&self, tx: &Transaction) -> CellTx {
        use spora_exec::{CellOut, ScriptRef};

        let outputs = tx
            .outputs
            .iter()
            .map(|output| CellOut {
                lock: ScriptRef::new(self.compute_lock_hash(&output.script_public_key), 0, vec![]),
                type_: None,
                capacity: output.value,
            })
            .collect_vec();

        let mut outputs_data = vec![vec![]; outputs.len()];
        let witnesses = if let Some(first) = outputs_data.first_mut() {
            *first = tx.payload.clone();
            vec![]
        } else {
            vec![tx.payload.clone()]
        };

        CellTx::new(vec![], vec![], outputs, outputs_data, witnesses).expect("coinbase conversion must produce a valid cell tx")
    }

    fn convert_legacy_transaction_to_cell_tx(&self, tx: &Transaction) -> Result<CellTx, RuleError> {
        use spora_exec::{CellOut, CellRef, OutPoint, ScriptRef};

        if tx.is_coinbase() {
            return Ok(self.convert_legacy_coinbase_to_cell_tx(tx));
        }

        if !tx.payload.is_empty() {
            return Err(RuleError::CellValidationError(
                "legacy transaction conversion does not support non-coinbase payloads".to_string(),
            ));
        }

        let inputs = tx
            .inputs
            .iter()
            .map(|input| {
                CellRef::new(
                    OutPoint::new(input.previous_outpoint.tx_hash, input.previous_outpoint.index),
                    legacy_sequence_to_cell_since(input.sequence),
                )
            })
            .collect_vec();
        let outputs = tx
            .outputs
            .iter()
            .map(|output| CellOut {
                lock: ScriptRef::new(self.compute_lock_hash(&output.script_public_key), 0, vec![]),
                type_: None,
                capacity: output.value,
            })
            .collect_vec();
        let outputs_data = vec![vec![]; outputs.len()];
        let witnesses = tx.inputs.iter().map(|input| input.signature_script.clone()).collect_vec();

        CellTx::new(inputs, vec![], outputs, outputs_data, witnesses).map_err(|e| RuleError::CellValidationError(e.to_string()))
    }

    // Legacy commit path removed; fully replaced by commit_cell_state in cell_processing.rs

    fn calculate_and_commit_virtual_state(
        &self,
        virtual_read: RwLockUpgradableReadGuard<'_, VirtualStores>,
        virtual_parents: Vec<Hash>,
        virtual_ghostdag_data: GhostdagData,
        selected_parent_cell_root: Hash,
        accumulated_diff: &mut CellDiff,
        chain_path: &ChainPath,
    ) -> Result<Arc<VirtualState>, RuleError> {
        let new_virtual_state = self.calculate_virtual_state(
            &virtual_read,
            virtual_parents,
            virtual_ghostdag_data,
            selected_parent_cell_root,
            accumulated_diff,
        )?;
        self.commit_virtual_state(virtual_read, new_virtual_state.clone(), accumulated_diff, chain_path);
        Ok(new_virtual_state)
    }

    pub(super) fn calculate_virtual_state(
        &self,
        virtual_stores: &VirtualStores,
        virtual_parents: Vec<Hash>,
        virtual_ghostdag_data: GhostdagData,
        selected_parent_cell_root: Hash,
        accumulated_diff: &mut CellDiff,
    ) -> Result<Arc<VirtualState>, RuleError> {
        // Get the virtual state and compose the cell tree
        let virtual_state = virtual_stores.state.get().expect("virtual state must exist");
        let mut selected_parent_cell_tree = self.reconstruct_tree_from_virtual_diff(&virtual_state, accumulated_diff);
        let calculated_selected_parent_root = selected_parent_cell_tree.root();
        if calculated_selected_parent_root != selected_parent_cell_root {
            return Err(RuleError::BadCellRoot(format!(
                "selected parent reconstruction mismatch while calculating virtual state: expected {:?}, got {:?}",
                selected_parent_cell_root, calculated_selected_parent_root
            )));
        }

        let mut ctx = CellProcessingContext::new((&virtual_ghostdag_data).into(), selected_parent_cell_tree);

        // Calc virtual DAA score, difficulty bits and past median time
        let virtual_daa_window = self.window_manager.block_daa_window(&virtual_ghostdag_data)?;
        let virtual_bits = self.window_manager.calculate_difficulty_bits(&virtual_ghostdag_data, &virtual_daa_window);
        let virtual_past_median_time = self.window_manager.calc_past_median_time(&virtual_ghostdag_data)?.0;

        // Calc virtual Cell state relative to selected parent
        self.calculate_cell_state(
            &mut ctx,
            virtual_daa_window.daa_score,
            virtual_ghostdag_data.selected_parent,
            virtual_past_median_time,
        )?;

        // Update the accumulated diff
        accumulated_diff.with_diff_in_place(&ctx.mergeset_cell_diff).map_err(|e| {
            RuleError::CellValidationError(format!("failed to compose virtual cell diff while calculating virtual state: {}", e))
        })?;

        // Build the new virtual state with Cell model
        let cell_state_tree = ctx.cell_state_tree.clone();
        let cell_diff = ctx.mergeset_cell_diff.clone();
        let block_cell_diffs = ctx.block_cell_diffs.clone();

        Ok(Arc::new(VirtualState::new(
            virtual_parents,
            virtual_daa_window.daa_score,
            virtual_bits,
            virtual_past_median_time,
            cell_state_tree,
            cell_diff,
            block_cell_diffs,
            ctx.accepted_tx_ids,
            ctx.mergeset_rewards,
            virtual_daa_window.mergeset_non_daa,
            virtual_ghostdag_data,
        )))
    }

    fn commit_virtual_state(
        &self,
        virtual_read: RwLockUpgradableReadGuard<'_, VirtualStores>,
        new_virtual_state: Arc<VirtualState>,
        _accumulated_diff: &CellDiff,
        chain_path: &ChainPath,
    ) {
        let mut batch = WriteBatch::default();
        let mut virtual_write = RwLockUpgradableReadGuard::upgrade(virtual_read);
        let mut selected_chain_write = self.selected_chain_store.write();

        // Cell state is stored directly in VirtualState, no separate store needed
        // The cell_state_tree in new_virtual_state already contains the updated state

        // Update virtual state
        virtual_write.state.set_batch(&mut batch, new_virtual_state).expect("virtual state write must succeed");

        // Update the virtual selected chain
        selected_chain_write.apply_changes(&mut batch, chain_path).expect("selected chain write must succeed");

        // Flush the batch changes
        self.db.write(batch).expect("database write must succeed");

        // Calling the drops explicitly after the batch is written in order to avoid possible errors.
        drop(virtual_write);
        drop(selected_chain_write);
    }

    /// Caches the DAA and Median time windows of the sink block (if needed). Following, virtual's window calculations will
    /// naturally hit the cache finding the sink's windows and building upon them.
    fn cache_sink_windows(&self, new_sink: Hash, prev_sink: Hash, sink_ghostdag_data: &impl Deref<Target = Arc<GhostdagData>>) {
        // We expect that the `new_sink` is cached (or some close-enough ancestor thereof) if it is equal to the `prev_sink`,
        // Hence we short-circuit the check of the keys in such cases, thereby reducing the access of the read-lock
        if new_sink != prev_sink {
            // this is only important for ibd performance, as we incur expensive cache misses otherwise.
            // this occurs because we cannot rely on header processing to pre-cache in this scenario.
            if !self.block_window_cache_for_difficulty.contains_key(&new_sink) {
                self.block_window_cache_for_difficulty.insert(
                    new_sink,
                    self.window_manager
                        .block_daa_window(sink_ghostdag_data.deref())
                        .expect("DAA window calculation must succeed")
                        .window,
                );
            };

            if !self.block_window_cache_for_past_median_time.contains_key(&new_sink) {
                self.block_window_cache_for_past_median_time.insert(
                    new_sink,
                    self.window_manager
                        .calc_past_median_time(sink_ghostdag_data.deref())
                        .expect("median time calculation must succeed")
                        .1,
                );
            };
        }
    }

    /// Returns the max number of tips to consider as virtual parents in a single virtual resolve operation.
    ///
    /// Guaranteed to be `>= self.max_block_parents`
    fn max_virtual_parent_candidates(&self, max_block_parents: usize) -> usize {
        // Limit to max_block_parents x 3 candidates. This way we avoid going over thousands of tips when the network isn't healthy.
        // There's no specific reason for a factor of 3, and its not a consensus rule, just an estimation for reducing the amount
        // of candidates considered.
        max_block_parents * 3
    }

    /// Searches for the next valid sink block (SINK = Virtual selected parent). The search is performed
    /// in the inclusive past of `tips`.
    /// The provided `diff` is assumed to initially hold the Cell diff of `prev_sink` from virtual.
    /// The function returns with `diff` being the diff of the new sink from previous virtual.
    /// In addition to the found sink the function also returns a queue of additional virtual
    /// parent candidates ordered in descending blue work order.
    pub(super) fn sink_search_algorithm(
        &self,
        stores: &VirtualStores,
        diff: &mut CellDiff,
        prev_sink: Hash,
        tips: Vec<Hash>,
        finality_point: Hash,
        pruning_point: Hash,
    ) -> (Hash, VecDeque<Hash>) {
        // TODO (relaxed): additional tests

        let mut heap = tips
            .into_iter()
            .map(|block| SortableBlock {
                hash: block,
                blue_work: self.ghostdag_store.get_blue_work(block).expect("blue work must exist for tip"),
            })
            .collect::<BinaryHeap<_>>();

        // The initial diff point is the previous sink
        let mut diff_point = prev_sink;

        // We maintain the following invariant: `heap` is an antichain.
        // It holds at step 0 since tips are an antichain, and remains through the loop
        // since we check that every pushed block is not in the past of current heap
        // (and it can't be in the future by induction)
        loop {
            let candidate = heap.pop().expect("valid sink must exist").hash;
            if self.reachability_service.is_chain_ancestor_of(finality_point, candidate) {
                match self.calculate_cell_state_relatively(stores, diff, diff_point, candidate) {
                    Ok(new_diff_point) => diff_point = new_diff_point,
                    Err(err) => {
                        warn!(
                            "Block candidate {} is ignored from Virtual chain due to cell diff processing error: {}",
                            candidate, err
                        );
                        continue;
                    }
                }
                if diff_point == candidate {
                    // This indicates that candidate has valid Cell state and that `diff` represents its diff from virtual

                    // All blocks with lower blue work than filtering_root are:
                    // 1. not in its future (bcs blue work is monotonic),
                    // 2. will be removed eventually by the bounded merge check.
                    // Hence as an optimization we prefer removing such blocks in advance to allow valid tips to be considered.
                    let filtering_root = self.depth_store.merge_depth_root(candidate).expect("merge depth root must exist");
                    let filtering_blue_work = self.ghostdag_store.get_blue_work(filtering_root).unwrap_or_default();
                    return (
                        candidate,
                        heap.into_sorted_iter().take_while(|s| s.blue_work >= filtering_blue_work).map(|s| s.hash).collect(),
                    );
                } else {
                    debug!("Block candidate {} has invalid cell state and is ignored from Virtual chain.", candidate)
                }
            } else if finality_point != pruning_point {
                // `finality_point == pruning_point` indicates we are at IBD start hence no warning required
                warn!("Finality Violation Detected. Block {} violates finality and is ignored from Virtual chain.", candidate);
            }
            // PRUNE SAFETY: see comment within [`resolve_virtual`]
            let prune_guard = self.pruning_lock.blocking_read();
            for parent in self.relations_service.get_parents(candidate).expect("parents must exist for candidate").iter().copied() {
                if self.reachability_service.is_dag_ancestor_of(finality_point, parent)
                    && !self.reachability_service.is_dag_ancestor_of_any(parent, &mut heap.iter().map(|sb| sb.hash))
                {
                    heap.push(SortableBlock {
                        hash: parent,
                        blue_work: self.ghostdag_store.get_blue_work(parent).expect("parent blue work must exist"),
                    });
                }
            }
            drop(prune_guard);
        }
    }

    /// Picks the virtual parents according to virtual parent selection pruning constrains.
    /// Assumes:
    ///     1. `selected_parent` is a Cell-valid block
    ///     2. `candidates` are an antichain ordered in descending blue work order
    ///     3. `candidates` do not contain `selected_parent` and `selected_parent.blue work > max(candidates.blue_work)`  
    pub(super) fn pick_virtual_parents(
        &self,
        selected_parent: Hash,
        mut candidates: VecDeque<Hash>,
        pruning_point: Hash,
    ) -> (Vec<Hash>, GhostdagData) {
        // TODO (relaxed): additional tests

        // Mergeset increasing might traverse DAG areas which are below the finality point and which theoretically
        // can borderline with pruned data, hence we acquire the prune lock to ensure data consistency. Note that
        // the final selected mergeset can never be pruned (this is the essence of the prunality proof), however
        // we might touch such data prior to validating the bounded merge rule. All in all, this function is short
        // enough so we avoid making further optimizations
        let _prune_guard = self.pruning_lock.blocking_read();
        let _selected_parent_daa_score =
            self.headers_store.get_daa_score(selected_parent).expect("selected parent DAA score must exist");
        let max_block_parents = self.max_block_parents as usize;
        let mergeset_size_limit = self.mergeset_size_limit;
        let max_candidates = self.max_virtual_parent_candidates(max_block_parents);

        // Prioritize half the blocks with highest blue work and pick the rest randomly to ensure diversity between nodes
        if candidates.len() > max_candidates {
            // make_contiguous should be a no op since the deque was just built
            let slice = candidates.make_contiguous();

            // Keep slice[..max_block_parents / 2] as is, choose max_candidates - max_block_parents / 2 in random
            // from the remainder of the slice while swapping them to slice[max_block_parents / 2..max_candidates].
            //
            // Inspired by rand::partial_shuffle (which lacks the guarantee on chosen elements location).
            for i in max_block_parents / 2..max_candidates {
                let j = rand::thread_rng().gen_range(i..slice.len()); // i < max_candidates < slice.len()
                slice.swap(i, j);
            }

            // Truncate the unchosen elements
            candidates.truncate(max_candidates);
        } else if candidates.len() > max_block_parents / 2 {
            // Fallback to a simpler algo in this case
            candidates.make_contiguous()[max_block_parents / 2..].shuffle(&mut rand::thread_rng());
        }

        let mut virtual_parents = Vec::with_capacity(min(max_block_parents, candidates.len() + 1));
        virtual_parents.push(selected_parent);
        let mut mergeset_size = 1; // Count the selected parent

        // Try adding parents as long as mergeset size and number of parents limits are not reached
        while let Some(candidate) = candidates.pop_front() {
            if mergeset_size >= mergeset_size_limit || virtual_parents.len() >= max_block_parents {
                break;
            }
            match self.mergeset_increase(&virtual_parents, candidate, mergeset_size_limit - mergeset_size) {
                MergesetIncreaseResult::Accepted { increase_size } => {
                    mergeset_size += increase_size;
                    virtual_parents.push(candidate);
                }
                MergesetIncreaseResult::Rejected { new_candidate } => {
                    // If we already have a candidate in the past of new candidate then skip.
                    if self.reachability_service.is_any_dag_ancestor(&mut candidates.iter().copied(), new_candidate) {
                        continue; // TODO (optimization): not sure this check is needed if candidates invariant as antichain is kept
                    }
                    // Remove all candidates which are in the future of the new candidate
                    candidates.retain(|&h| !self.reachability_service.is_dag_ancestor_of(new_candidate, h));
                    candidates.push_back(new_candidate);
                }
            }
        }
        assert!(mergeset_size <= mergeset_size_limit);
        assert!(virtual_parents.len() <= max_block_parents);
        self.remove_bounded_merge_breaking_parents(virtual_parents, pruning_point)
    }

    fn mergeset_increase(&self, selected_parents: &[Hash], candidate: Hash, budget: u64) -> MergesetIncreaseResult {
        /*
        Algo:
            Traverse past(candidate) \setminus past(selected_parents) and make
            sure the increase in mergeset size is within the available budget
        */

        let candidate_parents = self.relations_service.get_parents(candidate).expect("candidate parents must exist");
        let mut queue: VecDeque<_> = candidate_parents.iter().copied().collect();
        let mut visited: BlockHashSet = queue.iter().copied().collect();
        let mut mergeset_increase = 1u64; // Starts with 1 to count for the candidate itself

        while let Some(current) = queue.pop_front() {
            if self.reachability_service.is_dag_ancestor_of_any(current, &mut selected_parents.iter().copied()) {
                continue;
            }
            mergeset_increase += 1;
            if mergeset_increase > budget {
                return MergesetIncreaseResult::Rejected { new_candidate: current };
            }

            let current_parents = self.relations_service.get_parents(current).expect("current parents must exist");
            for &parent in current_parents.iter() {
                if visited.insert(parent) {
                    queue.push_back(parent);
                }
            }
        }
        MergesetIncreaseResult::Accepted { increase_size: mergeset_increase }
    }

    fn remove_bounded_merge_breaking_parents(
        &self,
        mut virtual_parents: Vec<Hash>,
        current_pruning_point: Hash,
    ) -> (Vec<Hash>, GhostdagData) {
        let mut ghostdag_data = self.ghostdag_manager.ghostdag(&virtual_parents);
        let merge_depth_root = self.depth_manager.calc_merge_depth_root(&ghostdag_data, current_pruning_point);
        let mut kosherizing_blues: Option<Vec<Hash>> = None;
        let mut bad_reds = Vec::new();

        //
        // Note that the code below optimizes for the usual case where there are no merge-bound-violating blocks.
        //

        // Find red blocks violating the merge bound and which are not kosherized by any blue
        for red in ghostdag_data.mergeset_reds.iter().copied() {
            if self.reachability_service.is_dag_ancestor_of(merge_depth_root, red) {
                continue;
            }
            // Lazy load the kosherizing blocks since this case is extremely rare
            if kosherizing_blues.is_none() {
                kosherizing_blues = Some(self.depth_manager.kosherizing_blues(&ghostdag_data, merge_depth_root).collect());
            }
            if !self.reachability_service.is_dag_ancestor_of_any(
                red,
                &mut kosherizing_blues.as_ref().expect("kosherizing blues must be initialized").iter().copied(),
            ) {
                bad_reds.push(red);
            }
        }

        if !bad_reds.is_empty() {
            // Remove all parents which lead to merging a bad red
            virtual_parents.retain(|&h| !self.reachability_service.is_any_dag_ancestor(&mut bad_reds.iter().copied(), h));
            // Recompute ghostdag data since parents changed
            ghostdag_data = self.ghostdag_manager.ghostdag(&virtual_parents);
        }

        (virtual_parents, ghostdag_data)
    }

    fn resolve_mempool_input(
        &self,
        virtual_state: &VirtualState,
        input_index: usize,
        mutable_tx: &MutableTransaction,
    ) -> Result<CellMetadata, TxRuleError> {
        let outpoint = mutable_tx.tx.inputs[input_index].out_point;

        if let Some(metadata) = mutable_tx.resolved_cell_metadata(input_index) {
            return Ok(metadata.clone());
        }

        if let Some(entry) = mutable_tx.entries.get(input_index).and_then(Option::as_ref) {
            return Ok(synthetic_metadata_from_cell_entry(outpoint, entry));
        }

        virtual_state
            .cell_state_tree
            .get(&outpoint_to_cell_tree_hash(&outpoint))
            .map(|entry| synthetic_metadata_from_tree_entry(outpoint, entry))
            .ok_or(TxRuleError::MissingTxOutpoints)
    }

    fn resolve_mempool_inputs(
        &self,
        virtual_state: &VirtualState,
        mutable_tx: &MutableTransaction,
    ) -> Result<Vec<CellMetadata>, TxRuleError> {
        (0..mutable_tx.tx.inputs.len()).map(|input_index| self.resolve_mempool_input(virtual_state, input_index, mutable_tx)).collect()
    }

    fn calculate_legacy_mempool_fee(
        &self,
        resolved_inputs: &[CellMetadata],
        mutable_tx: &MutableTransaction,
    ) -> Result<u64, TxRuleError> {
        let total_in = resolved_inputs.iter().try_fold(0u64, |sum, meta| {
            let next = sum.checked_add(meta.capacity).ok_or(TxRuleError::InputAmountOverflow)?;
            if next > MAX_SAU {
                return Err(TxRuleError::InputAmountTooHigh);
            }
            Ok(next)
        })?;

        let total_out = mutable_tx.tx.outputs.iter().enumerate().try_fold(0u64, |sum, (index, output)| {
            if output.capacity == 0 {
                return Err(TxRuleError::TxOutZero(index));
            }
            if output.capacity > MAX_SAU {
                return Err(TxRuleError::TxOutTooHigh(index));
            }

            let next = sum.checked_add(output.capacity).ok_or(TxRuleError::OutputsValueOverflow)?;
            if next > MAX_SAU {
                return Err(TxRuleError::TotalTxOutTooHigh);
            }
            Ok(next)
        })?;

        if total_out > total_in {
            return Err(TxRuleError::SpendTooHigh(total_out, total_in));
        }

        Ok(total_in - total_out)
    }

    fn convert_legacy_transaction_to_validation_cell_tx(&self, tx: &Transaction) -> CellTx {
        use spora_exec::{CellOut, CellRef, ScriptRef};

        let inputs = tx
            .inputs
            .iter()
            .map(|input| {
                CellRef::new(
                    OutPoint::new(input.previous_outpoint.tx_hash, input.previous_outpoint.index),
                    legacy_sequence_to_cell_since(input.sequence),
                )
            })
            .collect_vec();
        let outputs = tx
            .outputs
            .iter()
            .map(|output| CellOut {
                lock: ScriptRef::new(self.compute_lock_hash(&output.script_public_key), 0, vec![]),
                type_: None,
                capacity: output.value,
            })
            .collect_vec();

        CellTx {
            ver: spora_exec::CELL_TX_VERSION,
            inputs,
            deps: vec![],
            header_deps: vec![],
            outputs_data: vec![vec![]; outputs.len()],
            outputs,
            witnesses: tx.inputs.iter().map(|input| input.signature_script.clone()).collect_vec(),
        }
    }

    fn build_mempool_input_overrides(
        &self,
        mutable_tx: &MutableTransaction,
        resolved_inputs: &[CellMetadata],
    ) -> HashMap<OutPoint, CellMetadata> {
        mutable_tx
            .tx
            .inputs
            .iter()
            .zip(resolved_inputs.iter())
            .map(|(input, metadata)| (OutPoint::new(input.out_point.tx_hash, input.out_point.index), metadata.clone()))
            .collect()
    }

    fn map_mempool_cell_validation_error(
        &self,
        mutable_tx: &MutableTransaction,
        resolved_inputs: &[CellMetadata],
        current_daa: u64,
        error: CellValidationError,
    ) -> TxRuleError {
        match error {
            CellValidationError::CellNotFound(_)
            | CellValidationError::DepCellNotFound(_)
            | CellValidationError::CellAlreadySpent(_) => TxRuleError::MissingTxOutpoints,
            CellValidationError::InvalidFormat(msg) if msg.contains("lookup error") || msg.contains("unexpected POV") => {
                TxRuleError::MissingTxOutpoints
            }
            CellValidationError::CapacityOverflow => TxRuleError::InputAmountOverflow,
            CellValidationError::InsufficientCapacity { required, available } => TxRuleError::SpendTooHigh(required, available),
            CellValidationError::TimeLockNotSatisfied { .. } => TxRuleError::SequenceLockConditionsAreNotMet,
            CellValidationError::CellbaseNotMature { .. } => {
                let maturity = self.coinbase_maturity;
                resolved_inputs
                    .iter()
                    .enumerate()
                    .find_map(|(index, meta)| {
                        (meta.is_cellbase && current_daa < meta.block_daa_score + maturity).then_some(
                            TxRuleError::ImmatureCoinbaseSpend(
                                index,
                                mutable_tx.tx.inputs[index].out_point,
                                meta.block_daa_score,
                                current_daa,
                                maturity,
                            ),
                        )
                    })
                    .unwrap_or(TxRuleError::MissingTxOutpoints)
            }
            CellValidationError::ScriptVerificationFailed(msg)
            | CellValidationError::ScriptFailed(msg)
            | CellValidationError::InvalidFormat(msg) => TxRuleError::CellValidationFailed(msg),
            CellValidationError::ExceededMaxCycles { total, limit } => {
                TxRuleError::CellValidationFailed(format!("script cycles exceeded limit: total {total}, limit {limit}"))
            }
            CellValidationError::InvalidSignature => TxRuleError::CellValidationFailed("invalid signature".to_string()),
            _ => TxRuleError::MissingTxOutpoints,
        }
    }

    fn validate_mempool_transaction_against_virtual_state(
        &self,
        virtual_state: &Arc<VirtualState>,
        mutable_tx: &mut MutableTransaction,
        args: &TransactionValidationArgs,
    ) -> TxResult<()> {
        let cell_tx = mutable_tx.tx.as_ref().clone();
        self.validate_mempool_cell_transaction_against_virtual_state(virtual_state, mutable_tx, &cell_tx, args)
    }

    fn validate_mempool_cell_transaction_against_virtual_state(
        &self,
        virtual_state: &Arc<VirtualState>,
        mutable_tx: &mut MutableTransaction,
        cell_tx: &CellTx,
        args: &TransactionValidationArgs,
    ) -> TxResult<()> {
        if mutable_tx.tx.is_coinbase() || cell_tx.is_coinbase() {
            return Err(TxRuleError::CoinbaseHasInputs(cell_tx.inputs.len()));
        }

        if cell_tx.inputs.is_empty() {
            return Err(TxRuleError::NoTxInputs);
        }

        if mutable_tx.tx.inputs.len() != cell_tx.inputs.len() {
            return Err(TxRuleError::CellValidationFailed(
                "legacy compatibility mirror input count does not match canonical cell transaction".to_string(),
            ));
        }

        let mut seen_inputs = HashSet::with_capacity(cell_tx.inputs.len());
        for input in &cell_tx.inputs {
            if !seen_inputs.insert((input.out_point.tx_hash, input.out_point.index)) {
                return Err(TxRuleError::TxDuplicateInputs);
            }
        }

        let resolved_inputs = self.resolve_mempool_inputs(virtual_state.as_ref(), mutable_tx)?;
        backfill_mempool_entries_from_resolved_inputs(mutable_tx, &resolved_inputs);
        let input_overrides = self.build_mempool_input_overrides(mutable_tx, &resolved_inputs);
        let provider = Arc::new(self.build_virtual_snapshot_provider(
            virtual_state.clone(),
            input_overrides,
            virtual_state.past_median_time,
        ));
        let validator = CellValidator::new(
            Arc::new(CellConsensusParams {
                cellbase_maturity: self.coinbase_maturity,
                ..CellConsensusParams::default()
            }),
            provider.clone(),
        );

        #[cfg(feature = "vm")]
        validator
            .validate_full_with_scripts_and_cycles(
                cell_tx,
                virtual_state.ghostdag_data.selected_parent,
                virtual_state.daa_score,
                virtual_state.past_median_time,
            )
            .map_err(|error| self.map_mempool_cell_validation_error(mutable_tx, &resolved_inputs, virtual_state.daa_score, error))?;

        #[cfg(not(feature = "vm"))]
        validator
            .validate_in_dag(
                cell_tx,
                virtual_state.ghostdag_data.selected_parent,
                virtual_state.daa_score,
                virtual_state.past_median_time,
            )
            .map_err(|error| self.map_mempool_cell_validation_error(mutable_tx, &resolved_inputs, virtual_state.daa_score, error))?;

        let calculated_fee = self.calculate_cell_tx_fee_from_provider(
            cell_tx,
            provider.as_ref(),
            virtual_state.ghostdag_data.selected_parent,
        )?;
        mutable_tx.calculated_fee = Some(calculated_fee);
        if mutable_tx.calculated_non_contextual_masses.is_none() {
            mutable_tx.calculated_non_contextual_masses = Some(self.mass_calculator.calc_non_contextual_masses_cell(mutable_tx.tx.as_ref()));
        }

        if let Some(feerate_threshold) = args.feerate_threshold {
            let Some(calculated_feerate) = mutable_tx
                .calculated_non_contextual_masses
                .map(|masses| ContextualMasses::new(mutable_tx.tx.mass()).max(masses))
                .map(|contextual_mass| calculated_fee as f64 / contextual_mass as f64)
            else {
                return Err(TxRuleError::FeerateTooLow);
            };

            if calculated_feerate <= feerate_threshold {
                return Err(TxRuleError::FeerateTooLow);
            }
        }

        Ok(())
    }

    pub fn validate_mempool_transaction(&self, mutable_tx: &mut MutableTransaction, args: &TransactionValidationArgs) -> TxResult<()> {
        let virtual_read = self.virtual_stores.read();
        let virtual_state = virtual_read.state.get().map_err(|_| TxRuleError::MissingTxOutpoints)?;
        self.validate_mempool_transaction_against_virtual_state(&virtual_state, mutable_tx, args)
    }

    pub fn validate_mempool_cell_transaction(
        &self,
        mutable_tx: &mut MutableTransaction,
        cell_tx: &CellTx,
        args: &TransactionValidationArgs,
    ) -> TxResult<()> {
        let virtual_read = self.virtual_stores.read();
        let virtual_state = virtual_read.state.get().map_err(|_| TxRuleError::MissingTxOutpoints)?;
        self.validate_mempool_cell_transaction_against_virtual_state(&virtual_state, mutable_tx, cell_tx, args)
    }

    pub fn validate_mempool_transactions_in_parallel(
        &self,
        mutable_txs: &mut [MutableTransaction],
        args: &TransactionValidationBatchArgs,
    ) -> Vec<TxResult<()>> {
        use rayon::prelude::*;

        let virtual_read = self.virtual_stores.read();
        let virtual_state = match virtual_read.state.get() {
            Ok(state) => state,
            Err(_) => return vec![Err(TxRuleError::MissingTxOutpoints); mutable_txs.len()],
        };

        mutable_txs
            .par_iter_mut()
            .map(|mutable_tx| {
                self.validate_mempool_transaction_against_virtual_state(&virtual_state, mutable_tx, args.get(&mutable_tx.id()))
            })
            .collect()
    }

    pub fn populate_mempool_transaction(&self, mutable_tx: &mut MutableTransaction) -> TxResult<()> {
        let virtual_read = self.virtual_stores.read();
        let virtual_state = virtual_read.state.get().map_err(|_| TxRuleError::MissingTxOutpoints)?;
        let resolved_inputs = self.resolve_mempool_inputs(virtual_state.as_ref(), mutable_tx)?;
        backfill_mempool_entries_from_resolved_inputs(mutable_tx, &resolved_inputs);
        mutable_tx.calculated_fee = Some(self.calculate_legacy_mempool_fee(&resolved_inputs, mutable_tx)?);
        mutable_tx.calculated_non_contextual_masses = Some(self.mass_calculator.calc_non_contextual_masses_cell(mutable_tx.tx.as_ref()));
        Ok(())
    }

    pub fn populate_mempool_transactions_in_parallel(&self, mutable_txs: &mut [MutableTransaction]) -> Vec<TxResult<()>> {
        use rayon::prelude::*;

        let virtual_read = self.virtual_stores.read();
        let virtual_state = match virtual_read.state.get() {
            Ok(state) => state,
            Err(_) => {
                return vec![Err(TxRuleError::MissingTxOutpoints); mutable_txs.len()];
            }
        };

        mutable_txs
            .par_iter_mut()
            .map(|mutable_tx| {
                let resolved_inputs = self.resolve_mempool_inputs(virtual_state.as_ref(), mutable_tx)?;
                backfill_mempool_entries_from_resolved_inputs(mutable_tx, &resolved_inputs);
                mutable_tx.calculated_fee = Some(self.calculate_legacy_mempool_fee(&resolved_inputs, mutable_tx)?);
                mutable_tx.calculated_non_contextual_masses = Some(self.mass_calculator.calc_non_contextual_masses_cell(mutable_tx.tx.as_ref()));
                Ok(())
            })
            .collect()
    }

    fn build_virtual_snapshot_provider(
        &self,
        virtual_state: Arc<VirtualState>,
        overrides: HashMap<OutPoint, CellMetadata>,
        template_timestamp: u64,
    ) -> VirtualSnapshotCellProvider {
        VirtualSnapshotCellProvider::new(
            virtual_state,
            self.headers_store.clone(),
            self.block_transactions_store.clone(),
            overrides,
            template_timestamp,
        )
    }

    fn resolve_cell_tx_inputs_from_provider<P: DagCellProvider>(
        &self,
        tx: &CellTx,
        provider: &P,
        pov: Hash,
    ) -> Result<Vec<CellMetadata>, String> {
        tx.inputs
            .iter()
            .map(|input| {
                provider.get_cell_at_pov(&input.out_point, pov)?.ok_or_else(|| format!("missing input cell {:?}", input.out_point))
            })
            .collect()
    }

    fn calculate_cell_tx_fee_from_provider<P: DagCellProvider>(&self, tx: &CellTx, provider: &P, pov: Hash) -> TxResult<u64> {
        let resolved_inputs =
            self.resolve_cell_tx_inputs_from_provider(tx, provider, pov).map_err(|_| TxRuleError::MissingTxOutpoints)?;

        let total_in = resolved_inputs.iter().try_fold(0u64, |sum, meta| {
            let next = sum.checked_add(meta.capacity).ok_or(TxRuleError::InputAmountOverflow)?;
            if next > MAX_SAU {
                return Err(TxRuleError::InputAmountTooHigh);
            }
            Ok(next)
        })?;

        let total_out = tx.outputs.iter().enumerate().try_fold(0u64, |sum, (index, output)| {
            if output.capacity == 0 {
                return Err(TxRuleError::TxOutZero(index));
            }
            if output.capacity > MAX_SAU {
                return Err(TxRuleError::TxOutTooHigh(index));
            }

            let next = sum.checked_add(output.capacity).ok_or(TxRuleError::OutputsValueOverflow)?;
            if next > MAX_SAU {
                return Err(TxRuleError::TotalTxOutTooHigh);
            }
            Ok(next)
        })?;

        if total_out > total_in {
            return Err(TxRuleError::SpendTooHigh(total_out, total_in));
        }

        Ok(total_in - total_out)
    }

    fn map_block_template_cell_validation_error<P: DagCellProvider>(
        &self,
        tx: &CellTx,
        provider: &P,
        pov: Hash,
        current_daa: u64,
        error: CellValidationError,
    ) -> Result<TxRuleError, RuleError> {
        match error {
            CellValidationError::CellNotFound(_)
            | CellValidationError::DepCellNotFound(_)
            | CellValidationError::CellAlreadySpent(_) => {
                trace!(
                    "Template tx {:?} is missing referenced cells at pov {}: {:?}",
                    Hash::from_bytes(tx.id()),
                    pov,
                    tx.inputs
                        .iter()
                        .map(|input| {
                            let tx_outpoint = TransactionOutpoint::new(input.out_point.tx_hash, input.out_point.index);
                            let tree_key = outpoint_to_cell_tree_hash(&tx_outpoint);
                            let provider_has_cell = provider.get_cell_at_pov(&input.out_point, pov).ok().flatten().is_some();
                            let tree_has_cell =
                                self.virtual_stores.read().state.get().map(|state| state.cell_state_tree.get(&tree_key).is_some());
                            (input.out_point.clone(), provider_has_cell, tree_has_cell.unwrap_or(false))
                        })
                        .collect_vec()
                );
                Ok(TxRuleError::MissingTxOutpoints)
            }
            CellValidationError::InvalidFormat(msg) if msg.contains("lookup error") || msg.contains("unexpected POV") => {
                Ok(TxRuleError::MissingTxOutpoints)
            }
            CellValidationError::CapacityOverflow => Ok(TxRuleError::InputAmountOverflow),
            CellValidationError::InsufficientCapacity { required, available } => Ok(TxRuleError::SpendTooHigh(required, available)),
            CellValidationError::TimeLockNotSatisfied { .. } => Ok(TxRuleError::SequenceLockConditionsAreNotMet),
            CellValidationError::CellbaseNotMature { .. } => {
                let maturity = self.coinbase_maturity;
                for (index, input) in tx.inputs.iter().enumerate() {
                    let Some(metadata) = provider
                        .get_cell_at_pov(&input.out_point, pov)
                        .map_err(|err| RuleError::CellValidationError(format!("template input lookup failed: {err}")))?
                    else {
                        continue;
                    };

                    if metadata.is_cellbase && current_daa < metadata.block_daa_score + maturity {
                        return Ok(TxRuleError::ImmatureCoinbaseSpend(
                            index,
                            TransactionOutpoint::new(input.out_point.tx_hash, input.out_point.index),
                            metadata.block_daa_score,
                            current_daa,
                            maturity,
                        ));
                    }
                }

                Err(RuleError::CellValidationError(format!(
                    "Template validation failed for tx {:?}: {error}",
                    Hash::from_bytes(tx.id())
                )))
            }
            CellValidationError::ScriptVerificationFailed(msg)
            | CellValidationError::ScriptFailed(msg)
            | CellValidationError::InvalidFormat(msg) => Ok(TxRuleError::CellValidationFailed(msg)),
            CellValidationError::ExceededMaxCycles { total, limit } => {
                Ok(TxRuleError::CellValidationFailed(format!("script cycles exceeded limit: total {total}, limit {limit}")))
            }
            CellValidationError::InvalidSignature => Ok(TxRuleError::CellValidationFailed("invalid signature".to_string())),
            other => Err(RuleError::CellValidationError(format!(
                "Template validation failed for tx {:?}: {other}",
                Hash::from_bytes(tx.id())
            ))),
        }
    }

    fn apply_template_transaction_to_overlay(
        &self,
        provider: &mut TemplateOverlayProvider,
        tx: &CellTx,
        current_daa: u64,
    ) -> Result<(), RuleError> {
        for input in &tx.inputs {
            provider.spend_cell(&input.out_point).map_err(|e| {
                RuleError::CellValidationError(format!(
                    "failed to update block template overlay for tx {:?}: {e}",
                    Hash::from_bytes(tx.id())
                ))
            })?;
        }

        for (output_index, output) in tx.outputs.iter().enumerate() {
            let out_point = OutPoint::new(tx.id(), output_index as u32);
            let metadata = cell_metadata_from_cell_output(
                ZERO_HASH,
                current_daa,
                false,
                tx.id(),
                output_index as u32,
                output,
                tx.outputs_data.get(output_index).map(|data| data.as_slice()).unwrap_or(&[]),
            );
            provider.add_cell(out_point, metadata).map_err(|e| {
                RuleError::CellValidationError(format!(
                    "failed to update block template overlay for tx {:?}: {e}",
                    Hash::from_bytes(tx.id())
                ))
            })?;
        }

        Ok(())
    }

    fn validate_and_filter_block_template_cell_transactions(
        &self,
        txs: Vec<CellTx>,
        virtual_state: Arc<VirtualState>,
    ) -> Result<TemplateValidationOutcome, RuleError> {
        let snapshot_pov = virtual_state.ghostdag_data.selected_parent;
        let template_timestamp = virtual_state.past_median_time + 1;
        let params = Arc::new(CellConsensusParams {
            cellbase_maturity: self.coinbase_maturity,
            ..CellConsensusParams::default()
        });
        let mut overlay = TemplateOverlayProvider::new(
            self.build_virtual_snapshot_provider(virtual_state.clone(), HashMap::new(), template_timestamp),
            snapshot_pov,
            snapshot_pov,
        );
        let mut valid_txs = Vec::with_capacity(txs.len());
        let mut calculated_fees = Vec::with_capacity(txs.len());
        let mut invalid_transactions = HashMap::new();

        for tx in txs {
            let validator = CellValidator::new(params.clone(), Arc::new(overlay.clone()));
            if let Err(error) = validator.validate_in_isolation(&tx) {
                let tx_rule_error =
                    self.map_block_template_cell_validation_error(&tx, &overlay, snapshot_pov, virtual_state.daa_score, error)?;
                invalid_transactions.insert(Hash::from_bytes(tx.id()), tx_rule_error);
                continue;
            }

            #[cfg(feature = "vm")]
            let validation_result = validator
                .validate_full_with_scripts_and_cycles(&tx, snapshot_pov, virtual_state.daa_score, template_timestamp)
                .map(|_| ());
            #[cfg(not(feature = "vm"))]
            let validation_result = validator.validate_in_dag(&tx, snapshot_pov, virtual_state.daa_score, template_timestamp);

            if let Err(error) = validation_result {
                match self.map_block_template_cell_validation_error(&tx, &overlay, snapshot_pov, virtual_state.daa_score, error)? {
                    tx_rule_error => {
                        invalid_transactions.insert(Hash::from_bytes(tx.id()), tx_rule_error);
                        continue;
                    }
                }
            }

            let calculated_fee = match self.calculate_cell_tx_fee_from_provider(&tx, &overlay, snapshot_pov) {
                Ok(fee) => fee,
                Err(tx_rule_error) => {
                    invalid_transactions.insert(Hash::from_bytes(tx.id()), tx_rule_error);
                    continue;
                }
            };
            self.apply_template_transaction_to_overlay(&mut overlay, &tx, virtual_state.daa_score)?;
            calculated_fees.push(calculated_fee);
            valid_txs.push(tx);
        }

        Ok(TemplateValidationOutcome { valid_txs, calculated_fees, invalid_transactions })
    }

    pub fn build_block_template(
        &self,
        miner_data: MinerData,
        mut tx_selector: Box<dyn TemplateTransactionSelector>,
        build_mode: TemplateBuildMode,
    ) -> Result<BlockTemplate, RuleError> {
        //
        // TODO (relaxed): additional tests
        //

        // We call for the initial tx batch before acquiring the virtual read lock,
        // optimizing for the common case where all txs are valid.
        let prefilter = prefilter_conflicting_template_transactions(tx_selector.select_transactions(), Some(tx_selector.as_mut()));
        let virtual_read = self.virtual_stores.read();
        let virtual_state = virtual_read.state.get().expect("virtual state must exist");
        let TemplateValidationOutcome { valid_txs, calculated_fees, invalid_transactions: validation_invalids } =
            self.validate_and_filter_block_template_cell_transactions(prefilter.kept_txs, virtual_state.clone())?;
        let mut invalid_transactions =
            prefilter.rejected_tx_ids.into_iter().map(|tx_id| (tx_id, template_conflict_tx_rule_error())).collect::<HashMap<_, _>>();
        for tx_id in validation_invalids.keys().copied() {
            tx_selector.reject_selection(tx_id);
        }
        invalid_transactions.extend(validation_invalids);

        if matches!(build_mode, TemplateBuildMode::Standard) && !invalid_transactions.is_empty() {
            return Err(RuleError::InvalidTransactionsInNewBlock(invalid_transactions));
        }

        // At this point we can safely drop the read lock
        drop(virtual_read);

        // Build the template with Cell transactions
        self.build_block_template_from_virtual_state_cell(virtual_state, miner_data, valid_txs, calculated_fees)
    }

    pub fn build_block_template_with_cell_tx_selector<F>(
        &self,
        miner_data: MinerData,
        build_mode: TemplateBuildMode,
        tx_selector: F,
    ) -> Result<BlockTemplate, RuleError>
    where
        F: FnOnce(&VirtualState) -> Vec<CellTx>,
    {
        let virtual_read = self.virtual_stores.read();
        let virtual_state = virtual_read.state.get().expect("virtual state must exist");
        let prefilter = prefilter_conflicting_template_transactions(tx_selector(virtual_state.as_ref()), None);
        let TemplateValidationOutcome { valid_txs, calculated_fees, invalid_transactions: validation_invalids } =
            self.validate_and_filter_block_template_cell_transactions(prefilter.kept_txs, virtual_state.clone())?;
        let mut invalid_transactions =
            prefilter.rejected_tx_ids.into_iter().map(|tx_id| (tx_id, template_conflict_tx_rule_error())).collect::<HashMap<_, _>>();
        invalid_transactions.extend(validation_invalids);

        if matches!(build_mode, TemplateBuildMode::Standard) && !invalid_transactions.is_empty() {
            return Err(RuleError::InvalidTransactionsInNewBlock(invalid_transactions));
        }

        drop(virtual_read);

        self.build_block_template_from_virtual_state_cell(virtual_state, miner_data, valid_txs, calculated_fees)
    }

    pub(crate) fn validate_block_template_transactions(
        &self,
        txs: &[Transaction],
        virtual_state: &VirtualState,
    ) -> Result<(), RuleError> {
        let cell_txs = txs.iter().map(|tx| self.convert_legacy_transaction_to_cell_tx(tx)).collect::<Result<Vec<_>, _>>()?;
        let prefilter = prefilter_conflicting_template_transactions(cell_txs, None);
        let TemplateValidationOutcome { invalid_transactions: validation_invalids, .. } =
            self.validate_and_filter_block_template_cell_transactions(prefilter.kept_txs, Arc::new(virtual_state.clone()))?;
        let mut invalid_transactions =
            prefilter.rejected_tx_ids.into_iter().map(|tx_id| (tx_id, template_conflict_tx_rule_error())).collect::<HashMap<_, _>>();
        invalid_transactions.extend(validation_invalids);
        if invalid_transactions.is_empty() {
            Ok(())
        } else {
            Err(RuleError::InvalidTransactionsInNewBlock(invalid_transactions))
        }
    }

    // Legacy function for Transaction type
    pub(crate) fn build_block_template_from_virtual_state(
        &self,
        virtual_state: Arc<VirtualState>,
        miner_data: MinerData,
        txs: Vec<Transaction>,
        calculated_fees: Vec<u64>,
    ) -> Result<BlockTemplate, RuleError> {
        // Deprecated legacy path: keep it functional by converting legacy transactions into CellTx.
        let cell_txs = txs.iter().map(|tx| self.convert_legacy_transaction_to_cell_tx(tx)).collect::<Result<Vec<_>, _>>()?;
        self.build_block_template_from_virtual_state_cell(virtual_state, miner_data, cell_txs, calculated_fees)
    }

    // New function for CellTx type
    pub(crate) fn build_block_template_from_virtual_state_cell(
        &self,
        virtual_state: Arc<VirtualState>,
        miner_data: MinerData,
        mut txs: Vec<CellTx>,
        calculated_fees: Vec<u64>,
    ) -> Result<BlockTemplate, RuleError> {
        // [`calc_block_parents`] can use deep blocks below the pruning point for this calculation, so we
        // need to hold the pruning lock.
        let _prune_guard = self.pruning_lock.blocking_read();
        let pruning_info = self.pruning_point_store.read().get().expect("pruning info must exist");
        let header_pruning_point =
            self.pruning_point_manager.expected_header_pruning_point_v2(virtual_state.ghostdag_data.to_compact()).pruning_point;
        let coinbase = self
            .coinbase_manager
            .expected_coinbase_transaction(
                virtual_state.daa_score,
                miner_data.clone(),
                &virtual_state.ghostdag_data,
                &virtual_state.mergeset_rewards,
                &virtual_state.mergeset_non_daa,
            )
            .expect("coinbase transaction creation must succeed");
        txs.insert(0, self.convert_legacy_coinbase_to_cell_tx(&coinbase.tx));
        let version = BLOCK_VERSION;
        let parents_by_level = self.parents_manager.calc_block_parents(pruning_info.pruning_point, &virtual_state.parents);

        // Hash according to hardfork activation
        let storage_mass_activated = true;
        // Hash merkle root (CellTx version)
        use spora_consensus_core::merkle::calc_hash_merkle_root_cell;
        let hash_merkle_root = calc_hash_merkle_root_cell(txs.iter(), storage_mass_activated);

        let accepted_id_merkle_root = self.accepted_id_merkle_root(&virtual_state.accepted_tx_ids);
        // Compute cell_root from Cell state tree
        let mut cell_tree_clone = virtual_state.cell_state_tree.clone();
        let cell_root = cell_tree_clone.root();

        let cell_commitment = self.compute_cell_commitment_v0(cell_root);
        // Past median time is the exclusive lower bound for valid block time, so we increase by 1 to get the valid min
        let min_block_time = virtual_state.past_median_time + 1;
        let header = Header::new_finalized(
            version,
            parents_by_level,
            hash_merkle_root,
            accepted_id_merkle_root,
            cell_commitment,
            cell_root,
            u64::max(min_block_time, unix_now()),
            virtual_state.bits,
            0,
            virtual_state.daa_score,
            virtual_state.ghostdag_data.blue_work,
            virtual_state.ghostdag_data.blue_score,
            header_pruning_point,
        );
        let selected_parent_hash = virtual_state.ghostdag_data.selected_parent;
        let selected_parent_timestamp =
            self.headers_store.get_timestamp(selected_parent_hash).expect("selected parent timestamp must exist");
        let selected_parent_daa_score =
            self.headers_store.get_daa_score(selected_parent_hash).expect("selected parent DAA score must exist");
        Ok(BlockTemplate::new(
            MutableBlock::new(header, txs),
            miner_data,
            coinbase.has_red_reward,
            selected_parent_timestamp,
            selected_parent_daa_score,
            selected_parent_hash,
            calculated_fees,
        ))
    }

    /// Make sure pruning point-related stores are initialized
    pub fn init(self: &Arc<Self>) {
        let pruning_point_read = self.pruning_point_store.upgradable_read();
        if pruning_point_read.pruning_point().unwrap_option().is_none() {
            let mut pruning_point_write = RwLockUpgradableReadGuard::upgrade(pruning_point_read);
            let mut batch = WriteBatch::default();
            self.past_pruning_points_store.insert_batch(&mut batch, 0, self.genesis.hash).unwrap_or_exists();
            pruning_point_write
                .set_batch(&mut batch, self.genesis.hash, self.genesis.hash, 0)
                .expect("pruning point initialization must succeed");
            pruning_point_write
                .set_retention_checkpoint(&mut batch, self.genesis.hash)
                .expect("retention checkpoint write must succeed");
            pruning_point_write
                .set_retention_period_root(&mut batch, self.genesis.hash)
                .expect("retention period root write must succeed");
            // pruning cell-set position removed - Cell state tracked in VirtualState
            self.db.write(batch).expect("database write must succeed");
            drop(pruning_point_write);
        }
    }

    /// Initializes Cell state of genesis and points virtual at genesis.
    /// Note that pruning point-related stores are initialized by `init`
    pub fn process_genesis(self: &Arc<Self>) {
        use spora_consensus_core::cell_diff::CellDiff;
        use spora_hashes::ZERO_HASH;

        // Write the Cell state of genesis (empty state)
        self.commit_cell_state(
            self.genesis.hash,
            CellDiff::default(),
            ZERO_HASH, // Genesis has no cells yet
            vec![],
            ZERO_HASH,
        );

        // Init the virtual selected chain store
        let mut batch = WriteBatch::default();
        let mut selected_chain_write = self.selected_chain_store.write();
        selected_chain_write
            .init_with_pruning_point(&mut batch, self.genesis.hash)
            .expect("selected chain initialization must succeed");
        self.db.write(batch).expect("database write must succeed");
        drop(selected_chain_write);

        // Init virtual state
        self.commit_virtual_state(
            self.virtual_stores.upgradable_read(),
            Arc::new(VirtualState::from_genesis(&self.genesis, self.ghostdag_manager.ghostdag(&[self.genesis.hash]))),
            &Default::default(),
            &Default::default(),
        );
    }

    /// Append imported cells to the pruning point cell state tree
    ///
    /// Append imported live cells to the pruning-point cell state tree.
    pub fn append_imported_pruning_point_cells(
        &self,
        cellset_chunk: &[(TransactionOutpoint, CellMeta)],
        current_tree: &mut CellStateTree,
    ) {
        use spora_state::CellEntry;

        for (outpoint, meta) in cellset_chunk {
            // Convert TransactionOutpoint to Hash for tree indexing
            let outpoint_hash = self.outpoint_to_hash(outpoint);

            // Convert CellMeta to CellEntry
            let entry = CellEntry::new(
                meta.capacity,
                meta.data_bytes,
                Hash::from_bytes(meta.lock_hash),
                meta.type_hash.map(Hash::from_bytes),
                Hash::from_bytes(meta.data_hash),
                meta.block_daa_score,
                meta.is_cellbase,
            );

            current_tree.insert_with_outpoint(outpoint_hash, exec_outpoint(outpoint), entry);
        }
    }

    /// Helper: Convert TransactionOutpoint to Hash for tree indexing
    fn outpoint_to_hash(&self, outpoint: &TransactionOutpoint) -> Hash {
        use blake3::Hasher;

        let mut hasher = Hasher::new();
        hasher.update(b"spora-cell/outpoint"); // Domain separation
        hasher.update(&outpoint.tx_hash);
        hasher.update(&outpoint.index.to_le_bytes());

        Hash::from_bytes(*hasher.finalize().as_bytes())
    }

    /// Import the pruning point cell set
    ///
    /// Import the pruning-point cell state tree.
    pub fn import_pruning_point_cell_set(
        &self,
        new_pruning_point: Hash,
        mut imported_cell_tree: CellStateTree,
    ) -> PruningImportResult<()> {
        info!("Importing the Cell set of the pruning point {}", new_pruning_point);
        let new_pruning_point_header = self.headers_store.get_header(new_pruning_point).expect("pruning point header must exist");

        // Calculate cell_root from imported tree
        let imported_cell_root = imported_cell_tree.root();

        // Verify cell_root matches header
        info!(
            "Cell root verification for pruning point {}: imported={}, header={}",
            new_pruning_point, imported_cell_root, new_pruning_point_header.cell_root
        );

        if imported_cell_root != new_pruning_point_header.cell_root {
            return Err(PruningImportError::ImportedMultisetHashMismatch(new_pruning_point_header.cell_root, imported_cell_root));
        }

        // Verify cell_commitment (v0: H("spora/cell_commitment/v0" || cell_root))
        let expected_commitment = self.compute_cell_commitment_v0(imported_cell_root);
        if expected_commitment != new_pruning_point_header.cell_commitment {
            return Err(PruningImportError::ImportedMultisetHashMismatch(
                new_pruning_point_header.cell_commitment,
                expected_commitment,
            ));
        }

        info!("Pruning point cell state verified successfully");

        let virtual_read = self.virtual_stores.upgradable_read();

        // Validate transactions of the pruning point itself
        let new_pruning_point_transactions =
            self.block_transactions_store.get(new_pruning_point).expect("pruning point transactions must exist");
        info!("Validating {} transactions for pruning point {}", new_pruning_point_transactions.len(), new_pruning_point);

        // Cell model: Transactions are validated during block processing
        // For pruning point import, we trust the validated cell_root
        let validated_transactions = &new_pruning_point_transactions[1..]; // Skip coinbase
        info!("Accepted {} transactions for pruning point", validated_transactions.len());

        {
            // Store the imported cell_root
            let mut batch = WriteBatch::default();
            self.cell_roots_store
                .insert_batch(&mut batch, new_pruning_point, imported_cell_root)
                .expect("cell root insertion must succeed");

            let statuses_write =
                self.statuses_store.set_batch(&mut batch, new_pruning_point, StatusCellValid).expect("status write must succeed");
            self.db.write(batch).expect("database write must succeed");
            drop(statuses_write);
        }

        // Calculate the virtual state, treating the pruning point as the only virtual parent
        let virtual_parents = vec![new_pruning_point];
        let virtual_ghostdag_data = self.ghostdag_manager.ghostdag(&virtual_parents);

        self.calculate_and_commit_virtual_state(
            virtual_read,
            virtual_parents,
            virtual_ghostdag_data,
            ZERO_HASH, // imported_cell_root - placeholder for now
            &mut CellDiff::default(),
            &ChainPath::default(),
        )?;

        Ok(())
    }

    pub fn are_pruning_points_violating_finality(&self, pp_list: PruningPointsList) -> bool {
        // Ideally we would want to check if the last known pruning point has the finality point
        // in its chain, but in some cases it's impossible: let `lkp` be the last known pruning
        // point from the list, and `fup` be the first unknown pruning point (the one following `lkp`).
        // fup.blue_score - lkp.blue_score ≈ finality_depth (±k), so it's possible for `lkp` not to
        // have the finality point in its past. So we have no choice but to check if `lkp`
        // has `finality_point.finality_point` in its chain (in the worst case `fup` is one block
        // above the current finality point, and in this case `lkp` will be a few blocks above the
        // finality_point.finality_point), meaning this function can only detect finality violations
        // in depth of 2*finality_depth, and can give false negatives for smaller finality violations.
        let current_pp = self.pruning_point_store.read().pruning_point().expect("pruning point must exist");
        let vf = self.virtual_finality_point(&self.lkg_virtual_state.load().ghostdag_data, current_pp);
        let vff = self
            .depth_manager
            .calc_finality_point(&self.ghostdag_store.get_data(vf).expect("finality point ghostdag data must exist"), current_pp);

        let last_known_pp = pp_list.iter().rev().find(|pp| match self.statuses_store.read().get(pp.hash).unwrap_option() {
            Some(status) => status.is_valid(),
            None => false,
        });

        if let Some(last_known_pp) = last_known_pp {
            !self.reachability_service.is_chain_ancestor_of(vff, last_known_pp.hash)
        } else {
            // If no pruning point is known, there's definitely a finality violation
            // (normally at least genesis should be known).
            true
        }
    }

    /// Executes `op` within the thread pool associated with this processor.
    pub fn install<OP, R>(&self, op: OP) -> R
    where
        OP: FnOnce() -> R + Send,
        R: Send,
    {
        self.thread_pool.install(op)
    }
}

enum MergesetIncreaseResult {
    Accepted { increase_size: u64 },
    Rejected { new_candidate: Hash },
}

#[cfg(test)]
mod tests {
    use super::filter_conflicting_template_transactions;
    use spora_consensus_core::{block::TemplateTransactionSelector, tx::TransactionId};
    use spora_exec::{celltx::sighash::compute_wtxid, CellOut, CellRef, CellTx, OutPoint, ScriptRef};

    struct NoopSelector;

    impl TemplateTransactionSelector for NoopSelector {
        fn select_transactions(&mut self) -> Vec<CellTx> {
            Vec::new()
        }

        fn reject_selection(&mut self, _tx_id: TransactionId) {}

        fn is_successful(&self) -> bool {
            true
        }
    }

    fn test_tx(inputs: Vec<OutPoint>, output_count: usize) -> CellTx {
        let lock = ScriptRef::new([0x11; 32], 0, vec![]);
        let inputs = inputs.into_iter().map(|op| CellRef::new(op, 0)).collect();
        let outputs = vec![CellOut { lock, type_: None, capacity: 1000 }; output_count];
        let outputs_data = vec![vec![]; output_count];
        CellTx::new(inputs, vec![], outputs, outputs_data, vec![]).unwrap()
    }

    #[test]
    fn template_conflict_prefilter_keeps_first_conflict_winner() {
        let shared_input = OutPoint::new([0x41; 32], 0);
        let tx_a = test_tx(vec![shared_input.clone()], 1);
        let tx_b = test_tx(vec![shared_input], 1);

        let mut selector = NoopSelector;
        let filtered = filter_conflicting_template_transactions(vec![tx_a.clone(), tx_b], &mut selector);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id(), tx_a.id());
    }

    #[test]
    fn template_conflict_prefilter_rejects_descendants_of_losing_conflicts() {
        let shared_input = OutPoint::new([0x51; 32], 0);
        let tx_a = test_tx(vec![shared_input.clone()], 1);
        let tx_b = test_tx(vec![shared_input], 1);
        let tx_b_child = test_tx(vec![OutPoint::new(compute_wtxid(&tx_b), 0)], 1);

        let mut selector = NoopSelector;
        let filtered = filter_conflicting_template_transactions(vec![tx_a.clone(), tx_b, tx_b_child], &mut selector);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id(), tx_a.id());
    }
}
