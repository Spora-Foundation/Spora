use std::{collections::HashMap, sync::Arc, time::Duration};

use clap::{Arg, ArgAction, Command};
use parking_lot::Mutex;
use rayon::prelude::*;
use secp256k1::{
    rand::thread_rng,
    Keypair,
};
use tokio::time::{interval, Instant, MissedTickBehavior};
use tondi_addresses::Address;
use tondi_consensus_core::{
    config::params::TESTNET_PARAMS,
    tx::TransactionOutpoint,
};
use tondi_core::{info, time::unix_now, tondid_env::version, warn};
use tondi_grpc_client::{ClientPool, GrpcClient};
use tondi_notify::subscription::context::SubscriptionContext;
use tondi_rpc_core::{api::rpc::RpcApi, notify::mode::NotificationMode};

use treasure_boy::{
    load_addresses_from_file, new_rpc_client, pause_if_mempool_is_full, refresh_utxos,
    should_maximize_inputs, clean_old_pending_outpoints, maybe_send_tx,
    AddressDistributionTracker, Config, Stats, TxsFeeConfig, ClientPoolArg,
    MILLIS_PER_TICK, ADDRESS_PREFIX, ADDRESS_VERSION,
};

fn cli() -> Command {
    Command::new("treasure_boy")
        .about(format!("{} (treasure_boy) v{}", env!("CARGO_PKG_DESCRIPTION"), version()))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(Arg::new("private-key").long("private-key").short('k').value_name("private-key").help("Private key in hex format"))
        .arg(
            Arg::new("tps")
                .long("tps")
                .short('t')
                .value_name("tps")
                .default_value("1")
                .value_parser(clap::value_parser!(u64))
                .help("Transactions per second"),
        )
        .arg(
            Arg::new("rpcserver")
                .long("rpcserver")
                .short('s')
                .value_name("rpcserver")
                .default_value("localhost:16210")
                .help("RPC server"),
        )
        .arg(
            Arg::new("threads")
                .long("threads")
                .default_value("2")
                .value_parser(clap::value_parser!(u8))
                .help("The number of threads to use for TX generation. Set to 0 to use 1 thread per core. Default is 2."),
        )
        .arg(Arg::new("unleashed").long("unleashed").action(ArgAction::SetTrue).hide(true).help("Allow higher TPS"))
        .arg(Arg::new("addr").long("to-addr").short('a').value_name("addr").help("address to send to"))
        .arg(Arg::new("address-file").long("address-file").short('F').value_name("file").help("file containing addresses for batch airdrop (one address per line)"))
        .arg(
            Arg::new("outputs-per-tx")
                .long("outputs-per-tx")
                .short('o')
                .value_name("count")
                .default_value("1")
                .value_parser(clap::value_parser!(u64))
                .help("Number of outputs per transaction (recipients per tx)"),
        )
        .arg(
            Arg::new("priority-fee")
                .long("priority-fee")
                .short('f')
                .value_name("priority-fee")
                .default_value("0")
                .value_parser(clap::value_parser!(u64))
                .help("Transaction priority fee"),
        )
        .arg(
            Arg::new("randomize-fee")
                .long("randomize-fee")
                .short('r')
                .value_name("randomize-fee")
                .action(ArgAction::SetTrue)
                .default_value("false")
                .help("Randomize transaction priority fee"),
        )
}

fn parse_args() -> Config {
    let m = cli().get_matches();
    Config {
        private_key: m.get_one::<String>("private-key").cloned(),
        tps: m.get_one::<u64>("tps").cloned().unwrap(),
        rpc_server: m.get_one::<String>("rpcserver").cloned().unwrap_or("localhost:16210".to_owned()),
        threads: m.get_one::<u8>("threads").cloned().unwrap(),
        unleashed: m.get_one::<bool>("unleashed").cloned().unwrap_or(false),
        addr: m.get_one::<String>("addr").cloned(),
        address_file: m.get_one::<String>("address-file").cloned(),
        outputs_per_tx: m.get_one::<u64>("outputs-per-tx").cloned().unwrap_or(1),
        priority_fee: m.get_one::<u64>("priority-fee").cloned().unwrap_or(0),
        randomize_fee: m.get_one::<bool>("randomize-fee").cloned().unwrap_or(false),
    }
}

#[tokio::main]
async fn main() {
    tondi_core::log::init_logger(None, "");
    let args = parse_args();
    let stats = Arc::new(Mutex::new(Stats { num_txs: 0, since: unix_now(), num_utxos: 0, utxos_amount: 0, num_outs: 0 }));
    let subscription_context = SubscriptionContext::new();
    let rpc_client = GrpcClient::connect_with_args(
        NotificationMode::Direct,
        format!("grpc://{}", args.rpc_server),
        Some(subscription_context.clone()),
        true,
        None,
        false,
        Some(500_000),
        Default::default(),
    )
    .await
    .expect("Critical error: failed to connect to the RPC server.");

    info!("Connected to RPC");

    let mut pending: HashMap<TransactionOutpoint, Instant> = HashMap::new();

    let schnorr_key = if let Some(private_key_hex) = args.private_key {
        let mut private_key_bytes = [0u8; 32];
        faster_hex::hex_decode(private_key_hex.as_bytes(), &mut private_key_bytes).unwrap();
        Keypair::from_seckey_slice(secp256k1::SECP256K1, &private_key_bytes).unwrap()
    } else {
        let (sk, pk) = &secp256k1::generate_keypair(&mut thread_rng());
        let tondi_addr = Address::new(ADDRESS_PREFIX, ADDRESS_VERSION, &pk.x_only_public_key().0.serialize());
        info!(
            "Generated private key {} and address {}. Send some funds to this address and rerun treasure_boy with `--private-key {}`",
            sk.display_secret(),
            String::from(&tondi_addr),
            sk.display_secret()
        );
        return;
    };

    let tondi_addr = Address::new(ADDRESS_PREFIX, ADDRESS_VERSION, &schnorr_key.x_only_public_key().0.serialize());

    // Load addresses for batch airdrop
    let target_addresses = if let Some(address_file) = &args.address_file {
        match load_addresses_from_file(address_file) {
            Ok(addresses) => {
                info!("Loaded {} addresses from file: {}", addresses.len(), address_file);
                addresses
            }
            Err(e) => {
                panic!("Failed to load addresses from file '{}': {}", address_file, e);
            }
        }
    } else if let Some(addr_str) = &args.addr {
        vec![Address::try_from(addr_str.clone()).unwrap()]
    } else {
        vec![tondi_addr.clone()]
    };

    let fee_config = TxsFeeConfig { priority_fee: args.priority_fee, randomize_fee: args.randomize_fee };
    
    // 创建地址分发跟踪器
    let mut address_tracker = AddressDistributionTracker::new(target_addresses.clone());

    rayon::ThreadPoolBuilder::new().num_threads(args.threads as usize).build_global().unwrap();

    let mut log_message = format!(
        "Using Treasure Boy with:\n\
        \tprivate key: {}\n\
        \tfrom address: {}",
        schnorr_key.display_secret(),
        String::from(&tondi_addr)
    );
    if args.address_file.is_some() {
        log_message.push_str(&format!("\n\tbatch airdrop to {} addresses", target_addresses.len()));
        log_message.push_str(&format!("\n\toutputs per tx: {}", args.outputs_per_tx));
    } else if args.addr.is_some() {
        log_message.push_str(&format!("\n\tto address: {}", String::from(&target_addresses[0])));
    }
    if args.priority_fee != 0 {
        log_message.push_str(&format!(
            "\n\tpriority fee: {} SOMPS {}",
            fee_config.priority_fee,
            if fee_config.randomize_fee { "[randomize]" } else { "" }
        ));
    }
    info!("{}", log_message);

    let info = rpc_client.get_block_dag_info().await.expect("Failed to get block dag info.");

    let coinbase_maturity = match info.network.suffix {
        Some(11) => panic!("TN11 is not supported on this version"),
        None | Some(_) => TESTNET_PARAMS.coinbase_maturity().upper_bound(),
    };
    info!(
        "Node block-DAG info: \n\tNetwork: {}, \n\tBlock count: {}, \n\tHeader count: {}, \n\tDifficulty: {},
\tMedian time: {}, \n\tDAA score: {}, \n\tPruning point: {}, \n\tTips: {}, \n\t{} virtual parents: ...{}, \n\tCoinbase maturity: {}",
        info.network,
        info.block_count,
        info.header_count,
        info.difficulty,
        info.past_median_time,
        info.virtual_daa_score,
        info.pruning_point_hash,
        info.tip_hashes.len(),
        info.virtual_parent_hashes.len(),
        info.virtual_parent_hashes.last().unwrap(),
        coinbase_maturity,
    );

    const CLIENT_POOL_SIZE: usize = 8;
    let mut rpc_clients = Vec::with_capacity(CLIENT_POOL_SIZE);
    for _ in 0..CLIENT_POOL_SIZE {
        rpc_clients.push(Arc::new(new_rpc_client(&subscription_context, &args.rpc_server).await));
    }

    let submit_tx_pool = ClientPool::new(rpc_clients, 1000);
    let _ = submit_tx_pool.start(|c, arg: ClientPoolArg| async move {
        let ClientPoolArg { tx, stats, selected_utxos_len, selected_utxos_amount, pending_len, utxos_len } = arg;
        match c.submit_transaction(tx.as_ref().into(), false).await {
            Ok(_) => {
                let mut stats = stats.lock();
                stats.num_txs += 1;
                stats.num_utxos += selected_utxos_len;
                stats.utxos_amount += selected_utxos_amount;
                stats.num_outs += tx.outputs.len();
                let now = unix_now();
                let time_past = now - stats.since;
                if time_past > 10_000 {
                    info!(
                        "Tx rate: {:.1}/sec, avg UTXO amount: {}, avg UTXOs per tx: {}, avg outs per tx: {}, estimated available UTXOs: {}",
                        1000f64 * (stats.num_txs as f64) / (time_past as f64),
                        stats.utxos_amount / stats.num_utxos as u64,
                        stats.num_utxos / stats.num_txs,
                        stats.num_outs / stats.num_txs,
                        utxos_len.saturating_sub(pending_len),
                    );
                    stats.since = now;
                    stats.num_txs = 0;
                    stats.num_utxos = 0;
                    stats.utxos_amount = 0;
                    stats.num_outs = 0;
                }
            }
            Err(e) => {
                let mut tx = tx;
                tx.finalize();
                warn!("RPC error when submitting {}: {}", tx.id(), e);
            }
        }
        false
    });
    let tx_sender = submit_tx_pool.sender();

    let target_tps = args.tps.min(if args.unleashed { u64::MAX } else { 100 });
    let should_tick_per_second = target_tps * MILLIS_PER_TICK / 1000 == 0;
    let avg_txs_per_tick = if should_tick_per_second { target_tps } else { target_tps * MILLIS_PER_TICK / 1000 };
    let mut utxos = refresh_utxos(&rpc_client, tondi_addr.clone(), &mut pending, coinbase_maturity).await;
    let mut ticker = interval(Duration::from_millis(if should_tick_per_second { 1000 } else { MILLIS_PER_TICK }));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let mut maximize_inputs = false;
    let mut last_refresh = unix_now();
    // This allows us to keep track of the UTXOs we already tried to use for this period
    // until the UTXOs are refreshed. At that point, this will be reset as well.
    let mut next_available_utxo_index = 0;
    // Tracker so we can try to send as close as possible to the target TPS
    let mut remaining_txs_in_interval = target_tps;

    loop {
        ticker.tick().await;
        maximize_inputs = should_maximize_inputs(maximize_inputs, &utxos, &pending);
        let txs_to_send = if remaining_txs_in_interval > avg_txs_per_tick * 2 {
            remaining_txs_in_interval -= avg_txs_per_tick;
            avg_txs_per_tick
        } else {
            let count = remaining_txs_in_interval;
            remaining_txs_in_interval = target_tps;
            count
        };

        let now = unix_now();
        let has_funds = maybe_send_tx(
            txs_to_send,
            &tx_sender,
            &mut address_tracker,
            &mut utxos,
            &mut pending,
            schnorr_key,
            stats.clone(),
            maximize_inputs,
            &mut next_available_utxo_index,
            &fee_config,
            args.outputs_per_tx,
        )
        .await;
        if !has_funds {
            info!("Has not enough funds");
        }
        if !has_funds || now - last_refresh > 60_000 {
            info!("Refetching UTXO set");
            tokio::time::sleep(Duration::from_millis(100)).await; // We don't want this operation to be too frequent since its heavy on the node, so we wait some time before executing it.
            utxos = refresh_utxos(&rpc_client, tondi_addr.clone(), &mut pending, coinbase_maturity).await;
            last_refresh = unix_now();
            next_available_utxo_index = 0;
            pause_if_mempool_is_full(&rpc_client).await;
        }
        clean_old_pending_outpoints(&mut pending);
    }
}
