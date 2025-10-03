# SwiftHeavy Mining Algorithm

## Overview

This project implements the new SwiftHeavy mining algorithm, which replaces the original KHeavyHash algorithm. The new algorithm provides better memory-hardness and security through the combination of Blake3 hashing, matrix operations, and an 8KB scratchpad.

## Algorithm Features

### 1. Blake3 Hashing
- Uses Blake3 as the primary hash function
- Better performance than Keccak
- Superior parallelization support

### 2. 64x64 Matrix Operations
- Maintains original matrix-vector multiplication
- Ensures computational complexity
- Provides good randomness

### 3. 8KB Scratchpad
- 8KB memory buffer provides memory-hardness
- Prevents ASIC optimization
- Increases mining fairness

## Algorithm Flow

```
Input Hash → Create Scratchpad → Multiple Matrix Operations → Scratchpad Access Patterns → Result Combination → Blake3 Final Hash
```

### Detailed Steps:

1. **Initialize Scratchpad**: Generate 8KB random data based on input hash
2. **Multiple Rounds**: Perform 8 rounds of matrix operations and scratchpad interactions
3. **Matrix-Vector Multiplication**: Execute 64x64 matrix multiplication with vectors
4. **Scratchpad Access**: Update scratchpad state based on matrix results
5. **Result Combination**: Combine matrix results with scratchpad data
6. **Final Hash**: Use Blake3 to hash the final result

## File Structure

```
consensus/pow/src/
├── lib.rs              # Main PoW state management
├── matrix.rs           # Matrix operations and SwiftHeavy implementation
├── scratchpad.rs       # 8KB scratchpad implementation
├── xoshiro.rs          # Random number generator
└── wasm.rs             # WASM bindings
```

## Performance Comparison

SwiftHeavy vs Original Algorithm:
- **Memory Usage**: +8KB scratchpad
- **Computational Complexity**: +Multiple matrix operations and scratchpad interactions
- **ASIC Resistance**: Significantly improved
- **Performance**: Optimized for modern hardware

## Usage

```rust
use tondi_pow::matrix::Matrix;
use tondi_hashes::Hash;

let matrix = Matrix::generate(Hash::from_bytes([42; 32]));
let hash = Hash::from_bytes([123; 32]);
let result = matrix.swift_heavy_hash(hash);
```

## Testing

Run the test suite:
```bash
cargo test -p tondi-pow
```

Run benchmarks:
```bash
cargo bench -p tondi-pow
```

## WASM Support

The algorithm is fully compatible with WASM bindings and can be used in web applications.

## Security Considerations

- **Memory-Hardness**: 8KB scratchpad ensures significant memory requirements
- **ASIC Resistance**: Complex access patterns make hardware optimization difficult
- **Deterministic**: Same input always produces same output
- **Cryptographically Secure**: Uses Blake3 and proven matrix operations