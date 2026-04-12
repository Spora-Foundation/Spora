use crate::protowire;
use crate::{from, try_from};
use spora_rpc_core::{FromRpcHex, RpcError, RpcHash, RpcResult, RpcScriptVec, ToRpcHex};
use std::str::FromStr;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &spora_rpc_core::RpcTransaction, protowire::RpcTransaction, {
    Self {
        version: item.version.into(),
        inputs: item.inputs.iter().map(protowire::RpcTransactionInput::from).collect(),
        outputs: item.outputs.iter().map(protowire::RpcTransactionOutput::from).collect(),
        lock_time: item.lock_time,
        subnetwork_id: item.subnetwork_id.to_string(),
        gas: item.gas,
        payload: item.payload.to_rpc_hex(),
        mass: item.mass,
        verbose_data: item.verbose_data.as_ref().map(|x| x.into()),
    }
});

from!(item: &spora_rpc_core::RpcTransactionInput, protowire::RpcTransactionInput, {
    Self {
        previous_outpoint: Some((&item.previous_outpoint).into()),
        signature_script: item.signature_script.to_rpc_hex(),
        sequence: item.sequence,
        sig_op_count: item.sig_op_count.into(),
        since: item.since,
        witness: item.witness.as_ref().map(|w| w.to_rpc_hex()),
        verbose_data: item.verbose_data.as_ref().map(|x| x.into()),
    }
});

from!(item: &spora_rpc_core::RpcTransactionOutput, protowire::RpcTransactionOutput, {
    Self {
        amount: item.value,
        script_public_key: Some((&item.script_public_key).into()),
        verbose_data: item.verbose_data.as_ref().map(|x| x.into()),
        capacity: item.capacity.unwrap_or_default(),
        data_bytes: item.data_bytes.unwrap_or_default(),
        lock_hash: item.lock_hash.map(RpcHash::from).map(|x| x.to_string()).unwrap_or_default(),
        type_hash: item.type_hash.map(RpcHash::from).map(|x| x.to_string()).unwrap_or_default(),
        data_hash: item.data_hash.map(RpcHash::from).map(|x| x.to_string()).unwrap_or_default(),
    }
});

from!(item: &spora_rpc_core::RpcTransactionOutpoint, protowire::RpcOutpoint, {
    Self { transaction_id: item.transaction_id.to_string(), index: item.index }
});

from!(item: &spora_rpc_core::RpcCellEntry, protowire::RpcCellEntry, {
    Self {
        amount: item.amount,
        script_public_key: Some((&item.script_public_key).into()),
        block_daa_score: item.block_daa_score,
        is_coinbase: item.is_coinbase,
        capacity: item.capacity,
        data_bytes: item.data_bytes,
        lock_hash: RpcHash::from(item.lock_hash).to_string(),
        type_hash: item.type_hash.map(RpcHash::from).map(|x| x.to_string()).unwrap_or_default(),
        data_hash: RpcHash::from(item.data_hash).to_string(),
    }
});

from!(item: &spora_rpc_core::RpcScriptPublicKey, protowire::RpcScriptPublicKey, {
    Self { version: item.version().into(), script_public_key: item.script().to_rpc_hex() }
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
        script_public_key_type: item.script_public_key_type.to_string(),
        script_public_key_address: (&item.script_public_key_address).into(),
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
        version: item.version.try_into()?,
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
        lock_time: item.lock_time,
        subnetwork_id: spora_rpc_core::RpcSubnetworkId::from_str(&item.subnetwork_id)?,
        gas: item.gas,
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
        signature_script: Vec::from_rpc_hex(&item.signature_script)?,
        sequence: item.sequence,
        sig_op_count: item.sig_op_count.try_into()?,
        since: item.since,
        witness: item.witness.as_ref().map(|w| Vec::from_rpc_hex(w)).transpose()?,
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
        script_public_key: item
            .script_public_key
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("RpcTransactionOutput".to_string(), "script_public_key".to_string()))?
            .try_into()?,
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
        script_public_key: item
            .script_public_key
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("RpcTransactionOutput".to_string(), "script_public_key".to_string()))?
            .try_into()?,
        block_daa_score: item.block_daa_score,
        is_coinbase: item.is_coinbase,
    }
});

try_from!(item: &protowire::RpcScriptPublicKey, spora_rpc_core::RpcScriptPublicKey, {
    Self::new(u16::try_from(item.version)?, RpcScriptVec::from_rpc_hex(item.script_public_key.as_str())?)
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
        script_public_key_type: item.script_public_key_type.as_str().try_into()?,
        script_public_key_address: item.script_public_key_address.as_str().try_into()?,
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
