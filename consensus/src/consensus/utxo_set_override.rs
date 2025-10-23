// SPDX-License-Identifier: ISC
// Copyright (C) 2025 Spora developers
//
// Cell set override for devnet pre-allocation
// Migrated from UTXO model to Cell model

#[cfg(feature = "devnet-prealloc")]
mod cell_set_override_inner {
    use std::sync::Arc;

    use itertools::Itertools;
    use spora_consensus_core::{
        api::ConsensusApi,
        cell_diff::CellCollection,
        config::Config,
        header::Header,
        tx::TransactionOutpoint,
    };
    use spora_hashes::Hash;
    use spora_state::{CellStateTree, CellEntry};

    use crate::consensus::Consensus;

    /// Helper: Convert TransactionOutpoint to Hash for tree indexing
    fn outpoint_to_hash(outpoint: &TransactionOutpoint) -> Hash {
        use blake3::Hasher;
        
        let mut hasher = Hasher::new();
        hasher.update(b"spora-cell/outpoint"); // Domain separation
        hasher.update(&outpoint.transaction_id.as_bytes());
        hasher.update(&outpoint.index.to_le_bytes());
        
        Hash::from_bytes(*hasher.finalize().as_bytes())
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
    /// Replaces UTXO/MuHash based implementation with Cell State Tree
    pub fn set_genesis_cell_commitment_from_config(config: &mut Config) {
        let mut genesis_tree = CellStateTree::new();
        
        // Build cell state tree from initial cell set
        for (outpoint, cell_meta) in config.initial_cell_set.iter() {
            let outpoint_hash = outpoint_to_hash(outpoint);
            
            // Convert CellMeta to CellEntry for tree
            let entry = CellEntry::new(
                cell_meta.capacity,
                Hash::from_bytes(cell_meta.lock_hash),
                cell_meta.type_hash.map(Hash::from_bytes),
                Hash::from_bytes(cell_meta.data_hash),
            );
            
            genesis_tree.insert(outpoint_hash, entry);
        }
        
        // Calculate cell_root (Merkle root of all cells)
        let cell_root = genesis_tree.root();
        config.params.genesis.cell_root = cell_root;
        
        // Calculate cell_commitment (v0: H(domain || cell_root))
        config.params.genesis.cell_commitment = compute_cell_commitment_v0(cell_root);
        
        // Recalculate genesis hash with new cell_root and cell_commitment
        let genesis_header: Header = (&config.params.genesis).into();
        config.params.genesis.hash = genesis_header.hash;
    }

    /// Set initial cell set for pruning point import
    /// 
    /// Replaces UTXO set import with Cell set import
    pub fn set_initial_cell_set(
        initial_cell_set: &CellCollection,
        consensus: Arc<Consensus>,
        genesis_hash: Hash,
    ) {
        let cell_slice = &initial_cell_set
            .iter()
            .map(|(op, meta)| (*op, meta.clone()))
            .collect_vec()[..];
        
        // Build cell state tree for pruning point
        let mut cell_tree = CellStateTree::new();
        for (outpoint, meta) in cell_slice {
            let outpoint_hash = outpoint_to_hash(outpoint);
            let entry = CellEntry::new(
                meta.capacity,
                Hash::from_bytes(meta.lock_hash),
                meta.type_hash.map(Hash::from_bytes),
                Hash::from_bytes(meta.data_hash),
            );
            cell_tree.insert(outpoint_hash, entry);
        }
        
        // Import pruning point cell set
        consensus.append_imported_pruning_point_cells(cell_slice, &mut cell_tree);
        consensus.import_pruning_point_cell_set(genesis_hash, cell_tree).unwrap();
    }
}

#[cfg(feature = "devnet-prealloc")]
pub use cell_set_override_inner::*;
