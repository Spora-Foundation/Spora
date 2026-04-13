//! Conversion of Transaction related types

use crate::{RpcError, RpcResult, RpcTransaction, RpcTransactionInput, RpcTransactionOutput};
use spora_consensus_core::{
    mass::project_cell_tx_mass,
    tx::{CellInput, CellTx},
};

// ----------------------------------------------------------------------------
// consensus_core to rpc_core
// ----------------------------------------------------------------------------

impl From<&CellTx> for RpcTransaction {
    fn from(item: &CellTx) -> Self {
        let projected_mass = project_cell_tx_mass(item, None);
        Self {
            version: item.version,
            inputs: RpcTransactionInput::from_cell_refs(&item.inputs, &item.witnesses),
            outputs: RpcTransactionOutput::from_cell_outputs(&item.outputs, &item.outputs_data),
            payload: item.payload().map(ToOwned::to_owned).unwrap_or_default(),
            mass: projected_mass.selection_mass,
            verbose_data: None,
        }
    }
}

// ----------------------------------------------------------------------------
// rpc_core to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<RpcTransaction> for CellTx {
    type Error = RpcError;

    fn try_from(item: RpcTransaction) -> RpcResult<Self> {
        let is_coinbase = item.inputs.is_empty();

        if !is_coinbase && !item.payload.is_empty() {
            return Err(RpcError::General(
                "RpcTransaction.payload is only supported for coinbase transactions; attach data to outputs for normal CellTx"
                    .to_string(),
            ));
        }

        let inputs: Vec<CellInput> = item.inputs.iter().map(|input| CellInput::new(input.previous_outpoint.into(), input.since)).collect();

        let mut witnesses: Vec<Vec<u8>> = item.inputs.into_iter().map(|input| input.witness).collect();

        let outputs: Vec<_> = item
            .outputs
            .iter()
            .map(|output| spora_consensus_core::tx::CellOutput {
                lock: output.lock_script.clone().into(),
                type_: output.type_script.clone().map(Into::into),
                capacity: output.capacity.unwrap_or(output.value),
            })
            .collect();

        let mut outputs_data = item.outputs.iter().map(|output| output.output_data.clone().unwrap_or_default()).collect::<Vec<_>>();
        if is_coinbase && !item.payload.is_empty() {
            if let Some(first_output_data) = outputs_data.first_mut() {
                *first_output_data = item.payload.clone();
            } else {
                witnesses = vec![item.payload.clone()];
            }
        }

        Ok(CellTx { version: item.version, inputs, cell_deps: vec![], header_deps: vec![], outputs, outputs_data, witnesses })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spora_consensus_core::{
        mass::project_cell_tx_mass,
        tx::{CellOutput, OutPoint, Script},
    };

    #[test]
    fn cell_tx_roundtrip_preserves_canonical_fields() {
        let tx = CellTx::new(
            vec![CellInput::new(OutPoint::new([0x11; 32], 2), 42)],
            vec![],
            vec![CellOutput {
                lock: Script::new([0x22; 32], 1, vec![0xaa, 0xbb]),
                type_: Some(Script::new([0x33; 32], 2, vec![0xcc])),
                capacity: 1_337,
            }],
            vec![vec![1, 2, 3, 4]],
            vec![vec![0xde, 0xad]],
        )
        .unwrap();

        let rpc_tx = RpcTransaction::from(&tx);
        assert_eq!(rpc_tx.mass, project_cell_tx_mass(&tx, None).selection_mass);
        assert_eq!(rpc_tx.outputs[0].data_bytes, Some(4));
        assert_eq!(rpc_tx.outputs[0].data_hash, Some(*blake3::hash(&tx.outputs_data[0]).as_bytes()));

        let restored = CellTx::try_from(rpc_tx).expect("rpc tx converts back into CellTx");

        assert_eq!(restored.version, tx.version);
        assert_eq!(restored.inputs, tx.inputs);
        assert_eq!(restored.witnesses, tx.witnesses);
        assert_eq!(restored.outputs.len(), 1);
        assert_eq!(restored.outputs[0].capacity, tx.outputs[0].capacity);
        assert_eq!(restored.outputs[0].lock.code_hash, tx.outputs[0].lock.code_hash);
        assert_eq!(
            restored.outputs[0].type_.as_ref().map(|script| script.code_hash),
            tx.outputs[0].type_.as_ref().map(|script| script.code_hash)
        );
        assert_eq!(restored.outputs_data, tx.outputs_data);
    }

    #[test]
    fn coinbase_payload_roundtrip_restores_first_output_data() {
        let payload = vec![9, 8, 7, 6];
        let tx = CellTx::new(
            vec![],
            vec![],
            vec![CellOutput { lock: Script::new([0x44; 32], 0, vec![]), type_: None, capacity: 5_000 }],
            vec![payload.clone()],
            vec![],
        )
        .unwrap();

        let rpc_tx = RpcTransaction::from(&tx);
        assert_eq!(rpc_tx.mass, 0);
        assert_eq!(rpc_tx.payload, payload);

        let restored = CellTx::try_from(rpc_tx).expect("coinbase rpc tx converts back into CellTx");
        assert!(restored.is_coinbase());
        assert_eq!(restored.outputs_data, vec![payload]);
    }

    #[test]
    fn rpc_transaction_rejects_non_coinbase_payload() {
        let rpc_tx = RpcTransaction {
            version: 0,
            inputs: vec![RpcTransactionInput::from_cell_ref(&CellInput::new(OutPoint::new([0x11; 32], 2), 42), vec![0xaa])],
            outputs: vec![RpcTransactionOutput::from_cell_output(
                &CellOutput { lock: Script::new([0x22; 32], 0, vec![]), type_: None, capacity: 1_000 },
                &[1, 2, 3],
            )],
            payload: vec![9, 8, 7],
            mass: 0,
            verbose_data: None,
        };

        let error = CellTx::try_from(rpc_tx).expect_err("reserved payload must be rejected on normal transactions");
        assert!(error.to_string().contains("payload"));
    }
}
