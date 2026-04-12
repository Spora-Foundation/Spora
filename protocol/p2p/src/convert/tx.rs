use super::{error::ConversionError, option::TryIntoOptionEx};
use crate::pb as protowire;
use spora_consensus_core::{
    cell_metadata::cell_metadata_placeholder_script_public_key_with_metadata,
    mass::project_cell_tx_mass,
    subnets::SUBNETWORK_ID_SIZE,
    tx::{
        cell_meta_from_legacy_output, cell_out_from_legacy_script_public_key, CellEntry, CellRef, CellTx, ScriptPublicKey,
        TransactionId, TransactionOutpoint,
    },
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

impl From<&ScriptPublicKey> for protowire::ScriptPublicKey {
    fn from(script_public_key: &ScriptPublicKey) -> Self {
        Self { script: script_public_key.script().to_vec(), version: script_public_key.version() as u32 }
    }
}

fn protowire_subnetwork_id(is_coinbase: bool) -> protowire::SubnetworkId {
    let mut bytes = vec![0u8; SUBNETWORK_ID_SIZE];
    if is_coinbase {
        bytes[0] = 1;
    }
    protowire::SubnetworkId { bytes }
}

fn validate_legacy_wire_fields(tx: &protowire::TransactionMessage) -> Result<(), ConversionError> {
    if tx.lock_time != 0 {
        return Err(ConversionError::NonCanonicalLegacyField("lockTime"));
    }

    if tx.gas != 0 {
        return Err(ConversionError::NonCanonicalLegacyField("gas"));
    }

    if !tx.inputs.is_empty() && !tx.payload.is_empty() {
        return Err(ConversionError::NonCoinbasePayload);
    }

    if let Some(subnetwork_id) = tx.subnetwork_id.as_ref() {
        let expected = protowire_subnetwork_id(tx.inputs.is_empty());
        if subnetwork_id.bytes != expected.bytes {
            return Err(ConversionError::NonCanonicalLegacyField("subnetworkId"));
        }
    }

    Ok(())
}

impl From<&CellTx> for protowire::TransactionMessage {
    fn from(tx: &CellTx) -> Self {
        let projected_mass = project_cell_tx_mass(tx, None);
        Self {
            version: tx.ver as u32,
            inputs: tx
                .inputs
                .iter()
                .enumerate()
                .map(|(index, input)| protowire::TransactionInput {
                    previous_outpoint: Some((&input.out_point).into()),
                    signature_script: tx.witnesses.get(index).cloned().unwrap_or_default(),
                    sequence: input.since,
                    sig_op_count: 0,
                })
                .collect(),
            outputs: tx
                .outputs
                .iter()
                .enumerate()
                .map(|(index, output)| {
                    let output_data = tx.outputs_data.get(index).map(Vec::as_slice).unwrap_or(&[]);
                    let data_hash = *blake3::hash(output_data).as_bytes();
                    protowire::TransactionOutput {
                        value: output.capacity,
                        script_public_key: Some(
                            (&cell_metadata_placeholder_script_public_key_with_metadata(
                                output.lock.hash(),
                                output.type_.as_ref().map(|script| script.hash()),
                                data_hash,
                                output_data.len() as u64,
                            ))
                                .into(),
                        ),
                    }
                })
                .collect(),
            lock_time: 0,
            subnetwork_id: Some(protowire_subnetwork_id(tx.is_coinbase())),
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

impl TryFrom<protowire::ScriptPublicKey> for ScriptPublicKey {
    type Error = ConversionError;

    fn try_from(value: protowire::ScriptPublicKey) -> Result<Self, Self::Error> {
        Ok(Self::from_vec(value.version.try_into()?, value.script))
    }
}

impl TryFrom<protowire::CellEntry> for CellEntry {
    type Error = ConversionError;

    fn try_from(value: protowire::CellEntry) -> Result<Self, Self::Error> {
        let script_public_key = value.script_public_key.try_into_ex()?;
        Ok(cell_meta_from_legacy_output(value.amount, &script_public_key, value.block_daa_score, value.is_coinbase))
    }
}

impl TryFrom<protowire::OutpointAndCellEntryPair> for (TransactionOutpoint, CellEntry) {
    type Error = ConversionError;

    fn try_from(value: protowire::OutpointAndCellEntryPair) -> Result<Self, Self::Error> {
        Ok((value.outpoint.try_into_ex()?, value.cell_entry.try_into_ex()?))
    }
}

impl TryFrom<protowire::TransactionMessage> for CellTx {
    type Error = ConversionError;

    fn try_from(tx: protowire::TransactionMessage) -> Result<Self, Self::Error> {
        validate_legacy_wire_fields(&tx)?;

        let inputs: Vec<CellRef> = tx
            .inputs
            .iter()
            .map(|input| Ok(CellRef::new(input.previous_outpoint.clone().try_into_ex()?, input.sequence)))
            .collect::<Result<_, Self::Error>>()?;

        let mut witnesses: Vec<Vec<u8>> = tx.inputs.into_iter().map(|input| input.signature_script).collect();

        let outputs = tx
            .outputs
            .iter()
            .map(|output| {
                let script_public_key = output.script_public_key.clone().try_into_ex()?;
                Ok(cell_out_from_legacy_script_public_key(output.value, &script_public_key))
            })
            .collect::<Result<Vec<_>, Self::Error>>()?;

        let mut outputs_data = vec![vec![]; outputs.len()];
        if inputs.is_empty() && !tx.payload.is_empty() {
            if let Some(first_output_data) = outputs_data.first_mut() {
                *first_output_data = tx.payload.clone();
            } else {
                witnesses = vec![tx.payload.clone()];
            }
        }

        Ok(CellTx { ver: tx.version.try_into()?, inputs, deps: vec![], header_deps: vec![], outputs, outputs_data, witnesses })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spora_consensus_core::{
        mass::project_cell_tx_mass,
        tx::{CellOut, OutPoint, ScriptRef},
    };

    fn sample_lock_script(seed: u8) -> ScriptRef {
        ScriptRef::new([seed; 32], 1, vec![seed, seed.wrapping_add(1)])
    }

    fn sample_output(seed: u8) -> CellOut {
        CellOut { lock: sample_lock_script(seed), type_: None, capacity: 1_000 }
    }

    fn sample_non_coinbase_tx() -> CellTx {
        CellTx::new(vec![CellRef::new(OutPoint::new([7; 32], 0), 42)], vec![], vec![sample_output(9)], vec![vec![]], vec![vec![0xaa]])
            .expect("sample tx")
    }

    fn sample_coinbase_tx() -> CellTx {
        CellTx::new(vec![], vec![], vec![sample_output(5)], vec![vec![]], vec![]).expect("sample coinbase")
    }

    #[test]
    fn rejects_non_zero_legacy_lock_time() {
        let mut proto = protowire::TransactionMessage::from(&sample_non_coinbase_tx());
        proto.lock_time = 1;
        assert!(matches!(CellTx::try_from(proto), Err(ConversionError::NonCanonicalLegacyField("lockTime"))));
    }

    #[test]
    fn rejects_non_zero_legacy_gas() {
        let mut proto = protowire::TransactionMessage::from(&sample_non_coinbase_tx());
        proto.gas = 1;
        assert!(matches!(CellTx::try_from(proto), Err(ConversionError::NonCanonicalLegacyField("gas"))));
    }

    #[test]
    fn rejects_non_canonical_subnetwork_id() {
        let mut proto = protowire::TransactionMessage::from(&sample_non_coinbase_tx());
        proto.subnetwork_id = Some(protowire::SubnetworkId { bytes: vec![1; SUBNETWORK_ID_SIZE] });
        assert!(matches!(CellTx::try_from(proto), Err(ConversionError::NonCanonicalLegacyField("subnetworkId"))));
    }

    #[test]
    fn rejects_payload_on_non_coinbase_transaction() {
        let mut proto = protowire::TransactionMessage::from(&sample_non_coinbase_tx());
        proto.payload = vec![1, 2, 3];
        assert!(matches!(CellTx::try_from(proto), Err(ConversionError::NonCoinbasePayload)));
    }

    #[test]
    fn accepts_canonical_coinbase_payload_and_subnetwork() {
        let mut proto = protowire::TransactionMessage::from(&sample_coinbase_tx());
        proto.payload = vec![1, 2, 3];
        let tx = CellTx::try_from(proto).expect("canonical coinbase transaction");
        assert!(tx.is_coinbase());
        assert_eq!(tx.outputs_data[0], vec![1, 2, 3]);
    }

    #[test]
    fn transaction_message_uses_projected_selection_mass() {
        let tx = sample_non_coinbase_tx();
        let proto = protowire::TransactionMessage::from(&tx);
        assert_eq!(proto.mass, project_cell_tx_mass(&tx, None).selection_mass);
    }
}
