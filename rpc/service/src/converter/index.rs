use async_trait::async_trait;
use spora_addresses::Prefix;
use spora_consensus_core::{cell_diff::CellCollection, config::Config, tx::extract_address_from_script};
use spora_index_core::indexed_cells::CellSetByAddress;
use spora_index_core::notification::{CellsChangedNotification as IndexCellsChangedNotification, Notification as IndexNotification};
use spora_notify::converter::Converter;
use spora_rpc_core::{
    cell_set_into_rpc, CellsChangedNotification, Notification, PruningPointCellSetOverrideNotification, RpcCellEntry,
    RpcCellsByAddressesEntry,
};
use std::sync::Arc;

/// Conversion of consensus_core to rpc_core structures
#[derive(Debug)]
pub struct IndexConverter {
    prefix: Prefix,
}

impl IndexConverter {
    pub fn new(config: Arc<Config>) -> Self {
        Self { prefix: config.prefix() }
    }

    pub fn get_cells_by_addresses_entries(&self, item: &CellSetByAddress) -> Vec<RpcCellsByAddressesEntry> {
        cell_set_into_rpc(item)
    }

    fn cells_changed_entries(&self, cells: &CellCollection) -> Vec<RpcCellsByAddressesEntry> {
        cells
            .iter()
            .map(|(outpoint, meta)| {
                let cell_entry = RpcCellEntry::new(meta.capacity, meta.block_daa_score, meta.is_cellbase).with_cell_metadata(
                    meta.capacity,
                    meta.data_bytes,
                    meta.lock_hash,
                    meta.type_hash,
                    meta.data_hash,
                );
                let address =
                    meta.lock_script.as_ref().and_then(|lock_script| extract_address_from_script(lock_script, self.prefix).ok());

                RpcCellsByAddressesEntry { address, outpoint: outpoint.clone().into(), cell_entry }
            })
            .collect()
    }

    fn convert_cells_changed(&self, item: &IndexCellsChangedNotification) -> CellsChangedNotification {
        CellsChangedNotification {
            added: Arc::new(self.cells_changed_entries(&item.accumulated_cell_diff.add)),
            removed: Arc::new(self.cells_changed_entries(&item.accumulated_cell_diff.remove)),
        }
    }
}

#[async_trait]
impl Converter for IndexConverter {
    type Incoming = IndexNotification;
    type Outgoing = Notification;

    async fn convert(&self, incoming: IndexNotification) -> Notification {
        match incoming {
            IndexNotification::CellsChanged(msg) => Notification::CellsChanged(self.convert_cells_changed(&msg)),
            IndexNotification::PruningPointCellSetOverride(_) => {
                Notification::PruningPointCellSetOverride(PruningPointCellSetOverrideNotification {})
            }
        }
    }
}
