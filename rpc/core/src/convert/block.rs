//! Conversion of Block related types

use std::sync::Arc;

use crate::{RpcBlock, RpcError, RpcRawBlock, RpcResult};
use spora_consensus_core::block::{Block, MutableBlock};
use spora_consensus_core::tx::CellTx;

// ----------------------------------------------------------------------------
// consensus_core to rpc_core
// ----------------------------------------------------------------------------

impl From<&Block> for RpcBlock {
    fn from(item: &Block) -> Self {
        Self {
            header: item.header.as_ref().into(),
            transactions: item.transactions.iter().map(crate::RpcTransaction::from).collect(),
            verbose_data: None,
        }
    }
}

impl From<&Block> for RpcRawBlock {
    fn from(item: &Block) -> Self {
        Self { header: item.header.as_ref().into(), transactions: item.transactions.iter().map(crate::RpcTransaction::from).collect() }
    }
}

impl From<&MutableBlock> for RpcBlock {
    fn from(item: &MutableBlock) -> Self {
        Self {
            header: item.header.as_ref().into(),
            transactions: item.transactions.iter().map(crate::RpcTransaction::from).collect(),
            verbose_data: None,
        }
    }
}

impl From<&MutableBlock> for RpcRawBlock {
    fn from(item: &MutableBlock) -> Self {
        Self { header: item.header.as_ref().into(), transactions: item.transactions.iter().map(crate::RpcTransaction::from).collect() }
    }
}

impl From<MutableBlock> for RpcRawBlock {
    fn from(item: MutableBlock) -> Self {
        Self { header: item.header.into(), transactions: item.transactions.iter().map(crate::RpcTransaction::from).collect() }
    }
}

// ----------------------------------------------------------------------------
// rpc_core to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<RpcBlock> for Block {
    type Error = RpcError;
    fn try_from(item: RpcBlock) -> RpcResult<Self> {
        let transactions = item.transactions.into_iter().map(crate::RpcTransaction::try_into).collect::<RpcResult<Vec<CellTx>>>()?;
        Ok(Self { header: Arc::new(item.header.into()), transactions: Arc::new(transactions) })
    }
}

impl TryFrom<RpcRawBlock> for Block {
    type Error = RpcError;
    fn try_from(item: RpcRawBlock) -> RpcResult<Self> {
        let transactions = item.transactions.into_iter().map(crate::RpcTransaction::try_into).collect::<RpcResult<Vec<CellTx>>>()?;
        Ok(Self { header: Arc::new(item.header.into()), transactions: Arc::new(transactions) })
    }
}

#[cfg(test)]
mod tests {
    use spora_consensus_core::mass::project_cell_tx_mass;
    use spora_consensus_core::tx::{CellOut, CellRef, CellTx, OutPoint, ScriptRef};

    #[test]
    fn rpc_transaction_from_cell_tx_preserves_canonical_output_metadata() {
        let lock = ScriptRef::new([0x11; 32], 0, vec![0xaa, 0xbb]);
        let type_script = ScriptRef::new([0x22; 32], 1, vec![0xcc]);
        let output = CellOut { lock: lock.clone(), type_: Some(type_script.clone()), capacity: 4242 };
        let output_data = vec![1, 2, 3, 4];
        let tx = CellTx::new(
            vec![CellRef::new(OutPoint::new([0x33; 32], 7), 123)],
            vec![],
            vec![output.clone()],
            vec![output_data.clone()],
            vec![vec![0xde, 0xad, 0xbe, 0xef]],
        )
        .unwrap();

        let rpc_tx = crate::RpcTransaction::from(&tx);

        assert_eq!(rpc_tx.version, tx.ver);
        assert_eq!(rpc_tx.inputs.len(), 1);
        assert_eq!(rpc_tx.inputs[0].since, 123);
        assert_eq!(rpc_tx.inputs[0].witness, vec![0xde, 0xad, 0xbe, 0xef]);

        assert_eq!(rpc_tx.outputs.len(), 1);
        let rpc_output = &rpc_tx.outputs[0];
        assert_eq!(rpc_output.value, output.capacity);
        assert_eq!(rpc_output.capacity, Some(output.capacity));
        assert_eq!(rpc_output.data_bytes, Some(output_data.len() as u64));
        assert_eq!(rpc_output.lock_hash, Some(lock.hash()));
        assert_eq!(rpc_output.type_hash, Some(type_script.hash()));
        assert_eq!(rpc_output.data_hash, Some(*blake3::hash(&output_data).as_bytes()));
        assert_eq!(rpc_tx.mass, project_cell_tx_mass(&tx, None).selection_mass);
        assert_eq!(rpc_tx.payload, Vec::<u8>::new());
    }
}
