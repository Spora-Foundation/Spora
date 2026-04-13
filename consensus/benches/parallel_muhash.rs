use criterion::{black_box, criterion_group, criterion_main, Criterion};
use itertools::Itertools;
use rayon::prelude::*;
use spora_consensus_core::{
    muhash::MuHashExtensions,
    tx::{outpoint_from_id, CellEntry, CellOutput, CellInput, CellTx, Script, SignableTransaction, TransactionId},
};
use spora_muhash::MuHash;
use spora_utils::iter::parallelism_in_power_steps;

fn sample_lock_script(tag: u8) -> Script {
    Script::new([tag; 32], 0, vec![0x51, tag])
}

fn generate_transaction(ins: usize, outs: usize, randomness: u64) -> SignableTransaction {
    let inputs = (0..ins)
        .map(|i| CellInput::new(outpoint_from_id(TransactionId::from_u64_word(((randomness as usize) << 16 | i) as u64), 0), 0))
        .collect_vec();
    let entries = (0..ins)
        .map(|i| CellEntry::from_cell_metadata(22_222_222, 0, sample_lock_script((99 + i) as u8).hash(), None, [0; 32], 23_456, false))
        .collect_vec();
    let outputs =
        (0..outs).map(|i| CellOutput { capacity: 23_456, lock: sample_lock_script((101 + i) as u8), type_: None }).collect_vec();
    let outputs_data = vec![vec![]; outs];
    let witnesses = vec![vec![]; ins];
    let tx = CellTx::new(inputs, vec![], outputs, outputs_data, witnesses).expect("benchmark CellTx must be valid");
    SignableTransaction::with_entries(tx, entries)
}

pub fn parallel_muhash_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("muhash txs");
    let txs = (0..256).map(|i| generate_transaction(2, 2, i)).collect_vec();
    group.bench_function("seq", |b| {
        b.iter(|| {
            let mut mh = MuHash::new();
            for tx in &txs {
                mh.add_transaction(&tx.as_verifiable(), 222);
            }
            black_box(mh)
        })
    });

    for threads in parallelism_in_power_steps() {
        group.bench_function(format!("par {threads}"), |b| {
            let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
            b.iter(|| {
                pool.install(|| {
                    let mh =
                        txs.par_iter().map(|tx| MuHash::from_transaction(&tx.as_verifiable(), 222)).reduce(MuHash::new, |mut a, b| {
                            a.combine(&b);
                            a
                        });
                    black_box(mh)
                })
            })
        });
    }

    group.finish();
}

criterion_group!(benches, parallel_muhash_benchmark);
criterion_main!(benches);
