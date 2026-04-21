use crate::protowire;
use crate::{from, try_from};
use spora_rpc_core::{FromRpcHex, RpcError, RpcHash, RpcResult, ToRpcHex};
use std::str::FromStr;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &spora_rpc_core::RpcTransaction, protowire::RpcTransaction, {
    Self {
        version: item.version.into(),
        inputs: item.inputs.iter().map(protowire::RpcTransactionInput::from).collect(),
        outputs: item.outputs.iter().map(protowire::RpcTransactionOutput::from).collect(),
        payload: item.payload.to_rpc_hex(),
        mass: item.mass,
        verbose_data: item.verbose_data.as_ref().map(|x| x.into()),
        cell_deps: item.cell_deps.iter().map(protowire::RpcCellDep::from).collect(),
        header_deps: item.header_deps.iter().map(ToString::to_string).collect(),
    }
});

from!(item: &spora_rpc_core::RpcCellDep, protowire::RpcCellDep, {
    Self { out_point: Some((&item.out_point).into()), dep_type: item.dep_type as u32 }
});

from!(item: &spora_rpc_core::RpcTransactionInput, protowire::RpcTransactionInput, {
    Self {
        previous_outpoint: Some((&item.previous_outpoint).into()),
        since: item.since,
        witness: item.witness.to_rpc_hex(),
        verbose_data: item.verbose_data.as_ref().map(|x| x.into()),
    }
});

from!(item: &spora_rpc_core::RpcTransactionOutput, protowire::RpcTransactionOutput, {
    Self {
        amount: item.value,
        lock_script: Some((&item.lock_script).into()),
        verbose_data: item.verbose_data.as_ref().map(|x| x.into()),
        capacity: item.capacity.unwrap_or_default(),
        data_bytes: item.data_bytes.unwrap_or_default(),
        lock_hash: item.lock_hash.map(RpcHash::from).map(|x| x.to_string()).unwrap_or_default(),
        type_hash: item.type_hash.map(RpcHash::from).map(|x| x.to_string()).unwrap_or_default(),
        data_hash: item.data_hash.map(RpcHash::from).map(|x| x.to_string()).unwrap_or_default(),
        type_script: item.type_script.as_ref().map(|script| script.into()),
        output_data: item.output_data.as_ref().map(|data| data.to_rpc_hex()).unwrap_or_default(),
    }
});

from!(item: &spora_rpc_core::RpcTransactionOutpoint, protowire::RpcOutpoint, {
    Self { transaction_id: item.transaction_id.to_string(), index: item.index }
});

from!(item: &spora_rpc_core::RpcCellEntry, protowire::RpcCellEntry, {
    Self {
        amount: item.amount,
        block_daa_score: item.block_daa_score,
        is_coinbase: item.is_coinbase,
        capacity: item.capacity,
        data_bytes: item.data_bytes,
        lock_hash: RpcHash::from(item.lock_hash).to_string(),
        type_hash: item.type_hash.map(RpcHash::from).map(|x| x.to_string()).unwrap_or_default(),
        data_hash: RpcHash::from(item.data_hash).to_string(),
    }
});

from!(item: &spora_rpc_core::RpcScript, protowire::RpcScript, {
    Self { code_hash: RpcHash::from(item.code_hash).to_string(), hash_type: item.hash_type.into(), args: item.args.to_rpc_hex() }
});

from!(item: &spora_rpc_core::RpcTransactionVerboseData, protowire::RpcTransactionVerboseData, {
    Self {
        transaction_id: item.transaction_id.to_string(),
        hash: item.hash.to_string(),
        compute_mass: item.compute_mass,
        transient_mass: item.transient_mass,
        storage_mass: item.storage_mass,
        verified_cycles: item.verified_cycles,
        block_hash: item.block_hash.to_string(),
        block_time: item.block_time,
    }
});

from!(&spora_rpc_core::RpcTransactionInputVerboseData, protowire::RpcTransactionInputVerboseData);

from!(item: &spora_rpc_core::RpcTransactionOutputVerboseData, protowire::RpcTransactionOutputVerboseData, {
    Self {
        lock_script_type: item.lock_script_type.to_string(),
        lock_script_address: (&item.lock_script_address).into(),
        resolved_lock_kind: item.resolved_lock_kind.map(|kind| kind.to_string()).unwrap_or_default(),
        resolved_address_kind: item.resolved_address_kind.map(|kind| kind.to_string()).unwrap_or_default(),
    }
});

from!(item: &spora_rpc_core::RpcAcceptedTransactionIds, protowire::RpcAcceptedTransactionIds, {
    Self {
        accepting_block_hash: item.accepting_block_hash.to_string(),
        accepted_transaction_ids: item.accepted_transaction_ids.iter().map(|x| x.to_string()).collect(),
    }
});

from!(item: &spora_rpc_core::RpcCellsByAddressesEntry, protowire::RpcCellsByAddressesEntry, {
    Self {
        address: item.address.as_ref().map_or("".to_string(), |x| x.into()),
        outpoint: Some((&item.outpoint).into()),
        cell_entry: Some((&item.cell_entry).into()),
    }
});

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: &protowire::RpcTransaction, spora_rpc_core::RpcTransaction, {
    Self {
        version: item.version,
        inputs: item
            .inputs
            .iter()
            .map(spora_rpc_core::RpcTransactionInput::try_from)
            .collect::<RpcResult<Vec<spora_rpc_core::RpcTransactionInput>>>()?,
        cell_deps: item
            .cell_deps
            .iter()
            .map(spora_rpc_core::RpcCellDep::try_from)
            .collect::<RpcResult<Vec<spora_rpc_core::RpcCellDep>>>()?,
        header_deps: item.header_deps.iter().map(|hash| RpcHash::from_str(hash)).collect::<Result<Vec<_>, _>>()?,
        outputs: item
            .outputs
            .iter()
            .map(spora_rpc_core::RpcTransactionOutput::try_from)
            .collect::<RpcResult<Vec<spora_rpc_core::RpcTransactionOutput>>>()?,
        payload: Vec::from_rpc_hex(&item.payload)?,
        mass: item.mass,
        verbose_data: item.verbose_data.as_ref().map(spora_rpc_core::RpcTransactionVerboseData::try_from).transpose()?,
    }
});

try_from!(item: &protowire::RpcCellDep, spora_rpc_core::RpcCellDep, {
    let dep_type = match item.dep_type {
        0 => spora_rpc_core::RpcDepType::Code,
        1 => spora_rpc_core::RpcDepType::DepGroup,
        value => return Err(RpcError::General(format!("invalid RpcCellDep.depType: {value}"))),
    };
    Self {
        out_point: item
            .out_point
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("RpcCellDep".to_string(), "outPoint".to_string()))?
            .try_into()?,
        dep_type,
    }
});

try_from!(item: &protowire::RpcTransactionInput, spora_rpc_core::RpcTransactionInput, {
    Self {
        previous_outpoint: item
            .previous_outpoint
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("RpcTransactionInput".to_string(), "previous_outpoint".to_string()))?
            .try_into()?,
        since: item.since,
        witness: Vec::from_rpc_hex(&item.witness)?,
        verbose_data: item.verbose_data.as_ref().map(spora_rpc_core::RpcTransactionInputVerboseData::try_from).transpose()?,
    }
});

try_from!(item: &protowire::RpcTransactionOutput, spora_rpc_core::RpcTransactionOutput, {
    Self {
        value: item.amount,
        capacity: if item.capacity == 0 { None } else { Some(item.capacity) },
        data_bytes: if item.data_bytes == 0 { None } else { Some(item.data_bytes) },
        lock_hash: if item.lock_hash.is_empty() { None } else { Some(RpcHash::from_str(&item.lock_hash)?.as_bytes()) },
        type_hash: if item.type_hash.is_empty() { None } else { Some(RpcHash::from_str(&item.type_hash)?.as_bytes()) },
        data_hash: if item.data_hash.is_empty() { None } else { Some(RpcHash::from_str(&item.data_hash)?.as_bytes()) },
        lock_script: item
            .lock_script
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("RpcTransactionOutput".to_string(), "lock_script".to_string()))?
            .try_into()?,
        type_script: item.type_script.as_ref().map(spora_rpc_core::RpcScript::try_from).transpose()?,
        output_data: if item.output_data.is_empty() { None } else { Some(Vec::from_rpc_hex(&item.output_data)?) },
        verbose_data: item.verbose_data.as_ref().map(spora_rpc_core::RpcTransactionOutputVerboseData::try_from).transpose()?,
    }
});

try_from!(item: &protowire::RpcOutpoint, spora_rpc_core::RpcTransactionOutpoint, {
    Self { transaction_id: RpcHash::from_str(&item.transaction_id)?, index: item.index }
});

try_from!(item: &protowire::RpcCellEntry, spora_rpc_core::RpcCellEntry, {
    Self {
        amount: item.amount,
        capacity: item.capacity,
        data_bytes: item.data_bytes,
        lock_hash: if item.lock_hash.is_empty() { [0; 32] } else { RpcHash::from_str(&item.lock_hash)?.as_bytes() },
        type_hash: if item.type_hash.is_empty() { None } else { Some(RpcHash::from_str(&item.type_hash)?.as_bytes()) },
        data_hash: if item.data_hash.is_empty() { [0; 32] } else { RpcHash::from_str(&item.data_hash)?.as_bytes() },
        block_daa_score: item.block_daa_score,
        is_coinbase: item.is_coinbase,
    }
});

try_from!(item: &protowire::RpcScript, spora_rpc_core::RpcScript, {
    Self {
        code_hash: RpcHash::from_str(&item.code_hash)?.as_bytes(),
        hash_type: item.hash_type.try_into()?,
        args: Vec::from_rpc_hex(item.args.as_str())?,
    }
});

try_from!(item: &protowire::RpcTransactionVerboseData, spora_rpc_core::RpcTransactionVerboseData, {
    Self {
        transaction_id: RpcHash::from_str(&item.transaction_id)?,
        hash: RpcHash::from_str(&item.hash)?,
        compute_mass: item.compute_mass,
        transient_mass: item.transient_mass,
        storage_mass: item.storage_mass,
        verified_cycles: item.verified_cycles,
        block_hash: RpcHash::from_str(&item.block_hash)?,
        block_time: item.block_time,
    }
});

try_from!(&protowire::RpcTransactionInputVerboseData, spora_rpc_core::RpcTransactionInputVerboseData);

try_from!(item: &protowire::RpcTransactionOutputVerboseData, spora_rpc_core::RpcTransactionOutputVerboseData, {
    Self {
        lock_script_type: item.lock_script_type.parse()?,
        lock_script_address: item.lock_script_address.as_str().try_into()?,
        resolved_lock_kind: if item.resolved_lock_kind.is_empty() {
            None
        } else {
            Some(item.resolved_lock_kind.parse()?)
        },
        resolved_address_kind: if item.resolved_address_kind.is_empty() {
            None
        } else {
            Some(item.resolved_address_kind.parse()?)
        },
    }
});

try_from!(item: &protowire::RpcAcceptedTransactionIds, spora_rpc_core::RpcAcceptedTransactionIds, {
    Self {
        accepting_block_hash: RpcHash::from_str(&item.accepting_block_hash)?,
        accepted_transaction_ids: item.accepted_transaction_ids.iter().map(|x| RpcHash::from_str(x)).collect::<Result<Vec<_>, _>>()?,
    }
});

try_from!(item: &protowire::RpcCellsByAddressesEntry, spora_rpc_core::RpcCellsByAddressesEntry, {
    let address = if item.address.is_empty() { None } else { Some(item.address.as_str().try_into()?) };
    Self {
        address,
        outpoint: item
            .outpoint
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("CellsByAddressesEntry".to_string(), "outpoint".to_string()))?
            .try_into()?,
        cell_entry: item
            .cell_entry
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("CellsByAddressesEntry".to_string(), "cell_entry".to_string()))?
            .try_into()?,
    }
});

#[cfg(test)]
mod tests {
    use super::*;
    use spora_consensus_core::tx::{CellDep, CellInput, CellOutput, CellTx, DepType, OutPoint, Script};

    #[test]
    fn rpc_transaction_protowire_roundtrip_preserves_cell_and_header_deps() {
        let mut tx = CellTx::new(
            vec![CellInput::new(OutPoint::new([0x11; 32], 2), 42)],
            vec![
                CellDep { out_point: OutPoint::new([0x33; 32], 4), dep_type: DepType::Code },
                CellDep { out_point: OutPoint::new([0x44; 32], 5), dep_type: DepType::DepGroup },
            ],
            vec![CellOutput {
                lock: Script::new([0x22; 32], 1, vec![0xaa, 0xbb]),
                type_: Some(Script::new([0x23; 32], 2, vec![0xcc])),
                capacity: 1_337,
            }],
            vec![vec![1, 2, 3, 4]],
            vec![vec![0xde, 0xad]],
        )
        .expect("fixture CellTx is valid");
        tx.header_deps = vec![[0x55; 32], [0x66; 32]];

        let rpc_tx = spora_rpc_core::RpcTransaction::from(&tx);
        let wire = protowire::RpcTransaction::from(&rpc_tx);

        assert_eq!(wire.cell_deps.len(), 2);
        assert_eq!(wire.cell_deps[0].dep_type, 0);
        assert_eq!(wire.cell_deps[1].dep_type, 1);
        assert_eq!(
            wire.cell_deps[0].out_point.as_ref().expect("cell dep has out point").transaction_id,
            RpcHash::from_bytes([0x33; 32]).to_string()
        );
        assert_eq!(wire.header_deps, vec![RpcHash::from_bytes([0x55; 32]).to_string(), RpcHash::from_bytes([0x66; 32]).to_string()]);

        let restored_rpc =
            spora_rpc_core::RpcTransaction::try_from(&wire).expect("protowire transaction converts back into RPC model");
        assert_eq!(restored_rpc.cell_deps, rpc_tx.cell_deps);
        assert_eq!(restored_rpc.header_deps, rpc_tx.header_deps);
        assert_eq!(restored_rpc.inputs[0].witness, vec![0xde, 0xad]);
        assert_eq!(restored_rpc.outputs[0].output_data, Some(vec![1, 2, 3, 4]));
        assert_eq!(restored_rpc.outputs[0].type_script, rpc_tx.outputs[0].type_script);

        let restored_tx = CellTx::try_from(restored_rpc).expect("RPC transaction converts back into CellTx");
        assert_eq!(restored_tx.inputs, tx.inputs);
        assert_eq!(restored_tx.cell_deps, tx.cell_deps);
        assert_eq!(restored_tx.header_deps, tx.header_deps);
        assert_eq!(restored_tx.outputs, tx.outputs);
        assert_eq!(restored_tx.outputs_data, tx.outputs_data);
        assert_eq!(restored_tx.witnesses, tx.witnesses);
    }

    #[test]
    fn rpc_cell_dep_rejects_invalid_dep_type() {
        let wire = protowire::RpcCellDep {
            out_point: Some(protowire::RpcOutpoint { transaction_id: RpcHash::from_bytes([0x77; 32]).to_string(), index: 0 }),
            dep_type: 7,
        };

        let error = spora_rpc_core::RpcCellDep::try_from(&wire).expect_err("invalid dep type must be rejected");

        assert!(error.to_string().contains("invalid RpcCellDep.depType: 7"));
    }

    #[test]
    fn rpc_cell_dep_requires_out_point() {
        let wire = protowire::RpcCellDep { out_point: None, dep_type: 0 };

        let error = spora_rpc_core::RpcCellDep::try_from(&wire).expect_err("missing out point must be rejected");
        let message = error.to_string();

        assert!(message.contains("RpcCellDep"));
        assert!(message.contains("outPoint"));
    }
}
