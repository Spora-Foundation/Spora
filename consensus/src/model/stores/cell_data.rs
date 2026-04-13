use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};
use spora_consensus_core::BlockHasher;
use spora_database::{
    prelude::{BatchDbWriter, CachePolicy, CachedDbAccess, StoreResult, DB},
    registry::DatabaseStorePrefixes,
};
use spora_hashes::Hash;
use spora_state::SegmentInfo;
use std::sync::Arc;

#[derive(Clone, Serialize, Deserialize)]
struct StoredSegmentInfo(SegmentInfo);

impl spora_utils::mem_size::MemSizeEstimator for StoredSegmentInfo {}

pub trait CellDataStoreReader {
    fn get(&self, outpoint_hash: Hash) -> StoreResult<SegmentInfo>;
}

pub struct DbCellDataStore {
    db: Arc<DB>,
    access: CachedDbAccess<Hash, StoredSegmentInfo, BlockHasher>,
}

impl DbCellDataStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self {
            db: Arc::clone(&db),
            access: CachedDbAccess::new(Arc::clone(&db), cache_policy, DatabaseStorePrefixes::CellDataSegments.into()),
        }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, outpoint_hash: Hash, segment_info: SegmentInfo) -> StoreResult<()> {
        self.access.write(BatchDbWriter::new(batch), outpoint_hash, StoredSegmentInfo(segment_info))
    }
}

impl CellDataStoreReader for DbCellDataStore {
    fn get(&self, outpoint_hash: Hash) -> StoreResult<SegmentInfo> {
        self.access.read(outpoint_hash).map(|stored| stored.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spora_database::{create_temp_db, prelude::ConnBuilder};

    #[test]
    fn cell_data_store_roundtrip_persists_segment_info() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbCellDataStore::new(db, CachePolicy::Count(10));
        let outpoint_hash = Hash::from_u64_word(42);
        let expected = SegmentInfo { segment_id: 7, offset: 128, length: 512 };

        let mut batch = WriteBatch::default();
        store.insert_batch(&mut batch, outpoint_hash, expected.clone()).unwrap();
        store.db.write(batch).unwrap();

        assert_eq!(store.get(outpoint_hash).unwrap(), expected);
    }

    #[test]
    fn cell_data_store_clone_with_new_cache_reads_existing_mapping() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbCellDataStore::new(db, CachePolicy::Count(1));
        let outpoint_hash = Hash::from_u64_word(99);
        let expected = SegmentInfo { segment_id: 3, offset: 64, length: 256 };

        let mut batch = WriteBatch::default();
        store.insert_batch(&mut batch, outpoint_hash, expected.clone()).unwrap();
        store.db.write(batch).unwrap();

        let reloaded = store.clone_with_new_cache(CachePolicy::Count(8));
        assert_eq!(reloaded.get(outpoint_hash).unwrap(), expected);
    }
}
