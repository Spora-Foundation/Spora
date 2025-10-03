# TLC Airdrop Feature Usage Example

## Overview

Treasure Boy now supports Time Locked Contract (TLC) airdrop functionality, allowing creation of time-locked transactions that can only be unlocked after a specified time.

## Features

1. **Simple Time Locking**: Simple time locking based on timestamp or block height
2. **HTLC Support**: Supports Hash Time Locked Contract with secret unlock mechanism
3. **Batch Airdrop**: Supports TLC airdrop to multiple addresses
4. **Flexible Configuration**: Supports multiple network types and parameter configurations

## Usage Examples

### 1. Simple Time-Locked Airdrop

```bash
# Create timestamp-locked airdrop (unlocks on August 1, 2025)
cargo run --package treasure_boy -- \
  --private-key YOUR_PRIVATE_KEY \
  --address-file addresses.txt \
  --tlc-mode \
  --lock-time 1756684800 \
  --lock-time-type timestamp \
  --amount 1000000000 \
  --outputs-per-tx 10
```

### 2. Block Height-Locked Airdrop

```bash
# Create block height-locked airdrop (unlocks after block 100000)
cargo run --package treasure_boy -- \
  --private-key YOUR_PRIVATE_KEY \
  --address-file addresses.txt \
  --tlc-mode \
  --lock-time 100000 \
  --lock-time-type block \
  --amount 1000000000
```

### 3. HTLC Airdrop (with Secret)

```bash
# Create HTLC airdrop with secret unlock support
cargo run --package treasure_boy -- \
  --private-key YOUR_PRIVATE_KEY \
  --address-file addresses.txt \
  --tlc-mode \
  --lock-time 1756684800 \
  --lock-time-type timestamp \
  --htlc-secret "my_secret_key" \
  --recipient-pubkey 0101010101010101010101010101010101010101010101010101010101010101 \
  --sender-pubkey 0202020202020202020202020202020202020202020202020202020202020202 \
  --amount 1000000000
```

## Parameter Description

- `--tlc-mode`: Enable TLC airdrop mode
- `--lock-time`: Lock time (Unix timestamp or block height)
- `--lock-time-type`: Lock time type (timestamp or block)
- `--htlc-secret`: HTLC secret (optional)
- `--recipient-pubkey`: Recipient public key (32-byte hexadecimal)
- `--sender-pubkey`: Sender public key (32-byte hexadecimal)

## Important Notes

1. **Timestamp**: Must be greater than 500,000,000,000 (LOCK_TIME_THRESHOLD)
2. **Block Height**: Must be less than 500,000,000,000
3. **Public Key Format**: Must be a 32-byte hexadecimal string
4. **Network Support**: Supports mainnet, testnet, devnet

## Unlock Mechanism

### Simple Time Locking
- After the specified time, the address owner can unlock using their private key

### HTLC
- **Recipient Path**: Provide the correct secret and signature to unlock
- **Sender Path**: After the lock time expires, the sender can unlock using their own private key

## Security Considerations

1. Ensure private keys are stored securely
2. Verify network type is correct
3. Check that lock time settings are reasonable
4. For HTLC, ensure secrets and public keys are correct
