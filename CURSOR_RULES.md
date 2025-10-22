# CURSOR_RULES.md — Spora Fork Rules (CKB-inspired)

> **⚠️ 重要警告：这是一次不可逆的架构重写**
> 
> 本分支 `spora` 将**完全放弃 UTXO 模型**，不保留任何兼容层或过渡代码。
> - **不向后兼容**：现有 UTXO 交易将无法在新系统中验证
> - **不可逆删除**：UTXO 相关代码将被直接删除（保留 git 历史）
> - **彻底重写**：这不是渐进式迁移，而是从 Cell 模型重新开始
> 
---

## 0. 目标/边界

* **目标**：在新分支 `spora` 中，**完全放弃 UTXO 模型**，全面引入 **Cell 模型（lock/type/data + RW-Set）**，保持 **DAG 共识骨架**，先用 **GhostDAG**，为后续 **Spora 共识**预留挂载点。
* **参照**：CKB 的 **Cell 语义/脚本接口/CKB-VM 交互模式**，而**不复制**其线性链/NC-Max 共识。
  * **CKB 源码位置**：`/home/arthur/RustRoverProjects/ckb/` （可直接参考）
  * 重点参考：`ckb/script/`, `ckb/traits/`, `ckb/tx-pool/`, `ckb/store/`
* **⚠️ 重要决策：完全放弃 UTXO**
  * **不保留任何 UTXO 代码**（包括兼容层、过渡脚手架）
  * **不考虑向后兼容**
  * **这是一次彻底的架构重写**，不是渐进式迁移
  * 所有 UTXO 相关模块将被**直接删除**，而非标记 deprecated

**当前代码基础（Tondi v1.21.0）**：
- 语言：**Rust** (edition 2021, rustc 1.82.0)
- 共识：GhostDAG + UTXO（**将被完全替换为 GhostDAG + Cell**）
- 待删除模块：`indexes/utxoindex/`, `consensus/*/tx_validation_in_utxo_context.rs`, UTXO 相关验证逻辑
- 保留模块：`consensus/core/`（DAG 部分）, `database/`, `protocol/p2p/`（底层）

---

## 1. 目录与模块落点（请按此新建/重构）

**新增 Crate 结构（基于 Tondi workspace）**：

```
Tondi/
├── consensus/
│   ├── core/           # 现有：保留 DAG/GhostDAG 核心，删除所有 UTXO 类型
│   ├── spora/          # 新增 crate：Spora 共识接口与权重打分
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── weight.rs      # DA/执行证明/拓扑权重
│   │       └── interface.rs   # 共识切换接口
│   └── processes/
│       └── cell_validator/    # 新增：完全替代 transaction_validator
│           ├── mod.rs
│           ├── cell_validation_in_isolation.rs
│           ├── cell_validation_in_context.rs
│           ├── cell_validation_in_dag.rs
│           └── errors.rs
│
├── exec/               # 新增顶层 crate：Cell 执行层
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── celltx/            # Cell 交易定义
│       │   ├── types.rs       # CellTx, CellRef, CellOut, ScriptRef
│       │   ├── codec.rs       # borsh 序列化
│       │   └── sighash.rs     # blake3 签名哈希
│       ├── scheduler/         # 并行调度器
│       │   ├── dag.rs         # RW-Set → CellDAG 构图
│       │   ├── conflict.rs    # 冲突裁决（fee_density/blue_pref/wtxid）
│       │   └── executor.rs    # 拓扑分层并行执行
│       ├── vm/                # VM 适配层
│       │   ├── ckbvm.rs       # CKB-VM RISC-V 集成（参考 ../ckb/script/）
│       │   ├── interface.rs   # lock/type 脚本接口
│       │   └── syscalls.rs    # 系统调用：load_cell/load_tx/...
│       └── scripts/           # 标准脚本库
│           ├── secp256k1_lock.rs
│           └── capacity_type.rs
│
├── state/              # 新增顶层 crate：Cell 状态管理
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── index/             # Cell 索引
│       │   ├── cell_db.rs     # CellID → SegmentPtr
│       │   └── script_index.rs # lock/type → CellIDs
│       └── store/             # 数据可用性存储
│           ├── segment.rs     # 段文件管理（1GB segments）
│           ├── proof.rs       # NMT/KZG 承诺与抽样验证
│           └── writer.rs      # 顺序写入器
│
├── indexes/
│   ├── core/           # 现有：保留索引核心抽象
│   ├── processor/      # 现有：保留处理器
│   └── cellindex/      # 新增：Cell 索引服务（替代 utxoindex）
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           └── api.rs         # RPC 接口：get_cells_by_lock
│
├── mempool/            # 新增顶层 crate（当前未独立）
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── cellpool.rs        # CellTx 池
│       ├── scorer.rs          # fee_density·α + unlockability·β
│       └── relay.rs           # 包中继/RBF/CPFP
│
├── protocol/
│   └── p2p/
│       └── src/
│           └── gossip/        # 新增：Cell 相关 P2P 扩展
│               ├── cell_inv.rs
│               └── da_sample.rs
│
└── testvectors/        # 新增顶层目录：测试向量
    ├── s1_serialization/
    ├── s2_concurrency/
    ├── s3_reorg/
    ├── s4_vm/
    ├── s5_da/
    └── s6_errors/
```

**参考 CKB 的对应模块**：
- `ckb/script/` → `exec/vm/ckbvm.rs`
- `ckb/tx-pool/` → `mempool/cellpool.rs`
- `ckb/store/` → `state/store/`
- `ckb/traits/` → `exec/vm/interface.rs`

---

## 2. 废弃/替换清单（Agent 首轮 PR 必做）

**⚠️ 完全删除（不保留任何代码）**：

* `indexes/utxoindex/` → **整个 crate 直接删除**
* `consensus/src/processes/transaction_validator/tx_validation_in_utxo_context.rs` → **删除**
* `consensus/src/processes/transaction_validator/tx_validation_in_isolation.rs` → **删除或彻底重写**
* `consensus/src/processes/transaction_validator/tx_validation_in_header_context.rs` → **删除或彻底重写**
* `consensus/core/src/utxo/` (如果存在) → **删除**
* `consensus/core/src/tx.rs` 中的 `VerifiableTransaction` → **删除**，替换为 `VerifiableCellTx`
* `crypto/txscript/` 中的 UTXO 特定逻辑 → **移除所有 UTXO 假设**

**扫描待删除的代码（直接删除，不注释）**：

```bash
# 在 consensus/ 和 mining/ 中扫描
rg -n "utxo|UTXO|UtxoEntry|script_pub_key|ScriptPublicKey" \
  consensus/src/ consensus/core/src/ mining/src/
```

**待删除的具体文件和目录（扫描确认后全部删除）**：

1. **✗ 删除**：`indexes/utxoindex/` - 整个目录（包括 Cargo.toml）
2. **✗ 删除**：`consensus/src/processes/transaction_validator/tx_validation_in_utxo_context.rs` (361行)
3. **✗ 删除**：`consensus/src/processes/transaction_validator/tx_validation_in_isolation.rs`
4. **✗ 删除**：`consensus/src/processes/transaction_validator/tx_validation_in_header_context.rs`
5. **✗ 删除**：`consensus/core/src/tx.rs` 中的所有 UTXO 相关 trait 和类型
6. **✗ 删除**：`consensus/core/src/utxo/` (如果存在)
7. **⚠️ 延后处理**：`wallet/` 相关 UTXO 假设（等 Cell 交易类型稳定后重写钱包）

**从 Cargo.toml 移除的 crate**：

```toml
# workspace members 中删除：
"indexes/utxoindex",
"tondi-utxoindex",
```

**替换为（新建模块）**：

* **exec/celltx/types.rs**：
  ```rust
  pub const CELL_TX_VERSION: u16 = 0xC001;
  pub struct CellRef { pub id: [u8; 32], pub since: u64 }
  pub struct ScriptRef { pub code_hash: [u8; 32], pub hash_type: u8, pub args: Vec<u8> }
  pub struct CellOut { pub lock: ScriptRef, pub type_: Option<ScriptRef>, pub capacity: u64, pub data: Vec<u8> }
  pub struct CellTx { pub ver: u16, pub inputs: Vec<CellRef>, pub deps: Vec<CellRef>, pub outputs: Vec<CellOut>, pub fee: u64, pub sigs: Vec<Vec<u8>> }
  ```

* **exec/celltx/sighash.rs**：
  ```rust
  // SigMsg = blake3("Cell/sig" || network_id || wtxid || u32le(input_index) || rw_commitment)
  pub fn compute_cell_sighash(tx: &CellTx, input_index: usize, network_id: u8, rw_commitment: &[u8; 32]) -> [u8; 32]
  ```

* **exec/scheduler/dag.rs**：RW-Set → CellDAG 构图，冲突检测
* **state/index/cell_db.rs**：`CellID -> {segment_id, offset, len, lock_hash, type_hash}`
* **state/store/segment.rs**：段文件（1GB）+ NMT/KZG 承诺

> ⚠️ **重要**：**TxID/WTxID 计算**必须纳入 `tx_ver`，防止可锻性与域混淆。所有哈希使用 `blake3` 且加域前缀。

---

## 3. 命名与常量（统一口径）

* `tx_ver`：`0xC001`（CellTx v1）
* 域常量：

  * `CELL_SIG_DOMAIN = "Cell/sig"`
  * `CELL_TXID_DOMAIN = "Cell/txid"`
  * `TONDI_WTXID_DOMAIN = "tondi/wtxid"`（沿用 blake3，全见证）
* 区块承诺：**双命名空间 Merkle/CMR**

  * `ns=1`: `cell_root`（新）
  * `ns=0`: `legacy_root`（占位，移除后置为零常量或空根）
* 错误码前缀：`CONSENSUS_`, `EXEC_`, `DOS_`

---

## 4. 关键类型（从 CKB 学习，DAG 适配）

### 4.1 Cell 模型核心（参考 CKB util/types/src/core/cell.rs）

```rust
// exec/celltx/types.rs

/// Cell 引用（输入）
/// 参考 CKB OutPoint + since
pub struct CellRef {
    /// Cell 的唯一标识：tx_hash || output_index
    pub out_point: OutPoint,
    /// 时间锁：支持相对/绝对时间或 DAA 分数
    /// 高位标志位：0x80=相对锁，0x40=DAA分数（vs时间戳），0x20=区块锁
    pub since: u64,
}

/// OutPoint：指向某个交易的某个输出
#[derive(Hash, Eq, PartialEq, Clone)]
pub struct OutPoint {
    pub tx_hash: [u8; 32],       // 交易哈希
    pub index: u32,              // 输出索引（0-based）
}

/// 脚本引用（CKB Script）
/// 参考 CKB packed::Script
pub struct ScriptRef {
    /// 脚本代码的哈希（指向一个 Cell 的 data）
    pub code_hash: [u8; 32],
    /// 哈希类型：0=Data, 1=Type, 2=Data1（新版），3=Data2
    pub hash_type: u8,
    /// 脚本参数（传递给 VM）
    pub args: Vec<u8>,
}

/// Cell 输出（类似 CKB CellOutput + data）
pub struct CellOut {
    /// 锁脚本：定义谁能花费此 Cell
    pub lock: ScriptRef,
    /// 类型脚本（可选）：定义状态转移约束
    pub type_: Option<ScriptRef>,
    /// 容量（CKB 用 shannons，Tondi 用 saus）
    /// 必须 >= Cell 占用的存储空间（防止状态爆炸）
    pub capacity: u64,
    /// Cell 数据（任意字节，TLV 或其他编码）
    pub data: Vec<u8>,
}

/// Cell 交易（完整交易结构）
pub struct CellTx {
    /// 交易版本：0xC001（Cell v1）
    pub ver: u16,
    /// 输入：花费的 Cells
    pub inputs: Vec<CellRef>,
    /// 依赖：只读 Cells（如脚本代码 Cell）
    pub deps: Vec<CellDep>,
    /// 输出：创建的新 Cells
    pub outputs: Vec<CellOut>,
    /// 输出数据（与 outputs 一一对应）
    /// 注：CKB 分离 outputs 和 data，优化验证
    pub outputs_data: Vec<Vec<u8>>,
    /// 见证数据（签名、多签脚本等）
    pub witnesses: Vec<Vec<u8>>,
}

/// Cell 依赖（CKB CellDep）
pub struct CellDep {
    pub out_point: OutPoint,
    /// 依赖类型：Code=脚本代码，DepGroup=依赖组
    pub dep_type: DepType,
}

#[repr(u8)]
pub enum DepType {
    /// 单个 Cell 作为代码
    Code = 0,
    /// DepGroup：一个 Cell 包含多个 OutPoint（批量依赖）
    DepGroup = 1,
}

/// Cell 元数据（参考 CKB CellMeta）
pub struct CellMeta {
    pub cell_output: CellOut,
    pub out_point: OutPoint,
    /// DAG 特有：Cell 所在的交易信息
    pub transaction_info: Option<TransactionInfo>,
    pub data_bytes: u64,
    /// 内存缓存的 data 和 data_hash
    pub mem_cell_data: Option<Vec<u8>>,
    pub mem_cell_data_hash: Option<[u8; 32]>,
}

/// DAG 交易信息（区别于 CKB 的 BlockNumber）
pub struct TransactionInfo {
    /// 交易哈希
    pub tx_hash: [u8; 32],
    /// DAA 分数（区块蓝分，GhostDAG）
    pub daa_score: u64,
    /// 区块哈希（可能在多个区块中）
    pub block_hash: [u8; 32],
    /// 是否是 cellbase（挖矿奖励交易）
    pub is_cellbase: bool,
}

/// Cell 状态（参考 CKB CellStatus）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CellStatus {
    /// Cell 存在且未被花费
    Live(CellMeta),
    /// Cell 已被花费（在某个 DAA 分数后）
    Dead(u64),  // 花费时的 DAA 分数
    /// Cell 不在索引中（可能在孤儿块）
    Unknown,
}
```

### 4.2 DAG-Aware 扩展

```rust
// consensus/core/src/dag_cell.rs

/// 已解析的 Cell 交易（参考 CKB ResolvedTransaction）
pub struct ResolvedCellTx {
    pub transaction: CellTx,
    /// 已解析的输入 Cells
    pub resolved_inputs: Vec<CellMeta>,
    /// 已解析的依赖 Cells
    pub resolved_deps: Vec<CellMeta>,
}

/// DAG Cell Provider（CellStatus 查询接口）
pub trait DagCellProvider {
    /// 查询 Cell 状态
    /// at_daa_score: 在某个 DAA 分数时的状态（处理重组）
    fn cell(&self, out_point: &OutPoint, at_daa_score: Option<u64>) -> CellStatus;
    
    /// 批量查询（优化性能）
    fn cells(&self, out_points: &[OutPoint], at_daa_score: Option<u64>) -> Vec<CellStatus>;
    
    /// 检查 Cell 是否活着（快速路径）
    fn is_live(&self, out_point: &OutPoint, at_daa_score: Option<u64>) -> bool;
}
```

---

## 5. 调度与并发（必须确定性）

* **RW-Set 强制声明**：缺失/越界 → `EXEC_RWSET_INVALID`
* **建图规则**：

  * `A.outputs ∩ B.inputs ≠ ∅` → `A → B`
  * `A.inputs ∩ B.inputs ≠ ∅` → 冲突候选集
  * `A.outputs ∩ B.deps ≠ ∅` → `A → B`
* **冲突裁决键**（稳定排序，必须文档化写死）：
  `fee_density ↓ → blue_pref ↑ → wtxid ↑`
* **执行顺序**：`levels = topo_levels(G)`；同层并行，层间流水线

---

## 6. VM 与脚本接口（完整 CKB 风格）

### 6.1 脚本验证架构（参考 CKB script/verify.rs）

```rust
// exec/vm/verifier.rs

/// 交易脚本验证器（参考 CKB TransactionScriptsVerifier）
pub struct CellTxVerifier<DL: CellDataProvider> {
    /// 已解析的交易
    pub rtx: Arc<ResolvedCellTx>,
    /// Cell 数据加载器（从存储读取）
    pub data_loader: DL,
    /// 共识参数
    pub consensus: Arc<Consensus>,
    /// DAG 验证环境
    pub tx_env: Arc<DagTxEnv>,
}

/// DAG 交易验证环境
pub struct DagTxEnv {
    /// 当前 DAA 分数（用于 cellbase 成熟度检查）
    pub current_daa_score: u64,
    /// 父块集合（GhostDAG 父集）
    pub parent_hashes: Vec<[u8; 32]>,
    /// 区块时间戳
    pub block_timestamp: u64,
}

impl<DL: CellDataProvider> CellTxVerifier<DL> {
    /// 验证所有脚本
    pub fn verify(&self, max_cycles: Cycle) -> Result<Cycle, ScriptError> {
        // 1. 分组脚本（按 code_hash + hash_type + args 分组）
        let script_groups = self.group_scripts()?;
        
        // 2. 并行验证各组（如果 > 阈值）
        if script_groups.len() > PARALLEL_THRESHOLD {
            self.verify_parallel(script_groups, max_cycles)
        } else {
            self.verify_sequential(script_groups, max_cycles)
        }
    }
    
    /// 脚本分组（CKB 优化：相同脚本只运行一次）
    fn group_scripts(&self) -> Result<Vec<ScriptGroup>, Error> {
        let mut groups = Vec::new();
        
        // Lock scripts（每个 input 必验证）
        for (i, input) in self.rtx.resolved_inputs.iter().enumerate() {
            let script = &input.cell_output.lock;
            groups.push(ScriptGroup {
                script: script.clone(),
                group_type: ScriptGroupType::Lock,
                input_indices: vec![i],
                output_indices: vec![],
            });
        }
        
        // Type scripts（按 code_hash 分组）
        for (i, output) in self.rtx.transaction.outputs.iter().enumerate() {
            if let Some(type_script) = &output.type_ {
                // 找到所有相同 type script 的输入/输出
                let (inputs, outputs) = self.find_same_type_cells(type_script);
                groups.push(ScriptGroup {
                    script: type_script.clone(),
                    group_type: ScriptGroupType::Type,
                    input_indices: inputs,
                    output_indices: outputs,
                });
            }
        }
        
        Ok(groups)
    }
}

/// 脚本组
pub struct ScriptGroup {
    pub script: ScriptRef,
    pub group_type: ScriptGroupType,
    /// 关联的输入索引
    pub input_indices: Vec<usize>,
    /// 关联的输出索引（type script）
    pub output_indices: Vec<usize>,
}

pub enum ScriptGroupType {
    /// Lock script：验证 input 的花费权限
    Lock,
    /// Type script：验证状态转移规则
    Type,
}
```

### 6.2 CKB-VM 集成（参考 CKB script/syscalls/）

```rust
// exec/vm/ckbvm.rs

use ckb_vm::{
    DefaultMachineBuilder, SupportMachine, Syscalls,
    machine::asm::{AsmCoreMachine, AsmMachine},
};

/// CKB-VM 机器（RISC-V）
pub type CellVM = AsmMachine;

/// 系统调用生成器
pub fn generate_cell_syscalls<DL: CellDataProvider>(
    data_loader: &DL,
    rtx: &ResolvedCellTx,
    group: &ScriptGroup,
) -> Vec<Box<dyn Syscalls<CellVM>>> {
    vec![
        // 加载当前脚本
        Box::new(LoadScript::new(group.script.clone())),
        // 加载交易哈希
        Box::new(LoadTxHash::new(rtx.transaction.hash())),
        // 加载 Cell（input/output）
        Box::new(LoadCell::new(rtx.clone(), data_loader.clone())),
        // 加载 Cell Data
        Box::new(LoadCellData::new(rtx.clone(), data_loader.clone())),
        // 加载见证
        Box::new(LoadWitness::new(rtx.transaction.witnesses.clone())),
        // 加载 Header（DAG 父块头）
        Box::new(LoadHeader::new(data_loader.clone())),
        // VM 调试打印
        Box::new(Debugger::new()),
        // 获取当前 cycles
        Box::new(CurrentCycles),
        // Exec（动态加载脚本，CKB 高级特性）
        Box::new(Exec::new(rtx.clone(), data_loader.clone())),
    ]
}

/// LoadCell 系统调用（参考 CKB syscalls/load_cell.rs）
pub struct LoadCell<DL> {
    rtx: Arc<ResolvedCellTx>,
    data_loader: DL,
}

impl<DL: CellDataProvider> Syscalls<CellVM> for LoadCell<DL> {
    fn invoke(&mut self, id: u64, args: &[u64]) -> Result<u64, VMError> {
        // id: 系统调用号（如 2071）
        // args[0]: 目标地址
        // args[1]: 长度
        // args[2]: offset
        // args[3]: source（0=input, 1=output, 2=deps）
        // args[4]: index
        
        let source = args[3];
        let index = args[4] as usize;
        
        let cell = match source {
            0 => self.rtx.resolved_inputs.get(index),
            1 => self.rtx.transaction.outputs.get(index).map(|_| {
                // 构造输出 Cell（未写入存储）
                unimplemented!()
            }),
            2 => self.rtx.resolved_deps.get(index),
            _ => return Err(VMError::InvalidSource),
        }?;
        
        // 序列化 CellMeta，写入 VM 内存
        let serialized = self.serialize_cell(cell);
        self.write_to_vm(args[0], &serialized, args[2] as usize)?;
        
        Ok(serialized.len() as u64)
    }
}
```

### 6.3 标准锁脚本（Secp256k1）

```rust
// exec/scripts/secp256k1_lock.rs

/// Secp256k1 锁脚本（参考 CKB secp256k1_blake160）
/// 脚本参数：20 字节公钥哈希（blake2b(pubkey)[0..20]）
/// 见证：65 字节签名（r + s + v）
pub fn verify_secp256k1_lock(
    script: &ScriptRef,
    tx: &CellTx,
    input_index: usize,
) -> Result<(), ScriptError> {
    // 1. 从 script.args 提取公钥哈希（20 字节）
    if script.args.len() != 20 {
        return Err(ScriptError::InvalidArgs);
    }
    let pubkey_hash = &script.args[..20];
    
    // 2. 从 witnesses[input_index] 提取签名
    let witness = tx.witnesses.get(input_index)
        .ok_or(ScriptError::MissingWitness)?;
    if witness.len() < 65 {
        return Err(ScriptError::InvalidWitness);
    }
    let signature = &witness[..65];
    
    // 3. 计算 sighash（签名消息）
    let sighash = compute_cell_sighash(tx, input_index)?;
    
    // 4. 恢复公钥
    let pubkey = secp256k1_recover(signature, &sighash)?;
    
    // 5. 验证公钥哈希
    let recovered_hash = blake2b_160(&pubkey);
    if recovered_hash != pubkey_hash {
        return Err(ScriptError::SignatureVerificationFailed);
    }
    
    Ok(())
}
```

### 6.4 资源计量与限制

```rust
// exec/vm/limits.rs

/// VM 资源限制（防 DoS）
pub struct VmLimits {
    /// 最大执行周期（参考 CKB：70_000_000）
    pub max_cycles: Cycle,
    /// 最大内存（8MB）
    pub max_memory: usize,
    /// 最大脚本大小（500KB）
    pub max_script_size: usize,
}

/// Cycle 计算（CKB 方式）
pub fn calculate_cycles(
    tx: &CellTx,
    script_groups: &[ScriptGroup],
) -> Cycle {
    let mut total = 0;
    
    // 基础 cycles：交易大小相关
    total += (tx.serialized_size() / 1024) * 1000;
    
    // 每个脚本组的 cycles（VM 实际执行）
    for group in script_groups {
        total += group.consumed_cycles;
    }
    
    total
}

/// 错误码
pub enum VmError {
    ExceededMaxCycles(Cycle),
    ExceededMaxMemory(usize),
    InvalidSource,
    IndexOutOfBound,
}
```

---

## 7. 存储/DA 规范（RocksDB 仅做索引）

* **段文件**：顺序写、mmap/`O_DIRECT`，`segment_size = 1GB`；`chunk = 1MB`
* **承诺**：段内 **NMT** 或 **KZG**（接口抽象为 `ProofEngine`）
  `segment_root` 写入区块扩展头或 DA 索引
* **索引（RocksDB/Pebble）**：

  * `CellID -> {segment_id, offset, len, type_hash, lock_hash}`
  * `ScriptIndex(lock|type) -> CellIDs`
  * `SegmentID -> {root, parity_meta, seal_ts}`
* **抽样验证**：默认 20 片/段，失败 → 拒收块 `CONSENSUS_DA_SAMPLE_FAIL`

---

## 8. 序列化/哈希/签名（统一 LE）

* 所有 `u*` 统一 **小端 LE**；字符串 UTF-8；TLV 有界
* `wtxid = blake3(CELL_TXID_DOMAIN || canonical_tx_with_witness)`（含 `ver`、见证、脚本）
* `SigMsg = blake3(CELL_SIG_DOMAIN || network_id || wtxid || u32le(input_index) || rw_commitment)`
* 未知字段/枚举 → **fail-closed**

---

## 9. CellPool（Mempool）设计（参考 CKB tx-pool）

### 9.1 CellPool 架构

```rust
// mempool/src/cellpool.rs

/// Cell 交易池（参考 CKB TxPool）
pub struct CellPool {
    /// 池内交易（多索引：id, score, status）
    entries: MultiIndexCellEntryMap,
    /// Cell 依赖关系（edges）
    edges: CellEdges,
    /// 父子关系（links）
    links: TxLinksMap,
    /// 配置
    config: CellPoolConfig,
}

/// Cell 池条目（参考 CKB TxEntry）
pub struct CellEntry {
    pub rtx: Arc<ResolvedCellTx>,
    pub cycles: Cycle,
    pub size: usize,
    pub fee: Capacity,
    /// 祖先统计（CPFP）
    pub ancestors_size: usize,
    pub ancestors_fee: Capacity,
    pub ancestors_cycles: Cycle,
    pub ancestors_count: usize,
    /// 后代统计（RBF）
    pub descendants_fee: Capacity,
    pub descendants_size: usize,
    pub descendants_cycles: Cycle,
    pub descendants_count: usize,
    pub timestamp: u64,
}

/// Cell 边（依赖关系）
pub struct CellEdges {
    /// OutPoint -> 消费此 Cell 的交易
    inputs: HashMap<OutPoint, HashSet<TxHash>>,
    /// OutPoint -> 依赖此 Cell 的交易（deps）
    deps: HashMap<OutPoint, HashSet<TxHash>>,
    /// 头依赖（DAG 父块）
    header_deps: HashMap<[u8; 32], HashSet<TxHash>>,
}

impl CellPool {
    /// 添加交易到池
    pub fn add_tx(&mut self, tx: CellTx) -> Result<(), PoolError> {
        // 1. 预验证（基本规则）
        self.pre_verify(&tx)?;
        
        // 2. 解析交易（加载 Cells）
        let rtx = self.resolve_tx(&tx)?;
        
        // 3. 全验证（脚本 + 冲突）
        self.full_verify(&rtx)?;
        
        // 4. 计算 cycles 和 fee
        let cycles = self.estimate_cycles(&rtx)?;
        let fee = self.calculate_fee(&rtx)?;
        
        // 5. 检查冲突（双花）
        self.check_conflicts(&rtx)?;
        
        // 6. 构建 entry
        let entry = CellEntry::new(Arc::new(rtx), cycles, fee, tx.serialized_size());
        
        // 7. 更新祖先/后代
        self.update_ancestors_descendants(&entry)?;
        
        // 8. 插入池
        self.entries.insert(entry);
        self.update_edges(&tx);
        
        Ok(())
    }
    
    /// 获取最优交易（打包用）
    pub fn get_top_transactions(
        &self,
        max_size: usize,
        max_cycles: Cycle,
    ) -> Vec<CellTx> {
        let mut selected = Vec::new();
        let mut total_size = 0;
        let mut total_cycles = 0;
        
        // 按 ancestors_score 排序（fee_rate + 祖先）
        for entry in self.entries.iter_by_score_desc() {
            if total_size + entry.ancestors_size > max_size {
                continue;
            }
            if total_cycles + entry.ancestors_cycles > max_cycles {
                continue;
            }
            
            // 检查所有祖先是否已选
            if !self.all_ancestors_selected(&entry, &selected) {
                continue;
            }
            
            selected.push(entry.rtx.transaction.clone());
            total_size += entry.size;
            total_cycles += entry.cycles;
        }
        
        selected
    }
    
    /// RBF（Replace-By-Fee）
    pub fn replace_tx(
        &mut self,
        new_tx: CellTx,
        conflicts: Vec<TxHash>,
    ) -> Result<(), PoolError> {
        // RBF 规则（参考 BIP 125）
        // 1. 新交易必须花费至少一个与冲突交易相同的 Cell
        // 2. 新交易 fee_rate 必须更高
        // 3. 新交易绝对 fee 必须高于所有被替换交易的总和
        
        let new_fee = self.calculate_fee_for_tx(&new_tx)?;
        let conflict_total_fee = conflicts.iter()
            .map(|hash| self.entries.get(hash).map(|e| e.fee).unwrap_or(0))
            .sum::<u64>();
        
        if new_fee <= conflict_total_fee {
            return Err(PoolError::InsufficientFee);
        }
        
        // 移除冲突交易
        for conflict_hash in conflicts {
            self.remove_tx(&conflict_hash)?;
        }
        
        // 添加新交易
        self.add_tx(new_tx)
    }
}
```

### 9.2 评分系统

```rust
// mempool/src/scorer.rs

/// 交易评分（影响打包顺序）
pub fn compute_ancestors_score(entry: &CellEntry) -> f64 {
    // 祖先费率（CPFP：子交易带动父交易）
    let ancestors_fee_rate = entry.ancestors_fee as f64 / entry.ancestors_size as f64;
    
    // cycles 归一化
    let cycles_factor = 1.0 - (entry.ancestors_cycles as f64 / MAX_BLOCK_CYCLES as f64);
    
    // 时间因子（等待越久，优先级越高）
    let age_factor = 1.0 + (current_time() - entry.timestamp) as f64 / 3600000.0;  // 每小时 +1
    
    ancestors_fee_rate * cycles_factor * age_factor
}

/// 驱逐键（内存满时踢出低优先级交易）
pub fn compute_evict_key(entry: &CellEntry) -> EvictKey {
    // 后代费率（RBF：包含所有后代）
    let descendants_fee_rate = entry.descendants_fee as f64 / entry.descendants_size as f64;
    
    EvictKey {
        fee_rate: descendants_fee_rate,
        timestamp: entry.timestamp,
    }
}
```

### 9.3 冲突检测与裁决（DAG 特有）

```rust
// mempool/src/conflicts.rs

/// Cell 冲突检测器（DAG-aware）
pub struct ConflictDetector {
    /// OutPoint -> TxHash（追踪 Cell 的消费者）
    cell_spenders: HashMap<OutPoint, TxHash>,
}

impl ConflictDetector {
    /// 检查新交易是否与池内交易冲突
    pub fn detect_conflicts(&self, tx: &CellTx) -> Vec<TxHash> {
        let mut conflicts = Vec::new();
        
        for input in &tx.inputs {
            if let Some(existing_spender) = self.cell_spenders.get(&input.out_point) {
                conflicts.push(*existing_spender);
            }
        }
        
        conflicts
    }
    
    /// 裁决冲突（选择保留哪个交易）
    /// 规则：fee_rate ↓ → blue_pref ↑ → first_seen ↑
    pub fn resolve_conflict(
        &self,
        tx1: &CellEntry,
        tx2: &CellEntry,
    ) -> ConflictResolution {
        // 1. 费率比较
        let fee_rate1 = tx1.fee as f64 / tx1.size as f64;
        let fee_rate2 = tx2.fee as f64 / tx2.size as f64;
        
        if (fee_rate1 - fee_rate2).abs() > 0.01 {
            return if fee_rate1 > fee_rate2 {
                ConflictResolution::KeepFirst
            } else {
                ConflictResolution::KeepSecond
            };
        }
        
        // 2. 蓝色偏好（可选：依赖更多蓝块的交易优先）
        // TODO: 实现 blue_preference 计算
        
        // 3. 先到先得（时间戳）
        if tx1.timestamp < tx2.timestamp {
            ConflictResolution::KeepFirst
        } else {
            ConflictResolution::KeepSecond
        }
    }
}

pub enum ConflictResolution {
    KeepFirst,
    KeepSecond,
    KeepBoth,  // 不冲突
}
```

---

## 10. POW 共识层 + Cell 验证（DAG-Aware）

### 10.1 GhostDAG + Cell 集成

```rust
// consensus/src/processes/ghostdag/mod.rs

/// GhostDAG 共识（保持不变，但块验证改为 Cell）
pub struct GhostdagManager {
    pub k: u64,  // 参数 K（父块数量上限）
    pub genesis_hash: [u8; 32],
}

impl GhostdagManager {
    /// 计算区块的蓝分（Blue Score）
    pub fn ghostdag(&self, block_hash: &[u8; 32], store: &impl BlockStore) -> GhostdagData {
        // GhostDAG 算法（不变）
        // 返回：blue_score, blue_set, red_set, selected_parent
    }
}

/// GhostDAG 数据（每个块的共识信息）
pub struct GhostdagData {
    pub blue_score: u64,
    pub blue_work: u128,
    pub selected_parent: [u8; 32],
    pub mergeset_blues: Vec<[u8; 32]>,
    pub mergeset_reds: Vec<[u8; 32]>,
}
```

### 10.2 块验证流程（Cell 化）

```rust
// consensus/src/processes/block_validator.rs

pub struct BlockValidator {
    ghostdag_manager: Arc<GhostdagManager>,
    cell_provider: Arc<dyn DagCellProvider>,
    consensus_params: Arc<ConsensusParams>,
}

impl BlockValidator {
    /// 验证区块（Cell 版本）
    pub fn validate_block(&self, block: &Block) -> Result<BlockStatus, ValidationError> {
        // 1. 验证 PoW（保持不变）
        self.validate_pow(block)?;
        
        // 2. 验证区块头（包含 cell_root）
        self.validate_header(block)?;
        
        // 3. 验证 cellbase 交易（挖矿奖励）
        let cellbase = block.transactions.first()
            .ok_or(ValidationError::MissingCellbase)?;
        self.validate_cellbase(cellbase, block)?;
        
        // 4. 验证所有普通交易（Cell 交易）
        for tx in block.transactions.iter().skip(1) {
            self.validate_cell_transaction(tx, block)?;
        }
        
        // 5. 验证 cell_root 承诺（DAG 的 Cell 状态根）
        self.validate_cell_root(block)?;
        
        // 6. GhostDAG 排序（确定蓝/红集合）
        let ghostdag_data = self.ghostdag_manager.ghostdag(&block.hash(), self)?;
        
        Ok(BlockStatus::Valid(ghostdag_data))
    }
    
    /// 验证 Cell 交易（在 DAG 上下文）
    fn validate_cell_transaction(
        &self,
        tx: &CellTx,
        block: &Block,
    ) -> Result<(), ValidationError> {
        // 1. 解析交易：加载所有输入 Cells
        let rtx = self.resolve_transaction(tx, block.daa_score())?;
        
        // 2. 验证基础规则
        self.validate_cell_tx_basic(&rtx)?;
        
        // 3. 验证 Cell 可用性（未被花费）
        self.validate_cell_availability(&rtx, block.daa_score())?;
        
        // 4. 验证容量守恒
        self.validate_capacity_conservation(&rtx)?;
        
        // 5. 验证脚本（Lock + Type）
        let verifier = CellTxVerifier::new(
            Arc::new(rtx),
            self.cell_provider.clone(),
            self.consensus_params.clone(),
            Arc::new(DagTxEnv {
                current_daa_score: block.daa_score(),
                parent_hashes: block.parent_hashes().to_vec(),
                block_timestamp: block.timestamp(),
            }),
        );
        verifier.verify(self.consensus_params.max_block_cycles)?;
        
        Ok(())
    }
    
    /// 解析交易（DAG 版本）
    fn resolve_transaction(
        &self,
        tx: &CellTx,
        at_daa_score: u64,
    ) -> Result<ResolvedCellTx, Error> {
        let mut resolved_inputs = Vec::with_capacity(tx.inputs.len());
        let mut resolved_deps = Vec::with_capacity(tx.deps.len());
        
        // 解析 inputs（必须是 Live）
        for input in &tx.inputs {
            let status = self.cell_provider.cell(&input.out_point, Some(at_daa_score));
            match status {
                CellStatus::Live(meta) => {
                    // 检查 cellbase 成熟度
                    if meta.is_cellbase() {
                        let maturity_daa = meta.transaction_info.unwrap().daa_score
                            + self.consensus_params.cellbase_maturity;
                        if at_daa_score < maturity_daa {
                            return Err(Error::CellbaseNotMature);
                        }
                    }
                    resolved_inputs.push(meta);
                }
                CellStatus::Dead(spent_at) => {
                    return Err(Error::CellAlreadySpent { spent_at });
                }
                CellStatus::Unknown => {
                    return Err(Error::CellNotFound);
                }
            }
        }
        
        // 解析 deps（只读依赖）
        for dep in &tx.deps {
            let status = self.cell_provider.cell(&dep.out_point, Some(at_daa_score));
            if let CellStatus::Live(meta) = status {
                resolved_deps.push(meta);
            } else {
                return Err(Error::DepCellNotFound);
            }
        }
        
        Ok(ResolvedCellTx {
            transaction: tx.clone(),
            resolved_inputs,
            resolved_deps,
        })
    }
    
    /// 验证 cell_root 承诺（DAG 状态根）
    fn validate_cell_root(&self, block: &Block) -> Result<(), Error> {
        // 1. 计算块内所有交易创建/花费的 Cells
        let mut cell_changes = Vec::new();
        for tx in &block.transactions {
            for input in &tx.inputs {
                cell_changes.push(CellChange::Spent(input.out_point.clone()));
            }
            for (i, output) in tx.outputs.iter().enumerate() {
                let out_point = OutPoint {
                    tx_hash: tx.hash(),
                    index: i as u32,
                };
                cell_changes.push(CellChange::Created(out_point, output.clone()));
            }
        }
        
        // 2. 应用变更到父块的 cell_root
        let parent_cell_root = self.get_parent_cell_root(block.selected_parent_hash())?;
        let new_cell_root = self.apply_cell_changes(parent_cell_root, &cell_changes)?;
        
        // 3. 验证与块头的 cell_root 一致
        if new_cell_root != block.header.cell_root {
            return Err(Error::CellRootMismatch {
                expected: new_cell_root,
                actual: block.header.cell_root,
            });
        }
        
        Ok(())
    }
}

/// Cell 变更
pub enum CellChange {
    Created(OutPoint, CellOut),
    Spent(OutPoint),
}
```

### 10.3 Cellbase 交易（挖矿奖励）

```rust
// consensus/src/processes/cellbase_builder.rs

pub struct CellbaseBuilder {
    consensus: Arc<ConsensusParams>,
}

impl CellbaseBuilder {
    /// 构建 cellbase 交易（Cell 版本）
    pub fn build_cellbase(
        &self,
        block_daa_score: u64,
        miner_lock_script: ScriptRef,
        mergeset_rewards: &[(u64, ScriptRef)],  // (daa_score, miner_lock) from red blocks
    ) -> CellTx {
        let block_reward = self.calculate_block_reward(block_daa_score);
        
        let mut outputs = Vec::new();
        
        // 主矿工奖励
        outputs.push(CellOut {
            lock: miner_lock_script.clone(),
            type_: None,
            capacity: block_reward,
            data: vec![],
        });
        
        // Mergeset 奖励（DAG 特有：红块矿工也获得部分奖励）
        for (red_daa, red_miner_lock) in mergeset_rewards {
            let red_reward = block_reward / 2;  // 红块奖励减半
            outputs.push(CellOut {
                lock: red_miner_lock.clone(),
                type_: None,
                capacity: red_reward,
                data: vec![],
            });
        }
        
        CellTx {
            ver: 0xC001,
            inputs: vec![],  // cellbase 无输入
            deps: vec![],
            outputs,
            outputs_data: vec![vec![]; outputs.len()],
            witnesses: vec![],
        }
    }
}
```

### 10.4 交易打包（Cell Pool → Block）

```rust
// mining/src/block_template.rs

pub struct BlockTemplateBuilder {
    cell_pool: Arc<CellPool>,
    ghostdag_manager: Arc<GhostdagManager>,
    cell_provider: Arc<dyn DagCellProvider>,
}

impl BlockTemplateBuilder {
    /// 生成区块模板（Cell 版本）
    pub fn build_template(
        &self,
        miner_lock: ScriptRef,
        parent_hashes: Vec<[u8; 32]>,
    ) -> Result<BlockTemplate, Error> {
        // 1. 从 CellPool 选择交易（按 fee rate 排序）
        let candidates = self.cell_pool.get_top_transactions(
            self.consensus.max_block_size,
            self.consensus.max_block_cycles,
        )?;
        
        // 2. 构建交易 DAG（检测冲突）
        let tx_dag = self.build_transaction_dag(&candidates)?;
        
        // 3. 拓扑排序 + 裁决冲突
        let selected_txs = self.select_transactions(tx_dag)?;
        
        // 4. 构建 cellbase
        let cellbase = self.build_cellbase_for_template(miner_lock, &parent_hashes)?;
        
        // 5. 计算 cell_root
        let cell_root = self.calculate_cell_root(&cellbase, &selected_txs)?;
        
        // 6. 组装区块头
        let header = BlockHeader {
            version: 1,
            timestamp: current_timestamp(),
            parent_hashes,
            cell_root,
            tx_root: self.calculate_tx_merkle_root(&selected_txs)?,
            nonce: 0,  // 待矿工填充
            bits: self.calculate_target_bits()?,
        };
        
        Ok(BlockTemplate {
            header,
            cellbase,
            transactions: selected_txs,
        })
    }
    
    /// 构建交易 DAG（检测 Cell 冲突）
    fn build_transaction_dag(&self, txs: &[CellTx]) -> Result<TxDag, Error> {
        let mut dag = TxDag::new();
        let mut cell_producers = HashMap::new();  // OutPoint -> TxIndex
        
        for (i, tx) in txs.iter().enumerate() {
            // 记录此交易产生的 Cells
            for (j, _) in tx.outputs.iter().enumerate() {
                let out_point = OutPoint {
                    tx_hash: tx.hash(),
                    index: j as u32,
                };
                cell_producers.insert(out_point, i);
            }
            
            // 检查依赖
            for input in &tx.inputs {
                if let Some(&producer_idx) = cell_producers.get(&input.out_point) {
                    // 依赖关系：producer -> consumer
                    dag.add_edge(producer_idx, i);
                } else {
                    // 依赖块外的 Cell（从状态读取）
                    if !self.cell_provider.is_live(&input.out_point, None) {
                        return Err(Error::CellNotAvailable);
                    }
                }
            }
            
            // 检查冲突（双花）
            for input in &tx.inputs {
                if let Some(conflict_tx) = dag.find_conflict(&input.out_point) {
                    dag.add_conflict(i, conflict_tx);
                }
            }
        }
        
        Ok(dag)
    }
}
```

### 10.5 Spora 共识接口（预留）

```rust
// consensus/spora/src/interface.rs

/// Spora 共识接口（扩展 GhostDAG）
pub trait SporaConsensus {
    /// 计算块权重（Spora：DA + 执行 + 拓扑）
    fn compute_weight(&self, block: &Block) -> f64 {
        let da_weight = self.da_quality_weight(block);
        let exec_weight = self.execution_proof_weight(block);
        let topo_weight = self.topology_quality_weight(block);
        
        da_weight * 0.4 + exec_weight * 0.4 + topo_weight * 0.2
    }
    
    /// DA 质量权重（NMT 抽样成功率）
    fn da_quality_weight(&self, block: &Block) -> f64;
    
    /// 执行证明权重（zkSNARK / Fraud Proof）
    fn execution_proof_weight(&self, block: &Block) -> f64;
    
    /// 拓扑质量权重（蓝色比例）
    fn topology_quality_weight(&self, block: &Block) -> f64 {
        let ghostdag_data = self.get_ghostdag_data(&block.hash());
        ghostdag_data.mergeset_blues.len() as f64 
            / (ghostdag_data.mergeset_blues.len() + ghostdag_data.mergeset_reds.len()) as f64
    }
}
```

---

## 11. 测试向量（testvectors/，Agent 必跑）

* **S1 序列化/端序**：`ok_le_all`, `bad_len_over`, `bad_unknown_field`
* **S2 并发/裁决**：双花、跨层依赖、稳定排序
* **S3 重组/回放**：DAG 分叉、K 内翻转、>K 锁定
* **S4 VM 严验**：非规范签名、公钥重复、多签索引错位
* **S5 DA 抽样**：NMT 路径错误、段未封口
* **S6 错误码映射**：`CONSENSUS_* / EXEC_* / DOS_*` 一致

---

## 12. 提交/PR 规范（Cursor 重要）

* **commit 前缀**：`cell(exec|state|vm|scheduler|proto|p2p|test|cleanup): ...`
  * `cell(cleanup)`: 删除 UTXO 相关代码
  * `cell(exec)`: 执行层实现
  * `cell(state)`: 状态层实现
  * `cell(vm)`: VM 集成
  * `cell(test)`: 测试向量
  
* **⚠️ 重要规则**：
  * **禁止**保留任何 UTXO 代码（包括注释掉的代码）
  * **禁止**创建 UTXO 兼容层或过渡方案
  * 删除代码时**必须**同步更新 Cargo.toml 和文档
  * 每个 PR **必须**能通过编译（即使功能未完成）

* **PR 必带**：
  * 结构图/时序图（/docs/ 内）
  * bench：并行层宽、P50/P99 验证时延、吞吐
  * testvectors 通过截图/日志
  * 删除清单（如果涉及删除）
  
* **CI Gate**：`cargo test --workspace` + `testvectors all green` + `cargo clippy -- -D warnings`

---

## 13. 逐步执行脚本（Cursor Task List）

### 阶段 0：准备与清理（1-2 天）

**任务 0.1：UTXO 依赖全面扫描**
```bash
cd /home/arthur/RustRoverProjects/Tondi
# 扫描所有 UTXO 相关代码
rg -n "utxo|UTXO|UtxoEntry|script_pub_key|ScriptPublicKey" \
  --type rust consensus/ mining/ indexes/ > utxo_scan_full.txt

# 统计文件分布（决定删除顺序）
rg --type rust -c "UTXO|utxo" consensus/ | sort -t: -k2 -rn | head -20

# 找出所有依赖 utxoindex 的地方
rg "tondi-utxoindex|use.*utxo" --type rust -l
```

**任务 0.2：删除 UTXO 模块（不可逆操作，谨慎执行）**
```bash
# ⚠️ 确认你在 spora 分支！
git branch --show-current  # 应该输出 spora

# 1. 删除 utxoindex 整个目录
rm -rf indexes/utxoindex

# 2. 删除 UTXO 验证器
rm -f consensus/src/processes/transaction_validator/tx_validation_in_utxo_context.rs

# 3. 从 workspace 移除
# 编辑 Cargo.toml，删除以下行：
#   "indexes/utxoindex",
# 以及 [workspace.dependencies] 中的：
#   tondi-utxoindex = { ... }

# 4. 提交删除（保留 git 历史）
git add -A
git commit -m "cell(cleanup): Remove UTXO model completely

- Delete indexes/utxoindex crate
- Remove UTXO validation logic
- Remove from workspace dependencies

BREAKING CHANGE: UTXO model is no longer supported"
```

**任务 0.3：创建 Cell 工作目录骨架**
```bash
# 创建新模块目录
mkdir -p exec/src/{celltx,scheduler,vm,scripts}
mkdir -p state/src/{index,store}
mkdir -p mempool/src
mkdir -p consensus/spora/src
mkdir -p indexes/cellindex/src
mkdir -p testvectors/{s1_serialization,s2_concurrency,s3_reorg,s4_vm,s5_da,s6_errors}

# 创建占位 README
echo "# Cell Execution Layer" > exec/README.md
echo "# Cell State Management" > state/README.md
echo "# Cell Memory Pool" > mempool/README.md
```

**任务 0.4：研究 CKB 参考实现**
```bash
# 查看 CKB Cell 定义
cat /home/arthur/RustRoverProjects/ckb/util/types/src/core/cell.rs
# 查看 CKB 脚本接口
cat /home/arthur/RustRoverProjects/ckb/script/src/verify.rs
# 查看 CKB 交易池
ls -la /home/arthur/RustRoverProjects/ckb/tx-pool/src/
```

### 阶段 1：核心类型定义（2-3 天）

**任务 1.1：创建 `exec` crate**
```bash
cd exec
cargo init --lib
# 编辑 Cargo.toml，添加依赖：blake3, borsh, serde
```

**任务 1.2：实现 `exec/src/celltx/types.rs`**
- 定义 `CellRef`, `ScriptRef`, `CellOut`, `CellTx`
- 实现 `borsh::BorshSerialize` 和 `BorshDeserialize`
- 单元测试：序列化/反序列化往返
- **参考**：`/home/arthur/RustRoverProjects/ckb/util/types/src/core/cell.rs`

**任务 1.3：实现 `exec/src/celltx/sighash.rs`**
```rust
// 域常量
pub const CELL_SIG_DOMAIN: &[u8] = b"Cell/sig";
pub const CELL_TXID_DOMAIN: &[u8] = b"Cell/txid";

// 计算 wtxid（带见证）
pub fn compute_wtxid(tx: &CellTx) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(CELL_TXID_DOMAIN);
    hasher.update(&tx.ver.to_le_bytes());
    // ... 序列化所有字段
    hasher.finalize().into()
}

// 计算签名哈希
pub fn compute_sighash(
    tx: &CellTx,
    input_index: usize,
    network_id: u8,
    rw_commitment: &[u8; 32],
) -> [u8; 32] {
    let wtxid = compute_wtxid(tx);
    let mut hasher = blake3::Hasher::new();
    hasher.update(CELL_SIG_DOMAIN);
    hasher.update(&[network_id]);
    hasher.update(&wtxid);
    hasher.update(&(input_index as u32).to_le_bytes());
    hasher.update(rw_commitment);
    hasher.finalize().into()
}
```

**任务 1.4：编写 testvectors/s1_serialization**
- 测试小端序、越界、未知字段处理

### 阶段 2：调度器与冲突裁决（3-5 天）

**任务 2.1：实现 `exec/src/scheduler/dag.rs`**
```rust
pub struct CellDAG {
    nodes: Vec<NodeId>,
    edges: HashMap<NodeId, Vec<NodeId>>,
    conflicts: Vec<(NodeId, NodeId)>,
}

impl CellDAG {
    pub fn build(txs: &[CellTx]) -> Self {
        // 1. 构建 RW-Set：inputs/deps/outputs
        // 2. 建图：A.outputs ∩ B.inputs → A→B
        // 3. 冲突检测：A.inputs ∩ B.inputs
    }
}
```

**任务 2.2：实现 `exec/src/scheduler/conflict.rs`**
```rust
pub fn resolve_conflicts(
    conflicts: Vec<(NodeId, NodeId)>,
    txs: &[CellTx],
    blue_scores: &HashMap<NodeId, u64>,
) -> Vec<NodeId> {
    // 裁决键：fee_density ↓ → blue_pref ↑ → wtxid ↑
    // 返回赢家列表
}
```

**任务 2.3：实现 `exec/src/scheduler/executor.rs`**
```rust
pub fn execute_parallel(
    dag: &CellDAG,
    txs: &[CellTx],
    pool: &rayon::ThreadPool,
) -> Result<Vec<Receipt>, Error> {
    // 1. 拓扑分层：topo_levels(dag)
    // 2. 按层并行执行
    // 3. 返回执行收据
}
```

**任务 2.4：编写 testvectors/s2_concurrency**
- 双花场景、跨层依赖、稳定排序验证

### 阶段 3：VM 集成（5-7 天）

**任务 3.1：添加 CKB-VM 依赖**
```toml
[dependencies]
ckb-vm = "0.24"  # 查看最新版本
```

**任务 3.2：实现 `exec/src/vm/interface.rs`**
```rust
pub trait ScriptVerifier {
    fn verify_lock(&self, lock: &ScriptRef, tx: &CellTx, input_idx: usize) -> Result<(), VMError>;
    fn verify_type(&self, type_: &ScriptRef, tx: &CellTx, output_idx: usize) -> Result<(), VMError>;
}
```

**任务 3.3：实现 `exec/src/vm/ckbvm.rs`**
- 参考 `/home/arthur/RustRoverProjects/ckb/script/src/verify.rs`
- 实现系统调用：`load_cell`, `load_tx_hash`, `load_input`, etc.

**任务 3.4：实现标准锁脚本 `exec/src/scripts/secp256k1_lock.rs`**
- Secp256k1 签名验证（参考当前 `crypto/txscript`）

**任务 3.5：编写 testvectors/s4_vm**
- 非规范签名、多签、错误码测试

### 阶段 4：状态层（4-6 天）

**任务 4.1：创建 `state` crate**
```bash
cd state
cargo init --lib
# 添加依赖：rocksdb, parking_lot, borsh
```

**任务 4.2：实现 `state/src/index/cell_db.rs`**
```rust
pub struct CellDB {
    db: Arc<rocksdb::DB>,
}

pub struct CellMeta {
    pub segment_id: u32,
    pub offset: u64,
    pub len: u32,
    pub lock_hash: [u8; 32],
    pub type_hash: Option<[u8; 32]>,
}

impl CellDB {
    pub fn put(&self, cell_id: &[u8; 32], meta: &CellMeta) -> Result<()>;
    pub fn get(&self, cell_id: &[u8; 32]) -> Result<Option<CellMeta>>;
}
```

**任务 4.3：实现 `state/src/store/segment.rs`**
```rust
pub struct SegmentWriter {
    current_segment: File,
    segment_id: u32,
    offset: u64,
}

impl SegmentWriter {
    pub fn append_cell(&mut self, cell: &CellOut) -> Result<(u32, u64, u32)> {
        // 返回 (segment_id, offset, len)
    }
    
    pub fn seal(&mut self) -> Result<SegmentRoot> {
        // 计算 NMT/KZG 承诺
    }
}
```

**任务 4.4：实现 `state/src/store/proof.rs`**
- NMT（Namespaced Merkle Tree）或 KZG 承诺
- 抽样验证接口

**任务 4.5：编写 testvectors/s5_da**

### 阶段 5：共识集成（3-4 天）

**任务 5.1：创建 `consensus/spora` crate**
```bash
mkdir -p consensus/spora/src
cd consensus/spora
cargo init --lib
```

**任务 5.2：定义共识接口 `consensus/spora/src/interface.rs`**
```rust
pub trait Consensus {
    fn order_block(&self, block: &Block) -> (u64, BlockMeta);  // BlueScore
    fn validate_block(&self, block: &Block) -> Result<(), ConsensusError>;
    fn compute_weight(&self, block: &Block) -> f64;  // Spora 权重
}
```

**任务 5.3：在 `consensus/core` 中添加 `cell_root` 到区块头**
```rust
pub struct Header {
    // ... 现有字段
    pub cell_root: [u8; 32],     // ns=1 命名空间根
    pub legacy_root: [u8; 32],   // ns=0 占位（全零）
}
```

**任务 5.4：实现 `consensus/src/processes/cell_validator/`**
- 替代 `transaction_validator` 的 Cell 版本

### 阶段 6：Mempool（2-3 天）

**任务 6.1：创建 `mempool` crate**
```bash
mkdir -p mempool/src
cd mempool
cargo init --lib
```

**任务 6.2：实现 `mempool/src/cellpool.rs`**
- Cell 交易队列
- 依赖追踪（父子关系）

**任务 6.3：实现 `mempool/src/scorer.rs`**
```rust
pub fn compute_score(tx: &CellTx, context: &Context) -> f64 {
    let fee_density = tx.fee as f64 / tx.mass() as f64;
    let unlockability = compute_unlockability(tx);
    let deps_width = tx.deps.len() as f64;
    
    fee_density * 0.6 + unlockability * 0.3 - deps_width * 0.1
}
```

**任务 6.4：实现 RBF/CPFP 规则**

### 阶段 7：P2P 扩展（2-3 天）

**任务 7.1：在 `protocol/p2p` 中添加 Cell 消息**
```rust
// protocol/p2p/src/gossip/cell_inv.rs
pub struct CellInvMessage {
    pub cell_ids: Vec<[u8; 32]>,
}
```

**任务 7.2：实现 DA 抽样拉取**
```rust
// protocol/p2p/src/gossip/da_sample.rs
pub async fn sample_segment(segment_id: u32, peer: &Peer) -> Result<SampleProof>;
```

### 阶段 8：集成测试与基准（3-5 天）

**任务 8.1：编写 testvectors/s3_reorg**
- DAG 重组、K 内翻转

**任务 8.2：编写 testvectors/s6_errors**
- 错误码一致性验证

**任务 8.3：运行完整测试套件**
```bash
cargo test --workspace
cargo test --package exec --lib
cargo test --package state --lib
cargo test --package mempool --lib
```

**任务 8.4：基准测试**
```bash
# 并行层宽测试
cargo bench --bench scheduler_layers

# TPS 测试
cargo bench --bench tps_baseline
# 目标：≥30k TPS，P99 < 2× 基线
```

### 阶段 9：文档与 PR（2-3 天）

**任务 9.1：生成架构文档**
- `/docs/cell_architecture.md`：整体架构
- `/docs/cell_tx_format.md`：交易格式规范
- `/docs/scheduler_algorithm.md`：调度算法

**任务 9.2：更新 Cargo.toml workspace**
```toml
members = [
    # ... 现有成员
    "exec",
    "state",
    "mempool",
    "consensus/spora",
]
```

**任务 9.3：CI 配置**
- 确保所有测试通过
- 添加 clippy 和 rustfmt 检查

**任务 9.4：提交 PR**
```bash
git add .
git commit -m "cell(foundation): Initial Cell model implementation

- Add exec crate with CellTx types, sighash, scheduler
- Add state crate with Cell indexing and DA storage
- Add mempool crate with cellpool and scoring
- Add consensus/spora interface (placeholder)
- Add testvectors S1-S6
- Deprecate indexes/utxoindex

Ref: CURSOR_RULES.md Phase 1-8"

git push origin spora
```

---

## 14. KPI（第一阶段验收线）

* 并发层宽 ≥ 3；拓扑执行中位延迟 ≤ 1.8s
* 稳态 ≥ 30k TPS（单机/本地网），P99 验证 < 2× 基线
* testvectors 全绿；重组回放一致；错误码一致

---

## 15. 代码风格/安全

* **禁止**全局可变单例；使用依赖注入
* **所有地图结构**（map/HashMap）涉及共识逻辑**必须**有稳定遍历顺序
* **panic=abort** 禁止出现在共识路径；返回带枚举错误码
* 每个外部接口都要有 **fuzz 入口**（序列化/端序/长度/索引）

---

## 16. 从 CKB 学习到的核心概念（完整总结）

### 16.1 Cell 模型精髓（vs UTXO）

**CKB Cell 的三要素**：
1. **Lock Script**：谁能花费（类似 UTXO 的 ScriptPubKey）
2. **Type Script**（可选）：状态转移约束（UTXO 没有）
3. **Data**：任意数据（UTXO 只有金额）

**关键差异**：
- UTXO：`value + scriptPubKey`（简单）
- Cell：`capacity + lock + type + data`（图灵完备）
- Cell 的 `capacity` 包含存储成本（防状态爆炸）

### 16.2 脚本验证范式

**CKB 验证流程**：
1. **脚本分组**：相同 `code_hash + args` 的 Cell 合并验证（优化）
2. **Lock Script**：验证每个 input 的花费权限
3. **Type Script**：验证输入输出的状态转移（如 UDT 总量守恒）
4. **CKB-VM**：RISC-V 虚拟机，通过系统调用访问交易数据

**系统调用关键**：
- `LoadCell`：加载 input/output/deps Cells
- `LoadCellData`：加载 Cell data
- `LoadWitness`：加载见证数据（签名）
- `LoadHeader`：加载区块头（时间锁验证）
- `Exec`：动态加载脚本（组合性）

### 16.3 依赖机制（CellDep）

**两种依赖类型**：
1. **Code**：单个 Cell 作为脚本代码
2. **DepGroup**：一个 Cell 包含多个 OutPoint（批量依赖优化）

**为何需要 deps**：
- 脚本代码本身也是 Cell（链上代码）
- 避免每次交易都携带完整脚本
- 支持脚本升级（改变 code_hash）

### 16.4 Capacity 机制（状态租金）

```rust
// CKB 的核心约束
cell.capacity >= occupied_capacity(cell)

occupied_capacity(cell) = 
    size_of(cell.capacity) +
    size_of(cell.lock) +
    size_of(cell.type) +
    size_of(cell.data)
```

**意义**：
- 每个 Cell 必须付费占用链上存储
- 防止状态爆炸攻击
- 激励状态回收（销毁 Cell 释放 capacity）

### 16.5 交易池（TxPool）设计

**CKB TxPool 特性**：
1. **多索引结构**：按 id, score, status 索引
2. **Edges 追踪**：inputs, deps, header_deps 依赖关系
3. **祖先/后代统计**：CPFP（Child Pays For Parent）
4. **RBF**：Replace-By-Fee（费率必须更高）
5. **驱逐策略**：内存满时踢出低费率交易

**评分公式**：
```rust
ancestors_score = ancestors_fee / ancestors_size * cycles_factor * age_factor
```

### 16.6 DAG 适配要点（Tondi 特有）

**1. DAA 分数（vs BlockNumber）**：
- CKB 用 `block_number` 表示高度
- Tondi 用 `daa_score`（GhostDAG 蓝分）
- 所有成熟度检查改用 DAA 分数

**2. Cell 状态查询**：
```rust
// DAG 重组感知
fn cell(&self, out_point: &OutPoint, at_daa_score: Option<u64>) -> CellStatus;
```

**3. Cellbase 处理**：
- 蓝块矿工：全额奖励
- 红块矿工：减半奖励（或部分比例）
- Mergeset 机制（DAG 特有）

**4. Cell Root 承诺**：
- CKB：线性累积（简单）
- Tondi：DAG 状态树（需选择父块状态）

**5. 交易打包**：
- CKB：顺序打包（拓扑排序）
- Tondi：DAG 冲突检测 + 裁决

### 16.7 性能优化（从 CKB 借鉴）

**1. 脚本分组**：
- 相同脚本只运行一次
- 减少 VM 初始化开销

**2. 并行验证**：
- 不同脚本组并行执行
- 阈值：100+ inputs 才并行

**3. 缓存策略**：
- Cell data 内存缓存（`mem_cell_data`）
- Data hash 缓存（`mem_cell_data_hash`）
- 签名验证缓存（secp256k1）

**4. Freezer（归档）**：
- 古老区块数据移到 cold storage
- 索引保留，数据按需加载

### 16.8 安全考量

**1. DoS 防护**：
- 最大 cycles 限制（70M）
- 最大脚本大小（500KB）
- 最大内存（8MB）
- 超时机制（VM 执行）

**2. Capacity 验证**：
```rust
sum(inputs.capacity) >= sum(outputs.capacity) + fee
每个 output.capacity >= occupied_capacity(output)
```

**3. 双花检测**：
- CellPool 追踪所有 OutPoint
- 冲突交易触发 RBF 或拒绝

**4. 时间锁**：
```rust
since 字段：
  bit 63: 相对锁(1) vs 绝对锁(0)
  bit 62: DAA分数(1) vs 时间戳(0)
  bit 61-0: 锁定值
```

### 16.9 术语对照表

| CKB | Tondi-Cell | UTXO (旧) | 说明 |
|-----|-----------|----------|------|
| Cell | Cell | UTXO | 基本状态单元 |
| OutPoint | OutPoint | OutPoint | 引用（tx_hash + index） |
| Lock Script | Lock Script | ScriptPubKey | 花费条件 |
| Type Script | Type Script | - | 状态转移约束 |
| CellDep | CellDep | - | 只读依赖 |
| Capacity | Capacity | Value | 金额 + 存储费 |
| Block Number | DAA Score | Block Height | 区块序号 |
| Cellbase | Cellbase | Coinbase | 挖矿奖励 |
| TxPool | CellPool | Mempool | 交易池 |
| ResolvedTransaction | ResolvedCellTx | - | 已解析交易 |
| CKB-VM | CellVM | Script Engine | 虚拟机 |

### 16.10 关键文件参考（CKB 源码）

**必读文件**：
1. `ckb/util/types/src/core/cell.rs` - Cell 核心定义
2. `ckb/script/src/verify.rs` - 脚本验证器
3. `ckb/tx-pool/src/component/pool_map.rs` - 交易池实现
4. `ckb/script/src/syscalls/` - 系统调用实现
5. `ckb/store/src/transaction.rs` - 存储接口
6. `ckb/traits/src/` - 核心 trait 定义

**推荐阅读**：
- CKB RFC 文档：https://github.com/nervosnetwork/rfcs
- 特别关注：RFC-0002（交易结构），RFC-0004（VM），RFC-0022（交易池）

---

## 17. 术语速查

* **CellTx**：新交易；`ver=0xC001`
* **OutPoint**：`tx_hash || index`，唯一标识一个 Cell
* **Lock Script**：谁能花费（签名验证）
* **Type Script**：状态转移约束（可选）
* **CellDep**：只读依赖（脚本代码）
* **Capacity**：金额 + 存储租金
* **DAA Score**：GhostDAG 蓝分（替代区块高度）
* **Cellbase**：挖矿奖励交易（DAG 支持 mergeset 奖励）
* **ResolvedCellTx**：已解析交易（输入 Cells 已加载）
* **CellProvider**：查询 Cell 状态的接口（Live/Dead/Unknown）
* **RW-Set**：`inputs/deps/outputs` 声明
* **CellDAG**：依赖图；拓扑分层并行
* **ns-root**：命名空间根承诺（`cell_root`）
* **segment/chunk**：DA 段/块；NMT/KZG 承诺
* **CPFP**：Child Pays For Parent（子交易带动父交易）
* **RBF**：Replace-By-Fee（替换交易）

---

---

## 17. 立即行动指南

### 第一步：清理 UTXO（今天，必须完成）

**A. 扫描 UTXO 依赖（全面盘点）**
```bash
cd /home/arthur/RustRoverProjects/Tondi
# 扫描所有 UTXO 引用
rg -n "utxo|UTXO" --type rust -c consensus/ indexes/ mining/ | sort -t: -k2 -rn > utxo_hotspots.txt
cat utxo_hotspots.txt

# 找出依赖 utxoindex 的模块
rg "tondi-utxoindex" --type toml
rg "use.*utxo" --type rust -l > utxo_imports.txt
```

**B. 删除 UTXO 模块（⚠️ 不可逆操作）**
```bash
cd /home/arthur/RustRoverProjects/Tondi

# 确认在正确分支
git branch --show-current  # 必须是 spora

# 备份当前状态（可选）
git tag before-utxo-removal

# 删除 utxoindex
rm -rf indexes/utxoindex

# 删除 UTXO 验证器
rm -f consensus/src/processes/transaction_validator/tx_validation_in_utxo_context.rs

# 编辑 Cargo.toml 移除引用（手动或用 sed）
# 删除 members 中的 "indexes/utxoindex"
# 删除 workspace.dependencies 中的 tondi-utxoindex

# 提交删除
git add -A
git commit -m "cell(cleanup): Remove UTXO model completely

BREAKING CHANGE: UTXO model is no longer supported"
```

**C. 研究 CKB Cell 结构**
```bash
# Cell 定义
bat /home/arthur/RustRoverProjects/ckb/util/types/src/core/cell.rs | head -100
# 交易验证
bat /home/arthur/RustRoverProjects/ckb/script/src/verify.rs | head -150
# 交易池
ls -la /home/arthur/RustRoverProjects/ckb/tx-pool/src/
```

**D. 创建 Cell 工作目录骨架**
```bash
cd /home/arthur/RustRoverProjects/Tondi
mkdir -p exec/src/{celltx,scheduler,vm,scripts}
mkdir -p state/src/{index,store}
mkdir -p mempool/src
mkdir -p consensus/spora/src
mkdir -p indexes/cellindex/src
mkdir -p testvectors/{s1_serialization,s2_concurrency,s3_reorg,s4_vm,s5_da,s6_errors}

# 创建占位 README
echo "# Cell Execution Layer - CKB-inspired Cell model implementation" > exec/README.md
echo "# Cell State Management - DA storage with NMT/KZG proofs" > state/README.md
echo "# Cell Memory Pool - Parallel scheduler with RW-Set DAG" > mempool/README.md
```

### 第二步：实施阶段 1（2-3 天）

**优先级 P0：核心类型定义**
1. 创建 `exec/Cargo.toml`
2. 实现 `exec/src/celltx/types.rs`（参考 CKB cell.rs）
3. 实现 `exec/src/celltx/sighash.rs`（blake3 + 域前缀）
4. 编写单元测试

**验收标准**：
- `cargo test --package exec` 全绿
- 序列化/反序列化往返测试通过
- sighash 计算结果稳定（固定测试向量）

### 第三步：实施阶段 2（3-5 天）

**优先级 P0：调度器**
1. 实现 `exec/src/scheduler/dag.rs`（RW-Set 构图）
2. 实现 `exec/src/scheduler/conflict.rs`（冲突裁决）
3. 实现 `exec/src/scheduler/executor.rs`（拓扑分层并行）
4. 编写 testvectors/s2_concurrency

**验收标准**：
- 双花场景正确裁决
- 并行层宽 ≥ 3
- 冲突裁决稳定（确定性）

### 时间线估算（单人全职）

| 阶段 | 天数 | 累计 | 关键输出 |
|------|------|------|----------|
| 0 - 准备 | 2 | 2 | UTXO 扫描报告 + 目录结构 |
| 1 - 核心类型 | 3 | 5 | exec/celltx 可编译+测试通过 |
| 2 - 调度器 | 5 | 10 | 并行执行 demo |
| 3 - VM | 7 | 17 | CKB-VM 集成 + secp256k1 锁 |
| 4 - 状态层 | 6 | 23 | Cell 索引 + 段文件存储 |
| 5 - 共识集成 | 4 | 27 | cell_root 加入区块头 |
| 6 - Mempool | 3 | 30 | cellpool 运行 |
| 7 - P2P | 3 | 33 | Cell 消息中继 |
| 8 - 测试 | 5 | 38 | testvectors 全绿 + bench |
| 9 - 文档 | 3 | **41** | 架构文档 + PR |

**里程碑检查点**：
- **Day 5**：核心类型可用（M1）
- **Day 17**：VM 可验证脚本（M2）
- **Day 30**：Mempool 可用（M3）
- **Day 41**：完整系统上线（M4）

### 风险与缓解

| 风险 | 影响 | 概率 | 缓解措施 |
|------|------|------|----------|
| CKB-VM 集成复杂 | +5 天 | 中 | 先用简化 VM，后期替换 |
| 共识层侵入性大 | +3 天 | 高 | 保持接口抽象，最小化改动 |
| DA 存储性能差 | +4 天 | 中 | 先用简单追加，NMT 可选 |
| 测试覆盖不足 | +7 天 | 高 | 每阶段强制 TDD，不欠技术债 |

### 下一步行动（按优先级执行）

**🔴 优先级 P0：立即执行（今天，2-3 小时）**
```bash
cd /home/arthur/RustRoverProjects/Tondi

# 1. 确认分支
git branch --show-current  # 必须是 spora

# 2. 全面扫描 UTXO 依赖
rg -n "utxo|UTXO" --type rust -c consensus/ indexes/ mining/ | sort -t: -k2 -rn > utxo_hotspots.txt
rg "tondi-utxoindex" --type toml > utxo_cargo_deps.txt

# 3. 备份并删除
git tag before-utxo-removal
rm -rf indexes/utxoindex
rm -f consensus/src/processes/transaction_validator/tx_validation_in_utxo_context.rs

# 4. 查看需要手动修改的文件
cat utxo_cargo_deps.txt  # 需要手动编辑这些 Cargo.toml

# 5. 研究 CKB Cell 结构（学习参考）
bat /home/arthur/RustRoverProjects/ckb/util/types/src/core/cell.rs | head -100
```

**🟡 优先级 P1：今天完成（2-3 小时）**
```bash
# 1. 手动编辑 Cargo.toml
#    删除 "indexes/utxoindex" 和 tondi-utxoindex 依赖

# 2. 创建 Cell 工作目录
mkdir -p exec/src/{celltx,scheduler,vm,scripts}
mkdir -p state/src/{index,store}
mkdir -p mempool/src

# 3. 提交删除
git add -A
git commit -m "cell(cleanup): Remove UTXO model completely

BREAKING CHANGE: UTXO model is no longer supported"
```

**🟢 优先级 P2：明天开始（阶段 1）**
- 创建 `exec/Cargo.toml` 并添加依赖
- 实现 `exec/src/celltx/types.rs`（参考 CKB）
- 编写第一个单元测试
- 实现 `exec/src/celltx/sighash.rs`（blake3）

---

**联系方式**：如遇到阻塞问题，提供：
1. 当前阶段编号
2. 错误日志/编译输出
3. 已尝试的方案

把这份 `CURSOR_RULES.md` 放到仓库根目录后，从 **阶段 0 任务 0.1** 开始执行。骨架代码已在各阶段的任务描述中提供。
