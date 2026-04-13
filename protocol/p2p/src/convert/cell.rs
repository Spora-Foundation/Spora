use crate::pb as protowire;
use spora_consensus_core::tx::{CellEntry, TransactionOutpoint};

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<&CellEntry> for protowire::CellEntry {
    fn from(entry: &CellEntry) -> Self {
        let metadata = entry.embedded_cell_metadata().expect("p2p CellEntry requires canonical Cell metadata");
        Self {
            amount: entry.amount(),
            block_daa_score: entry.block_daa_score,
            is_coinbase: entry.is_cellbase,
            capacity: entry.capacity(),
            data_bytes: metadata.data_bytes,
            lock_hash: metadata.lock_hash.to_vec(),
            type_hash: metadata.type_hash.map(|hash| hash.to_vec()).unwrap_or_default(),
            data_hash: metadata.data_hash.to_vec(),
        }
    }
}

impl From<(&TransactionOutpoint, &CellEntry)> for protowire::OutpointAndCellEntryPair {
    fn from((outpoint, entry): (&TransactionOutpoint, &CellEntry)) -> Self {
        Self { outpoint: Some(outpoint.into()), cell_entry: Some(entry.into()) }
    }
}
