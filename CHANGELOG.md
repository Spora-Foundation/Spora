# Changelog

## [1.21.0] - 2025-10-14

### 🚀 Major Features

#### Enhanced RPC API
- **New UTXO Query Endpoint**: Added `get_utxos_by_address` RPC endpoint for efficient UTXO retrieval by address
- **Improved Database Access**: Enhanced database access layer with better UTXO indexing and query capabilities
- **GRPC Integration**: Full GRPC support for the new UTXO query functionality across all client libraries

#### Economic Model Updates
- **Total Rewards Adjustment**: Updated total rewards to 300 billion SAU for improved economic sustainability
- **Consensus Parameter Updates**: Refined consensus parameters to support the new reward structure
- **Coinbase Processing**: Enhanced coinbase transaction processing with updated reward calculations

### 🔧 Technical Improvements

#### Address System Enhancements
- **Testnet Address Prefix**: Updated testnet address prefix to `tondi0` for better network identification
- **Script Version Decoupling**: Separated script execution versions from address encoding versions for clearer architecture
- **Enhanced Error Handling**: Comprehensive AddressError handling across all address-related modules
- **Improved Documentation**: Updated Copperoot documentation with clearer version-to-prefix mapping

#### Script System Improvements
- **Version Constants Update**:
  - `SCRIPT_VER_TAPROOT`: Updated from 88 to 1
  - `MAX_SCRIPT_PUBLIC_KEY_VERSION`: Updated from 3 to 193
- **Copperoot Version Separation**:
  - `SCRIPT_VER_COPPEROOT_MERKLE = 2`
  - `SCRIPT_VER_COPPEROOT_VERKLE = 3`
- **Address Version Mapping**:
  - CopperootMerkle = 192 ('c' prefix)
  - CopperootVerkle = 96 ('v' prefix)

#### Configuration and Serialization
- **Serde Integration**: Added Serde derive support for airdrop configuration structures
- **Enhanced Serialization**: Improved configuration serialization for better API compatibility

### 🐛 Bug Fixes

#### Build System Fixes
- **WASM Build Support**: Fixed WASM build issues with proper dependency management
- **BIP32 Dependencies**: Added missing BIP32 dependencies for WASM compatibility
- **Build Script Updates**: Improved build scripts for better cross-platform support

#### Network and Consensus Fixes
- **Genesis Timestamp**: Fixed TESTNET_GENESIS timestamp for proper network initialization
- **RPC Mock Testing**: Improved RPC core mock testing for better test reliability
- **Address Validation**: Enhanced address validation with proper error propagation

#### Code Quality Improvements
- **Error Propagation**: Fixed Address::new() error propagation with proper ? operator usage
- **Lock Handling**: Improved lock handling with better error messages
- **Test Coverage**: Enhanced integration tests with proper error handling

### 📈 Performance & Reliability

#### Database Optimizations
- **UTXO Indexing**: Improved UTXO indexing performance with better data structures
- **Query Optimization**: Enhanced database query performance for UTXO operations
- **Memory Management**: Better memory usage in address and script processing

#### Network Improvements
- **Address Processing**: Faster address validation and processing
- **Script Execution**: Optimized script version handling and execution
- **Error Recovery**: Better error recovery mechanisms throughout the codebase

### 🔒 Security Enhancements

#### Address Security
- **Version Validation**: Enhanced address version validation with proper error handling
- **Script Security**: Improved script version security with clear separation of concerns
- **Input Validation**: Better input validation for address and script operations

#### Error Handling
- **Comprehensive Error Types**: Added AddressError types for better error reporting
- **Error Consistency**: Improved error message consistency across modules
- **Security Validation**: Enhanced security validation for address operations

### 📚 Documentation Updates

#### API Documentation
- **RPC API**: Updated RPC API documentation with new UTXO query endpoints
- **Address System**: Enhanced address system documentation with version mappings
- **Copperoot Guide**: Updated Copperoot documentation with clearer version information

#### Code Documentation
- **Inline Documentation**: Improved inline documentation throughout address and script modules
- **Error Documentation**: Enhanced error documentation with clear examples
- **Integration Examples**: Updated integration test examples with proper error handling

### 🧪 Testing & Quality Assurance

#### Enhanced Test Coverage
- **Integration Tests**: Improved integration tests with better error handling
- **Address Tests**: Enhanced address validation tests with comprehensive scenarios
- **RPC Tests**: Added tests for new UTXO query functionality
- **Error Handling Tests**: Comprehensive error handling test coverage

#### Test Stability
- **Mock Improvements**: Better RPC mock testing for reliable test execution
- **Test Data**: Updated test data with proper address formats and versions
- **Cross-Platform Testing**: Improved cross-platform test compatibility

### 🔄 Breaking Changes

#### Address Format Changes
- **Testnet Prefix**: Changed testnet address prefix from previous format to `tondi0`
- **Version Constants**: Updated script version constants (breaking change for custom implementations)
- **Error Handling**: Address::new() now returns Result type (breaking change for error handling)

#### API Changes
- **RPC Endpoints**: Added new `get_utxos_by_address` endpoint
- **Database Interface**: Enhanced database access interface (may require updates for custom implementations)
- **Configuration**: Updated configuration structures with Serde support

### 📋 Migration Guide

#### For Developers
1. **Address Handling**: Update address validation code to handle new error types
2. **Script Versions**: Update any hardcoded script version constants
3. **Testnet Addresses**: Update testnet address generation to use `tondi0` prefix
4. **RPC Integration**: Utilize new `get_utxos_by_address` endpoint for UTXO queries

#### For Node Operators
1. **Configuration**: No configuration changes required
2. **Database**: Database will automatically handle new UTXO indexing
3. **Network**: Testnet addresses will use new `tondi0` prefix

---

## [1.20.0] - 2025-01-13

### 🚀 Major Features

#### Copperoot: Advanced Taproot Variant Implementation
- **BLAKE3-256 Hashing**: Complete implementation with domain separation for superior performance over SHA256
- **Bech32m Address Support**: CopperootMerkle addresses (version 192) with 'c' prefix for human-readable identification
- **Merkle Tree Support**: 8-layer depth limit with consensus validation and complete tweak verification
- **Complete MuSig2 Integration**: Full two-round protocol implementation with wrapper API and session management
- **Safe MuSig2 Interface**: Multiple witness creation methods with validation to prevent misuse patterns
- **TLV Framework**: Forward-compatible Type-Length-Value extensions for RGB integration and future protocol enhancements
- **ScriptVariant Trait**: Abstract execution semantics for reusable execution flow across Taproot-like variants

### 🔧 Technical Improvements

#### Advanced Cryptographic Features
- **Domain-Separated Hash Functions**: CopperootSighash, CopperootLeaf, CopperootNode, CopperTweak with enhanced security
- **Enhanced Control Block Structure**: TLV-based control blocks with Merkle proof support and strict validation
- **Annex Support**: BIP341-compliant annex handling with strict 0x50 prefix validation
- **Witness Structure**: Key-path and script-path spending with comprehensive validation
- **Cross-Chain Protection**: Chain ID isolation preventing cross-chain transaction replay attacks

#### Wallet and Transaction Enhancements
- **Address Type Detection**: Automatic identification of CopperootMerkle vs Bitcoin Taproot addresses
- **Script Classification**: Version-based dispatch with format validation for all script types
- **Enhanced Security**: Comprehensive constraint checking, error handling, and misuse prevention
- **Mempool Policy**: Strict script version validation with NonStandard classification for unknown formats

#### Consensus and Validation
- **Strict Version Validation**: MAX_SCRIPT_PUBLIC_KEY_VERSION constraint preventing future version confusion
- **Script Format Validation**: Standardized OP_1 <32-byte x-only pubkey> format for all script versions
- **Complete Merkle Commitment Verification**: Full tweak verification with parity bit validation
- **Production-Ready Validation**: Mempool policy checks and standard transaction validation

### 🎯 Reserved Features (Future Activation)

#### Verkle Tree Infrastructure
- **CopperootVerkle Support**: Reserved for future activation (version 193 currently disabled)
- **Verkle Proof Framework**: TLV-based proof system for scalable state management
- **Proof Aggregation**: Batch operation support for large-scale applications
- **Advanced Verification**: Inner Product Argument (IPA) commitments for efficient proof generation
- **Future ZK Integration**: Framework ready for zero-knowledge proof integration

#### RGB Protocol Integration Preparation
- **RGB Commitment Support**: TLV-based RGB root anchoring without consensus changes
- **Cross-Layer Information**: Annex TLV extensions for RGB state epoch and namespace identification
- **Light Client Optimization**: Stateless verification framework for mobile wallet synchronization
- **Batch RGB Operations**: Support for multi-asset and multi-state operations

### 📊 Performance Benefits

#### Computational Improvements
- **BLAKE3 vs SHA256**: 2-4x faster hashing on modern hardware with SIMD optimization
- **Parallel Processing**: BLAKE3 tree structure enables parallel hashing of large inputs
- **Memory Efficiency**: Reduced memory footprint for tree operations
- **SIMD Acceleration**: Additional 2-3x speedup on supported CPU architectures

#### Scalability Enhancements
- **Merkle Tree Verification**: O(log n) verification with enhanced constants
- **Proof Size Optimization**: Consistent proof sizes with comprehensive validation
- **Batch Operations**: Efficient multi-signature and multi-contract support
- **Future Verkle Trees**: 10-100x more efficient than Merkle proofs for large state

### 🔒 Security Enhancements

#### Cryptographic Security
- **256-bit Security Level**: BLAKE3-256 maintains same security as SHA256 with better performance
- **Domain Separation**: Comprehensive tagged hashing prevents cross-protocol attacks
- **Constant-Time Operations**: All cryptographic operations implement constant-time algorithms
- **Input Validation**: Comprehensive validation prevents malformed input attacks

#### Implementation Security
- **Memory Safety**: Rust's memory safety guarantees for all cryptographic operations
- **Type Safety**: Automatic handling of secp256k1 version differences between libraries
- **Error Handling**: Clear error messages for validation failures and misuse prevention
- **Misuse Prevention**: Signature validation prevents common MuSig2 misuse patterns

### 📋 Backward Compatibility

#### Seamless Integration
- **Full Taproot Compatibility**: Existing Bitcoin Taproot addresses continue to work without modification
- **Gradual Migration Path**: New addresses can use Copperoot features while maintaining compatibility
- **Hybrid Support**: Nodes support both Taproot and Copperoot simultaneously
- **Version Isolation**: Clear separation between Bitcoin Taproot (v1) and Copperoot variants

### 🧪 Testing and Validation

#### Comprehensive Test Coverage
- **Key Spend Tests**: Direct signature verification using Copperoot-specific sighash computation
- **Script Spend Tests**: Full script path validation with Merkle proof verification
- **MuSig2 Testing**: Complete wrapper functionality testing and session management
- **Witness Parsing**: Annex detection and BIP341-compliant witness structure validation
- **Edge Case Handling**: Proper error propagation for invalid signatures and malformed witnesses

### 🌟 Future Roadmap

#### Phase 1: Current Status (CopperootMerkle Active)
- ✅ Production-ready CopperootMerkle implementation
- ✅ Complete MuSig2 integration with safety features
- ✅ BLAKE3-256 hashing with domain separation
- ✅ TLV framework for future extensions

#### Phase 2: Future Enhancements (CopperootVerkle Activation)
- 🔄 Verkle tree activation for large-scale applications
- 🔄 Advanced proof aggregation for batch operations
- 🔄 ZK-proof integration for privacy-preserving transactions
- 🔄 RGB protocol deep integration

### 📚 Documentation and Resources

- **Comprehensive Documentation**: Complete implementation guide in docs/copperoot.md
- **API Reference**: Detailed API documentation for all Copperoot features
- **Test Vectors**: Complete test vectors for validation and compatibility testing
- **Migration Guide**: Step-by-step migration path from Taproot to Copperoot
- **Security Audit**: Formal security consideration documentation

## [1.10.0] - 2025-10-02

### 🚀 Major Features

#### SwiftHash Algorithm Upgrade
- **Algorithm Replacement**: Replaced KHeavyHash with SwiftHeavy algorithm
- **Nonce-Hardened Matrix**: Implemented nonce-hardened matrix with improved mining efficiency
- **Performance Optimization**: Significant improvements in hash calculation and mining speed

#### Enhanced Wallet Time-Locked Functionality
- **pay_to_address_with_lock_time_script**: Added support for time-locked address scripts
- **Enhanced Lock Time Support**: Improved payment public key time locking capabilities
- **Account Transfer Improvements**: Renamed sompi unit to SAU (Smallest Addressable Unit)

#### SDK Adapter Signature Integration
- **Official Dependency**: Integrated adapter signature crate from official crates.io
- **Official Bitcoin Library**: Replaced unofficial rust-bitcoin with official version
- **Enhanced Security**: Improved overall encryption module security and reliability

### 🔧 Technical Improvements

#### RPC API Enhancements
- **GetVirtualChainFromBlockRequest**: Added minimum confirmation count support
- **Enhanced Virtual Chain Operations**: Improved request handling and functionality

#### Consensus and Mining Optimizations
- **KIP 10 Activation**: Set DAA activation score to 0 for all networks
- **UTXO Index Improvements**: Fixed rare file descriptor overflow issues
- **Test Stability**: Resolved coinbase subsidy test and body validation issues

#### Code Quality and Stability
- **Cargo Test Fixes**: Improved test stability and resolved test failures
- **Subsidy Calculation**: Fixed subsidy calculation tests and txscript doctests
- **WASM Build Restrictions**: Resolved WASM build limitation issues
- **Public COIN_TYPE**: Fixed accessibility of tondi COIN_TYPE

### 🐛 Bug Fixes

#### Infrastructure Fixes
- **File Descriptor Overflow**: Resolved rare overflow fix with utxoindex
- **Cargo Test Stability**: Fixed multiple cargo test failures
- **Build System**: Improved WASM build restrictions and compatibility

#### Wallet and Transaction Fixes
- **Account Transfer**: Fixed sompi to SAU renaming in account transfer functionality
- **Payment Scripts**: Resolved payment public key time locking issues
- **Transaction Validation**: Fixed coinbase subsidy and body validation tests

### 📈 Performance & Reliability

#### Enhanced Performance
- **Hash Algorithm**: Superior performance with SwiftHash and BLAKE3
- **Mining Efficiency**: Improved mining performance with nonce-hardened matrix
- **Memory Management**: Better memory usage and optimization

#### Reliability Improvements
- **Test Coverage**: Enhanced test stability and coverage
- **Error Handling**: Improved error handling throughout the codebase
- **Dependency Management**: Upgraded to official dependencies for better reliability

---

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
