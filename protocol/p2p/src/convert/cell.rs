use crate::pb as protowire;
use spora_consensus_core::tx::{cell_entry_legacy_script_public_key, CellEntry, TransactionOutpoint};

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<&CellEntry> for protowire::CellEntry {
    fn from(entry: &CellEntry) -> Self {
        Self {
            amount: entry.amount(),
            script_public_key: Some((&cell_entry_legacy_script_public_key(entry)).into()),
            block_daa_score: entry.block_daa_score,
            is_coinbase: entry.is_cellbase,
        }
    }
}

impl From<(&TransactionOutpoint, &CellEntry)> for protowire::OutpointAndCellEntryPair {
    fn from((outpoint, entry): (&TransactionOutpoint, &CellEntry)) -> Self {
        Self { outpoint: Some(outpoint.into()), cell_entry: Some(entry.into()) }
    }
}
