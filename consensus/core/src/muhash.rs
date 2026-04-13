use crate::{
    cell_metadata::CellMetadata,
    cell_metadata::EmbeddedCellMetadata,
    hashing::HasherExtensions,
    tx::{outpoint_from_id, CellEntry, TransactionOutpoint, VerifiableTransaction},
};
use spora_core::{info, trace};
use spora_hashes::HasherBase;

// Re-export MuHash for public use
pub use spora_muhash::MuHash;

pub trait MuHashExtensions {
    fn add_transaction(&mut self, tx: &impl VerifiableTransaction, block_daa_score: u64);
    fn add_cell_entry(&mut self, outpoint: &TransactionOutpoint, entry: &CellEntry);
    fn from_transaction(tx: &impl VerifiableTransaction, block_daa_score: u64) -> Self;
    fn from_cell_entry(outpoint: &TransactionOutpoint, entry: &CellEntry) -> Self;
}

impl MuHashExtensions for MuHash {
    fn add_transaction(&mut self, tx: &impl VerifiableTransaction, block_daa_score: u64) {
        let tx_id = tx.id();
        info!("Adding transaction {} to multiset (is_coinbase: {}, block_daa_score: {})", tx_id, tx.is_coinbase(), block_daa_score);

        for (index, input) in tx.inputs().iter().enumerate() {
            let mut writer = self.remove_element_builder();
            if let Some(metadata) = tx.cell_metadata(index) {
                write_cell_metadata(&mut writer, &metadata);
            } else {
                let entry = tx
                    .cell_entry(index)
                    .expect("MuHash input removal requires either canonical cell metadata or a populated cell entry");
                write_cell_entry(&mut writer, entry, &input.out_point);
            }
            writer.finalize();
            let (capacity, is_coinbase, block_daa_score) = if let Some(metadata) = tx.cell_metadata(index) {
                (metadata.capacity, metadata.is_cellbase, metadata.block_daa_score)
            } else {
                let entry = tx.cell_entry(index).expect("cell entry must exist when metadata is absent");
                (entry.capacity(), entry.is_cellbase, entry.block_daa_score)
            };
            info!(
                "Removed cell from multiset: tx={:?}, index={}, value={}, is_coinbase={}, block_daa_score={}",
                input.out_point.tx_hash, input.out_point.index, capacity, is_coinbase, block_daa_score
            );
        }

        let cell_tx = tx.tx();
        for (i, output) in cell_tx.outputs.iter().enumerate() {
            let outpoint = outpoint_from_id(tx_id, i as u32);
            let data = cell_tx.outputs_data.get(i).map(Vec::as_slice).unwrap_or_default();
            let data_hash = *blake3::hash(data).as_bytes();
            let entry = CellEntry {
                out_point: outpoint,
                capacity: output.capacity,
                data_bytes: data.len() as u64,
                lock_hash: output.lock.hash(),
                type_hash: output.type_.as_ref().map(|s| s.hash()),
                data_hash,
                block_daa_score,
                is_cellbase: tx.is_coinbase(),
            };
            self.add_cell_entry(&outpoint, &entry);
            info!(
                "Added cell to multiset: tx={}, index={}, value={}, is_coinbase={}, block_daa_score={}",
                tx_id,
                i,
                output.capacity,
                tx.is_coinbase(),
                block_daa_score,
            );
        }
    }

    fn add_cell_entry(&mut self, outpoint: &TransactionOutpoint, entry: &CellEntry) {
        let mut writer = self.add_element_builder();
        write_cell_entry(&mut writer, entry, outpoint);
        writer.finalize();
        trace!(
            "Cell entry details: outpoint={:?}:{}, amount={}, is_coinbase={}, block_daa_score={}",
            outpoint.tx_hash,
            outpoint.index,
            entry.capacity(),
            entry.is_cellbase,
            entry.block_daa_score
        );
    }

    fn from_transaction(tx: &impl VerifiableTransaction, block_daa_score: u64) -> Self {
        let mut mh = Self::new();
        mh.add_transaction(tx, block_daa_score);
        mh
    }

    fn from_cell_entry(outpoint: &TransactionOutpoint, entry: &CellEntry) -> Self {
        let mut mh = Self::new();
        mh.add_cell_entry(outpoint, entry);
        mh
    }
}

fn write_embedded_cell_metadata(writer: &mut impl HasherBase, metadata: &EmbeddedCellMetadata) {
    writer.update(metadata.lock_hash);
    writer.write_bool(metadata.type_hash.is_some());
    if let Some(type_hash) = metadata.type_hash {
        writer.update(type_hash);
    }
    writer.update(metadata.data_hash).update(metadata.data_bytes.to_le_bytes());
}

fn write_cell_metadata(writer: &mut impl HasherBase, metadata: &CellMetadata) {
    writer
        .update(metadata.out_point.tx_hash)
        .update(metadata.out_point.index.to_le_bytes())
        .update(metadata.block_daa_score.to_le_bytes())
        .update(metadata.capacity.to_le_bytes())
        .write_bool(metadata.is_cellbase);

    write_embedded_cell_metadata(
        writer,
        &EmbeddedCellMetadata {
            lock_hash: metadata.lock_hash,
            type_hash: metadata.type_hash,
            data_hash: metadata.data_hash,
            data_bytes: metadata.data_bytes,
        },
    );
}

fn write_cell_entry(writer: &mut impl HasherBase, entry: &CellEntry, outpoint: &TransactionOutpoint) {
    writer
        // Outpoint
        .update(outpoint.tx_hash)
        .update(outpoint.index.to_le_bytes())
        // Cell entry
        .update(entry.block_daa_score.to_le_bytes())
        .update(entry.capacity().to_le_bytes())
        .write_bool(entry.is_cellbase);

    // CellMeta always carries metadata
    if let Some(metadata) = entry.embedded_cell_metadata() {
        write_embedded_cell_metadata(writer, &metadata);
    }
}
