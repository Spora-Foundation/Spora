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
    }
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
        block_hash: item.block_hash.to_string(),
        block_time: item.block_time,
    }
});

from!(&spora_rpc_core::RpcTransactionInputVerboseData, protowire::RpcTransactionInputVerboseData);

from!(item: &spora_rpc_core::RpcTransactionOutputVerboseData, protowire::RpcTransactionOutputVerboseData, {
    Self {
        lock_script_type: item.lock_script_type.to_string(),
        lock_script_address: (&item.lock_script_address).into(),
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
        block_hash: RpcHash::from_str(&item.block_hash)?,
        block_time: item.block_time,
    }
});

try_from!(&protowire::RpcTransactionInputVerboseData, spora_rpc_core::RpcTransactionInputVerboseData);

try_from!(item: &protowire::RpcTransactionOutputVerboseData, spora_rpc_core::RpcTransactionOutputVerboseData, {
    Self {
        lock_script_type: item.lock_script_type.parse()?,
        lock_script_address: item.lock_script_address.as_str().try_into()?,
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
