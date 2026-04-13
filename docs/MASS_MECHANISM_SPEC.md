# Spora Mass 机制技术规范

## 目录

1. [概述](#概述)
2. [KIP-0009: 扩展质量公式](#kip-0009-扩展质量公式)
3. [KIP-0013: 瞬时存储处理](#kip-0013-瞬时存储处理)
4. [Plurality (多样性) 机制](#plurality-多样性-机制)
5. [Spora 实现详解](#spora-实现详解)
6. [质量计算流程](#质量计算流程)
7. [区块限制与交易选择](#区块限制与交易选择)
8. [费用计算](#费用计算)
9. [常量与参数](#常量与参数)

---

## 概述

Spora 的 Mass 机制是一种多维资源计量系统，用于量化交易对网络资源的消耗。该系统基于 Kaspa 改进提案 KIP-0009 和 KIP-0013 实现，采用三种质量维度：

| 质量类型 | 计量资源 | 计算公式 | 规范来源 |
|---------|---------|---------|---------|
| **Compute Mass** | 计算资源 | 交易大小 + 脚本大小 + 签名操作 | KIP-0009 |
| **Storage Mass** | 持久化存储 | 基于输入/输出金额的调和平均 | KIP-0009 |
| **Transient Mass** | 瞬时存储 | 交易序列化大小 × 4 | KIP-0013 |

交易的整体质量取三者最大值：

```
mass(tx) = max(compute_mass, storage_mass, transient_mass)
```

---

## KIP-0009: 扩展质量公式

### 2.1 动机

KIP-0009 旨在解决状态膨胀问题。在 UTXO 模型中，创建大量小额输出会永久占用节点存储资源，而传统的交易费用机制无法有效抑制这种行为。

### 2.2 核心公式

#### 2.2.1 存储质量公式

```
storage_mass(tx) = C · (Σ(1/o) - |I|²/Σ(v))⁺

其中:
- C: 常量参数 (10¹²)
- O: 输出集合
- I: 输入集合
- o: 单个输出金额
- v: 单个输入金额
- x⁺ = max(x, 0)
```

**直观理解**：
- `Σ(1/o)` 是输出金额的调和级数，小额输出贡献更大的质量
- `|I|²/Σ(v)` 是输入的负质量，用于抵消输入被消费时释放的存储
- 当输出金额分布比输入更"分散"时，存储质量为正

#### 2.2.2 宽松公式 (Relaxed Formula)

对于满足特定条件的交易，可以使用简化计算：

```
storage_mass*(tx) = C · (Σ(1/o) - Σ(1/v))⁺
```

**适用条件**：
- `|O| = 1` (单输出交易)
- `|I| = 1` (单输入交易)
- `|O| = |I| = 2` (双输入双输出交易)

宽松公式的优势在于输入和输出对称处理，计算更简单。

### 2.3 计算质量公式

```
compute_mass = size_mass + script_mass + sigops_mass

size_mass   = serialized_size × mass_per_tx_byte
script_mass = script_pubkey_size × mass_per_script_pubkey_byte
sigops_mass = sigops_count × mass_per_sig_op
```

---

## KIP-0013: 瞬时存储处理

### 3.1 动机

KIP-0009 引入了计算质量和存储质量，但两者都无法直接限制区块的字节大小。在 10 BPS (每秒10个区块) 的网络中，最坏情况下节点可能需要近 1TB 的存储空间。

**计算示例**：
```
假设: 每个区块质量 = 500,000 (全部用于 payload 字节)
      修剪深度 = 1,119,290 区块
      最终性深度 = 432,000 区块

最坏情况存储 = (1,119,290 + 432,000) × 500,000 bytes
             ≈ 775 GB
```

### 3.2 瞬时质量公式

```
transient_mass = serialized_size × TRANSIENT_BYTE_TO_MASS_FACTOR

其中: TRANSIENT_BYTE_TO_MASS_FACTOR = 4
```

**设计原理**：
- 区块质量限制: 500,000
- 目标区块大小: 125,000 字节
- 因此: 500,000 / 125,000 = 4 mass/byte

### 3.3 典型交易分析

| 交易类型 | 大小 | 计算质量 | 瞬时质量 | 每区块交易数 | 区块总大小 |
|---------|------|---------|---------|-------------|-----------|
| 1→2 标准交易 | 316B | 2,036 | 1,264 | 245 | 77KB |
| 2→2 标准交易 | 434B | 3,154 | 1,736 | 158 | 68KB |
| KRC20 交易 | 429B | 1,819 | 1,716 | 274 | 117KB |

---

## Plurality (多样性) 机制

### 4.1 背景问题

KIP-0009 的原始公式假设所有 UTXO 具有标准脚本公钥大小（约35字节）。但在实际应用中，存在非标准脚本（如多签、智能合约），其大小可能远超标准值。

**问题示例**：
- 标准 UTXO: 63 (基础) + 35 (脚本) = 98 字节
- 大脚本 UTXO: 63 + 1000 = 1063 字节

如果两者支付相同的存储质量，大脚本 UTXO 实际上在"免费"使用额外的存储资源。

### 4.2 Plurality 定义

Plurality 表示一个 UTXO 占据的"标准存储单元"数量：

```
P = ⌈entry_size / UTXO_UNIT_SIZE⌉

其中: UTXO_UNIT_SIZE = 100 字节
```

**计算示例**：
| UTXO 大小 | Plurality (P) | 说明 |
|----------|---------------|------|
| 98 字节 | 1 | 标准大小 |
| 150 字节 | 2 | 占用2个单元 |
| 250 字节 | 3 | 占用3个单元 |

### 4.3 公式调整

引入 Plurality 后，存储质量公式需要相应调整：

#### 4.3.1 调和分量调整

原始输出调和项：
```
Σ(1/o)
```

调整后：
```
Σ(P²/o)

其中:
- P: 该输出的 plurality
- o: 输出金额
```

**推导过程**：
- 将 plurality 为 P 的输出视为 P 个子输出
- 每个子输出金额 = o/P
- 每个子输出的调和贡献 = 1/(o/P) = P/o
- P 个子输出的总贡献 = P × (P/o) = P²/o

#### 4.3.2 算术分量调整

原始输入算术项：
```
|I|² / Σ(v)
```

调整后：
```
(ΣPᵢ)² / Σ(v)

其中:
- Pᵢ: 第 i 个输入的 plurality
- v: 输入金额
```

### 4.4 Spora 中的 Cell Plurality

在 Spora 的 Cell 模型中，Plurality 计算如下：

```rust
// 规范 Cell 存储大小 (字节)
const CANONICAL_CELL_CONST_STORAGE: u64 = 
    32  // outpoint::tx_id
    + 4 // outpoint::index
    + 8 // capacity
    + 8 // data_len
    + 32 // lock_hash
    + 1 // type flag
    + 32 // data_hash
    + 8 // DAA score
    + 1 // is_cellbase
    = 126 bytes

// Plurality 计算
fn cell_plurality(lock_script: &ScriptRef) -> u64 {
    (CANONICAL_CELL_CONST_STORAGE + lock_script.args.len() as u64)
        .div_ceil(CELL_UNIT_SIZE)  // CELL_UNIT_SIZE = 100
}
```

**示例**：
- 空 args: (126 + 0) / 100 = **2** (向上取整)
- 32字节 args: (126 + 32) / 100 = **2**
- 100字节 args: (126 + 100) / 100 = **3**

### 4.5 为什么空 Cell 的 Plurality 是 2？

即使是最小的 Cell，其固定开销为 126 字节，超过了 100 字节的标准单元大小，因此 plurality 至少为 2。这确保了：

1. 所有 Cell 都有合理的存储质量基准
2. 防止通过创建极小 Cell 来规避存储质量
3. 与 KIP-0009 的 UTXO 模型保持一致性

---

## Spora 实现详解

### 5.1 核心结构

```rust
// 非上下文质量 (可在孤立验证时计算)
pub struct NonContextualMasses {
    pub compute_mass: u64,    // 计算质量
    pub transient_mass: u64,  // 瞬时质量
}

// 上下文质量 (需要 UTXO/Cell 上下文)
pub struct ContextualMasses {
    pub storage_mass: u64,    // 存储质量
}

// 投影交易质量 (用于内存池和区块模板)
pub struct ProjectedTransactionMass {
    pub effective_compute_mass: u64,
    pub transient_mass: u64,
    pub storage_mass: Option<u64>,
    pub selection_mass: u64,  // 用于交易选择
}
```

### 5.2 Cell 质量抽象

```rust
pub struct CellMass {
    pub plurality: u64,  // 多样性 (存储单元数)
    pub amount: u64,     // 金额 (sau)
}

// 从不同来源创建 CellMass
impl From<&CellEntry> for CellMass { ... }
impl From<&CellMetadata> for CellMass { ... }
impl From<(&CellOut, usize)> for CellMass { ... }
```

### 5.3 计算流程

#### 5.3.1 非上下文质量计算

```rust
pub fn calc_non_contextual_masses_cell(&self, tx: &CellTx) -> NonContextualMasses {
    // 1. 计算序列化大小
    let size = cell_tx_estimated_serialized_size(tx);
    
    // 2. 计算大小质量
    let compute_mass_for_size = size * self.mass_per_tx_byte;
    
    // 3. 计算 Lock Script 质量 (Cell 模型特有)
    let total_lock_script_size: u64 = tx.outputs.iter().map(|output| {
        let mut script_size = 32 + 1 + output.lock.args.len() as u64;
        if let Some(ref type_script) = output.type_ {
            script_size += 32 + 1 + type_script.args.len() as u64;
        }
        script_size
    }).sum();
    let total_lock_script_mass = total_lock_script_size * self.mass_per_script_pub_key_byte;
    
    // 4. 计算 SigOps 质量 (Cell 模型每个输入隐含 1 sigop)
    let total_sigops = tx.inputs.len() as u64;
    let total_sigops_mass = total_sigops * self.mass_per_sig_op;
    
    // 5. 汇总
    let compute_mass = compute_mass_for_size + total_lock_script_mass + total_sigops_mass;
    let transient_mass = size * TRANSIENT_BYTE_TO_MASS_FACTOR;
    
    NonContextualMasses::new(compute_mass, transient_mass)
}
```

#### 5.3.2 存储质量计算

```rust
pub fn calc_storage_mass(
    is_coinbase: bool,
    inputs: impl ExactSizeIterator<Item = CellMass>,
    outputs: impl Iterator<Item = CellMass>,
    storm_param: u64,  // C = 10^12
) -> Option<u64> {
    if is_coinbase {
        return Some(0);
    }
    
    // 1. 计算输出调和部分: Σ(C · P² / amount)
    let (outs_plurality, harmonic_outs) = outputs.try_fold(
        (0u64, 0u64),
        |(acc_plurality, acc_harm), CellMass { plurality, amount }| {
            Some((
                acc_plurality + plurality,
                acc_harm.checked_add(
                    storm_param
                        .checked_mul(plurality)?
                        .checked_mul(plurality)?
                        .checked_div(amount)?
                )?,
            ))
        },
    )?;
    
    // 2. 判断是否使用宽松公式
    let relaxed_formula_path = if outs_plurality == 1 {
        true  // |O| = 1
    } else if inputs.len() > 2 {
        false // 输入 > 2，不能使用宽松公式
    } else {
        let ins_plurality = inputs.clone().map(|c| c.plurality).sum::<u64>();
        ins_plurality == 1 || (outs_plurality == 2 && ins_plurality == 2)
    };
    
    // 3. 计算输入负质量
    let harmonic_ins = if relaxed_formula_path {
        // 宽松公式: Σ(C · P² / amount)
        inputs.map(|CellMass { plurality, amount }| {
            storm_param * plurality * plurality / amount
        }).fold(0u64, |total, current| total.saturating_add(current))
    } else {
        // 标准公式: |I| · (C / mean_amount)
        let (ins_plurality, sum_ins) = inputs.fold(
            (0u64, 0u64),
            |(acc_plur, acc_amt), CellMass { plurality, amount }| {
                (acc_plur + plurality, acc_amt + amount)
            }
        );
        let mean_ins = sum_ins / ins_plurality;
        ins_plurality.saturating_mul(storm_param / mean_ins)
    };
    
    // 4. 返回 max(0, harmonic_outs - harmonic_ins)
    Some(harmonic_outs.saturating_sub(harmonic_ins))
}
```

---

## 质量计算流程

### 6.1 交易生命周期中的质量计算

```
┌─────────────────────────────────────────────────────────────────┐
│                        交易创建阶段                              │
│  (钱包层)                                                        │
├─────────────────────────────────────────────────────────────────┤
│  1. 估算 Compute Mass (基于交易结构)                              │
│  2. 估算 Storage Mass (基于输入/输出金额)                         │
│  3. 计算费用 = selection_mass × fee_rate                         │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│                        内存池验证阶段                            │
│  (Mempool)                                                       │
├─────────────────────────────────────────────────────────────────┤
│  1. 计算 NonContextualMasses (孤立验证)                           │
│     - Compute Mass                                               │
│     - Transient Mass                                             │
│  2. 检查是否超过 MAXIMUM_STANDARD_TRANSACTION_MASS (100,000)      │
│  3. 等待上下文验证以计算 Storage Mass                             │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│                        上下文验证阶段                            │
│  (Consensus)                                                     │
├─────────────────────────────────────────────────────────────────┤
│  1. 解析输入的 Cell 元数据                                        │
│  2. 计算 ContextualMasses:                                       │
│     - Storage Mass (需要输入金额和 plurality)                     │
│  3. 计算 Selection Mass = max(compute, transient, storage)       │
│  4. 验证费用是否满足 minimum_relay_fee                           │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│                        区块打包阶段                              │
│  (Block Template Builder)                                        │
├─────────────────────────────────────────────────────────────────┤
│  1. 按 fee/selection_mass 排序交易                                │
│  2. 累计 mass 直到达到 max_block_mass (500,000)                   │
│  3. 分别检查:                                                    │
│     - total_compute_mass ≤ 500,000                               │
│     - total_transient_mass ≤ 500,000                             │
│     - total_storage_mass ≤ 500,000                               │
└─────────────────────────────────────────────────────────────────┘
```

### 6.2 有效计算质量 (Effective Compute Mass)

当交易经过 VM 验证后，可以使用实际的 CPU 周期来优化计算质量：

```rust
pub fn effective_compute_mass(&self) -> Option<u64> {
    self.verified_cycles.map(|cycles| {
        let vm_limits = VmLimits::default();
        let effective_size = vm_limits.effective_size(
            serialized_size as usize,
            cycles
        );
        effective_size.max(self.compute_mass)
    })
}
```

这允许计算密集型但字节数少的交易获得更准确的质量评估。

---

## 区块限制与交易选择

### 7.1 区块质量限制

```rust
// 区块质量上限
pub const max_block_mass: u64 = 500_000;

// 验证逻辑 (Crescendo 激活后)
fn check_block_mass(&self, block: &Block) -> Result<Mass> {
    let mut total_compute_mass: u64 = 0;
    let mut total_transient_mass: u64 = 0;
    
    for tx in block.transactions.iter() {
        let masses = self.mass_calculator.calc_non_contextual_masses_cell(tx);
        
        total_compute_mass = total_compute_mass.saturating_add(masses.compute_mass);
        total_transient_mass = total_transient_mass.saturating_add(masses.transient_mass);
        
        // 独立检查每种质量
        if total_compute_mass > self.max_block_mass {
            return Err(RuleError::ExceedsComputeMassLimit);
        }
        if total_transient_mass > self.max_block_mass {
            return Err(RuleError::ExceedsTransientMassLimit);
        }
    }
    
    Ok((NonContextualMasses::new(total_compute_mass, total_transient_mass), 
        ContextualMasses::new(0)))
}
```

### 7.2 交易选择算法

内存池使用加权概率抽样选择交易：

```rust
// 交易价值计算
fn calc_tx_value(&self, tx: &CandidateTransaction) -> f64 {
    let mass_limit = self.policy.max_block_mass as f64;
    let mass = tx.calculated_mass as f64;
    let fee = tx.calculated_fee as f64;
    
    // 价值 = 费用 / 质量 / 质量限制
    fee / mass / mass_limit
}
```

**选择策略**：
1. 按价值权重随机抽样
2. 检查区块质量上限
3. 标记冲突交易
4. 达到再平衡阈值时重建候选列表

---

## 费用计算

### 8.1 最小中继费用

```rust
pub fn calc_minimum_required_transaction_relay_fee(mass: u64) -> u64 {
    // 基础费用率: 1000 sau/kg (即 1 sau/gram)
    const MINIMUM_RELAY_TRANSACTION_FEE: u64 = 1000;
    
    // mass 单位是 grams，需要转换为 kg
    let mut minimum_fee = (mass * MINIMUM_RELAY_TRANSACTION_FEE) / 1000;
    
    // 确保最小费用至少为基准值
    if minimum_fee == 0 {
        minimum_fee = MINIMUM_RELAY_TRANSACTION_FEE;
    }
    
    // 不超过最大货币单位
    minimum_fee.min(MAX_SAU)
}
```

### 8.2 费用率计算

```rust
pub fn calculated_feerate(&self) -> Option<f64> {
    self.selection_mass().and_then(|mass| {
        self.calculated_fee.map(|fee| fee as f64 / mass as f64)
    })
}
```

费用率单位：**sau/gram**

---

## 常量与参数

### 9.1 共识参数

| 参数 | 值 | 说明 |
|-----|-----|------|
| `mass_per_tx_byte` | 1 | 每字节质量 |
| `mass_per_script_pub_key_byte` | 10 | 每脚本公钥字节质量 |
| `mass_per_sig_op` | 1000 | 每个签名操作质量 |
| `max_block_mass` | 500,000 | 区块质量上限 |
| `storage_mass_parameter` | 10¹² | KIP-0009 常量 C |

### 9.2 系统常量

| 常量 | 值 | 说明 |
|-----|-----|------|
| `SAU_PER_SPORA` | 100,000,000 | 1 SPORA = 10⁸ sau |
| `STORAGE_MASS_PARAMETER` | 10¹² | C = 10,000 × SAU_PER_SPORA |
| `TRANSIENT_BYTE_TO_MASS_FACTOR` | 4 | KIP-0013 字节转换因子 |
| `CELL_UNIT_SIZE` | 100 | Plurality 计算单元大小 |
| `MAXIMUM_STANDARD_TRANSACTION_MASS` | 100,000 | 标准交易质量上限 |

### 9.3 Cell 存储常量

| 常量 | 值 (字节) | 说明 |
|-----|----------|------|
| `CANONICAL_CELL_CONST_STORAGE` | 126 | Cell 固定开销 |
| `CELL_ENTRY_OVERHEAD` | 45 | CellEntry 非输出部分 |

---

## 附录

### A. 存储质量计算示例

**示例 1: 标准 1→2 交易**
```
输入: 100 SPORA (plurality = 2)
输出: 50 SPORA, 50 SPORA (plurality = 2 each)

harmonic_outs = C × 2²/50 + C × 2²/50 = 4C/50 + 4C/50 = 8C/50
arithmetic_ins = 2 × (C / 100) = 2C/100 = C/50

storage_mass = max(0, 8C/50 - C/50) = 7C/50 = 7 × 10¹² / 50 = 1.4 × 10¹¹
```

**示例 2: 使用宽松公式的 1→1 交易**
```
输入: 100 SPORA (plurality = 2)
输出: 99 SPORA (plurality = 2, 1 SPORA 作为费用)

harmonic_outs = C × 2²/99 = 4C/99
harmonic_ins = C × 2²/100 = 4C/100 = C/25

storage_mass = max(0, 4C/99 - C/25) ≈ 0.04C = 4 × 10¹⁰
```

### B. Plurality 对存储质量的影响

比较两个交易，输入输出金额相同，但 plurality 不同：

| 场景 | 输入 | 输出 | Storage Mass |
|-----|------|------|-------------|
| 标准 2→2 | P=2 each | P=2 each | X |
| 大脚本 2→2 | P=2, P=4 | P=2 each | > X |
| 超大脚本 2→2 | P=2, P=10 | P=2 each | >> X |

Plurality 机制确保大脚本交易支付与其存储占用成正比的存储质量。

---

## 参考文献

1. [KIP-0009: Extended mass formula for mitigating state bloat](https://github.com/kaspanet/kips/blob/master/kip-0009.md)
2. [KIP-0013: Transient Storage Handling](https://github.com/kaspanet/kips/blob/master/kip-0013.md)
3. Kaspa Research: "Quadratic Storage Mass and KIP9"
