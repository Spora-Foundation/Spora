use rocksdb::WriteBatch;
use std::sync::Arc;
use tondi_consensus_core::BlockHasher;
use tondi_database::prelude::CachePolicy;
use tondi_database::prelude::StoreError;
use tondi_database::prelude::DB;
use tondi_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use tondi_database::registry::DatabaseStorePrefixes;
use tondi_hashes::Hash;
use tondi_math::Uint3072;
use tondi_muhash::MuHash;

pub trait UtxoMultisetsStoreReader {
    fn get(&self, hash: Hash) -> Result<MuHash, StoreError>;
}

pub trait UtxoMultisetsStore: UtxoMultisetsStoreReader {
    fn insert(&self, hash: Hash, multiset: MuHash) -> Result<(), StoreError>;
    fn delete(&self, hash: Hash) -> Result<(), StoreError>;
}

/// A DB + cache implementation of `DbUtxoMultisetsStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbUtxoMultisetsStore {
    db: Arc<DB>,
    access: CachedDbAccess<Hash, Uint3072, BlockHasher>,
}

impl DbUtxoMultisetsStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::UtxoMultisets.into()) }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: Hash, multiset: MuHash) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::HashAlreadyExists(hash));
        }
        self.set_batch(batch, hash, multiset)
    }

    pub fn set_batch(&self, batch: &mut WriteBatch, hash: Hash, multiset: MuHash) -> Result<(), StoreError> {
        let normalized = multiset.finalize();
        let mut normalized_muhash = MuHash::new();
        normalized_muhash.add_element(&normalized.as_bytes());
        self.access.write(BatchDbWriter::new(batch), hash, normalized_muhash.try_into().expect("multiset is expected to be finalized"))?;
        Ok(())
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: Hash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

impl UtxoMultisetsStoreReader for DbUtxoMultisetsStore {
    fn get(&self, hash: Hash) -> Result<MuHash, StoreError> {
        Ok(self.access.read(hash)?.into())
    }
}

impl UtxoMultisetsStore for DbUtxoMultisetsStore {
    fn insert(&self, hash: Hash, multiset: MuHash) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::HashAlreadyExists(hash));
        }
        let normalized = multiset.finalize();
        let mut normalized_muhash = MuHash::new();
        normalized_muhash.add_element(&normalized.as_bytes());
        self.access.write(DirectDbWriter::new(&self.db), hash, normalized_muhash.try_into().expect("multiset is expected to be finalized"))?;
        Ok(())
    }

    fn delete(&self, hash: Hash) -> Result<(), StoreError> {
        self.access.delete(DirectDbWriter::new(&self.db), hash)
    }
}
