//! Conversion functions for UTXO related types.

use crate::RpcUtxoEntry;
use crate::RpcUtxosByAddressesEntry;
use tondi_addresses::Prefix;
use tondi_consensus_core::tx::ScriptPublicKey;
use tondi_index_core::indexed_utxos::CompactUtxoCollection;
use tondi_index_core::indexed_utxos::UtxoSetByScriptPublicKey;
use tondi_txscript::extract_script_pub_key_address;

// ----------------------------------------------------------------------------
// index to rpc_core
// ----------------------------------------------------------------------------

pub fn utxo_set_into_rpc(item: &UtxoSetByScriptPublicKey, prefix: Option<Prefix>) -> Vec<RpcUtxosByAddressesEntry> {
    item.iter()
        .flat_map(|(script_public_key, utxo_collection)| {
            let address = prefix.and_then(|x| extract_script_pub_key_address(script_public_key, x).ok());
            utxo_collection
                .iter()
                .map(|(outpoint, entry)| RpcUtxosByAddressesEntry {
                    address: address.clone(),
                    outpoint: (*outpoint).into(),
                    utxo_entry: RpcUtxoEntry::new(entry.amount, script_public_key.clone(), entry.block_daa_score, entry.is_coinbase),
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>()
}

pub fn utxo_map_into_rpc(spk: &ScriptPublicKey, utxo_collection: &CompactUtxoCollection) -> Vec<RpcUtxosByAddressesEntry> {
    utxo_collection
        .iter()
        .map(|(outpoint, entry)| {
            let outpoint = (*outpoint).into();
            let utxo_entry = RpcUtxoEntry::new(entry.amount, spk.clone(), entry.block_daa_score, entry.is_coinbase);
            RpcUtxosByAddressesEntry { address: None, outpoint, utxo_entry }
        })
        .collect()
}
