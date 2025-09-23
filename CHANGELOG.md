# Changelog
## [1.0.2] - 2025-09-17

### 🚀 New Features

#### Time Locked Contract (TLC) Airdrop Functionality
- **TLC Configuration**: Added `TlcAirdropConfig` struct for comprehensive TLC configuration management
- **Script Generation**: Implemented `generate_tlc_script()` for creating time-locked contract scripts
- **Transaction Generation**: Added `generate_tlc_airdrop_tx()` for TLC transaction creation
- **Batch Operations**: Implemented `tlc_airdrop()` for efficient batch TLC airdrop operations
- **HTLC Support**: Added Hash Time Locked Contract functionality with Blake3 secret hashing
- **CLI Enhancements**: New CLI parameters for TLC operations:
  - `--tlc-mode`: Enable TLC functionality
  - `--lock-time`: Set lock time duration
  - `--lock-time-type`: Specify lock time type (block height or timestamp)
  - `--htlc-secret`: Configure HTLC secret for recipient path
  - `--recipient-pubkey`: Set recipient public key
  - `--sender-pubkey`: Set sender public key

#### Enhanced Documentation and Examples
- **Comprehensive README**: Updated Treasure Boy README with TLC usage examples and API documentation
- **Example Documentation**: Added detailed TLC airdrop example in `tlc_airdrop_example.md`
- **Integration Tests**: Enhanced integration test script with TLC testing functions

### 🔧 Technical Improvements

#### Security and Code Quality
- **Legacy Code Removal**: Removed deprecated `gen1.rs` compatibility module and related legacy code
- **Enhanced Security**: Improved encryption module security by hiding sensitive data in debug output
- **Better Error Handling**: Enhanced error handling in encryption/decryption functions with improved validation
- **Test Coverage**: Re-enabled message signing tests with updated test vectors for BLAKE3
- **Performance Optimization**: Optimized wallet file reading in storage interface
- **Code Cleanup**: Improved documentation and cleaned up code comments

#### Dependency Updates
- **Blake3 Integration**: Added Blake3 dependency for HTLC secret hashing
- **Cargo Updates**: Updated Cargo.lock with latest dependency versions

### 🧪 Testing & Quality Assurance

#### Comprehensive Test Coverage
- **Unit Tests**: Added 22 unit tests covering TLC functionality
- **Integration Tests**: Implemented 9 integration tests for TLC operations
- **Security Validation**: Enhanced security validation for TLC scripts and transactions
- **Edge Case Testing**: Comprehensive testing of boundary conditions and error scenarios

### 📈 Performance & Reliability

#### Enhanced Stability
- **Backward Compatibility**: Maintained full backward compatibility for all public APIs
- **Memory Safety**: Improved memory management and safety practices
- **Input Validation**: Enhanced input validation throughout TLC implementation
- **Error Recovery**: Better error recovery and handling mechanisms

### 🐛 Bug Fixes

#### Infrastructure Fixes
- **Storage Interface**: Fixed wallet file reading optimization issues
- **Message Handling**: Improved message signing and validation
- **Encryption Module**: Enhanced encryption/decryption error handling

---

## [1.0.1] - 2025-09-17

### 🚀 New Features

#### Time Locked Contract (TLC) Implementation
- **Complete TLC Functionality**: Implemented full Time Locked Contract structure with IF/ELSE branches
- **Dual Spending Paths**: Support for both recipient (with secret) and sender (timeout) spending paths
- **Multi-Signature Support**: Support for both Schnorr (32-byte) and ECDSA (33-byte) signature schemes
- **Blake3 Integration**: Uses Blake3 hash algorithm for secret verification
- **P2SH Structure**: Proper Pay-to-Script-Hash implementation for secure contract execution
- **WASM Bindings**: JavaScript integration support for web applications

#### Enhanced Transaction Scripts
- **Lock Time Support**: Added `pay_to_address_with_lock_time` functionality
- **Signature Script Generation**: 
  - `htlc_signature_script_with_secret()` for recipient path
  - `htlc_signature_script_with_timeout()` for sender path
- **Comprehensive Testing**: 9 specialized test suites covering functionality, security, and edge cases

### 🐛 Bug Fixes

#### Infrastructure Fixes
- **WRPC Resolver**: Fixed Tondi WRPC resolver configuration for improved network connectivity
- **Docker Toolchain**: Fixed Docker clang toolchain configuration for better build reliability

#### Code Quality Improvements
- **Code Cleanup**: Removed unused imports and fixed compiler warnings
- **Documentation**: Enhanced documentation with detailed explanations and examples
- **Error Handling**: Improved error handling and input validation throughout TLC implementation

### 🧪 Testing & Quality Assurance

#### Comprehensive Test Coverage
- **Basic Functionality Tests**: Creation, validation, and structure verification
- **Security Validation Tests**: Script integrity and embedded data validation
- **Edge Case Tests**: Boundary values and identical key scenarios
- **Signature Script Security Tests**: Comprehensive security validation
- **Error Condition Tests**: Invalid address version, zero lock time, and max lock time scenarios

### 📈 Performance & Reliability

#### Enhanced Stability
- **Production-Ready Security**: Implemented security standards for production deployment
- **Input Validation**: Comprehensive input validation and error handling
- **Memory Safety**: Improved memory management and safety practices
- **Build Reliability**: Enhanced Docker and build system reliability

### 🔧 Technical Improvements

#### Script Engine Enhancements
- **Advanced Script Operations**: Enhanced script building capabilities
- **Contract Execution**: Improved contract execution and validation
- **Signature Verification**: Enhanced signature verification processes
- **Hash Operations**: Optimized hash operations with Blake3 integration

---

## [1.0.0] - 2025-09-14

### 🎉 Initial Release

This is the first major release of Tondi, a high-performance PoW programmable settlement layer forked from Tondi. Tondi represents a next-generation blockchain architecture designed for high-frequency trading, stablecoin settlement, and Layer 2 anchoring.

### 🚀 Major Features

#### Core Architecture
- **High-Performance PoW DAG**: Inherited and enhanced Tondi's GHOSTDAG consensus mechanism
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
- **PSTB/PSTT Support**: Renamed from PSTB/PSTT for Tondi-specific standards
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
- **vs Tondi**: 1-2x improvement over original Tondi implementation
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

This release represents the culmination of extensive development work by the Tondi development team, building upon the solid foundation provided by the Tondi project while introducing significant innovations in performance, privacy, and Layer 2 capabilities.

### 📝 Breaking Changes

#### Address Format Changes
- Changed from Tondi address format to Tondi-specific format
- Updated address prefixes: `tondi:`, `tonditest:`, `tondidev:`
- Modified address validation and checksum algorithms

#### Wallet Format Changes
- Updated HD wallet coin type from Tondi's to 7890
- Renamed PSTB/PSTT to PSTB/PSTT for Tondi standards
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
