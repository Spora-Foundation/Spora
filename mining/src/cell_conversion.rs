use blake3::Hasher;
use spora_consensus_core::{
    cell_metadata::CellMetadata,
    tx::{CellOut, ScriptPublicKey, TransactionOutpoint},
};
use spora_hashes::Hash;

#[cfg(test)]
use spora_consensus_core::tx::{
    legacy_sequence_to_cell_since, CellEntry, CellRef, CellTx, OutPoint, OutPointCompat, ScriptRef, Transaction, TransactionId,
};
#[cfg(test)]
use std::collections::HashMap;

pub(crate) fn compute_lock_hash(script_public_key: &ScriptPublicKey) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(b"spora-cell/lock");
    hasher.update(&script_public_key.version().to_le_bytes());
    hasher.update(script_public_key.script());
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
pub(crate) fn cell_output_to_placeholder_entry(
    output: &CellOut,
    output_data: &[u8],
    block_daa_score: u64,
    is_cellbase: bool,
) -> CellEntry {
    CellEntry::from_cell_metadata(
        output.capacity,
        output_data.len() as u64,
        output.lock.hash(),
        output.type_.as_ref().map(|type_script| type_script.hash()),
        *blake3::hash(output_data).as_bytes(),
        block_daa_score,
        is_cellbase,
    )
}

pub(crate) fn cell_output_to_metadata(
    out_point: TransactionOutpoint,
    output: &CellOut,
    output_data: &[u8],
    block_daa_score: u64,
    is_cellbase: bool,
) -> CellMetadata {
    CellMetadata {
        out_point,
        capacity: output.capacity,
        data_bytes: output_data.len() as u64,
        lock_hash: output.lock.hash(),
        type_hash: output.type_.as_ref().map(|type_script| type_script.hash()),
        data_hash: *blake3::hash(output_data).as_bytes(),
        block_daa_score,
        is_cellbase,
        block_hash: Hash::default(),
        lock_code_hash: None,
        type_code_hash: None,
        lock_script: Some(output.lock.clone()),
        type_script: output.type_.clone(),
        data: Some(output_data.to_vec()),
    }
}

#[cfg(test)]
pub(crate) fn legacy_tx_to_cell_tx_with_context(
    tx: &Transaction,
    parent_cell_ids: &HashMap<TransactionId, TransactionId>,
) -> Result<CellTx, &'static str> {
    if tx.is_coinbase() {
        let outputs = tx
            .outputs
            .iter()
            .map(|output| CellOut {
                lock: ScriptRef::new(compute_lock_hash(&output.script_public_key), 0, vec![]),
                type_: None,
                capacity: output.value,
            })
            .collect::<Vec<_>>();
        let mut outputs_data = vec![vec![]; outputs.len()];
        if let Some(first) = outputs_data.first_mut() {
            *first = tx.payload.clone();
            return CellTx::new(vec![], vec![], outputs, outputs_data, vec![]);
        }

        return CellTx::new(vec![], vec![], outputs, outputs_data, vec![tx.payload.clone()]);
    }

    if !tx.payload.is_empty() {
        return Err("legacy transaction conversion does not support non-coinbase payloads");
    }

    let inputs = tx
        .inputs
        .iter()
        .map(|input| {
            let outpoint_tx_id = input.previous_outpoint.transaction_id();
            let parent_tx_id = parent_cell_ids.get(&outpoint_tx_id).copied().unwrap_or(outpoint_tx_id);
            CellRef::new(
                OutPoint::new(parent_tx_id.as_bytes().into(), input.previous_outpoint.index),
                legacy_sequence_to_cell_since(input.sequence),
            )
        })
        .collect::<Vec<_>>();
    let outputs = tx
        .outputs
        .iter()
        .map(|output| CellOut {
            lock: ScriptRef::new(compute_lock_hash(&output.script_public_key), 0, vec![]),
            type_: None,
            capacity: output.value,
        })
        .collect::<Vec<_>>();
    let outputs_data = vec![vec![]; outputs.len()];
    let witnesses = tx.inputs.iter().map(|input| input.signature_script.clone()).collect::<Vec<_>>();

    CellTx::new(inputs, vec![], outputs, outputs_data, witnesses)
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn legacy_tx_to_cell_tx(tx: &Transaction) -> Result<CellTx, &'static str> {
    legacy_tx_to_cell_tx_with_context(tx, &HashMap::new())
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn legacy_txs_to_cell_txs<'a>(txs: impl IntoIterator<Item = &'a Transaction>) -> Result<Vec<CellTx>, &'static str> {
    let mut parent_cell_ids = HashMap::new();
    let mut cell_txs = Vec::new();

    for tx in txs {
        let cell_tx = legacy_tx_to_cell_tx_with_context(tx, &parent_cell_ids)?;
        parent_cell_ids.insert(tx.id(), cell_tx.id().into());
        cell_txs.push(cell_tx);
    }

    Ok(cell_txs)
}

#[cfg(test)]
#[cfg(test)]
pub(crate) fn legacy_tx_cell_id(tx: &Transaction) -> Option<TransactionId> {
    legacy_tx_to_cell_tx(tx).ok().map(|cell_tx| cell_tx.id().into())
}
