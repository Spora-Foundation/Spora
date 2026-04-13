use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};
use spora_consensus_core::tx::{CellOutput, CellInput, CellTx};
use spora_consensus_core::BlockHasher;
use spora_database::prelude::CachePolicy;
use spora_database::prelude::StoreError;
use spora_database::prelude::DB;
use spora_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use spora_database::registry::DatabaseStorePrefixes;
use spora_hashes::Hash;
use spora_utils::mem_size::MemSizeEstimator;
use std::sync::Arc;

pub trait BlockTransactionsStoreReader {
    fn get(&self, hash: Hash) -> Result<Arc<Vec<CellTx>>, StoreError>;

    fn get_transaction(&self, hash: Hash) -> Result<CellTx, StoreError>;
}

pub trait BlockTransactionsStore: BlockTransactionsStoreReader {
    // This is append only
    fn insert(&self, hash: Hash, transactions: Arc<Vec<CellTx>>) -> Result<(), StoreError>;
    fn delete(&self, hash: Hash) -> Result<(), StoreError>;
}

#[derive(Clone, Serialize, Deserialize)]
struct BlockBody(Arc<Vec<CellTx>>);

impl MemSizeEstimator for BlockBody {
    fn estimate_mem_bytes(&self) -> usize {
        const NORMAL_SIG_SIZE: usize = 66;
        let (inputs, outputs) = self.0.iter().fold((0, 0), |(ins, outs), tx| (ins + tx.inputs.len(), outs + tx.outputs.len()));
        // TODO: consider tracking transactions by bytes accurately (preferably by saving the size in a field)
        // We avoid zooming in another level and counting exact bytes for sigs and scripts for performance reasons.
        // Outliers with longer signatures are rare enough and their size is eventually bounded by mempool standards
        // or in the worst case by max block mass.
        // A similar argument holds for spk within outputs, but in this case the constant is already counted through the SmallVec used within.
        inputs * (size_of::<CellInput>() + NORMAL_SIG_SIZE)
            + outputs * size_of::<CellOutput>()
            + self.0.len() * size_of::<CellTx>()
            + size_of::<Vec<CellTx>>()
            + size_of::<Self>()
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct TxIdx {
    hash: Hash,
    tidx: usize,
}

impl TxIdx {
    pub const fn new(hash: Hash, tidx: usize) -> Self {
        Self { hash, tidx }
    }
}

impl MemSizeEstimator for TxIdx {
    fn estimate_mem_bytes(&self) -> usize {
        size_of::<Self>()
    }
}

/// A DB + cache implementation of `BlockTransactionsStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbBlockTransactionsStore {
    db: Arc<DB>,
    access: CachedDbAccess<Hash, BlockBody, BlockHasher>,
    txidxs: CachedDbAccess<Hash, TxIdx, BlockHasher>,
}

impl DbBlockTransactionsStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self {
            db: Arc::clone(&db),
            access: CachedDbAccess::new(db.clone(), cache_policy, DatabaseStorePrefixes::BlockTransactions.into()),
            txidxs: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::TransactionIndex.into()),
        }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    pub fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        self.access.has(hash)
    }

    pub fn get_transaction_location(&self, hash: Hash) -> Result<(Hash, usize), StoreError> {
        let TxIdx { hash: block_hash, tidx } = self.txidxs.read(hash)?;
        Ok((block_hash, tidx))
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: Hash, transactions: Arc<Vec<CellTx>>) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::HashAlreadyExists(hash));
        }
        for (tidx, tx) in transactions.iter().enumerate() {
            self.txidxs.write(BatchDbWriter::new(batch), tx.id().into(), TxIdx::new(hash, tidx))?;
        }
        self.access.write(BatchDbWriter::new(batch), hash, BlockBody(transactions))?;
        Ok(())
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: Hash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

impl BlockTransactionsStoreReader for DbBlockTransactionsStore {
    fn get(&self, hash: Hash) -> Result<Arc<Vec<CellTx>>, StoreError> {
        Ok(self.access.read(hash)?.0)
    }

    fn get_transaction(&self, hash: Hash) -> Result<CellTx, StoreError> {
        let TxIdx { hash, tidx } = self.txidxs.read(hash)?;
        let txs = self.access.read(hash)?.0;
        Ok(txs[tidx].clone())
    }
}

impl BlockTransactionsStore for DbBlockTransactionsStore {
    fn insert(&self, hash: Hash, transactions: Arc<Vec<CellTx>>) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::HashAlreadyExists(hash));
        }
        for (tidx, tx) in transactions.iter().enumerate() {
            self.txidxs.write(DirectDbWriter::new(&self.db), tx.id().into(), TxIdx::new(hash, tidx))?;
        }
        self.access.write(DirectDbWriter::new(&self.db), hash, BlockBody(transactions))?;
        Ok(())
    }

    fn delete(&self, hash: Hash) -> Result<(), StoreError> {
        self.access.delete(DirectDbWriter::new(&self.db), hash)
    }
}
