# GHOSTDAG 在 CBM/LP 攻击模型下的工程缓解与研究路线

- 日期: 2026-04-23
- 威胁来源: Wu et al., "Security-Performance Tradeoff in DAG-based Proof-of-Work Blockchain Protocols", NDSS 2024
- 适用范围: Spora GHOSTDAG 共识层 + 网络传播层
- 状态: **提案** — 待评审与优先级确认

---

## 1. Problem Statement

### 1.1 论文核心论点

NDSS 2024 论文证明：DAG-based PoW 协议的安全性-性能权衡在高吞吐设定下不可忽略。该结论基于两个现象：

**Phenomenon 1 — Block Jam：** 当优先区块在时间 t 内的总大小超过 C·t（C 为网络容量），这些区块无法在假设的短延迟 D 内传播完毕。

**Phenomenon 2 — Late Predecessor：** 一个区块的传播完成不等于可接受——只有当其所有前驱都已被接收并验证后，节点才能接受该区块并基于它继续挖矿。若某前驱晚于后继到达，后继的"实际延迟"（actual delay = propagation delay + processing delay）将超过其自身的传播延迟。

论文进一步指出，现有 DAG 协议的安全性证明依赖 **Decoupling Assumption**：

> 若某类区块足够小且享有优先传播策略，则 (1) 它们可在固定短延迟 D 内传播完毕，且 (2) 所有矿工在收到后即可立即接受并开始挖矿。

Block Jam 否定 (1)；Late Predecessor 否定 (2)。

### 1.2 Late-Predecessor 攻击机制

LP 攻击将网络节点分为 s 个等大不相交子集 G₁, …, Gₛ。对每个潜在 late predecessor B*：

- 立即投递给 B* 所在 local set Gⱼ
- 对其余节点延迟到 δ*_max

攻击者目标：最大化受影响区块的平均 actual delay E[Δ]，从而降低诚实链增长速率与安全阈值。

论文 Theorem 2 给出受影响链的增长率为：

```
g = (1 - σ) · γ · f    其中 γ = α / (1 + α · f · E[Δ])
```

安全阈值 β < γ 随 E[Δ] 增大而下降。

### 1.3 对 Spora GHOSTDAG 的具体影响

当前 Spora GHOSTDAG 的安全参数 K 基于 UDBM 计算：

```
K = f(2 · D · λ, δ)    // D = NETWORK_DELAY_BOUND = 5s, λ = BPS
// 1 BPS → K=18, 10 BPS → K=124
```

若 LP 攻击使 actual delay 从 D 扩展至 Δ_actual = δ + υ，则：

1. **有效 anticone 超出 K 的概率上升**：K 的设计假设区块在 D 内被全网接受，actual delay 增大在工程效果上类似于更高的有效并发出块压力，anticone 更频繁地突破 K 约束
2. **诚实链增长速率下降**：按 Theorem 2，E[Δ] 增大直接降低 γ，压缩安全阈值
3. **Orphan 堆积加剧**：当前 `check_parents_exist` 强制所有 parent 必须已存在，缺失则进入 orphan pool；LP 攻击制造大量 missing-parent 场景，导致 orphan 积压

**但 GHOSTDAG 与 Prism/OHIE 存在结构性差异：**

GHOSTDAG 仅有一种区块类型，所有区块地位平等、传播特性相同。这消除了 Prism 那种"攻击者集中打击 proposer/voter 等优先区块"的单点脆弱性。LP 攻击的效果被分散到所有区块上。

> ⚠️ **重要限定**："单类型区块 → LP 效果更分散"是一个合理的假说，但尚未被严格证明。论文明确指出其分析适用于所有 DAG-based PoW 协议，包括 GHOSTDAG 家族。CBM 下的 GHOSTDAG 安全性退化幅度是一个需要独立证明的开放问题。

---

## 2. Threat Model

### 2.1 威胁模型（三层分离）

#### 分析层：CBM/LP 抽象攻击者

| 能力 | 描述 |
|------|------|
| 控制 β < 50% 算力 | 标准 PoW 攻击者模型 |
| 网络层分区 | 将节点分为 s 组，组间延迟可控制在 δ_max 内任意取值 |
| 延迟诚实区块传播 | 在 δ_max 内任意延迟/重排诚实区块 |
| 不篡改诚实区块内容 | 只能延迟，不能修改 |

该层定义的是论文 CBM 框架下的理论攻击者——其能力边界由模型约束，不保证现实中可完整实现。

#### 现实层：可实现的网络攻击载体

| 攻击载体 | 对应理论能力 | 可行性评估 |
|----------|-------------|----------|
| AS-level 路由劫持 | 网络层分区 | 已有现实先例 [Apostolaki et al., 2017]；LP 攻击不需要完全断开，只需增大组间延迟 |
| Sybil 节点占用连接槽 | 延迟诚实区块传播 | 可行；通过部署大量不转发区块的节点占满诚实节点的 peer 槽位 |
| 矿池地理分布利用 | 网络层分区（自然分区） | 被动利用现有矿池分布即可造成区域性延迟差异，无需主动攻击 |

该层回答的问题是：CBM 攻击者的哪些能力在今天的互联网中可以被合理地实现？

#### 实现层：当前 Spora 诚实节点行为假设

| 假设 | 含义 | 若假设不成立的影响 |
|------|------|-------------------|
| 诚实节点在收到完整区块（header+body）后才接受 | 当前 `check_parents_exist` 要求所有 parent 有完整状态 | 正是 LP 攻击卡住的断点 |
| 诚实节点不区分前驱的传播延迟 | GHOSTDAG 着色不依赖延迟信息 | 使延迟攻击成为零成本操作 |
| 诚实节点按到达顺序中继区块 | 无优先级感知的 relay | 无法对抗性地加速被延迟的前驱 |

该层回答的问题是：Spora 当前实现中有哪些行为模式被 LP 攻击所利用？

### 2.2 攻击目标

1. **Liveness 破坏**：降低 γ 使 β > γ，攻击者可控制 leader sequence / 诚实链，审查交易
2. **Consistency 破坏**（OHIE 场景）：重排已确认交易，但 GHOSTDAG 的 K-cluster 约束对这类攻击提供额外阻力
3. **Latency 恶化**：即使未达到 liveness 破坏阈值，确认延迟也会显著增长

### 2.3 Spora 当前实现的脆弱点

```
区块到达节点
  │
  ├─ check_parents_exist() ──── 所有 parent 必须已在 statuses_store 中
  │   │                         若缺失 → RuleError::MissingParents → 进入 orphan pool
  │   └─ ❌ 此处是 LP 攻击的核心卡点：
  │        一个 parent 的 body 延迟到达 → 整个后继区块无法开始处理
  │
  ├─ validate_header_in_isolation() ──── 仅检查 header 本身（PoW、版本、时间戳）
  │
  ├─ ghostdag() ──── 计算 GHOSTDAG 数据，仅依赖 parent hashes
  │   │                ← 仅需 headers 即可完成
  │   └─ ✅ 此步不依赖 body
  │
  ├─ pre_pow_validation() / post_pow_validation()
  │
  └─ validate_body() ──── 检查交易、Cell 状态等
      └─ ❌ 此步依赖 parent bodies（check parent bodies exist）
```

**关键观察**：GHOSTDAG 计算本身仅需 parent hashes（来自 header），不依赖 body。但当前 `validate_parent_relations` 要求所有 parent 已经被完整处理（含 body），这使得 consensus-critical path 被非关键依赖卡住。

---

## 3. Immediate Mitigations（无共识变更）

> **原则：不改 GHOSTDAG 的数学，先让 GHOSTDAG 所依赖的"可接受性路径"做瘦、做快、做可观测。**

### 3.1 Header-First Propagation

**优先级：P0 — 最对症的缓解措施**

**原理**：将区块传播拆为 header（~数百字节）和 body（含交易，~KB-MB 级）两阶段。节点收到 header 后即可执行 GHOSTDAG 计算并开始基于该区块挖矿，body 验证异步进行。

**为什么命中 LP 攻击要害**：

论文的 late predecessor 定义是：前驱尚未到达 → 后继无法接受。关键在于"到达"的语义——如果"到达"只需要 header 到达而非完整 body 到达，则 LP 攻击的效果被大幅削弱，因为：

- Header 体积极小（~200-500 字节），即使网络拥塞也能快速传播
- LP 攻击者需要延迟的是 header 级别的 parent，而非 body 级别的 parent
- 这直接缩短了 acceptance path 的关键依赖链

**⚠️ 关键限定**：

Header-first **只能缓解，不会自动消灭问题**。原因：

1. 若某区块的 **header-level parents** 本身缺失（即前驱的 header 也未到），则 LP 攻击仍然有效——只是攻击面从 body 层转移到了 header 层
2. 若 GHOSTDAG 计算需要完整的 header-DAG frontier（不仅是 direct parents，还包括 mergeset 中的间接前驱），而该 frontier 本身到达不同步，则 attack surface 依然存在
3. 因此，Header-first 的有效性严格依赖于：**Spora 的挖矿视图和 GHOSTDAG 状态转换能否仅由 header 闭包驱动**

**实施要点**：

```
现有流程：
  InvRelayBlock → RequestBlock(完整区块) → validate_header + validate_body → 接受

改进流程：
  InvRelayHeader → RequestHeader → validate_header + ghostdag → header_accepted
  InvRelayBlock → RequestBlock(body only) → validate_body → fully_accepted

挖矿条件变更：
  现有：parent 必须 StatusHeaderOnly 或更高
  改进：parent 只需 StatusHeaderAccepted（新增状态，表示 header 已验证、GHOSTDAG 已计算）
```

**代码改动范围**：

| 组件 | 改动 |
|------|------|
| `BlockStatus` 枚举 | 新增 `StatusHeaderAccepted`，介于 `StatusHeaderOnly` 和 有 body 之间 |
| `check_parents_exist()` | 允许 parent 处于 `StatusHeaderAccepted` 状态 |
| P2P 消息类型 | 新增 `InvRelayHeader`、`RequestHeader`、`Header` 消息 |
| `HeaderProcessor` | 分离 header 接受逻辑，不再要求 parent 有 body |
| `BodyProcessor` | body 验证在 header 接受后异步进行 |
| 挖矿模板生成 | 允许基于 `StatusHeaderAccepted` 的 parent 生成模板 |

**⚠️ 必须显式回答的设计约束**：

在 header-only 基础上挖矿引入一个核心问题——若 parent 的 body 最终验证失败（例如包含无效交易），则基于该 header 挖出的后继区块的 GHOSTDAG 数据仍然有效（因为 GHOSTDAG 仅依赖 header 中的 parent hashes），但其 body 层面的状态转移需要回滚。系统必须满足以下约束之一：

> **要么** header-accepted 上的挖矿在语义上始终安全（即 body 失败不会导致 GHOSTDAG 结构层面的回滚，只需丢弃 body 层状态），**要么** 系统必须定义清晰的 unwind / recovery 规则（例如 body 失败时如何处理已基于该 header 挖出的后继区块）。

该约束是 header-first 方案能否进入生产的前提条件，须在设计阶段显式回答。

### 3.2 Dependency-Aware Relay Priority

**优先级：P0 — 低风险高回报的网络层优化**

**原理**：在区块中继时，优先传播那些当前阻塞最多未接受区块的缺失依赖，而非简单按到达顺序或引用计数传播。

**为什么对 LP 攻击有效**：

LP 攻击通过延迟特定前驱来阻塞大量后继区块的接受。如果网络层优先传播"解除阻塞最多区块的依赖"，则攻击者的延迟效果被最大化消减。这是论文自身建议的"设计者应关注网络层"的直接实现。

**与 popularity-aware 的区别**：

```
popularity-aware:  priority(block) = count(children referencing block)
                     → 图中心性度量，反映区块在 DAG 中的结构重要性

dependency-aware:  priority(block) = count(orphaned/pending blocks
                                           whose acceptance is blocked by this block)
                     → 接受关键性度量，反映区块对当前共识进展的实际阻塞程度
```

Popularity 是静态图属性；dependency 是动态运行时状态。真正应该优先抢救的是那些造成 orphan backlog 的缺失 parent / selected-parent / merge frontier headers。

**优先级评分的数据源定义**：

dependency criticality score 的计算口径必须明确。当前版本以 **orphan pool + pending-header dependency graph** 为主，不将 pending-body backlog 直接计入 consensus-critical priority。理由：header 层依赖是 GHOSTDAG 共识进展的直接瓶颈（参见 Section 2.3 的处理流水线），而 body 层依赖只影响状态确认，不影响链增长。

```
priority(block) = α · count_orphans_waiting_for(block)
               + β · count_pending_headers_waiting_for(block)
               + γ · 0  // pending-body 不计入
// 其中 α > β > 0，γ = 0
// 权重设计：orphan 比 pending-header 更紧迫，因为 orphan 已占用完整区块资源
```

**实施要点**：

```
中继决策逻辑（伪代码）：

on_receive_inv(block_hash):
  if block is already known: skip
  block = request_block(block_hash)
  
  // 计算该区块的 dependency criticality
  score = compute_dependency_score(block_hash)
  // 数据源：orphan pool + pending header queue，不含 pending-body
  
  if score > CRITICAL_THRESHOLD:
    relay_with_high_priority(block)
    // 立即广播，使用紧凑编码，跳过常规排队
  else:
    relay_with_normal_priority(block)
```

**代码改动范围**：

| 组件 | 改动 |
|------|------|
| `HandleRelayInvsFlow` | 在广播前计算 dependency criticality |
| `OrphanPool` | 新增接口：按 missing-parent 统计阻塞区块数 |
| `Hub` 广播接口 | 新增 priority-aware broadcast 方法 |

**风险**：低。纯网络层行为变更，不影响共识规则。不同节点可能因本地视图不同而有不同的优先级判断，但这只影响传播效率，不影响安全性。

### 3.3 Missing-Parent / Orphan / Header-Body Skew Telemetry

**优先级：P0 — 必要的可观测性基础设施**

**原理**：在实施任何缓解措施之前，必须能够观测到 LP 攻击的征兆。这包括：

1. **Orphan age 分布**：orphan 在 pool 中等待多久才被 unorphan
2. **Missing-parent queue 深度**：当前有多少区块因 missing parents 而阻塞
3. **Header-body skew**：header 已接受但 body 未到的区块数及其等待时间
4. **按 peer/region 分层的 propagation asymmetry**：从不同 peer 收到区块的延迟分布是否存在双模态特征（LP 攻击的签名）

**为什么关键**：

- 没有这些数据，无法区分"正常网络抖动"和"LP 攻击"
- 这些指标是后续所有防御措施的输入信号
- 论文强调 LP 攻击会导致节点间的 actual delay 异质性，这种异质性可通过 telemetry 捕获

**核心运营 SLO（一等公民指标）**：

以下两个指标直接度量 acceptance path 的健康程度，应作为 dashboard 上的旗舰数字：

1. **Dependency Completion Latency**：从区块被创建到其所有 consensus-critical 前驱被本节点接受的时间。这比单纯 propagation delay 更贴近 acceptance path 的真实瓶颈。
2. **Header-Accepted Latency**：从区块被创建到其 header 被本节点接受（GHOSTDAG 计算完成）的时间。在 header-first 方案落地后，这是挖矿进展的直接信号。

**辅助 metrics**：

```rust
// 一等公民 SLO
METRIC_DEPENDENCY_COMPLETION_LATENCY  // histogram: 前驱闭包完成时间（核心 SLO）
METRIC_HEADER_ACCEPTED_LATENCY        // histogram: header 接受延迟（核心 SLO）

// 辅助指标
METRIC_ORPHAN_POOL_SIZE               // gauge: 当前 orphan 数
METRIC_ORPHAN_AGE_P50/P99             // histogram: orphan 等待时间
METRIC_MISSING_PARENT_QUEUE_DEPTH      // gauge: 等待 parent 的区块数
METRIC_HEADER_BODY_SKEW               // gauge: header 已接受但 body 未到的区块数
METRIC_PEER_PROPAGATION_DELAY         // histogram: 按 peer 分层的区块传播延迟
METRIC_LATE_PREDECESSOR_RATE          // counter: 检测到 late predecessor 的频率
```

**代码改动范围**：

| 组件 | 改动 |
|------|------|
| `HandleRelayInvsFlow` | 在处理 inv 时记录 propagation delay |
| `OrphanPool` | 记录 orphan age 和 missing-parent 关系 |
| `HeaderProcessor` | 记录 header-body 时间差 |
| `MetricsRegistry` | 注册新指标 |

**风险**：极低。纯观测性改动。

### 3.4 Adaptive Safety Margins

**优先级：P1 — 参数加固，非共识变更**

**原理**：保持 K 为共识常量不变，但基于观测延迟调整本地 relay / mining / pruning policy 的保守程度。

**与 "动态 K" 的关键区别**：

```
动态 K（高风险）：
  每个节点根据本地观测独立调整 K → 不同节点 K 不同 → 共识分裂风险

Adaptive Safety Margins（低风险）：
  K 保持共识常量 → 所有节点对 K 达成一致
  但本地 policy 可以更保守：
  - relay: 更积极地传播 dependency-critical 区块
  - mining: 在观测到高延迟时，选择更保守的 parent 集合（见下方 heuristic）
  - pruning: 增大 merge_depth 安全余量
  - orphan: 扩大 orphan pool 容量
```

**实施要点**：

- 观测近期区块的 actual delay 分布（来自 3.3 的 telemetry）
- 当 P99 actual delay 显著超过 D 时，激活 stress mode：
  - 扩大 orphan pool 容量上限
  - 增大 timestamp_deviation_tolerance
  - **Conservative parent selection heuristic**：
    > 在 stress mode 下，mining template 优先选择 **blue score 高且 unresolved dependency 少的 tips**；限制极新 tip 的纳入比例（例如，仅允许不超过 20% 的 direct parents 来自最近 D/2 秒内产出的区块）。
    >
    > 这降低了新产出但尚未充分传播的区块被选为 parent 的概率，从而在 anticone 压力较大时减少 fork 风险。
- 这些都是本地 policy，不同节点可以不同步，不影响共识一致性

**风险**：低。不改共识参数，只调本地行为。

---

## 4. Research Agenda（共识层变更方向）

> **原则：共识层变更必须在 CBM-GHOSTDAG 安全证明完成之后才能推进。未证明安全的共识变更不应进入主网。**
>
> ⚠️ 工程缓解措施（Section 3）可以先于完整证明实施；但任何共识层改动（Section 4.3–4.4）必须等待证明。

### 4.1 CBM-GHOSTDAG 安全证明

**优先级：P0（研究项）— 所有共识层变更的理论前提**

**目标**：在 CBM 模型下重新证明 GHOSTDAG 的 chain growth / chain quality / common prefix 性质。

**现有差距**：

当前 GHOSTDAG 的安全性分析基于 UDBM（uniform delay blockchain model），假设：
- 所有区块在 Δ 轮内传播到所有节点
- 收到 = 接受（无 processing delay）
- 区块传播相互独立

论文证明这三个假设在高吞吐 DAG 协议中均不成立（Lemma 2: globally-uniform actual delay 概率仅 1/n）。

**需要证明的关键命题**：

1. **GHOSTDAG 在 CBM 下的 chain growth rate**：
   - 推广 Theorem 2 到 GHOSTDAG 的 blue-score 增长
   - 量化 E[Δ_actual] 对 blue-score 增长率的影响

2. **GHOSTDAG 在 CBM 下的安全阈值**：
   - 确定 β < γ(CBM) 的具体表达式
   - 与 UDBM 下的阈值对比，量化退化幅度

3. **单类型区块的分散效应**（假说需证明或证伪）：
   - GHOSTDAG 的单类型区块设计是否使 LP 攻击效果更分散？
   - 与 Prism（有优先区块可被集中攻击）相比，GHOSTDAG 的安全退化是否更温和？
   - 这个判断目前是假说，不是定理

**方法论**：

- 采用论文的 CBM 框架，将 LP 攻击策略应用于 GHOSTDAG
- 所有区块均为潜在 late predecessor 和潜在受影响区块
- 利用 Theorem 1 计算 E[Δ]，代入推广的 chain growth / quality / prefix 定理
- 用 SimBlock 仿真验证理论预测

### 4.2 Congestion-Aware Mining Policy

**优先级：P1（研究项）— Soft consensus / policy layer**

**原理**：当网络观测到拥塞（actual delay 上升）时，节点自主调整挖矿行为：

- **Backoff**：在高延迟期间降低出块频率（通过 DAA 自然实现）
- **Parent selection tightening**：在 mining template 中倾向于选择 blue score 更高、传播更广的 parent，减少 anticone 风险
- **Merge depth safety margin**：在拥塞时增大 merge depth 的安全余量

**与 NC-Max 的关系**：

论文在讨论中提到 NC-Max 的策略——在交易同步和确认之间强制间隔。这个思路可以借鉴到 GHOSTDAG 中：当检测到高延迟时，不改变共识规则，但在挖矿模板中注入额外的保守性。

**风险**：中。这是 policy 层面的变更，不需要硬分叉，但需要确保所有诚实节点在拥塞期间的行为仍然一致。如果某些节点 backoff 而其他节点不 backoff，可能反而增加 fork 概率。

**前提**：需要 3.3 的 telemetry 数据作为拥塞检测的输入。

### 4.3 Stronger Missing-Parent Admission Rules

**优先级：P1（研究项）— Soft consensus / policy layer**

**原理**：当前 `check_parents_exist` 是二值判断——所有 parent 存在则通过，任一缺失则拒绝。可以考虑更精细的 admission 策略：

- **Parent age threshold**：拒绝接受 parent 中包含"过新"区块的区块（这些区块可能尚未充分传播）
- **Selected-parent propagation check**：在选择 selected parent 时，优先选择传播确认度更高的 parent（例如，已被更多 peer 确认收到的 parent）
- **Header-only parent admission**：允许 header 已接受的 parent 参与 GHOSTDAG 计算，但限制其在 blue weighting 中的贡献（直到 body 也被验证）

**风险**：中。改变了区块接受的前提条件，但仍在 policy 层面而非共识规则层面。不同节点的本地视图不同可能导致短期分叉，但 GHOSTDAG 的 DAG 结构天然容忍这种差异。

**前提**：需要 3.1（Header-First）和 4.1（CBM 安全证明）的结果。

### 4.4 共识层重设计：phasmDAG（Bi-objective Closure-Aware DAG Consensus）

**优先级：P2（研究项）— Hard consensus redesign，长期方向**

**参考文档**：[phasmDAG.md](phasmDAG.md)

本节原包含两个独立思路（Delay-Weighted Coloring 与 Closure-Gated Confirmation）。经评审，phasmDAG 方案在形式化程度和概念清洁度上均优于原方案，正式替代之。下文先给出 phasmDAG 的核心要点，再说明替代理由与待补强项。

#### 4.4.1 phasmDAG 核心要点

phasmDAG 的根本诊断是：**GhostDAG 的数学对象试图让一个结构排序同时承担 Order / Acceptability / Finality 三种语义**，这在 CBM/LP 攻击下不成立。修复方式不是工程层补丁，而是将三种语义显式拆开：

| 语义层 | 回答的问题 | GhostDAG 中的对应 |
|--------|-----------|-------------------|
| **Layer A — Order** | 这个块在结构上站在哪里？ | blue score / K-cluster 着色 |
| **Layer B — Closure** | 这个块的依赖世界是否已经成熟？ | 隐含在 orphan pool / missing-parent 逻辑中，非协议状态 |
| **Layer C — Finality** | 结合前两者，它什么时候能被经济上信任？ | 直接由 blue score + finality depth 推导 |

phasmDAG 引入双目标共识函数：

```
Φ(S) = U_order(S) - λ · R_closure(S)
```

- `U_order(S)` 奖励结构上有价值的并发与一致排序
- `R_closure(S)` 惩罚依赖不完整、前驱脆弱、闭包未成熟的支撑结构
- `λ` 控制协议偏乐观还是偏保守

最终性不再是 blue score 的直接推论，而是 Order + Closure 的联合函数：

```
Finality(B) = F(OrderStrength(B), ClosureLevel(B), Depth(B))
```

协议暴露三级确认语义：**Tentative Ordering → Strong Confirmation → Deep Settlement**。

#### 4.4.2 替代原 4.4 的理由

| 对比维度 | 原 4.4a Delay-Weighted Coloring | phasmDAG `R_closure` |
|---------|-------------------------------|---------------------|
| 惩罚对象 | 观测到的传播延迟 | 依赖脆弱性与闭包不完整性 |
| 拓扑偏差风险 | 高：不同节点观测到不同 delay → 共识权重被网络地理结构污染 | 低：依赖完整性是结构性属性，不直接依赖时序观测 |
| 概念清洁度 | 把时序信号直接注入共识权重 | 把结构稳健性注入共识函数 |

| 对比维度 | 原 4.4b Closure-Gated Confirmation | phasmDAG Layer B + Layer C |
|---------|--------------------------------|------------------------|
| Closure 语义 | 二值 gate（闭包完成/未完成） | 连续成熟度 `C(B) ∈ [0,1]` |
| 最终性模型 | 强最终性更慢但更稳（定性描述） | 三级确认语义 + Order/Closure 联合函数（形式化） |
| 协议状态 | Closure 不是协议状态 | Closure 是协议一等状态 |

**结论**：phasmDAG 在惩罚语义、拓扑偏差风险、概念清洁度、最终性形式化四个维度均优于原方案，正式替代之。

#### 4.4.3 phasmDAG 待补强项

phasmDAG 目前是 research proposal 级别，从提案到可实施协议需补强以下关键缺口：

1. **`C(B)` 闭包成熟度的精确语义**（最核心缺口）：
   - "依赖完整性""前驱稳定性""frontier 稳定窗口"在直觉上清晰，但要成为协议一等状态，必须给出确定性计算规则
   - 核心难题：闭包涉及的是"未到达的东西"——如何对"你没看到的东西"达成共识？
   - 可能路径：(a) 基于后继区块的 attestation 统计（确定性）；(b) 基于传播延迟的概率后验（更表达力但更难证明安全）

2. **λ 的共识一致性问题**：
   - 如果 λ 是协议参数，所有节点必须用同一个 λ
   - 不同的 λ 在不同网络条件下给出不同最优行为
   - 需要决定：λ 是静态协议常量，还是 epoch 级共识内生更新参数？

3. **"工程缓解不够"的前提需要证明**：
   - phasmDAG Section 2.2 断言 "engineering mitigations are not enough"，但未给出证明
   - CBM-GHOSTDAG 安全证明（4.1）的结果将决定该断言是否成立
   - 如果工程修复在目标 BPS 下安全阈值足够高，phasmDAG 的紧迫性下降

4. **三级确认语义的客户端适配成本**：
   - 从"一个 confirmation number"变成 "tentative / strong / settlement" 三级
   - 所有钱包、浏览器、交易所都需适配，工程成本不可低估

**推进条件**：

- 必须在 4.1（CBM-GHOSTDAG 安全证明）完成后
- 必须有充分的仿真证据表明 Tier 1-2 缓解措施不足以应对高吞吐场景
- `C(B)` 的精确计算语义必须确定
- 需要独立的安全性证明，证明新的共识规则在 CBM 下的安全性不低于原始 GHOSTDAG 在 UDBM 下的安全性

> ⚠️ **定位说明**：phasmDAG 应被视为**独立协议族探索**（如 ClosureDAG），而非生产 GHOSTDAG 的增量修改。它与 GhostDAG 是 medium-far 到 far 的协议分化，如果实施，值得一个独立的协议身份而非 GhostDAG+ 的后缀。

---

## 5. 影响评估与优先级决策

### 5.1 核心判断

工程缓解层大概率会让系统**更接近它本来该有的 TPS 和最终性**，而不是神奇地创造更高天花板。它们主要是把被 late predecessor 和 dependency skew 偷走的性能拿回来；真正会明显压低 TPS/最终性的，反而是那些更保守的 policy 和更重的共识改造。

### 5.2 必要性分类决策表

#### ✅ 必做（几乎纯正面，正常工况下对 TPS/最终性伤害极小）

| 方案 | TPS 影响 | 最终性影响 | 必要性理由 |
|------|---------|-----------|----------|
| **Telemetry** (3.3) | 几乎无直接运行时影响 | 无直接协议改善，但巨大间接价值 | 没有它，你不知道系统是被攻击、拥塞、还是参数不对；是所有后续动作的眼睛 |
| **Dependency-Aware Relay** (3.2) | 小到中等正面：减少 missing-parent 造成的吞吐空转 | 正面：尤其改善尾部延迟(P95/P99)，比单纯拉高平均值更有意义 | 低风险、低成本、直接命中 LP 攻击要害 |
| **Header-First** (3.1) | 中性偏正面：不是凭空增加带宽，而是把被 body 卡顿白白浪费的吞吐拿回来 | 快速确认性改善；**前提**：header-only 挖矿安全不变量必须满足 | 最有杠杆效应的缓解措施；如果不做，高 BPS 目标下 LP 攻击的卡点无解 |

> 一句话：**看得见、传得对、让 header 先跑起来。**

#### ⚠️ 条件性执行（高压/攻击场景下拿性能换安全）

| 方案 | TPS 影响 | 最终性影响 | 执行条件 |
|------|---------|-----------|----------|
| **Adaptive Safety Margins** (3.4) | 压力时略负面：保守 parent 选择会牺牲一点吞吐；平时中性 | 中位数略慢，但灾难性退化风险下降 | 如果 telemetry 显示 Spora 在目标 BPS 下已有舒适余量，可保持轻量；如果逼近极限则变得重要 |
| **Congestion-Aware Mining** (4.2) | 拥塞时负面（主动刹车） | 短期更慢，长期更稳 | 只在有 telemetry 信号且 header-first 已落地后激活；否则是在信号质量不足时盲目加保守 |
| **Stronger Missing-Parent Admission** (4.3) | 过度防守时负面：更多区块被暂时搁置 | 安全性可改善，但延迟可能恶化 | 需精调的 policy lever，不是天然正确答案；应在看到实际失败模式后再收紧 |

> 本质：这些是**保守阀门**，不是无条件全上。它们在系统健康时没什么用，在系统承压时能防止从"还能跑"直接掉进"失真/积压/分叉恶化"。

#### 🔬 研究项（不当作当前主网必做项）

| 方案 | TPS 影响 | 最终性影响 | 定位 |
|------|---------|-----------|------|
| **CBM-GHOSTDAG 证明** (4.1) | 无直接运行时影响 | 无直接运行时影响 | 理论完整性：让你终于知道"现在的 TPS/最终性宣传值，哪些是安全包络内的，哪些只是理想模型里的数字"；是共识层改动的前置门控 |
| **phasmDAG** (4.4) | 峰值 TPS 可能略降，实际 TPS（高压下）可能改善 | 快确认接近现有；强最终性更慢但更干净；高压尾部最终性明显更稳 | 独立协议族探索（ClosureDAG），替代原 Delay-Weighted Coloring + Closure-Gated Confirmation |

### 5.3 仅实施必做项的预期效果

如果只实施必做三项（Telemetry + Dependency-Aware Relay + Header-First）：

- **TPS**：在非理想网络条件下，实际可实现的 TPS 有小幅到中等改善——主要来自减少被 late predecessor 和 dependency skew 偷走的空转吞吐
- **快速确认/临时最终性**：尾部延迟(P95/P99)明显改善，节点更快收敛到可用 DAG 状态
- **强最终性**：不变——除非你刻意引入 closure-gated 语义（此时更慢但更稳健）

### 5.4 Header-First 的 TPS/最终性细节

这是必做项中影响最大的一项，值得展开说明：

**TPS 方面**：
- Header acceptance 与 body completion 解耦后，consensus-critical path 变轻，减少了 body 到达慢导致的人为流水线停顿和 orphan 堆积
- 在相同网络条件下，有效 TPS 上限可能改善，尤其当瓶颈是 dependency closure 而非原始带宽时
- 但它不凭空创造带宽：如果 body 仍然很大，block jam 在数据面仍然存在

**最终性方面**：
- 如果 GHOSTDAG 进度能安全运行在 header 闭包上，header 级排序/最终性信心应该更快、偏斜更少
- **但**：如果后续 body 失败会使矿工已经基于该 header 构建的内容无效，则会出现脏的 unwind 语义
- 所以最终性改善有一个硬前提：**header-accepted 挖矿必须语义安全，或回滚语义必须显式定义**（参见 Section 3.1 的设计约束）

---

## 附录 A：GHOSTDAG 天然优势与未被证明的假说

| 特性 | 是否为 GHOSTDAG 提供额外保护 | 证明状态 |
|------|---------------------------|----------|
| 单一区块类型，无优先区块可被集中攻击 | ✅ 消除了 Prism 式的单点脆弱性 | 假说（需 CBM 证明确认） |
| K-cluster 约束限制单区块影响范围 | ✅ 被 LP 攻击延迟的区块对 blue score 的影响受 K 约束 | 假说（需形式化） |
| Merge depth / finality depth 限制深回滚 | ✅ 作为爆炸半径控制机制 | 已有（但定位为二级保险，非 LP 检测器） |
| Orphan pool 机制缓解 missing-parent 场景 | ✅ 实践中缓解 late predecessor | 工程事实，非安全证明 |

**关键未证明命题**：

> GHOSTDAG 的单一区块类型设计使得 LP 攻击效果更分散，因此其安全退化比 Prism/OHIE 更温和。

这是一个合理的直觉，但论文明确指出其分析适用于所有 DAG-based PoW 协议。单类型区块意味着攻击者也不必针对特定类型——每个区块都同时是结构性和事务性的。分散效应是否存在、幅度多大，需要独立证明。

## 附录 B：Merge Depth 的正确定位

Merge depth 是 **爆炸半径控制机制**，不是 LP 攻击检测器。

- ✅ 正确用途：当前驱缺失导致局部视图分化时，merge-depth / finality-depth 限制深回滚的爆炸半径
- ❌ 错误期望：LP 攻击的签名未必干净地表现为 merge-depth violation

LP 攻击的真正检测信号应来自：

| 信号来源 | 检测内容 |
|----------|----------|
| Orphan age 分布 | 是否存在异常长时间未被 unorphan 的区块 |
| Missing-parent queue | 是否有特定区块被大量后继引用但自身迟迟未到 |
| Header-body skew | header 已到但 body 延迟到达的比例是否异常升高 |
| 按 peer 分层的 propagation asymmetry | 不同 peer 的传播延迟是否呈现双模态分布（LP 攻击签名） |

## 附录 C：实施路线图

```
Phase 1A — 可观测性 + 网络层缓解（0-2 个月）
├─ 3.3 Missing-Parent / Orphan / Header-Body Skew Telemetry
│   └─ 包含两个核心 SLO：dependency completion latency、header-accepted latency
├─ 3.2 Dependency-Aware Relay Priority
│   └─ 数据源：orphan pool + pending-header dependency graph（不含 pending-body）
└─ 3.4 Adaptive Safety Margins
    └─ Conservative parent selection heuristic

Phase 1B — Header-First 原型验证（2-4 个月）
├─ 3.1 Header-First Propagation 设计验证
│   ├─ P2P 消息类型原型
│   ├─ BlockStatus 状态机原型
│   └─ ⚠️ 必须回答：body failure 时的 unwind/recovery 规则
└─ 仿真环境验证 header-first 对 LP 攻击的实际缓解效果

Phase 2 — Header-First 正式落地（4-8 个月）
├─ 3.1 Header-First Propagation 生产化
│   ├─ P2P 消息类型扩展
│   ├─ BlockStatus 状态机扩展
│   └─ 挖矿模板生成适配
├─ 4.1 CBM-GHOSTDAG 安全证明（与 Phase 1B 并行启动，持续至 Phase 2）
└─ 仿真：验证 Tier 1 缓解措施对安全阈值的实际改善

Phase 3 — Policy 层加固（8-14 个月）
├─ 4.2 Congestion-Aware Mining Policy
└─ 4.3 Stronger Missing-Parent Admission Rules
    └─ 前提：3.1（Header-First）和 4.1（CBM 安全证明）的结果

Phase 4 — 共识层重设计（14+ 个月，视 CBM 证明结果决定）
├─ 4.4 phasmDAG（Bi-objective Closure-Aware DAG Consensus）
│   ├─ 替代原 Delay-Weighted Coloring + Closure-Gated Confirmation
│   ├─ 关键前置：C(B) 闭包成熟度精确语义的确定
│   └─ 定位为独立协议族探索（ClosureDAG），非 GhostDAG 增量修改
└─ 仅当 Phase 2-3 不足以应对高吞吐场景时推进

phasmDAG 与工程缓解的衔接关系：
├─ Phase 1-2 是 phasmDAG 的前置基础设施
│   ├─ Telemetry 为 C(B) 提供观测基础（dependency completion latency）
│   └─ Header-First 使 closure 层可在 header 级独立计算
├─ CBM-GHOSTDAG 安全证明是分叉点
│   ├─ 安全阈值可接受 → 维持 GhostDAG + 工程加固
│   └─ 安全阈值不足 → 启动 phasmDAG 研究路线
└─ phasmDAG Phase 2（仿真）应与工程方案 Phase 2 并行
    └─ 用同一套 SimBlock 环境对比 GHOSTDAG+工程缓解 vs phasmDAG 在 CBM/LP 下的表现
```

**决策门控**：

1. Phase 1B → Phase 2 的门控：header-first 的 body failure unwind/recovery 规则是否已显式回答
2. Phase 2 → Phase 3 的门控：CBM-GHOSTDAG 安全证明是否完成
3. Phase 3 → Phase 4 的门控：Tier 1-2 缓解措施的仿真安全阈值是否低于可接受范围（例如 β < 0.4 在 90% 带宽利用率下）。如果足够安全，则无需进入 Phase 4
4. Phase 4 启动的附加前提：phasmDAG 的 `C(B)` 闭包成熟度精确语义是否已确定
