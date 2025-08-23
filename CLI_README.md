# Tondi CLI - Command Line Interface

Tondi CLI is a comprehensive command-line interface for managing Tondi wallets, accounts, and interacting with the Tondi network.

## Installation

### Build from Source
```bash
git clone <repository-url>
cd Tondi
cargo build --release --package tondi-cli
```

## Getting Started

### First Run
1. Launch: `tondi-cli`
2. Configure network: `network testnet-11`
3. Configure server: `server public`

### Basic Workflow
1. Create wallet: `wallet create [name]`
2. Open wallet: `open <name>`
3. Create accounts: `account create bip32 [name]`
4. Select account: `select <account-name>`
5. Perform transactions

## Commands Reference

### Wallet Management
- `wallet create [<name>]` - Create new wallet
- `open <name>` - Open existing wallet
- `close` - Close current wallet
- `list` - List accounts and balances

### Account Management
- `account create <type> [<name>]` - Create new account
- `account import <type> <format>` - Import existing account
- `select <account-name>` - Select active account
- `account name <name>` - Rename account

### Transactions
- `send <address> <amount>` - Send funds
- `transfer <account> <amount>` - Transfer between accounts
- `estimate <amount>` - Estimate fees
- `sweep` - Consolidate UTXOs

### Network Operations
- `network <name>` - Switch networks (mainnet, testnet-10, testnet-11)
- `server <address>` - Configure server connection
- `connect <address>` - Connect to node
- `ping` - Test connection

### Monitoring
- `monitor` - Real-time balance monitoring
- `mute [on|off]` - Toggle event notifications
- `track <event>` - Enable specific event tracking
- `metrics` - Display performance metrics

### System
- `help` - Show command help
- `guide` - Display usage guide
- `exit` - Exit application
- `reload` - Reload configuration

## Examples

### Setup Wallet
```bash
network testnet-11
server public
wallet create mywallet
open mywallet
account create bip32 primary
select primary
```

### Send Transaction
```bash
estimate 50
send tondi:qp0k0fsdj8qnwvj9tcljcrjvcj9s0qj5j 50
history list
```

## Security Notes
- Store mnemonic phrases securely offline
- Use testnet for development
- Only connect to trusted nodes
- Never share private keys

## Support
- Use `guide` for basic usage
- Use `help` for command reference
- Check error messages for guidance
