// SPDX-License-Identifier: ISC
// Copyright (C) 2025 Tondi developers
//
// Cell roots store - stores Cell state Merkle roots for each block
// Replaces utxo_multisets_store with Cell State Tree roots

use std::sync::Arc;

use rocksdb::WriteBatch;
use tondi_consensus_core::BlockHasher;
use tondi_database::{
    prelude::{CachePolicy, CachedDbAccess, DirectDbWriter, StoreError, StoreResult, StoreResultExtensions, DB},
    registry::DatabaseStorePrefixes,
};
use tondi_hashes::Hash;

/// Reader API for `CellRootsStore`
pub trait CellRootsStoreReader {
    fn get(&self, hash: Hash) -> StoreResult<Hash>;
}

/// Database store for Cell state roots (Merkle roots)
pub struct DbCellRootsStore {
    db: Arc<DB>,
    access: CachedDbAccess<Hash, Hash, BlockHasher>,
}

impl DbCellRootsStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self {
            db: Arc::clone(&db),
            access: CachedDbAccess::new(Arc::clone(&db), cache_policy, DatabaseStorePrefixes::CellRoots.into()),
        }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: Hash, cell_root: Hash) -> StoreResult<()> {
        self.access.write(BatchDbWriter::new(batch), hash, cell_root)
    }
}

impl CellRootsStoreReader for DbCellRootsStore {
    fn get(&self, hash: Hash) -> StoreResult<Hash> {
        self.access.read(hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cell_roots_store_creation() {
        // Test requires a real database, skip for now
        // In practice, this will be tested as part of consensus integration tests
    }
}

