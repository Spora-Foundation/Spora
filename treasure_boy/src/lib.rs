use std::{collections::HashMap, fs, io::BufRead, sync::Arc, time::Duration};

use itertools::Itertools;
use parking_lot::Mutex;
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
use tondi_notify::subscription::context::SubscriptionContext;
use tondi_rpc_core::{api::rpc::RpcApi, notify::mode::NotificationMode, RpcUtxoEntry};
use tondi_txscript::pay_to_address_script;

pub const DEFAULT_SEND_AMOUNT: u64 = 10 * SAU_PER_TONDI;
pub const FEE_RATE: u64 = 10;
pub const MILLIS_PER_TICK: u64 = 10;
pub const ADDRESS_PREFIX: Prefix = Prefix::Devnet;
pub const ADDRESS_VERSION: Version = Version::PubKey;

#[derive(Debug, Clone)]
pub struct Stats {
    pub num_txs: usize,
    pub num_utxos: usize,
    pub utxos_amount: u64,
    pub num_outs: usize,
    pub since: u64,
}

#[derive(Debug, Clone)]
pub struct AddressDistributionTracker {
    addresses: Vec<Address>,
    distribution_counts: Vec<usize>,
    current_index: usize,
}

impl AddressDistributionTracker {
    pub fn new(addresses: Vec<Address>) -> Self {
        let distribution_counts = vec![0; addresses.len()];
        Self {
            addresses,
            distribution_counts,
            current_index: 0,
        }
    }

    pub fn get_next_addresses(&mut self, count: usize) -> Vec<&Address> {
        let mut selected_addresses = Vec::new();
        
        for _ in 0..count {
            if self.addresses.is_empty() {
                break;
            }
            
            // 选择当前索引的地址
            let addr = &self.addresses[self.current_index];
            selected_addresses.push(addr);
            
            // 增加分发计数
            self.distribution_counts[self.current_index] += 1;
            
            // 移动到下一个地址（循环）
            self.current_index = (self.current_index + 1) % self.addresses.len();
        }
        
        selected_addresses
    }

    pub fn get_random_addresses(&mut self, count: usize) -> Vec<&Address> {
        let mut selected_addresses = Vec::new();
        
        if self.addresses.is_empty() {
            return selected_addresses;
        }
        
        for _ in 0..count {
            // 随机选择一个地址
            let mut rng = thread_rng();
            let index = rng.gen_range(0..self.addresses.len());
            let addr = &self.addresses[index];
            selected_addresses.push(addr);
            
            // 增加分发计数
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

#[derive(Debug, Clone)]
pub struct Config {
    pub private_key: Option<String>,
    pub tps: u64,
    pub rpc_server: String,
    pub threads: u8,
    pub unleashed: bool,
    pub addr: Option<String>,
    pub address_file: Option<String>,
    pub outputs_per_tx: u64,
    pub priority_fee: u64,
    pub randomize_fee: bool,
}

#[derive(Debug, Clone)]
pub struct TxsFeeConfig {
    pub priority_fee: u64,
    pub randomize_fee: bool,
}

pub struct ClientPoolArg {
    pub tx: Transaction,
    pub stats: Arc<Mutex<Stats>>,
    pub selected_utxos_len: usize,
    pub selected_utxos_amount: u64,
    pub pending_len: usize,
    pub utxos_len: usize,
}

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

pub async fn new_rpc_client(subscription_context: &SubscriptionContext, address: &str) -> GrpcClient {
    GrpcClient::connect_with_args(
        NotificationMode::Direct,
        format!("grpc://{}", address),
        Some(subscription_context.clone()),
        true,
        None,
        false,
        Some(500_000),
        Default::default(),
    )
    .await
    .unwrap()
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
    signed_tx.tx
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

    // 为每个目标地址创建一个输出
    let outputs = target_addresses
        .iter()
        .map(|addr| {
            let script_public_key = pay_to_address_script(addr);
            TransactionOutput { 
                value: send_amount / target_addresses.len() as u64, 
                script_public_key 
            }
        })
        .collect_vec();
    
    let unsigned_tx = Transaction::new_non_finalized(TX_VERSION, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
    let signed_tx =
        sign(MutableTransaction::with_entries(unsigned_tx, utxos.iter().map(|(_, entry)| entry.clone()).collect_vec()), schnorr_key);
    signed_tx.tx
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

pub async fn populate_pending_outpoints_from_mempool(
    rpc_client: &GrpcClient,
    tondi_addr: Address,
    pending_outpoints: &mut HashMap<TransactionOutpoint, Instant>,
) {
    let entries = rpc_client.get_mempool_entries_by_addresses(vec![tondi_addr], true, false).await.unwrap();
    let now = Instant::now();

    for entry in entries {
        for entry in entry.sending {
            for input in entry.transaction.inputs {
                pending_outpoints.insert(input.previous_outpoint.into(), now);
            }
        }
    }
}

pub async fn fetch_spendable_utxos(
    rpc_client: &GrpcClient,
    tondi_addr: Address,
    coinbase_maturity: u64,
    pending: &mut HashMap<TransactionOutpoint, Instant>,
) -> Vec<(TransactionOutpoint, UtxoEntry)> {
    let resp = rpc_client.get_utxos_by_addresses(vec![tondi_addr]).await.unwrap();
    let dag_info = rpc_client.get_block_dag_info().await.unwrap();

    let mut utxos = resp.into_iter()
        .filter(|entry| {
            is_utxo_spendable(&entry.utxo_entry, dag_info.virtual_daa_score, coinbase_maturity)
        })
        .map(|entry| (TransactionOutpoint::from(entry.outpoint), UtxoEntry::from(entry.utxo_entry)))
        // Eliminates UTXOs we already tried to spend so we don't try to spend them again in this period
        .filter(|(outpoint,_)| !pending.contains_key(outpoint))
        .collect::<Vec<_>>();
    utxos.sort_by(|a, b| b.1.amount.cmp(&a.1.amount));
    utxos
}

pub async fn refresh_utxos(
    rpc_client: &GrpcClient,
    tondi_addr: Address,
    pending: &mut HashMap<TransactionOutpoint, Instant>,
    coinbase_maturity: u64,
) -> Vec<(TransactionOutpoint, UtxoEntry)> {
    populate_pending_outpoints_from_mempool(rpc_client, tondi_addr.clone(), pending).await;
    fetch_spendable_utxos(rpc_client, tondi_addr, coinbase_maturity, pending).await
}

pub fn clean_old_pending_outpoints(pending: &mut HashMap<TransactionOutpoint, Instant>) {
    let now = Instant::now();
    pending.retain(|_, &mut time| now.duration_since(time) <= Duration::from_secs(3600));
}

pub fn should_maximize_inputs(
    old_value: bool,
    utxos: &[(TransactionOutpoint, UtxoEntry)],
    pending: &HashMap<TransactionOutpoint, Instant>,
) -> bool {
    let estimated_utxos = if utxos.len() > pending.len() { utxos.len() - pending.len() } else { 0 };
    if !old_value && estimated_utxos > 1_000_000 {
        info!("Starting to maximize inputs");
        true
    } else if old_value && estimated_utxos < 500_000 {
        info!("Stopping to maximize inputs");
        false
    } else {
        old_value
    }
}

pub async fn pause_if_mempool_is_full(rpc_client: &GrpcClient) {
    loop {
        let mempool_size = rpc_client.get_info().await.unwrap().mempool_size;
        if mempool_size < 200_000 {
            break;
        }

        const PAUSE_DURATION: u64 = 10;
        info!("Mempool has {} entries. Pausing for {} seconds to reduce mempool pressure", mempool_size, PAUSE_DURATION);
        tokio::time::sleep(Duration::from_secs(PAUSE_DURATION)).await;
    }
}

pub async fn maybe_send_tx(
    txs_to_send: u64,
    tx_sender: &async_channel::Sender<ClientPoolArg>,
    address_tracker: &mut AddressDistributionTracker,
    utxos: &mut [(TransactionOutpoint, UtxoEntry)],
    pending: &mut HashMap<TransactionOutpoint, Instant>,
    schnorr_key: Keypair,
    stats: Arc<Mutex<Stats>>,
    maximize_inputs: bool,
    next_available_utxo_index: &mut usize,
    fee_config: &TxsFeeConfig,
    outputs_per_tx: u64,
) -> bool {
    let num_outs = if maximize_inputs { 1 } else { outputs_per_tx };

    let mut has_fund = false;

    let selected_utxos_groups = (0..txs_to_send)
        .map(|_| {
            let (selected_utxos, selected_amount) =
                select_utxos(utxos, DEFAULT_SEND_AMOUNT, num_outs, maximize_inputs, next_available_utxo_index, fee_config);
            if selected_amount == 0 {
                return None;
            }

            // If any iteration successfully selected UTXOs, we assume to still
            // have funds in this tick
            has_fund = true;

            let now = Instant::now();
            for input in selected_utxos.iter() {
                pending.insert(input.0, now);
            }

            Some((selected_utxos, selected_amount))
        })
        .collect::<Vec<_>>();

    if !has_fund {
        return false;
    }

    let txs = selected_utxos_groups
        .into_iter()
        .map(|utxo_option| {
            if let Some((selected_utxos, selected_amount)) = utxo_option {
                // 获取目标地址（智能分发）
                let target_addresses = if address_tracker.addresses.len() == 1 {
                    vec![&address_tracker.addresses[0]]
                } else {
                    address_tracker.get_next_addresses(num_outs as usize)
                };
                
                let tx = generate_multi_output_tx(schnorr_key, &selected_utxos, selected_amount, &target_addresses);

                return Some((tx, selected_utxos.len(), selected_utxos.into_iter().map(|(_, entry)| entry.amount).sum::<u64>()));
            }

            None
        })
        .collect::<Vec<_>>();

    for (tx, selected_utxos_len, selected_utxos_amount) in txs.into_iter().flatten() {
        tx_sender
            .send(ClientPoolArg {
                tx,
                stats: stats.clone(),
                selected_utxos_len,
                selected_utxos_amount,
                pending_len: pending.len(),
                utxos_len: utxos.len(),
            })
            .await
            .unwrap();
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use secp256k1::{SecretKey, SECP256K1};
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
        
        // 测试获取下一个地址
        let selected = tracker.get_next_addresses(2);
        assert_eq!(selected.len(), 2);
        assert_eq!(tracker.current_index, 2);
        assert_eq!(tracker.distribution_counts[0], 1);
        assert_eq!(tracker.distribution_counts[1], 1);
        assert_eq!(tracker.distribution_counts[2], 0);
        
        // 测试循环
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
        let addresses = vec![
            Address::new(Prefix::Devnet, Version::PubKey, &[1; 32]),
            Address::new(Prefix::Devnet, Version::PubKey, &[2; 32]),
        ];
        
        let mut tracker = AddressDistributionTracker::new(addresses);
        
        tracker.get_next_addresses(3); // 地址0: 2次, 地址1: 1次
        
        let stats = tracker.get_distribution_stats();
        assert!(stats.contains("min=1"));
        assert!(stats.contains("max=2"));
        assert!(stats.contains("total=3"));
        assert!(stats.contains("avg=1.5"));
    }

    #[test]
    fn test_required_fee() {
        // 测试费用计算
        let fee1 = required_fee(1, 1);
        let fee2 = required_fee(2, 2);
        
        assert!(fee2 > fee1);
        assert_eq!(fee1, FEE_RATE * estimated_mass(1, 1));
    }

    #[test]
    fn test_estimated_mass() {
        // 测试质量估算
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
            TransactionOutpoint {
                transaction_id: tondi_consensus_core::Hash::from_bytes([0xFF; 32]),
                index: 0,
            },
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
            TransactionOutpoint {
                transaction_id: tondi_consensus_core::Hash::from_bytes([0xFF; 32]),
                index: 0,
            },
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
                TransactionOutpoint {
                    transaction_id: tondi_consensus_core::Hash::from_bytes([0x01; 32]),
                    index: 0,
                },
                UtxoEntry {
                    amount: 100000,
                    script_public_key: tondi_consensus_core::tx::ScriptPublicKey::from_vec(0, vec![0xff; 35]),
                    block_daa_score: 1000,
                    is_coinbase: false,
                },
            ),
            (
                TransactionOutpoint {
                    transaction_id: tondi_consensus_core::Hash::from_bytes([0x02; 32]),
                    index: 0,
                },
                UtxoEntry {
                    amount: 200000,
                    script_public_key: tondi_consensus_core::tx::ScriptPublicKey::from_vec(0, vec![0xff; 35]),
                    block_daa_score: 1000,
                    is_coinbase: false,
                },
            ),
        ];
        
        let fee_config = TxsFeeConfig {
            priority_fee: 0,
            randomize_fee: false,
        };
        
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
        
        // 测试非 coinbase UTXO
        assert!(is_utxo_spendable(&entry, 1020, 100)); // 需要10个确认
        
        entry.block_daa_score = 1015;
        assert!(!is_utxo_spendable(&entry, 1020, 100)); // 确认不足
        
        // 测试 coinbase UTXO
        entry.is_coinbase = true;
        entry.block_daa_score = 1000;
        // coinbase 需要 coinbase_maturity * 2 = 200 个确认
        // 1000 + 200 = 1200，所以 virtual_daa_score 需要 > 1200
        assert!(is_utxo_spendable(&entry, 1201, 100)); // 需要 coinbase_maturity * 2 个确认
        assert!(!is_utxo_spendable(&entry, 1200, 100)); // 确认不足
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
        };
        
        assert_eq!(config.tps, 1);
        assert_eq!(config.threads, 2);
        assert_eq!(config.outputs_per_tx, 1);
        assert!(!config.unleashed);
        assert!(!config.randomize_fee);
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
        
        // 验证地址格式
        assert!(format!("{addr}").starts_with("tondidev:"));
    }
}
