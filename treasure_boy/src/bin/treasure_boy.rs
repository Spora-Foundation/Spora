use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use clap::{Arg, ArgAction, Command};
use secp256k1::Keypair;
use tondi_addresses::Address;
use tondi_core::{error, info, time::unix_now, tondid_env::version};
use tondi_grpc_client::GrpcClient;
use tondi_notify::subscription::context::SubscriptionContext;
use tondi_rpc_core::notify::mode::NotificationMode;

use std::fs;
use treasure_boy::{
    ask_batch_count, batch_airdrop, load_addresses_from_file, single_airdrop, tlc_airdrop, AddressDistributionTracker, Config,
    NetworkType, RandGenWallet, Stats, TlcAirdropConfig, TxsFeeConfig, ADDRESS_VERSION, DEFAULT_SEND_AMOUNT,
};

fn generate_addresses(count: u32, output_file: Option<String>, network: NetworkType) -> Result<(), Box<dyn std::error::Error>> {
    let prefix = network.address_prefix();
    let mut wallets = Vec::with_capacity(count as usize);
    for _ in 0..count {
        wallets.push(RandGenWallet::gen(prefix)?);
    }

    let content = serde_json::to_string_pretty(&wallets)?;
    match output_file {
        Some(file_path) => {
            fs::write(&file_path, content)?;
            let addresses = wallets.into_iter().map(|w| w.address).collect::<Vec<_>>();
            fs::write(format!("{file_path}.addresses"), addresses.join("\n"))?;
            info!("Generated {} addresses and saved to: {}", count, file_path);
        }
        None => {
            println!("Generated {} addresses:", count);
            println!("{}", content);
        }
    }

    Ok(())
}

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
        .arg(
            Arg::new("address-file")
                .long("address-file")
                .short('F')
                .value_name("file")
                .help("file containing addresses for batch airdrop (one address per line)"),
        )
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
        .arg(
            Arg::new("amount")
                .long("amount")
                .short('A')
                .value_name("amount")
                .default_value(format!("{}", DEFAULT_SEND_AMOUNT))
                .value_parser(clap::value_parser!(u64))
                .help("Amount to send per address in SAU (Smallest Atomic Unit)"),
        )
        .arg(
            Arg::new("generate-addresses")
                .long("generate-addresses")
                .short('g')
                .value_name("count")
                .value_parser(clap::value_parser!(u32))
                .help("Generate specified number of random addresses and save to file"),
        )
        .arg(
            Arg::new("output-file")
                .long("output-file")
                .short('O')
                .value_name("file")
                .help("Output file for generated addresses (used with --generate-addresses)"),
        )
        .arg(
            Arg::new("network")
                .long("network")
                .short('n')
                .value_name("network")
                .default_value("testnet")
                .value_parser(["mainnet", "testnet", "devnet"])
                .help("Network type: mainnet, testnet, or devnet"),
        )
        .arg(Arg::new("tlc-mode").long("tlc-mode").action(ArgAction::SetTrue).help("Enable Time Locked Contract (TLC) airdrop mode"))
        .arg(
            Arg::new("lock-time")
                .long("lock-time")
                .value_name("lock-time")
                .value_parser(clap::value_parser!(u64))
                .help("Lock time for TLC (Unix timestamp or block height)"),
        )
        .arg(
            Arg::new("lock-time-type")
                .long("lock-time-type")
                .value_name("type")
                .value_parser(["timestamp", "block"])
                .default_value("timestamp")
                .help("Lock time type: timestamp or block height"),
        )
        .arg(
            Arg::new("htlc-secret")
                .long("htlc-secret")
                .value_name("secret")
                .help("Secret for HTLC (optional, creates simple time lock if not provided)"),
        )
        .arg(
            Arg::new("recipient-pubkey")
                .long("recipient-pubkey")
                .value_name("pubkey")
                .help("Recipient's public key for HTLC (32 bytes hex)"),
        )
        .arg(Arg::new("sender-pubkey").long("sender-pubkey").value_name("pubkey").help("Sender's public key for HTLC (32 bytes hex)"))
}

fn parse_args() -> Config {
    let m = cli().get_matches();

    // Parse network type
    let network_str = m.get_one::<String>("network").unwrap();
    let network = match network_str.as_str() {
        "mainnet" => NetworkType::Mainnet,
        "testnet" => NetworkType::Testnet,
        "devnet" => NetworkType::Devnet,
        _ => NetworkType::Testnet, // Default fallback
    };

    // Parse TLC configuration
    let tlc_mode = m.get_one::<bool>("tlc-mode").cloned().unwrap_or(false);
    let tlc_config = if tlc_mode {
        let lock_time = m.get_one::<u64>("lock-time").cloned().unwrap_or(0);
        let lock_time_type_str = m.get_one::<String>("lock-time-type").unwrap();
        let is_timestamp = lock_time_type_str == "timestamp";

        let secret = m.get_one::<String>("htlc-secret").map(|s| s.as_bytes().to_vec());
        let recipient_pubkey = m.get_one::<String>("recipient-pubkey").and_then(|s| {
            let mut bytes = [0u8; 32];
            if faster_hex::hex_decode(s.as_bytes(), &mut bytes).is_ok() {
                Some(bytes)
            } else {
                None
            }
        });
        let sender_pubkey = m.get_one::<String>("sender-pubkey").and_then(|s| {
            let mut bytes = [0u8; 32];
            if faster_hex::hex_decode(s.as_bytes(), &mut bytes).is_ok() {
                Some(bytes)
            } else {
                None
            }
        });

        Some(TlcAirdropConfig { lock_time, is_timestamp, secret, recipient_pubkey, sender_pubkey })
    } else {
        None
    };

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
        generate_addresses: m.get_one::<u32>("generate-addresses").cloned(),
        output_file: m.get_one::<String>("output-file").cloned(),
        network,
        send_amount: m.get_one::<u64>("amount").cloned().unwrap_or(DEFAULT_SEND_AMOUNT),
        tlc_mode,
        tlc_config,
    }
}

#[tokio::main]
async fn main() {
    tondi_core::log::init_logger(None, "");
    let args = parse_args();

    // If address generation mode is specified, generate addresses and exit
    if let Some(count) = args.generate_addresses {
        match generate_addresses(count, args.output_file, args.network.clone()) {
            Ok(_) => return,
            Err(e) => {
                eprintln!("Error generating addresses: {}", e);
                std::process::exit(1);
            }
        }
    }

    // Check private key, error if no private key provided
    let schnorr_key = if let Some(private_key_hex) = args.private_key {
        let mut private_key_bytes = [0u8; 32];
        if let Err(e) = faster_hex::hex_decode(private_key_hex.as_bytes(), &mut private_key_bytes) {
            eprintln!("Error: Invalid hex format for private key: {}", e);
            std::process::exit(1);
        }
        match Keypair::from_seckey_slice(secp256k1::SECP256K1, &private_key_bytes) {
            Ok(keypair) => keypair,
            Err(e) => {
                eprintln!("Error: Invalid private key: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        eprintln!("Error: --private-key is required for transaction operations");
        eprintln!("Use --generate-addresses to generate random addresses");
        std::process::exit(1);
    };

    let tondi_addr = Address::new(args.network.address_prefix(), ADDRESS_VERSION, &schnorr_key.x_only_public_key().0.serialize());

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
        match Address::try_from(addr_str.clone()) {
            Ok(addr) => vec![addr],
            Err(e) => {
                eprintln!("Error: Invalid address '{}': {}", addr_str, e);
                std::process::exit(1);
            }
        }
    } else {
        // If no target addresses specified, ask user to generate addresses
        println!("No target addresses specified.");
        println!("Options:");
        println!("  1. Generate addresses for batch airdrop");
        println!("  2. Use current address for single transaction");
        println!("  3. Exit");
        print!("Choose option (1-3): ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        let choice = input.trim();

        match choice {
            "1" => {
                match ask_batch_count() {
                    Ok(count) => {
                        let temp_file = format!("temp_addresses_{}.txt", std::process::id());
                        if let Err(e) = generate_addresses(count, Some(temp_file.clone()), args.network.clone()) {
                            eprintln!("Error generating addresses: {}", e);
                            return;
                        }
                        match load_addresses_from_file(&temp_file) {
                            Ok(addresses) => {
                                std::fs::remove_file(&temp_file).ok(); // Clean up temp file
                                addresses
                            }
                            Err(e) => {
                                eprintln!("Error loading generated addresses: {}", e);
                                std::fs::remove_file(&temp_file).ok(); // Clean up temp file
                                return;
                            }
                        }
                    }
                    Err(_) => {
                        println!("Operation cancelled.");
                        return;
                    }
                }
            }
            "2" => {
                vec![tondi_addr.clone()]
            }
            "3" => {
                println!("Exiting...");
                return;
            }
            _ => {
                println!("Invalid choice. Using current address for single transaction.");
                vec![tondi_addr.clone()]
            }
        }
    };

    let fee_config = TxsFeeConfig { priority_fee: args.priority_fee, randomize_fee: args.randomize_fee };

    // Create address distribution tracker
    let _address_tracker = AddressDistributionTracker::new(target_addresses.clone());

    rayon::ThreadPoolBuilder::new().num_threads(args.threads as usize).build_global().unwrap();

    // Display configuration information (before connecting RPC)
    let mut log_message = format!(
        "Using Treasure Boy with:\n\
        \tprivate key: {}\n\
        \tfrom address: {}\n\
        \trpc server: {}",
        schnorr_key.display_secret(),
        String::from(&tondi_addr),
        args.rpc_server
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
    if args.tlc_mode {
        if let Some(tlc_config) = &args.tlc_config {
            log_message.push_str(&format!(
                "\n\tTLC mode: enabled\n\tlock time: {} ({})",
                tlc_config.lock_time,
                if tlc_config.is_timestamp { "timestamp" } else { "block height" }
            ));
            if tlc_config.secret.is_some() {
                log_message.push_str("\n\tHTLC: enabled with secret");
            } else {
                log_message.push_str("\n\tTLC: simple time lock");
            }
        }
    }
    info!("{}", log_message);

    // Only connect RPC server if private key is specified
    let _stats = Arc::new(Mutex::new(Stats { num_txs: 0, since: unix_now(), num_utxos: 0, utxos_amount: 0, num_outs: 0 }));
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

    // Determine whether to use TLC airdrop, single airdrop, or batch airdrop
    if args.tlc_mode {
        // TLC airdrop
        if let Some(tlc_config) = &args.tlc_config {
            info!("Performing TLC airdrop to {} addresses", target_addresses.len());

            match tlc_airdrop(
                schnorr_key,
                target_addresses,
                args.send_amount,
                args.outputs_per_tx,
                &rpc_client,
                &fee_config,
                tlc_config,
                args.threads as usize,
                args.network.clone(),
            )
            .await
            {
                Ok(txs) => {
                    info!("TLC airdrop completed successfully");
                    info!("Sent {} TLC transactions", txs.len());

                    // Display first few transaction IDs
                    for (i, tx) in txs.iter().take(5).enumerate() {
                        info!("TLC Transaction {}: {:?}", i + 1, tx.id());
                    }
                    if txs.len() > 5 {
                        info!("... and {} more TLC transactions", txs.len() - 5);
                    }
                }
                Err(e) => {
                    error!("TLC airdrop failed: {}", e);
                    std::process::exit(1);
                }
            }
        } else {
            error!("TLC mode enabled but no TLC configuration provided");
            std::process::exit(1);
        }
    } else if target_addresses.len() == 1 {
        // Single airdrop
        info!("Performing single airdrop to: {}", String::from(&target_addresses[0]));

        match single_airdrop(
            schnorr_key,
            target_addresses[0].clone(),
            args.send_amount,
            &rpc_client,
            &fee_config,
            args.network.clone(),
        )
        .await
        {
            Ok(tx) => {
                info!("Single airdrop completed successfully");
                info!("Transaction ID: {:?}", tx.id());
            }
            Err(e) => {
                error!("Single airdrop failed: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        // Batch airdrop
        info!("Performing batch airdrop to {} addresses", target_addresses.len());

        match batch_airdrop(
            schnorr_key,
            target_addresses,
            args.send_amount,
            args.outputs_per_tx,
            &rpc_client,
            &fee_config,
            args.threads as usize,
            args.network.clone(),
        )
        .await
        {
            Ok(txs) => {
                info!("Batch airdrop completed successfully");
                info!("Sent {} transactions", txs.len());

                // Display first few transaction IDs
                for (i, tx) in txs.iter().take(5).enumerate() {
                    info!("Transaction {}: {:?}", i + 1, tx.id());
                }
                if txs.len() > 5 {
                    info!("... and {} more transactions", txs.len() - 5);
                }
            }
            Err(e) => {
                error!("Batch airdrop failed: {}", e);
                std::process::exit(1);
            }
        }
    }
}
