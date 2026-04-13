use async_trait::async_trait;
use spora_consensus_core::config::Config;
use spora_index_core::indexed_cells::CellSetByAddress;
use spora_index_core::notification::Notification as IndexNotification;
use spora_notify::converter::Converter;
use spora_rpc_core::{cell_set_into_rpc, Notification, RpcCellsByAddressesEntry};
use std::sync::Arc;

/// Conversion of consensus_core to rpc_core structures
#[derive(Debug)]
pub struct IndexConverter;

impl IndexConverter {
    pub fn new(_config: Arc<Config>) -> Self {
        Self
    }

    pub fn get_cells_by_addresses_entries(&self, item: &CellSetByAddress) -> Vec<RpcCellsByAddressesEntry> {
        cell_set_into_rpc(item)
    }
}

#[async_trait]
impl Converter for IndexConverter {
    type Incoming = IndexNotification;
    type Outgoing = Notification;

    async fn convert(&self, incoming: IndexNotification) -> Notification {
        (&incoming).into()
    }
}
