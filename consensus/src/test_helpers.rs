use rand::{rngs::SmallRng, Rng};
use spora_consensus_core::{block::Block, header::Header};
use spora_hashes::{Hash, HASH_SIZE};

pub fn header_from_precomputed_hash(hash: Hash, parents: Vec<Hash>) -> Header {
    Header::from_precomputed_hash(hash, parents)
}

pub fn block_from_precomputed_hash(hash: Hash, parents: Vec<Hash>) -> Block {
    Block::from_precomputed_hash(hash, parents)
}

// Legacy transaction-output helper functions have been removed.
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
