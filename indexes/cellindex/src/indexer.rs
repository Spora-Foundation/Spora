// SPDX-License-Identifier: ISC
// Copyright (C) 2026 Spora developers
//
// Cell indexer service

use crate::{errors::CellIndexError, CellFilter, CellQuery, CellQueryResult, Result};
use parking_lot::RwLock;
use spora_consensus_core::cell_diff::{BlockCellDiff, CellCollection, CellMeta as DiffCellMeta};
use spora_exec::{CellOutput, CellTx, OutPoint, Script};
use spora_state::index::CellMeta;
use spora_state::{CellDB, ScriptIndex, SegmentInfo, SegmentProof, SegmentReader, SegmentWriter};
use std::path::Path;
use std::sync::Arc;

/// DA proof response for a segment-backed Cell payload.
#[derive(Clone, Debug)]
pub struct CellDataProof {
    pub out_point: OutPoint,
    pub segment_info: SegmentInfo,
    pub payload: Vec<u8>,
    pub proof: SegmentProof,
}

/// Cell indexer service
///
/// Provides high-level indexing and query operations
pub struct CellIndexer {
    /// Cell database
    cell_db: Arc<CellDB>,

    /// Script index
    script_index: Arc<ScriptIndex>,

    /// DA segment writer for large cell data payloads
    segment_writer: Arc<SegmentWriter>,

    /// DA segment reader used to hydrate payloads on demand
    segment_reader: Arc<SegmentReader>,

    /// Processing statistics
    stats: Arc<RwLock<IndexerStats>>,
}

/// Indexer statistics
#[derive(Debug, Clone, Default)]
pub struct IndexerStats {
    /// Total blocks processed
    pub blocks_processed: u64,

    /// Total transactions indexed
    pub txs_indexed: u64,

    /// Total Cells indexed
    pub cells_indexed: u64,

    /// Total queries served
    pub queries_served: u64,
}

impl CellIndexer {
    /// Create a new Cell indexer
    pub fn new<P: AsRef<Path>>(cell_db_path: P, script_index_path: P) -> Result<Self> {
        let cell_db_path = cell_db_path.as_ref();
        let cell_db = CellDB::open(cell_db_path)?;
        let script_index = ScriptIndex::open(script_index_path)?;
        let segments_dir = cell_db_path.join("segments");
        let segment_writer = SegmentWriter::new(&segments_dir)?;
        let segment_reader = SegmentReader::new(&segments_dir)?;

        Ok(Self {
            cell_db: Arc::new(cell_db),
            script_index: Arc::new(script_index),
            segment_writer: Arc::new(segment_writer),
            segment_reader: Arc::new(segment_reader),
            stats: Arc::new(RwLock::new(IndexerStats::default())),
        })
    }

    /// Index a transaction
    ///
    /// Adds all outputs to the index and marks inputs as spent
    pub fn index_transaction(&self, tx: &CellTx, daa_score: u64, block_hash: [u8; 32], is_cellbase: bool) -> Result<()> {
        let tx_hash = spora_exec::celltx::sighash::compute_wtxid(tx);

        // Index outputs (new Cells)
        for (idx, output) in tx.outputs.iter().enumerate() {
            let out_point = OutPoint::new(tx_hash, idx as u32);
            let cell_data = tx.outputs_data.get(idx).cloned().unwrap_or_default();
            let (stored_cell_data, segment_info) = self.persist_cell_data(&cell_data)?;

            let meta = CellMeta {
                cell_output: output.clone(),
                cell_data: stored_cell_data,
                daa_score,
                block_hash,
                is_cellbase,
                segment_info,
            };

            self.cell_db.put(&out_point, &meta)?;

            // Add to script index
            let lock_hash = output.lock.hash();
            self.script_index.add_lock(&lock_hash, &out_point)?;

            if let Some(ref type_script) = output.type_ {
                let type_hash = type_script.hash();
                self.script_index.add_type(&type_hash, &out_point)?;
            }
        }

        // Mark inputs as spent
        for input in &tx.inputs {
            // Remove from script index using the live metadata before spending.
            if let Some(meta) = self.cell_db.get(&input.previous_output)? {
                let lock_hash = meta.cell_output.lock.hash();
                self.script_index.remove_lock(&lock_hash, &input.previous_output)?;

                if let Some(ref type_script) = meta.cell_output.type_ {
                    let type_hash = type_script.hash();
                    self.script_index.remove_type(&type_hash, &input.previous_output)?;
                }
            }

            self.cell_db.spend_in_block(&input.previous_output, daa_score, block_hash)?;
        }

        // Update stats
        let mut stats = self.stats.write();
        stats.txs_indexed += 1;
        stats.cells_indexed += tx.outputs.len() as u64;

        Ok(())
    }

    /// Query Cells by filter
    pub fn query(&self, query: &CellQuery) -> Result<CellQueryResult> {
        let out_points = match &query.filter {
            CellFilter::ByLock(lock_hash) => self.script_index.get_by_lock(lock_hash)?,
            CellFilter::ByType(type_hash) => self.script_index.get_by_type(type_hash)?,
            CellFilter::ByOutPoint(out_point) => {
                vec![out_point.clone()]
            }
        };

        let mut cells = Vec::new();
        let mut total_count = 0usize;

        for out_point in &out_points {
            if let Some(meta) = self.cell_db.get(out_point)? {
                let meta = self.hydrate_cell_data(meta)?;
                // Apply capacity filter
                if let Some(min_cap) = query.min_capacity {
                    if meta.cell_output.capacity < min_cap {
                        continue;
                    }
                }
                if let Some(max_cap) = query.max_capacity {
                    if meta.cell_output.capacity > max_cap {
                        continue;
                    }
                }

                total_count += 1;
                if cells.len() < query.limit {
                    cells.push((out_point.clone(), meta));
                }
            }
        }

        // Update stats
        self.stats.write().queries_served += 1;

        Ok(CellQueryResult { cells, total_count })
    }

    /// Get a single Cell by OutPoint
    pub fn get_cell(&self, out_point: &OutPoint) -> Result<Option<CellMeta>> {
        self.cell_db.get(out_point)?.map(|meta| self.hydrate_cell_data(meta)).transpose()
    }

    /// Build a DA proof for a segment-backed Cell payload.
    pub fn get_cell_data_proof(&self, out_point: &OutPoint) -> Result<Option<CellDataProof>> {
        let Some(meta) = self.cell_db.get(out_point)? else {
            return Ok(None);
        };

        let Some(segment_info) = meta.segment_info.clone() else {
            return Err(CellIndexError::QueryFailed(format!("cell {:?} is not segment-backed", out_point)));
        };

        let proof = self.segment_reader.build_proof_for_segment_info(&segment_info)?;
        Ok(Some(CellDataProof { out_point: *out_point, payload: proof.chunk_data.clone(), segment_info, proof }))
    }

    /// Check if a Cell is spent
    pub fn is_spent(&self, out_point: &OutPoint) -> Result<Option<u64>> {
        Ok(self.cell_db.is_spent(out_point)?)
    }

    /// Update Cell index with a diff (CKB-style incremental update)
    ///
    /// GHOSTDAG-aware: processes Cell diff from virtual state changes.
    ///
    /// Note that consensus notifications currently carry the net virtual diff
    /// only; they do not preserve the exact block hash that created or spent
    /// each touched cell. Therefore this path maintains the live query index
    /// only and must not fabricate spend-journal history.
    pub fn update_with_diff(&self, diff: &spora_consensus_core::cell_diff::CellDiff) -> Result<()> {
        // STEP 1: Remove consumed Cells
        for (outpoint, meta) in diff.remove.iter() {
            self.script_index.remove_lock(&meta.lock_hash, outpoint)?;
            if let Some(type_hash) = meta.type_hash {
                self.script_index.remove_type(&type_hash, outpoint)?;
            }

            self.cell_db.remove_live_cell(outpoint)?;
        }

        // STEP 2: Add created Cells
        for (outpoint, meta) in diff.add.iter() {
            let indexed_meta = index_cell_meta_from_diff(meta, [0u8; 32], meta.block_daa_score);

            self.cell_db.put(outpoint, &indexed_meta)?;
            self.script_index.add_lock(&meta.lock_hash, outpoint)?;
            if let Some(type_hash) = meta.type_hash {
                self.script_index.add_type(&type_hash, outpoint)?;
            }
        }

        Ok(())
    }

    /// Update Cell index with block-aware provenance.
    ///
    /// This path preserves canonical creation and spending anchors in CellDB so
    /// downstream historical queries can reconstruct journal state without
    /// fabricating block hashes.
    pub fn update_with_block_diffs(&self, block_diffs: &[BlockCellDiff]) -> Result<()> {
        for block_diff in block_diffs {
            let mut spends = Vec::with_capacity(block_diff.cell_diff.remove.len());
            for (outpoint, meta) in &block_diff.cell_diff.remove {
                self.script_index.remove_lock(&meta.lock_hash, outpoint)?;
                if let Some(type_hash) = meta.type_hash {
                    self.script_index.remove_type(&type_hash, outpoint)?;
                }

                spends.push((*outpoint, block_diff.block_daa_score, block_diff.block_hash.as_bytes().clone()));
            }
            if !spends.is_empty() {
                self.cell_db.batch_spend_in_block(&spends)?;
            }

            let mut adds = Vec::with_capacity(block_diff.cell_diff.add.len());
            for (outpoint, meta) in &block_diff.cell_diff.add {
                let indexed_meta =
                    index_cell_meta_from_diff(meta, block_diff.block_hash.as_bytes().clone(), block_diff.block_daa_score);

                adds.push((*outpoint, indexed_meta));
                self.script_index.add_lock(&meta.lock_hash, outpoint)?;
                if let Some(type_hash) = meta.type_hash {
                    self.script_index.add_type(&type_hash, outpoint)?;
                }
            }
            if !adds.is_empty() {
                self.cell_db.batch_put(&adds)?;
            }
        }

        self.stats.write().blocks_processed += block_diffs.len() as u64;

        Ok(())
    }

    /// Seed the index directly from a live cell collection.
    ///
    /// This is used during cold-start backfills where consensus already knows the
    /// current live set but the secondary query index has not been built yet.
    pub fn seed_cell_collection(&self, cells: &CellCollection, block_hash: [u8; 32]) -> Result<()> {
        for (outpoint, meta) in cells {
            let indexed_meta = index_cell_meta_from_diff(meta, block_hash, meta.block_daa_score);

            self.cell_db.put(outpoint, &indexed_meta)?;
            self.script_index.add_lock(&meta.lock_hash, outpoint)?;
            if let Some(type_hash) = meta.type_hash {
                self.script_index.add_type(&type_hash, outpoint)?;
            }
        }

        Ok(())
    }

    /// Sum the capacity of all currently live cells tracked by the index.
    pub fn total_live_capacity(&self) -> Result<u64> {
        self.cell_db.total_live_capacity().map_err(CellIndexError::from)
    }

    /// Returns true when the index contains no live cells yet.
    pub fn is_empty(&self) -> Result<bool> {
        self.cell_db.has_live_cells().map(|has_live_cells| !has_live_cells).map_err(CellIndexError::from)
    }

    /// Get indexer statistics
    pub fn stats(&self) -> IndexerStats {
        self.stats.read().clone()
    }

    fn persist_cell_data(&self, cell_data: &[u8]) -> Result<(Vec<u8>, Option<SegmentInfo>)> {
        if cell_data.is_empty() {
            return Ok((Vec::new(), None));
        }

        let (segment_id, offset, length) = self.segment_writer.append(cell_data)?;
        Ok((Vec::new(), Some(SegmentInfo { segment_id, offset, length })))
    }

    fn hydrate_cell_data(&self, mut meta: CellMeta) -> Result<CellMeta> {
        if meta.cell_data.is_empty() {
            if let Some(segment_info) = &meta.segment_info {
                meta.cell_data = self.segment_reader.read(segment_info.segment_id, segment_info.offset, segment_info.length)?;
            }
        }

        Ok(meta)
    }
}

fn index_cell_meta_from_diff(meta: &DiffCellMeta, block_hash: [u8; 32], block_daa_score: u64) -> CellMeta {
    let lock = placeholder_script_from_hash(meta.lock_hash);
    let type_ = meta.type_hash.map(placeholder_script_from_hash);

    CellMeta {
        cell_output: CellOutput { lock, type_, capacity: meta.capacity },
        cell_data: Vec::new(),
        daa_score: block_daa_score,
        block_hash,
        is_cellbase: meta.is_cellbase,
        segment_info: None,
    }
}

fn placeholder_script_from_hash(script_hash: [u8; 32]) -> Script {
    // The index stores the original script hashes separately in ScriptIndex. The
    // placeholder script only preserves enough structure for RPC bridge queries.
    Script::new(script_hash, 0, Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use spora_consensus_core::{cell_diff::CellDiff, tx::TransactionOutpoint};
    use spora_exec::{CellOutput, Script};
    use tempfile::TempDir;

    fn create_test_tx() -> CellTx {
        let lock = Script::new([0x00; 32], 0, vec![0; 20]);
        CellTx::new(vec![], vec![], vec![CellOutput { lock, type_: None, capacity: 1000 }], vec![vec![0xAA; 100]], vec![]).unwrap()
    }

    #[test]
    fn test_indexer_creation() {
        let tmp_db = TempDir::new().unwrap();
        let tmp_script = TempDir::new().unwrap();

        let _indexer = CellIndexer::new(tmp_db.path(), tmp_script.path()).unwrap();
    }

    #[test]
    fn test_index_transaction() {
        let tmp_db = TempDir::new().unwrap();
        let tmp_script = TempDir::new().unwrap();
        let indexer = CellIndexer::new(tmp_db.path(), tmp_script.path()).unwrap();

        let tx = create_test_tx();
        indexer.index_transaction(&tx, 100, [0x42; 32], false).unwrap();

        let out_point = OutPoint::new(spora_exec::celltx::sighash::compute_wtxid(&tx), 0);
        let meta = indexer.get_cell(&out_point).unwrap().unwrap();
        assert_eq!(meta.cell_data, vec![0xAA; 100]);
        assert!(meta.segment_info.is_some());

        let stats = indexer.stats();
        assert_eq!(stats.txs_indexed, 1);
        assert_eq!(stats.cells_indexed, 1);
    }

    #[test]
    fn test_get_cell_data_proof_for_segment_backed_cell() {
        let tmp_db = TempDir::new().unwrap();
        let tmp_script = TempDir::new().unwrap();
        let indexer = CellIndexer::new(tmp_db.path(), tmp_script.path()).unwrap();

        let tx = create_test_tx();
        indexer.index_transaction(&tx, 100, [0x42; 32], false).unwrap();

        let out_point = OutPoint::new(spora_exec::celltx::sighash::compute_wtxid(&tx), 0);
        let data_proof = indexer.get_cell_data_proof(&out_point).unwrap().unwrap();

        assert_eq!(data_proof.out_point, out_point);
        assert_eq!(data_proof.payload, vec![0xAA; 100]);
        assert_eq!(data_proof.payload, data_proof.proof.chunk_data);
        assert_eq!(data_proof.segment_info.length, 100);
        assert!(data_proof.proof.verify().unwrap());
    }

    #[test]
    fn test_query_by_lock() {
        let tmp_db = TempDir::new().unwrap();
        let tmp_script = TempDir::new().unwrap();
        let indexer = CellIndexer::new(tmp_db.path(), tmp_script.path()).unwrap();

        let lock = Script::new([0x00; 32], 0, vec![0; 20]);
        let tx = create_test_tx();

        indexer.index_transaction(&tx, 100, [0x42; 32], false).unwrap();

        let lock_hash = lock.hash();
        let query = CellQuery { filter: CellFilter::ByLock(lock_hash), limit: 10, min_capacity: None, max_capacity: None };

        let result = indexer.query(&query).unwrap();
        assert_eq!(result.total_count, 1);
    }

    #[test]
    fn test_capacity_filter() {
        let tmp_db = TempDir::new().unwrap();
        let tmp_script = TempDir::new().unwrap();
        let indexer = CellIndexer::new(tmp_db.path(), tmp_script.path()).unwrap();

        let lock = Script::new([0x00; 32], 0, vec![0; 20]);
        let tx = create_test_tx();

        indexer.index_transaction(&tx, 100, [0x42; 32], false).unwrap();

        let lock_hash = lock.hash();

        // Query with min_capacity filter
        let query = CellQuery { filter: CellFilter::ByLock(lock_hash), limit: 10, min_capacity: Some(500), max_capacity: Some(2000) };

        let result = indexer.query(&query).unwrap();
        assert_eq!(result.total_count, 1); // Should match (capacity=1000)

        // Query with too high min_capacity
        let query_high = CellQuery { filter: CellFilter::ByLock(lock_hash), limit: 10, min_capacity: Some(5000), max_capacity: None };

        let result_high = indexer.query(&query_high).unwrap();
        assert_eq!(result_high.total_count, 0); // Should not match
    }

    #[test]
    fn test_update_with_diff_adds_and_removes_cells() {
        let tmp_db = TempDir::new().unwrap();
        let tmp_script = TempDir::new().unwrap();
        let indexer = CellIndexer::new(tmp_db.path(), tmp_script.path()).unwrap();

        let out_point = TransactionOutpoint::new([0x11; 32], 0);
        let meta = DiffCellMeta {
            out_point,
            capacity: 1234,
            data_bytes: 0,
            lock_hash: [0x22; 32],
            type_hash: None,
            data_hash: [0x33; 32],
            block_daa_score: 99,
            is_cellbase: false,
        };

        let mut add_diff = CellDiff::new();
        add_diff.add.insert(meta.out_point.clone(), meta.clone());
        indexer.update_with_diff(&add_diff).unwrap();

        let added = indexer.query(&CellQuery::by_lock(meta.lock_hash, 10)).unwrap();
        assert_eq!(added.total_count, 1);
        assert_eq!(added.cells[0].1.cell_output.capacity, 1234);

        let mut remove_diff = CellDiff::new();
        remove_diff.remove.insert(meta.out_point.clone(), meta.clone());
        indexer.update_with_diff(&remove_diff).unwrap();

        let removed = indexer.query(&CellQuery::by_lock(meta.lock_hash, 10)).unwrap();
        assert_eq!(removed.total_count, 0);
        assert!(removed.cells.is_empty());

        assert_eq!(indexer.cell_db.is_spent(&meta.out_point).unwrap(), None);
    }

    #[test]
    fn test_get_cell_data_proof_errors_for_inline_cells() {
        let tmp_db = TempDir::new().unwrap();
        let tmp_script = TempDir::new().unwrap();
        let indexer = CellIndexer::new(tmp_db.path(), tmp_script.path()).unwrap();

        let out_point = OutPoint::new([0x61; 32], 0);
        let meta = CellMeta {
            cell_output: CellOutput { lock: Script::new([0x00; 32], 0, vec![0; 20]), type_: None, capacity: 777 },
            cell_data: vec![0x44; 16],
            daa_score: 10,
            block_hash: [0x71; 32],
            is_cellbase: false,
            segment_info: None,
        };
        indexer.cell_db.put(&out_point, &meta).unwrap();

        let err = indexer.get_cell_data_proof(&out_point).unwrap_err();
        assert!(matches!(err, CellIndexError::QueryFailed(_)));
    }

    #[test]
    #[allow(deprecated)]
    fn test_update_with_block_diffs_preserves_creation_and_spend_journal() {
        let tmp_db = TempDir::new().unwrap();
        let tmp_script = TempDir::new().unwrap();
        let indexer = CellIndexer::new(tmp_db.path(), tmp_script.path()).unwrap();

        let out_point = TransactionOutpoint::new([0x51; 32], 0);
        let meta = DiffCellMeta {
            out_point: out_point.clone(),
            capacity: 4321,
            data_bytes: 0,
            lock_hash: [0x61; 32],
            type_hash: None,
            data_hash: [0x71; 32],
            block_daa_score: 150,
            is_cellbase: false,
        };

        let mut add_diff = CellDiff::new();
        add_diff.add.insert(out_point.clone(), meta.clone());
        indexer.update_with_block_diffs(&[BlockCellDiff::new([0x81; 32].into(), 150, add_diff)]).unwrap();

        let live_meta = indexer.get_cell(&out_point).unwrap().unwrap();
        assert_eq!(live_meta.block_hash, [0x81; 32]);
        assert_eq!(live_meta.daa_score, 150);

        let mut remove_diff = CellDiff::new();
        remove_diff.remove.insert(out_point.clone(), meta);
        indexer.update_with_block_diffs(&[BlockCellDiff::new([0x91; 32].into(), 220, remove_diff)]).unwrap();

        assert_eq!(indexer.cell_db.is_spent(&out_point).unwrap(), Some(220));
        assert!(indexer.get_cell(&out_point).unwrap().is_none());
        assert!(indexer.cell_db.get_cell_snapshot_at_daa(&out_point, 200).unwrap().is_some());
        assert!(indexer.cell_db.get_cell_snapshot_at_daa(&out_point, 220).unwrap().is_none());
    }

    #[test]
    fn test_total_live_capacity() {
        let tmp_db = TempDir::new().unwrap();
        let tmp_script = TempDir::new().unwrap();
        let indexer = CellIndexer::new(tmp_db.path(), tmp_script.path()).unwrap();

        indexer.index_transaction(&create_test_tx(), 100, [0x42; 32], false).unwrap();
        assert_eq!(indexer.total_live_capacity().unwrap(), 1000);
    }
}
