# Bi-objective Closure-Aware DAG Consensus

## A Root-Level Protocol Direction Beyond GhostDAG

* Status: Research Proposal / Protocol Sketch
* Audience: Consensus researchers, protocol architects, systems engineers
* Intended role: Concept note for a next-generation DAG consensus family

---

## 1. Executive Summary

This document proposes a **root-level consensus redesign** motivated by the challenge raised in *Security-Performance Tradeoff in DAG-based Proof-of-Work Blockchain Protocols* (NDSS 2024): in high-throughput DAG systems, **structural ordering**, **practical acceptability**, and **economic finality** cannot be safely treated as if they collapse into a single mathematical object once block jam and late predecessor effects appear.

本文提出一种**根本性协议方向**，而不是工程层补丁：将 DAG 共识从“单结构对象”升级为**双目标、分层语义**系统。核心思想是把当前被混合在一起的三种语义显式拆开：

1. **Order**：区块在 DAG 中的结构顺序与主干关系。
2. **Closure**：区块依赖世界是否已经成熟到足以让不同诚实节点在实质上推理同一个对象。
3. **Finality**：在结构支持与闭包成熟度共同作用下，一个区块何时才经济上足够安全。

Instead of asking one GhostDAG-like structural rule to approximate all three at once, this proposal introduces a **bi-objective consensus functional**:

[
\max_{S} \Big(U_{order}(S) - \lambda R_{closure}(S)\Big)
]

where:

* (U_{order}(S)) rewards structurally useful concurrency and coherent order,
* (R_{closure}(S)) penalizes dependency fragility, closure incompleteness, and unstable visibility.

中文概括就是：**不是所有结构上合法的并发块集，都同样值得进入强共识。**

This proposal therefore does **not** aim to maximize raw optimistic TPS at all costs. Instead, it aims to improve:

* realized TPS under imperfect networks,
* graceful degradation under congestion and skew,
* semantic cleanliness of finality,
* and the honesty of the protocol’s security model.

---

## 2. Motivation

### 2.1 The root limitation of GhostDAG-style thinking

GhostDAG is powerful because it captures concurrency structurally. However, its core mathematical object is still primarily a **structural classifier/order extractor**. In practice, production systems then ask this object to do too much:

* represent canonical order,
* approximate safe mining view,
* and imply finality-like confidence.

This is acceptable only when network delay behaves close to optimistic assumptions.

GhostDAG 类方案的根本局限在于：它擅长处理**结构并发**，但并没有把**闭包成熟度**作为一等对象建模。于是协议会不自觉地把以下三件事混为一谈：

* 结构上已经被纳入，
* 工程上已经可被安全接受，
* 经济上已经可以视为最终。

论文指出，这种塌缩在高吞吐、传播重叠、late predecessor 条件下并不成立。

### 2.2 Why engineering mitigations are not enough

Header-first relay, dependency-aware relay, telemetry, and safe-mode policies are valuable. But they still attempt to protect a protocol whose **core mathematical object remains structurally blind to closure**.

如果目标只是把系统做得更稳，这些工程优化已经很有价值；但如果目标是**根本解决**论文提出的挑战，那就必须修改协议的数学核心，而不是只修传播管线。

### 2.3 Design goal

The design goal is therefore:

> **Separate structural order, closure maturity, and finality mathematically, then reconnect them through an explicit higher-level consensus functional.**

---

## 3. High-Level Protocol Philosophy

### 3.1 Three semantic layers

The protocol reasons over three layers.

#### Layer A — Order Layer

This layer determines structural support and preferred ordering relations in the DAG.

#### Layer B — Closure Layer

This layer measures whether a block and its dependency world are sufficiently mature, stable, and coherently visible across honest nodes.

#### Layer C — Finality Layer

This layer derives confirmation/finality from both order and closure, rather than from order alone.

中文说法：

* **Order 层**回答“这个块在结构上站在哪里”；
* **Closure 层**回答“这个块的依赖世界是否已经成熟”；
* **Finality 层**回答“结合前两者，它什么时候能被经济上信任”。

### 3.2 Consequence

This means finality is no longer a direct byproduct of a single DAG score. Instead:

[
Finality(B)=F\big(OrderStrength(B), ClosureLevel(B)\big)
]

where:

* `OrderStrength(B)` comes from structural DAG support,
* `ClosureLevel(B)` comes from dependency maturity and support coherence.

---

## 4. Core Mathematical Sketch

## 4.1 DAG state

Let the observed DAG be (G=(V,E)), where each block (B \in V) has:

* parent/merge references,
* timestamp and proof information,
* descendant support,
* closure-related evidence.

For each block (B), define two core quantities.

### 4.1.1 Order strength

[
O(B) \in \mathbb{R}_{\ge 0}
]

`O(B)` is a structural support quantity. It may be derived from a GhostDAG-like ordering core, or another DAG support rule. Intuitively it measures how strongly the DAG structure supports placing `B` in the canonical history/order.

### 4.1.2 Closure level

[
C(B) \in [0,1]
]

`C(B)` is a closure maturity quantity. It measures how complete and stable the block’s dependency world is.

`C(B)` should be influenced by quantities such as:

* dependency completeness,
* support coherence from descendants,
* frontier stability,
* unresolved dependency risk,
* observed evidence that the block’s ancestry and merge context have broadly stabilized.

中文关键点：**Closure 不是“这个块自己有没有到”，而是“围绕这个块的依赖世界是不是已经足够成熟”。**

---

## 4.2 Bi-objective consensus functional

The protocol evaluates candidate consensus-supporting structures (S) with:

[
\Phi(S)=U_{order}(S)-\lambda R_{closure}(S)
]

where:

### Order utility

[
U_{order}(S)
]
rewards:

* coherent structural order,
* useful concurrency,
* descendant-backed support,
* stable preferred-chain evolution.

### Closure risk

[
R_{closure}(S)
]
penalizes:

* dependency incompleteness,
* fragile frontier reliance,
* high unresolved predecessor exposure,
* structures that are valid combinatorially but likely unstable under heterogeneous visibility.

### Control parameter

[
\lambda > 0
]
sets the tradeoff between structural concurrency capture and closure conservatism.

中文解释：

* `U_order` 奖励“结构上有价值的并发”；
* `R_closure` 惩罚“虽然看起来合法，但在闭包意义上很脆弱的结构”；
* `λ` 决定协议偏乐观还是偏保守。

This is the fundamental departure from GhostDAG:

> **the protocol is no longer maximizing structure alone.**

---

## 4.3 Finality functional

Strong confirmation is determined not by order alone but by a joint function:

[
F(B)=f\big(O(B),C(B),D(B)\big)
]

where `D(B)` may optionally represent settlement depth / descendant accumulation.

A simple form could be:

[
F(B)=\sigma\big(\alpha O(B)+\beta C(B)+\gamma D(B)-\tau\big)
]

where (\sigma) is a monotone squashing or thresholding function.

中文意思是：最终性不再是“blue score 到了就算强确认”，而是**结构支持、闭包成熟度、沉淀深度**共同决定。

---

## 5. Protocol Objects and State Variables

Each block maintains or accumulates the following logical state.

### 5.1 Structural state

* parent set
* selected-parent-like relation
* structural support score
* ordering rank or comparable structural position

### 5.2 Closure state

* dependency completeness score
* unresolved predecessor exposure
* closure support attestations from descendants
* frontier stability score
* closure maturity level

### 5.3 Finality state

* tentative inclusion status
* strong confirmation status
* settlement confidence

中文这里最关键的是：**闭包状态成为协议显式状态，而不是隐含在网络实现里。**

---

## 6. Confirmation Semantics

## 6.1 Three confirmation grades

The protocol should expose at least three grades of confirmation.

### Grade 1 — Tentative Ordering

A block is structurally well-placed in the DAG, but closure maturity is not yet sufficient for strong trust.

### Grade 2 — Strong Confirmation

A block has both high structural support and sufficiently high closure maturity.

### Grade 3 — Settlement / Deep Finality

A block has accumulated enough joint support that rollback becomes economically or combinatorially negligible.

中文这意味着用户和应用看到的不再是单一确认值，而是：

* tentative inclusion，
* strong confirmation，
* deep settlement。

这比传统“一个 confirmation number 走天下”的模型更诚实。

---

## 6.2 Why this addresses the paper’s challenge

The late predecessor problem arises because “block received” does not imply “block accepted and safely mineable.” In this design, that gap is no longer hidden. It is represented explicitly in `ClosureLevel`.

Similarly, block jam no longer merely appears as an external implementation annoyance. Its effect propagates into closure maturity and therefore into finality semantics.

中文说就是：论文中的两大现象——block jam 和 late predecessor——不再只是协议外部的“脏现实”，而是被协议核心数学直接吸收和表达。

---

## 7. Relationship to GhostDAG

## 7.1 What remains similar

The order layer can still be GhostDAG-like. The protocol may continue to exploit:

* DAG structure,
* structural concurrency,
* a preferred-chain or preferred-order relation,
* merge-aware support accumulation.

### 7.2 What changes fundamentally

What changes is the role of the order layer.

In GhostDAG, the structural object is asked to carry too much semantic weight. In this proposal, order remains necessary but becomes **insufficient** for strong finality.

中文最短总结：

* **GhostDAG**：结构对象近似承担排序 + 可接受性 + 最终性。
* **本方案**：结构对象只负责排序；闭包对象负责成熟度；最终性由两者共同决定。

### 7.3 Fork distance

This is therefore not a small GhostDAG branch. It is a **medium-far to far** protocol divergence.

If implemented seriously, it deserves a distinct protocol identity rather than a simple suffix like `GhostDAG+`.

---

## 8. Expected Effects on TPS and Finality

## 8.1 TPS

### Happy-path theoretical TPS

May remain similar or decrease slightly relative to aggressively optimistic GhostDAG settings, because not all raw concurrency is treated as equally valuable.

### Realized TPS under imperfect networks

Likely improves, because the protocol wastes less concurrency on structures that are structurally legal but closure-fragile.

### Stress-path TPS

Should degrade more gracefully than a pure structural protocol.

中文一句话：

* **广告式峰值 TPS** 可能不更高，甚至略低；
* **真实可实现 TPS / 高压场景下的有效 TPS** 反而更可能提升。

## 8.2 Finality

### Fast tentative confirmation

Should remain reasonably fast because the order layer can still advance quickly.

### Strong finality

Will likely be slower than pure GhostDAG-style structural confidence, because closure must also mature.

### Semantic quality of finality

Substantially improves: finality becomes more honest, more robust, and less dependent on hidden optimistic assumptions.

中文一句话：

* **快速确认**：接近现有，或略慢；
* **强最终性**：更慢，但更干净、更可信；
* **高压尾部最终性**：会明显更稳。

---

## 9. Design Choices and Open Parameters

The following are open design questions.

### 9.1 How to define closure maturity

Possible ingredients include:

* dependency completeness ratio,
* coherence of descendant support,
* frontier stability windows,
* closure attestations,
* bounded unresolved dependency age.

### 9.2 How to compute order utility

This can remain GhostDAG-like, or evolve into a weighted structural support rule.

### 9.3 How to choose lambda

The parameter `λ` is economically and operationally meaningful:

* small `λ`: more optimistic, more concurrency-seeking,
* large `λ`: more conservative, more closure-sensitive.

### 9.4 Whether closure is deterministic or probabilistic

A deterministic lattice is easier to reason about.
A probabilistic closure posterior is more expressive but harder to implement and prove.

中文这些问题本质上决定了协议到底更像：

* “GhostDAG + closure layer”，
  还是
* “全新的 probabilistic DAG consensus”。

---

## 10. Threat-Model Fit

This proposal is directly motivated by the observation that, in DAG systems under load, different honest nodes may receive and accept different dependency worlds at different times. The protocol therefore should not pretend that a single structural score fully captures consensus safety. fileciteturn0file0L172-L180 fileciteturn0file0L242-L248

中文也就是说，这个方案的威胁模型适配点非常明确：它不是试图否认论文中的现象，而是承认这些现象是**协议本体必须吸收的现实**。

---

## 11. Research Roadmap

## Phase 1 — Formal model

* define `OrderStrength`
* define `ClosureLevel`
* define `Finality(B)`
* specify the state machine and admissible transitions

## Phase 2 — Simulation model

* simulate block jam
* simulate late predecessor / staggered acceptance
* compare against GhostDAG under identical network assumptions

## Phase 3 — Security analysis

* chain growth under joint order/closure semantics
* quality and consistency under closure-aware finality
* adversarial threshold as a function of closure degradation

## Phase 4 — Prototype protocol

* implement order layer
* implement closure tracking
* expose three confirmation grades

中文路线图就是：

1. 先把数学对象定清楚；
2. 再在 CBM/LP 场景里做仿真；
3. 然后推安全阈值；
4. 最后才做原型。

---

## 12. Naming

If the protocol remains only an engineering hardening of GhostDAG, it should remain under the GhostDAG name family.

If this proposal is implemented seriously, it deserves a distinct name because closure becomes first-class protocol mathematics.

Suggested research names:

* **ClosureDAG**
* **Bi-objective Closure-Aware DAG Consensus**
* **Closure-Aware DAG Finality Protocol**

中文如果真做到这一步，`ClosureDAG` 这样的名字才算名正言顺；因为这时 closure 已经不是工程细节，而是协议数学的第一等公民。

---

## 13. Conclusion

This proposal argues that the root limitation of GhostDAG-like protocols is not merely that their relay path can be attacked, but that their mathematics lets one structural object stand in for ordering, acceptability, and finality all at once.

本文的核心结论是：**根问题不是 GhostDAG 的排序规则太弱，而是它试图让一个结构排序对象同时替代顺序、可接受性与最终性。**

The root-level solution is therefore:

> **split structural order, closure maturity, and finality into distinct mathematical roles, and reconnect them through an explicit higher-level consensus functional.**

That is the essence of **Bi-objective Closure-Aware DAG Consensus**.
