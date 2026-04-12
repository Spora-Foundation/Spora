//!
//! WASM specific conversion functions
//!

use crate::model::*;
use spora_consensus_client::*;
use std::sync::Arc;

impl From<RpcCellsByAddressesEntry> for CellEntry {
    fn from(entry: RpcCellsByAddressesEntry) -> CellEntry {
        let RpcCellsByAddressesEntry { address, outpoint, cell_entry } = entry;
        let RpcCellEntry {
            amount,
            capacity,
            data_bytes,
            lock_hash,
            type_hash,
            data_hash,
            script_public_key,
            block_daa_score,
            is_coinbase,
        } = cell_entry;

        let mut cell_entry = CellEntry {
            address,
            outpoint: outpoint.into(),
            amount,
            capacity: None,
            data_bytes: None,
            lock_hash: None,
            type_hash: None,
            data_hash: None,
            script_public_key,
            block_daa_score,
            is_coinbase,
        };

        let has_canonical_metadata =
            capacity != amount || data_bytes != 0 || lock_hash != [0; 32] || type_hash.is_some() || data_hash != [0; 32];

        if has_canonical_metadata {
            cell_entry =
                cell_entry.with_cell_metadata(capacity, data_bytes, lock_hash.into(), type_hash.map(Into::into), data_hash.into());
        }

        cell_entry
    }
}

impl From<RpcCellsByAddressesEntry> for CellEntryReference {
    fn from(entry: RpcCellsByAddressesEntry) -> Self {
        Self { cell: Arc::new(entry.into()) }
    }
}

impl From<&RpcCellsByAddressesEntry> for CellEntryReference {
    fn from(entry: &RpcCellsByAddressesEntry) -> Self {
        Self { cell: Arc::new(entry.clone().into()) }
    }
}

cfg_if::cfg_if! {
    if #[cfg(feature = "wasm32-sdk")] {

        impl From<TransactionInput> for RpcTransactionInput {
            fn from(tx_input: TransactionInput) -> Self {
                let inner = tx_input.inner();
                RpcTransactionInput::from_cell_ref(
                    &spora_consensus_core::tx::CellRef::new(inner.previous_outpoint.clone().into(), inner.sequence),
                    inner.signature_script.clone().unwrap_or_default(),
                )
            }
        }

        impl From<TransactionOutput> for RpcTransactionOutput {
            fn from(output: TransactionOutput) -> Self {
                let inner = output.inner();
                let cell_out = spora_consensus_core::tx::cell_out_from_legacy_script_public_key(inner.value, &inner.script_public_key);
                RpcTransactionOutput::from_cell_output(&cell_out, &[])
            }
        }

        impl From<Transaction> for RpcTransaction {
            fn from(tx: Transaction) -> Self {
                RpcTransaction::from(&tx)
            }
        }

        impl From<&Transaction> for RpcTransaction {
            fn from(tx: &Transaction) -> Self {
                let inner = tx.inner();
                let inputs: Vec<RpcTransactionInput> =
                    inner.inputs.clone().into_iter().map(|input| input.into()).collect::<Vec<RpcTransactionInput>>();
                let outputs: Vec<RpcTransactionOutput> =
                    inner.outputs.clone().into_iter().map(|output| output.into()).collect::<Vec<RpcTransactionOutput>>();

                RpcTransaction {
                    version: inner.version,
                    inputs,
                    outputs,
                    lock_time: inner.lock_time,
                    subnetwork_id: inner.subnetwork_id.clone().into(),
                    gas: inner.gas,
                    payload: inner.payload.clone(),
                    mass: inner.mass,
                    verbose_data: None,
                }
            }
        }
    }
}
