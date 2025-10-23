# Cell Execution Layer

CKB-inspired Cell model implementation for Spora DAG blockchain.

## Overview

This crate implements the execution layer for Cell transactions, including:

- **CellTx Types**: Cell transaction structure (Lock/Type scripts, capacity, data)
- **Scheduler**: Parallel transaction execution with RW-Set DAG
- **VM Integration**: CKB-VM (RISC-V) for script verification
- **Standard Scripts**: Secp256k1 lock script, capacity type script

## Architecture

```
exec/
├── celltx/          # Cell transaction types and encoding
│   ├── types.rs     # CellTx, CellRef, CellOut, ScriptRef
│   ├── codec.rs     # Molecule serialization
│   └── sighash.rs   # blake3 signature hashing
├── scheduler/       # Parallel execution scheduler
│   ├── dag.rs       # RW-Set → CellDAG construction
│   ├── conflict.rs  # Conflict resolution (fee_density/blue_pref/wtxid)
│   └── executor.rs  # Topological parallel execution
├── vm/              # VM adapter layer
│   ├── ckbvm.rs     # CKB-VM RISC-V integration
│   ├── interface.rs # Lock/type script interface
│   └── syscalls.rs  # System calls: load_cell/load_tx/...
└── scripts/         # Standard script library
    ├── secp256k1_lock.rs
    └── capacity_type.rs
```

## References

- CKB Cell Model: `/home/arthur/RustRoverProjects/ckb/util/types/src/core/cell.rs`
- CKB Script Verifier: `/home/arthur/RustRoverProjects/ckb/script/src/verify.rs`
- Spec: `/home/arthur/RustRoverProjects/Spora/spora.md` Section 4-6

## Status

🚧 **Under Construction** - Part of the Spora fork (Cell model migration)

See `spora.md` for full implementation plan.


