# Treasure Boy

`treasure_boy` is a transaction generator and airdrop tool for the Spora network. It is now scoped to standard single-output and batch airdrops only.

## Current Scope

- Generate random addresses for `mainnet`, `testnet`, and `devnet`
- Send to a single address
- Send to many addresses from a file
- Control `TPS`, `outputs-per-tx`, thread count, and priority fee behavior

## Removed Legacy Features

Legacy TLC/HTLC airdrop support has been removed from `treasure_boy`.

The old `txscript`-based time-lock path was not a valid long-term execution model, and `treasure_boy` does not yet have a safe VM-native replacement for combined ownership + timelock outputs. Until that exists, this tool intentionally exposes only standard airdrop flows.

## Build

```bash
cargo build --release -p treasure_boy
```

## Usage

### Generate Random Addresses

```bash
cargo run --package treasure_boy -- --generate-addresses 10
```

Save generated addresses to files:

```bash
cargo run --package treasure_boy -- \
  --generate-addresses 100 \
  --network testnet \
  --output-file addresses.json
```

This writes:

- `addresses.json` with wallet metadata
- `addresses.json.addresses` with one address per line

### Single Airdrop

```bash
cargo run --package treasure_boy -- \
  --private-key YOUR_PRIVATE_KEY \
  --to-addr spora0:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvp55hu9 \
  --amount 1000000000 \
  --tps 5
```

### Batch Airdrop

```bash
cargo run --package treasure_boy -- \
  --private-key YOUR_PRIVATE_KEY \
  --address-file addresses.txt \
  --outputs-per-tx 10 \
  --amount 1000000000 \
  --tps 2
```

### Higher Throughput Batch Airdrop

```bash
cargo run --package treasure_boy -- \
  --private-key YOUR_PRIVATE_KEY \
  --address-file addresses.txt \
  --outputs-per-tx 20 \
  --priority-fee 1000 \
  --randomize-fee \
  --threads 8 \
  --unleashed
```

## CLI Options

| Option | Short | Description | Default |
| --- | --- | --- | --- |
| `--private-key` | `-k` | Private key in hex format | Required for sending |
| `--tps` | `-t` | Transactions per second | `1` |
| `--rpcserver` | `-s` | RPC server address | `localhost:16210` |
| `--threads` |  | Number of TX generation threads | `2` |
| `--to-addr` | `-a` | Single destination address |  |
| `--address-file` | `-F` | File with one destination address per line |  |
| `--outputs-per-tx` | `-o` | Number of outputs per transaction | `1` |
| `--priority-fee` | `-f` | Priority fee in SOMPS | `0` |
| `--randomize-fee` | `-r` | Randomize the priority fee up to the configured cap | `false` |
| `--amount` | `-A` | Amount per recipient in SAU | `1000000000` |
| `--generate-addresses` | `-g` | Generate random addresses instead of sending |  |
| `--output-file` | `-O` | Output path used with `--generate-addresses` |  |
| `--network` | `-n` | `mainnet`, `testnet`, or `devnet` | `testnet` |
| `--unleashed` |  | Allow higher TPS mode | `false` |

## Address File Format

One address per line, with optional blank lines or comments:

```text
# Comments are allowed
spora0:qrgqpkue0tzhmqd77tljdhwjc757hc26uestam0gc4kycjx4k8uu603uewc
spora0:qqmquth4lyayewfl32pj8w9w9dpzqk6c9ngyp4xxmyqusxruhjm0jwhje4w
spora0:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvp55hu9
```

## API Surface

Key types and functions:

- `Config`
- `Stats`
- `NetworkType`
- `AddressDistributionTracker`
- `TxsFeeConfig`
- `single_airdrop(...)`
- `batch_airdrop(...)`
- `load_addresses_from_file(...)`
- `generate_tx(...)`
- `generate_multi_output_tx(...)`

## Testing

```bash
cargo test --package treasure_boy
```

Integration helper script:

```bash
./treasure_boy/airdrop_integration_test.sh
```

## Operational Notes

- Make sure the private key controls enough mature cells before running batch sends.
- Higher `outputs-per-tx` usually reduces transaction count but increases per-tx mass.
- `priority_fee` and `randomize_fee` are useful for stress and mempool testing.
- Verify you are connected to the intended network before sending funds.
