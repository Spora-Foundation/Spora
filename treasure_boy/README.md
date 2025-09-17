# Treasure Boy

A high-performance transaction generator and airdrop tool for the Tondi blockchain network. Treasure Boy enables efficient batch transactions, address generation, and automated airdrop operations with configurable transaction rates and fee management.

## Features

- **High-Performance Transaction Generation**: Generate transactions at configurable TPS (Transactions Per Second)
- **Batch Airdrop Operations**: Send tokens to multiple addresses efficiently
- **Time Locked Contract (TLC) Airdrop**: Create time-locked transactions for delayed token distribution
- **Hash Time Locked Contract (HTLC) Support**: Advanced conditional transactions with secret-based unlocking
- **Address Generation**: Generate random addresses for different network types
- **Multi-threaded Processing**: Parallel transaction processing for optimal performance
- **Flexible Fee Management**: Configurable priority fees with optional randomization
- **Network Support**: Support for mainnet, testnet, and devnet networks
- **UTXO Management**: Intelligent UTXO selection and management
- **Fair Distribution**: Address distribution tracking for balanced airdrops

## Installation

### Prerequisites

- Rust 1.70+ 
- Access to a Tondi RPC node

### Build from Source

```bash
git clone <repository-url>
cd Tondi/treasure_boy
cargo build --release
```

## Usage

### Basic Commands

#### Generate Random Addresses

Generate 10 random testnet addresses:
```bash
cargo run --package treasure_boy -- --generate-addresses 10
```

Generate addresses and save to file:
```bash
cargo run --package treasure_boy -- --generate-addresses 100 --output-file addresses.txt
```

#### Single Address Transaction

Send tokens to a single address:
```bash
cargo run --package treasure_boy -- \
  --private-key YOUR_PRIVATE_KEY \
  --to-addr tonditest:qr556222uq03hzf3nvxfl45x3ek07lrh7tp88xw2eh6tpuw2m9qs5e8tzc8 \
  --tps 5 \
  --amount 1000000000
```

#### Batch Airdrop

Send tokens to multiple addresses from a file:
```bash
cargo run --package treasure_boy -- \
  --private-key YOUR_PRIVATE_KEY \
  --address-file addresses.txt \
  --outputs-per-tx 10 \
  --tps 2 \
  --amount 1000000000
```

### Command Line Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--private-key` | `-k` | Private key in hex format | Required for transactions |
| `--tps` | `-t` | Transactions per second | 1 |
| `--rpcserver` | `-s` | RPC server address | localhost:16210 |
| `--threads` | | Number of threads for TX generation (0 = auto) | 2 |
| `--to-addr` | `-a` | Single address to send to | |
| `--address-file` | `-F` | File containing addresses (one per line) | |
| `--outputs-per-tx` | `-o` | Number of outputs per transaction | 1 |
| `--priority-fee` | `-f` | Transaction priority fee in SOMPS | 0 |
| `--randomize-fee` | `-r` | Randomize transaction priority fee | false |
| `--amount` | `-A` | Amount to send per address in SAU | 1000000000 |
| `--generate-addresses` | `-g` | Generate random addresses | |
| `--output-file` | `-O` | Output file for generated addresses | |
| `--network` | `-n` | Network type (mainnet/testnet/devnet) | testnet |
| `--unleashed` | | Enable high TPS mode | false |
| `--tlc-mode` | | Enable Time Locked Contract airdrop mode | false |
| `--lock-time` | | Lock time for TLC (Unix timestamp or block height) | Required for TLC |
| `--lock-time-type` | | Lock time type: timestamp or block | timestamp |
| `--htlc-secret` | | Secret for HTLC (optional) | |
| `--recipient-pubkey` | | Recipient's public key for HTLC (32 bytes hex) | |
| `--sender-pubkey` | | Sender's public key for HTLC (32 bytes hex) | |

### Network Types

- **testnet**: Uses `tonditest:` prefix (default)
- **mainnet**: Uses `tondi:` prefix  
- **devnet**: Uses `tondidev:` prefix

### Address File Format

Create a text file with one address per line:
```
# Comments are supported
tonditest:qr556222uq03hzf3nvxfl45x3ek07lrh7tp88xw2eh6tpuw2m9qs5e8tzc8
tonditest:qpl979v8dyhfw8v2d7x5rwre5ghph9d8jy0z8md06dnldkark3fs6wntgqx
tonditest:qpfmfyce6qhzknxgwvsucpdjv9xe20tmn20yc8uclxw87sk858f0k6e72wy
```

## Examples

### Example 1: Generate Test Addresses

```bash
# Generate 50 testnet addresses
cargo run --package treasure_boy -- --generate-addresses 50 --network testnet

# Generate mainnet addresses and save to file
cargo run --package treasure_boy -- --generate-addresses 100 --network mainnet --output-file mainnet_addresses.txt
```

### Example 2: Single Transaction

```bash
# Send 1 TONDI to a single address at 10 TPS
cargo run --package treasure_boy -- \
  --private-key c99b1ccf1087af2a56ffedb885943962e0159a7705cac583eef3e9958cd035b3 \
  --to-addr tonditest:qr556222uq03hzf3nvxfl45x3ek07lrh7tp88xw2eh6tpuw2m9qs5e8tzc8 \
  --tps 10 \
  --amount 1000000000
```

### Example 3: Batch Airdrop

```bash
# Airdrop to 1000 addresses with 20 outputs per transaction
cargo run --package treasure_boy -- \
  --private-key c99b1ccf1087af2a56ffedb885943962e0159a7705cac583eef3e9958cd035b3 \
  --address-file addresses.txt \
  --outputs-per-tx 20 \
  --tps 5 \
  --amount 1000000000 \
  --priority-fee 1000 \
  --randomize-fee
```

### Example 4: High-Performance Mode

```bash
# Enable unleashed mode for high TPS
cargo run --package treasure_boy -- \
  --private-key c99b1ccf1087af2a56ffedb885943962e0159a7705cac583eef3e9958cd035b3 \
  --address-file addresses.txt \
  --tps 1000 \
  --unleashed \
  --threads 8
```

### Example 5: Time Locked Contract (TLC) Airdrop

```bash
# Simple time lock airdrop (unlockable after specific timestamp)
cargo run --package treasure_boy -- \
  --private-key c99b1ccf1087af2a56ffedb885943962e0159a7705cac583eef3e9958cd035b3 \
  --address-file addresses.txt \
  --tlc-mode \
  --lock-time 1756684800 \
  --lock-time-type timestamp \
  --amount 1000000000
```

### Example 6: Hash Time Locked Contract (HTLC) Airdrop

```bash
# HTLC airdrop with secret-based unlocking
cargo run --package treasure_boy -- \
  --private-key c99b1ccf1087af2a56ffedb885943962e0159a7705cac583eef3e9958cd035b3 \
  --address-file addresses.txt \
  --tlc-mode \
  --lock-time 1756684800 \
  --lock-time-type timestamp \
  --htlc-secret "my_secret_key" \
  --recipient-pubkey 0101010101010101010101010101010101010101010101010101010101010101 \
  --sender-pubkey 0202020202020202020202020202020202020202020202020202020202020202 \
  --amount 1000000000
```

### Example 7: Block Height Time Lock

```bash
# Time lock based on block height
cargo run --package treasure_boy -- \
  --private-key c99b1ccf1087af2a56ffedb885943962e0159a7705cac583eef3e9958cd035b3 \
  --address-file addresses.txt \
  --tlc-mode \
  --lock-time 100000 \
  --lock-time-type block \
  --amount 1000000000
```

## Configuration

### Default Values

- **Default Send Amount**: 1 TONDI (1,000,000,000 SAU)
- **Default TPS**: 1 transaction per second
- **Default Network**: testnet
- **Default RPC Server**: localhost:16210
- **Default Threads**: 2
- **Default Priority Fee**: 0 SOMPS

### Performance Tuning

- **Threads**: Set to 0 for automatic detection (1 thread per CPU core)
- **TPS**: Higher values require more UTXOs and network bandwidth
- **Outputs per TX**: More outputs per transaction reduce total transaction count
- **Unleashed Mode**: Bypasses TPS limits for high-performance scenarios

## API Reference

### Core Types

#### `Config`
Main configuration structure containing all operation parameters.

#### `AddressDistributionTracker`
Tracks address distribution for fair airdrop operations.

#### `Stats`
Transaction statistics including UTXO count, amounts, and timing.

#### `NetworkType`
Enum representing supported network types (Mainnet, Testnet, Devnet).

#### `TlcAirdropConfig`
Configuration structure for Time Locked Contract airdrop operations, including lock time, secret, and public keys for HTLC functionality.

### Key Functions

#### `single_airdrop(config: Config) -> Result<(), Box<dyn Error>>`
Performs a single airdrop to one address.

#### `batch_airdrop(config: Config) -> Result<(), Box<dyn Error>>`
Performs batch airdrop to multiple addresses.

#### `load_addresses_from_file(file_path: &str) -> Result<Vec<Address>, Box<dyn Error>>`
Loads addresses from a text file.

#### `generate_addresses(count: u32, network: NetworkType) -> Vec<String>`
Generates random addresses for the specified network.

#### `tlc_airdrop(config: Config) -> Result<(), Box<dyn Error>>`
Performs TLC airdrop to multiple addresses with time-locked outputs.

#### `generate_tlc_script(address: &Address, config: &TlcAirdropConfig) -> Result<ScriptPublicKey, Box<dyn Error>>`
Generates a Time Locked Contract script for the specified address and configuration.

#### `generate_tlc_airdrop_tx(keypair: Keypair, utxos: &[(TransactionOutpoint, UtxoEntry)], amount: u64, addresses: &[&Address], config: &TlcAirdropConfig) -> Result<Transaction, Box<dyn Error>>`
Generates a transaction with TLC outputs for airdrop operations.

## Error Handling

Treasure Boy provides comprehensive error handling for common scenarios:

- **Invalid Addresses**: Automatically skips invalid addresses in batch operations
- **Insufficient UTXOs**: Clear error messages when not enough funds are available
- **Network Errors**: Automatic retry logic for transient network issues
- **RPC Connection**: Validates RPC server connectivity before operations

## Testing

Run the test suite:

```bash
# Run all tests
cargo test --package treasure_boy

# Run specific test categories
cargo test --package treasure_boy --test cli_tests
cargo test --package treasure_boy --test integration_tests
```

## Security Considerations

- **Private Key Security**: Never hardcode private keys in scripts or config files
- **Network Validation**: Always verify you're connected to the correct network
- **Amount Limits**: Set appropriate amount limits to prevent accidental large transfers
- **Fee Management**: Monitor priority fees to avoid excessive transaction costs

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests for new functionality
5. Ensure all tests pass
6. Submit a pull request

## License

This project is licensed under the same license as the Tondi project.

## Support

For issues and questions:
- Check the test files for usage examples
- Review the CLI help: `cargo run --package treasure_boy -- --help`
- Examine the source code for detailed implementation

## Version History

- **v0.17.0**: Current version with full feature set
- Support for all network types
- Comprehensive test coverage
- Performance optimizations
