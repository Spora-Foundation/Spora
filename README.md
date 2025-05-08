# Tondi Chain 

[English](./README.md) | [简体中文](./README.zh.md)

**Tondi** is a high-throughput, privacy-first and Taproot-only DAG blockchain tailored for client-side smart contract anchoring and digital asset settlement. Built on a heavily refactored version of Kaspa, Tondi delivers Taproot-native confidentiality, extreme concurrency, and native compatibility with RGB and other zero-knowledge protocols — all without sacrificing simplicity or auditability.

---

## 🌝 Project Overview

Tondi is designed as a Bitcoin-aligned execution layer — a minimal, stateless blockchain optimized for:

* High-frequency transactions
* Anchor-based smart contract validation
* Hidden asset issuance and governance commitments
* Cross-chain compatibility with BTC, Tondi, and Solana

It eliminates unnecessary complexity while maximizing composability and verifiability through client-side protocols.

---

## 🔧 Key Features

| Component             | Description                                                                           |
| --------------------- | ------------------------------------------------------------------------------------- |
| 🧱 DAG Architecture   | Based on GhostDAG with cutoff optimization, enabling 10,000+ TPS and fast convergence |
| 🔒 Taproot-Only Model | All outputs are P2TR; KeyPath ScriptPath are fully supported           |
| ⚡ Schnorr Signatures  | Signatures are uniformly Schnorr-based with support for batch validation              |
| 🔑 Anchor Obfuscation | RGB and commitment outputs are indistinguishable from native transfers                |
| 🧠 Blake3 Hash Engine | Ultra-fast cryptographic hashing, replacing Blake2b/SHA256                            |
| 🚫 Stateless Design   | No on-chain VM, global state, or script execution; all validation is client-side      |
| 🚁️ Cross-Chain Ready | Architecture supports adaptor signature schemes for atomic swaps                      |

---

## 🧱 Ideal Use Cases

* RGB token and stablecoin settlement
* Anchor-based DAO voting and SBT issuance
* High-frequency payment rails with Taproot privacy
* Interoperability bridges with Bitcoin, Solana, or other execution layers

---

## 🚀 Build Instructions

```bash
git clone https://github.com/AvatoLabs/Tondi.git
cd Tondi
cargo build --release
```
