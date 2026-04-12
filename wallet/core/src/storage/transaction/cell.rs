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
    #[serde(rename = "scriptPubKey")]
    pub script_public_key: ScriptPublicKey,
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
            script_public_key: cell.script_public_key.clone(),
            is_coinbase: cell.is_coinbase,
        }
    }
}
