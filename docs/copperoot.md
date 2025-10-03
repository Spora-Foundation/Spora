# Copperoot: A Modern Taproot Variant

## Overview

Copperoot is an advanced Taproot variant that enhances Bitcoin's Taproot protocol with modern cryptographic primitives and improved performance. It maintains full backward compatibility with existing Taproot functionality while introducing significant improvements in hashing, tree structures, and multi-signature capabilities.

## Script Version Architecture

Copperoot introduces a new script version system that uses distinct version bytes designed to create human-readable address prefixes:

- **SCRIPT_VER_CLASSIC (0)**: Classic script types (PubKey, PubKeyECDSA, ScriptHash)
- **SCRIPT_VER_TAPROOT (88)**: Taproot (BIP341/SHA256) - Addresses start with 't' (0b01011_000)
- **SCRIPT_VER_COPPEROOT_MERKLE (192)**: Pay-to-Copperoot-Merkle (BLAKE3) - Addresses start with 'c' (0b11000_000)
- **SCRIPT_VER_COPPEROOT_VERKLE (96)**: Pay-to-Copperoot-Verkle (BLAKE3) - Reserved, Addresses start with 'v' (0b01100_000)

### Address Prefix Design

The script version numbers are specifically chosen to create intuitive address prefixes:

**Bech32m Character Mapping:**
- Version byte's **first 5 bits** are encoded as the **first character** after HRP
- Taproot: `88 = 0b01011_000` → `0b01011 = 11` → **'t'** in Bech32m charset
- CopperootMerkle: `192 = 0b11000_000` → `0b11000 = 24` → **'c'** in Bech32m charset
- CopperootVerkle: `96 = 0b01100_000` → `0b01100 = 12` → **'v'** in Bech32m charset

**Important Note on Second Character:**
The second character is **not controlled by the version byte alone**. It depends on:
- Version byte's last 3 bits (000 for 88, 192, and 96)
- Public key's first 2 bits

This means Taproot addresses start with 't' but the second character varies (e.g., 'tq', 'tr', 'tp', 'tz'), CopperootMerkle addresses start with 'c' and CopperootVerkle addresses start with 'v' with similar variation.

**Example Addresses:**
```
Taproot (version 88):
  tondi:trazle76u3gwal94drp4qlvlh9vkjddh7mjpv2hhe422xzjsrs8tvca30pn
  └─────┘└┬┘
         │└─ 'razle...' = public key data (Bech32m encoded)
         └── 't' = version 88 (first 5 bits: 0b01011)

CopperootMerkle (version 192):
  tondi:crazle76u3gwal94drp4qlvlh9vkjddh7mjpv2hhe422xzjsrs8tvv5jz65
  └─────┘└┬┘
         │└─ 'razle...' = same public key data (Bech32m encoded)
         └── 'c' = version 192 (first 5 bits: 0b11000)

Note: The 'razle...' portion is identical because both examples use the same public key.
      Different public keys will produce different encodings.

CopperootVerkle (version 2 - currently disabled):
  tondi:q... (starts with 'q', second character varies by public key)
```

### Version Validation and Security

The implementation enforces strict version validation with format checking:

1. **MAX_SCRIPT_PUBLIC_KEY_VERSION (192)**: Hard limit for supported script versions
   - Enforced at consensus layer (transaction validation)
   - Enforced at mempool layer (policy checks)
   - Scripts with versions > 192 are immediately rejected
   - Prevents future version confusion and ensures network-wide consistency

2. **Version-Based Classification**: ScriptClass determination uses version number for initial dispatch

3. **Format Validation**: Each script version requires specific byte pattern validation:
   - **Taproot**: Must match `OP_1 <32-byte x-only pubkey>` format
   - **CopperootMerkle**: Must match `OP_1 <32-byte x-only pubkey>` format  
   - **CopperootVerkle**: Must match `OP_1 <32-byte x-only pubkey>` format (currently disabled)
   - **Classic**: Legacy format validation for PubKey, PubKeyECDSA, ScriptHash

4. **NonStandard Classification**: Scripts with correct version but wrong format are classified as NonStandard

5. **Control Block Consistency**: Control block TLV extensions must match script versions (ProofType 0x00 for Merkle, 0x01 for Verkle)

6. **Mempool Policy**: Unknown versions and NonStandard scripts are rejected at the mempool level

### Why MAX_SCRIPT_PUBLIC_KEY_VERSION is Essential

`MAX_SCRIPT_PUBLIC_KEY_VERSION` serves multiple critical purposes:

**Consensus Safety:**
- Prevents accidental acceptance of future script versions
- Ensures all nodes reject unknown script types consistently
- Avoids consensus splits from version number overflow

**Mempool Protection:**
- Rejects transactions with unknown versions before they enter mempool
- Prevents DOS attacks using invalid script versions
- Ensures clean mempool state with only valid transaction types

**Network Consistency:**
- All nodes enforce the same maximum version
- Future version activations require coordinated network upgrade
- Clear boundary between supported and unsupported versions

**Implementation Use Cases:**
```rust
// Consensus validation - reject unknown versions
if utxo_entry.script_public_key.version() > MAX_SCRIPT_PUBLIC_KEY_VERSION {
    return Err(TxScriptError::InvalidScriptPublicKeyVersion(...));
}

// Mempool policy - reject non-standard versions
if output.script_public_key.version() > MAX_SCRIPT_PUBLIC_KEY_VERSION {
    return Err(NonStandardError::RejectScriptPublicKeyVersion(...));
}

// Testing - create valid transaction outputs
ScriptPublicKey::new(MAX_SCRIPT_PUBLIC_KEY_VERSION, script)
```

Currently: `MAX_SCRIPT_PUBLIC_KEY_VERSION = 192` (CopperootVerkle reserved but disabled)

## Current Implementation Status

Copperoot is currently implemented with the following features:

- ✅ **BLAKE3-256 Hashing**: Complete implementation with domain separation
- ✅ **Merkle Tree Support**: 8-layer depth limit with consensus validation and complete tweak verification
- ✅ **MuSig2 Integration**: Complete two-round protocol implementation with wrapper API and session management
- ✅ **Safe MuSig2 Interface**: Multiple witness creation methods with validation to prevent misuse
- ✅ **Address System**: CopperootMerkle (version 192) and Taproot (version 88) with human-readable 'c' and 't' prefixes
- ✅ **Witness Structure**: Key-path and script-path spending with annex support
- ✅ **Control Blocks**: Merkle proof support with strict validation and enhanced security
- ✅ **Transaction Validation**: Mempool policy checks and standard transaction validation with strict script format checking
- ✅ **ScriptVariant Trait**: Abstract execution semantics for Copperoot implementation
- ✅ **Test Suite**: Comprehensive test coverage including address generation, key spend, script spend validation, and MuSig2 functionality
- ✅ **Script Classification**: Version-based dispatch with format validation for all script types
- ✅ **Enhanced Security**: Comprehensive constraint checking, error handling, and misuse prevention
- ⚠️ **Verkle Trees**: Reserved for future activation (version 2 currently disabled for mainnet)

## Key Features

### 1. BLAKE3-256 Hashing

Copperoot replaces SHA256 with BLAKE3-256 for all cryptographic operations, providing:

- **Superior Performance**: BLAKE3 is significantly faster than SHA256, especially on modern hardware
- **SIMD Optimization**: Native support for SIMD instructions across multiple architectures
- **Domain Separation**: Tagged hash functions ensure cryptographic security across different use cases
- **Parallel Processing**: BLAKE3's tree structure enables parallel hashing of large inputs

#### Domain-Separated Hash Functions

```rust
// Copperoot-specific tagged hashes with domain separation
CopperootSighash(data)     // For signature hash computation
CopperootLeaf(version, script)  // For script leaf hashing
CopperootNode(left, right)      // For Merkle tree nodes
CopperTweak(internal_key, merkle_root)  // For key tweaking

// Additional domain separation tags for sighash computation
Amounts(data)              // For input amounts encoding
ScriptPubKeys(data)        // For script public keys encoding
Prevouts(data)             // For previous outputs encoding
Sequences(data)            // For input sequences encoding
Outputs(data)              // For outputs encoding
```

### 2. Merkle Tree Support

Copperoot implements Merkle tree functionality with:

- **8-Layer Depth Limit**: Strict consensus rule preventing excessive tree depth
- **Binary Branching**: Standard binary tree structure for compatibility
- **BLAKE3-256 Hashing**: All tree operations use BLAKE3-256 for performance
- **Control Block Validation**: Strict validation of control block structure and depth
- **Complete Tweak Validation**: Full verification of Merkle commitment with parity bit validation
- **Enhanced Security**: Comprehensive constraint checking and error handling

### 4. Verkle Tree Support (Future)

Copperoot introduces Verkle tree functionality with TLV-based version isolation for future protocol extensions:

- **8-Layer Depth Limit**: Prevents excessive tree depth while maintaining flexibility
- **256-ary Branching**: Each node can have up to 256 children
- **IPA Commitments**: Uses Inner Product Argument (IPA) for efficient proof generation
- **secp256k1 Compatibility**: Fully compatible with Bitcoin's elliptic curve
- **TLV Version Isolation**: Clear separation from Merkle trees using TLV ProofType
- **Mainnet Disabled**: Currently disabled for mainnet launch, reserved for future activation

#### Verkle Tree Structure

```
Root (Level 0)
├── Child 0x00 (Level 1)
│   ├── Child 0x00 (Level 2)
│   └── Child 0x01 (Level 2)
├── Child 0x01 (Level 1)
└── ... (up to 256 children per node)
```

#### TLV-Based Version Isolation

Verkle trees use TLV extensions to provide clear version isolation and future extensibility:

**Control Block TLV for Verkle**:
```
Control Block Structure (Verkle):
- Parity + Leaf Version (1 byte): BIP341 compatible layout
- Internal Public Key (32 bytes)
- TLV Extensions (variable length):
  - Type 0x01: ProofType = 0x01 (Verkle) - RESERVED
  - Type 0x02: HashScheme = 0x01 (BLAKE3)
  - Type 0x03: TreeScheme = 0x01 (Verkle) - RESERVED
  - Type 0x11: VerkleProof (variable) - Path length + commitments + leaf data
  - Type 0x12-0x1F: Reserved for future Verkle proof types
```

**Future Extensibility**:
- **v4 CopperootKZG**: Potential future version using KZG commitments
- **v5 CopperootPlonk**: Potential future version using PLONK proofs
- **TLV Type Expansion**: New proof types can be added without breaking existing functionality
- **Cross-Chain Isolation**: ChainID/GenesisHash TLV prevents cross-chain replay attacks

#### Benefits

- **Compact Proofs**: Verkle proofs are much smaller than Merkle proofs
- **Efficient Verification**: Faster proof verification compared to traditional Merkle trees
- **Scalability**: Better performance for large script trees
- **Version Isolation**: TLV-based separation prevents confusion between Merkle and Verkle
- **Future-Proof**: Extensible design supports future proof systems without breaking changes

#### When Verkle Trees Become Essential

Verkle trees provide significant advantages over Merkle trees in specific scenarios where Copperoot needs to scale beyond basic Taproot functionality:

**Large State / Contract Storage (Account/VM/RGBX Client State)**:
- **Key-Value Mass Storage**: Verkle's flattened key paths + aggregated proofs enable efficient batch queries (multiple keys proven simultaneously) with reduced volume and easier stateless synchronization
- **Contract Storage**: If Tondi needs to support contract storage or large-scale asset mappings, Verkle trees provide significant value
- **RGBX Client State**: Large RGB state trees benefit from Verkle's compact proofs and efficient verification

**Stateless / Light Client Architecture**:
- **Stateless Execution Design**: Blocks only provide Verkle witnesses for changed keys, allowing full nodes to verify without persisting complete state
- **Client Verification + Anchoring**: Particularly friendly for RGBX/Nexus patterns where mobile light wallets need faster synchronization
- **Reduced Storage Requirements**: Light clients can verify state transitions without downloading entire state trees

**Multi-Proof Aggregation (Batch Spending / Multi-Contract Same Block)**:
- **Proof Deduplication**: Verkle trees can deduplicate/aggregate multiple leaf proofs under the same parent node, resulting in smaller block witnesses
- **Batch Operations**: Suitable for large-scale multi-party multi-contract same-block settlements (exchange custody deposits/withdrawals, rollup settlements, batch channel closures)
- **Cross-Contract Aggregation**: Multiple contracts can share proof components, reducing overall witness size

**Future ZK Integration**:
- **KZG Commitments**: Potential integration with KZG commitments or IPA outer layer + ZK recursion to compress "state correctness" into short proofs
- **L2 Assertion + Main Chain Sampling**: Combination of "L2 assertions + main chain sampling" provides more scalability potential than pure Merkle trees
- **ZK Recursive Proofs**: Verkle structure naturally supports ZK recursive proof systems for enhanced privacy and scalability

### 5. Complete MuSig2 Implementation

Copperoot provides a complete MuSig2 implementation with a wrapper API that maintains compatibility while offering full two-round protocol functionality:

- **Key Aggregation**: Combines multiple public keys into a single aggregated key
- **Two-Round Protocol**: Complete MuSig2 signing protocol implementation with session management
- **Wrapper API**: `MuSig2Session` and `MuSig2Round2` provide a clean interface over the underlying `musig2` crate
- **BIP340 Compatibility**: Uses SHA256 for MuSig2 operations (BIP340 compliant)
- **Session Management**: Built-in first round handling with nonce exchange
- **Address Integration**: `Address::address_from_xonly()` for creating CopperootMerkle addresses from MuSig2 aggregated keys
- **Safe Witness Support**: Multiple witness creation methods with validation to prevent misuse
- **Type Safety**: Automatic handling of secp256k1 version differences between musig2 and project dependencies
- **Complete Workflow**: Full two-round protocol with nonce exchange and partial signature aggregation
- **Production Ready**: Comprehensive test coverage and error handling
- **Misuse Prevention**: Signature validation prevents common misuse patterns like incomplete adaptor signatures

#### MuSig2 Workflow

1. **Key Aggregation**: Combine participant public keys using `KeyAggContext`
2. **Session Creation**: Create `MuSig2Session` with built-in first round handling
3. **Nonce Exchange**: Exchange public nonces between participants using `receive_nonce()`
4. **Round 2 Transition**: Use `finalize_round2()` to enter second round and get partial signatures
5. **Partial Signature Exchange**: Exchange partial signatures using `receive_signature()`
6. **Finalization**: Use `finalize()` to get the aggregated signature
7. **Verification**: Verify the final aggregated signature using BIP340 Schnorr

#### MuSig2 Session Management

The `MuSig2Session` provides a complete wrapper around the underlying `musig2` crate:

```rust
// Create session with built-in first round
let mut session = MuSig2Session::new(
    key_agg,           // KeyAggContext
    my_index,          // Participant index
    seckey,            // Secret key
    msg,               // Message to sign
    nonce_seed         // Nonce seed
)?;

// Exchange nonces
session.receive_nonce(peer_index, peer_nonce)?;

// Enter round 2 and get partial signature
let (mut round2, my_partial) = session.finalize_round2(seckey, msg)?;

// Exchange partial signatures
if let Some(peer_partial) = peer_partial {
    round2.receive_signature(peer_index, peer_partial)?;
}

// Get final aggregated signature
let final_sig = round2.finalize()?;
```

#### Safe MuSig2 Interface

Copperoot provides multiple witness creation methods to prevent common misuse patterns:

**Recommended (with validation)**:
```rust
// Validates signature before constructing witness
let witness = CopperootWitness::p2cr_key_spend_from_musig2_checked(
    &secp, &agg_x, &msg32, &sig, CopperootSighashType::All
)?;
```

**Unchecked (use with caution)**:
```rust
// No validation - caller must ensure correctness
let witness = CopperootWitness::p2cr_key_spend_from_musig2_unchecked(
    schnorr_sig, CopperootSighashType::All
);
```

**Deprecated (backward compatibility)**:
```rust
// Deprecated - use checked version instead
let witness = CopperootWitness::p2cr_key_spend_from_musig2(
    schnorr_sig, CopperootSighashType::All
);
```

#### Safety Features

The `p2cr_key_spend_from_musig2_checked` function prevents common misuse patterns:

- **Signature Validation**: Verifies signature against aggregated public key and message
- **Message Consistency**: Ensures the signature was created with the correct message
- **Adaptor Signature Protection**: Prevents using incomplete adaptor signatures (s̃)
- **Type Safety**: Handles secp256k1 version differences between musig2 and project dependencies
- **Error Handling**: Returns clear error messages for validation failures

#### Common Misuse Prevention

1. **Wrong Public Key**: Signature must match the aggregated x-only public key
2. **Wrong Message**: Signature must be created with the correct CopperootSighash
3. **Incomplete Adaptor Signatures**: Only final signatures (not s̃) are accepted
4. **Sighash Mismatch**: Sighash type must match the message used for signing
5. **Version Conflicts**: Automatic handling of secp256k1 version differences

### 6. TLV (Type-Length-Value) Extensibility Framework

Copperoot introduces a comprehensive TLV framework that provides forward compatibility and extensibility for future protocol enhancements. While TLV is not required for basic Taproot functionality, it becomes essential for Copperoot's advanced features and future extensions like Verkle trees and RGB integration.

#### Why TLV is Essential for Copperoot

**Extensibility and Future-Proofing**:
- Taproot's witness and control block formats are fixed, with BIP341 providing no reserved fields for future upgrades
- Adding new proof types (Verkle), additional commitments (RGB roots), chain ID isolation, or DA reference hashes without TLV would require hardcoding in fixed byte positions, leading to conflicts
- TLV enables graceful handling: unknown fields are ignored, known fields are strictly validated, allowing mainnet upgrades where old nodes "compatibly reject" while new nodes "fully validate" without causing forks

**Version Isolation**:
- Current versions include v2 CopperootMerkle and v3 CopperootVerkle, with potential future v4 CopperootKZG
- Using "first byte differentiation" becomes increasingly complex and error-prone
- TLV allows clear specification in control blocks: `type=ProofType, value=Merkle/Verkle/KZG`, while also carrying `SchemeID`, `RootHash`, and `Namespace` information

**Cross-Layer Information Carrying (RGB Integration)**:
- RGB commitments need to be attached to transactions, but transaction scripts only understand "Taproot spending"
- TLV provides a clean mechanism to inform clients that a transaction anchors `RGBX_ROOT = blake3(...)`
- Wallets and light clients can read TLV to quickly identify RGB transactions and fetch proofs from indexers or P2P networks
- Nodes that don't support RGB can ignore unknown TLV fields while still validating Merkle/Taproot components

**Security and Future-Proofing**:
- TLV is inherently a parse-safe format: read type, then length, then value, making errors easily detectable
- Prevents data misalignment where fields are incorrectly interpreted as other components
- Future extensions like annex enhancements, Verkle proofs, or batch proof pointers can be added by defining new types without polluting original fields

#### Enhanced Control Block Structure

Copperoot extends the control block to support both Merkle and Verkle proofs with TLV extensions:

```
Control Block Structure (TLV Format):
- Parity + Leaf Version (1 byte): BIP341 compatible layout
- Internal Public Key (32 bytes)
- TLV Extensions (variable length):
  - Type 0x01: ProofType (1 byte) - 0x00 (Merkle) or 0x01 (Verkle - RESERVED)
  - Type 0x02: HashScheme (1 byte) - 0x00 (SHA256) or 0x01 (BLAKE3)
  - Type 0x03: TreeScheme (1 byte) - 0x00 (Merkle) or 0x01 (Verkle - RESERVED)
  - Type 0x04: ChainID/GenesisHash (32 bytes) - Cross-chain replay protection
  - Type 0x10: MerkleProof (variable) - Path length + sibling hashes
  - Type 0x11: VerkleProof (variable) - Path length + commitments + leaf data
  - Type 0x12-0x1F: Reserved for future proof types
  - Unknown TLV types must be ignored (forward compatibility)
```

**Mainnet Launch Status**:
- Proof Type 0x00: Active for CopperootMerkle (Merkle tree)
- Proof Type 0x01: Reserved for CopperootVerkle (Verkle tree) - INACTIVE
- Annex Type V: Reserved for Verkle proofs - INACTIVE

#### TLV Encoding Specification

The TLV (Type-Length-Value) format provides forward compatibility and extensibility:

```
TLV Structure:
- Type (1 byte): TLV type identifier
- Length (varint): Length of value field
- Value (variable): TLV payload

Control Block TLV Types:
- 0x01: ProofType (1 byte) - 0x00=Merkle, 0x01=Verkle(RESERVED)
- 0x02: HashScheme (1 byte) - 0x00=SHA256, 0x01=BLAKE3
- 0x03: TreeScheme (1 byte) - 0x00=Merkle, 0x01=Verkle(RESERVED)
- 0x04: ChainID/GenesisHash (32 bytes) - Cross-chain replay protection
- 0x10: MerkleProof (variable) - Path length (1 byte) + sibling hashes (32 bytes each)
- 0x11: VerkleProof (variable) - Path length (1 byte) + commitments (33 bytes each) + leaf data
- 0x12-0x1F: Reserved for future proof types
- Unknown types: Must be ignored (forward compatibility)
```

#### Annex TLV Extensions

The BIP341 annex provides a "free zone" for additional data that doesn't affect consensus validation. Copperoot leverages this for RGB integration and other cross-layer information:

```
Annex TLV Structure (BIP341 Annex):
- Annex Marker: 0x50 (required prefix)
- TLV Extensions (variable length):
  - Type 0x20: RGBX_ROOT (32 bytes) - RGB commitment root hash
  - Type 0x21: RGBX_PROOF_REF (variable) - Cartridge CID/hash reference
  - Type 0x22: StateEpoch (8 bytes) - RGB state epoch identifier
  - Type 0x23: Namespace (variable) - RGB namespace identifier
  - Type 0x24: BatchProofRef (variable) - Reference to batch proof data
  - Type 0x25: DAHash (32 bytes) - Data availability reference hash
  - Type 0x26-0x2F: Reserved for RGB extensions
  - Type 0x30-0x3F: Reserved for future protocol extensions
  - Unknown types: Must be ignored (forward compatibility)
```

#### Witness TLV Extensions

Witness TLV extensions can carry additional parameters after script input parameters:

```
Witness TLV Structure (after script inputs):
- Type 0x40: MultiSigAggParams (variable) - Multi-signature aggregation parameters
- Type 0x41: VersionFlags (4 bytes) - Version and feature flags
- Type 0x42: ProofFragments (variable) - Aggregated proof fragments
- Type 0x43: BatchCommitment (32 bytes) - Batch operation commitment
- Type 0x44-0x4F: Reserved for witness extensions
- Unknown types: Must be ignored (forward compatibility)
```

#### Test Vectors

**Test Vector 1: CopperootMerkle Key Spend**
```
ScriptPubKey: 5120<32-byte x-only pubkey>
Witness: [64-byte signature]
Address: tondi:c... (v2 witness version)
```

**Test Vector 2: CopperootMerkle Script Spend with Merkle Proof**
```
ScriptPubKey: 5120<32-byte x-only pubkey>
Witness: [input_items..., script, control_block]
Control Block: [parity_leaf_version(1)] [internal_key(32)] [TLV_extensions...]
TLV Extensions:
  - Type 0x01, Length 1, Value 0x00 (Merkle)
  - Type 0x02, Length 1, Value 0x01 (BLAKE3)
  - Type 0x10, Length 65, Value [path_len(1)] [sibling_hashes(32*2)]
```

**Test Vector 3: CopperootMerkle with Annex**
```
Witness: [signature, annex]
Annex: [0x50, ...] (must start with 0x50, positioned as second-to-last)
```

**Test Vector 4: Unknown TLV Ignored (Forward Compatibility)**
```
Control Block: [parity_leaf_version(1)] [internal_key(32)] [TLV_extensions...]
TLV Extensions:
  - Type 0x01, Length 1, Value 0x00 (Merkle) - recognized
  - Type 0xFF, Length 4, Value [0x12, 0x34, 0x56, 0x78] - ignored
  - Type 0x10, Length 33, Value [path_len(1)] [sibling_hash(32)] - recognized
```

**Test Vector 5: Invalid Control Block (Missing Required TLV)**
```
Control Block: [parity_leaf_version(1)] [internal_key(32)] [TLV_extensions...]
TLV Extensions:
  - Type 0x02, Length 1, Value 0x01 (BLAKE3) - missing ProofType
Result: INVALID (ProofType 0x01 is required)
```

**Test Vector 6: RGB Integration with Annex TLV**
```
Witness: [signature, annex]
Annex: [0x50, TLV_extensions...]
TLV Extensions:
  - Type 0x20, Length 32, Value [RGBX_ROOT_HASH] - RGB commitment root
  - Type 0x22, Length 8, Value [STATE_EPOCH] - RGB state epoch
  - Type 0x23, Length 16, Value [NAMESPACE_ID] - RGB namespace
Result: VALID - RGB-aware nodes process RGB data, others ignore TLV
```

**Test Vector 7: Forward Compatibility with Unknown TLV**
```
Control Block: [parity_leaf_version(1)] [internal_key(32)] [TLV_extensions...]
TLV Extensions:
  - Type 0x01, Length 1, Value 0x00 (Merkle) - recognized
  - Type 0xFF, Length 4, Value [0x12, 0x34, 0x56, 0x78] - unknown, ignored
  - Type 0x10, Length 33, Value [path_len(1)] [sibling_hash(32)] - recognized
Result: VALID - unknown TLV types are ignored for forward compatibility
```

### 7. RGB Integration with TLV Framework

Copperoot's TLV framework provides a clean mechanism for RGB (Red-Green-Blue) protocol integration, enabling cross-layer information carrying without modifying Tondi's consensus rules.

#### RGB on Tondi Integration Points

**Key RGB Requirements**:
- Multi-asset and multi-state key commitments (batch transfers, AMM, NFT)
- Light client synchronization (stateless verification)
- Cross-contract aggregated proofs
- Clean separation from Bitcoin/Taproot consensus

**Copperoot TLV Solution**:
- **Merkle Trees**: Small state (single transfers) → short proofs, suitable for TLV-carrying RGBX_ROOT, mainnet only anchors the root
- **Verkle Trees**: Large state (AMM, batch withdrawals) → aggregatable proofs, TLV carries RGBX_ROOT + ProofType=Verkle, client-side validation
- **TLV Value**: RGB can attach commitments and proof references without changing Tondi consensus rules, providing clean isolation without polluting BTC/Taproot core

#### RGB TLV Usage Examples

**Small State RGB Transaction (Merkle)**:
```
Control Block TLV:
  - Type 0x01: ProofType = 0x00 (Merkle)
  - Type 0x02: HashScheme = 0x01 (BLAKE3)
  - Type 0x10: MerkleProof = [path_len, sibling_hashes...]

Annex TLV:
  - Type 0x20: RGBX_ROOT = blake3(rgb_state_root)
  - Type 0x22: StateEpoch = current_epoch
  - Type 0x23: Namespace = rgb_contract_namespace

Result: Mainnet validates Merkle proof, RGB clients process RGB data
```

**Large State RGB Transaction (Verkle - Future)**:
```
Control Block TLV:
  - Type 0x01: ProofType = 0x01 (Verkle) - RESERVED
  - Type 0x02: HashScheme = 0x01 (BLAKE3)
  - Type 0x11: VerkleProof = [path_len, commitments, leaf_data...]

Annex TLV:
  - Type 0x20: RGBX_ROOT = blake3(aggregated_rgb_state)
  - Type 0x21: RGBX_PROOF_REF = cartridge_cid
  - Type 0x24: BatchProofRef = batch_proof_identifier

Result: Verkle-aware nodes validate proofs, RGB clients process batch data
```

#### RGB Client Integration

**Wallet/Light Client Workflow**:
1. **Transaction Detection**: Parse TLV to identify RGB transactions
2. **RGB Data Extraction**: Extract RGBX_ROOT, StateEpoch, Namespace from annex TLV
3. **Proof Fetching**: Use RGBX_PROOF_REF to fetch proofs from RGB indexers or P2P networks
4. **State Validation**: Verify RGB state transitions against RGBX_ROOT commitment
5. **Cross-Layer Verification**: Ensure RGB state consistency with Copperoot proof validation

**Node Compatibility**:
- **RGB-Unaware Nodes**: Ignore unknown TLV types, validate only Merkle/Taproot components
- **RGB-Aware Nodes**: Process RGB TLV data, perform cross-layer validation
- **RGB-Only Clients**: Focus on RGB TLV data, rely on nodes for Copperoot validation

#### RGB Protocol Benefits

**Clean Separation**:
- RGB commitments don't affect Tondi consensus validation
- RGB proofs can be fetched and validated independently
- RGB state transitions are isolated from Bitcoin/Taproot logic

**Scalability**:
- Small RGB operations use Merkle trees with TLV-carried roots
- Large RGB operations use Verkle trees with aggregatable proofs
- Batch RGB operations can reference external proof data

**Future Extensibility**:
- New RGB features can be added via new TLV types
- RGB protocol upgrades don't require Tondi consensus changes
- Multiple RGB implementations can coexist using different TLV types

#### Future Scenarios: When Verkle Trees Become Essential

The TLV framework enables Copperoot to evolve from Merkle-based proofs to Verkle-based proofs when specific scalability requirements emerge:

**Scenario 1: Large-Scale RGB State Management**
```
Current (Merkle): Single RGB transfer
- Control Block: ProofType=0x00 (Merkle), MerkleProof=[path, siblings]
- Annex: RGBX_ROOT=blake3(single_transfer_state)
- Proof Size: ~1KB for single transfer

Future (Verkle): Batch RGB operations
- Control Block: ProofType=0x01 (Verkle), VerkleProof=[path, commitments, leaf_data]
- Annex: RGBX_ROOT=blake3(batch_state), BatchProofRef=batch_id
- Proof Size: ~2KB for 100+ transfers (10-100x more efficient)
```

**Scenario 2: Contract Storage and Account State**
```
Current (Merkle): Simple script execution
- Control Block: ProofType=0x00 (Merkle), MerkleProof=[path, siblings]
- Witness: Script inputs, control block
- State: Minimal, script-local

Future (Verkle): Contract account state
- Control Block: ProofType=0x01 (Verkle), VerkleProof=[path, commitments, leaf_data]
- Annex: AccountState=blake3(contract_storage), StateEpoch=current_epoch
- State: Large key-value storage with efficient batch updates
```

**Scenario 3: Stateless Light Client Synchronization**
```
Current (Merkle): Full state download required
- Light Client: Must download entire Merkle tree for verification
- Storage: O(n) where n = total state size
- Sync Time: Hours for large state

Future (Verkle): Stateless verification
- Light Client: Only downloads changed key witnesses
- Storage: O(k) where k = number of changed keys
- Sync Time: Minutes for large state changes
```

**Scenario 4: Multi-Contract Batch Settlement**
```
Current (Merkle): Individual contract proofs
- Exchange Settlement: 1000 individual Merkle proofs
- Total Proof Size: 1000 × 1KB = 1MB
- Verification: 1000 individual verifications

Future (Verkle): Aggregated batch proofs
- Exchange Settlement: 1 aggregated Verkle proof
- Total Proof Size: 10KB (100x reduction)
- Verification: 1 aggregated verification
```

**Scenario 5: ZK-Enhanced State Verification**
```
Current (Merkle): Direct proof verification
- Verification: Direct Merkle path verification
- Privacy: No privacy guarantees
- Scalability: Linear with state size

Future (Verkle + ZK): Compressed state proofs
- Verification: ZK proof of Verkle path correctness
- Privacy: Zero-knowledge state transitions
- Scalability: Constant proof size regardless of state size
```

#### Migration Path: Merkle to Verkle

The TLV framework provides a smooth migration path from Merkle to Verkle trees:

**Phase 1: Merkle-Only (Current)**
- All transactions use ProofType=0x00 (Merkle)
- Simple RGB operations with TLV-carried roots
- Standard Taproot-compatible verification

**Phase 2: Hybrid Support (Future)**
- Nodes support both Merkle and Verkle proofs
- TLV-based proof type selection
- Backward compatibility maintained

**Phase 3: Verkle-Dominant (Future)**
- Large-scale operations default to Verkle
- Merkle reserved for simple operations
- Optimized for batch and contract operations

**Phase 4: ZK-Enhanced (Future)**
- ZK proofs over Verkle commitments
- Maximum privacy and scalability
- L2 integration and cross-chain compatibility

### 7. ScriptVariant Trait Implementation

Copperoot implements the `ScriptVariant` trait to provide abstract execution semantics:

- **Abstract Interface**: Reusable execution flow while parameterizing hash functions and verification logic
- **Witness Parsing**: Automatic witness structure parsing from signature scripts
- **Commitment Verification**: Script path spending with Merkle proof validation
- **Sighash Computation**: Key spend signature hash computation using BLAKE3-256
- **Component Extraction**: Automatic extraction of signatures and script components from witnesses
- **Type Safety**: Generic implementation supporting different Taproot-like script variants

Note: Previously named `TapLike`, renamed to `ScriptVariant` to avoid confusion with Bitcoin's Taproot implementation.

## Technical Implementation

### SIGHASH Computation

Copperoot uses BLAKE3-256 for all signature hash computations:

```rust
pub fn copperoot_key_spend_signature_hash(
    &mut self,
    input_index: usize,
    prevouts: &Prevouts<TxOut>,
    sighash_type: CopperootSighashType,
) -> Result<CopperootSighash, CopperootError>
```

### Witness Structure

Copperoot witnesses support both key-path and script-path spending with strict validation:

```rust
pub struct CopperootWitness {
    inner: BtcWitness,           // Bitcoin witness
    verkle_proof: Option<VerkleProof>,  // Optional Verkle proof (reserved)
}

// Key-path spending methods
impl CopperootWitness {
    pub fn p2cr_key_spend(signature: Signature, sighash_type: CopperootSighashType) -> Self
    pub fn p2cr_key_spend_with_annex(signature: Signature, sighash_type: CopperootSighashType, annex: Vec<u8>) -> Result<Self, TxScriptError>
    
    // MuSig2 witness creation methods (with safety features)
    pub fn p2cr_key_spend_from_musig2_checked(
        secp: &Secp256k1<All>, 
        agg_x: &XOnlyPublicKey, 
        msg32: &[u8; 32], 
        sig: &musig2::CompactSignature, 
        sighash_type: CopperootSighashType
    ) -> Result<Self, TxScriptError>
    pub fn p2cr_key_spend_from_musig2_unchecked(signature: Signature, sighash_type: CopperootSighashType) -> Self
    #[deprecated] pub fn p2cr_key_spend_from_musig2(signature: Signature, sighash_type: CopperootSighashType) -> Self
    
    pub fn verify_keypath_sig(&self, msg: &Message, xpub: &XOnlyPublicKey) -> Result<(), secp256k1::Error>
}

// Script-path spending methods
impl CopperootWitness {
    pub fn p2cr_script_spend_with_inputs(input_items: Vec<Vec<u8>>, script: Vec<u8>, control_block: Vec<u8>, annex: Option<Vec<u8>>) -> Self
}
```

### Script Path Spending

Copperoot supports script path spending with Merkle proofs (Verkle proofs reserved):

```rust
// Merkle tree script path spending (active)
pub fn p2cr_script_spend_with_inputs(
    input_items: Vec<Vec<u8>>,
    script: Vec<u8>,
    control_block: Vec<u8>,
    annex: Option<Vec<u8>>,
) -> Self

// Verkle tree script path spending (reserved for future)
#[cfg(feature = "verkle")]
pub fn p2crv_script_spend_with_verkle(
    script: Vec<u8>,
    control_block: Vec<u8>,
    verkle_proof: VerkleProof,
) -> Self
```

## Compatibility

### Backward Compatibility

Copperoot maintains full backward compatibility with existing Taproot:

- **Existing Taproot Addresses**: Continue to work without modification
- **Standard Taproot Scripts**: Fully supported
- **Witness Formats**: Compatible with existing witness structures

### Migration Path

1. **Gradual Adoption**: New addresses can use Copperoot features
2. **Hybrid Support**: Nodes can support both Taproot and Copperoot
3. **Feature Detection**: Control block version identifies Copperoot transactions

## Performance Benefits

### Hashing Performance

- **BLAKE3 vs SHA256**: 2-4x faster on modern hardware
- **SIMD Acceleration**: Additional 2-3x speedup on supported CPUs
- **Parallel Processing**: Scales with multiple cores

### Tree Operations

- **Verkle Proofs**: 10-100x smaller than Merkle proofs
- **Verification Speed**: 5-10x faster proof verification
- **Memory Usage**: Reduced memory footprint for large trees

### Multi-Signature

- **MuSig2 Implementation**: Two-round signing protocol with standard implementation
- **Batch Verification**: Verify multiple signatures together
- **Reduced Bandwidth**: Smaller signature sizes

## Security Considerations

### Cryptographic Security

- **BLAKE3 Security**: 256-bit security level, same as SHA256
- **Domain Separation**: Prevents cross-protocol attacks
- **Verkle Tree Security**: Based on well-studied polynomial commitments

### Implementation Security

- **Constant-Time Operations**: All cryptographic operations are constant-time
- **Input Validation**: Comprehensive input validation and error handling
- **Memory Safety**: Rust's memory safety guarantees
- **MuSig2 Safety**: Signature validation prevents common misuse patterns
- **Type Safety**: Automatic handling of secp256k1 version differences
- **Error Handling**: Clear error messages for validation failures

## Use Cases

### 1. High-Performance Applications

- **Exchange Systems**: Faster transaction processing
- **Payment Processors**: Reduced latency for payment verification
- **DeFi Protocols**: Efficient multi-signature operations

### 2. Scalable Script Systems

- **Complex Smart Contracts**: Large script trees with efficient proofs
- **Multi-Party Protocols**: Advanced multi-signature schemes
- **Privacy-Enhanced Transactions**: Verkle trees for privacy-preserving proofs

### 3. Enterprise Solutions

- **Corporate Wallets**: Multi-signature corporate accounts
- **Custody Solutions**: Secure multi-party custody systems
- **Institutional Trading**: High-performance trading infrastructure

## Implementation Status

### Completed Features

- ✅ BLAKE3-256 hashing with domain separation
- ✅ Merkle tree implementation (8-layer depth limit with consensus validation and complete tweak verification)
- ✅ Complete MuSig2 implementation with wrapper API and two-round protocol
- ✅ Safe MuSig2 witness creation with validation and misuse prevention
- ✅ Enhanced control block structure with strict validation
- ✅ Backward compatibility with Taproot
- ✅ CopperootMerkle address support with bech32m encoding
- ✅ Annex support with strict validation (0x50 prefix, position as second-to-last witness item)
- ✅ Key-path and script-path spending
- ✅ Comprehensive test coverage including MuSig2 wrapper functionality
- ✅ MuSig2 misuse prevention and safety features
- ✅ Mempool policy checks and standard transaction validation
- ✅ Strict script format validation for all script versions
- ✅ ScriptVariant trait implementation for abstract execution semantics
- ✅ Version-based script classification with format checking
- ✅ Enhanced Merkle commitment verification with parity bit validation
- ✅ P2CrSpend enum for structured witness parsing
- ✅ Complete witness serialization and deserialization

### Reserved Features (Future Activation)

- ⚠️ CopperootVerkle address support (disabled for mainnet)
- ⚠️ Verkle tree implementation (reserved for future activation)
- ⚠️ Verkle proof support in control blocks

### Future Enhancements

- 🔄 CopperootVerkle (Verkle) activation for mainnet
- 🔄 Advanced Verkle tree optimizations
- 🔄 Additional MuSig2 optimizations
- 🔄 Performance benchmarking
- 🔄 Security audit and formal verification

### Future Scenarios and Use Cases

#### Large-Scale State Management

**Contract Storage and Account State**:
- **Current**: Simple script execution with minimal state
- **Future**: Large key-value storage with efficient batch updates using Verkle trees
- **Benefits**: 10-100x more efficient proof generation for contract state changes
- **Use Cases**: DeFi protocols, NFT marketplaces, gaming contracts

**RGB State Management**:
- **Current**: Single RGB transfers with Merkle proofs (~1KB per transfer)
- **Future**: Batch RGB operations with Verkle proofs (~2KB for 100+ transfers)
- **Benefits**: Massive reduction in proof size for batch operations
- **Use Cases**: Exchange settlements, payment processors, batch asset transfers

#### Stateless Architecture

**Light Client Synchronization**:
- **Current**: Full state download required (O(n) storage, hours sync time)
- **Future**: Stateless verification with changed key witnesses only (O(k) storage, minutes sync time)
- **Benefits**: Mobile wallets can sync quickly without downloading entire state
- **Use Cases**: Mobile RGB wallets, IoT devices, embedded systems

**Client Verification + Anchoring**:
- **Current**: Full node dependency for state verification
- **Future**: Light clients can verify state transitions independently
- **Benefits**: Reduced infrastructure requirements, improved decentralization
- **Use Cases**: RGBX/Nexus patterns, cross-chain bridges, L2 state verification

#### Batch Operations and Aggregation

**Multi-Contract Batch Settlement**:
- **Current**: Individual contract proofs (1000 × 1KB = 1MB total)
- **Future**: Aggregated batch proofs (10KB total, 100x reduction)
- **Benefits**: Massive reduction in block space usage for batch operations
- **Use Cases**: Exchange custody deposits/withdrawals, rollup settlements, batch channel closures

**Cross-Contract Aggregation**:
- **Current**: Each contract requires separate proof verification
- **Future**: Multiple contracts can share proof components
- **Benefits**: Reduced verification overhead, improved block throughput
- **Use Cases**: Multi-protocol DeFi operations, cross-contract interactions

#### ZK Integration and Privacy

**ZK-Enhanced State Verification**:
- **Current**: Direct proof verification with no privacy guarantees
- **Future**: ZK proofs of Verkle path correctness with zero-knowledge state transitions
- **Benefits**: Privacy-preserving state transitions with constant proof size
- **Use Cases**: Private DeFi, confidential transactions, privacy-preserving smart contracts

**L2 Assertion + Main Chain Sampling**:
- **Current**: Linear scalability with state size
- **Future**: Constant proof size regardless of state size with L2 integration
- **Benefits**: Unlimited scalability with main chain security guarantees
- **Use Cases**: High-throughput L2s, cross-chain state synchronization, enterprise applications

#### Performance and Scalability

**Proof Size Comparison**:
- **Merkle**: ~1KB per proof, linear scaling with tree depth
- **Verkle**: ~2KB for 100+ operations, logarithmic scaling with tree size
- **ZK-Verkle**: ~5KB constant size regardless of state size

**Verification Speed**:
- **Merkle**: O(log n) verification time
- **Verkle**: O(log n) verification time with better constants
- **ZK-Verkle**: O(1) verification time with precomputed parameters

**Storage Requirements**:
- **Merkle**: Full state tree required for verification
- **Verkle**: Only changed key witnesses required
- **ZK-Verkle**: Only commitment and proof required

## Getting Started

### Basic Usage

```rust
use tondi_txscript::standard::copperoot::CopperootWitness;
use tondi_consensus_core::tx::copperoot::sighash::CopperootSighashType;
use tondi_addresses::{Address, Prefix};
use secp256k1::{Secp256k1, Keypair, Message};

// Create a CopperootMerkle address from x-only public key
let secp = Secp256k1::new();
let keypair = Keypair::new(&secp, &mut secp256k1::rand::thread_rng());
let xonly_pubkey = keypair.x_only_public_key().0;
let address = Address::address_from_xonly(Prefix::Mainnet, &xonly_pubkey.serialize())?;

// Create a Copperoot key spend witness
let message = Message::from_digest_slice(&[0u8; 32]).unwrap();
let signature = secp.sign_schnorr(&message, &keypair);
let witness = CopperootWitness::p2cr_key_spend(signature, CopperootSighashType::All);

// Create a key spend witness with annex
let annex = vec![0x50, 0x01, 0x02, 0x03]; // Must start with 0x50
let witness = CopperootWitness::p2cr_key_spend_with_annex(signature, CopperootSighashType::All, annex)?;

// Create a script spend witness
let witness = CopperootWitness::p2cr_script_spend_with_inputs(
    input_items,
    script,
    control_block,
    Some(annex), // Optional annex
);
```

### Advanced Features

```rust
use musig2::{KeyAggContext, FirstRound, SecNonceSpices, CompactSignature};
use tondi_txscript::standard::copperoot::CopperootWitness;
use tondi_consensus_core::tx::copperoot::sighash::CopperootSighashType;
use tondi_addresses::{Address, Prefix};
use secp256k1::{Secp256k1, XOnlyPublicKey, Message};
use musig2::secp256k1::{Keypair, PublicKey as MuPubKey, SecretKey};

// MuSig2 workflow with safe witness creation
// 1. Aggregate public keys
let secp = Secp256k1::new();
let pubkeys = vec![MuPubKey::from(keypair1), MuPubKey::from(keypair2)];
let key_agg = KeyAggContext::new(pubkeys)?;
let agg_pk = key_agg.aggregated_pubkey();
let agg_xonly = XOnlyPublicKey::from_slice(&agg_pk.x_only_public_key().0.serialize())?;

// 2. Create CopperootMerkle address from aggregated key
let address = Address::address_from_xonly(Prefix::Mainnet, &agg_xonly.serialize())?;

// 3. MuSig2 signing process (two rounds)
// Create sessions with built-in first round
let mut s1 = MuSig2Session::new(key_agg.clone(), 0, &sk1, msg, NonceSeed([0; 32]))?;
let mut s2 = MuSig2Session::new(key_agg.clone(), 1, &sk2, msg, NonceSeed([1; 32]))?;

// Exchange nonces
s1.receive_nonce(1, s2.our_public_nonce())?;
s2.receive_nonce(0, s1.our_public_nonce())?;

// Enter round 2 and exchange partial signatures
let (mut r2_1, part1) = s1.finalize_round2(&sk1, msg)?;
let (mut r2_2, part2) = s2.finalize_round2(&sk2, msg)?;

if let Some(p2) = part2 { r2_1.receive_signature(1, p2)?; }
if let Some(p1) = part1 { r2_2.receive_signature(0, p1)?; }

let sig: CompactSignature = r2_1.finalize()?;

// 4. Create witness with validation (RECOMMENDED)
let msg32 = copperoot_sighash.to_byte_array();
let witness = CopperootWitness::p2cr_key_spend_from_musig2_checked(
    &secp, &agg_xonly, &msg32, &sig, CopperootSighashType::All
)?;

// 5. Verify signature (optional - already validated in step 4)
let message = Message::from_digest_slice(&msg32).unwrap();
let result = witness.verify_keypath_sig(&message, &agg_xonly)?;
```

#### MuSig2 Safety Best Practices

**Always use the checked version for production**:
```rust
// ✅ RECOMMENDED: Validates signature before witness creation
let witness = CopperootWitness::p2cr_key_spend_from_musig2_checked(
    &secp, &agg_x, &msg32, &sig, CopperootSighashType::All
)?;
```

**Only use unchecked version if you've already validated elsewhere**:
```rust
// ⚠️ USE WITH CAUTION: No validation performed
// Ensure signature is valid for agg_x and msg32
let witness = CopperootWitness::p2cr_key_spend_from_musig2_unchecked(
    schnorr_sig, CopperootSighashType::All
);
```

**Avoid the deprecated version**:
```rust
// ❌ DEPRECATED: Will show compiler warning
let witness = CopperootWitness::p2cr_key_spend_from_musig2(
    schnorr_sig, CopperootSighashType::All
);
```

## CopperootMerkle and CopperootVerkle Address Specifications

### Overview

Copperoot introduces two distinct address types to ensure clear protocol separation and optimal performance:

- **CopperootMerkle**: Uses traditional Merkle trees for script verification - **ACTIVE**
- **CopperootVerkle**: Uses Verkle trees for efficient proof generation - **RESERVED FOR FUTURE**

This separation provides:

- **Protocol Isolation**: Clear distinction from Bitcoin Taproot addresses
- **Consensus Safety**: Prevents cross-chain transaction replay attacks
- **Performance Optimization**: Nodes can optimize for specific tree types
- **Wallet Compatibility**: Explicit address type identification for wallets and SDKs
- **Future Extensibility**: Independent namespace for each tree type

### Address Format

#### ScriptPubKey Structure

```
OP_1 <32-byte x-only public key>
```

The ScriptPubKey format is identical to Bitcoin's Taproot, but the witness version and validation rules differ.

#### Witness Versions

- **CopperootMerkle Version**: `v2` (decimal 2) - Pay-to-Copperoot-Merkle (ACTIVE)
- **CopperootVerkle Version**: `v3` (decimal 3) - Pay-to-Copperoot-Verkle (RESERVED - INACTIVE)
- **Reserved for**: Tondi Copperoot protocol
- **Separation**: Ensures no overlap with Bitcoin Taproot (v1) or other protocols
- **Mainnet Launch**: Only CopperootMerkle (v2) is active; CopperootVerkle (v3) is reserved for future activation

#### Address Encoding

**Bech32m Encoding**:
- **HRP (Human Readable Part)**:
  - Mainnet: `tondi`
  - Testnet: `tonditest`
  - Simnet: `tondisim`
  - Devnet: `tondidev`
- **Data Part**: `v2` (CopperootMerkle) or `v3` (CopperootVerkle) + 32-byte x-only public key
- **Checksum**: Bech32m checksum algorithm

**Example Addresses**:
```
CopperootMerkle - ACTIVE:
Mainnet:  tondi:c... (CopperootMerkle addresses start with 'c' after HRP)
Testnet:  tonditest:c...
Simnet:   tondisim:c...
Devnet:   tondidev:c...

CopperootVerkle - RESERVED (INACTIVE):
Mainnet:  tondi:q... (CopperootVerkle addresses start with 'q' after HRP)
Testnet:  tonditest:q...
Simnet:   tondisim:q...
Devnet:   tondidev:q...
NOTE: CopperootVerkle addresses are reserved but currently invalid for mainnet launch
```

### Address Type Mapping

| Address Type | Script Type | Hash Function | Tree Structure | Use Case |
|--------------|-------------|---------------|----------------|----------|
| **P2PKH** | Legacy | SHA256 | N/A | Legacy key spends |
| **P2WPKH** | SegWit v0 | SHA256 | N/A | SegWit key spends |
| **P2TR** | Bitcoin Taproot | SHA256 | Merkle Tree | Bitcoin Taproot |
| **CopperootMerkle** | Tondi Copperoot | BLAKE3-256 | Merkle Tree | Tondi Copperoot (Merkle) |
| **CopperootVerkle** | Tondi Copperoot | BLAKE3-256 | Verkle Tree | Tondi Copperoot (Verkle) - RESERVED |

### Implementation Details

#### Address Validation

```rust
use tondi_addresses::{Address, AddressError};

// CopperootMerkle address validation
pub fn validate_p2cr_address(address: &str) -> Result<Address, AddressError> {
    let addr = Address::try_from(address)?;
    if addr.version != Version::CopperootMerkle {
        return Err(AddressError::InvalidVersion(addr.version as u8));
    }
    Ok(addr)
}

// CopperootVerkle address validation (currently disabled)
pub fn validate_p2crv_address(address: &str) -> Result<Address, AddressError> {
    let addr = Address::try_from(address)?;
    if addr.version != Version::CopperootVerkle {
        return Err(AddressError::InvalidVersion(addr.version as u8));
    }
    // CopperootVerkle is disabled for mainnet launch
    Err(AddressError::InvalidVersion(addr.version as u8))
}
```

#### Script Generation

```rust
use tondi_txscript::{Script, opcodes::codes::OP_1};
use secp256k1::XOnlyPublicKey;

// Generate CopperootMerkle ScriptPubKey
pub fn create_p2cr_scriptpubkey(public_key: &XOnlyPublicKey) -> Script {
    let mut script = Script::new();
    script.push_opcode(OP_1);
    script.push_slice(&public_key.serialize());
    script
}

// Generate CopperootVerkle ScriptPubKey (reserved for future)
pub fn create_p2crv_scriptpubkey(public_key: &XOnlyPublicKey) -> Script {
    let mut script = Script::new();
    script.push_opcode(OP_1);
    script.push_slice(&public_key.serialize());
    script
}
```

#### Witness Structure

```rust
use tondi_txscript::standard::copperoot::{CopperootWitness, CopperootControlBlock};
use tondi_consensus_core::tx::copperoot::sighash::CopperootSighashType;
use secp256k1::schnorr::Signature;

// CopperootMerkle witness for key path spending
pub struct CopperootMerkleWitness {
    signature: Signature,
    sighash_type: CopperootSighashType,
}

// CopperootMerkle witness for script path spending (Merkle tree)
pub struct CopperootMerkleScriptWitness {
    script: Vec<u8>,
    control_block: CopperootControlBlock,
}

// CopperootVerkle witness for key path spending (reserved)
pub struct CopperootVerkleWitness {
    signature: Signature,
    sighash_type: CopperootSighashType,
}

// CopperootVerkle witness for script path spending (Verkle tree - reserved)
pub struct CopperootVerkleScriptWitness {
    script: Vec<u8>,
    control_block: CopperootControlBlock,
    verkle_proof: VerkleProof,
}
```

### Security Considerations

#### Cross-Chain Protection

- **Version Isolation**: Witness versions v2 (CopperootMerkle) and v3 (CopperootVerkle) prevent confusion with Bitcoin Taproot (v1)
- **Hash Function Separation**: BLAKE3-256 vs SHA256 ensures different validation paths
- **Tree Structure Differences**: Merkle vs Verkle trees prevent script compatibility
- **Type Separation**: CopperootMerkle and CopperootVerkle are completely isolated, preventing cross-type confusion

#### Address Validation

- **HRP Validation**: Network-specific HRP prevents cross-network confusion
- **Key Format Validation**: 32-byte x-only public key format enforcement
- **Checksum Verification**: Bech32m checksum prevents typos and corruption

### Migration and Compatibility

#### Backward Compatibility

- **Existing Taproot**: Bitcoin Taproot addresses continue to work as P2TR
- **Gradual Migration**: New addresses can use CopperootMerkle or CopperootVerkle for Copperoot features
- **Hybrid Support**: Nodes support P2TR, CopperootMerkle, and CopperootVerkle simultaneously
- **Type Selection**: Users can choose between Merkle (CopperootMerkle) and Verkle (CopperootVerkle) based on their needs

#### Wallet Integration

```rust
use tondi_addresses::{Address, Version};

// Address type detection
pub fn detect_address_type(address: &str) -> Result<Version, AddressError> {
    let addr = Address::try_from(address)?;
    Ok(addr.version)
}

// Example usage
match detect_address_type("tondi:c...")? {
    Version::CopperootMerkle => println!("CopperootMerkle address"),
    Version::CopperootVerkle => println!("CopperootVerkle address (disabled)"),
    Version::Taproot => println!("P2TR address"),
    _ => println!("Other address type"),
}
```

### RPC and CLI Support

#### Address Type Identification

```json
{
  "address": "tondi:c...",
  "type": "p2cr",
  "script_type": "pay_to_copperoot_merkle",
  "witness_version": 2,
  "public_key": "02...",
  "network": "mainnet"
}

{
  "address": "tondi:q...",
  "type": "p2crv",
  "script_type": "pay_to_copperoot_verkle",
  "witness_version": 2,
  "public_key": "02...",
  "network": "mainnet"
}
```

#### Transaction Creation

```bash
# Create CopperootMerkle address (Merkle tree)
tondi-cli getnewaddress "" p2cr

# Create CopperootVerkle address (Verkle tree)
tondi-cli getnewaddress "" p2crv

# Send to CopperootMerkle address
tondi-cli sendtoaddress "tondi:c..." 1.0

# Send to CopperootVerkle address
tondi-cli sendtoaddress "tondi:q..." 1.0
```

## Mainnet Launch Status

**Copperoot Mainnet Launch - Phase 1**:

- ✅ **CopperootMerkle (Merkle)**: Fully active and operational
  - Witness version v2 (decimal 2)
  - BLAKE3-256 hashing with domain separation
  - MuSig2 support with safety features
  - 8-layer depth limit with consensus validation
  - Annex support with strict validation (0x50 prefix, position as second-to-last witness item)
  - Mempool policy checks and standard transaction validation
  - ScriptVariant trait implementation for execution semantics

- 🔒 **CopperootVerkle (Verkle)**: Reserved but inactive
  - Witness version v2 (decimal 2) - RESERVED 
  - Control block type=1 - RESERVED
  - Annex type=V - RESERVED
  - All CopperootVerkle transactions are rejected as invalid
  - Verkle tree implementation exists but is disabled for mainnet

## Implementation Notes

### ScriptPubKey Format Compatibility

Copperoot and Taproot share identical ScriptPubKey format (`OP_1 <32-byte x-only pubkey>`), which creates a challenge for automatic script type detection. The implementation handles this by:

- **Address Version Distinction**: Copperoot uses address version v2 (2) vs Taproot's version v1 (1)
- **Control Block Differences**: Copperoot control blocks use TLV extensions with different proof types
- **Hash Function Separation**: Copperoot uses BLAKE3-256 vs Taproot's SHA256 for all operations
- **Test Strategy**: Direct validation bypasses automatic detection for testing scenarios

### Testing Strategy

The test suite includes comprehensive coverage for both key spend and script spend scenarios:

- **Key Spend Tests**: Direct signature verification using Copperoot-specific sighash computation
- **Script Spend Tests**: Full script path validation with Merkle proof verification
- **Witness Parsing**: Annex detection and BIP341-compliant witness structure validation
- **Error Handling**: Proper error propagation for invalid signatures and malformed witnesses

## Protocol Consistency and Implementation Notes

### Version Mapping Table

| Component | CopperootMerkle | CopperootVerkle | Bitcoin Taproot |
|-----------|----------------|-----------------|-----------------|
| Script Version | 192 | 2 | 88 |
| Witness Version | v2 (2) | v2 (2) | v1 (1) |
| Control Block | TLV Extensions | TLV Extensions | BIP341 Standard |
| Hash Function | BLAKE3-256 | BLAKE3-256 | SHA256 |
| Tree Structure | Merkle | Verkle (RESERVED) | Merkle |
| Proof Type | 0x00 (Active) | 0x01 (RESERVED) | N/A |

### Domain Separation Constants

Copperoot uses domain-separated hash functions to prevent cross-protocol attacks:

```rust
// Copperoot-specific domain separation tags
const COPPEROOT_SIGHASH_TAG: &[u8] = b"CopperootSighash";
const COPPEROOT_LEAF_TAG: &[u8] = b"CopperootLeaf";
const COPPEROOT_NODE_TAG: &[u8] = b"CopperootNode";
const COPPEROOT_TWEAK_TAG: &[u8] = b"CopperTweak";

// Chain-specific domain separation (prevents cross-chain replay)
const CHAIN_ID_TAG: &[u8] = b"TondiChainID";
const GENESIS_HASH_TAG: &[u8] = b"TondiGenesisHash";
```

### Forward Compatibility Strategy

Copperoot's TLV framework provides comprehensive forward compatibility through multiple mechanisms:

#### 1. TLV Extensions with Unknown Type Handling

**Unknown TLV Type Processing**:
```
Control Block: [parity_leaf_version(1)] [internal_key(32)] [TLV_extensions...]
TLV Extensions:
  - Type 0x01, Length 1, Value 0x00 (Merkle) - recognized and processed
  - Type 0xFF, Length 4, Value [0x12, 0x34, 0x56, 0x78] - unknown, ignored
  - Type 0x10, Length 33, Value [path_len(1)] [sibling_hash(32)] - recognized and processed

Result: VALID - unknown TLV types are safely ignored
```

**Forward Compatibility Benefits**:
- Old nodes can process transactions with new TLV types without errors
- New nodes can add functionality without breaking existing transactions
- Protocol upgrades can be deployed gradually without hard forks

#### 2. Versioned Control Blocks with BIP341 Compatibility

**Base Structure Compatibility**:
```
BIP341 Control Block (Base):
- Parity + Leaf Version (1 byte)
- Internal Public Key (32 bytes)

Copperoot Control Block (Extended):
- Parity + Leaf Version (1 byte) - BIP341 compatible
- Internal Public Key (32 bytes) - BIP341 compatible
- TLV Extensions (variable) - Copperoot extensions
```

**Compatibility Guarantees**:
- BIP341-compatible base structure ensures existing Taproot parsers can extract basic information
- TLV extensions provide additional functionality without breaking base structure
- Graceful degradation allows old parsers to work with new control blocks

#### 3. Reserved Features with Future Activation

**Current Reserved Features**:
- **CopperootVerkle (v3)**: Verkle tree functionality reserved but disabled
- **Annex Type V**: Verkle proof annex type reserved
- **TLV Types 0x12-0x1F**: Reserved for future proof types
- **TLV Types 0x26-0x2F**: Reserved for RGB extensions
- **TLV Types 0x30-0x3F**: Reserved for future protocol extensions

**Future Activation Strategy**:
- Reserved features can be activated through soft fork mechanisms
- TLV type ranges provide clear namespace for future extensions
- Version isolation prevents conflicts between different proof systems

#### 4. Cross-Chain Protection and Isolation

**Chain ID Isolation**:
```
Control Block TLV:
  - Type 0x04: ChainID/GenesisHash (32 bytes)
  - Value: Tondi genesis block hash or chain identifier

Purpose: Prevents cross-chain transaction replay attacks
```

**Domain Separation**:
```
Hash Function Tags:
- CopperootSighash(data) - Tondi-specific sighash
- CopperootLeaf(version, script) - Tondi-specific leaf hashing
- CopperootNode(left, right) - Tondi-specific node hashing
- CopperTweak(internal_key, merkle_root) - Tondi-specific key tweaking
```

#### 5. Graceful Degradation Examples

**Old Node Processing New Transaction**:
```
Input: Control block with unknown TLV types
Processing:
  1. Extract BIP341-compatible base structure
  2. Validate Merkle proof (if present)
  3. Ignore unknown TLV types
  4. Accept transaction as valid

Result: Transaction accepted, unknown features ignored
```

**New Node Processing Old Transaction**:
```
Input: Control block without TLV extensions
Processing:
  1. Extract BIP341-compatible base structure
  2. Validate Merkle proof (if present)
  3. Process with default TLV values
  4. Accept transaction as valid

Result: Transaction accepted, default behavior applied
```

#### 6. Future Protocol Extension Examples

**Example 1: Adding KZG Commitments (v4)**:
```
Control Block TLV:
  - Type 0x01: ProofType = 0x02 (KZG) - new type
  - Type 0x02: HashScheme = 0x01 (BLAKE3)
  - Type 0x13: KZGProof (variable) - new proof type
  - Type 0x04: ChainID/GenesisHash (32 bytes)

Compatibility: Old nodes ignore unknown types, new nodes process KZG proofs
```

**Example 2: Adding Batch Proof Support**:
```
Annex TLV:
  - Type 0x20: RGBX_ROOT (32 bytes) - existing
  - Type 0x24: BatchProofRef (variable) - new type
  - Type 0x25: DAHash (32 bytes) - new type

Compatibility: RGB-unaware nodes ignore RGB TLV, RGB-aware nodes process batch data
```

#### 7. Implementation Guidelines

**TLV Parser Requirements**:
- Must ignore unknown TLV types without error
- Must validate known TLV types strictly
- Must preserve TLV data for future processing
- Must handle malformed TLV gracefully

**Node Upgrade Strategy**:
- Phase 1: Deploy TLV parser with unknown type handling
- Phase 2: Add support for new TLV types
- Phase 3: Activate new functionality through soft fork
- Phase 4: Deprecate old functionality (if needed)

**Testing Requirements**:
- Test unknown TLV type handling
- Test malformed TLV recovery
- Test cross-version compatibility
- Test forward compatibility scenarios

### Security Considerations

1. **Cross-Chain Protection**: Domain separation prevents transaction replay across chains
2. **Version Isolation**: Different witness versions prevent confusion with Bitcoin Taproot
3. **Hash Function Separation**: BLAKE3 vs SHA256 ensures different validation paths
4. **TLV Validation**: Strict validation of required TLV types prevents malformed control blocks

## Conclusion

Copperoot represents a significant advancement in Bitcoin's Taproot protocol, providing:

- **Modern Cryptography**: BLAKE3-256 for superior performance with domain separation
- **Advanced Tree Structures**: Verkle trees for efficient proofs (reserved for future activation)
- **Complete Multi-Signature**: MuSig2 with full two-round protocol implementation and wrapper API
- **Safe MuSig2 Interface**: Multiple witness creation methods with validation to prevent misuse
- **Strict Validation**: Version-based script classification with format checking for all script types
- **Dual Address Types**: CopperootMerkle (active) and CopperootVerkle (reserved) for clear protocol separation
- **TLV Extensions**: Forward-compatible control block format with extensible proof types
- **Backward Compatibility**: Seamless integration with existing systems
- **Production-Ready**: Mempool validation, standard transaction checks, and comprehensive testing
- **Robust Testing**: Full test coverage with proper error handling and edge case validation
- **Abstract Execution**: ScriptVariant trait implementation for reusable execution semantics
- **Enhanced Security**: Comprehensive constraint checking, error handling, and misuse prevention

The implementation is production-ready for CopperootMerkle functionality and provides a solid foundation for next-generation Bitcoin applications requiring high performance, scalability, and advanced cryptographic features. The complete MuSig2 implementation with wrapper API and session management ensures robust multi-signature operations while maintaining backward compatibility. The strict script format validation and enhanced Merkle commitment verification prevent misclassification and ensure security. The ScriptVariant trait provides abstract execution semantics that enable code reuse across different Taproot-like variants. CopperootVerkle functionality is reserved for future activation, with the Verkle tree implementation already in place but disabled for mainnet launch.

### Key Protocol Improvements

1. **Witness Version Compliance**: Uses v2/v3 instead of 0x12/0x13 for Bech32m compatibility
2. **Annex Position Correctness**: Annex positioned as second-to-last witness item per BIP341
3. **TLV Control Blocks**: Forward-compatible control block format with extensible proof types
4. **Script Format Consistency**: Standardized `OP_1 <32-byte x-only pubkey>` format description
5. **Domain Separation**: Comprehensive domain separation prevents cross-protocol attacks
6. **Test Vector Coverage**: Complete test vectors for all major use cases and edge cases

## References

- [BLAKE3 Specification](https://github.com/BLAKE3-team/BLAKE3-specs)
- [Verkle Trees](https://dankradfeist.de/ethereum/2021/06/18/verkle-trie-for-eth1.html)
- [MuSig2 Paper](https://eprint.iacr.org/2020/1261)
- [Taproot BIP](https://github.com/bitcoin/bips/blob/master/bip-0341.mediawiki)
- [Tondi Documentation](https://tondi.aspectron.org/docs/)
