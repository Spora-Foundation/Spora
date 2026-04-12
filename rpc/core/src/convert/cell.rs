//! Conversion functions for Cell-indexed RPC types.

use crate::RpcCellEntry;
use crate::RpcCellsByAddressesEntry;
use spora_addresses::Prefix;
use spora_consensus_core::tx::{extract_script_pub_key_address, ScriptPublicKey};
use spora_index_core::indexed_cells::CellSetByScriptPublicKey;
use spora_index_core::indexed_cells::CompactCellCollection;

// ----------------------------------------------------------------------------
// index to rpc_core
// ----------------------------------------------------------------------------

pub fn cell_set_into_rpc(item: &CellSetByScriptPublicKey, prefix: Option<Prefix>) -> Vec<RpcCellsByAddressesEntry> {
    item.iter()
        .flat_map(|(script_public_key, cell_collection)| {
            let address = prefix.and_then(|x| extract_script_pub_key_address(script_public_key, x).ok());
            cell_collection
                .iter()
                .map(|(outpoint, entry)| RpcCellsByAddressesEntry {
                    address: address.clone(),
                    outpoint: (*outpoint).into(),
                    cell_entry: RpcCellEntry::new(entry.amount, script_public_key.clone(), entry.block_daa_score, entry.is_coinbase)
                        .with_cell_metadata(entry.capacity, entry.data_bytes, entry.lock_hash, entry.type_hash, entry.data_hash),
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>()
}

pub fn cell_collection_into_rpc(spk: &ScriptPublicKey, cell_collection: &CompactCellCollection) -> Vec<RpcCellsByAddressesEntry> {
    cell_collection
        .iter()
        .map(|(outpoint, entry)| {
            let outpoint = (*outpoint).into();
            let cell_entry = RpcCellEntry::new(entry.amount, spk.clone(), entry.block_daa_score, entry.is_coinbase)
                .with_cell_metadata(entry.capacity, entry.data_bytes, entry.lock_hash, entry.type_hash, entry.data_hash);
            RpcCellsByAddressesEntry { address: None, outpoint, cell_entry }
        })
        .collect()
}
