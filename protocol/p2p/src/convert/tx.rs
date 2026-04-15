use super::{error::ConversionError, option::TryIntoOptionEx};
use crate::pb as protowire;
use spora_consensus_core::{
    cell_diff::CellMeta,
    mass::project_cell_tx_mass,
    tx::{CellInput, CellOutput, CellTx, Script, TransactionId, TransactionOutpoint},
};
use spora_hashes::Hash;

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<Hash> for protowire::TransactionId {
    fn from(hash: Hash) -> Self {
        Self { bytes: Vec::from(hash.as_bytes()) }
    }
}

impl From<&Hash> for protowire::TransactionId {
    fn from(hash: &Hash) -> Self {
        Self { bytes: Vec::from(hash.as_bytes()) }
    }
}

impl From<&TransactionOutpoint> for protowire::Outpoint {
    fn from(outpoint: &TransactionOutpoint) -> Self {
        Self { transaction_id: Some(TransactionId::from_bytes(outpoint.tx_hash).into()), index: outpoint.index }
    }
}

impl From<&Script> for protowire::Script {
    fn from(script: &Script) -> Self {
        Self { code_hash: script.code_hash.to_vec(), hash_type: script.hash_type as u32, args: script.args.clone() }
    }
}

fn validate_reserved_wire_fields(tx: &protowire::TransactionMessage) -> Result<(), ConversionError> {
    if tx.lock_time != 0 {
        return Err(ConversionError::NonCanonicalReservedField("lockTime"));
    }

    if tx.gas != 0 {
        return Err(ConversionError::NonCanonicalReservedField("gas"));
    }

    if !tx.inputs.is_empty() && !tx.payload.is_empty() {
        return Err(ConversionError::NonCoinbasePayload);
    }

    Ok(())
}

impl From<&CellTx> for protowire::TransactionMessage {
    fn from(tx: &CellTx) -> Self {
        let projected_mass = project_cell_tx_mass(tx, None);
        Self {
            version: tx.version as u32,
            inputs: tx
                .inputs
                .iter()
                .enumerate()
                .map(|(index, input)| protowire::TransactionInput {
                    previous_outpoint: Some((&input.previous_output).into()),
                    witness: tx.witnesses.get(index).cloned().unwrap_or_default(),
                    since: input.since,
                })
                .collect(),
            outputs: tx
                .outputs
                .iter()
                .enumerate()
                .map(|(index, output)| protowire::TransactionOutput {
                    value: output.capacity,
                    lock_script: Some((&output.lock).into()),
                    type_script: output.type_.as_ref().map(Into::into),
                    output_data: tx.outputs_data.get(index).cloned().unwrap_or_default(),
                })
                .collect(),
            lock_time: 0,
            gas: 0,
            payload: tx.payload().map(ToOwned::to_owned).unwrap_or_default(),
            mass: projected_mass.selection_mass,
        }
    }
}

// ----------------------------------------------------------------------------
// protowire to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<protowire::TransactionId> for TransactionId {
    type Error = ConversionError;

    fn try_from(value: protowire::TransactionId) -> Result<Self, Self::Error> {
        Ok(Self::from_bytes(value.bytes.as_slice().try_into()?))
    }
}

impl TryFrom<protowire::Outpoint> for TransactionOutpoint {
    type Error = ConversionError;

    fn try_from(item: protowire::Outpoint) -> Result<Self, Self::Error> {
        let tx_id: TransactionId = item.transaction_id.try_into_ex()?;
        Ok(Self::new(tx_id.as_bytes(), item.index))
    }
}

impl TryFrom<protowire::Script> for Script {
    type Error = ConversionError;

    fn try_from(value: protowire::Script) -> Result<Self, Self::Error> {
        Ok(Self::new(value.code_hash.as_slice().try_into()?, value.hash_type.try_into()?, value.args))
    }
}

impl TryFrom<protowire::CellEntry> for CellMeta {
    type Error = ConversionError;

    fn try_from(value: protowire::CellEntry) -> Result<Self, Self::Error> {
        let lock_hash = value.lock_hash.as_slice().try_into()?;
        let type_hash = if value.type_hash.is_empty() { None } else { Some(value.type_hash.as_slice().try_into()?) };
        let data_hash = value.data_hash.as_slice().try_into()?;
        Ok(CellMeta::from_cell_metadata(
            value.capacity.max(value.amount),
            value.data_bytes,
            lock_hash,
            type_hash,
            data_hash,
            value.block_daa_score,
            value.is_coinbase,
        ))
    }
}

impl TryFrom<protowire::OutpointAndCellEntryPair> for (TransactionOutpoint, CellMeta) {
    type Error = ConversionError;

    fn try_from(value: protowire::OutpointAndCellEntryPair) -> Result<Self, Self::Error> {
        Ok((value.outpoint.try_into_ex()?, value.cell_entry.try_into_ex()?))
    }
}

impl TryFrom<protowire::TransactionMessage> for CellTx {
    type Error = ConversionError;

    fn try_from(tx: protowire::TransactionMessage) -> Result<Self, Self::Error> {
        validate_reserved_wire_fields(&tx)?;

        let inputs: Vec<CellInput> = tx
            .inputs
            .iter()
            .map(|input| Ok(CellInput::new(input.previous_outpoint.clone().try_into_ex()?, input.since)))
            .collect::<Result<_, Self::Error>>()?;

        let mut witnesses: Vec<Vec<u8>> = tx.inputs.into_iter().map(|input| input.witness).collect();

        let outputs = tx
            .outputs
            .iter()
            .map(|output| {
                Ok(CellOutput {
                    lock: output.lock_script.clone().try_into_ex()?,
                    type_: output.type_script.clone().map(Script::try_from).transpose()?,
                    capacity: output.value,
                })
            })
            .collect::<Result<Vec<_>, Self::Error>>()?;

        let mut outputs_data = tx.outputs.iter().map(|output| output.output_data.clone()).collect::<Vec<_>>();
        if inputs.is_empty() && !tx.payload.is_empty() && outputs_data.iter().all(Vec::is_empty) {
            if let Some(first_output_data) = outputs_data.first_mut() {
                *first_output_data = tx.payload.clone();
            } else {
                witnesses = vec![tx.payload.clone()];
            }
        }

        Ok(CellTx { version: tx.version, inputs, cell_deps: vec![], header_deps: vec![], outputs, outputs_data, witnesses })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spora_consensus_core::{
        mass::project_cell_tx_mass,
        tx::{CellOutput, OutPoint, Script},
    };

    fn sample_lock_script(seed: u8) -> Script {
        Script::new([seed; 32], 1, vec![seed, seed.wrapping_add(1)])
    }

    fn sample_output(seed: u8) -> CellOutput {
        CellOutput { lock: sample_lock_script(seed), type_: None, capacity: 1_000 }
    }

    fn sample_non_coinbase_tx() -> CellTx {
        CellTx::new(
            vec![CellInput::new(OutPoint::new([7; 32], 0), 42)],
            vec![],
            vec![sample_output(9)],
            vec![vec![]],
            vec![vec![0xaa]],
        )
        .expect("sample tx")
    }

    fn sample_coinbase_tx() -> CellTx {
        CellTx::new(vec![], vec![], vec![sample_output(5)], vec![vec![]], vec![]).expect("sample coinbase")
    }

    #[test]
    fn rejects_non_zero_reserved_lock_time() {
        let mut proto = protowire::TransactionMessage::from(&sample_non_coinbase_tx());
        proto.lock_time = 1;
        assert!(matches!(CellTx::try_from(proto), Err(ConversionError::NonCanonicalReservedField("lockTime"))));
    }

    #[test]
    fn rejects_non_zero_reserved_gas() {
        let mut proto = protowire::TransactionMessage::from(&sample_non_coinbase_tx());
        proto.gas = 1;
        assert!(matches!(CellTx::try_from(proto), Err(ConversionError::NonCanonicalReservedField("gas"))));
    }

    #[test]
    fn rejects_payload_on_non_coinbase_transaction() {
        let mut proto = protowire::TransactionMessage::from(&sample_non_coinbase_tx());
        proto.payload = vec![1, 2, 3];
        assert!(matches!(CellTx::try_from(proto), Err(ConversionError::NonCoinbasePayload)));
    }

    #[test]
    fn accepts_canonical_coinbase_payload() {
        let mut proto = protowire::TransactionMessage::from(&sample_coinbase_tx());
        proto.outputs[0].output_data.clear();
        proto.payload = vec![1, 2, 3];
        let tx = CellTx::try_from(proto).expect("canonical coinbase transaction");
        assert!(tx.is_coinbase());
        assert_eq!(tx.outputs_data[0], vec![1, 2, 3]);
    }

    #[test]
    fn roundtrip_preserves_native_output_fields() {
        let tx = CellTx::new(
            vec![CellInput::new(OutPoint::new([7; 32], 0), 42)],
            vec![],
            vec![CellOutput {
                lock: Script::new([9; 32], 1, vec![0xaa, 0xbb]),
                type_: Some(Script::new([4; 32], 2, vec![0xcc])),
                capacity: 1_337,
            }],
            vec![vec![1, 2, 3, 4]],
            vec![vec![0xdd]],
        )
        .expect("sample tx");

        let restored = CellTx::try_from(protowire::TransactionMessage::from(&tx)).expect("p2p tx converts back into CellTx");
        assert_eq!(restored.outputs[0].capacity, tx.outputs[0].capacity);
        assert_eq!(restored.outputs[0].lock, tx.outputs[0].lock);
        assert_eq!(restored.outputs[0].type_, tx.outputs[0].type_);
        assert_eq!(restored.outputs_data, tx.outputs_data);
    }

    #[test]
    fn transaction_message_uses_projected_selection_mass() {
        let tx = sample_non_coinbase_tx();
        let proto = protowire::TransactionMessage::from(&tx);
        assert_eq!(proto.mass, project_cell_tx_mass(&tx, None).selection_mass);
    }
}
