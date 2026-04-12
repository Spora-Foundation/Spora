# Spora Cell Mass 统一方案

> 日期：2026-04-12  
> 状态：Proposed  
> 范围：`consensus` / `mining` / `mempool` / `rpc` / `wallet`  
> 原则：不保留历史包袱，只保留必要的协议语义

---

## 1. 结论

Spora 当前的 `mass` / `cycles` 实现不应继续修补兼容层，而应直接收敛为一套统一资源模型：

- `CellTx` 只保留交易数据语义，不再承载共识资源计算逻辑
- 所有 `compute_mass` / `transient_mass` / `storage_mass` / `verified_cycles` / `effective_size` 都由单一资源计算器生成
- `mempool`、`block template`、`block validation`、`rpc verbose data` 使用同一份资源结果
- `storage mass` 必须只走上下文公式，不允许再用 output-footprint 近似值冒充
- `cycles` 必须使用真实 VM 验证结果，不允许再用 `compute_mass` 充当

这不是“重命名”问题，而是资源模型边界问题。

---

## 2. 当前问题

### 2.1 三套资源口径同时存在

当前代码里至少有三套彼此不完全一致的资源语义：

1. `exec/src/celltx/types.rs`
   - `CellTx::compute_mass()`
   - `CellTx::transient_mass()`
   - `CellTx::storage_mass()`
   - `CellTx::mass()`

2. `consensus/core/src/mass/mod.rs`
   - `MassCalculator::calc_non_contextual_masses_cell()`
   - `MassCalculator::calc_contextual_masses()`
   - `calc_storage_mass()`

3. `mempool` / `CellPool` / selector
   - 既使用 `calculated_non_contextual_masses`
   - 又把 `cell_tx.compute_mass()` 塞进 `cycles`
   - 又用 `tx.mass()` 参与 feerate / selection

结果是同一笔交易在不同模块下可能有不同的“质量”。

### 2.2 `storage mass` 没有真正进入主路径

Spora 已经实现了 KIP-0009 风格的上下文 `storage mass` 公式，但在主路径上仍然存在两个问题：

- 块级 isolation 验证直接取 `CellTx::storage_mass()`
- mempool feerate 仍把 `tx.mass()` 当成 storage side

而 `tx.mass()` 只是 output footprint 的别名，不是 resolved-input 上下文公式结果。

### 2.3 `cycles` 在 mempool 中被错误建模

`CellPool` 的 score 模型本来是 CKB 风格的：

```text
effective_size = max(serialized_size, cycles / cycles_per_byte)
fee_density = fee / effective_size
```

但当前传入的 `cycles` 不是 VM 实际 cycles，而是 `cell_tx.compute_mass()`。

这会直接污染：

- fee density
- RBF
- block template 选择
- 对“脚本成本高但字节数低”的交易排序

### 2.4 资源计算职责放错了层

`CellTx` 是协议对象，不应同时承担：

- 数据载体
- 近似资源估算器
- 共识真相源
- RPC 兼容字段来源

CKB 的边界更清楚：

- 交易对象只表示数据
- size 在 verifier / tx-pool 里推导
- cycles 由 VM verifier 返回
- fee rate 由 tx-pool 用统一口径计算

Spora 应保留自身的 `transient_mass` 和 `storage_mass`，但不能继续把这些逻辑散落在 `CellTx` 方法上。

---

## 3. 设计目标

### 3.1 单一真相源

任何一个交易资源值，在任意阶段都应有唯一来源：

- `serialized_size`
- `non_contextual_compute_mass`
- `transient_mass`
- `contextual_storage_mass`
- `verified_cycles`
- `effective_compute_mass`
- `selection_mass`

### 3.2 明确区分三种资源维度

Spora 不应退回“只有 size/cycles”的模型，而应明确保留三类资源：

- `compute`
  - 交易字节、脚本表面、sigop、以及 VM 真正执行成本
- `transient`
  - mempool / relay / block body 的临时占用
- `storage`
  - 持久 live-cell footprint

### 3.3 与 CKB 对齐边界，而不是复制实现

要学习的是 CKB 的职责分层：

- 交易对象不负责资源真相
- verifier 产出真实执行成本
- tx-pool 用统一指标做排序和费率判断

Spora 与 CKB 的差异不在架构层，而在资源维度更多。

### 3.4 支持渐进迁移

该方案必须允许：

- 先统一内部资源管线
- 再逐步收口 RPC / wallet / legacy bridge
- 最后删除 `CellTx` 上的兼容方法

---

## 4. 推荐目标模型

### 4.1 `CellTx` 退化为纯数据对象

`CellTx` 只保留：

- 结构字段
- `id()`
- `version()`
- `is_coinbase()`
- `serialized_size()` 或 canonical size helper
- fee / capacity 相关纯数据 helper

以下方法不再作为共识主路径接口：

- `compute_mass()`
- `transient_mass()`
- `storage_mass()`
- `mass()`

短期可以保留，但必须：

- 改名为 `estimated_*` 或 `compat_*`
- 文档中明确标注“非共识真相源”
- 禁止在 `consensus` / `mining` 主路径继续调用

### 4.2 引入统一资源对象

建议在 `consensus/core` 新增：

```rust
pub struct TxNonContextualResources {
    pub serialized_size: u64,
    pub compute_mass: u64,
    pub transient_mass: u64,
}

pub struct TxContextualResources {
    pub storage_mass: u64,
    pub fee: u64,
}

pub struct TxExecutionResources {
    pub verified_cycles: Option<u64>,
    pub effective_compute_mass: u64,
}

pub struct TxResources {
    pub non_contextual: TxNonContextualResources,
    pub contextual: Option<TxContextualResources>,
    pub execution: TxExecutionResources,
    pub selection_mass: u64,
}
```

其中：

- `non_contextual` 可在未解析 inputs 时计算
- `contextual` 只有在 resolved inputs 存在时才能计算
- `verified_cycles` 只有在 VM 执行后存在
- `selection_mass` 是 mempool / template / RBF 的统一一维比较值

### 4.3 引入统一 builder

建议新增：

```rust
pub struct TxResourceCalculator {
    mass_calculator: MassCalculator,
    vm_limits: VmLimits,
}
```

提供三个入口：

```rust
fn calc_non_contextual(&self, tx: &CellTx) -> TxNonContextualResources

fn calc_contextual(
    &self,
    tx: &dyn VerifiableTransaction,
    fee: u64,
) -> Option<TxContextualResources>

fn finalize(
    &self,
    non_contextual: TxNonContextualResources,
    contextual: Option<TxContextualResources>,
    verified_cycles: Option<u64>,
) -> TxResources
```

统一规则：

```text
effective_compute_mass =
    if verified_cycles exists {
        max(non_contextual.compute_mass, effective_size(serialized_size, verified_cycles))
    } else {
        non_contextual.compute_mass
    }

selection_mass =
    max(
        effective_compute_mass,
        non_contextual.transient_mass,
        contextual.storage_mass if exists else 0
    )
```

这是最关键的收口点。

---

## 5. 资源语义定义

### 5.1 `serialized_size`

唯一来源：统一 size estimator。

必须删除当前“双 estimator”状态：

- `consensus/core/src/mass/mod.rs::cell_tx_estimated_serialized_size`
- `exec/src/celltx/types.rs::serialized_size`

只能保留一个 canonical 版本。

建议保留 `consensus/core` 的 estimator，并让 `CellTx::serialized_size()` 仅委托到它，或者反过来；关键是只能有一份公式。

### 5.2 `non_contextual_compute_mass`

唯一来源：`MassCalculator::calc_non_contextual_masses_cell()`。

这表示：

- 与 inputs resolved 无关
- 与 VM 实际执行结果无关
- 作为 pre-VM compute hint

`CellTx::compute_mass()` 不能再参与主路径决策。

### 5.3 `transient_mass`

唯一来源：统一 `MassCalculator`。

它是：

- relay / mempool / block body 的临时资源约束
- 与 `serialized_size` 强相关
- 不应在别处重新手写

### 5.4 `contextual_storage_mass`

唯一来源：`calc_contextual_masses()`。

语义要求：

- 必须依赖 resolved inputs
- 必须使用 KIP-0009 公式
- 不能由 `output footprint` 替代

如果当下没有 resolved inputs，只能表示为 `None`，不能伪造。

### 5.5 `verified_cycles`

唯一来源：`CellValidator::validate_full_with_scripts_and_cycles()`。

这不是可选优化值，而是实际执行成本。

所有这些地方都应该读它：

- mempool score
- RBF
- block template selector
- RPC verbose
- 未来 fee estimator

### 5.6 `effective_compute_mass`

建议定义为：

```text
max(non_contextual_compute_mass, effective_size(serialized_size, verified_cycles))
```

理由：

- pre-VM hint 仍保留，避免“脚本便宜但结构异常大”的 tx 被低估
- post-VM cycles 可以抬高真实执行成本
- 与 CKB 的 `effective_size` 思想兼容

### 5.7 `selection_mass`

建议作为唯一排序和费率分母：

```text
max(effective_compute_mass, transient_mass, contextual_storage_mass)
```

这使 Spora 的三维资源最终投影成单一排序值，但投影规则只存在一处。

---

## 6. 模块改造方案

### 6.1 Consensus

### 目标

共识层必须成为资源语义的唯一定义者。

### 改造

1. `body_validation_in_isolation`
   - 禁止直接调用 `CellTx::{compute_mass, transient_mass, storage_mass}`
   - 改为 `TxResourceCalculator::calc_non_contextual(...)`
   - isolation 阶段只检查：
     - `compute_mass`
     - `transient_mass`
     - 基础格式合法性

2. `body_validation_in_context`
   - 在 inputs resolved 且 VM cycles 已知后，统一调用 `finalize(...)`
   - block-level 限制按同一组资源累计

3. `virtual_processor`
   - mempool validate 成功后生成完整 `TxResources`
   - feerate threshold 一律用 `selection_mass`
   - 不再使用 `tx.mass()`

### 决策

`storage mass` 不能在 isolation 阶段作为最终真值使用。  
如果必须保留“块头/模板里带 storage commitment”之类语义，也应该校验该 commitment 是否等于 contextual result，而不是直接把 `CellTx::storage_mass()` 当真值。

### 6.2 Mempool / Mining

### 目标

mempool 的排序、RBF、template selection 必须全部基于统一资源对象。

### 改造

1. `MutableTransaction` 增加：

```rust
pub verified_cycles: Option<u64>;
pub resources: Option<TxResources>;
```

2. `populate` / `validate` 阶段：
   - 先填 `non_contextual`
   - resolved inputs 后填 `contextual`
   - VM 后填 `verified_cycles`
   - 最终写入 `resources`

3. `FeerateTransactionKey`
   - 只读 `resources.selection_mass`
   - 不再自行拼 `ContextualMasses::new(tx.mass()).max(...)`

4. `CellPool`
   - `cycles` 字段只存真实 VM cycles
   - score model 的 `effective_size` 使用真实 cycles

5. `block_template::selector`
   - `calculated_mass` 改为 `selection_mass`
   - 不再接受来源不明的 mass

### 决策

`CellPool` 可以继续保留，因为它解决的是 GhostDAG / dependency / score 问题，不是资源语义问题。  
要改的是输入给它的资源字段，而不是强行删除它。

### 6.3 RPC

### 目标

对外暴露的 `mass` 字段必须有确定语义。

### 建议

短期：

- `mass` 返回 `selection_mass`
- verbose 增加：
  - `compute_mass`
  - `transient_mass`
  - `storage_mass`
  - `verified_cycles`
  - `effective_compute_mass`

中期：

- 弃用模糊的 `mass`
- 明确拆字段

### 原因

当前 `mass = storage_mass()` 只会误导调用方，以为这是“交易总体质量”。

### 6.4 Wallet

### 目标

钱包可以继续做 unsigned estimation，但必须明确那是预估，不是共识真值。

### 建议

- wallet 侧保留 `unsigned mass estimator`
- 命名改为 `estimate_*`
- 文档明确：
  - fee estimation 使用估算值
  - 节点接收/排序使用验证后的真实资源值

---

## 7. 与 CKB 的对照

| 维度 | CKB | Spora 推荐方案 |
|---|---|---|
| 交易对象 | 纯数据对象 | 纯数据对象 |
| size 来源 | verifier / tx-pool | `TxResourceCalculator` |
| cycles 来源 | VM verifier | VM verifier |
| tx-pool 排序分母 | size / effective size | `selection_mass` |
| persistent state cost | 无单独 storage mass | 保留 `contextual_storage_mass` |
| 临时占用 | size 隐含表达 | 保留 `transient_mass` |
| 排序投影 | 单维 | 三维投影到 `selection_mass` |

应该学 CKB 的，是“统一推导入口”和“模块边界”，不是把 Spora 的 storage 维度删掉。

---

## 8. 迁移步骤

### Phase 1: 建立单一资源结构

- 新增 `TxResources` / `TxResourceCalculator`
- 所有现有调用点先并行计算新旧值
- 增加一致性日志与断言

产物：

- 新结构体
- 单元测试
- debug assertion

### Phase 2: Consensus 切换

- `body_validation_in_isolation` 改用统一 builder
- `virtual_processor` 改用 `selection_mass`
- 停止在共识主路径调用 `CellTx::compute_mass()/storage_mass()`

产物：

- 共识主路径只剩一套资源口径

### Phase 3: Mempool / CellPool 切换

- 保存 `verified_cycles`
- `CellPool` score 改用真实 cycles
- `FeerateTransactionKey` 改用 `resources.selection_mass`

产物：

- 排序 / RBF / 打包与验块口径一致

### Phase 4: RPC / Wallet 语义收口

- RPC `mass` 改语义或拆字段
- wallet estimation 重命名为 `estimate_*`
- 文档明确“估算值 vs 共识真值”

### Phase 5: 删除历史兼容接口

- 删除或废弃：
  - `CellTx::compute_mass`
  - `CellTx::transient_mass`
  - `CellTx::storage_mass`
  - `CellTx::mass`

如果保留，也必须：

- 改名
- 标成 `compat` / `estimated`
- 禁止主路径使用

---

## 9. 测试要求

### 9.1 一致性测试

同一笔交易在以下路径得到的资源值必须一致：

- mempool pre/post validation
- block template candidate
- block validation
- RPC verbose view

### 9.2 cycles 真实反馈测试

构造：

- 字节小、脚本很重的交易
- 字节大、脚本很轻的交易

验证：

- mempool 排序
- block template 选择
- 最终 block acceptance

三者排序一致。

### 9.3 storage mass 上下文测试

构造：

- outputs footprint 相同
- inputs amount / plurality 不同

验证：

- `output footprint` 相同不代表 `contextual_storage_mass` 相同
- 主路径确实使用 contextual 结果

### 9.4 回归测试

验证以下字段不再被错误复用：

- `cycles != compute_mass`
- `storage_mass != output_footprint_alias`
- `selection_mass` 为唯一 feerate 分母

---

## 10. 不采用的方案

### 10.1 继续修补 `CellTx::{compute_mass, storage_mass}`

不采用。

原因：

- 会继续让协议对象承担资源语义
- 只会把三套公式硬凑成两套
- 后续 RPC / wallet / mining 仍会继续误用

### 10.2 完全照搬 CKB，只保留 size + cycles

不采用。

原因：

- Spora 明确有 storage 维度
- transient 与 persistent 的分离是合理的
- 问题不在资源维度多，而在没有统一 builder

### 10.3 继续允许 `storage mass` 用 output footprint 近似

不采用。

原因：

- 这会让 KIP-0009 实现形同虚设
- resolved inputs 已存在时继续用近似值没有合理性

---

## 11. 最终建议

最优雅、最合理的方案不是“再修一点现有方法”，而是：

1. 把 `CellTx` 还原成纯数据对象  
2. 在 `consensus/core` 建立唯一资源 builder  
3. 用 `verified_cycles` 替代 mempool 中伪造的 `cycles`  
4. 用 `contextual_storage_mass` 替代当前 `tx.mass()` 的伪语义  
5. 用 `selection_mass` 作为唯一排序 / RBF / 打包分母  

如果按这个方向做，Spora 最终会得到：

- 比当前更简单的代码边界
- 比 CKB 更完整的三维资源模型
- 比现在更一致的 mempool / 验块 / RPC 语义

这条路是一次性做对，而不是继续背兼容债。

---

## 12. 推荐落地顺序

如果只做一个最小高收益切口，建议顺序是：

1. 引入 `TxResources`
2. mempool 保存真实 `verified_cycles`
3. `FeerateTransactionKey` 改读 `selection_mass`
4. block validation 改读统一 builder
5. 最后删除 `CellTx::*mass()`

第一步做完，后面所有模块都会自然收敛。
