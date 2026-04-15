# Spora 地址与锁脚本现代化设计提案

**Status**: Draft RFC (Audit-Fixed, core consensus path implemented)  
**Date**: 2026-04-15  
**Category**: Protocol Design / Wallet UX / Script Architecture  
**Audience**: Core Protocol, Wallet, RPC, Indexer  
**Requires**: `Script { code_hash, hash_type, args }`, native builtin verifier, address crate upgrade

---

## 0. Implementation Status (2026-04-15 Snapshot)

已落地：

1. 地址家族主路径：`StdSingle` / `StdSingleECDSA` / `Account` / `FullScript`
2. canonical script mapping：`Address <-> Script { code_hash, hash_type, args }` 主链路接通
3. `FullScript` 规范化校验：minimal ULEB128、`hash_type` 白名单、`args_len` 上限、拒绝 trailing bytes
4. 标准 witness envelope 约束：`pubkey_len`/`signature_len` 强校验、拒绝 trailing bytes
5. ECDSA 验签规范：共识验证路径强制 low-S
6. CLI 消息签名兼容：支持 `xonly_pubkey32 || signature64` envelope（并兼容 legacy 64-byte 签名输入）
7. 构造器语义收口：主路径调用点已基本改为 `new_std_single/new_std_single_ecdsa`，减少 `Address::new` 的隐式 key-id 归一化依赖
8. 地址层入参约束收紧：`Address::new(Version::StdSingle*)` 仅接受 20-byte key-id；32/33-byte 公钥必须走显式 `new_std_single*` 构造器
9. 地址解析收口：`StdSingle*` 旧式 32/33-byte payload 字符串输入已默认拒绝（避免隐式归一化引发歧义）
12. 脚本分类命名收口：`ScriptClass` 枚举定义为 `NonStandard/StdSingle/StdSingleECDSA/Account/FullScript`，标准锁通过 builtin code_hash 匹配识别
13. 非标准锁处理：非 builtin 标准锁（hash_type > 4 或 code_hash 不匹配）统一归类为 `ScriptClass::NonStandard`，走 VM 脚本执行路径
14. 标准地址映射收口：`address_to_builtin_standard_lock()` 仅处理 `StdSingle/StdSingleECDSA/Account`，`StdSingle*` 仅输出 canonical builtin tuple
15. 端到端错误语义收口：body/mempool/template 校验路径对脚本验证失败统一返回 `CellValidationFailed` 及其变体（`ScriptVerificationFailed/ScriptFailed/ExceededMaxCycles/InvalidSignature` 等）
16. 迁移行为回归测试：`CellValidator`、`body_processor` 与 `virtual_processor` 已覆盖标准锁与非标准锁在各验证路径的一致处理语义
17. virtual replay 错误映射补齐：`virtual_processor/cell_processing.rs` 已对 `ScriptVerificationFailed/ScriptFailed/ExceededMaxCycles/InvalidSignature` 显式映射到 `TxInContextFailed(CellValidationFailed)`，避免退化为泛化错误
18. 复跑验证：`cargo test -p spora-consensus --features vm --lib` 已通过（100 passed / 0 failed）
19. 配置面裁剪：与 legacy inline lock 相关的分阶段治理参数已从共识配置层移除，避免运行态策略漂移

待收口：

1. **生产级 secp256k1/ecdsa 锁脚本 dep 分发闭环**
   - ✅ `secp256k1_blake3_lock` ELF 已编译并嵌入代码（`SECP256K1_BLAKE3_LOCK_SCRIPT`）
   - ✅ Code hash 函数已提供（`secp256k1_blake3_lock_code_hash()`）
   - 缺口：尚未建立 dep cell 分发机制（genesis cell 或链上部署）
   - 当前：标准地址使用 native builtin fast path；自定义脚本使用 ELF 嵌入

---

## 1. Executive Summary

### 1.1 This Document's Decision

Spora 是一条未发布的新链，不应该继续把“地址 = 完整公钥”作为主模型。

推荐方案是：

1. **默认地址切换为模板化地址**
   - 地址默认不直接暴露完整公钥
   - 地址解析结果是一个**标准锁模板**，而不是一段内嵌公钥的小脚本
2. **默认签名路径切换为 Schnorr-first**
   - 普通用户默认使用 Schnorr 单签地址
   - ECDSA 保留为兼容路径，而不是默认路径
3. **退出 inline pubkey 地址默认路径**
   - `PubKey` 和 `PubKeyECDSA` 不再作为可创建/可展示的地址版本
   - legacy inline 锁默认在共识层直接禁用
   - 所有新地址统一使用模板化标准地址
4. **保留 CKB 式脚本抽象**
   - 继续使用 `Script { code_hash, hash_type, args }`
   - 标准地址映射到 canonical lock template
   - 高级账户与自定义脚本走 full-script 路径

### 1.2 Why This Is Better Than Both Current Spora And Raw CKB

相比当前 Spora：

1. 地址更短
2. 公钥默认不公开
3. 验签方式不再和地址字节布局硬绑定
4. 更适合 MuSig2、账户恢复、多签、策略升级

相比 CKB：

1. 默认只暴露一种“标准收款地址”，而不是把脚本细节直接推给用户
2. 钱包默认值更强，用户不需要理解 full address / code hash / hash type
3. 标准单签、多签、账户策略可以被包装成“账户类型”，而不是脚本类型

目标不是“复制 CKB UX”，而是“复用 CKB 的抽象层，做出比 CKB 更简单的默认体验”。

---

## 2. Current State And Problem

### 2.1 Current Main Path In The Repository

当前主路径已进入“现代地址 + 标准 builtin 锁”的终态：

1. 新地址主模型：
   - `StdSingle` / `StdSingleECDSA` / `Account` / `FullScript`
2. 脚本分类处理：
   - 标准锁通过 `classify_script()` 识别 builtin code_hash 匹配（`BUILTIN_SCHNORR_BLAKE3_160` / `BUILTIN_ECDSA_BLAKE3_160` / `BUILTIN_ACCOUNT_DESCRIPTOR`）
   - 非标准锁（`hash_type > 4` 或 code_hash 不匹配）归类为 `NonStandard`，走 VM 脚本执行路径
3. 执行层：
   - builtin native fast path 与 script tuple 分类并存

### 2.2 Why This Should Not Be The Main Model

这个模型可以工作，但不适合作为未发布新链的默认方案：

1. **地址过早暴露公钥**
   - 隐私弱于 hash-based address
2. **地址直接绑定验证方式**
   - Schnorr 和 ECDSA 变成不同地址族
   - 未来做账户策略升级会越来越别扭
3. **高级账户演进空间差**
   - MuSig2 聚合单签还能勉强放进去
   - 多签恢复、guardian、policy account 很快就需要第二套地址体系
4. **与 CKB 的脚本 tuple 抽象脱节**
   - CKB 的关键优点不是 Blake2b，而是 `code_hash/hash_type/args`
   - 当前标准地址没有真正利用这层抽象

结论：

**非模板化 inline 锁已退出默认模型；标准地址统一使用 builtin 模板化锁。**

---

## 3. Design Goals

### 3.1 Primary Goals

1. **现代化**
   - 默认使用 Schnorr
   - 默认不暴露公钥
   - 默认地址更短、更稳
2. **便捷**
   - 普通用户只看见一个“Spora 地址”
   - 钱包内部处理锁模板与 witness 细节
3. **保留 CKB 支持**
   - 保留 `Script { code_hash, hash_type, args }`
   - 支持 full-script address
   - 保持 builtin / code cell / script hash 模型可扩展
4. **统一标准地址体系**
   - 标准地址统一使用 `StdSingle` / `StdSingleECDSA` / `Account` / `FullScript`
   - 所有地址统一映射到 `Script { code_hash, hash_type, args }` 抽象
5. **兼容未来账户能力**
   - MuSig2
   - 多签
   - 账户恢复
   - 自定义脚本账户

### 3.2 Non-Goals

1. 不把非标准 inline 脚本地址作为产品层可见能力
2. 不追求完全复制 CKB 的地址展示方式
3. 不要求普通钱包 UI 暴露 `code_hash/hash_type/args`

---

## 4. Guiding Principles

### 4.1 User Experience First, Script Abstraction Second

用户应当优先看到：

1. 一个默认标准地址
2. 一个默认账户类型
3. 一个默认签名算法

而不是：

1. 多种脚本 hash 类型
2. 多种地址格式解释
3. 多种 witness 组装方式

### 4.2 Keep The Script Model, Hide The Script Model

协议层应当保留 CKB 式脚本抽象；钱包层应当隐藏它。

换句话说：

1. **协议设计更接近 CKB**
2. **产品体验应当比 CKB 更简单**

### 4.3 Builtins Should Feel Native

标准单签、多签、账户模板不必强制走 VM 慢路径。

建议做法：

1. 地址解析后得到 canonical `Script`
2. 若 `code_hash` 命中 builtin registry，则走 native verifier
3. 若未命中，则按 VM 路径处理

这样同时得到：

1. CKB 式可扩展抽象
2. 当前 Spora 原生验签性能

---

## 5. Proposed Address Families

### 5.1 Address Version Layout

建议定义以下四个现代地址版本：

| Version | Name | Payload | Purpose | Default |
|---|---|---:|---|---|
| `0` | `StdSingle` | 20 bytes | Standard Schnorr single-sig | **Yes** |
| `1` | `StdSingleECDSA` | 20 bytes | Standard ECDSA single-sig compatibility | No |
| `2` | `Account` | 32 bytes | Descriptor/policy account | No |
| `3` | `FullScript` | variable | Full script address for advanced/custom locks | No |

**注意**：所有标准地址均映射到 builtin lock template，不再支持非模板化 inline 脚本。

### 5.2 Rationale

这样拆分的原因是：

1. `StdSingle` 给普通用户
2. `StdSingleECDSA` 给外部生态、硬件钱包兼容需求
3. `Account` 给 MuSig2、多签、恢复型账户
4. `FullScript` 给开发者和高级场景
5. 非模板化 inline 脚本地址已退出默认产品路径，非标准锁归类为 `NonStandard` 走 VM 执行

---

## 6. Canonical Script Mapping

### 6.1 Standard Addresses Must Map To Canonical Script Tuples

新的标准地址不再生成内嵌公钥 opcode 小脚本，而是映射到真正的 script tuple：

```text
Script {
  code_hash: [u8; 32],
  hash_type: u8,
  args: Vec<u8>,
}
```

### 6.2 Builtin Lock Templates

已定义的 builtin lock template：

1. `BUILTIN_SCHNORR_BLAKE3_160` - 标准 Schnorr 单签
2. `BUILTIN_ECDSA_BLAKE3_160` - 标准 ECDSA 单签
3. `BUILTIN_ACCOUNT_DESCRIPTOR` - 账户描述符（多签/MuSig2/恢复账户）

建议映射如下：

| Address Version | code_hash | hash_type | args |
|---|---|---|---|
| `StdSingle` | `BUILTIN_SCHNORR_BLAKE3_160` | `Type` | `key_id20` |
| `StdSingleECDSA` | `BUILTIN_ECDSA_BLAKE3_160` | `Type` | `key_id20` |
| `Account` | `BUILTIN_ACCOUNT_DESCRIPTOR` | `Type` | `descriptor_hash32` |
| `FullScript` | embedded / decoded | embedded / decoded | embedded / decoded |

### 6.3 Why `args` Must Mean Actual Args

新方案应当明确恢复 `args` 的语义：

1. `args` 是传给 lock template 的参数
2. `args` 不再默认承载完整 script bytecode
3. 标准地址不应继续把 `args` 当作 raw mini-script

这是保留 CKB 支持的关键一步。

---

## 7. Payload Definitions

### 7.1 `StdSingle`

```text
payload = key_id20
key_id20 = blake3("spora/key-id/schnorr/v1" || xonly_pubkey32)[0..20]
```

特性：

1. 地址短
2. 默认不泄露公钥
3. 与标准 Schnorr 单签 lock 一一对应

### 7.2 `StdSingleECDSA`

```text
payload = key_id20
key_id20 = blake3("spora/key-id/ecdsa/v1" || compressed_pubkey33)[0..20]
```

特性：

1. 兼容 ECDSA 工具链
2. 仍然不直接泄露公钥
3. 与标准 ECDSA 单签 lock 一一对应

### 7.3 `Account`

```text
payload = descriptor_hash32
descriptor_hash32 = blake3("spora/account/v1" || canonical_account_descriptor)
```

`canonical_account_descriptor` 可以表达：

1. MuSig2 聚合账户
2. `m-of-n` 多签账户
3. 社交恢复 / guardian 账户
4. 限时 fallback 策略

### 7.4 `FullScript`

建议格式：

```text
payload = code_hash(32) || hash_type(1) || args_len(varint) || args
```

用途：

1. 高级脚本账户
2. 开发者调试
3. CKB 式 full address 场景
4. 不适合作为默认钱包收款地址

### 7.5 非标准脚本处理

非 builtin 标准锁脚本（`hash_type > 4` 或 code_hash 不匹配 builtin 定义）统一归类为 `ScriptClass::NonStandard`：

1. 非标准锁不走 native fast path
2. 非标准锁通过 VM 脚本执行路径验证
3. 所有标准地址统一使用模板化 builtin 锁体系

### 7.6 `Account` Descriptor Canonicalization (Normative)

`descriptor_hash32` 必须基于**唯一可重建**的 canonical 字节序列计算，推荐：

```text
descriptor_hash32 = blake3("spora/account/v1" || borsh(canonical_descriptor_v1))
```

`canonical_descriptor_v1` 约束：

1. 必须包含 `descriptor_version`（当前固定为 `1`）
2. `pubkeys` 必须使用 compressed 33-byte 编码
3. `pubkeys` 必须按字典序升序排列并去重
4. `threshold` 必须满足 `1 <= threshold <= pubkeys.len()`
5. 所有可选字段必须使用显式 presence 位，不允许“语义等价但编码不同”
6. 保留字段必须为零值（为后续升级保留）

未满足以上任一约束，视为无效 descriptor。

### 7.7 `FullScript` Payload Canonical Rules (Normative)

`FullScript` payload：

```text
payload = code_hash(32) || hash_type(1) || args_len(varint_uleb128_minimal) || args
```

规范约束：

1. `varint` 必须是最短 ULEB128 编码（禁止过长同值编码）
2. `args_len` 必须与 `args` 实际长度严格一致
3. 建议 `args_len <= 10000`（与 script size 风险边界一致）
4. `hash_type` 必须是协议允许值；未知值直接拒绝
5. 解析时禁止 trailing bytes；出现即判 invalid

---

## 8. Witness Design

### 8.1 Goal

标准 witness 设计要同时满足：

1. 对 wallet/hardware wallet 足够简单
2. 对 native verifier 足够稳定
3. 对 VM 脚本足够明确

### 8.2 Recommended Standard Witness Envelope

建议定义统一 witness 结构：

```text
StandardSignatureWitnessV1 {
  version: u8,        // must be 1
  sig_hash_type: u8,  // consensus-defined enum
  pubkey_len: u8,     // 32 or 33
  pubkey: [u8; pubkey_len],
  signature_len: u8,  // 64 (consensus canonical)
  signature: [u8; signature_len],
}
```

推荐语义：

1. `StdSingle`
   - `pubkey_len = 32`
   - `signature_len = 64`
2. `StdSingleECDSA`
   - `pubkey_len = 33`
   - `signature_len = 64`（共识路径固定 compact 64-byte）

### 8.3 Verification Rule

`StdSingle`:

1. witness 读取 x-only pubkey
2. 计算 `blake3("spora/key-id/schnorr/v1" || pubkey)[0..20]`
3. 比对 `script.args`
4. 再做 Schnorr signature verify

`StdSingleECDSA`:

1. witness 读取 compressed pubkey
2. 计算 `blake3("spora/key-id/ecdsa/v1" || pubkey)[0..20]`
3. 比对 `script.args`
4. 再做 ECDSA verify

### 8.4 Why Not Recover-Only As The Main Path

虽然 ECDSA 可做 recoverable signature，但不建议把 recover-only 作为主 witness 模型：

1. Schnorr 无法走同一路径
2. 统一 witness envelope 更利于 wallet 和 SDK
3. 显式公钥更利于调试、索引、硬件钱包集成

recoverable ECDSA 可以作为实现优化或兼容选项，但不建议作为规范默认值。

### 8.5 Byte-Level Consensus Rules (Normative)

为避免多实现分叉，标准 witness 必须满足：

1. 字段顺序和长度必须严格按 `StandardSignatureWitnessV1` 编码
2. `version != 1` 直接拒绝
3. `pubkey_len` 与锁类型必须匹配（`StdSingle=32`, `StdSingleECDSA=33`）
4. `signature_len` 必须等于 `64`
5. ECDSA 签名必须满足 low-S 规则
6. 禁止 trailing bytes
7. 校验顺序建议固定：`format -> key-id match -> signature verify`（便于跨实现一致）

---

## 9. Wallet And UX Rules

### 9.1 Default User Experience

普通用户应当只看到：

1. 一个默认“Spora 地址”
2. 一个默认账户类型：“标准账户”
3. 一个默认签名算法：Schnorr

### 9.2 Wallet Output Rules

钱包默认行为：

1. 新建账户默认生成 `StdSingle`
2. 收款页默认只展示 `StdSingle`
3. 高级页面可导出：
   - `StdSingleECDSA`
   - `Account`
   - `FullScript`

### 9.3 Wallet Import Rules

钱包导入时应自动识别：

1. `StdSingle`
2. `StdSingleECDSA`
3. `Account`
4. `FullScript`

所有地址版本映射到统一账户模型，而不是把版本差异直接暴露给普通用户。

### 9.5 Message Signing / Verification (Normative UX Rule)

由于 `StdSingle` 地址 payload 为 `key_id20`，地址本身不携带公钥。  
钱包和 CLI 在离线验签场景应输出并接受如下 envelope：

```text
message_signature_envelope = xonly_pubkey32 || schnorr_signature64
```

校验流程：

1. 由 `xonly_pubkey32` 计算 `key_id20`
2. 比对地址 payload
3. 验证消息签名

### 9.4 Why This UX Is Better Than CKB

CKB 的抽象很强，但普通用户容易接触到：

1. 短地址 / 全地址
2. 多种 code hash / hash type
3. 不同脚本形态

Spora 应避免这点：

1. 默认只提供一个标准收款地址
2. 把脚本差异留在钱包内部
3. 把多签 / MuSig2 / 恢复表达成“账户类型”，不是“脚本细节”

---

## 10. CKB Compatibility Strategy

### 10.1 What We Preserve

保留以下 CKB 核心思想：

1. `Script { code_hash, hash_type, args }`
2. full-script address 能力
3. builtin lock template 与 code cell lock 共存
4. `hash_type` 编码语义与 CKB 对齐

### 10.2 What We Intentionally Do Not Copy

不直接复制以下 CKB 产品层表现：

1. 默认让用户理解 `code_hash/hash_type/args`
2. 默认让用户在 short/full address 之间自行选择
3. 默认把 lock template 细节暴露到 UI

### 10.3 Result

协议层：

1. 更接近 CKB
2. 更利于工具链与脚本扩展

产品层：

1. 比 CKB 更简单
2. 比 CKB 更适合主流钱包

---

## 11. Native Verification Strategy

### 11.1 Builtin Fast Path

建议引入 builtin registry：

```text
code_hash -> BuiltinLockKind
```

当 lock script 命中以下 builtin 时走 native verifier：

1. `BUILTIN_SCHNORR_BLAKE3_160`
2. `BUILTIN_ECDSA_BLAKE3_160`
3. `BUILTIN_ACCOUNT_DESCRIPTOR`（如可原生验证）

非 builtin 标准锁（`ScriptClass::NonStandard`）走 VM 脚本执行路径。

### 11.2 VM Path

以下情况走 VM：

1. `FullScript`
2. 自定义 lock code
3. 非 builtin 的 script-hash wrapper

### 11.3 Why This Matters

这个设计保留了当前 Spora 的性能优势：

1. 标准单签不必为“更 CKB”而丢掉原生性能
2. 高级脚本仍可以扩展
3. 地址抽象和执行性能不再冲突

---

## 12. Migration Plan From Current Implementation

### 12.1 标准锁验证策略

标准锁验证策略：

1. `Params` 共识配置仅包含标准 builtin 锁参数
2. `CellValidator` 对标准锁（`StdSingle/StdSingleECDSA/Account`）执行 native fast path 验证
3. 非标准锁（`NonStandard`）统一走 VM 脚本执行路径验证
4. body / mempool / template / replay 全路径统一错误语义（`CellValidationFailed` 及其变体）

### 12.2 Add New Address Versions

在 `crypto/addresses` 中新增：

1. `StdSingle`
2. `StdSingleECDSA`
3. `Account`
4. `FullScript`

### 12.3 Refactor Address To Script Conversion

将当前 `pay_to_address_lock_script()` 拆分为：

1. `address_to_lock_script()`
   - 面向所有地址版本
2. `address_to_builtin_standard_lock()`
   - `StdSingle` / `StdSingleECDSA` / `Account`
3. `address_to_full_script_lock()`
   - `FullScript`

### 12.4 Refactor Lock Classification

当前标准锁分类基于 `classify_script()` 函数，通过匹配 builtin code_hash 识别标准锁：

1. **address class**
   - 地址版本分类（`Version` 枚举）
2. **lock class**
   - `ScriptClass` 枚举：`NonStandard/StdSingle/StdSingleECDSA/Account/FullScript`
   - 通过 builtin code_hash 匹配识别标准锁

所有地址统一按 `code_hash/hash_type/args` 分类，标准锁走 native fast path，非标准锁走 VM 执行路径。

### 12.5 Wallet Default Switch

钱包默认创建和展示地址切换为：

1. `StdSingle`

而不是：

1. `PubKey`

### 12.6 RPC / Indexer Changes

RPC 与索引层建议增加：

1. 地址版本枚举返回
2. canonical lock template classification
3. `resolved_lock_kind`
4. `resolved_address_kind`

这样上层 SDK 不必重复猜测脚本类型。

兼容要求：

1. 新增字段必须为 optional，老客户端可忽略
2. `lockScriptType` 输出标准分类：`nonstandard/stdsingle/stdsingleecdsa/account/fullscript`
3. 字符串解析严格匹配 `ScriptClass::from_str` 定义，不支持的别名直接返回错误

---

## 13. Recommended Final Decision

### 13.1 Default

**默认收款地址**：`StdSingle`  
**默认签名算法**：Schnorr  
**默认锁模板**：`BUILTIN_SCHNORR_BLAKE3_160`

### 13.2 Compatibility

**兼容单签地址**：`StdSingleECDSA`  
**非标准脚本地址**：归类为 `NonStandard`，走 VM 执行路径，不作为标准产品路径

### 13.3 Advanced Accounts

**高级账户地址**：`Account`  
**开发者高级脚本地址**：`FullScript`

### 13.4 Strategic Positioning

Spora 的最终定位应当是：

1. 在协议层比当前实现更接近 CKB
2. 在产品层比 CKB 更现代、更简洁
3. 在执行层保留 native builtin fast path

---

## 14. Implementation Order

建议按以下顺序落地：

1. 先新增地址版本与 canonical script mapping
2. 再新增 builtin standard lock verifier
3. 再切换钱包默认地址到 `StdSingle`
4. 最后补齐 `Account` 与 `FullScript`

这样可以先完成：

1. 默认 UX 现代化
2. 地址语义稳定化
3. 与当前实现的低风险并存

---

## 15. Short Conclusion

Spora 不应继续把“地址 = 完整公钥”作为默认模型。

正确方向是：

1. **默认使用 hash-based、template-based 标准地址**
2. **默认走 Schnorr-first**
3. **direct pubkey 地址默认禁用**
4. **保留 CKB 式脚本 tuple 作为底层抽象**
5. **通过钱包默认值，把复杂性藏起来**

这条路线同时满足：

1. 对 CKB 的脚本模型支持
2. 对现代钱包 UX 的优先级
3. 对未来 MuSig2、多签、账户策略的可扩展性
