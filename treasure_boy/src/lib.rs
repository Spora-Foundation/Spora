use std::{
    collections::HashMap,
    fs,
    io::{BufRead, Write},
    time::Duration,
};

use itertools::Itertools;
use secp256k1::{
    rand::{thread_rng, Rng},
    Keypair,
};
use tokio::time::Instant;
use tondi_addresses::{Address, Prefix, Version};
use tondi_consensus_core::{
    constants::{SAU_PER_TONDI, TX_VERSION},
    sign::sign,
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{MutableTransaction, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry},
};
use tondi_core::{info, warn};
use tondi_grpc_client::GrpcClient;
use tondi_rpc_core::{api::rpc::RpcApi, RpcUtxoEntry};
use tondi_txscript::pay_to_address_script;

/// Default amount to send per address in SAU (Smallest Atomic Unit)
pub const DEFAULT_SEND_AMOUNT: u64 = 10 * SAU_PER_TONDI;
/// Base fee rate for transaction fees
pub const FEE_RATE: u64 = 10;
/// Milliseconds per tick for timing operations
pub const MILLIS_PER_TICK: u64 = 10;
/// Address version for generated addresses
pub const ADDRESS_VERSION: Version = Version::PubKey;

/// Statistics tracking for transaction operations
#[derive(Debug, Clone)]
pub struct Stats {
    /// Number of transactions processed
    pub num_txs: usize,
    /// Number of UTXOs available
    pub num_utxos: usize,
    /// Total amount of UTXOs in SAU
    pub utxos_amount: u64,
    /// Number of outputs generated
    pub num_outs: usize,
    /// Timestamp when stats were created
    pub since: u64,
}

/// Tracks address distribution for fair airdrop operations
#[derive(Debug, Clone)]
pub struct AddressDistributionTracker {
    addresses: Vec<Address>,
    distribution_counts: Vec<usize>,
    current_index: usize,
}

impl AddressDistributionTracker {
    pub fn new(addresses: Vec<Address>) -> Self {
        let distribution_counts = vec![0; addresses.len()];
        Self { addresses, distribution_counts, current_index: 0 }
    }

    pub fn get_next_addresses(&mut self, count: usize) -> Vec<&Address> {
        let mut selected_addresses = Vec::new();

        for _ in 0..count {
            if self.addresses.is_empty() {
                break;
            }

            // Select address at current index
            let addr = &self.addresses[self.current_index];
            selected_addresses.push(addr);

            // Increment distribution count
            self.distribution_counts[self.current_index] += 1;

            // Move to next address (circular)
            self.current_index = (self.current_index + 1) % self.addresses.len();
        }

        selected_addresses
    }

    /// Batch get addresses, optimize performance for large address pools
    pub fn get_next_addresses_batch(&mut self, batch_size: usize, addresses_per_tx: usize) -> Vec<Vec<&Address>> {
        let mut batches = Vec::new();

        for _ in 0..batch_size {
            let mut tx_addresses = Vec::new();

            for _ in 0..addresses_per_tx {
                if self.addresses.is_empty() {
                    break;
                }

                let addr = &self.addresses[self.current_index];
                tx_addresses.push(addr);

                // Increase distribution count
                self.distribution_counts[self.current_index] += 1;

                // Move to next address (loop)
                self.current_index = (self.current_index + 1) % self.addresses.len();
            }

            if !tx_addresses.is_empty() {
                batches.push(tx_addresses);
            }
        }

        batches
    }

    pub fn get_random_addresses(&mut self, count: usize) -> Vec<&Address> {
        let mut selected_addresses = Vec::new();

        if self.addresses.is_empty() {
            return selected_addresses;
        }

        // Create RNG once for better performance
        let mut rng = thread_rng();

        for _ in 0..count {
            // Randomly select an address
            let index = rng.gen_range(0..self.addresses.len());
            let addr = &self.addresses[index];
            selected_addresses.push(addr);

            // Increase distribution count
            self.distribution_counts[index] += 1;
        }

        selected_addresses
    }

    pub fn get_distribution_stats(&self) -> String {
        if self.addresses.is_empty() {
            return "No addresses".to_string();
        }

        let min_count = self.distribution_counts.iter().min().unwrap_or(&0);
        let max_count = self.distribution_counts.iter().max().unwrap_or(&0);
        let total_distributions: usize = self.distribution_counts.iter().sum();

        format!(
            "Distribution: min={}, max={}, total={}, avg={:.1}",
            min_count,
            max_count,
            total_distributions,
            total_distributions as f64 / self.addresses.len() as f64
        )
    }
}

/// Network type for treasure_boy operations
#[derive(Debug, Clone, PartialEq)]
pub enum NetworkType {
    /// Mainnet network
    Mainnet,
    /// Testnet network (default)
    Testnet,
    /// Development network
    Devnet,
}

impl Default for NetworkType {
    fn default() -> Self {
        NetworkType::Testnet
    }
}

impl NetworkType {
    /// Get the address prefix for this network type
    pub fn address_prefix(&self) -> Prefix {
        match self {
            NetworkType::Mainnet => Prefix::Mainnet,
            NetworkType::Testnet => Prefix::Testnet,
            NetworkType::Devnet => Prefix::Devnet,
        }
    }
    
    /// Get the default RPC port for this network type
    pub fn default_rpc_port(&self) -> u16 {
        match self {
            NetworkType::Mainnet => 16110,
            NetworkType::Testnet => 16210,
            NetworkType::Devnet => 16210,
        }
    }
}

/// Configuration for treasure_boy operations
#[derive(Debug, Clone)]
pub struct Config {
    /// Private key in hex format for transaction signing
    pub private_key: Option<String>,
    /// Target transactions per second
    pub tps: u64,
    /// RPC server address
    pub rpc_server: String,
    /// Number of threads for parallel processing
    pub threads: u8,
    /// Allow higher TPS (unleashed mode)
    pub unleashed: bool,
    /// Single target address for transactions
    pub addr: Option<String>,
    /// File containing addresses for batch airdrop
    pub address_file: Option<String>,
    /// Number of outputs per transaction
    pub outputs_per_tx: u64,
    /// Priority fee for transactions
    pub priority_fee: u64,
    /// Randomize priority fee
    pub randomize_fee: bool,
    /// Number of addresses to generate
    pub generate_addresses: Option<u32>,
    /// Output file for generated addresses
    pub output_file: Option<String>,
    /// Network type for address generation and operations
    pub network: NetworkType,
}

/// Configuration for transaction fees
#[derive(Debug, Clone)]
pub struct TxsFeeConfig {
    /// Priority fee amount
    pub priority_fee: u64,
    /// Whether to randomize the priority fee
    pub randomize_fee: bool,
}

/// Load addresses from a text file, one address per line.
/// Supports comments (lines starting with #) and empty lines.
/// Invalid addresses are skipped with a warning.
pub fn load_addresses_from_file(file_path: &str) -> Result<Vec<Address>, Box<dyn std::error::Error>> {
    let file = fs::File::open(file_path)?;
    let reader = std::io::BufReader::new(file);
    let mut addresses = Vec::new();

    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            match Address::try_from(trimmed.to_string()) {
                Ok(addr) => addresses.push(addr),
                Err(e) => {
                    warn!("Invalid address '{}': {}", trimmed, e);
                }
            }
        }
    }

    Ok(addresses)
}

/// Perform a single airdrop transaction to one address.
///
/// # Arguments
/// * `schnorr_key` - The private key for signing the transaction
/// * `target_address` - The address to send funds to
/// * `amount` - The amount to send in SAU
/// * `rpc_client` - The RPC client for blockchain interaction
/// * `fee_config` - Configuration for transaction fees
///
/// # Returns
/// Returns the signed transaction on success, or an error on failure.
pub async fn single_airdrop(
    schnorr_key: Keypair,
    target_address: Address,
    amount: u64,
    rpc_client: &GrpcClient,
    fee_config: &TxsFeeConfig,
    network: NetworkType,
) -> Result<Transaction, Box<dyn std::error::Error>> {
    info!("Starting single airdrop to: {}", String::from(&target_address));

    // Get UTXOs
    let from_address = Address::new(network.address_prefix(), ADDRESS_VERSION, &schnorr_key.x_only_public_key().0.serialize());
    let rpc_utxos = rpc_client.get_utxos_by_addresses(vec![from_address.clone()]).await?;

    if rpc_utxos.is_empty() {
        return Err("No UTXOs available for sending".into());
    }

    // Convert UTXOs format
    let utxos: Vec<(TransactionOutpoint, UtxoEntry)> =
        rpc_utxos.into_iter().map(|entry| (entry.outpoint.into(), entry.utxo_entry.into())).collect();

    // Select UTXOs
    let mut next_available_utxo_index = 0;
    let (selected_utxos, selected_amount) = select_utxos(
        &utxos,
        amount,
        1, // Single airdrop has only one output
        false,
        &mut next_available_utxo_index,
        fee_config,
    );

    if selected_utxos.is_empty() {
        return Err("Insufficient funds for transaction".into());
    }

    // Generate transaction
    let tx = generate_multi_output_tx(schnorr_key, &selected_utxos, selected_amount, &[&target_address]);

    // Send transaction
    rpc_client.submit_transaction((&tx).into(), false).await?;

    info!("Single airdrop completed successfully");
    Ok(tx)
}

/// Perform batch airdrop transactions to multiple addresses.
///
/// # Arguments
/// * `schnorr_key` - The private key for signing transactions
/// * `target_addresses` - List of addresses to send funds to
/// * `amount_per_address` - Amount to send to each address in SAU
/// * `outputs_per_tx` - Number of outputs per transaction
/// * `rpc_client` - The RPC client for blockchain interaction
/// * `fee_config` - Configuration for transaction fees
/// * `threads` - Number of threads for parallel processing
///
/// # Returns
/// Returns a vector of successfully sent transactions, or an error on failure.
pub async fn batch_airdrop(
    schnorr_key: Keypair,
    target_addresses: Vec<Address>,
    amount_per_address: u64,
    outputs_per_tx: u64,
    rpc_client: &GrpcClient,
    fee_config: &TxsFeeConfig,
    threads: usize,
    network: NetworkType,
) -> Result<Vec<Transaction>, Box<dyn std::error::Error>> {
    info!("Starting batch airdrop to {} addresses", target_addresses.len());

    // Create address distribution tracker
    let mut address_tracker = AddressDistributionTracker::new(target_addresses);

    // Get UTXOs
    let from_address = Address::new(network.address_prefix(), ADDRESS_VERSION, &schnorr_key.x_only_public_key().0.serialize());
    let rpc_utxos = rpc_client.get_utxos_by_addresses(vec![from_address.clone()]).await?;

    // Convert UTXOs format
    let utxos: Vec<(TransactionOutpoint, UtxoEntry)> =
        rpc_utxos.into_iter().map(|entry| (entry.outpoint.into(), entry.utxo_entry.into())).collect();

    if utxos.is_empty() {
        return Err("No UTXOs available for sending".into());
    }

    // Calculate the number of transactions to send
    let total_addresses = address_tracker.addresses.len();
    let txs_needed = (total_addresses as f64 / outputs_per_tx as f64).ceil() as u64;

    info!("Need to send {} transactions with {} outputs each", txs_needed, outputs_per_tx);

    let mut successful_txs = Vec::new();
    let mut pending: HashMap<TransactionOutpoint, Instant> = HashMap::new();
    let mut next_available_utxo_index = 0;

    // Set thread pool (ignore if already initialized)
    let _ = rayon::ThreadPoolBuilder::new().num_threads(threads).build_global();

    // Batch process transactions
    let batch_size = 10; // Process 10 transactions per batch
    for batch_start in (0..txs_needed).step_by(batch_size as usize) {
        let batch_end = (batch_start + batch_size).min(txs_needed);
        let batch_txs = batch_end - batch_start;

        info!("Processing batch: transactions {} to {}", batch_start + 1, batch_end);

        // Pre-batch allocate addresses
        let address_assignments = if address_tracker.addresses.len() == 1 {
            (0..batch_txs).map(|_| vec![&address_tracker.addresses[0]]).collect::<Vec<_>>()
        } else {
            address_tracker.get_next_addresses_batch(batch_txs as usize, outputs_per_tx as usize)
        };

        // Generate transactions sequentially to avoid mutable borrowing issues
        let mut txs = Vec::new();
        for (_, target_addresses) in (0..batch_txs as usize).zip(address_assignments.iter()) {
            let (selected_utxos, selected_amount) = select_utxos(
                &utxos,
                amount_per_address * outputs_per_tx,
                outputs_per_tx,
                false,
                &mut next_available_utxo_index,
                fee_config,
            );

            if !selected_utxos.is_empty() {
                let tx = generate_multi_output_tx(schnorr_key, &selected_utxos, selected_amount, target_addresses);
                txs.push(Some(tx));
            } else {
                txs.push(None);
            }
        }

        // Send transactions
        for tx_option in txs {
            if let Some(tx) = tx_option {
                match rpc_client.submit_transaction((&tx).into(), false).await {
                    Ok(_) => {
                        successful_txs.push(tx);
                        info!("Transaction submitted successfully");
                    }
                    Err(e) => {
                        warn!("Failed to submit transaction: {}", e);
                    }
                }
            }
        }

        // Clean up used UTXOs
        clean_old_pending_outpoints(&mut pending);
    }

    info!("Batch airdrop completed: {} transactions sent successfully", successful_txs.len());
    Ok(successful_txs)
}

/// Interactive ask for batch airdrop count
pub fn ask_batch_count() -> Result<u32, Box<dyn std::error::Error>> {
    println!("How many addresses do you want to generate for batch airdrop?");
    println!("Enter a number (or 'q' to quit):");

    loop {
        print!("> ");
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input.to_lowercase() == "q" {
            return Err("User cancelled".into());
        }

        match input.parse::<u32>() {
            Ok(count) => {
                if count == 0 {
                    println!("Please enter a number greater than 0.");
                    continue;
                }
                if count > 10000 {
                    println!("Warning: Generating {} addresses may take a while. Continue? (y/n)", count);
                    print!("> ");
                    std::io::stdout().flush()?;

                    let mut confirm = String::new();
                    std::io::stdin().read_line(&mut confirm)?;

                    if confirm.trim().to_lowercase() != "y" {
                        continue;
                    }
                }
                return Ok(count);
            }
            Err(_) => {
                println!("Please enter a valid number.");
                continue;
            }
        }
    }
}

pub fn required_fee(num_utxos: usize, num_outs: u64) -> u64 {
    FEE_RATE * estimated_mass(num_utxos, num_outs)
}

pub fn estimated_mass(num_utxos: usize, num_outs: u64) -> u64 {
    200 + 34 * num_outs + 1000 * (num_utxos as u64)
}

pub fn generate_tx(
    schnorr_key: Keypair,
    utxos: &[(TransactionOutpoint, UtxoEntry)],
    send_amount: u64,
    num_outs: u64,
    tondi_addr: &Address,
) -> Transaction {
    let script_public_key = pay_to_address_script(tondi_addr);
    let inputs = utxos
        .iter()
        .map(|(op, _)| TransactionInput { previous_outpoint: *op, signature_script: vec![], sequence: 0, sig_op_count: 1 })
        .collect_vec();

    let outputs = (0..num_outs)
        .map(|_| TransactionOutput { value: send_amount / num_outs, script_public_key: script_public_key.clone() })
        .collect_vec();
    let unsigned_tx = Transaction::new_non_finalized(TX_VERSION, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
    let signed_tx =
        sign(MutableTransaction::with_entries(unsigned_tx, utxos.iter().map(|(_, entry)| entry.clone()).collect_vec()), schnorr_key);
    let mut final_tx = signed_tx.tx;
    final_tx.finalize();
    final_tx
}

pub fn generate_multi_output_tx(
    schnorr_key: Keypair,
    utxos: &[(TransactionOutpoint, UtxoEntry)],
    send_amount: u64,
    target_addresses: &[&Address],
) -> Transaction {
    let inputs = utxos
        .iter()
        .map(|(op, _)| TransactionInput { previous_outpoint: *op, signature_script: vec![], sequence: 0, sig_op_count: 1 })
        .collect_vec();

    // Create an output for each target address
    let outputs = target_addresses
        .iter()
        .map(|addr| {
            let script_public_key = pay_to_address_script(addr);
            TransactionOutput { value: send_amount / target_addresses.len() as u64, script_public_key }
        })
        .collect_vec();

    let unsigned_tx = Transaction::new_non_finalized(TX_VERSION, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
    let signed_tx =
        sign(MutableTransaction::with_entries(unsigned_tx, utxos.iter().map(|(_, entry)| entry.clone()).collect_vec()), schnorr_key);
    let mut final_tx = signed_tx.tx;
    final_tx.finalize();
    final_tx
}

pub fn select_utxos(
    utxos: &[(TransactionOutpoint, UtxoEntry)],
    min_amount: u64,
    num_outs: u64,
    maximize_utxos: bool,
    next_available_utxo_index: &mut usize,
    fee_config: &TxsFeeConfig,
) -> (Vec<(TransactionOutpoint, UtxoEntry)>, u64) {
    const MAX_UTXOS: usize = 84;
    let mut selected_amount: u64 = 0;
    let mut selected = Vec::new();
    let mut rng = thread_rng();

    while *next_available_utxo_index < utxos.len() {
        let (outpoint, entry) = utxos[*next_available_utxo_index].clone();
        selected_amount += entry.amount;
        selected.push((outpoint, entry));

        let fee = required_fee(selected.len(), num_outs);
        let priority_fee = if fee_config.randomize_fee && fee_config.priority_fee > 0 {
            rng.gen_range(0..fee_config.priority_fee)
        } else {
            fee_config.priority_fee
        };

        *next_available_utxo_index += 1;

        if selected_amount >= min_amount + fee + priority_fee && (!maximize_utxos || selected.len() == MAX_UTXOS) {
            return (selected, selected_amount - fee - priority_fee);
        }

        if selected.len() > MAX_UTXOS {
            return (vec![], 0);
        }
    }

    (vec![], 0)
}

pub fn is_utxo_spendable(entry: &RpcUtxoEntry, virtual_daa_score: u64, coinbase_maturity: u64) -> bool {
    let needed_confs = if !entry.is_coinbase {
        10
    } else {
        coinbase_maturity * 2 // TODO: We should compare with sink blue score in the case of coinbase
    };
    entry.block_daa_score + needed_confs < virtual_daa_score
}

pub fn clean_old_pending_outpoints(pending: &mut HashMap<TransactionOutpoint, Instant>) {
    let now = Instant::now();
    pending.retain(|_, &mut time| now.duration_since(time) <= Duration::from_secs(3600));
}

#[cfg(test)]
mod tests {
    use super::*;
    use secp256k1::{SecretKey, SECP256K1};
    use std::str::FromStr;
    use tondi_bip32::{DerivationPath, ExtendedPrivateKey, Language, Mnemonic, WordCount};

    #[test]
    fn test_address_distribution_tracker_new() {
        let addresses = vec![
            Address::new(Prefix::Devnet, Version::PubKey, &[1; 32]),
            Address::new(Prefix::Devnet, Version::PubKey, &[2; 32]),
            Address::new(Prefix::Devnet, Version::PubKey, &[3; 32]),
        ];

        let tracker = AddressDistributionTracker::new(addresses.clone());

        assert_eq!(tracker.addresses.len(), 3);
        assert_eq!(tracker.distribution_counts.len(), 3);
        assert_eq!(tracker.current_index, 0);
        assert!(tracker.distribution_counts.iter().all(|&count| count == 0));
    }

    #[test]
    fn test_address_distribution_tracker_get_next_addresses() {
        let addresses = vec![
            Address::new(Prefix::Devnet, Version::PubKey, &[1; 32]),
            Address::new(Prefix::Devnet, Version::PubKey, &[2; 32]),
            Address::new(Prefix::Devnet, Version::PubKey, &[3; 32]),
        ];

        let mut tracker = AddressDistributionTracker::new(addresses.clone());

        // Test getting next addresses
        let selected = tracker.get_next_addresses(2);
        assert_eq!(selected.len(), 2);
        assert_eq!(tracker.current_index, 2);
        assert_eq!(tracker.distribution_counts[0], 1);
        assert_eq!(tracker.distribution_counts[1], 1);
        assert_eq!(tracker.distribution_counts[2], 0);

        // Test looping
        let selected = tracker.get_next_addresses(2);
        assert_eq!(selected.len(), 2);
        assert_eq!(tracker.current_index, 1); // 2 + 2 = 4, 4 % 3 = 1
        assert_eq!(tracker.distribution_counts[2], 1);
        assert_eq!(tracker.distribution_counts[0], 2);
    }

    #[test]
    fn test_address_distribution_tracker_empty() {
        let mut tracker = AddressDistributionTracker::new(vec![]);

        let selected = tracker.get_next_addresses(5);
        assert_eq!(selected.len(), 0);

        let stats = tracker.get_distribution_stats();
        assert_eq!(stats, "No addresses");
    }

    #[test]
    fn test_address_distribution_tracker_stats() {
        let addresses =
            vec![Address::new(Prefix::Devnet, Version::PubKey, &[1; 32]), Address::new(Prefix::Devnet, Version::PubKey, &[2; 32])];

        let mut tracker = AddressDistributionTracker::new(addresses);

        tracker.get_next_addresses(3); // Address 0: 2 times, Address 1: 1 time

        let stats = tracker.get_distribution_stats();
        assert!(stats.contains("min=1"));
        assert!(stats.contains("max=2"));
        assert!(stats.contains("total=3"));
        assert!(stats.contains("avg=1.5"));
    }

    #[test]
    fn test_address_distribution_tracker_batch() {
        let addresses = vec![
            Address::new(Prefix::Devnet, Version::PubKey, &[1; 32]),
            Address::new(Prefix::Devnet, Version::PubKey, &[2; 32]),
            Address::new(Prefix::Devnet, Version::PubKey, &[3; 32]),
        ];

        let mut tracker = AddressDistributionTracker::new(addresses.clone());

        // Test batch allocation: 3 transactions, 2 addresses per transaction
        let batches = tracker.get_next_addresses_batch(3, 2);

        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].len(), 2);
        assert_eq!(batches[1].len(), 2);
        assert_eq!(batches[2].len(), 2);

        // Validate address allocation order
        assert_eq!(batches[0][0], &addresses[0]);
        assert_eq!(batches[0][1], &addresses[1]);
        assert_eq!(batches[1][0], &addresses[2]);
        assert_eq!(batches[1][1], &addresses[0]); // Loop back to first address
        assert_eq!(batches[2][0], &addresses[1]);
        assert_eq!(batches[2][1], &addresses[2]);

        // Validate distribution count
        assert_eq!(tracker.distribution_counts[0], 2); // Address 0 used 2 times
        assert_eq!(tracker.distribution_counts[1], 2); // Address 1 used 2 times
        assert_eq!(tracker.distribution_counts[2], 2); // Address 2 used 2 times
    }

    #[test]
    fn test_address_distribution_tracker_large_batch() {
        // Test performance with large address pool
        let addresses: Vec<Address> = (0..1000).map(|i| Address::new(Prefix::Devnet, Version::PubKey, &[i as u8; 32])).collect();

        let mut tracker = AddressDistributionTracker::new(addresses);

        // Batch allocate 100 transactions, 3 addresses per transaction
        let batches = tracker.get_next_addresses_batch(100, 3);

        assert_eq!(batches.len(), 100);
        for batch in &batches {
            assert_eq!(batch.len(), 3);
        }

        // Validate all addresses are uniformly distributed
        let total_distributions: usize = tracker.distribution_counts.iter().sum();
        assert_eq!(total_distributions, 300); // 100 * 3 = 300

        // Validate distribution stats
        let stats = tracker.get_distribution_stats();
        assert!(stats.contains("total=300"));
    }

    #[test]
    fn test_required_fee() {
        // Test fee calculation
        let fee1 = required_fee(1, 1);
        let fee2 = required_fee(2, 2);

        assert!(fee2 > fee1);
        assert_eq!(fee1, FEE_RATE * estimated_mass(1, 1));
    }

    #[test]
    fn test_estimated_mass() {
        // Test quality estimation
        let mass1 = estimated_mass(1, 1);
        let mass2 = estimated_mass(2, 2);

        assert!(mass2 > mass1);
        assert_eq!(mass1, 200 + 34 * 1 + 1000 * 1);
    }

    #[test]
    fn test_generate_tx() {
        let (secret_key, public_key) = secp256k1::generate_keypair(&mut thread_rng());
        let keypair = Keypair::from_seckey_slice(secp256k1::SECP256K1, &secret_key.secret_bytes()).unwrap();
        let addr = Address::new(Prefix::Devnet, Version::PubKey, &public_key.x_only_public_key().0.serialize());

        let utxos = vec![(
            TransactionOutpoint { transaction_id: tondi_consensus_core::Hash::from_bytes([0xFF; 32]), index: 0 },
            UtxoEntry {
                amount: 1000000,
                script_public_key: tondi_consensus_core::tx::ScriptPublicKey::from_vec(0, vec![0xff; 35]),
                block_daa_score: 1000,
                is_coinbase: false,
            },
        )];

        let tx = generate_tx(keypair, &utxos, 100000, 2, &addr);

        assert_eq!(tx.inputs.len(), 1);
        assert_eq!(tx.outputs.len(), 2);
        assert_eq!(tx.outputs[0].value, 50000);
        assert_eq!(tx.outputs[1].value, 50000);
    }

    #[test]
    fn test_generate_multi_output_tx() {
        let (secret_key, public_key) = secp256k1::generate_keypair(&mut thread_rng());
        let keypair = Keypair::from_seckey_slice(secp256k1::SECP256K1, &secret_key.secret_bytes()).unwrap();
        let addr1 = Address::new(Prefix::Devnet, Version::PubKey, &public_key.x_only_public_key().0.serialize());
        let addr2 = Address::new(Prefix::Devnet, Version::PubKey, &[0x42; 32]);

        let utxos = vec![(
            TransactionOutpoint { transaction_id: tondi_consensus_core::Hash::from_bytes([0xFF; 32]), index: 0 },
            UtxoEntry {
                amount: 1000000,
                script_public_key: tondi_consensus_core::tx::ScriptPublicKey::from_vec(0, vec![0xff; 35]),
                block_daa_score: 1000,
                is_coinbase: false,
            },
        )];

        let target_addresses = vec![&addr1, &addr2];
        let tx = generate_multi_output_tx(keypair, &utxos, 100000, &target_addresses);

        assert_eq!(tx.inputs.len(), 1);
        assert_eq!(tx.outputs.len(), 2);
        assert_eq!(tx.outputs[0].value, 50000);
        assert_eq!(tx.outputs[1].value, 50000);
    }

    #[test]
    fn test_select_utxos() {
        let utxos = vec![
            (
                TransactionOutpoint { transaction_id: tondi_consensus_core::Hash::from_bytes([0x01; 32]), index: 0 },
                UtxoEntry {
                    amount: 100000,
                    script_public_key: tondi_consensus_core::tx::ScriptPublicKey::from_vec(0, vec![0xff; 35]),
                    block_daa_score: 1000,
                    is_coinbase: false,
                },
            ),
            (
                TransactionOutpoint { transaction_id: tondi_consensus_core::Hash::from_bytes([0x02; 32]), index: 0 },
                UtxoEntry {
                    amount: 200000,
                    script_public_key: tondi_consensus_core::tx::ScriptPublicKey::from_vec(0, vec![0xff; 35]),
                    block_daa_score: 1000,
                    is_coinbase: false,
                },
            ),
        ];

        let fee_config = TxsFeeConfig { priority_fee: 0, randomize_fee: false };

        let mut index = 0;
        let (selected, amount) = select_utxos(&utxos, 50000, 1, false, &mut index, &fee_config);

        assert!(!selected.is_empty());
        assert!(amount > 0);
        assert!(index > 0);
    }

    #[test]
    fn test_is_utxo_spendable() {
        let mut entry = RpcUtxoEntry {
            amount: 100000,
            script_public_key: tondi_consensus_core::tx::ScriptPublicKey::from_vec(0, vec![0xff; 35]),
            block_daa_score: 1000,
            is_coinbase: false,
        };

        // Test non-coinbase UTXO
        // block_daa_score: 1000, needed_confs: 10, virtual_daa_score: 1020
        // 1000 + 10 = 1010 < 1020, so it should be spendable
        assert!(is_utxo_spendable(&entry, 1020, 100)); // Confirmation sufficient

        entry.block_daa_score = 1015;
        // block_daa_score: 1015, needed_confs: 10, virtual_daa_score: 1020
        // 1015 + 10 = 1025 > 1020, so it should not be spendable
        assert!(!is_utxo_spendable(&entry, 1020, 100)); // Confirmation insufficient

        // Test coinbase UTXO
        entry.is_coinbase = true;
        entry.block_daa_score = 1000;
        // coinbase needs coinbase_maturity * 2 = 200 confirmations
        // 1000 + 200 = 1200, so virtual_daa_score needs > 1200
        assert!(is_utxo_spendable(&entry, 1201, 100)); // Need coinbase_maturity * 2 confirmations
        assert!(!is_utxo_spendable(&entry, 1200, 100)); // Confirmation insufficient
    }

    #[test]
    fn test_config_defaults() {
        let config = Config {
            private_key: None,
            tps: 1,
            rpc_server: "localhost:16210".to_string(),
            threads: 2,
            unleashed: false,
            addr: None,
            address_file: None,
            outputs_per_tx: 1,
            priority_fee: 0,
            randomize_fee: false,
            generate_addresses: None,
            output_file: None,
            network: NetworkType::Testnet,
        };

        assert_eq!(config.tps, 1);
        assert_eq!(config.threads, 2);
        assert_eq!(config.outputs_per_tx, 1);
        assert!(!config.unleashed);
        assert!(!config.randomize_fee);
        assert_eq!(config.network, NetworkType::Testnet);
    }

    #[test]
    fn test_network_type_functionality() {
        // Test mainnet
        let mainnet = NetworkType::Mainnet;
        assert_eq!(mainnet.address_prefix(), Prefix::Mainnet);
        assert_eq!(mainnet.default_rpc_port(), 16110);

        // Test testnet
        let testnet = NetworkType::Testnet;
        assert_eq!(testnet.address_prefix(), Prefix::Testnet);
        assert_eq!(testnet.default_rpc_port(), 16210);

        // Test devnet
        let devnet = NetworkType::Devnet;
        assert_eq!(devnet.address_prefix(), Prefix::Devnet);
        assert_eq!(devnet.default_rpc_port(), 16210);

        // Test default
        let default_network = NetworkType::default();
        assert_eq!(default_network, NetworkType::Testnet);
    }

    #[test]
    fn test_dev_address_generation() {
        let seed_words = "zero zero zero zero zero zero zero zero zero zero zero zoo";
        let mnemonic = Mnemonic::new(seed_words, Language::English).unwrap();
        let seed = mnemonic.to_seed("");
        let extended_private_key = ExtendedPrivateKey::<SecretKey>::new(seed).unwrap();
        let seed_secret = extended_private_key.private_key();
        assert_eq!(format!("{}", seed_secret.display_secret()), "b086376aaec35dcab2d02ee45bd729112f9238bff92dbeb6340fda5b07062d83");

        let derive_path = DerivationPath::from_str("m/44'/7890'/0'/0/0").unwrap();
        let derive_private_key = extended_private_key.derive_path(&derive_path).unwrap();
        let secret_key = derive_private_key.private_key();
        assert_eq!(format!("{}", secret_key.display_secret()), "c99b1ccf1087af2a56ffedb885943962e0159a7705cac583eef3e9958cd035b3");

        let xpub = secret_key.x_only_public_key(&SECP256K1).0;
        assert_eq!(format!("{xpub}"), "757815720a73acd5a162c32a398b8ffdec534a4ced4445dc33032150d04ff976");
        let addr = Address::new(Prefix::Devnet, ADDRESS_VERSION, &xpub.serialize());
        assert_eq!(format!("{addr}"), "tondidev:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvw88ne6");
    }

    #[test]
    fn test_dev_random_address_generation() {
        let mnemonic = Mnemonic::random(WordCount::Words12, Language::English).unwrap();

        let seed = mnemonic.to_seed("");
        let extended_private_key = ExtendedPrivateKey::<SecretKey>::new(seed).unwrap();
        let seed_secret = extended_private_key.private_key();
        println!("Seed Secret: {}", seed_secret.display_secret());

        let derive_path = DerivationPath::from_str("m/44'/7890'/0'/0/0").unwrap();
        let derive_private_key = extended_private_key.derive_path(&derive_path).unwrap();
        let secret_key = derive_private_key.private_key();
        println!("Secret Key: {}", secret_key.display_secret());

        let xpub = secret_key.x_only_public_key(&SECP256K1).0;
        println!("XOnlyPublicKey: {xpub}");
        let addr = Address::new(Prefix::Devnet, ADDRESS_VERSION, &xpub.serialize());
        println!("Address: {addr}");

        // Validate address format
        assert!(format!("{addr}").starts_with("tondidev:"));
    }
}
