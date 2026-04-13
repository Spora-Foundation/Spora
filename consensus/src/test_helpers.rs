use rand::{rngs::SmallRng, Rng};
use spora_consensus_core::{block::Block, header::Header};
#[cfg(test)]
use spora_consensus_core::{cell_diff::CellMeta, coinbase::MinerData};
#[cfg(any(feature = "vm", test))]
use spora_consensus_core::{cell_metadata::CellMetadata, tx::TransactionOutpoint};
#[cfg(any(feature = "vm", test))]
use spora_exec::scripts::{always_success_code_hash, ALWAYS_SUCCESS_SCRIPT};
#[cfg(any(feature = "vm", test))]
use spora_exec::OutPoint;
#[cfg(any(feature = "vm", test))]
use spora_exec::Script;
use spora_hashes::{Hash, HASH_SIZE};

pub fn header_from_precomputed_hash(hash: Hash, parents: Vec<Hash>) -> Header {
    Header::from_precomputed_hash(hash, parents)
}

pub fn block_from_precomputed_hash(hash: Hash, parents: Vec<Hash>) -> Block {
    Block::from_precomputed_hash(hash, parents)
}

// Transaction-output helper functions have been removed.
// Use Cell model equivalents from cell_diff::CellMeta instead.

pub fn generate_random_hash(rng: &mut SmallRng) -> Hash {
    let random_bytes = rng.gen::<[u8; HASH_SIZE]>();
    Hash::from_bytes(random_bytes)
}

pub fn generate_random_hashes(rng: &mut SmallRng, amount: usize) -> Vec<Hash> {
    let mut hashes = Vec::with_capacity(amount);
    let mut i = 0;
    while i < amount {
        hashes.push(generate_random_hash(rng));
        i += 1;
    }
    hashes
}

///Note: generate_random_block is filled with random data, it does not represent a consensus-valid block!
pub fn generate_random_block(
    rng: &mut SmallRng,
    parent_amount: usize,
    _number_of_transactions: usize,
    _input_amount: usize,
    _output_amount: usize,
) -> Block {
    // Cell model: generate_random_block does not produce transactions.
    // Block-level Cell transactions should be constructed using CellTx from spora_exec.
    Block::new(
        generate_random_header(rng, parent_amount),
        vec![], // Empty transactions for now
    )
}

///Note: generate_random_header is filled with random data, it does not represent a consensus-valid header!
pub fn generate_random_header(rng: &mut SmallRng, parent_amount: usize) -> Header {
    Header::new_finalized(
        rng.gen(),
        vec![generate_random_hashes(rng, parent_amount)],
        generate_random_hash(rng),
        generate_random_hash(rng),
        generate_random_hash(rng),
        generate_random_hash(rng), // cell_root
        generate_random_hash(rng), // segment_root
        rng.gen(),
        rng.gen(),
        rng.gen(),
        rng.gen(),
        rng.gen::<u64>().into(),
        rng.gen(),
        generate_random_hash(rng),
    )
}

//TODO: create `assert_eq_<spora-sturct>!()` helper macros in `consensus::test_helpers`

#[cfg(any(feature = "vm", test))]
pub fn always_success_lock_script() -> Script {
    Script::new(always_success_code_hash(), 0, vec![])
}

#[cfg(any(feature = "vm", test))]
pub fn always_success_cell_metadata(out_point: &OutPoint, block_hash: Hash) -> CellMetadata {
    CellMetadata {
        out_point: TransactionOutpoint { tx_hash: out_point.tx_hash, index: out_point.index },
        capacity: 1_000,
        data_bytes: ALWAYS_SUCCESS_SCRIPT.len() as u64,
        lock_hash: [0; 32],
        type_hash: None,
        data_hash: [0; 32],
        block_daa_score: 0,
        is_cellbase: false,
        block_hash,
        lock_code_hash: None,
        type_code_hash: None,
        lock_script: Some(always_success_lock_script()),
        type_script: None,
        data: Some(ALWAYS_SUCCESS_SCRIPT.to_vec()),
    }
}

#[cfg(test)]
pub fn empty_miner_data() -> MinerData {
    MinerData::new(Script::new([0; 32], 0, vec![]), vec![])
}

#[cfg(test)]
pub fn test_cell_entry(capacity: u64, block_daa_score: u64, is_cellbase: bool) -> CellMeta {
    CellMeta::from_cell_metadata(capacity, 0, [0; 32], None, [0; 32], block_daa_score, is_cellbase)
}
