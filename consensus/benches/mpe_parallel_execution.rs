use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use rayon::prelude::*;
use spora_exec::{CellDep, CellInput, CellOutput, CellTx, DepType, OutPoint, Script};
use spora_hashes::Hash;

#[allow(dead_code, unused_imports)]
#[path = "../src/pipeline/virtual_processor/access_summary.rs"]
mod access_summary;
#[allow(dead_code, unused_imports)]
#[path = "../src/pipeline/virtual_processor/execution_dag.rs"]
mod execution_dag;

use access_summary::BlockAccessSummary;
use execution_dag::ExecutionDAG;

fn hash(tag: u8) -> Hash {
    Hash::from_bytes([tag; 32])
}

fn outpoint(block: usize, index: u32) -> OutPoint {
    OutPoint::new([(block & 0xff) as u8; 32], index)
}

fn output(tag: u8) -> CellOutput {
    CellOutput { lock: Script::new([tag; 32], 0, vec![tag]), type_: None, capacity: 10_000 }
}

fn tx(block: usize, tx_index: usize, read_dep: Option<OutPoint>) -> CellTx {
    let input = CellInput::new(outpoint(block + 1_000, tx_index as u32), 0);
    let deps = read_dep.map(|out_point| vec![CellDep { out_point, dep_type: DepType::Code }]).unwrap_or_default();

    CellTx::new(vec![input], deps, vec![output((block + tx_index) as u8)], vec![vec![]], vec![vec![]])
        .expect("benchmark CellTx must be valid")
}

fn block_txs(block: usize, tx_count: usize) -> Vec<CellTx> {
    let read_dep = (block > 0 && block % 4 == 0).then(|| outpoint(block - 1, 0));
    (0..tx_count).map(|tx_index| tx(block, tx_index, read_dep.clone())).collect()
}

fn summaries(block_count: usize, tx_count: usize) -> Vec<BlockAccessSummary> {
    (0..block_count)
        .map(|block| {
            let txs = block_txs(block, tx_count);
            BlockAccessSummary::try_from_block_txs_with_cellscript_scheduler(hash(block as u8), &txs)
                .expect("benchmark access summary must be valid")
        })
        .collect()
}

fn simulated_block_analysis(block_index: usize, iterations: usize) -> u64 {
    let mut value = block_index as u64;
    for step in 0..iterations {
        value = value.wrapping_mul(1_664_525).wrapping_add(step as u64).rotate_left(7);
    }
    value
}

fn serial_simulated_execution(block_count: usize, work: usize) -> u64 {
    (0..block_count).map(|block| simulated_block_analysis(block, work)).fold(0, |acc, value| acc ^ value)
}

fn mpe_simulated_execution(dag: &ExecutionDAG, work: usize) -> u64 {
    dag.layers
        .iter()
        .map(|layer| {
            if layer.len() == 1 {
                simulated_block_analysis(layer[0], work)
            } else {
                layer.par_iter().map(|&block| simulated_block_analysis(block, work)).reduce(|| 0, |acc, value| acc ^ value)
            }
        })
        .fold(0, |acc, value| acc ^ value)
}

fn mpe_parallel_execution_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("mpe execution dag");

    for &(block_count, tx_count) in &[(8, 4), (32, 4), (128, 2)] {
        group.bench_with_input(BenchmarkId::new("summary+dag", block_count), &(block_count, tx_count), |b, &(blocks, txs)| {
            b.iter(|| {
                let summaries = summaries(blocks, txs);
                black_box(ExecutionDAG::build(&summaries))
            })
        });
    }

    let summaries = summaries(128, 2);
    let dag = ExecutionDAG::build(&summaries);

    for work in [128, 1_024, 8_192] {
        group.bench_with_input(BenchmarkId::new("serial simulated analysis", work), &work, |b, &work| {
            b.iter(|| black_box(serial_simulated_execution(128, work)))
        });
        group.bench_with_input(BenchmarkId::new("mpe simulated analysis", work), &work, |b, &work| {
            b.iter(|| black_box(mpe_simulated_execution(&dag, work)))
        });
    }

    group.finish();
}

criterion_group!(benches, mpe_parallel_execution_benchmark);
criterion_main!(benches);
