//!
//! WASM specific conversion functions
//!

use crate::model::*;
use spora_consensus_client::*;
use std::sync::Arc;

impl From<RpcCellsByAddressesEntry> for CellEntry {
    fn from(entry: RpcCellsByAddressesEntry) -> CellEntry {
        let RpcCellsByAddressesEntry { address, outpoint, cell_entry } = entry;
        let RpcCellEntry { amount, capacity, data_bytes, lock_hash, type_hash, data_hash, block_daa_score, is_coinbase } = cell_entry;

        let mut cell_entry = CellEntry {
            address,
            outpoint: outpoint.into(),
            amount,
            capacity: None,
            data_bytes: None,
            lock_hash: None,
            type_hash: None,
            data_hash: None,
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
                    &spora_consensus_core::tx::CellRef::new(inner.previous_outpoint.clone().into(), inner.since),
                    inner.witness.clone().unwrap_or_default(),
                )
            }
        }

        impl From<TransactionOutput> for RpcTransactionOutput {
            fn from(output: TransactionOutput) -> Self {
                let inner = output.inner();
                let cell_out = spora_consensus_core::tx::CellOut {
                    lock: inner.lock_script.clone(),
                    type_: inner.type_script.clone(),
                    capacity: inner.capacity,
                };
                RpcTransactionOutput::from_cell_output(&cell_out, inner.output_data.as_deref().unwrap_or(&[]))
            }
        }

        impl From<Transaction> for RpcTransaction {
            fn from(tx: Transaction) -> Self {
                RpcTransaction::from(&tx)
            }
        }

        impl From<&Transaction> for RpcTransaction {
            fn from(tx: &Transaction) -> Self {
                let cell_tx =
                    tx.cell_tx().unwrap_or_else(|err| panic!("Transaction must be canonical before RPC conversion: {err}"));
                let projected_mass = tx
                    .signable_transaction()
                    .ok()
                    .map(|signable_tx| {
                        spora_consensus_core::mass::project_verifiable_transaction_mass(&signable_tx.as_verifiable(), None).selection_mass
                    })
                    .unwrap_or_else(|| spora_consensus_core::mass::project_cell_tx_mass(&cell_tx, None).selection_mass);
                let mut rpc_tx = RpcTransaction::from(&cell_tx);
                rpc_tx.mass = projected_mass;
                rpc_tx
            }
        }
    }
}
