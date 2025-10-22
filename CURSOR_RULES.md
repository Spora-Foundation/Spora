# CURSOR_RULES.md — Spora Fork Rules (CKB-inspired)

## 0. 目标/边界

* **目标**：在新分支 `spora` 中，**移除 UTXO 模型**，引入 **Cell 模型（lock/type/data + RW-Set）**，保持 **DAG 共识骨架**，先用 **GhostDAG**，为后续 **Spora 共识**预留挂载点。
* **参照**：CKB 的 **Cell 语义/脚本接口/CKB-VM 交互模式**，而**不复制**其线性链/NC-Max 共识。
  * **CKB 源码位置**：`/home/arthur/RustRoverProjects/ckb/` （可直接参考）
  * 重点参考：`ckb/script/`, `ckb/traits/`, `ckb/tx-pool/`, `ckb/store/`
* **一次性 fork**：不考虑与原 UTXO 的共存；保留少量过渡脚手架仅用于构建通过和回归测试替身。

**当前代码基础（Tondi v1.21.0）**：
- 语言：**Rust** (edition 2021, rustc 1.82.0)
- 共识：GhostDAG + UTXO
- 现有模块：`consensus/`, `indexes/utxoindex/`, `crypto/txscript/`, `database/`, `mining/`, `mempool/`（未独立）

---

## 1. 目录与模块落点（请按此新建/重构）

**新增 Crate 结构（基于 Tondi workspace）**：

```
Tondi/
├── consensus/
│   ├── core/           # 现有：保留 DAG/GhostDAG 核心
│   ├── spora/          # 新增 crate：Spora 共识接口与权重打分
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── weight.rs      # DA/执行证明/拓扑权重
│   │       └── interface.rs   # 共识切换接口
│   └── processes/
│       ├── cell_validator/    # 新增：替代 transaction_validator
│       │   ├── mod.rs
│       │   ├── cell_validation_in_isolation.rs
│       │   ├── cell_validation_in_context.rs
│       │   └── errors.rs
│       └── transaction_validator/  # 保留但标记 deprecated
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
│   ├── utxoindex/      # 现有：标记 deprecated
│   └── cellindex/      # 新增：Cell 索引服务
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

**待封存/标记 deprecated 的模块**：

* `indexes/utxoindex/` → 整个 crate 标记 `#[deprecated]`
* `consensus/src/processes/transaction_validator/tx_validation_in_utxo_context.rs` → 封存
* `consensus/core/src/utxo/` (如果存在) → 封存
* `crypto/txscript/` 中的 UTXO 特定逻辑 → 保留通用脚本引擎，移除 UTXO 假设

**扫描并标记待移除的关键词**（第一阶段不删除，只注释标记）：

```bash
# 在 consensus/ 和 mining/ 中扫描
rg -n "utxo|UTXO|UtxoEntry|script_pub_key|ScriptPublicKey" \
  consensus/src/ consensus/core/src/ mining/src/
```

**需要扫描的具体文件**（已知含 UTXO 逻辑）：

1. `consensus/src/processes/transaction_validator/tx_validation_in_utxo_context.rs` (361行)
2. `consensus/src/processes/transaction_validator/tx_validation_in_isolation.rs`
3. `consensus/core/src/tx.rs` - `VerifiableTransaction` trait
4. `indexes/utxoindex/` 整个目录
5. `wallet/` 相关 UTXO 假设（延后处理）

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

## 4. 关键类型（伪码）

```rust
// exec/celltx/types.rs
pub struct CellRef { pub id: [32]; pub since: u64 }          // 支持高度/时间锁
pub struct ScriptRef { pub code_hash: [32]; pub hash_type: u8; pub args: Vec<u8> }
pub struct CellOut {
  pub lock: ScriptRef,
  pub type_: Option<ScriptRef>,
  pub capacity: u64,  // 以字节/单位表示的资源上限
  pub data: Vec<u8>,  // TLV 编码
}
pub struct CellTx {
  pub ver: u16,       // = 0xC001
  pub inputs: Vec<CellRef>,
  pub deps: Vec<CellRef>,     // 只读依赖
  pub outputs: Vec<CellOut>,
  pub fee: u64,
  pub sigs: Vec<Vec<u8>>,     // Schnorr/BLS 可聚合（后续）
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

## 6. VM 与脚本接口（CKB 风格）

* `lock`：花费条件（签名/时间锁/多签）
* `type`：状态转移约束（容量守恒/白名单/数值关系）
* VM 选项：

  * 直接集成 **CKB-VM（RISC-V）** 作为 `exec/vm/ckbvm/`
  * 或先做 **WASM/AluVM** 适配层，接口一致
* 资源计量：`steps|max`, `mem|max`, `io|max`；超限 → `EXEC_VM_EXCEEDED`

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

## 9. mempool 与打包

* `cellpool/`：独立评分器：`score = fee_density·α + unlockability·β + deps_width·γ`
* 包中继：父子打分绑定；RBF/CPFP 规则文档化
* 禁止与 UTXO 队列混放（已放弃 UTXO，但保留接口桩便于回归）

---

## 10. 共识/切换挂载

* `consensus/iface.go`：

  ```go
  type Consensus interface {
    Order(*Block) (BlueScore, Meta)
    Validate(*Block) error
    Weight(*Block) float64 // Spora 用
  }
  ```
* 现用 `ghostdag` 实现；`spora/` 放接口与打分骨架（拓扑质量、DA 抽样、执行证明）

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

* **commit 前缀**：`cell(exec|state|vm|scheduler|proto|p2p|test): ...`
* **禁止**提交：对 `utxo*` 的"临时兼容层"以外的改动
* **PR 必带**：

  * 结构图/时序图（/docs/ 内）
  * bench：并行层宽、P50/P99 验证时延、吞吐
  * testvectors 通过截图/日志
* **CI Gate**：`go test ./...` 或 `cargo test` + `testvectors all green`

---

## 13. 逐步执行脚本（Cursor Task List）

### 阶段 0：准备与扫描（1-2 天）

**任务 0.1：UTXO 依赖扫描**
```bash
cd /home/arthur/RustRoverProjects/Tondi
# 扫描所有 UTXO 相关代码
rg -n "utxo|UTXO|UtxoEntry|script_pub_key|ScriptPublicKey" \
  --type rust consensus/ mining/ indexes/ > utxo_scan.txt

# 统计文件分布
rg --type rust -c "UTXO|utxo" consensus/ | sort -t: -k2 -rn | head -20
```

**任务 0.2：创建工作分支结构**
```bash
# 创建占位目录
mkdir -p exec/src/{celltx,scheduler,vm,scripts}
mkdir -p state/src/{index,store}
mkdir -p mempool/src
mkdir -p testvectors/{s1_serialization,s2_concurrency,s3_reorg,s4_vm,s5_da,s6_errors}
```

**任务 0.3：研究 CKB 参考实现**
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

## 16. 术语速查

* **CellTx**：新交易；`ver=0xC001`
* **RW-Set**：`inputs/deps/outputs` 声明
* **CellDAG**：依赖图；拓扑分层并行
* **ns-root**：命名空间根承诺（`cell_root`）
* **segment/chunk**：DA 段/块；NMT/KZG 承诺

---

---

## 17. 立即行动指南

### 第一步：了解现状（今天）

**A. 扫描 UTXO 依赖**
```bash
cd /home/arthur/RustRoverProjects/Tondi
rg -n "utxo|UTXO" --type rust -c consensus/ indexes/ mining/ | sort -t: -k2 -rn > utxo_hotspots.txt
cat utxo_hotspots.txt
```

**B. 研究 CKB Cell 结构**
```bash
# Cell 定义
bat /home/arthur/RustRoverProjects/ckb/util/types/src/core/cell.rs
# 交易验证
bat /home/arthur/RustRoverProjects/ckb/script/src/verify.rs
# 交易池
ls -la /home/arthur/RustRoverProjects/ckb/tx-pool/src/
```

**C. 创建工作目录骨架**
```bash
cd /home/arthur/RustRoverProjects/Tondi
mkdir -p exec/src/{celltx,scheduler,vm,scripts}
mkdir -p state/src/{index,store}
mkdir -p mempool/src
mkdir -p consensus/spora/src
mkdir -p testvectors/{s1_serialization,s2_concurrency,s3_reorg,s4_vm,s5_da,s6_errors}
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

### 下一步行动（建议优先级）

**立即执行（今天）**：
```bash
# 1. 运行 UTXO 扫描
cd /home/arthur/RustRoverProjects/Tondi
rg -n "utxo|UTXO" --type rust consensus/ indexes/ > utxo_scan.txt

# 2. 查看 CKB Cell 定义
bat /home/arthur/RustRoverProjects/ckb/util/types/src/core/cell.rs | head -100

# 3. 创建 exec crate
mkdir exec && cd exec
cargo init --lib
```

**明天开始**：
- 实现 `exec/src/celltx/types.rs`
- 编写第一个单元测试
- 参考 CKB 和 CURSOR_RULES.md § 4 的类型定义

---

**联系方式**：如遇到阻塞问题，提供：
1. 当前阶段编号
2. 错误日志/编译输出
3. 已尝试的方案

把这份 `CURSOR_RULES.md` 放到仓库根目录后，从 **阶段 0 任务 0.1** 开始执行。骨架代码已在各阶段的任务描述中提供。
