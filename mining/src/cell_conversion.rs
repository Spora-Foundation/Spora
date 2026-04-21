use spora_consensus_core::{
    cell_metadata::CellMetadata,
    tx::{CellOutput, TransactionOutpoint},
};
use spora_hashes::Hash;

#[cfg(test)]
pub(crate) fn cell_output_to_placeholder_entry(
    output: &CellOutput,
    output_data: &[u8],
    block_daa_score: u64,
    is_cellbase: bool,
) -> spora_consensus_core::cell_diff::CellMeta {
    spora_consensus_core::cell_diff::CellMeta::from_cell_metadata(
        output.capacity,
        output_data.len() as u64,
        output.lock.hash(),
        output.type_.as_ref().map(|type_script| type_script.hash()),
        *blake3::hash(output_data).as_bytes(),
        block_daa_score,
        is_cellbase,
    )
    .with_resolved_metadata(Some(output.lock.clone()), output.type_.clone(), Some(output_data.to_vec()))
}

pub(crate) fn cell_output_to_metadata(
    out_point: TransactionOutpoint,
    output: &CellOutput,
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
