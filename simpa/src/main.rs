use async_channel::unbounded;
use clap::Parser;
use futures::{future::try_join_all, Future};
use itertools::Itertools;
use simulator::network::SporaNetworkSimulator;
use spora_alloc::init_allocator_with_default_settings;
use spora_consensus::{
    config::ConfigBuilder,
    consensus::Consensus,
    constants::perf::PerfParams,
    model::stores::{
        block_transactions::BlockTransactionsStoreReader,
        ghostdag::{GhostdagStoreReader, KType},
        headers::HeaderStoreReader,
        relations::RelationsStoreReader,
    },
    params::{Params, TenBps, DEVNET_PARAMS, NETWORK_DELAY_BOUND, SIMNET_PARAMS},
};
use spora_consensus_core::{
    api::ConsensusApi, block::Block, blockstatus::BlockStatus, config::bps::calculate_ghostdag_k, errors::block::BlockProcessResult,
    mining_rules::MiningRules, BlockHashSet, BlockLevel, HashMapCustomHasher,
};
use spora_consensus_notify::root::ConsensusNotificationRoot;
use spora_core::{
    info,
    task::{service::AsyncService, tick::TickService},
    time::unix_now,
    trace, warn,
};
use spora_database::prelude::ConnBuilder;
use spora_database::{create_temp_db, load_existing_db};
use spora_hashes::Hash;
use spora_perf_monitor::{builder::Builder, counters::CountersSnapshot};
use spora_utils::fd_budget;
use std::{
    cmp::Reverse,
    collections::{BinaryHeap, VecDeque},
    sync::Arc,
    time::Duration,
};

pub mod simulator;

/// spora Network Simulator
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Simulation blocks per second
    #[arg(short, long, default_value_t = 1.0)]
    bps: f64,

    /// Simulation delay (seconds)
    #[arg(short, long, default_value_t = 2.0)]
    delay: f64,

    /// Number of miners
    #[arg(short, long, default_value_t = 1)]
    miners: u64,

    /// Target transactions per block
    #[arg(short, long, default_value_t = 200)]
    tpb: u64,

    /// Target simulation time (seconds)
    #[arg(short, long, default_value_t = 600)]
    sim_time: u64,

    /// Target number of blocks the simulation should produce (overrides --sim-time if specified)
    #[arg(short = 'n', long)]
    target_blocks: Option<u64>,

    /// Number of pool-thread threads used by the header and body processors.
    /// Defaults to the number of logical CPU cores.
    #[arg(short, long)]
    processors_threads: Option<usize>,

    /// Number of pool-thread threads used by the virtual processor (for parallel transaction verification).
    /// Defaults to the number of logical CPU cores.
    #[arg(short, long)]
    virtual_threads: Option<usize>,

    /// If on, validates headers first before starting to validate block bodies
    #[arg(short = 'f', long, default_value_t = false)]
    headers_first: bool,

    /// Applies a scale factor to memory allocation bounds
    #[arg(long, default_value_t = 1.0)]
    ram_scale: f64,

    /// Logging level for all subsystems {off, error, warn, info, debug, trace}
    ///  -- You may also specify `<subsystem>=<level>,<subsystem2>=<level>,...` to set the log level for individual subsystems
    #[arg(long = "loglevel", default_value = format!("info,{}=trace", env!("CARGO_PKG_NAME")))]
    log_level: String,

    /// Output directory to save the simulation DB
    #[arg(short, long)]
    output_dir: Option<String>,

    /// Input directory of a previous simulation DB (NOTE: simulation args must be compatible with the original run)
    #[arg(short, long)]
    input_dir: Option<String>,

    /// Indicates whether to test pruning. Currently this means we shorten the pruning constants and avoid validating
    /// the DAG in a separate consensus following the simulation phase
    #[arg(long, default_value_t = false)]
    test_pruning: bool,

    /// Use testnet-11 consensus params
    #[arg(long, default_value_t = false)]
    testnet11: bool,
    /// Enable performance metrics: cpu, memory, disk io usage
    #[arg(long, default_value_t = false)]
    perf_metrics: bool,
    #[arg(long, default_value_t = 10)]
    perf_metrics_interval_sec: u64,

    /// Enable rocksdb statistics
    #[arg(long, default_value_t = false)]
    rocksdb_stats: bool,
    #[arg(long)]
    rocksdb_stats_period_sec: Option<u32>,
    #[arg(long)]
    rocksdb_files_limit: Option<i32>,
    #[arg(long)]
    rocksdb_mem_budget: Option<usize>,
    #[arg(long, default_value_t = false)]
    long_payload: bool,
    #[arg(long)]
    retention_period_days: Option<f64>,

    /// Optional cycle budget for chunked virtual-state processing.
    #[arg(long)]
    resumable_virtual_state_step_cycles: Option<u64>,
}

#[cfg(feature = "heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn main() {
    #[cfg(feature = "heap")]
    let _profiler = dhat::Profiler::builder().file_name("simpa-heap.json").build();

    init_allocator_with_default_settings();

    // Get CLI arguments
    let args = Args::parse();

    // Initialize the logger
    cfg_if::cfg_if! {
        if #[cfg(feature = "semaphore-trace")] {
            spora_core::log::init_logger(None, &format!("{},{}=debug", args.log_level, spora_utils::sync::semaphore_module_path()));
        } else {
            spora_core::log::init_logger(None, &args.log_level);
        }
    };

    // Configure the panic behavior
    // As we log the panic, we want to set it up after the logger
    spora_core::panic::configure_panic();

    // Print package name and version
    info!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

    main_impl(args);
}

fn main_impl(mut args: Args) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let stop_perf_monitor = args.perf_metrics.then(|| {
        let ts = Arc::new(TickService::new());

        let cb = move |counters: CountersSnapshot| {
            trace!("[{}] {}", spora_perf_monitor::SERVICE_NAME, counters.to_process_metrics_display());
            trace!("[{}] {}", spora_perf_monitor::SERVICE_NAME, counters.to_io_metrics_display());
            #[cfg(feature = "heap")]
            trace!("heap stats: {:?}", dhat::HeapStats::get());
        };
        let m = Arc::new(
            Builder::new()
                .with_fetch_interval(Duration::from_secs(args.perf_metrics_interval_sec))
                .with_fetch_cb(cb)
                .with_tick_service(ts.clone())
                .build(),
        );
        let monitor = m.clone();
        rt.spawn(async move { monitor.start().await });
        m.stop()
    });

    if args.miners > 1 {
        warn!(
            "Warning: number of miners was configured to {}. Currently each miner added doubles the simulation
        memory and runtime footprint, while a single miner is sufficient for most simulation purposes (delay is simulated anyway).",
            args.miners
        );
    }
    args.bps = if args.testnet11 { TenBps::bps() as f64 } else { args.bps };
    let mut params = if args.testnet11 { SIMNET_PARAMS } else { DEVNET_PARAMS };
    params.coinbase_maturity = 200;
    params.storage_mass_parameter = 10_000;
    let mut builder = ConfigBuilder::new(params)
        .apply_args(|config| apply_args_to_consensus_params(&args, &mut config.params))
        .apply_args(|config| apply_args_to_perf_params(&args, &mut config.perf))
        .adjust_perf_params_to_consensus_params()
        .apply_args(|config| {
            config.ram_scale = args.ram_scale;
            config.retention_period_days = args.retention_period_days;
            config.resumable_virtual_state_step_cycles = args.resumable_virtual_state_step_cycles.filter(|value| *value > 0);
        })
        .skip_proof_of_work()
        .enable_sanity_checks();
    if !args.test_pruning {
        builder = builder.set_archival();
    }
    let config = Arc::new(builder.build());
    let default_fd = fd_budget::limit() / 2;
    let mut conn_builder = ConnBuilder::default().with_parallelism(num_cpus::get()).with_files_limit(default_fd);
    if let Some(rocksdb_files_limit) = args.rocksdb_files_limit {
        conn_builder = conn_builder.with_files_limit(rocksdb_files_limit);
    }
    if let Some(rocksdb_mem_budget) = args.rocksdb_mem_budget {
        conn_builder = conn_builder.with_mem_budget(rocksdb_mem_budget);
    }
    // Load an existing consensus or run the simulation
    let (consensus, _lifetime) = if let Some(input_dir) = args.input_dir {
        let mut config = (*config).clone();
        config.process_genesis = false;
        let config = Arc::new(config);
        let (lifetime, db) = match (args.rocksdb_stats, args.rocksdb_stats_period_sec) {
            (true, Some(rocksdb_stats_period_sec)) => {
                load_existing_db!(input_dir, conn_builder.enable_stats().with_stats_period(rocksdb_stats_period_sec))
            }
            (true, None) => load_existing_db!(input_dir, conn_builder.enable_stats()),
            (false, _) => load_existing_db!(input_dir, conn_builder),
        };
        let (dummy_notification_sender, _) = unbounded();
        let notification_root = Arc::new(ConsensusNotificationRoot::new(dummy_notification_sender));
        let consensus = Arc::new(Consensus::new(
            db,
            config.clone(),
            Default::default(),
            notification_root,
            Default::default(),
            Default::default(),
            unix_now(),
            Arc::new(MiningRules::default()),
        ));
        (consensus, lifetime)
    } else {
        let until = if args.target_blocks.is_none() { config.genesis.timestamp + args.sim_time * 1000 } else { u64::MAX }; // milliseconds
        let mut sim = SporaNetworkSimulator::new(args.delay, args.bps, args.target_blocks, config.clone(), args.output_dir);
        let (consensus, handles, lifetime) = sim
            .init(
                args.miners,
                args.tpb,
                args.rocksdb_stats,
                args.rocksdb_stats_period_sec,
                args.rocksdb_files_limit,
                args.rocksdb_mem_budget,
                args.long_payload,
            )
            .run(until);
        consensus.shutdown(handles);
        (consensus, lifetime)
    };

    if args.test_pruning {
        let hashes = topologically_ordered_hashes(&consensus, consensus.pruning_point());
        let num_blocks = hashes.len();
        let num_txs = print_stats(&consensus, &hashes, args.delay, args.bps, config.ghostdag_k());
        info!("There are {num_blocks} blocks with {num_txs} transactions overall above the current pruning point");

        if args.retention_period_days.is_some() {
            let hashes_retention = topologically_ordered_hashes(&consensus, consensus.get_retention_period_root());
            info!("There are {} blocks above the retention period root", hashes_retention.len());
        }

        consensus.validate_pruning_points(consensus.get_sink()).unwrap();

        // Test whether we can still retrieve a populated transaction given a txid and the accepting block daa score.
        let mut attempted_reconstruction = 0usize;
        let mut successful_reconstruction = 0usize;
        let mut failed_reconstruction = 0usize;
        let mut missing_ghostdag_sources = 0usize;
        let mut reconstruction_error_examples = Vec::new();

        for hash in hashes.iter().cloned() {
            let is_chain_block = match consensus.is_chain_block(hash) {
                Ok(is_chain_block) => is_chain_block,
                Err(err) => {
                    warn!("Skipping historical tx reconstruction checks for {hash}: failed to query chain-block status: {err}");
                    continue;
                }
            };
            if !is_chain_block {
                // only chain blocks are worth checking the acceptance data of
                continue;
            }

            if let Ok(block_acceptance_data) = consensus.get_block_acceptance_data(hash) {
                block_acceptance_data.iter().for_each(|cbad| {
                    let source_block = cbad.block_hash;
                    let Ok(source_block_ghostdag) = consensus.ghostdag_store.get_data(source_block) else {
                        missing_ghostdag_sources += 1;
                        return;
                    };
                    let source_block_pov = source_block_ghostdag.selected_parent;
                    if consensus.get_block_status(source_block_pov) != Some(BlockStatus::StatusCellValid) {
                        // Pruning may reduce the source block POV to header-only. Historical input
                        // reconstruction is not guaranteed once that POV is no longer queryable.
                        return;
                    }

                    cbad.accepted_transactions.iter().for_each(|ate| {
                        attempted_reconstruction += 1;
                        match consensus
                            .get_resolved_cell_transaction_in_accepting_block(ate.transaction_id, source_block)
                            .map(spora_consensus_core::tx::ResolvedCellTransaction::into_signable_transaction)
                        {
                            Ok(_) => successful_reconstruction += 1,
                            Err(err) => {
                                failed_reconstruction += 1;
                                if reconstruction_error_examples.len() < 3 {
                                    reconstruction_error_examples
                                        .push(format!("{} in source block {}: {}", ate.transaction_id, source_block, err));
                                }
                            }
                        }
                    });
                });
            }
        }

        assert!(
            successful_reconstruction > 0 || attempted_reconstruction == 0,
            "Expected at least one historical transaction reconstruction to succeed above the pruning point"
        );
        info!(
            "Historical reconstruction after pruning: attempted {}, succeeded {}, skipped {}, missing ghostdag sources {}",
            attempted_reconstruction, successful_reconstruction, failed_reconstruction, missing_ghostdag_sources
        );
        for example in reconstruction_error_examples {
            warn!("Historical reconstruction sample failure: {example}");
        }

        drop(consensus);
        return;
    }

    // Benchmark the DAG validation time
    let (_lifetime2, db2) = create_temp_db!(ConnBuilder::default().with_parallelism(num_cpus::get()).with_files_limit(default_fd));
    let (dummy_notification_sender, _) = unbounded();
    let notification_root = Arc::new(ConsensusNotificationRoot::new(dummy_notification_sender));
    let consensus2 = Arc::new(Consensus::new(
        db2,
        config.clone(),
        Default::default(),
        notification_root,
        Default::default(),
        Default::default(),
        unix_now(),
        Arc::new(MiningRules::default()),
    ));
    let handles2 = consensus2.run_processors();
    if args.headers_first {
        rt.block_on(validate(&consensus, &consensus2, &config, args.delay, args.bps, true));
    }
    rt.block_on(validate(&consensus, &consensus2, &config, args.delay, args.bps, false));
    consensus2.shutdown(handles2);
    if let Some(stop_perf_monitor) = stop_perf_monitor {
        _ = rt.block_on(stop_perf_monitor);
    }
    drop(consensus);
}

fn apply_args_to_consensus_params(args: &Args, params: &mut Params) {
    // We have no actual PoW in the simulation, so the true max is most reflective,
    // however we avoid the actual max since it is reserved for the DB prefix scheme
    params.max_block_level = BlockLevel::MAX - 1;
    params.genesis.timestamp = 0;
    if args.testnet11 {
        info!(
            "Using spora-testnet-11 configuration (GHOSTDAG K={}, DAA window size={}, Median time window size={})",
            params.ghostdag_k(),
            params.difficulty_window_size(),
            params.past_median_time_window_size(),
        );
    } else {
        let max_delay = args.delay.max(NETWORK_DELAY_BOUND as f64);
        let k = u64::max(calculate_ghostdag_k(2.0 * max_delay * args.bps, 0.05), params.ghostdag_k() as u64);
        let k = u64::min(k, KType::MAX as u64) as KType; // Clamp to KType::MAX
        params.ghostdag_k = k;
        params.mergeset_size_limit = k as u64 * 10;
        params.max_block_parents = u8::max((0.66 * k as f64) as u8, 10);
        params.target_time_per_block = (1000.0 / args.bps) as u64;
        params.merge_depth = (params.merge_depth as f64 * args.bps) as u64;
        params.coinbase_maturity = (params.coinbase_maturity as f64 * f64::max(1.0, args.bps * args.delay * 0.25)) as u64;

        // Use the new sampling algorithms
        params.timestamp_deviation_tolerance = (600.0 * args.bps) as u64;
        params.past_median_time_sample_rate = (10.0 * args.bps) as u64;
        params.difficulty_sample_rate = (2.0 * args.bps) as u64;

        info!("2D\u{03bb}={}, GHOSTDAG K={}, DAA window size={}", 2.0 * args.delay * args.bps, k, params.difficulty_window_size());
    }
    if args.test_pruning {
        params.pruning_proof_m = 16;
        params.min_difficulty_window_size = 16;
        params.timestamp_deviation_tolerance = 16;
        params.sampled_difficulty_window_size = params.sampled_difficulty_window_size.min(32);

        params.ghostdag_k = 20;
        params.finality_depth = 100 * 2;
        params.merge_depth = 64 * 2;
        params.mergeset_size_limit = 32 * 2;
        params.pruning_depth = 100 * 2 * 2 + 50;

        info!("Setting pruning depth to {:?}", params.pruning_depth());
    }
}

fn apply_args_to_perf_params(args: &Args, perf_params: &mut PerfParams) {
    if let Some(processors_pool_threads) = args.processors_threads {
        perf_params.block_processors_num_threads = processors_pool_threads;
    }
    if let Some(virtual_pool_threads) = args.virtual_threads {
        perf_params.virtual_processor_num_threads = virtual_pool_threads;
    }
}

async fn validate(src_consensus: &Consensus, dst_consensus: &Consensus, params: &Params, delay: f64, bps: f64, header_only: bool) {
    let hashes = topologically_ordered_hashes(src_consensus, params.genesis.hash);
    let num_blocks = hashes.len();
    let num_txs = print_stats(src_consensus, &hashes, delay, bps, params.ghostdag_k());
    if header_only {
        info!("Validating {num_blocks} headers...");
    } else {
        info!("Validating {num_blocks} blocks with {num_txs} transactions overall...");
    }

    let start = std::time::Instant::now();
    if header_only {
        let chunks = hashes.into_iter().chunks(1000);
        let mut iter = chunks.into_iter();
        let mut chunk = iter.next().unwrap();
        let mut prev_joins = submit_chunk(src_consensus, dst_consensus, &mut chunk, true);

        for (i, mut chunk) in iter.enumerate() {
            let current_joins = submit_chunk(src_consensus, dst_consensus, &mut chunk, true);
            let statuses = try_join_all(prev_joins).await.unwrap();
            trace!("Validated chunk {}", i);
            assert!(statuses.iter().all(|s| s.is_header_only()));
            prev_joins = current_joins;
        }

        let statuses = try_join_all(prev_joins).await.unwrap();
        assert!(statuses.iter().all(|s| s.is_header_only()));
    } else {
        for (i, hash) in hashes.into_iter().enumerate() {
            let block = Block::from_arcs(
                src_consensus.headers_store.get_header(hash).unwrap(),
                src_consensus.block_transactions_store.get(hash).unwrap(),
            );
            let status = dst_consensus.validate_and_insert_block(block).virtual_state_task.await.unwrap();
            assert!(status.is_cell_valid_or_pending());
            if i > 0 && i % 1000 == 0 {
                trace!("Validated {} blocks", i);
            }
        }
    }

    // Assert that at least one body tip was resolved with valid cells
    assert!(dst_consensus
        .body_tips()
        .iter()
        .copied()
        .any(|h| dst_consensus.get_block_status(h) == Some(BlockStatus::StatusCellValid)));
    let elapsed = start.elapsed();
    info!(
        "Total validation time: {:?}, {} processing rate: {:.2} (b/s), transaction processing rate: {:.2} (t/s)",
        elapsed,
        if header_only { "header" } else { "block" },
        num_blocks as f64 / elapsed.as_secs_f64(),
        num_txs as f64 / elapsed.as_secs_f64(),
    );
}

fn submit_chunk(
    src_consensus: &Consensus,
    dst_consensus: &Consensus,
    chunk: &mut impl Iterator<Item = Hash>,
    header_only: bool,
) -> Vec<impl Future<Output = BlockProcessResult<BlockStatus>>> {
    let mut futures = Vec::new();
    for hash in chunk {
        let block = Block::from_arcs(
            src_consensus.headers_store.get_header(hash).unwrap(),
            if header_only { Default::default() } else { src_consensus.block_transactions_store.get(hash).unwrap() },
        );
        let f = dst_consensus.validate_and_insert_block(block).virtual_state_task;
        futures.push(f);
    }
    futures
}

fn topologically_ordered_hashes(src_consensus: &Consensus, genesis_hash: Hash) -> Vec<Hash> {
    let mut queue: VecDeque<Hash> = std::iter::once(genesis_hash).collect();
    let mut reachable = BlockHashSet::new();
    let relations = src_consensus.relations_stores.read();
    while let Some(current) = queue.pop_front() {
        for child in relations[0].get_children(current).unwrap().read().iter() {
            if reachable.insert(*child) {
                queue.push_back(*child);
            }
        }
    }

    let mut remaining_parents = spora_consensus_core::BlockHashMap::with_capacity(reachable.len());
    let mut ready = BinaryHeap::new();
    for hash in reachable.iter().copied() {
        let parent_count = relations[0]
            .get_parents(hash)
            .unwrap()
            .iter()
            .copied()
            .filter(|parent| *parent != genesis_hash && reachable.contains(parent))
            .count();
        remaining_parents.insert(hash, parent_count);
        if parent_count == 0 {
            ready.push(Reverse(hash));
        }
    }

    let mut ordered = Vec::with_capacity(reachable.len());
    while let Some(Reverse(current)) = ready.pop() {
        ordered.push(current);
        for child in relations[0].get_children(current).unwrap().read().iter().copied() {
            let Some(remaining) = remaining_parents.get_mut(&child) else {
                continue;
            };

            *remaining -= 1;
            if *remaining == 0 {
                ready.push(Reverse(child));
            }
        }
    }

    assert_eq!(ordered.len(), reachable.len(), "simpa replay ordering must include every descendant of {genesis_hash}");
    ordered
}

fn print_stats(src_consensus: &Consensus, hashes: &[Hash], delay: f64, bps: f64, k: KType) -> usize {
    let blues_mean = hashes.iter().map(|&h| src_consensus.ghostdag_store.get_data(h).unwrap().mergeset_blues.len()).sum::<usize>()
        as f64
        / hashes.len() as f64;
    let reds_mean = hashes.iter().map(|&h| src_consensus.ghostdag_store.get_data(h).unwrap().mergeset_reds.len()).sum::<usize>()
        as f64
        / hashes.len() as f64;
    let parents_mean = hashes.iter().map(|&h| src_consensus.headers_store.get_header(h).unwrap().direct_parents().len()).sum::<usize>()
        as f64
        / hashes.len() as f64;
    let mut missing_bodies = 0usize;
    let num_txs = hashes
        .iter()
        .map(|&h| match src_consensus.block_transactions_store.get(h) {
            Ok(txs) => txs.len(),
            Err(_) => {
                missing_bodies += 1;
                0
            }
        })
        .sum::<usize>();
    let txs_mean = num_txs as f64 / hashes.len() as f64;
    info!("[DELAY={delay}, BPS={bps}, GHOSTDAG K={k}]");
    info!("[Average stats of generated DAG] blues: {blues_mean}, reds: {reds_mean}, parents: {parents_mean}, txs: {txs_mean}");
    if missing_bodies > 0 {
        info!("Skipped transaction stats for {missing_bodies} blocks whose bodies are no longer queryable");
    }
    num_txs
}

#[cfg(test)]
mod tests {
    use super::*;
    use spora_consensus::{config::ConfigBuilder, consensus::test_consensus::TestConsensus, params::DEVNET_PARAMS};
    use spora_consensus_core::api::ConsensusApi;

    #[test]
    fn test_pruning_via_simpa() {
        let mut args = Args::parse_from(std::iter::empty::<&str>());
        args.bps = 1.0;
        args.target_blocks = Some(500);
        args.tpb = 1;
        args.test_pruning = true;
        args.log_level = "info".to_string();

        spora_core::log::try_init_logger(&args.log_level);
        // As we log the panic, we want to set it up after the logger
        spora_core::panic::configure_panic();
        main_impl(args);
    }

    #[tokio::test]
    async fn replay_order_keeps_parents_before_children_with_skewed_timestamps() {
        let config = ConfigBuilder::new(DEVNET_PARAMS).skip_proof_of_work().build();
        let consensus = TestConsensus::new(&config);
        let wait_handles = consensus.init();

        let mut early_parent = consensus.build_block_with_parents(1.into(), vec![config.genesis.hash]);
        early_parent.header.timestamp = config.genesis.timestamp + 10;
        early_parent.header.finalize();
        let early_parent_hash = early_parent.header.hash;
        consensus.validate_and_insert_block(early_parent.to_immutable()).virtual_state_task.await.unwrap();

        let mut late_parent = consensus.build_block_with_parents(2.into(), vec![config.genesis.hash]);
        late_parent.header.timestamp = config.genesis.timestamp + 100;
        late_parent.header.finalize();
        let late_parent_hash = late_parent.header.hash;
        consensus.validate_and_insert_block(late_parent.to_immutable()).virtual_state_task.await.unwrap();

        let mut merger = consensus.build_block_with_parents(3.into(), vec![early_parent_hash, late_parent_hash]);
        merger.header.timestamp = config.genesis.timestamp + 110;
        merger.header.finalize();
        let merger_hash = merger.header.hash;
        consensus.validate_and_insert_block(merger.to_immutable()).virtual_state_task.await.unwrap();

        let ordered = topologically_ordered_hashes(&consensus, config.genesis.hash);
        let positions: std::collections::HashMap<_, _> =
            ordered.iter().copied().enumerate().map(|(index, hash)| (hash, index)).collect();

        assert!(positions[&early_parent_hash] < positions[&merger_hash]);
        assert!(positions[&late_parent_hash] < positions[&merger_hash]);

        consensus.shutdown(wait_handles);
    }
}
