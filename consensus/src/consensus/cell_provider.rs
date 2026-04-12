// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Consensus Cell Provider - GHOSTDAG-aware Cell state queries

#[cfg(feature = "vm")]
use crate::processes::cell_validator::CellScriptDataProvider;
use crate::{
    model::{
        services::reachability::MTReachabilityService,
        stores::{
            block_transactions::BlockTransactionsStoreReader, cell_diffs::CellDiffsStoreReader, cell_roots::CellRootsStoreReader,
            ghostdag::GhostdagStoreReader, headers::HeaderStoreReader, reachability::ReachabilityStoreReader,
            statuses::StatusesStoreReader,
        },
    },
    processes::{CellStateProvider, DagCellProvider},
};
use parking_lot::RwLock;
use spora_consensus_core::{blockhash, cell_metadata::CellMetadata, tx::TransactionOutpoint};
#[cfg(feature = "vm")]
use spora_database::prelude::StoreError;
use spora_exec::OutPoint;
use spora_hashes::Hash;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Consensus Cell Provider
///
/// Provides Cell state queries with explicit POV blocks instead of ambiguous DAA-only history.
pub struct ConsensusCellProvider<
    T: GhostdagStoreReader,
    U: ReachabilityStoreReader,
    V: HeaderStoreReader,
    W: CellDiffsStoreReader,
    X: CellRootsStoreReader,
    Y: BlockTransactionsStoreReader,
    Z: StatusesStoreReader,
> {
    ghostdag_store: Arc<T>,
    reachability_service: MTReachabilityService<U>,
    headers_store: Arc<V>,
    cell_diffs_store: Arc<W>,
    cell_roots_store: Arc<X>,
    block_transactions_store: Arc<Y>,
    statuses_store: Arc<RwLock<Z>>,
}

impl<
        T: GhostdagStoreReader,
        U: ReachabilityStoreReader,
        V: HeaderStoreReader,
        W: CellDiffsStoreReader,
        X: CellRootsStoreReader,
        Y: BlockTransactionsStoreReader,
        Z: StatusesStoreReader,
    > Clone for ConsensusCellProvider<T, U, V, W, X, Y, Z>
{
    fn clone(&self) -> Self {
        Self {
            ghostdag_store: self.ghostdag_store.clone(),
            reachability_service: self.reachability_service.clone(),
            headers_store: self.headers_store.clone(),
            cell_diffs_store: self.cell_diffs_store.clone(),
            cell_roots_store: self.cell_roots_store.clone(),
            block_transactions_store: self.block_transactions_store.clone(),
            statuses_store: self.statuses_store.clone(),
        }
    }
}

impl<
        T: GhostdagStoreReader,
        U: ReachabilityStoreReader,
        V: HeaderStoreReader,
        W: CellDiffsStoreReader,
        X: CellRootsStoreReader,
        Y: BlockTransactionsStoreReader,
        Z: StatusesStoreReader,
    > ConsensusCellProvider<T, U, V, W, X, Y, Z>
{
    /// Create a new consensus cell provider
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ghostdag_store: Arc<T>,
        reachability_service: MTReachabilityService<U>,
        headers_store: Arc<V>,
        cell_diffs_store: Arc<W>,
        cell_roots_store: Arc<X>,
        block_transactions_store: Arc<Y>,
        statuses_store: Arc<RwLock<Z>>,
    ) -> Self {
        Self {
            ghostdag_store,
            reachability_service,
            headers_store,
            cell_diffs_store,
            cell_roots_store,
            block_transactions_store,
            statuses_store,
        }
    }

    fn to_transaction_outpoint(out_point: &OutPoint) -> TransactionOutpoint {
        TransactionOutpoint { tx_hash: out_point.tx_hash, index: out_point.index }
    }

    fn ensure_queryable_pov(&self, pov: Hash) -> Result<(), String> {
        if pov == blockhash::NONE {
            return Ok(());
        }

        let status = self.statuses_store.read().get(pov).map_err(|e| format!("Status lookup error: {}", e))?;
        if status.has_block_body() && status.is_cell_valid_or_pending() {
            Ok(())
        } else {
            Err(format!("POV block {} is not queryable (status: {:?})", pov, status))
        }
    }

    fn compute_data_hash(data: &[u8]) -> [u8; 32] {
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

    fn load_metadata_from_block(&self, block: Hash, outpoint: &TransactionOutpoint) -> Result<Option<CellMetadata>, String> {
        let transactions = self.block_transactions_store.get(block).map_err(|e| format!("Transaction lookup error: {}", e))?;
        let header = self.headers_store.get_header(block).map_err(|e| format!("Header lookup error: {}", e))?;

        for (tx_index, tx) in transactions.iter().enumerate() {
            let tx_id: Hash = tx.id().into();
            if tx_id != Hash::from_bytes(outpoint.tx_hash) {
                continue;
            }

            let output_index = outpoint.index as usize;
            if output_index >= tx.outputs.len() {
                return Ok(None);
            }

            let output = &tx.outputs[output_index];
            let output_data = tx.outputs_data.get(output_index).map(|data| data.as_slice()).unwrap_or(&[]);
            return Ok(Some(CellMetadata {
                out_point: *outpoint,
                capacity: output.capacity,
                data_bytes: output_data.len() as u64,
                lock_hash: output.lock.hash(),
                type_hash: output.type_.as_ref().map(|script| script.hash()),
                data_hash: Self::compute_data_hash(output_data),
                block_daa_score: header.daa_score,
                is_cellbase: tx_index == 0 && tx.is_coinbase(),
                block_hash: block,
                lock_code_hash: Some(output.lock.code_hash),
                type_code_hash: output.type_.as_ref().map(|script| script.code_hash),
                lock_script: Some(output.lock.clone()),
                type_script: output.type_.clone(),
                data: Some(output_data.to_vec()),
            }));
        }

        Ok(None)
    }

    fn resolve_added_cell_in_diff(&self, pov: Hash, outpoint: &TransactionOutpoint) -> Result<Option<CellMetadata>, String> {
        if let Some(metadata) = self.load_metadata_from_block(pov, outpoint)? {
            return Ok(Some(metadata));
        }

        let ghostdag_data = self.ghostdag_store.get_data(pov).map_err(|e| format!("GhostDAG lookup error: {}", e))?;

        if let Some(metadata) = self.load_metadata_from_block(ghostdag_data.selected_parent, outpoint)? {
            return Ok(Some(metadata));
        }

        for block in ghostdag_data.mergeset_blues.iter().copied().skip(1) {
            if let Some(metadata) = self.load_metadata_from_block(block, outpoint)? {
                return Ok(Some(metadata));
            }
        }

        Ok(None)
    }

    fn get_cell_at_pov_internal(&self, outpoint: &TransactionOutpoint, pov: Hash) -> Result<Option<CellMetadata>, String> {
        let mut current = pov;
        while current != blockhash::NONE {
            self.ensure_queryable_pov(current)?;

            let diff = self.cell_diffs_store.get(current).map_err(|e| format!("Cell diff lookup error: {}", e))?;
            if diff.remove.contains_key(outpoint) {
                return Ok(None);
            }
            if diff.add.contains_key(outpoint) {
                return self.resolve_added_cell_in_diff(current, outpoint).and_then(|metadata| {
                    metadata
                        .ok_or_else(|| {
                            format!("Cell {} is present in diff for {} but creator transaction is missing", outpoint, current)
                        })
                        .map(Some)
                });
            }

            current = self.ghostdag_store.get_selected_parent(current).map_err(|e| format!("GhostDAG lookup error: {}", e))?;
        }

        Ok(None)
    }
}

#[derive(Clone)]
pub(crate) struct OverlayCellProvider<B> {
    snapshot_pov: Hash,
    base_pov: Hash,
    base: B,
    added: HashMap<OutPoint, CellMetadata>,
    removed: HashSet<OutPoint>,
}

impl<B> OverlayCellProvider<B> {
    pub(crate) fn new(base: B, snapshot_pov: Hash, base_pov: Hash) -> Self {
        Self { snapshot_pov, base_pov, base, added: HashMap::new(), removed: HashSet::new() }
    }

    pub(crate) fn add_cell(&mut self, out_point: OutPoint, metadata: CellMetadata) -> Result<(), String> {
        if self.removed.contains(&out_point) || self.added.contains_key(&out_point) {
            return Err(format!("overlay attempted to create duplicate outpoint {:?}", out_point));
        }

        self.added.insert(out_point, metadata);
        Ok(())
    }

    pub(crate) fn spend_cell(&mut self, out_point: &OutPoint) -> Result<(), String> {
        if self.removed.contains(out_point) {
            return Err(format!("overlay attempted to spend outpoint {:?} more than once", out_point));
        }

        if self.added.remove(out_point).is_none() {
            self.removed.insert(out_point.clone());
        }

        Ok(())
    }

    fn ensure_snapshot_pov(&self, pov: Hash) -> Result<(), String> {
        if pov == self.snapshot_pov {
            Ok(())
        } else {
            Err(format!("unexpected POV {pov}, expected {}", self.snapshot_pov))
        }
    }
}

impl<B: DagCellProvider> OverlayCellProvider<B> {
    pub(crate) fn get_cell_metadata(&self, out_point: &OutPoint) -> Result<Option<CellMetadata>, String> {
        if self.removed.contains(out_point) {
            return Ok(None);
        }

        if let Some(metadata) = self.added.get(out_point) {
            return Ok(Some(metadata.clone()));
        }

        self.base.get_cell_at_pov(out_point, self.base_pov)
    }

    pub(crate) fn ensure_dep_available(&self, out_point: &OutPoint) -> Result<(), String> {
        if self.get_cell_metadata(out_point)?.is_some() {
            Ok(())
        } else {
            Err(format!("cell dependency {:?} is unavailable in mergeset overlay", out_point))
        }
    }
}

impl<B: DagCellProvider> CellStateProvider for OverlayCellProvider<B> {
    fn is_cell_available(&self, out_point: &OutPoint, pov: Hash) -> Result<bool, String> {
        self.ensure_snapshot_pov(pov)?;
        Ok(self.get_cell_metadata(out_point)?.is_some())
    }

    fn get_cell_capacity(&self, out_point: &OutPoint, pov: Hash) -> Result<Option<u64>, String> {
        self.ensure_snapshot_pov(pov)?;
        Ok(self.get_cell_metadata(out_point)?.map(|meta| meta.capacity))
    }
}

impl<B: DagCellProvider> DagCellProvider for OverlayCellProvider<B> {
    fn get_cell_at_pov(&self, out_point: &OutPoint, pov: Hash) -> Result<Option<CellMetadata>, String> {
        self.ensure_snapshot_pov(pov)?;
        self.get_cell_metadata(out_point)
    }

    fn get_block_timestamp(&self, block_hash: Hash) -> Result<u64, String> {
        self.base.get_block_timestamp(block_hash)
    }
}

#[cfg(feature = "vm")]
impl<B: CellScriptDataProvider + DagCellProvider> CellScriptDataProvider for OverlayCellProvider<B> {
    fn get_cell_data(&self, out_point: &OutPoint, pov: Hash) -> Result<Option<Vec<u8>>, String> {
        self.ensure_snapshot_pov(pov)?;

        if self.removed.contains(out_point) {
            return Ok(None);
        }

        if let Some(metadata) = self.added.get(out_point) {
            return Ok(metadata.data.clone());
        }

        self.base.get_cell_data(out_point, self.base_pov)
    }

    fn get_header(&self, block_hash: Hash) -> Result<Option<spora_exec::vm::ResolvedHeader>, String> {
        self.base.get_header(block_hash)
    }
}

impl<
        T: GhostdagStoreReader,
        U: ReachabilityStoreReader,
        V: HeaderStoreReader,
        W: CellDiffsStoreReader,
        X: CellRootsStoreReader,
        Y: BlockTransactionsStoreReader,
        Z: StatusesStoreReader,
    > CellStateProvider for ConsensusCellProvider<T, U, V, W, X, Y, Z>
{
    fn is_cell_available(&self, out_point: &OutPoint, pov: Hash) -> Result<bool, String> {
        let outpoint = Self::to_transaction_outpoint(out_point);
        Ok(self.get_cell_at_pov_internal(&outpoint, pov)?.is_some())
    }

    fn get_cell_capacity(&self, out_point: &OutPoint, pov: Hash) -> Result<Option<u64>, String> {
        let outpoint = Self::to_transaction_outpoint(out_point);
        Ok(self.get_cell_at_pov_internal(&outpoint, pov)?.map(|meta| meta.capacity))
    }
}

impl<
        T: GhostdagStoreReader,
        U: ReachabilityStoreReader,
        V: HeaderStoreReader,
        W: CellDiffsStoreReader,
        X: CellRootsStoreReader,
        Y: BlockTransactionsStoreReader,
        Z: StatusesStoreReader,
    > DagCellProvider for ConsensusCellProvider<T, U, V, W, X, Y, Z>
{
    fn get_cell_at_pov(&self, out_point: &OutPoint, pov: Hash) -> Result<Option<CellMetadata>, String> {
        let outpoint = Self::to_transaction_outpoint(out_point);
        self.get_cell_at_pov_internal(&outpoint, pov)
    }

    fn get_block_timestamp(&self, block_hash: Hash) -> Result<u64, String> {
        self.headers_store.get_timestamp(block_hash).map_err(|e| format!("Header timestamp lookup error: {}", e))
    }
}

#[cfg(feature = "vm")]
impl<
        T: GhostdagStoreReader,
        U: ReachabilityStoreReader,
        V: HeaderStoreReader,
        W: CellDiffsStoreReader,
        X: CellRootsStoreReader,
        Y: BlockTransactionsStoreReader,
        Z: StatusesStoreReader,
    > CellScriptDataProvider for ConsensusCellProvider<T, U, V, W, X, Y, Z>
{
    fn get_cell_data(&self, out_point: &OutPoint, pov: Hash) -> Result<Option<Vec<u8>>, String> {
        let outpoint = Self::to_transaction_outpoint(out_point);
        Ok(self.get_cell_at_pov_internal(&outpoint, pov)?.and_then(|meta| meta.data))
    }

    fn get_header(&self, block_hash: Hash) -> Result<Option<spora_exec::vm::ResolvedHeader>, String> {
        let header = match self.headers_store.get_header(block_hash) {
            Ok(header) => header,
            Err(StoreError::KeyNotFound(_)) => return Ok(None),
            Err(err) => return Err(format!("Header lookup error: {}", err)),
        };
        Ok(Some(spora_exec::vm::ResolvedHeader {
            hash: block_hash.as_bytes(),
            timestamp: header.timestamp,
            daa_score: header.daa_score,
            parents: header.direct_parents().iter().map(|hash| hash.as_bytes()).collect(),
        }))
    }
}
