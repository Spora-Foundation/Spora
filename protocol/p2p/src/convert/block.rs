use super::{error::ConversionError, option::TryIntoOptionEx};
use crate::pb as protowire;
use spora_consensus_core::{block::Block, tx::{Transaction, CellTx}};

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<&Block> for protowire::BlockMessage {
    fn from(block: &Block) -> Self {
        // TODO(cell-model): Implement proper CellTx to protowire conversion
        // For now, return empty transactions
        Self { header: Some(block.header.as_ref().into()), transactions: vec![] }
    }
}

// ----------------------------------------------------------------------------
// protowire to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<protowire::BlockMessage> for Block {
    type Error = ConversionError;

    fn try_from(block: protowire::BlockMessage) -> Result<Self, Self::Error> {
        // TODO(cell-model): Implement proper protowire to CellTx conversion
        // For now, accept empty transactions
        Ok(Self::new(
            block.header.try_into_ex()?,
            vec![],  // Empty CellTx vector for now
        ))
    }
}
