# Changelog
## [1.0.0] - 2025-09-14

### 🎉 Initial Release

This is the first major release of Tondi, a high-performance PoW programmable settlement layer forked from Kaspa. Tondi represents a next-generation blockchain architecture designed for high-frequency trading, stablecoin settlement, and Layer 2 anchoring.

### 🚀 Major Features

#### Core Architecture
- **High-Performance PoW DAG**: Inherited and enhanced Kaspa's GHOSTDAG consensus mechanism
- **Blake3 Hashing**: Upgraded from SHA256 to Blake3 for superior performance and SIMD support
- **Dynamic Block Frequency**: Achieves ≥10 blocks/second with automatic adjustment
- **Parallel Transaction Processing**: Supports 15,000-25,000 TPS with 1-2 second confirmation latency
- **Stateless UTXO Model**: Maintains Bitcoin-compatible UTXO structure without global state bloat

#### Bitcoin Taproot Compatibility
- **Native Taproot Support**: Full compatibility with Bitcoin Taproot (P2TR) addresses
- **OP_SUCCESS Compatibility**: Implemented Bitcoin Taproot OP_SUCCESS opcodes for enhanced script flexibility
- **Bech32m Address Support**: Native support for Bech32m address encoding
- **Taproot Script Spend**: Support for Taproot script path spending
- **Taproot Key Spend**: Support for Taproot key path spending

### 🛠️ New Components

#### Treasure Boy Transaction Generator
- **High-Performance Transaction Generation**: Configurable TPS (Transactions Per Second) support
- **Batch Airdrop Operations**: Efficient multi-address token distribution
- **Multi-threaded Processing**: Parallel transaction processing for optimal performance
- **Network Selection**: Support for mainnet, testnet, and devnet networks
- **Flexible Fee Management**: Configurable priority fees with randomization options
- **UTXO Management**: Intelligent UTXO selection and management
- **Address Generation**: Random address generation for different network types

#### Enhanced RPC APIs
- **Block Header API**: `get_block_header` RPC endpoint for detailed block information
- **Block Status API**: `get_block_status` RPC endpoint for block confirmation status
- **Transaction API**: `get_transaction` RPC endpoint for transaction details
- **GRPC Client Improvements**: Enhanced GRPC client functionality and error handling

#### Wallet Enhancements
- **HD Wallet Support**: Hierarchical deterministic wallet with coin type 7890
- **PSTB/PSTT Support**: Renamed from PSKB/PSKT for Tondi-specific standards
- **Multi-signature Wallets**: Enhanced multisig support with MuSig2
- **Address Management**: Improved address generation and management
- **Enhanced CLI**: Better user experience with improved prompts and pretty mode

#### Developer Experience
- **Comprehensive Documentation**: Extensive API documentation and usage guides
- **Test Coverage**: Comprehensive test suite with integration tests
- **CLI Improvements**: Enhanced command-line interface with better UX
- **Error Handling**: Improved error messages and handling throughout the codebase


### 📈 Performance Benchmarks
#### Throughput Comparison
- **vs Bitcoin**: 15,000-25,000 TPS vs 7 TPS (2,000-3,500x improvement)
- **vs Kaspa**: 1-2x improvement over original Kaspa implementation
- **vs Solana**: Comparable performance with decentralized PoW architecture
- **vs ETH Rollups**: Independent cost structure and optimized for settlement

#### Confirmation Times
- **Finality**: 1-2 seconds with fork resistance
- **Block Frequency**: Dynamic ≥10 blocks/second
- **Latency**: Sub-second transaction confirmation

### 🐛 Bug Fixes
#### Core Fixes
- Fixed coin type legacy issues
- Resolved GRPC client call problems
- Fixed transaction ID generation
- Corrected HD wallet derivation paths
- Resolved address checksum validation issues

#### Network Fixes
- Fixed testnet DNS seeders
- Resolved network type selection issues
- Fixed address format inconsistencies
- Corrected transaction fee calculations

#### Wallet Fixes
- Fixed multisig wallet operations
- Resolved address generation issues
- Fixed transaction signing problems
- Corrected balance calculation errors

### 📚 Documentation

#### Comprehensive Documentation
- **Technical Whitepaper**: Detailed technical architecture and design decisions
- **API Documentation**: Complete RPC and GRPC API documentation
- **Developer Guides**: Step-by-step development and integration guides
- **CLI Documentation**: Complete command-line interface documentation
- **Security Guidelines**: Security best practices and recommendations

### 🏆 Acknowledgments

This release represents the culmination of extensive development work by the Tondi development team, building upon the solid foundation provided by the Kaspa project while introducing significant innovations in performance, privacy, and Layer 2 capabilities.

### 📝 Breaking Changes

#### Address Format Changes
- Changed from Kaspa address format to Tondi-specific format
- Updated address prefixes: `tondi:`, `tonditest:`, `tondidev:`
- Modified address validation and checksum algorithms

#### Wallet Format Changes
- Updated HD wallet coin type from Kaspa's to 7890
- Renamed PSKB/PSKT to PSTB/PSTT for Tondi standards
- Changed satoshi unit to SAU (Smallest Addressable Unit)

#### API Changes
- Updated RPC endpoints with new naming conventions
- Modified GRPC service definitions
- Changed transaction format and serialization

### 🔧 Configuration Changes

#### Network Configuration
- Updated default network parameters
- Modified consensus rules and block validation
- Changed fee calculation algorithms
- Updated mining parameters

#### Wallet Configuration
- Modified default wallet settings
- Updated key derivation paths
- Changed address generation parameters
- Modified transaction fee defaults

---

## Development Information

### Repository
- **GitHub**: https://github.com/AvatoLabs/Tondi
- **License**: MIT
- **Authors**: Tondi developers

### Build Requirements
- **Rust**: 1.89.0 or later
- **Platform**: Linux, macOS, Windows
- **Architecture**: x86_64, ARM64

### Installation
```bash
git clone https://github.com/AvatoLabs/Tondi.git
cd Tondi
cargo build --release
```

### Quick Start
```bash
# Start Tondi daemon
./target/release/tondid

# Use CLI wallet
./target/release/tondi-cli

# Generate transactions with Treasure Boy
cargo run --package treasure_boy -- --help
```
