//!
//! Cell record representation used by wallet transactions.
//!

use crate::imports::*;
use serde::{Deserialize, Serialize};
use spora_addresses::Address;

pub use spora_consensus_core::tx::TransactionId;

/// [`CellRecord`] represents an incoming transaction cell entry
/// stored within [`TransactionRecord`].
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct CellRecord {
    pub address: Option<Address>,
    pub index: TransactionIndexType,
    pub amount: u64,
    pub lock_hash: TransactionId,
    #[serde(rename = "isCoinbase")]
    pub is_coinbase: bool,
}

impl From<&CellEntryReference> for CellRecord {
    fn from(cell: &CellEntryReference) -> Self {
        let CellEntryReference { cell } = cell;
        CellRecord {
            index: cell.outpoint.get_index(),
            address: cell.address.clone(),
            amount: cell.amount,
            lock_hash: cell
                .embedded_cell_metadata()
                .expect("transaction cell records require canonical Cell metadata")
                .lock_hash
                .into(),
            is_coinbase: cell.is_coinbase,
        }
    }
}
