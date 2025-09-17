# TLC空投功能使用示例

## 概述

Treasure Boy现在支持Time Locked Contract (TLC)空投功能，允许创建时间锁定的交易，在指定时间后才能解锁。

## 功能特性

1. **简单时间锁定**: 基于时间戳或区块高度的简单时间锁定
2. **HTLC支持**: 支持Hash Time Locked Contract，包含秘密解锁机制
3. **批量空投**: 支持向多个地址进行TLC空投
4. **灵活配置**: 支持多种网络类型和参数配置

## 使用示例

### 1. 简单时间锁定空投

```bash
# 创建时间戳锁定的空投（2025年8月1日解锁）
cargo run --package treasure_boy -- \
  --private-key YOUR_PRIVATE_KEY \
  --address-file addresses.txt \
  --tlc-mode \
  --lock-time 1756684800 \
  --lock-time-type timestamp \
  --amount 1000000000 \
  --outputs-per-tx 10
```

### 2. 区块高度锁定空投

```bash
# 创建区块高度锁定的空投（区块100000后解锁）
cargo run --package treasure_boy -- \
  --private-key YOUR_PRIVATE_KEY \
  --address-file addresses.txt \
  --tlc-mode \
  --lock-time 100000 \
  --lock-time-type block \
  --amount 1000000000
```

### 3. HTLC空投（带秘密）

```bash
# 创建HTLC空投，支持秘密解锁
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

## 参数说明

- `--tlc-mode`: 启用TLC空投模式
- `--lock-time`: 锁定时间（Unix时间戳或区块高度）
- `--lock-time-type`: 锁定时间类型（timestamp或block）
- `--htlc-secret`: HTLC秘密（可选）
- `--recipient-pubkey`: 接收方公钥（32字节十六进制）
- `--sender-pubkey`: 发送方公钥（32字节十六进制）

## 注意事项

1. **时间戳**: 必须大于500,000,000,000（LOCK_TIME_THRESHOLD）
2. **区块高度**: 必须小于500,000,000,000
3. **公钥格式**: 必须是32字节的十六进制字符串
4. **网络支持**: 支持mainnet、testnet、devnet

## 解锁机制

### 简单时间锁定
- 在指定时间后，地址所有者可以使用私钥解锁

### HTLC
- **接收方路径**: 提供正确的秘密和签名即可解锁
- **发送方路径**: 在锁定时间到期后，发送方可以使用自己的私钥解锁

## 安全考虑

1. 确保私钥安全存储
2. 验证网络类型正确
3. 检查锁定时间设置合理
4. 对于HTLC，确保秘密和公钥正确
