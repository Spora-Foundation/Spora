use super::tx::MempoolTransaction;
use std::collections::HashMap;
use spora_consensus_core::tx::{TransactionId, TransactionOutpoint};

/// MempoolTransactionCollection maps a transaction id to a mempool transaction
pub(crate) type MempoolTransactionCollection = HashMap<TransactionId, MempoolTransaction>;

/// OutpointIndex maps an outpoint to a transaction id
pub(crate) type OutpointIndex = HashMap<TransactionOutpoint, TransactionId>;
