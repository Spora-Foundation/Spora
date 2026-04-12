//! Conversion of Notification related types

use crate::{
    BlockAddedNotification, CellsChangedNotification, FinalityConflictNotification, FinalityConflictResolvedNotification,
    NewBlockTemplateNotification, Notification, PruningPointCellSetOverrideNotification, RpcAcceptedTransactionIds, RpcCellEntry,
    RpcCellsByAddressesEntry, SinkBlueScoreChangedNotification, VirtualChainChangedNotification, VirtualDaaScoreChangedNotification,
};
use spora_consensus_core::{cell_diff::CellCollection, cell_metadata::cell_metadata_placeholder_script_public_key_with_metadata};
use spora_consensus_notify::notification as consensus_notify;
use spora_index_core::notification as index_notify;
use std::sync::Arc;

// ----------------------------------------------------------------------------
// consensus_core to rpc_core
// ----------------------------------------------------------------------------

impl From<consensus_notify::Notification> for Notification {
    fn from(item: consensus_notify::Notification) -> Self {
        (&item).into()
    }
}

impl From<&consensus_notify::Notification> for Notification {
    fn from(item: &consensus_notify::Notification) -> Self {
        match item {
            consensus_notify::Notification::BlockAdded(msg) => Notification::BlockAdded(msg.into()),
            consensus_notify::Notification::VirtualChainChanged(msg) => Notification::VirtualChainChanged(msg.into()),
            consensus_notify::Notification::FinalityConflict(msg) => Notification::FinalityConflict(msg.into()),
            consensus_notify::Notification::FinalityConflictResolved(msg) => Notification::FinalityConflictResolved(msg.into()),
            consensus_notify::Notification::CellsChanged(msg) => Notification::CellsChanged(msg.into()),
            consensus_notify::Notification::SinkBlueScoreChanged(msg) => Notification::SinkBlueScoreChanged(msg.into()),
            consensus_notify::Notification::VirtualDaaScoreChanged(msg) => Notification::VirtualDaaScoreChanged(msg.into()),
            consensus_notify::Notification::PruningPointCellSetOverride(msg) => Notification::PruningPointCellSetOverride(msg.into()),
            consensus_notify::Notification::NewBlockTemplate(msg) => Notification::NewBlockTemplate(msg.into()),
        }
    }
}

impl From<&consensus_notify::BlockAddedNotification> for BlockAddedNotification {
    fn from(item: &consensus_notify::BlockAddedNotification) -> Self {
        Self { block: Arc::new((&item.block).into()) }
    }
}

impl From<&consensus_notify::VirtualChainChangedNotification> for VirtualChainChangedNotification {
    fn from(item: &consensus_notify::VirtualChainChangedNotification) -> Self {
        Self {
            removed_chain_block_hashes: item.removed_chain_block_hashes.clone(),
            added_chain_block_hashes: item.added_chain_block_hashes.clone(),
            // If acceptance data array is empty, it means that the subscription was set to not
            // include accepted_transaction_ids. Otherwise, we expect acceptance data to correlate
            // with the added chain block hashes
            accepted_transaction_ids: Arc::new(if item.added_chain_blocks_acceptance_data.is_empty() {
                vec![]
            } else {
                item.added_chain_block_hashes
                    .iter()
                    .zip(item.added_chain_blocks_acceptance_data.iter())
                    .map(|(hash, acceptance_data)| RpcAcceptedTransactionIds {
                        accepting_block_hash: hash.to_owned(),
                        // We collect accepted tx ids from all mergeset blocks
                        accepted_transaction_ids: acceptance_data
                            .iter()
                            .flat_map(|x| x.accepted_transactions.iter().map(|tx| tx.transaction_id))
                            .collect(),
                    })
                    .collect()
            }),
        }
    }
}

impl From<&consensus_notify::FinalityConflictNotification> for FinalityConflictNotification {
    fn from(item: &consensus_notify::FinalityConflictNotification) -> Self {
        Self { violating_block_hash: item.violating_block_hash }
    }
}

impl From<&consensus_notify::FinalityConflictResolvedNotification> for FinalityConflictResolvedNotification {
    fn from(item: &consensus_notify::FinalityConflictResolvedNotification) -> Self {
        Self { finality_block_hash: item.finality_block_hash }
    }
}

fn rpc_cells_changed_entries(cells: &CellCollection) -> Vec<RpcCellsByAddressesEntry> {
    cells
        .iter()
        .map(|(outpoint, meta)| {
            let placeholder_script_public_key = cell_metadata_placeholder_script_public_key_with_metadata(
                meta.lock_hash,
                meta.type_hash,
                meta.data_hash,
                meta.data_bytes,
            );
            let cell_entry = RpcCellEntry::new(meta.capacity, placeholder_script_public_key, meta.block_daa_score, meta.is_cellbase)
                .with_cell_metadata(meta.capacity, meta.data_bytes, meta.lock_hash, meta.type_hash, meta.data_hash);

            RpcCellsByAddressesEntry { address: None, outpoint: outpoint.clone().into(), cell_entry }
        })
        .collect()
}

impl From<&consensus_notify::CellsChangedNotification> for CellsChangedNotification {
    fn from(item: &consensus_notify::CellsChangedNotification) -> Self {
        // Consensus/index layer diffs currently carry canonical Cell metadata but not a recoverable
        // legacy ScriptPublicKey/address pair. We therefore preserve the canonical Cell payload here
        // and leave `address` empty. Address-scoped filtering in rpc-core still relies on the legacy
        // ScriptPublicKey tracker and needs a separate Cell-aware index to become exact.
        Self {
            added: Arc::new(rpc_cells_changed_entries(&item.accumulated_cell_diff.add)),
            removed: Arc::new(rpc_cells_changed_entries(&item.accumulated_cell_diff.remove)),
        }
    }
}

impl From<&consensus_notify::SinkBlueScoreChangedNotification> for SinkBlueScoreChangedNotification {
    fn from(item: &consensus_notify::SinkBlueScoreChangedNotification) -> Self {
        Self { sink_blue_score: item.sink_blue_score }
    }
}

impl From<&consensus_notify::VirtualDaaScoreChangedNotification> for VirtualDaaScoreChangedNotification {
    fn from(item: &consensus_notify::VirtualDaaScoreChangedNotification) -> Self {
        Self { virtual_daa_score: item.virtual_daa_score }
    }
}

impl From<&consensus_notify::PruningPointCellSetOverrideNotification> for PruningPointCellSetOverrideNotification {
    fn from(_: &consensus_notify::PruningPointCellSetOverrideNotification) -> Self {
        Self {}
    }
}

impl From<&consensus_notify::NewBlockTemplateNotification> for NewBlockTemplateNotification {
    fn from(_: &consensus_notify::NewBlockTemplateNotification) -> Self {
        Self {}
    }
}

// ----------------------------------------------------------------------------
// index to rpc_core
// ----------------------------------------------------------------------------

impl From<index_notify::Notification> for Notification {
    fn from(item: index_notify::Notification) -> Self {
        (&item).into()
    }
}

impl From<&index_notify::Notification> for Notification {
    fn from(item: &index_notify::Notification) -> Self {
        match item {
            index_notify::Notification::CellsChanged(msg) => Notification::CellsChanged(msg.into()),
            index_notify::Notification::PruningPointCellSetOverride(msg) => Notification::PruningPointCellSetOverride(msg.into()),
        }
    }
}

impl From<&index_notify::CellsChangedNotification> for CellsChangedNotification {
    fn from(item: &index_notify::CellsChangedNotification) -> Self {
        Self {
            added: Arc::new(rpc_cells_changed_entries(&item.accumulated_cell_diff.add)),
            removed: Arc::new(rpc_cells_changed_entries(&item.accumulated_cell_diff.remove)),
        }
    }
}

impl From<&index_notify::PruningPointCellSetOverrideNotification> for PruningPointCellSetOverrideNotification {
    fn from(_: &index_notify::PruningPointCellSetOverrideNotification) -> Self {
        Self {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spora_consensus_core::{
        cell_diff::{CellDiff, CellMeta},
        tx::TransactionOutpoint,
    };
    use spora_hashes::Hash;

    fn sample_outpoint(byte: u8, index: u32) -> TransactionOutpoint {
        TransactionOutpoint { tx_hash: Hash::from_bytes([byte; 32]).as_bytes(), index }
    }

    fn sample_cell_meta(byte: u8, index: u32) -> CellMeta {
        CellMeta {
            out_point: sample_outpoint(byte, index),
            capacity: 1_000 + index as u64,
            data_bytes: 32 + index as u64,
            lock_hash: [byte; 32],
            type_hash: Some([byte.wrapping_add(1); 32]),
            data_hash: [byte.wrapping_add(2); 32],
            block_daa_score: 10_000 + index as u64,
            is_cellbase: index == 0,
        }
    }

    #[test]
    fn consensus_cells_changed_conversion_preserves_diff_payload() {
        let mut diff = CellDiff::new();
        let added_outpoint = sample_outpoint(0x11, 0);
        let removed_outpoint = sample_outpoint(0x22, 1);
        diff.add.insert(added_outpoint.clone(), sample_cell_meta(0x11, 0));
        diff.remove.insert(removed_outpoint.clone(), sample_cell_meta(0x22, 1));

        let notification = consensus_notify::CellsChangedNotification::new(
            Arc::new(diff),
            Arc::new(vec![Hash::from_bytes([0x33; 32])]),
            Arc::new(vec![]),
        );
        let rpc_notification: CellsChangedNotification = (&notification).into();

        assert_eq!(rpc_notification.added.len(), 1);
        assert_eq!(rpc_notification.removed.len(), 1);

        let added = &rpc_notification.added[0];
        assert_eq!(added.outpoint, added_outpoint.into());
        assert_eq!(added.cell_entry.capacity, 1_000);
        assert_eq!(added.cell_entry.lock_hash, [0x11; 32]);
        assert_eq!(added.cell_entry.type_hash, Some([0x12; 32]));
        assert_eq!(added.cell_entry.data_hash, [0x13; 32]);

        let removed = &rpc_notification.removed[0];
        assert_eq!(removed.outpoint, removed_outpoint.into());
        assert_eq!(removed.cell_entry.capacity, 1_001);
        assert_eq!(removed.cell_entry.lock_hash, [0x22; 32]);
        assert_eq!(removed.cell_entry.type_hash, Some([0x23; 32]));
        assert_eq!(removed.cell_entry.data_hash, [0x24; 32]);
    }
}
