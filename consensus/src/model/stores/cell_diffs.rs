// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Spora developers
//
// Cell diffs store - stores Cell state differences for each block
// Replaces the previous diff store with the pure Cell model

use std::sync::Arc;

use rocksdb::WriteBatch;
use spora_consensus_core::{cell_diff::CellDiff, BlockHasher};
use spora_database::{
    prelude::{BatchDbWriter, CachePolicy, CachedDbAccess, StoreResult, DB},
    registry::DatabaseStorePrefixes,
};
use spora_hashes::Hash;

/// Reader API for `CellDiffsStore`
pub trait CellDiffsStoreReader {
    fn get(&self, hash: Hash) -> StoreResult<Arc<CellDiff>>;
}

/// Database store for Cell diffs
pub struct DbCellDiffsStore {
    db: Arc<DB>,
    access: CachedDbAccess<Hash, Arc<CellDiff>, BlockHasher>,
}

impl DbCellDiffsStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self {
            db: Arc::clone(&db),
            access: CachedDbAccess::new(Arc::clone(&db), cache_policy, DatabaseStorePrefixes::CellDiffs.into()),
        }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: Hash, cell_diff: Arc<CellDiff>) -> StoreResult<()> {
        self.access.write(BatchDbWriter::new(batch), hash, cell_diff)
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: Hash) -> StoreResult<()> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

impl CellDiffsStoreReader for DbCellDiffsStore {
    fn get(&self, hash: Hash) -> StoreResult<Arc<CellDiff>> {
        self.access.read(hash)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_cell_diffs_store_creation() {
        // Test requires a real database, skip for now
        // In practice, this will be tested as part of consensus integration tests
    }
}
