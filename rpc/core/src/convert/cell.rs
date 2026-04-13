//! Conversion functions for Cell-indexed RPC types.

use crate::RpcCellEntry;
use crate::RpcCellsByAddressesEntry;
use spora_index_core::indexed_cells::CellSetByAddress;
use spora_index_core::indexed_cells::CompactCellCollection;

// ----------------------------------------------------------------------------
// index to rpc_core
// ----------------------------------------------------------------------------

pub fn cell_set_into_rpc(item: &CellSetByAddress) -> Vec<RpcCellsByAddressesEntry> {
    item.iter()
        .flat_map(|(address, cell_collection)| {
            cell_collection
                .iter()
                .map(|(outpoint, entry)| RpcCellsByAddressesEntry {
                    address: Some(address.clone()),
                    outpoint: (*outpoint).into(),
                    cell_entry: RpcCellEntry::new(entry.amount, entry.block_daa_score, entry.is_coinbase).with_cell_metadata(
                        entry.capacity,
                        entry.data_bytes,
                        entry.lock_hash,
                        entry.type_hash,
                        entry.data_hash,
                    ),
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>()
}

pub fn cell_collection_into_rpc(cell_collection: &CompactCellCollection) -> Vec<RpcCellsByAddressesEntry> {
    cell_collection
        .iter()
        .map(|(outpoint, entry)| {
            let outpoint = (*outpoint).into();
            let cell_entry = RpcCellEntry::new(entry.amount, entry.block_daa_score, entry.is_coinbase).with_cell_metadata(
                entry.capacity,
                entry.data_bytes,
                entry.lock_hash,
                entry.type_hash,
                entry.data_hash,
            );
            RpcCellsByAddressesEntry { address: None, outpoint, cell_entry }
        })
        .collect()
}
