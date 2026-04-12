use criterion::{criterion_group, criterion_main, Criterion};
use rayon::prelude::*;

use spora_consensus_core::{
    tx::{CellOut, CellRef, CellTx, ScriptPublicKey, ScriptRef, TransactionOutpoint},
    Hash,
};

fn constuct_tx() -> CellTx {
    let outpoint = TransactionOutpoint { tx_hash: Hash::from_bytes([0xFF; 32]).as_bytes(), index: 0 };
    let inputs = vec![CellRef::new(outpoint, 0)];
    let outputs = vec![CellOut { capacity: 10000, lock: ScriptRef::new([0xff; 32], 0, vec![0xff; 35]), type_: None }];
    let outputs_data = vec![vec![]];
    let witnesses = vec![vec![]];
    CellTx::new(inputs, vec![], outputs, outputs_data, witnesses).expect("valid CellTx")
}

fn construct_txs_serially() {
    let _ = (0..10000)
        .map(|_| {
            constuct_tx();
        })
        .collect::<Vec<_>>();
}

fn construct_txs_parallel() {
    let _ = (0..10000)
        .into_par_iter()
        .map(|_| {
            constuct_tx();
        })
        .collect::<Vec<_>>();
}

pub fn bench_compare_tx_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("compare txs");
    group.bench_function("Transaction::SerialCreation", |b| b.iter(construct_txs_serially));
    group.bench_function("Transaction::ParallelCreation", |b| b.iter(construct_txs_parallel));
    group.finish();
}

criterion_group!(benches, bench_compare_tx_generation);
criterion_main!(benches);
