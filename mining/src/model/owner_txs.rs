use spora_addresses::Address;
use spora_consensus_core::tx::{MutableTransaction, TransactionId};
use std::collections::{HashMap, HashSet};

use super::TransactionIdSet;

pub type AddressSet = HashSet<Address>;

/// Transaction ids involved in either sending to or receiving from an
/// address.
#[derive(Default)]
pub struct OwnerTransactions {
    pub sending_txs: TransactionIdSet,
    pub receiving_txs: TransactionIdSet,
}

impl OwnerTransactions {
    pub fn is_empty(&self) -> bool {
        self.sending_txs.is_empty() && self.receiving_txs.is_empty()
    }
}

/// Transactions grouped by owning addresses
#[derive(Default)]
pub struct GroupedOwnerTransactions {
    pub transactions: HashMap<TransactionId, MutableTransaction>,
    pub owners: HashMap<Address, OwnerTransactions>,
}
