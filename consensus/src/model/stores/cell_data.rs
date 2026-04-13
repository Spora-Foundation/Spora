use rocksdb::WriteBatch;
use spora_consensus_core::BlockHasher;
use spora_database::{
    prelude::{BatchDbWriter, CachePolicy, CachedDbAccess, StoreResult, DB},
    registry::DatabaseStorePrefixes,
};
use spora_hashes::Hash;
use spora_state::SegmentInfo;
use std::sync::Arc;

pub trait CellDataStoreReader {
    fn get(&self, outpoint_hash: Hash) -> StoreResult<SegmentInfo>;
}

pub struct DbCellDataStore {
    db: Arc<DB>,
    access: CachedDbAccess<Hash, SegmentInfo, BlockHasher>,
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
        self.access.write(BatchDbWriter::new(batch), outpoint_hash, segment_info)
    }
}

impl CellDataStoreReader for DbCellDataStore {
    fn get(&self, outpoint_hash: Hash) -> StoreResult<SegmentInfo> {
        self.access.read(outpoint_hash)
    }
}
