use super::tx::MempoolTransaction;
use spora_consensus_core::tx::{TransactionId, TransactionOutpoint};
use std::collections::HashMap;

/// MempoolTransactionCollection maps a transaction id to a mempool transaction
pub(crate) type MempoolTransactionCollection = HashMap<TransactionId, MempoolTransaction>;

/// CellTransactionIndex maps a Cell transaction id to the matching mempool transaction id
pub(crate) type CellTransactionIndex = HashMap<TransactionId, TransactionId>;

/// OutpointIndex maps an outpoint to a transaction id
pub(crate) type OutpointIndex = HashMap<TransactionOutpoint, TransactionId>;
