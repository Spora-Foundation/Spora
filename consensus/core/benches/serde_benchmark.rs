use criterion::{black_box, criterion_group, criterion_main, Criterion};
use spora_consensus_core::tx::{outpoint_from_id, CellInput, CellOutput, CellTx, Script, TransactionId};
use std::time::{Duration, Instant};

fn sample_lock_script(tag: u8) -> Script {
    Script::new([tag; 32], 0, vec![0x51, tag])
}

fn sample_cell_tx() -> CellTx {
    CellTx::new(
        vec![
            CellInput::new(outpoint_from_id(TransactionId::from_slice(&[0x16; 32]), 0xffff_ffff), 0),
            CellInput::new(outpoint_from_id(TransactionId::from_slice(&[0x4b; 32]), 0xffff_ffff), 1),
        ],
        vec![],
        vec![
            CellOutput { capacity: 300, lock: sample_lock_script(0xaa), type_: None },
            CellOutput { capacity: 300, lock: sample_lock_script(0xbb), type_: None },
        ],
        vec![vec![], vec![]],
        vec![vec![1; 32], vec![2; 32]],
    )
    .expect("benchmark CellTx must be valid")
}

fn sample_script_ref() -> Script {
    sample_lock_script(0xcc)
}

fn serialize_cell_tx_benchmark(c: &mut Criterion) {
    let cell_tx = sample_cell_tx();
    let size = bincode::serialized_size(&cell_tx).unwrap();
    let mut buf = Vec::with_capacity(size as usize);
    c.bench_function("Serialize CellTx", move |b| {
        b.iter_custom(|iters| {
            let start = Duration::default();
            (0..iters).fold(start, |acc, _| {
                let started = Instant::now();
                #[allow(clippy::unit_arg)]
                black_box(bincode::serialize_into(&mut buf, &cell_tx).unwrap());
                let elapsed = started.elapsed();
                buf.clear();
                acc + elapsed
            })
        })
    });
}

fn deserialize_cell_tx_benchmark(c: &mut Criterion) {
    let serialized = bincode::serialize(&sample_cell_tx()).unwrap();
    c.bench_function("Deserialize CellTx", |b| b.iter(|| black_box(bincode::deserialize::<CellTx>(&serialized).unwrap())));
}

fn serialize_script_ref_benchmark(c: &mut Criterion) {
    let script_ref = sample_script_ref();
    let size = bincode::serialized_size(&script_ref).unwrap();
    let mut buf = Vec::with_capacity(size as usize);
    c.bench_function("Serialize Script", move |b| {
        b.iter_custom(|iters| {
            let start = Duration::default();
            (0..iters).fold(start, |acc, _| {
                let started = Instant::now();
                #[allow(clippy::unit_arg)]
                black_box(bincode::serialize_into(&mut buf, &script_ref).unwrap());
                let elapsed = started.elapsed();
                buf.clear();
                acc + elapsed
            })
        })
    });
}

fn deserialize_script_ref_benchmark(c: &mut Criterion) {
    let serialized = bincode::serialize(&sample_script_ref()).unwrap();
    c.bench_function("Deserialize Script", |b| b.iter(|| black_box(bincode::deserialize::<Script>(&serialized).unwrap())));
}

criterion_group!(
    benches,
    serialize_cell_tx_benchmark,
    deserialize_cell_tx_benchmark,
    serialize_script_ref_benchmark,
    deserialize_script_ref_benchmark
);
criterion_main!(benches);
