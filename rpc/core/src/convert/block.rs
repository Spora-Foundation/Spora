//! Conversion of Block related types

use std::sync::Arc;

use crate::{RpcBlock, RpcError, RpcRawBlock, RpcResult, RpcTransaction};
use spora_consensus_core::block::{Block, MutableBlock};

// ----------------------------------------------------------------------------
// consensus_core to rpc_core
// ----------------------------------------------------------------------------

impl From<&Block> for RpcBlock {
    fn from(item: &Block) -> Self {
        // TODO(cell-model): Implement CellTx to RpcTransaction conversion
        Self {
            header: item.header.as_ref().into(),
            transactions: vec![],  // Empty for now - Cell model migration
            verbose_data: None,
        }
    }
}

impl From<&Block> for RpcRawBlock {
    fn from(item: &Block) -> Self {
        // TODO(cell-model): Implement CellTx to RpcTransaction conversion
        Self { header: item.header.as_ref().into(), transactions: vec![] }
    }
}

impl From<&MutableBlock> for RpcBlock {
    fn from(item: &MutableBlock) -> Self {
        // TODO(cell-model): Implement CellTx to RpcTransaction conversion
        Self {
            header: item.header.as_ref().into(),
            transactions: vec![],  // Empty for now - Cell model migration
            verbose_data: None,
        }
    }
}

impl From<&MutableBlock> for RpcRawBlock {
    fn from(item: &MutableBlock) -> Self {
        // TODO(cell-model): Implement CellTx to RpcTransaction conversion
        Self { header: item.header.as_ref().into(), transactions: vec![] }
    }
}

impl From<MutableBlock> for RpcRawBlock {
    fn from(item: MutableBlock) -> Self {
        // TODO(cell-model): Implement CellTx to RpcTransaction conversion
        Self { header: item.header.into(), transactions: vec![] }
    }
}

// ----------------------------------------------------------------------------
// rpc_core to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<RpcBlock> for Block {
    type Error = RpcError;
    fn try_from(item: RpcBlock) -> RpcResult<Self> {
        // TODO(cell-model): Implement RpcTransaction to CellTx conversion
        Ok(Self {
            header: Arc::new(item.header.into()),
            transactions: Arc::new(vec![]),  // Empty for now - Cell model migration
        })
    }
}

impl TryFrom<RpcRawBlock> for Block {
    type Error = RpcError;
    fn try_from(item: RpcRawBlock) -> RpcResult<Self> {
        // TODO(cell-model): Implement RpcTransaction to CellTx conversion
        Ok(Self {
            header: Arc::new(item.header.into()),
            transactions: Arc::new(vec![]),  // Empty for now - Cell model migration
        })
    }
}
