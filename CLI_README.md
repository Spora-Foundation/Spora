# Spora CLI - Command Line Interface

Spora CLI is a comprehensive command-line interface for managing Spora wallets, accounts, and interacting with the Spora network.

## Installation

### Build from Source
```bash
git clone <repository-url>
cd Spora
cargo build --release --package spora-cli
```

## Getting Started

### First Run
1. Launch: `spora-cli`
2. Configure network: `network devnet`
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

### Quick Commands
- `c` - Clear screen (alias for `clear`)
- `ls` - List accounts (alias for `list`)

### Display Customization
- `pretty` - Toggle between emoji and ASCII display modes
  - Emoji mode: 🟢 🌐 💼 🖥️ 🔄 💰
  - ASCII mode: [+] N: W:+ D:+ S: B:

### Transactions
- `send <address> <amount>` - Send funds
- `transfer <account> <amount>` - Transfer between accounts
- `estimate <amount>` - Estimate fees
- `sweep` - Consolidate UTXOs

### Network Operations
- `network <name>` - Switch networks (mainnet, testnet-10, devnet)
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
- `clear` - Clear terminal screen


## Examples

### Setup Wallet
```bash
network devnet
server public
wallet create mywallet
open mywallet
account create bip32 primary
select primary
```

### Send Transaction
```bash
estimate 50
send spora:qp0k0fsdj8qnwvj9tcljcrjvcj9s0qj5j 50
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
