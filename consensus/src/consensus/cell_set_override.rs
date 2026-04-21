// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Spora developers
//
// Cell set override for devnet pre-allocation
// Migrated from the previous transaction-output model to the Cell model

#[cfg(feature = "devnet-prealloc")]
mod cell_set_override_inner {
    use std::sync::Arc;

    use itertools::Itertools;
    use spora_consensus_core::{
        api::ConsensusApi, cell_diff::CellCollection, config::Config, header::Header, tx::TransactionOutpoint,
    };
    use spora_exec::OutPoint;
    use spora_hashes::Hash;
    use spora_state::{CellEntry, CellStateTree};

    use crate::consensus::Consensus;

    /// Helper: Convert TransactionOutpoint to Hash for tree indexing
    fn outpoint_to_hash(outpoint: &TransactionOutpoint) -> Hash {
        use blake3::Hasher;

        let mut hasher = Hasher::new();
        hasher.update(b"spora-cell/outpoint"); // Domain separation
        hasher.update(&outpoint.tx_hash);
        hasher.update(&outpoint.index.to_le_bytes());

        Hash::from_bytes(*hasher.finalize().as_bytes())
    }

    fn exec_outpoint(outpoint: &TransactionOutpoint) -> OutPoint {
        OutPoint::new(outpoint.tx_hash, outpoint.index)
    }

    /// Compute cell_commitment v0 from cell_root
    ///
    /// V0 format: H("spora/cell_commitment/v0" || cell_root)
    fn compute_cell_commitment_v0(cell_root: Hash) -> Hash {
        use blake3::Hasher;

        let mut hasher = Hasher::new();
        hasher.update(b"spora/cell_commitment/v0");
        hasher.update(cell_root.as_bytes().as_ref());

        Hash::from_bytes(*hasher.finalize().as_bytes())
    }

    /// Set genesis cell_root and cell_commitment from initial cell set
    ///
    /// Replaces the previous MuHash-based implementation with Cell State Tree
    pub fn set_genesis_cell_commitment_from_config(config: &mut Config) {
        let mut genesis_tree = CellStateTree::new();

        // Build cell state tree from initial cell set
        for (outpoint, cell_meta) in config.initial_cell_set.iter() {
            let outpoint_hash = outpoint_to_hash(outpoint);

            // Convert CellMeta to CellEntry for tree
            let entry = CellEntry::new(
                cell_meta.capacity,
                cell_meta.data_bytes,
                Hash::from_bytes(cell_meta.lock_hash),
                cell_meta.type_hash.map(Hash::from_bytes),
                Hash::from_bytes(cell_meta.data_hash),
                cell_meta.block_daa_score,
                cell_meta.is_cellbase,
            )
            .with_resolved_metadata(cell_meta.lock_script.clone(), cell_meta.type_script.clone(), cell_meta.data.clone());

            genesis_tree.insert_with_outpoint(outpoint_hash, exec_outpoint(outpoint), entry);
        }

        // Calculate cell_root (MuHash root of all cells)
        let cell_root = genesis_tree.root();

        // Calculate cell_commitment (v0: H(domain || cell_root))
        config.params.genesis.cell_root = cell_root;
        config.params.genesis.cell_commitment = compute_cell_commitment_v0(cell_root);

        // Recalculate genesis hash with new cell_root and cell_commitment
        let genesis_header: Header = (&config.params.genesis).into();
        config.params.genesis.hash = genesis_header.hash;
    }

    /// Set initial cell set for pruning point import
    ///
    /// Replaces the previous cell-set import shim with Cell set import
    pub fn set_initial_cell_set(initial_cell_set: &CellCollection, consensus: Arc<Consensus>, genesis_hash: Hash) {
        let cell_slice = &initial_cell_set.iter().map(|(op, meta)| (*op, meta.clone())).collect_vec()[..];

        // Build cell state tree for pruning point
        let mut cell_tree = CellStateTree::new();
        for (outpoint, meta) in cell_slice {
            let outpoint_hash = outpoint_to_hash(outpoint);
            let entry = CellEntry::new(
                meta.capacity,
                meta.data_bytes,
                Hash::from_bytes(meta.lock_hash),
                meta.type_hash.map(Hash::from_bytes),
                Hash::from_bytes(meta.data_hash),
                meta.block_daa_score,
                meta.is_cellbase,
            )
            .with_resolved_metadata(meta.lock_script.clone(), meta.type_script.clone(), meta.data.clone());
            cell_tree.insert_with_outpoint(outpoint_hash, exec_outpoint(outpoint), entry);
        }

        // Import pruning point cell set
        consensus.append_imported_pruning_point_cells(cell_slice, &mut cell_tree);
        consensus.import_pruning_point_cell_set(genesis_hash, cell_tree).unwrap();
    }
}

#[cfg(feature = "devnet-prealloc")]
pub use cell_set_override_inner::*;
