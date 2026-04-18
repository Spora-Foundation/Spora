# CellScript: Spora 区块链的领域特定语言

**状态**: 实现中 (Phase 1 编译器 MVP 已收尾，Phase 2 资产生命周期/共享状态 operational exit gate 已关闭，Phase 3 节点/调度器集成 operational exit gate 已关闭，Phase 4 生产强化已开始；文档已按当前代码状态收紧)
**日期**: 2026-04-13  
**作者**: Spora 核心团队  
**类别**: 语言设计 / 协议工程  
**依赖**: ckbvm (RISC-V), CellTx 信封, GhostDAG 共识  

**实现快照文档**: [CELLSCRIPT_IMPLEMENTATION_STATUS.md](/Users/arthur/RustroverProjects/Spora/docs/CELLSCRIPT_IMPLEMENTATION_STATUS.md)
**执行阶段表**: [CELLSCRIPT_EXECUTION_PHASES.md](/Users/arthur/RustroverProjects/Spora/docs/CELLSCRIPT_EXECUTION_PHASES.md)

---

## 实现状态

> ⚠️ **重要提示**: 本文档是设计提案，描述的是**设计意图**而非**实现真相**。
> 
> 若需要了解当前代码的实际状态，请优先查看 **[CELLSCRIPT_IMPLEMENTATION_STATUS.md](/Users/arthur/RustroverProjects/Spora/docs/CELLSCRIPT_IMPLEMENTATION_STATUS.md)**。
> 
> 以下表格已按当前代码状态收紧，但可能仍有滞后。

| 组件 | 状态 | 路径 |
|---|---|---|
| 词法分析器 (Lexer) | ✅ 已实现 | `cellscript/src/lexer/` |
| 解析器 (Parser) | ✅ 当前语法子集稳定 | `cellscript/src/parser/` |
| AST 定义 | ✅ 已实现 | `cellscript/src/ast/` |
| 类型检查器 | 🟡 生产加固中，非完整语义证明 | `cellscript/src/types/` |
| 线性检查器 | 🟡 生产加固中，非完整资源证明 | `cellscript/src/types/` |
| Spora IR | 🟡 lowering 主路径真实可用，复杂状态语义仍不完整 | `cellscript/src/ir/` |
| RISC-V 代码生成 | 🟡 `asm` 主路径稳定 / `ELF` 可用子集 | `cellscript/src/codegen/` |
| CLI 编译器 | 🟡 主编译入口可用 | `cellscript/src/main.rs` |
| 标准库 | 🟡 基础运行时支持已接通，非完整标准库 | `cellscript/src/stdlib/` |
| REPL 交互式解释器 | 🟡 基础可用 | `cellscript/src/repl.rs` |
| 调度器元数据生成 | 🟡 metadata / `scheduler_witness_borsh_hex` 可用，`spora-exec` 可 admission，MPE `BlockAccessSummary` 已开始消费 shared touch 冲突域 | `cellscript/src/stdlib/mod.rs` |
| 模块系统/名称解析 | 🟡 本地包 / path 依赖可用 | `cellscript/src/resolve/` |
| 生命周期验证 | 🟡 部分集成到主编译路径/LSP，完整运行时转换验证未完成 | `cellscript/src/lifecycle/` |
| 优化器 | 🟡 保守 AST 优化已接入 `opt_level > 0` 主编译链，非完整优化器 | `cellscript/src/optimize/` |
| 文档生成器 | 🟡 API 文档 + lowering audit / obligation 输出可用子集 | `cellscript/src/docgen/` |
| 代码格式化器 | 🟡 部分可用 | `cellscript/src/fmt/` |
| LSP 服务器 | 🟡 最小真实路径，metadata-aware hover/诊断/code action | `cellscript/src/lsp/` |
| 包管理器 | 🟡 本地包 / path 依赖可用，registry/remote 流程未完成 | `cellscript/src/package/` |
| 测试框架 | 🟡 compile-test 发现与期望失败诊断可用，非运行时/属性测试框架 | `cellscript/src/test/` |
| Wasm 目标 | 🚧 metadata-only / fail-closed，非可执行后端 | `cellscript/src/wasm/` |
| 增量编译 | 🚧 预留模块/stub，未接主编译链 | `cellscript/src/incremental/` |
| CLI 子命令 | 🟡 本地工作流已接主入口，registry/runtime 命令仍 fail-closed/feature-gated | `cellscript/src/cli/` |
| 集合类型 | 🚧 基础定义 | `cellscript/src/stdlib/collections.rs` |
| 调试信息 | 🚧 原型级 | `cellscript/src/debug/` |
| 测试套件 | 🟡 默认特性 `cargo test -p cellscript` 当前 333 项通过 | `cellscript/src/`, `cellscript/tests/` |

**编译器项目路径**: `/Users/arthur/RustroverProjects/Spora/cellscript/`  

---

## 文档范围与规范性边界

本文档区分以下四类内容，读者不应混淆：

### 1. 核心语言语义（规范性）
- 第3-6节定义的类型系统、所有权模型、效果系统
- `resource`、`shared`、`action`、`consume`、`create`等关键字的行为
- 线性检查、生命周期验证的规则
- **约束**：必须严格符合红线哲学，禁止任何隐藏效果、隐藏成本、隐藏控制流的特性

### 2. 附录与路线图（非规范性，探索性）
- 附录A中的"🚧 原型级"标记内容
- "v1后研究方向"章节
- 明确标记为"设计中"、"计划中"的功能
- **约束**：这些不构成实现承诺，未来需单独的设计提案和安全性论证

### 3. 编译器内部实现（工程细节，非语义承诺）
- Spora IR的数据结构表示
- AST、类型检查器、优化器的内部算法
- 代码生成器的实现策略
- **约束**：编译器可使用常规工程数据结构（Vec/HashMap等），但这些是内部实现细节，不构成对用户代码的语言级保证，也不得泄漏为共识执行路径的用户可见抽象

### 4. 链下工具（共识外）
- SDK、交易构建器、LSP、文档生成器
- 测试框架、模拟环境
- **约束**：这些工具在共识执行环境外运行，可使用常规工程手段，不影响共识语言设计

**重要原则**：
- 核心语言语义是薄而锋利的
- 附录项目是探索性的，不是承诺
- 编译器内部实现细节不得成为用户依赖的语义
- 链下工具的工程便利不得影响共识路径的严格性

---

### 当前可依赖的主路径

截至当前代码状态，可以认为稳定的主路径是：

1. `.cell` 单文件 / 包目录 / `Cell.toml` 输入解析
2. `lex -> parse -> type check -> 最小 IR lowering -> RISC-V asm / ELF`
3. 本地 `Cell.toml`、`path` 依赖、`source_roots`、默认输出目录
4. `cellc` 主入口编译、基础 REPL、examples / CLI / library 回归测试

不能把下列模块视为“已完成并可依赖”：

- 完整 registry / runtime CLI 生态
- 完整优化器
- 完整 LSP / Docgen / Fmt / Package Manager
- WebAssembly 主目标链
- 复杂控制流、资源副作用和跨模块调用的完整 lowering
- `launch` / `pool` / `claim` / `settle` / `transfer` 的完整可执行协议语义
- 调度器元数据在共识/MPE 执行层的强制消费

---

## 1. 执行摘要

### 1.1 本文档的提案内容

本文档提出 **CellScript**，一种面向 Spora 区块链的、窄域的、资产生命周期导向的领域特定语言。CellScript 编译为 RISC-V ELF 二进制文件，在现有的 ckbvm 基础设施上运行。它不引入新的虚拟机，也不取代 CellTx 信封。它在现有执行栈之上分层了一个类型安全、线性强制的编程模型。

### 1.2 为什么这个 DSL 应该存在

Spora 的 Cell 模型功能强大但底层。今天，编写 Spora 脚本意味着：

1. 手动编码原始字节形式的见证数据
2. 手动管理 Cell 生命周期（创建、消费、数据布局）
3. 编写通过编号调用系统调用的 RISC-V C 或汇编代码（2061、2071、2075 等）
4. 没有编译器强制保证的线性资源使用
5. 无法表达 DAG 并行执行的调度器提示

这大致相当于用 EVM 字节码编写以太坊合约。它能工作，但无法扩展到合约作者生态系统。

CellScript 的存在是为了弥合这一差距：为协议设计者提供一种理解 Cell、理解线性、理解 DAG 调度的语言，并编译成 ckbvm 已经在执行的相同 RISC-V ELF 二进制文件。

### 1.3 为什么不直接使用 Solidity / Move / Sway

**Solidity** 假设基于账户的存储模型。每个 `SSTORE`/`SLOAD` 都针对合约持久存储中的 256 位槽位。Spora 没有这样的模型。Cell 是具有 OutPoint 身份的离散对象，被原子性地消费和创建。将 Solidity 适配到 Cell 语义需要彻底改变其存储模型，到那时你就不再拥有 Solidity 了。

**Move** 更接近。它具有带有线性语义的资源类型。但 Move 的模块系统假设具有命名地址的全局模块存储，其字节码是基于栈的，没有 CellDep、OutPoint 或 DAG 调度的概念。Sui 上的 Move 添加了共享对象，但 Sui 的执行模型（Narwhal/Bullshark）与 GhostDAG 合并集处理根本不同。移植 Move 意味着分叉语言并永久分叉。

**Sway** (Fuel) 是 UTXO 感知的且类似 Rust，但它针对 FuelVM 谓词，而不是 RISC-V ELF。Fuel 的 UTXO 模型比 Cell 更简单（没有类型脚本、没有 CellDep、没有基于 since 的时间锁）。Sway 没有共享状态对象或 DAG 并行调度提示的概念。

**结论**：这些语言都不是为 {Cell 模型 + GhostDAG + ckbvm + 3 维 mass} 设计的。在它们中的任何一个之上构建都需要比从头构建一个专注的 DSL 更多的适配工作。CellScript 是故意窄域的——它做的事情更少，但这些事情完美映射到 Spora。

### 1.4 建议的名称

**CellScript**。该名称直接、描述性强，并将语言相对于其主要抽象（Cell）进行定位。考虑过但拒绝的替代名称：

- *SporaLang*: 太通用，没有说明语言的作用
- *CellLisp*: 错误的范式关联
- *SporeScript*: 可爱但不专业

CellScript。源文件使用 `.cell` 扩展名。

---

## 2. 架构适配

### 2.1 CellScript 如何适配 Spora 的 Cell/状态本体论

Spora 的状态模型围绕离散的 Cell 对象组织：

```
CellTx {
    ver: u16 (0xC001),
    // 注：以下Vec是协议层CellTx信封的序列化格式，不是CellScript语言级类型
    inputs: Vec<CellInput>,          // 要消费的 Cell
    deps: Vec<CellDep>,            // 只读 Cell 引用
    header_deps: Vec<[u8;32]>,     // 区块头引用
    outputs: Vec<CellOutput>,         // 要创建的新 Cell
    outputs_data: Vec<Vec<u8>>,    // 附加到输出的数据
    witnesses: Vec<Vec<u8>>,       // 签名、证明
}
```

CellScript 的类型系统直接映射到这个结构：

| CellScript 概念 | CellTx 映射 |
|---|---|
| `resource` 声明 | `CellOutput` + `outputs_data[i]` |
| `consume expr` | `inputs` 中的条目作为 `CellInput` |
| `create expr` | `outputs` + `outputs_data` 中的条目 |
| `read_ref expr` | `deps` 中的条目作为 `CellDep` |
| `shared` 声明 | 通过 `CellDep`（读取）或 `CellInput`（写入）访问的 Cell |
| 本地 `let` 绑定 | 见证数据或中间计算；永不在 CellStateTree 中 |
| `action` 函数 | 编译为 RISC-V ELF 的类型脚本逻辑 |
| `lock` 函数 | 编译为 RISC-V ELF 的锁定脚本逻辑 |

这不是隐喻性映射。编译器字面生成 CellTx 形状的输出。`create` 表达式生成 `CellOutput` 结构。`consume` 表达式生成 `CellInput`。程序员用 CellScript 编写；编译器发出有效的 CellTx 组件和 RISC-V ELF 脚本。

### 2.2 CellScript 如何适配 DAG 导向的执行

在单链区块链中，交易在区块内顺序执行。在 Spora 的 GhostDAG 模型中：

- 多个区块可以并发挖掘
- 蓝色区块的合并集按规范顺序处理
- VirtualProcessor 从每个区块累积 CellDiff
- 区块内的并行执行是可能的（P1，已完成），跨蓝色区块的并行执行也是可能的（MPE，设计中）

CellScript 通过以下方式支持这一点：

1. **效果分类**：每个 `action` 都标有效果类别（`Pure`、`ReadOnly`、`Mutating`、`Creating`、`Destroying`）。编译器从 action 主体推断这一点。

2. **访问摘要发出**：设计目标是让编译器在指定的见证字段中发出与 `BlockAccessSummary` 兼容的元数据 blob；当前实现先通过 compile metadata / sidecar 暴露这些信息，列出：
   - `spent_outpoints`: 消费的 OutPoints
   - `created_outpoints`: 创建的 OutPoints（从确定性 OutPoint 推导预测）
   - `read_deps`: 通过 `read_ref` 读取的 OutPoints
   - `touches_shared`: 访问的共享对象的 type_hashes

3. **调度器提示嵌入**：元数据包括 `parallelizable: bool` 和 `estimated_cycles: u64`，使区块模板构建器和 MPE 执行 DAG 能够在不重新分析脚本代码的情况下做出调度决策。

重要的设计选择是，这个接触面应该默认从 action 主体**推断**：
- `consume` 意味着消费的输入
- `create` 意味着创建的输出
- `read_ref` 意味着依赖读取

只有非明显的部分需要显式注释：
- 共享状态写入域
- 效果类别消歧
- 编译器无法自行推断足够调度器表面的罕见情况

这直接支持 MPE 设计文档的阶段 2（`BlockAccessSummary`）和阶段 3（`区块级执行 DAG`），而无需更改 GhostDAG 本身。

### 2.3 CellScript 如何适配 MPE 并行化设计

MPE 设计文档确定了核心需求：蓝色区块处理必须分解为**纯效果生成**，然后是**顺序提交**。CellScript 通过设计与此对齐：

```
                    CellScript 源代码
                          │
                          ▼
                  ┌───────────────┐
                  │   编译器      │
                  └───────┬───────┘
                          │
              ┌───────────┼───────────┐
              ▼           ▼           ▼
        RISC-V ELF   类型化数据   调度器
        (锁定/类型   布局         元数据
         脚本)                  (见证)
              │           │           │
              └───────────┼───────────┘
                          ▼
              ┌───────────────────────┐
              │   ckbvm 执行          │
              │   (现有基础设施)      │
              └───────────┬───────────┘
                          │
                          ▼
              ┌───────────────────────┐
              │  BlockExecutionEffect │
              │  (纯的、可组合的)     │
              └───────────────────────┘
```

每个 CellScript action 产生确定性效果。调度器元数据让执行层识别可以在同一块内并行运行的独立操作，以及跨合并集中区块的并行运行。

### 2.4 CellScript 如何适配 ckbvm 集成

CellScript **不**引入新的 VM。它编译为在 ckbvm 上执行的标准 RISC-V ELF 二进制文件，使用现有的系统调用接口：

| 系统调用 | 编号 | CellScript 用法 |
|---|---|---|
| `LOAD_TX_HASH` | 2061 | 在 sighash 计算中隐式使用 |
| `LOAD_SCRIPT_HASH` | 2062 | 被 `self.script_hash()` 使用 |
| `LOAD_CELL` | 2071 | 被 `consume`、`create`、`read_ref` 使用 |
| `LOAD_HEADER` | 2072 | 被 `header_dep` 访问使用 |
| `LOAD_INPUT` | 2073 | 被 `consume` 内部使用 |
| `LOAD_WITNESS` | 2074 | 被见证数据访问使用 |
| `LOAD_SCRIPT` | 2075 | 被 `self.script()` 使用 |
| `LOAD_CELL_BY_FIELD` | 2081 | 被字段级 Cell 访问使用 |
| `LOAD_CELL_DATA` | 2092 | 被 `cell.data()` 使用 |
| `CURRENT_CYCLES` | 2042 | 被 `remaining_cycles()` 使用 |
| `DEBUG_PRINT` | 2177 | 被 `debug!()` 宏使用 |
| `BLAKE3` (Spora 扩展) | TBD | 被 `hash()` 内置函数使用 |

编译器将薄 CellScript 标准库链接到每个 ELF 二进制文件中。该标准库提供：
- Cell 数据的 Borsh 序列化/反序列化
- 具有安全 Rust 风格 API 的系统调用包装器
- 运行时的线性强制（调试模式）和编译时的线性强制（始终）
- 调度器元数据序列化

生成的 ELF 二进制文件与手写 ckbvm 脚本无法区分。现有的原始脚本和 CellScript 编译的脚本可以在同一交易中共存。

---

## 3. 语言哲学

### 3.1 CellScript 的用途

CellScript 是一种用于在 Spora 上表达**资产生命周期和状态转换逻辑**的语言。具体来说：

- **资产定义**：声明具有强制供应规则的可替代和不可替代资产类型
- **状态转换**：定义 Cell 的有效状态更改（创建、变异、销毁）
- **池机制**：表达流动性池不变量（恒定乘积、加权储备）
- **结算逻辑**：定义待定状态（收据、归属计划）如何解析为最终状态
- **授权**：表达锁定条件（签名验证、多签、时间锁）
- **生命周期管理**：编码状态机转换（已创建 → 活跃 → 已结算 → 已销毁）

### 3.2 CellScript 不用于什么

CellScript 故意不针对：

- **通用计算**：没有无界数据上的循环。没有任意字符串处理。没有浮点数。如果你需要运行神经网络，CellScript 是错误的工具。
- **链下逻辑**：CellScript 没有网络、没有文件 I/O、没有随机性。它在 ckbvm 内确定性地执行。
- **UI/前端**：CellScript 不生成客户端代码。Rust/TypeScript/Go 中的 SDK 通过 RPC 加上交易构建/规划 API 与 CellScript 编译的脚本交互。
- **跨链消息传递**：CellScript 验证 Spora 内的状态转换。桥接逻辑需要构建 CellScript 兼容交易的链下中继器。

### 3.3 协议设计者自由度 vs 应用级自由度

CellScript 占据中间地带：

- **比 Solidity 更受约束**：你不能编写任意程序。类型系统强制线性。编译器拒绝复制资源或在没有显式销毁的情况下丢弃它们的代码。
- **比 Bitcoin Script 更具表达力**：你有结构化类型、控制流、具体辅助函数和协议可见的 Cell 操作。你可以表达复杂的 AMM 不变量、归属计划和多边结算协议。
- **与 Move 类似级别**：面向资源，具有显式生命周期管理。但更窄——没有通用模块系统，没有动态分派，没有无界集合。

目标用户是**协议设计者**，他们以"我有一个具有这些规则的资产、一个具有这些不变量的池、以及一个具有这些步骤的结算过程"的方式思考。CellScript 使这些思想可以直接表达并由编译器验证。

### 3.4 故意牺牲的表达力

| 特性 | Solidity 有它 | CellScript 省略它 | 原因 |
|---|---|---|---|
| 无界循环 | 是 (`for`, `while`) | 否（仅限有界迭代） | 周期预算很难：每笔交易最多 1000 万。无界循环是 DoS 向量。 |
| 动态分派 | 是（接口） | 否 | 类型脚本由 code_hash 静态解析。动态分派在 Cell 模型中增加间接性而没有好处。 |
| 继承 | 是 | 否 | 组合优于继承。CellScript 使用类似特性的能力。 |
| 重入 | 是（并为此后悔） | 结构上不可能 | Cell 模型原子性地消费输入。不存在回调模式。 |
| 任意存储 | 是（映射/数组） | 否（Cell 数据是固定布局） | Cell 具有类型化的固定大小数据。"增长"存储意味着创建新 Cell。 |
| 字符串操作 | 是 | 否 | 脚本验证状态转换，不处理文本。 |

---

## 4. 核心语义模型

### 4.1 `resource` — 线性 Cell 类型

**含义**：`resource` 是表示 Cell 的线性类型。它不能被复制。它不能被静默丢弃。每个资源实例必须显式消费（花费）、转移（移动到新所有者）或销毁（燃烧）。这在编译时强制执行。

**存在原因**：Cell 模型的基本属性是 Cell 被原子性地消费和创建。Cell 不能存在于两个地方。它不能被花费两次。CellScript 的 `resource` 类型使这一属性成为编译时保证，而不是程序员必须手动维护的运行时不变量。

**解决的问题**：
- 在语言级别防止双重花费错误
- 防止"丢失的 Cell"（创建但从未使用的资源）
- 使资产供应不变量可由编译器检查

**提交细节**：

```CellScript
resource FungibleToken {
    amount: u64,
    symbol: [u8; 8],
}
```

映射到：
- `CellOutput.type_` = 指向 FungibleToken 类型脚本的脚本
- `outputs_data[i]` = Borsh 序列化的 `{ amount: u64, symbol: [u8; 8] }`
- `CellOutput.capacity` = 此数据布局所需的最低容量
- `CellOutput.lock` = 所有者的锁定脚本（由 `transfer` 目标设置）

**与 Solidity/Move/Sway 的区别**：
- **Solidity**: 没有线性类型。ERC-20 余额是 `mapping(address => uint256)`——一个可变存储槽，不是离散对象。没有什么阻止映射被任意读写。
- **Move**: 具有带有 `key`、`store`、`copy`、`drop` 能力的资源。CellScript 的模型更简单：资源具有能力（`store`、`transfer`、`destroy`）但没有 `copy` 或 `drop`。Move 资源存在于由 `(address, module, type)` 寻址的全局存储中；CellScript 资源存在于由 `OutPoint` 寻址的 Cell 中。
- **Sway**: 在协议级别具有原生资产但没有用户定义的线性类型。Sway 谓词验证 UTXO 花费条件但不定义类型化的资源对象。

### 4.2 `shared` — 共享状态 Cell

**含义**：`shared` 对象是一个可以被多个交易并发读取（通过 `CellDep`）但被独占写入（通过 `CellInput` 消费 + 重新创建）的 Cell。这模拟了共享协议状态，如流动性池、注册表或配置 Cell。

**存在原因**：许多 DeFi 协议需要共享可变状态。在基于账户的模型中，这是隐式的（每个合约都有共享状态）。在 Cell 模型中，共享状态必须显式设计。`shared` 关键字将 Cell 标记为协议共享，并触发编译器：
1. 为读取者发出通过 CellDep 的读取模式
2. 为写入者发出消费并重新创建的模式
3. 在调度器元数据中包括 `type_hash` 以进行争用检测

**如何映射到 Spora**：
- 读取：交易将共享 Cell 作为 `CellDep` 包括，使用 `dep_type: Code`
- 写入：交易将共享 Cell 作为输入（`CellInput`）消费，并创建一个更新的数据的新 Cell 作为输出（`CellOutput`）
- 识别：共享 Cell 由其 `type_hash`（脚本的 blake3 哈希）识别
- 争用：多个写入同一共享 Cell 的交易冲突——每个区块只能有一个成功，由规范顺序解决

**区别**：
- **Solidity**: 所有合约状态都是隐式共享的。没有选择加入，没有争用感知。
- **Move (Sui)**: 具有显式的 `shared` 对象，具有共识排序访问。类似的概念，但 Sui 使用不同的共识机制（不是 GhostDAG）。
- **Sway**: 没有共享状态概念。UTXO 要么被花费，要么没有。

### 4.3 `receipt` — 短暂的操作证明

**含义**：`receipt` 是某个操作发生的单次使用证明。它是一个具有特殊类型脚本的 Cell，强制执行：（1）它只能由特定操作创建，以及（2）它必须被消费恰好一次。收据是 Cell 模型中基于账户系统中"事件"的等价物，但有一个关键区别：它们是有状态的必须显式认领的对象。

**存在原因**：许多协议需要证明某事发生（已存款、归属期已开始、已投票），然后稍后对该证明采取行动。在基于账户的系统中，这是通过事件日志或存储标志完成的。在 Cell 模型中，收据是具有生命周期保证的一流对象。

**如何映射到 Spora**：
- 收据是一个 `CellOutput`，其类型脚本编码：
  - 创建者：产生此收据的操作
  - 认领条件：时间锁、签名要求或其他谓词
  - 负载：收据证明的任何数据（存款金额、归属计划等）
- 类型脚本强制执行收据 Cell 只能由有效的认领操作消费
- 一旦被消费，收据就消失了——它不能被重放

### 4.4 `launch` — v1 后的交易构建器模式

**含义**：`launch` 是一个 v1 后的交易构建器模式，将新资产类型的创建与其初始配置捆绑在一起。它结合：
1. 创建资产的类型脚本 Cell（部署合约）
2. 铸造初始供应
3. 可选地播种流动性池
4. 将初始代币分发到指定地址

**存在原因**：实际上，在任何链上启动新代币都涉及多个协调输出。未来的 CellScript 交易构建器可以把它变成单个原子 CellTx，减少部分部署错误。

**如何映射到 Spora**：未来的 `launch` lowering 会编译为单个 CellTx，具有：
- 输出 0：类型脚本 Cell（资产的代码，作为数据 = ELF 二进制文件的 Cell 部署）
- 输出 1..N：初始代币 Cell（铸造的供应分发给接收者）
- 输出 N+1：可选的池 Cell（以初始流动性播种）
- 输出 N+2：可选的 LP 收据 Cell（初始流动性提供的证明）

当前状态：`launch` 不是 v1 语言核心。在交易构建器 lowering 存在之前，示例应使用显式 `create` 操作和普通 action 建模 launch。

### 4.5 池模式 — 共享流动性对象

**含义**：池是由 `shared` Cell、action 逻辑、不变量和 receipt/resource 输出组成的协议模式。它不是独立的语言关键字或声明类。

**存在原因**：AMM 池是 DeFi 中最常见的共享状态模式。它们值得标准 metadata 和工具支持，但不变量族属于协议特定逻辑，不属于语言核心语义：
- 调度器元数据仍可包含底层 shared Cell 的 type_hash 以进行争用检测
- 审计 metadata 可暴露池特定运行时义务
- 标准库可提供 AMM 模板，而不把 AMM 数学硬编码进语言

**如何映射到 Spora**：池是一个共享 Cell，其中：
- `CellOutput.type_` = 池类型脚本（强制执行 AMM 不变量）
- `outputs_data[i]` = Borsh 序列化的池状态：`{ reserve_a: u64, reserve_b: u64, total_lp: u64, fee_rate: u16 }`
- 交换交易消费池 Cell 并创建具有更新储备的新池 Cell
- LP 添加/删除交易修改储备并创建/消费 LP 收据 Cell

### 4.6 `settle` — 最终化操作

**含义**：`settle` 操作将待定状态转换为最终状态。它消费收据 Cell，评估其认领条件，并产生最终资产 Cell。结算是协议交互的"结束括号"。

**如何映射到 Spora**：结算操作编译为执行以下操作的 CellTx：
- 消费一个或多个收据/待定 Cell（输入）
- 验证认领条件（通过 `since` 字段的时间锁、通过见证的签名、不变量）
- 产生最终资产 Cell（输出）
- 将生命周期状态从 `Pending` 转换为 `Settled`

### 4.7 交易局部值与 CellStateTree 提交

**含义**：普通本地绑定仅在交易执行期间存在，不会提交到 CellStateTree。中间计算、见证数据解析和临时状态使用普通 `let` 绑定。

**如何映射到 Spora**：
- 见证数据（`CellTx.witnesses`）本质上是交易局部的
- 中间计算结果存在于 ckbvm 内存中
- 只有 `create` 会产生进入 CellStateTree 的 Cell 输出
- 线性资源检查确保 Cell 支撑的值被消费、返回或显式物化

**CellStateTree 提交**：当通过 `create` 创建 `resource`、`shared` 或 `receipt` 对象时，它成为 CellStateTree 中的 Cell，由 MuHash 跟踪以进行 O(1) 增量根计算。这个行为不需要单独的关键字。

**如何映射到 Spora**：
- CellStateTree 存储 `CellEntry { capacity, data_bytes, lock_hash, type_hash, data_hash, block_daa_score, is_cellbase }`
- MuHash 累加器提供区块承诺中使用的 `cell_root`：`cell_commitment = H("spora/cell_commitment/v0" || cell_root)`
- CellDiff 跟踪每个区块的添加和删除

---

## 5. 类型系统提案

### 5.1 最小有用类型系统

CellScript 的类型系统是故意最小的。它包括：

**原始类型**：
- `u8`, `u16`, `u32`, `u64`, `u128`: 无符号整数
- `bool`: 布尔值
- `[u8; N]`: 固定大小的字节数组（N ≤ 256）
- `Hash`: `[u8; 32]` 的别名
- `Address`: 锁定脚本引用（编码为脚本）

**复合类型**：
- `resource T { ... }`: 线性类型（不能复制，不能隐式丢弃）
- `shared T { ... }`: 共享状态类型（线性，具有争用语义）
- `receipt T { ... }`: 单次使用证明类型（线性，具有认领语义）
- `struct T { ... }`: 非线性数据类型（可以复制，嵌入在资源中）

**集合类型**：
- `[T; N]`: 固定大小的数组（N 必须是编译时常量）
- 没有 `Vec`，没有 `HashMap`。CellScript 中不存在可变大小的集合。如果你需要可变大小的数据，你创建多个 Cell。

### 5.2 所有权和线性

资源是**线性的**：它们必须被使用恰好一次。编译器跟踪资源在程序中的所有权并拒绝以下代码：

1. **复制资源**：`let b = a;` 其中 `a: resource T` 移动所有权。`a` 不再可用。
2. **丢弃资源**：让资源在没有消费、转移或销毁的情况下超出范围是编译错误。
3. **资源别名**：`&resource T` 引用是受限生命周期的只读借用。你不能通过借用提取资源。

```CellScript
resource Token { amount: u64 }

action bad_example(t: Token) {
    // 错误：Token `t` 从未被消费、转移或销毁。
    // 这是编译错误，不是警告。
}

action good_example(t: Token) {
    destroy t;  // 显式销毁——编译器满意
}
```

### 5.3 能力模型

CellScript 使用三种能力代替 Move 的四种能力：

| 能力 | 含义 | 默认值 |
|---|---|---|
| `store` | 可以作为 Cell 持久化在 CellStateTree 中 | `resource`、`shared` 默认为是 |
| `transfer` | 可以更改所有者（锁定脚本） | `resource` 默认为是 |
| `destroy` | 可以显式燃烧 | 必须声明 |

能力在资源类型上声明：

```
resource Token has store, transfer, destroy {
    amount: u64,
}

resource SoulBound has store {
    // 没有 transfer，没有 destroy——永久绑定到创建者
    identity: Hash,
}
```

为什么不是 Move 的能力（`key`、`store`、`copy`、`drop`）？
- `copy` 不存在。资源永远不可复制。句号。
- `drop` 不作为隐式能力存在。销毁必须通过 `destroy` 能力显式进行。
- `key` 被 `store` 取代。所有存储的资源都由 OutPoint 索引，而不是由单独的键索引。

### 5.4 Post-v1 模板，而不是核心泛型

CellScript v1 的可执行源码不支持用户自定义泛型类型参数。这是刻意收窄的语言边界：CellScript 是 Cell 生命周期语言，进入 verifier 的持久化 schema 应当是具体、可审计、可被 metadata 稳定寻址的。

下面的写法不是 v1 可执行语法：

```
resource Vault<T: store> {
    content: T,
    unlock_at_daa: u64,
}
```

参数化作者体验属于 post-v1 package/codegen/template 层。模板可以生成专门化的 `.cell` 模块，例如 `TokenVault` 或 `NftVault`，但生成后的 CellScript 必须包含具体字段类型、具体生命周期规则，以及 `#[type_id("...")]` 这样的稳定 schema metadata。

实现说明：parser/type checker 会拒绝 `resource Vault<T>` 这类用户泛型定义，也会拒绝 `Vault<Token>` 这类用户自定义泛型实例化。`Vec<T>` 仍然作为局部有界集合 API 和编译器/runtime metadata 使用的受控内建集合记法保留；它不是通用用户泛型类型系统。

### 5.5 对象身份

每个持久对象都有一个身份：它的 `OutPoint` (`tx_hash || index`)。当资源被消费并重新创建时（例如，更新共享状态），OutPoint 发生变化。CellScript 为稳定身份提供 `type_id` 模式：

```
// stable type identity for tooling/schema metadata
#[type_id("spora::registry::Registry:v1")]
shared Registry has store {
    entries: [RegistryEntry; 64],
}
```

当前实现说明：`#[type_id("...")]` 是 type definition 级属性，可用于 `resource` / `shared` / `receipt` / `struct`。编译器会解析它、拒绝同模块重复值、在 IR 中保留，并在 metadata schema v20 中输出 `types[].type_id` 和 `types[].type_id_hash_blake3`。这还不是完整的 CKB type_id lineage verifier；验证 Cell 的 OutPoint 链追溯到创世交易仍属于后续可执行 verifier / transaction-builder 语义。

### 5.6 共享对象表示

共享对象由 `type_hash` 索引：

```
                ┌─────────────────┐
                │  ScriptIndex    │
                │  (RocksDB)      │
                ├─────────────────┤
                │ type_hash →     │
                │   [OutPoint; N] │  // N为编译期已知上限
                └────────┬────────┘
                         │
            ┌────────────┴────────────┐
            ▼                         ▼
    ┌───────────────┐        ┌───────────────┐
    │ CellDep 读取  │        │ CellInput 写入 │
    │ (并发)        │        │ (独占)        │
    └───────────────┘        └───────────────┘
```

读取者将共享 Cell 作为 `CellDep` 包括。写入者消费它并重新创建它。调度器元数据包括 `touches_shared: [Hash; N]`（固定大小数组）以启用冲突检测。

### 5.7 生命周期表示

资源可以具有生命周期状态，编码为状态机：

```
#[lifecycle(Created -> Active -> Settled -> Destroyed)]
resource VestingGrant {
    state: LifecycleState,
    beneficiary: Address,
    amount: u64,
    cliff_daa: u64,
    end_daa: u64,
}
```

生命周期属性生成类型脚本逻辑：
1. 验证状态转换（仅允许前向转换）
2. 强制执行转换条件（例如，`Active -> Settled` 需要 `current_daa >= end_daa`）
3. 防止无效状态（不能从 `Settled` 返回到 `Active`）

`state` 字段存储在 Cell 数据中。类型脚本读取输入 Cell 的状态，读取输出 Cell 的状态，并验证转换是否有效。

### 5.8 错误处理模型

CellScript **严格禁止**异常/try-catch式栈展开语义。这是为了保持控制流的显式性和可审计性。

#### 强制规则

1. **禁止异常**: 没有 `throw`、`try`、`catch`、`finally` 关键字
2. **强制Result**: 所有可能失败的操作必须返回 `Result<T, E>`
3. **禁止unwrap**: `unwrap()`、`expect()`、`unwrap_or()` 在共识代码中**编译错误**
4. **显式处理**: 错误必须通过 `match` 或 `if let` 显式处理
5. **无panic**: `panic!` 或任何隐式panic语义不允许

#### 示例

```cellscript
// 正确：显式错误处理
action safe_divide(a: u64, b: u64) -> Result<u64, MathError> {
    if b == 0 {
        return Err(MathError::DivisionByZero)
    }
    Ok(a / b)
}

// 正确：调用者显式处理Result
action caller() -> Result<Token, Error> {
    // ? 传播操作符：允许，但仅限于显式lowering为match形式
    let result = safe_divide(100, 0)?;
    // 上述?等价于以下显式match：
    // match safe_divide(100, 0) {
    //     Ok(v) => v,
    //     Err(e) => return Err(e.into())
    // }
}

// 错误：unwrap 不允许
action bad_example(opt: Option<u64>) -> u64 {
    opt.unwrap()  // 编译错误：unwrap 不允许在共识代码中
}

// 错误：expect 不允许
action another_bad(opt: Option<u64>) -> u64 {
    opt.expect("must have value")  // 编译错误：expect 不允许
}
```

#### `?` 传播操作符的精确语义

`?` **允许**使用，但必须满足以下条件：

1. **透明lowering**: `expr?` 必须降低为等价的显式match：
   ```cellscript
   match expr {
       Ok(v) => v,
       Err(e) => return Err(e.into())
   }
   ```

2. **无隐式转换**: 如果涉及错误类型转换，必须显式使用`.map_err()`，不得隐式

3. **无效果边界变化**: `?` 不得跨越效果边界（如从纯函数跳到effectful上下文）

4. **静态可预测**: 编译器必须能够在编译期确定所有`?`的展开点

**禁止的`?`用法**:
- 跨越异步边界（CellScript无async，但明确禁止）
- 任何导致非局部控制流的情况

#### 与红线哲学的对齐

- **红线#4** (No exceptions): 完全遵守
- **红线#7** (No syntax sugar that obscures lowered behavior): `unwrap` 隐藏了失败路径，因此禁止；`?`在透明lowering条件下允许
- **红线#10** (Keep control flow statically legible): 显式 `match` 使所有控制路径可见；`?`的展开点编译期确定

---

## 6. 语法提案

### 6.1 设计原则

CellScript 语法遵循以下规则：
- **类似 Rust 的表达式语法**：`let`、`if`、`match`、块表达式
- **类似 Move 的资源语义**：`move`、显式消费/创建
- **Cell 特定概念的原始关键字**：`resource`、`shared`、`action`、`consume`、`create`、`read_ref`
- **语句末尾没有分号**（如 Kotlin/Swift）——减少噪音
- **显式优于隐式**：每个 Cell 生命周期操作在源代码中可见

### 6.2 示例：可替代资产

```cellscript
// fungible_token.cell — 具有铸造、转移、燃烧的最小可替代代币

module spora::fungible_token

/// 可替代代币资源。线性的：必须被消费或转移。
resource Token has store, transfer, destroy {
    amount: u64
    symbol: [u8; 8]
}

/// 权限 Cell — 持有此权限的人可以铸造新代币。
resource MintAuthority has store {
    token_symbol: [u8; 8]
    max_supply: u64
    minted: u64
}

/// 铸造新代币。只有 MintAuthority 持有者可以调用此操作。
action mint(auth: &mut MintAuthority, to: Address, amount: u64) -> Token {
    assert_invariant(auth.minted + amount <= auth.max_supply,
        "exceeds max supply")

    auth.minted = auth.minted + amount

    create Token {
        amount: amount,
        symbol: auth.token_symbol
    } with_lock(to)
}

/// 将代币转移给新所有者。消费输入，创建输出。
action transfer_token(token: Token, to: Address) -> Token {
    consume token
    create Token {
        amount: token.amount,
        symbol: token.symbol
    } with_lock(to)
}

/// 将一个代币分成两部分。
action split(token: Token, split_amount: u64, 
             owner_a: Address, owner_b: Address) -> (Token, Token) {
    assert_invariant(split_amount < token.amount, "split exceeds balance")
    consume token

    let a = create Token {
        amount: split_amount,
        symbol: token.symbol
    } with_lock(owner_a)

    let b = create Token {
        amount: token.amount - split_amount,
        symbol: token.symbol
    } with_lock(owner_b)

    (a, b)
}

/// 将两个相同类型的代币合并为一个。
action merge(a: Token, b: Token, to: Address) -> Token {
    assert_invariant(a.symbol == b.symbol, "symbol mismatch")
    let total = a.amount + b.amount
    consume a
    consume b

    create Token {
        amount: total,
        symbol: a.symbol
    } with_lock(to)
}

/// 燃烧代币。需要 `destroy` 能力。
action burn(token: Token) {
    assert_invariant(token.amount > 0, "cannot burn zero")
    destroy token
}
```

### 6.3 示例：收据对象

```cellscript
// vesting_receipt.cell — 具有时间锁定认领的存款证明

module spora::vesting

use spora::fungible_token::Token

/// 归属收据。证明代币已被存入以进行时间锁定释放。
receipt VestingReceipt has store {
    beneficiary: Address
    amount: u64
    token_symbol: [u8; 8]
    cliff_daa_score: u64    // 在此 DAA 分数之前不能认领
    vesting_end_daa: u64    // 在此 DAA 分数之后完全归属
    deposited_at_daa: u64   // 存款时间
}

/// 通过存入代币创建归属收据。
action deposit_for_vesting(
    token: Token,
    beneficiary: Address,
    cliff_daa: u64,
    vesting_end_daa: u64
) -> VestingReceipt {
    assert_invariant(cliff_daa < vesting_end_daa, "cliff must precede end")
    
    let current_daa = env::current_daa_score()
    
    // 锁定代币（被消费，尚不可认领）
    consume token

    create VestingReceipt {
        beneficiary: beneficiary,
        amount: token.amount,
        token_symbol: token.symbol,
        cliff_daa_score: cliff_daa,
        vesting_end_daa: vesting_end_daa,
        deposited_at_daa: current_daa
    } with_lock(beneficiary)
}

/// 认领已归属的代币。消费收据，创建代币。
action claim(receipt: VestingReceipt) -> Token {
    let current_daa = env::current_daa_score()
    
    assert_invariant(current_daa >= receipt.cliff_daa_score,
        "cliff not reached")
    
    // 计算已归属金额（线性归属）
    let vested = if current_daa >= receipt.vesting_end_daa {
        receipt.amount
    } else {
        let elapsed = current_daa - receipt.cliff_daa_score
        let total_period = receipt.vesting_end_daa - receipt.cliff_daa_score
        receipt.amount * elapsed / total_period
    }

    consume receipt

    create Token {
        amount: vested,
        symbol: receipt.token_symbol
    } with_lock(receipt.beneficiary)
}
```

### 6.4 示例：共享池对象

```cellscript
// amm_pool.cell — 恒定乘积 AMM 池

module spora::amm

use spora::fungible_token::Token

/// LP（流动性提供者）收据 — 流动性提供的证明。
receipt LPReceipt has store {
    pool_id: Hash
    lp_amount: u64
    provider: Address
}

/// 具有恒定乘积不变量（x * y = k）的 AMM 池。
shared Pool has store {
    token_a_symbol: [u8; 8]
    token_b_symbol: [u8; 8]
    reserve_a: u64
    reserve_b: u64
    total_lp: u64
    fee_rate_bps: u16       // 以基点为单位的手续费（例如，30 = 0.3%）
}

/// 用初始流动性播种新池。
action seed_pool(
    token_a: Token,
    token_b: Token,
    fee_rate_bps: u16,
    provider: Address
) -> (Pool, LPReceipt) {
    assert_invariant(token_a.symbol != token_b.symbol, "same token")
    assert_invariant(token_a.amount > 0 && token_b.amount > 0, "zero liquidity")
    assert_invariant(fee_rate_bps <= 10000, "fee too high")
    
    let initial_lp = math::isqrt(token_a.amount * token_b.amount)
    
    consume token_a
    consume token_b

    let pool = create Pool {
        token_a_symbol: token_a.symbol,
        token_b_symbol: token_b.symbol,
        reserve_a: token_a.amount,
        reserve_b: token_b.amount,
        total_lp: initial_lp,
        fee_rate_bps: fee_rate_bps
    }

    let receipt = create LPReceipt {
        pool_id: pool.type_hash(),
        lp_amount: initial_lp,
        provider: provider
    } with_lock(provider)

    (pool, receipt)
}

/// 通过池将代币 A 交换为代币 B。
action swap_a_for_b(pool: &mut Pool, input: Token, min_output: u64, 
                     to: Address) -> Token {
    assert_invariant(input.symbol == pool.token_a_symbol, "wrong input token")
    
    let fee = input.amount * pool.fee_rate_bps as u64 / 10000
    let net_input = input.amount - fee
    
    // 恒定乘积：(reserve_a + net_input) * (reserve_b - output) = reserve_a * reserve_b
    let output = pool.reserve_b * net_input / (pool.reserve_a + net_input)
    
    assert_invariant(output >= min_output, "slippage exceeded")
    assert_invariant(output < pool.reserve_b, "insufficient reserves")
    
    consume input
    
    pool.reserve_a = pool.reserve_a + input.amount
    pool.reserve_b = pool.reserve_b - output

    create Token {
        amount: output,
        symbol: pool.token_b_symbol
    } with_lock(to)
}

/// 向池添加流动性。
action add_liquidity(
    pool: &mut Pool,
    token_a: Token,
    token_b: Token,
    provider: Address
) -> LPReceipt {
    assert_invariant(token_a.symbol == pool.token_a_symbol, "wrong token a")
    assert_invariant(token_b.symbol == pool.token_b_symbol, "wrong token b")
    
    // 计算与贡献成比例的 LP 代币
    let lp_from_a = token_a.amount * pool.total_lp / pool.reserve_a
    let lp_from_b = token_b.amount * pool.total_lp / pool.reserve_b
    let lp_amount = math::min(lp_from_a, lp_from_b)
    
    consume token_a
    consume token_b
    
    pool.reserve_a = pool.reserve_a + token_a.amount
    pool.reserve_b = pool.reserve_b + token_b.amount
    pool.total_lp = pool.total_lp + lp_amount

    create LPReceipt {
        pool_id: pool.type_hash(),
        lp_amount: lp_amount,
        provider: provider
    } with_lock(provider)
}
```

### 6.5 示例：启动操作

```cellscript
// launch.cell — 具有池播种的原子资产启动

module spora::launch

use spora::fungible_token::{Token, MintAuthority}
use spora::amm::{Pool, LPReceipt, seed_pool}

/// 启动具有初始分发和池播种的新代币。
action launch_token(
    symbol: [u8; 8],
    max_supply: u64,
    initial_mint: u64,
    pool_seed_amount: u64,
    pool_paired_token: Token,
    fee_rate_bps: u16,
    creator: Address,
    distribution: [(Address, u64); 4]
) -> (MintAuthority, Pool, LPReceipt) {
    assert_invariant(initial_mint <= max_supply, "initial exceeds max")
    assert_invariant(pool_seed_amount <= initial_mint, "pool seed exceeds mint")
    
    // 计算分发总额
    let dist_total = distribution[0].1 + distribution[1].1 
                   + distribution[2].1 + distribution[3].1
    assert_invariant(dist_total + pool_seed_amount <= initial_mint,
        "allocation exceeds mint")

    // 创建铸造权限
    let auth = create MintAuthority {
        token_symbol: symbol,
        max_supply: max_supply,
        minted: initial_mint
    } with_lock(creator)

    // 创建分发代币
    for (addr, amount) in distribution {
        if amount > 0 {
            create Token {
                amount: amount,
                symbol: symbol
            } with_lock(addr)
        }
    }

    // 创建池种子代币
    let pool_token = create Token {
        amount: pool_seed_amount,
        symbol: symbol
    } with_lock(creator)

    // 原子性地播种池
    let (pool, lp_receipt) = seed_pool(
        pool_token,
        pool_paired_token,
        fee_rate_bps,
        creator
    )

    (auth, pool, lp_receipt)
}
```

### 6.6 示例：结算操作

```cellscript
// settle.cell — 最终化归属和认领收据

module spora::settle

use spora::fungible_token::Token
use spora::vesting::VestingReceipt

/// 批量结算多个归属收据。
/// 
/// 注意：for循环仅限于固定大小数组，编译期展开为显式索引访问
/// receipts: [VestingReceipt; 4] — 数组大小必须在编译期已知
action batch_settle(
    receipts: [VestingReceipt; 4],
    beneficiary: Address
) -> Token {
    let current_daa = env::current_daa_score()
    
    let mut total_amount: u64 = 0

    // for循环仅限于固定大小数组[N]，编译期确定迭代次数N
    // 精确lowering形式：
    //   for receipt in arr { body }
    // 降低为：
    //   { let mut i = 0; while i < N { let receipt = arr[i]; body; i = i + 1 } }
    // 其中N是编译时常量，while循环有界且可预测
    for receipt in receipts {
        assert_invariant(current_daa >= receipt.vesting_end_daa,
            "not fully vested")
        assert_invariant(receipt.beneficiary == beneficiary,
            "wrong beneficiary")
        total_amount = total_amount + receipt.amount
        consume receipt
    }

    settle create Token {
        amount: total_amount,
        symbol: receipts[0].token_symbol
    } with_lock(beneficiary)
}
```

### 6.7 示例：交易局部中间值

```cellscript
// swap_router.cell — 具有短暂中间状态的多跳交换

module spora::router

use spora::fungible_token::Token
use spora::amm::Pool

/// 通过两个池路由交换（A -> B -> C）。
action multi_hop_swap(
    pool_ab: &mut Pool,
    pool_bc: &mut Pool,
    input: Token,
    min_final_output: u64,
    to: Address
) -> Token {
    // 中间代币 B — 交易局部值，不会作为输出提交
    let intermediate: Token = swap_a_for_b(pool_ab, input, 0, to)
    
    // 中间代币仅存在于此交易的范围内。
    // 编译器验证它在操作结束前被消费。
    
    let output = swap_a_for_b(pool_bc, intermediate, min_final_output, to)
    output
}
```

### 6.8 关键语法决策摘要

| 语法元素 | 关键字 | 原理 |
|---|---|---|
| 线性类型 | `resource` | 清楚传达"这是 Cell 支持的资产，不是普通结构体" |
| 共享状态 | `shared` | 为调度器感知标记争用敏感对象 |
| 状态转换 | `action` | 将 Cell 生命周期操作与实用函数区分开 |
| Cell 消费 | `consume` | 显式关键字防止意外的"花费后使用" |
| Cell 创建 | `create` | 镜像 `consume`；使 Cell 生命周期视觉上对称 |
| CellDep 访问 | `read_ref` | 澄清这是非消费性读取 |
| 约束检查 | `assert_invariant` | 比 `assert` 更强——编译器验证所有路径 |
| 交易局部值 | `let` | 本地绑定不会命中 CellStateTree，除非通过 `create` 显式物化 |
| 生命周期属性 | `#[lifecycle(...)]` | 状态机作为元数据，不是语法污染 |
| 所有者分配 | `with_lock(addr)` | 使锁定脚本分配显式 |
| 销毁 | `destroy` | 能力门控；需要 `destroy` 能力 |

---

## 附录 A: 编译器实现详情

### A.1 项目结构

```
cellscript/
├── Cargo.toml              # 项目配置
├── src/
│   ├── main.rs             # CLI 入口
│   ├── lib.rs              # 库入口
│   ├── error/              # 错误处理
│   │   └── mod.rs          # Span, CompileError, ErrorReporter
│   ├── lexer/              # 词法分析
│   │   ├── mod.rs          # Lexer 实现
│   │   └── token.rs        # Token, TokenKind 定义
│   ├── parser/             # 解析器
│   │   └── mod.rs          # Parser, AST 构建
│   ├── ast/                # 抽象语法树
│   │   └── mod.rs          # AST 节点定义
│   ├── types/              # 类型系统
│   │   └── mod.rs          # TypeChecker, 线性检查
│   ├── ir/                 # 中级表示
│   │   └── mod.rs          # Spora IR, IRGenerator
│   └── codegen/            # 代码生成
│       └── mod.rs          # RISC-V 代码生成
└── examples/               # 示例程序
    └── token.cell          # 可替代代币示例
```

### A.2 词法分析器

词法分析器将源代码转换为 token 流，支持：

- **关键字**: `module`, `resource`, `action`, `consume`, `create`, `transfer`, `destroy`, `if`, `for`, `let`, `mut`, 等
- **类型**: `u8`, `u16`, `u32`, `u64`, `u128`, `bool`, `Address`, `Hash`
- **字面量**: 整数、十六进制、字符串、字节字符串
- **运算符**: 算术、比较、逻辑运算符
- **注释**: 单行 `//` 和多行 `/* */`

### A.3 解析器

递归下降解析器，支持：

- 模块声明 (`module`)
- 资源定义 (`resource`, `shared`, `receipt`)
- 结构体定义 (`struct`)
- Action 定义 (`action`)
- Lock 定义 (`lock`)
- 表达式：二元运算、一元运算、函数调用、字段访问、数组索引
- 语句：let、if、for、while、return
- Cell 操作：`create`, `consume`, `transfer`, `destroy`, `claim`, `settle`

### A.4 类型系统

- 原始类型：`u8`, `u16`, `u32`, `u64`, `u128`, `bool`, `Address`, `Hash`
- 复合类型：数组 `[T; N]`、元组 `(T1, T2)`、命名类型
- 引用类型：`&T`, `&mut T`
- 线性类型检查：确保资源被正确使用（消费、转移或销毁）
- 能力系统：`store`, `transfer`, `destroy`

### A.5 Spora IR

中级表示包含：

- **类型定义**: `IrTypeDef` (Resource, Shared, Receipt, Struct)
- **Action**: `IrAction` 包含参数、函数体、效果类别、调度器提示
- **Lock**: `IrLock` 锁定脚本
- **指令**: LoadConst, LoadVar, StoreVar, Binary, Unary, Call, Consume, Create, Transfer, Destroy, Claim, Settle
- **效果类别**: Pure, ReadOnly, Mutating, Creating, Destroying
- **调度器提示**: parallelizable, touches_shared, estimated_cycles

### A.6 代码生成

RISC-V 代码生成器：

- 生成 RISC-V 汇编代码
- 支持 ckbvm 系统调用 (syscall 2071, 2073 等)
- Borsh 序列化/反序列化支持
- 类型描述符生成
- 运行时支持函数

### A.7 标准库

标准库提供以下功能：

**Borsh 序列化/反序列化**：
- `borsh_serialize_u8/u16/u32/u64/u128`
- `borsh_serialize_bool/address/hash`
- `borsh_deserialize_u64`

**ckbvm 系统调用包装器**：
- `syscall_load_tx_hash` (2061)
- `syscall_load_script_hash` (2062)
- `syscall_load_cell` (2071)
- `syscall_load_header` (2072)
- `syscall_load_input` (2073)
- `syscall_load_witness` (2074)
- `syscall_load_script` (2075)
- `syscall_load_cell_by_field` (2081)
- `syscall_load_cell_data` (2092)
- `syscall_current_cycles` (2042)
- `syscall_debug_print` (2177)

**数学函数**：
- `math_min/max` - 最小/最大值
- `math_isqrt` - 整数平方根 (牛顿迭代法)
- `math_abs_diff` - 绝对差值

**哈希函数**：
- `hash_blake3` - BLAKE3 哈希

**环境函数**：
- `env_current_daa_score` - 当前 DAA 分数
- `env_remaining_cycles` - 剩余周期

### A.8 REPL 交互式解释器

REPL 提供交互式 CellScript 编程环境：

**命令**：
- `:quit, :q` - 退出 REPL
- `:help, :h` - 显示帮助
- `:history` - 显示输入历史
- `:clear` - 清除上下文
- `:show ir` - 切换 IR 显示
- `:show asm` - 切换汇编显示
- `:lex <code>` - 词法分析
- `:parse <code>` - 解析为 AST

**使用示例**：
```bash
$ cellc -i
   ____     _       _   _           _   
  / ___|__| | ___ | |_| |__   ___ | |_ 
 | |   / _` |/ _ \| __| '_ \ / _ \| __|
 | |__| (_| | (_) | |_| | | | (_) | |_ 
  \____\__,_|\___/ \__|_| |_|\___/ \__|
        CellScript Interactive Shell
              Version 0.1.0

cellc> let x = 42
cellc> resource Token { amount: u64 }
cellc> action mint() { create Token { amount: 100 } }
```

### A.9 调度器元数据

编译器自动生成调度器元数据 (SchedulerWitness)。当前实现将其暴露在编译元数据的
`actions[].scheduler_witness_borsh_hex` 字段中；`spora-exec` 的 `CellTx`
已有按 `0xCE11` magic/version 放置、发现并解码 CellScript scheduler witness 的低层 helper，
并能在 admission 时拒绝非法 effect/operation/source、越界 Input/CellDep/Output index，以及与可信摘要不一致的 operation/source/index/binding_hash multiset。共识侧 MPE `BlockAccessSummary`
现在会消费 transaction-admitted witness，把 Input/CellDep/Output access 合并进块访问摘要，并把 `touches_shared` 分成 shared read/write 争用域；write/read 和 write/write 会序列化 DAG，read/read 仍可并行。
Mempool validation 和 template prefilter 现在会在接收/选择前拒绝 malformed CellScript scheduler metadata；template policy 的 strict 测试路径也覆盖 missing / mismatched trusted summary。编译元数据可通过 `ActionMetadata::scheduler_witness_bytes()` 输出 witness bytes；`CellTx::push_cellscript_compiled_scheduler_witness(...)` 会把这些 bytes 对具体交易做 admission、写入 witness，并返回 strict policy 使用的 trusted access summary。Mining 的 mempool-entry / candidate snapshot / template selector 已能保存并传递 producer-backed trusted summary，包括可信空 summary；wallet transaction generator 已能把 compiled scheduler witness 附加到最终交易，并在 `PendingTransaction` 上暴露 trusted access summary；focused mining 测试已经证明 producer-returned summary 能通过 sidecar insertion 进入 selector exposure。剩余缺口是 selector-provided builder summary 进入 strict template prefilter 的测试，以及 RPC/外部提交路径是否需要显式携带 trusted summary。`read_ref`、`&mut shared` 参数以及返回值中含 `shared`
类型的组合调用现在会进入
`touches_shared` 推断；它已进入第一条 MPE 调度消费路径，但仍不是完整的 v1 共识声明契约。

```rust
struct SchedulerWitness {
    magic: u16,              // 0xCE11
    version: u8,             // 1
    effect_class: u8,        // 0=Pure, 1=ReadOnly, 2=Mutating, 3=Creating, 4=Destroying
    parallelizable: bool,
    touches_shared_count: u32,
    touches_shared: Vec<Hash>,
    estimated_cycles: u64,
    access_count: u32,
    accesses: Vec<SchedulerAccessWitness>,
}
```

### A.10 模块系统和名称解析

模块系统支持：

**模块声明**：
```cellscript
module my_contract;
```

**导入语句**：
```cellscript
use spora::fungible_token::Token;
use spora::utils::math;
use my_module as mm;  // 别名
```

**符号解析**：
- 本地符号优先
- 导入符号通过完全限定名解析
- 支持循环依赖检测

### A.11 生命周期验证

> ⚠️ **实现状态**: 生命周期验证模块 (`src/lifecycle/`) 已部分集成到主编译路径。当前可信范围包括声明检查、静态 create/reset 检查、生命周期状态/相邻转换元数据，以及完整 fixed-scalar consume-to-create verifier 路径中的状态范围和 `old_state + 1 == new_state` 检查。动态/嵌套/复杂输出转换验证仍未完成。

`#[lifecycle(...)]` 属性验证：

**状态定义**：
```cellscript
#[lifecycle(Created, Active, Settled)]
receipt VestingGrant { ... }
```

**验证规则**（部分已实现，完整运行时覆盖仍在推进）：
- 至少 2 个状态
- 状态名唯一
- 只允许前向转换（Created → Active → Settled）
- 禁止跳过中间状态
- 禁止反向转换

**API**：
- `LifecycleChecker::register_lifecycle()` - 注册生命周期
- `LifecycleChecker::validate_transition()` - 验证状态转换
- `LifecycleChecker::get_lifecycle_info()` - 获取生命周期信息

### A.12 优化器

> ⚠️ **实现状态**: 优化器模块 (`src/optimize/`) 已接入 `opt_level > 0` 主编译链。当前实现是保守 AST 优化：原始 AST 先通过类型/生命周期检查，优化后 AST 再次检查，然后才进入 IR lowering。以下 SSA、内联和更激进的优化 passes 仍是设计目标。

设计中支持的多级优化：

**常量折叠**（已实现受限子集）：
- 编译期计算语法局部的整数字面量、布尔字面量、字符串/字节串相等性表达式
- 支持 `+`, `-`, `*`, `/`, `%`, 比较运算、布尔 `&&` / `||` 的字面量折叠；除零不会被折叠

**代数简化**（已实现保守子集）：
- `x + 0 = x`
- `x * 1 = x`
- 双重否定消除
- 不执行会丢弃非字面量求值的规则，例如 `x * 0 = 0`

**死代码消除/分支折叠**（已实现受限子集）：
- 字面量条件的 `if` statement / `if` expression 分支折叠
- `while false` 删除
- 不删除任意纯表达式语句，不做跨作用域常量传播或内联

**优化级别**（CLI 支持，优化 passes 待完善）：
- `-O0`: 无优化
- `-O1`: 常量折叠
- `-O2`: + 代数简化
- `-O3`: + 死代码消除

### A.13 示例程序

**token.cell**: 可替代代币 (Fungible Token)
- 展示 `resource`、`action`、`lock` 基本用法
- 实现 `mint`、`transfer`、`burn` 操作

**amm_pool.cell**: AMM 流动性池
- 展示 `shared` 状态
- 实现 `add_liquidity`、`remove_liquidity`、`swap`
- 包含数学计算（恒定乘积公式）

**vesting.cell**: 代币归属
- 展示 `receipt` 和 `#[lifecycle]`
- 实现线性归属计算
- 时间条件验证

**launch.cell**: 代币启动
- 展示复杂合约组合
- 集成多个子合约
- 流动性池播种

**nft.cell**: 非同质化代币
- 展示 `receipt` 生命周期
- 实现挂单/出价系统
- 版税机制

**multisig.cell**: 多签钱包
- 展示复杂状态管理
- 实现提案/签名/执行流程
- 阈值验证

**timelock.cell**: 时间锁
- 展示时间相关操作
- 绝对/相对时间锁定
- 紧急释放机制

### A.14 开发工具

**文档生成器 (docgen/)**：
- 从源代码提取文档注释
- 生成 HTML/Markdown/JSON 格式
- 支持类型、函数、字段文档

**代码格式化器 (fmt/)**：
- 自动格式化 CellScript 代码
- 可配置缩进、换行风格
- 支持代码片段格式化

**LSP 服务器 (lsp/)**：
- 代码补全（关键字、类型、符号）
- 跳转到定义
- 悬停提示
- 文档符号
- 诊断信息

**包管理器 (package/)**：
- Cell.toml 清单管理
- 依赖解析（本地、Git、注册表）
- 版本控制（SemVer）
- 循环依赖检测
- 包初始化

**测试框架 (test/)**：
- 单元测试
- 集成测试
- 文档测试
- 属性测试（随机输入）
- 测试报告生成

**Wasm 目标 (wasm/)**：
- WebAssembly 代码生成
- 二进制编码
- 运行时支持
- 浏览器和 Node.js 兼容

**增量编译 (incremental/)**：
- 编译缓存
- 依赖图追踪
- 变更检测
- 并行编译
- 缓存清理和统计

**CLI 子命令 (cli/)**：
- `cellc build` - 构建项目
- `cellc test` - 运行测试
- `cellc doc` - 生成文档
- `cellc fmt` - 格式化代码
- `cellc init` - 初始化项目
- `cellc add/remove` - 管理依赖
- `cellc clean` - 清理构建产物
- `cellc repl` - 交互式解释器
- `cellc check` - 快速检查
- `cellc run` - 运行程序
- `cellc publish` - 发布包
- `cellc install` - 安装包
- `cellc update` - 更新依赖
- `cellc info` - 显示包信息
- `cellc login` - 登录注册表

**集合类型边界政策**：

CellScript v1 核心语言**不允许**以下动态容器进入共识执行路径：
- `Vec<T>` / 动态数组 — 隐藏堆分配，迭代成本不稳定，破坏成本可预测性
- `HashMap<K, V>` / `HashSet<T>` — 隐藏hash语义，可能隐式重排，分配不透明
- 任何需要运行时堆分配的数据结构
- `sort` 等算法成本不稳定的操作

**允许的有界形式**（仅允许这些）：
- `[T; N]` — 固定大小数组，N必须是编译时常量，成本完全可预测
- `Option<T>` — 可选值，但**禁止**`unwrap()`/`expect()`；必须显式`match`处理
- `Result<T, E>` — 显式错误处理，**禁止**`unwrap()`/`unwrap_or()`/`expect()`

**可变大小数据的正确建模**：
如需表达"列表"或"映射"语义，应使用以下显式模式之一：
1. **多Cell模式**: 每个元素作为一个独立Cell，通过共享索引Cell管理
2. **固定数组+显式长度**: `[T; MAX]`配合显式`len`字段，超出部分截断或拒绝
3. **Merkle承诺模式**: 数据在链下，链上仅存储Merkle根和证明验证逻辑

**编译器内部实现**（不构成语言语义承诺）:
编译器实现可使用常规工程数据结构（Vec/HashMap等），但这些：
- 是编译器内部实现细节
- 不构成对用户代码的语言级保证
- 不得泄漏为共识执行路径的用户可见抽象

**链下工具专用**（不得进入ckbvm执行）:
动态容器**允许**用于：
- 链下SDK/交易构建工具代码
- 测试框架和模拟环境
- LSP、文档生成器等开发工具

这两类的区别：
- 编译器内部：需要保证正确性，但使用工程常规手段
- 链下工具：仅需工程实用性，不触及共识

**v1后研究方向**（明确不属于当前核心）：
- `BoundedVec<T, const N: usize>` — 编译期已知上限的受限向量
- 显式arena分配器的受限形式
- 这些需要单独的设计提案和安全性论证，不得在当前文档中暗示为已承诺功能

**调试信息 (debug/)**：
- DWARF 调试信息生成
- 行号表
- 类型表
- 变量表
- 源码级调试支持

---

## 7. 编译模型

### 7.1 编译器管道

```
源代码 (.cell)
    │
    ▼
┌─────────────┐
│  词法分析器  │  将源代码标记化为 CellScript 标记流
└──────┬──────┘
       │
       ▼
┌─────────────┐
│  解析器      │  生成 AST（模块、资源、操作、表达式）
└──────┬──────┘
       │
       ▼
┌─────────────┐
│  名称        │  解析模块导入、类型引用、操作调用
│  解析        │
└──────┬──────┘
       │
       ▼
┌──────────────────┐
│  类型检查器      │  验证类型、强制线性、检查能力、
│  + 线性          │  验证生命周期转换、检查 assert_invariant
│    检查器        │  完整性
└──────┬───────────┘
       │
       ▼
┌─────────────┐
│  Spora IR   │  降级到具有显式 Cell 操作的中级表示
│  发出       │  (consume_set, create_set 等)
└──────┬──────┘
       │
       ▼
┌─────────────┐
│  优化器      │  死代码消除、常量折叠、内联小函数、
│             │  合并冗余系统调用
└──────┬──────┘
       │
       ▼
┌─────────────┐
│  RISC-V     │  将 Spora IR 降级到 RISC-V 机器代码，链接 CellScript
│  代码生成   │  标准库，发出 ELF 二进制文件
└──────┬──────┘
       │
       ├──── 锁定脚本 ELF（授权逻辑）
       ├──── 类型脚本 ELF（状态转换验证）
       ├──── 类型化数据布局（Cell 数据的 Borsh 模式）
       └──── 调度器元数据（见证编码的提示）
```

### 7.2 Spora IR

Spora IR 是一个中级表示，在降级到 RISC-V 之前抽象地描述 Cell 操作。每个操作编译为具有显式 Cell 生命周期操作的 IR 函数：

```rust
// Spora IR（概念性类似 Rust 的伪代码）

struct SporaIR {
    /// 要消费的 Cell（成为输入）
    /// 注：Vec是编译器内部IR表示，用户代码中不允许
    consume_set: Vec<CellPattern>,

    /// 要读取而不消费的 Cell（成为 CellDeps）
    /// 注：Vec是编译器内部IR表示，用户代码中不允许
    read_refs: Vec<CellPattern>,

    /// 要创建的新 Cell（成为输出 + outputs_data）
    /// 注：Vec是编译器内部IR表示，用户代码中不允许
    create_set: Vec<(CellOutputPattern, DataLayout)>,

    /// 用于调度器的效果分类
    effect_class: EffectClass,

    /// 此脚本验证的生命周期转换规则
    /// 注：Vec是编译器内部IR表示，用户代码中不允许
    lifecycle_rules: Vec<StateTransition>,

    /// 作为见证元数据发出的调度器提示
    scheduler_hints: SchedulerHints,

    /// 实际计算（基本块、SSA 形式）
    /// 注：Vec是编译器内部IR表示，用户代码中不允许
    body: Vec<BasicBlock>,
}

enum EffectClass {
    /// 没有 Cell 读取或写入（纯计算）
    Pure,
    /// 仅通过 CellDep 读取 Cell
    ReadOnly,
    /// 读取和写入 Cell（消费 + 创建）
    Mutating,
    /// 仅创建新 Cell（不消费输入）
    Creating,
    /// 仅消费 Cell（没有新输出）
    Destroying,
}

struct SchedulerHints {
    /// 此操作可以与其他操作并行运行吗？
    parallelizable: bool,
    /// 接触的共享对象的 type_hashes
    /// 注：Vec是编译器内部IR表示，实际见证格式为固定大小数组
    touches_shared: Vec<[u8; 32]>,
    /// 估计的周期成本（用于模板构建器优先级排序）
    estimated_cycles: u64,
}

struct StateTransition {
    from: LifecycleState,
    to: LifecycleState,
    condition: TransitionCondition,
}
```

重要约束：
- Spora IR 必须**不**假设所有 DSL 生成的 Cell 都有通用的强制对象头。
- 类型化布局仍然是类型脚本的主要属性加上编译器生成的模式。
- 如果语言后来为 `shared`、`receipt` 或 `settle` 等特定高级模式标准化头，该头必须是**可选且特定于模式的**，而不是强加于每个 Cell 的协议范围前缀。
- 接触信息应该默认**由编译器推断**。IR 应该保留：
  - 来自显式 Cell 操作的推断接触集
  - 仅在共享写入或效果域意图需要澄清时的可选开发者注释

这很重要，因为 Spora 应该保留 Cell 模型的核心优势：
- 任意字节布局
- 脚本级自验证
- 没有强加于所有状态的协议级 ORM

编译器可以在标准化布局带来实际价值的地方发出它们，但协议不应该要求"所有 DSL Cell 以固定头字节开头"。

### 7.3 这与其他编译目标的区别

**与 Move 字节码对比**：
- Move 编译为由 Move VM 执行的基于栈的字节码。模块存储在由 `(address, module_name)` 寻址的全局命名空间中。
- CellScript 编译为 RISC-V 机器代码。没有中间字节码。脚本作为 Cell 数据存储，由 `code_hash` 引用。不存在全局模块命名空间——脚本由内容哈希识别。
- Move 的验证器在模块发布时运行。CellScript 的类型检查器在编译时运行。输出的 ELF 已经过验证。

**与 Sway/Fuel 对比**：
- Sway 编译为 FuelVM 字节码（基于寄存器的 VM）。FuelVM 具有原生资产支持但没有 CellDep 或共享状态的概念。
- CellScript 编译为用于 ckbvm 的 RISC-V ELF。编译目标是通用 ISA，不是区块链特定的字节码。这意味着 CellScript 可以在任何 RISC-V 执行环境中运行，不仅仅是 ckbvm。
- Sway 的谓词模型验证花费条件。CellScript 的类型脚本模型验证输入和输出的状态转换，这严格更具表达力。

**与 Solidity/EVM 对比**：
- Solidity 编译为 EVM 字节码（基于栈，256 位字大小）。存储建模为 `(contract_address, slot) -> 256-bit value`。
- CellScript 没有持久存储槽。状态存储在 Cell 中。"更新状态"意味着消费旧 Cell 并创建新 Cell。这与 `SSTORE` 根本不同。
- EVM 没有线性、效果类别或调度器提示的概念。并行化（如果有）必须外部推断。

---

## 8. 运行时 / 执行模型

### 8.1 没有新 VM

CellScript 在现有 ckbvm 之上运行。"运行时"是链接到每个编译的 ELF 二进制文件中的薄标准库。没有单独的运行时进程，没有解释器层，没有字节码 VM。

```
┌─────────────────────────────────────────┐
│              ckbvm (RISC-V)             │
│  ┌───────────────────────────────────┐  │
│  │  CellScript 标准库（链接在内）    │  │
│  │  - Borsh 序列化/反序列化          │  │
│  │  - 系统调用包装器                 │  │
│  │  - 线性运行时检查                 │  │
│  │  - 不变量断言支持                 │  │
│  └───────────────────────────────────┘  │
│  ┌───────────────────────────────────┐  │
│  │  编译的操作逻辑                   │  │
│  │  (RISC-V 机器代码)                │  │
│  └───────────────────────────────────┘  │
└─────────────────────────────────────────┘
```

### 8.2 脚本类型

CellScript 产生两种脚本：

**类型脚本** = CellScript 编译的状态转换验证器
- 为消费的输入和新创建的输出运行
- 验证数据布局转换（旧状态 → 新状态）
- 强制执行生命周期规则
- 强制执行不变量（AMM 恒定乘积、供应上限等）
- 按 `type_hash` 分组——具有相同类型脚本的所有输入和输出在一个组中运行

**锁定脚本** = CellScript 编译的授权逻辑
- 仅为输入运行（证明花费权）
- 验证签名、时间锁、多签条件
- 按 `lock_hash` 分组——具有相同锁定脚本的所有输入在一个组中运行

### 8.3 调度器感知

设计目标是让编译器在指定的见证字段中发出调度器元数据。当前实现已经生成
`scheduler_witness_borsh_hex` metadata sidecar；`spora-exec` 已有 CellTx witness
放置/发现/解码/admission helper；共识 MPE `BlockAccessSummary` 已开始消费 transaction-admitted witness。
当前 `touches_shared` 会覆盖 `read_ref`、`&mut shared` 参数触点，以及返回值中含 `shared`
类型的组合调用。MPE DAG 会把 `Pure` / `ReadOnly` 的 shared touch 视为 shared read，把其它 effect 的 shared touch 视为 shared write；read/read overlap 可并行，write/read 或 write/write overlap 会形成 DAG 依赖。共识 MPE 现在也有 strict trusted-access-set 路径：当交易构建器或编译元数据提供可信 operation/source/index/binding_hash multiset 时，缺失或不匹配会在 merge 前失败。Mempool validation 和 template prefilter 已经消费 admission policy：malformed CellScript scheduler metadata 会在接收/选择前失败，template strict policy fixtures 也覆盖 missing / mismatched trusted summary。低层 producer helper 已能从 compiled metadata witness bytes 生成并附加 witness，同时返回 trusted summary；Mining 的 mempool-entry / candidate snapshot / template selector 已能保存并传递这个 summary；wallet transaction generator 已能把 compiled scheduler witness 附加到最终交易并把 trusted summary 暴露给调用方；focused mining 测试证明 producer-returned summary 能通过 sidecar insertion 进入 selector exposure；focused consensus 测试证明 selector-provided builder summary 会被 strict template prefilter 接收或拒绝。剩余未闭合的是外部提交路径的 trusted summary 认证/传递策略，以及更完整的 producer-backed 恶意元数据测试。
元数据格式：

```
// 用于调度器元数据的 Witness[N]（设计目标：按约定最后一个见证条目）
// 当前代码路径：作为 CompileMetadata.actions[].scheduler_witness_borsh_hex 暴露
// Payload 使用 Borsh 编码：
struct SchedulerWitness {
    magic: u16,                 // 0xCE11
    version: u8,                // 1
    effect_class: u8,           // 0=Pure, 1=ReadOnly, 2=Mutating, 3=Creating, 4=Destroying
    parallelizable: bool,
    touches_shared_count: u32,
    touches_shared: Vec<Hash>,
    estimated_cycles: u64,
    access_count: u32,
    accesses: Vec<SchedulerAccessWitness>,
}
```

源代码级策略：
- `touches` 不应该是冗长的手写清单
- 编译器应该自动推断正常的消费/读取/创建行为
- 仅在调度器相关表面从语法本身不明显时才需要显式 `touches` 语法，特别是：
  - 对 `shared` 对象的写入
  - 效果类别覆盖或消歧
  - 故意缩小或澄清的争用域

因此模型是：
- 默认推断
- 必要时显式

区块模板构建器读取此元数据以：
1. 在将冲突交易包含在区块中之前过滤它们（P2a，已完成）
2. 确定哪些交易可以在区块内并行执行（P1，已完成）
3. 为 MPE 合并集级并行化提供 `BlockAccessSummary` shared read/write 争用域（已开始）

此元数据仍不是完整的 v1 共识声明契约。当前 MPE 路径会先 admission witness 再使用其 shared-touch 争用域；strict 路径还可以在 merge 前对照可信 access-set summary。恶意、缺失或不一致元数据仍必须通过后续 builder-backed mempool/template/adversarial 测试收口。执行层始终执行完整验证，调度信息不能替代 verifier 语义。

这种信任边界是故意的。

CellScript 使用：
- 乐观提示
- 悲观验证

这意味着：
- 诚实节点可以使用元数据来改进调度和模板构建
- 不诚实或不正确的元数据最多只能降低优化质量
- 链仍然依赖完整的 `ckbvm` 执行和现有的 Cell 有效性规则进行最终判断

因此，语言应该避免在 v1 中将 `touches`、`SchedulerHints` 或任何未来的效果元数据转变为共识强制声明契约。这样做需要交叉验证"声明的接触表面"与"实际运行时行为"，这会增加运行时复杂性和共识风险，而在此阶段没有相应的回报。

### 8.4 共享状态争用

当同一块中的多个交易接触同一共享对象（相同的 `type_hash`）时：

1. **模板构建器** (P2a)：通过调度器元数据检测冲突。每块每共享对象最多包括一个写入者。多个读取者可以共存。
2. **区块验证** (P1)：尽可能并行验证交易。接触同一共享对象的交易被序列化。
3. **虚拟处理器** (MPE 未来)：合并集中的多个蓝色区块可能每个都包含对同一共享对象的写入。规范顺序解决此问题：第一个蓝色区块（按 GhostDAG 顺序）获胜，后续冲突写入被跳过。

CellScript 不在语言级别解决争用问题。它使争用可见（通过 `shared` 关键字和调度器元数据），以便执行栈可以高效处理它。

同样重要的是，CellScript 应该保留**降级透明性**：
- `consume` 映射到 `inputs`
- `read_ref` 映射到 `deps`
- `create` 映射到 `outputs + outputs_data`
- 共享写入映射到普通 Cell 上的消费并重新创建模式

开发者必须能够直接检查生成的 CellTx 形状。这不是可选的人体工程学优化。这对于以下方面是必要的：
- 调试容量使用
- 调试 Mass 行为
- 理解状态增长
- 理解调度器冲突
- 使 DSL 对底层协议模型保持诚实

### 8.5 原子执行单元

一个 CellTx = 一个原子执行单元。所有输入被消费，所有输出被创建，所有脚本通过，或整个交易失败。没有部分执行。这是从 Cell 模型继承的，CellScript 没有改变它。

系统级并行性来自同一块中的多个 CellTx（P1）或跨合并集中蓝色区块的多个 CellTx（MPE）。

---

## 9. 标准操作和协议模式

### 9.1 `launch` — 创建新资产类型

**语义保证**：原子性地创建类型脚本 Cell、铸造初始供应、可选地播种池。要么创建所有输出，要么都不创建。

**为什么标准**：代币启动是任何区块链上最常见的第一个操作。使其原子化可防止部分部署状态（类型脚本已部署但没有铸造代币，或代币已铸造但池未播种）。

**实现级别**：**v1 后交易构建器特性**。它是确定性的多 `create` CellTx 模板，不是 v1 核心表达式。当前实现应在可执行表达式位置拒绝 `launch`，直到 builder lowering 存在。

### 9.2 `mint` — 创建新单位

**语义保证**：创建新的资源实例，由类型脚本针对权限 Cell 验证。供应不变量在链上强制执行。

**为什么原生/标准**：铸造操作需要权限验证。标准库提供标准权限模式（MintAuthority 资源），类型脚本可以验证。

**实现级别**：**标准库**。`mint` 操作是 CellScript 标准库中的库函数。类型脚本验证权限 Cell 被消费并重新创建，使用更新的 `minted` 计数器。

### 9.3 `burn` — 销毁单位

**语义保证**：销毁资源实例。`destroy` 能力必须在类型上声明。类型脚本验证销毁被授权。

**实现级别**：**标准库**。标准销毁模式：消费资源 Cell，不创建替换，验证 `destroy` 能力。

### 9.4 `transfer` — 在所有者之间移动资产

**语义保证**：消费具有一个锁定脚本的资源 Cell，创建具有不同锁定脚本的新资源 Cell。数据被保留。`transfer` 能力必须被声明。

**实现级别**：**`consume` + `create` 之上的语言糖**。`transfer token to address` 保留资源字段，只改变输出 lock。它值得保留，因为这是高频操作，并让 verifier 工具能识别 lock 重绑定。

```cellscript
transfer my_token to recipient_address
// 等价于：
// consume my_token
// create Token { ...my_token fields... } with_lock(recipient_address)
```

### 9.5 `seed_pool` — 初始化流动性池模式

**语义保证**：创建具有初始储备的共享池 Cell。返回 LP 收据。恒定乘积不变量在创建时建立。

**实现级别**：**标准库/协议模式，并带编译器可见 metadata**。这不应是语言原语。编译器可以为审计和策略工具暴露结构化池义务，但 AMM 数学属于库、生成 verifier 或交易构建器策略。

### 9.6 `swap` — 通过池交换

**语义保证**：原子性地通过池将一种资产交换为另一种。池的不变量（x·y ≥ k 扣除手续费后）由类型脚本验证。

**实现级别**：**标准库/协议模式**。交换操作是作用于 `shared` 池值的库函数。池类型脚本或生成 verifier 执行不变量检查。

### 9.7 `wrap` / `unwrap` — 原生容量转换

**语义保证**：`wrap` 将原生容量（SAU）转换为包装资产代币。`unwrap` 转换回。包装的总供应等于锁定的容量。

**实现级别**：**🚧 未实现**。这是 v1 后计划的标准库特性。当前标准库不包含 `wrap`/`unwrap` 实现；包装资产模式需通过显式 `create`/`consume` 操作和自定义包装器 Cell 类型手动实现。

### 9.8 `claim` — 消费收据以获取资产

**语义保证**：消费收据 Cell，验证认领条件，产生资产 Cell。收据的类型脚本强制执行单次使用。

**实现级别**：**义务分类语法或 intrinsic**。`claim receipt` 降低为消费 receipt Cell + 验证条件 + 创建输出 Cell。它的价值不是新的 CellTx 原语，而是 `claim-conditions` 义务的 metadata 锚点。

```cellscript
let tokens = claim vesting_receipt
// 编译器验证：receipt.type_script 强制执行单次使用
// 编译器验证：认领条件（DAA 分数 >= cliff）被检查
```

### 9.9 `settle` — 最终化待定状态

**语义保证**：将资源从待定生命周期状态转换为最终状态。消费待定 Cell，产生最终 Cell。

**实现级别**：**义务分类语法或 intrinsic**。`settle` 标记最终化路径，使 metadata 和策略工具能把 settlement 与普通 consume/create 更新区分开。它应保持通用和生命周期导向，不承载具体业务语义。

---

## 10. 对比矩阵

| 维度 | CellScript | Solidity | Move | Sway |
|---|---|---|---|---|
| **执行目标** | RISC-V ELF (ckbvm) | EVM 字节码 | Move 字节码 | FuelVM 字节码 |
| **状态模型** | Cell（类 UTXO，类型化） | 账户 + 存储槽 | 全局存储中的资源 | UTXO + 原生资产 |
| **通用表达力** | 窄域（资产聚焦） | 宽域（图灵完备） | 中等（模块范围） | 中等（谓词感知） |
| **资产表达力** | 原生（资源类型、生命周期、shared 池模式） | 手动（ERC-20 模式） | 原生（资源类型） | 部分（原生资产，无类型脚本） |
| **线性类型** | 是（由编译器 + 能力模型强制执行） | 否 | 是（能力：key/store/copy/drop） | 否 |
| **共享状态** | 显式（`shared` 关键字，CellDep/CellInput） | 隐式（所有存储都是共享的） | 显式（Sui 共享对象） | 否（纯 UTXO） |
| **调度器提示** | 原生（IR 发出、效果类别、见证元数据） | 无（顺序 EVM） | 无 | 部分（谓词） |
| **DAG 感知** | 原生（为 GhostDAG 合并集设计） | 无（单链） | 无（单链或 Narwhal） | 无（单链） |
| **并行化支持** | 原生（效果类别、访问摘要、争用检测） | 无 | 部分（Sui 对象级） | 部分（谓词独立性） |
| **冷启动友好性** | 当前中等；v1 后 launch builder 落地后高（原子部署 + 铸造 + 池） | 低（部署 → 初始化 → 批准 → 添加流动性 = 4+ 笔交易） | 中等（发布模块 → 初始化） | 中等（部署谓词） |
| **开发者人体工程学** | 好（类似 Rust 的语法，窄域） | 高（知名，庞大生态系统） | 好（但新概念，陡峭学习曲线） | 好（类似 Rust，但 Fuel 特定） |
| **生态系统成熟度** | 无（全新） | 庞大 | 增长中 | 小 |
| **重入风险** | 不可能（Cell 模型，无回调） | 高（委托调用、外部调用） | 低（默认无动态分派） | 低（谓词中无回调） |
| **存储成本模型** | 3 维 mass（计算 + 瞬态 + 存储） | Gas（单维） | Gas（单维） | Gas（单维） |
| **时间锁支持** | 原生（since 字段：bit63=相对，bit62=daa） | 手动（block.timestamp 比较） | 手动 | 手动 |
| **适合 Spora** | 完美（为 Cell + DAG + ckbvm + mass 设计） | 差（账户模型，无 DAG） | 部分（资源是，DAG 否，ckbvm 否） | 部分（UTXO 是，DAG 否，FuelVM） |

### 为什么 CellScript 为 Spora 胜出

关键差异化因素是：

1. **Cell 原生**：CellScript 的语义模型 1:1 映射到 Spora 的 CellTx。没有阻抗不匹配。
2. **DAG 感知**：调度器提示由编译器发出，启用 P1/P2a/MPE 优化。
3. **ckbvm 目标**：编译为 RISC-V ELF。不需要新 VM。与现有原始脚本向后兼容。
4. **Mass 感知**：编译器可以在编译时估计 mass 贡献（计算、瞬态、存储），在交易构建之前启用费用估计。

---

## 11. 执行计划

本节用更具观点性的执行计划取代通用编译器路线图。

实际执行时，阶段状态以 [`CELLSCRIPT_EXECUTION_PHASES.md`](./CELLSCRIPT_EXECUTION_PHASES.md) 为准。该文件维护当前 active 阶段、退出门槛、剩余收尾项和 `go on` 推进规则。

关键决策是：

- **实现路径**：使用当前 CellScript 风格的后端模型构建
- **语义目标**：将语言表面引导向 Hypha 模型

实际上：
- 保留 `ckbvm`
- 保留 `CellTx`
- 保留锁定/类型/数据分解
- 保留见证导向执行
- 引入一个窄 DSL，其语义中心是：
  - `resource`
  - `shared`
  - `receipt`
  - `settle`
  - 交易局部计算
  - v1 后 launch builder 模式

这为 Spora 提供了一条现在可实现的途径，而不会永远被困在"更好的原始 CKB 脚本编写"中。

### 11.1 执行原则

不要试图在以下之间选择：
- "永远的纯 CellScript"
- "第一天就完整的 Hypha"

那是错误的岔路。

正确的路径是：

**CellScript 风格编译器架构 + Hypha 风格语义表面**

意思是：
- 编译器和运行时路径保持接近当前 Spora 现实
- 语言和 IR 从一开始就围绕资源/对象/效果语义设计

### 11.2 公开名称 vs 内部框架

建议的命名分割：
- **公开语言名称**：`Hypha`
- **内部实现框架**：CellScript 后端

原因：
- "Hypha" 是更好的长期语义品牌
- "CellScript 后端" 准确描述了实现路径并减少内部混淆

### 11.3 不可协商的约束

v1 执行计划必须保留这些边界：

#### 保持不变

- `ckbvm`
- RISC-V ELF 目标
- 当前系统调用接口
- `CellTx` 信封
- `CellOutput` / `CellInput` / `CellDep`
- 见证验证
- 锁定/类型/数据分解
- 当前状态承诺模型
- 当前调度器 DAG 模型

#### 作为新自定义层引入

- DSL 解析器和编译器
- 特定模式的可选标准化对象布局约定
- 效果清单约定
- Spora IR
- 编译器已知的生命周期规则
- 编译器已知的标准操作和协议模式 metadata

#### 明确推迟

- 新 VM
- 通用合约运行时
- 动态分发生态系统
- 主要为了打动语言人而存在的语言特性
- 隐藏状态接触表面的无限制存储抽象

#### 硬设计规则

- 所有 DSL Cell 没有强制通用对象头
- v1 中调度器提示没有共识强制执行
- 没有模糊 `CellTx.inputs / outputs / deps / witnesses` 的黑盒降级

### 11.4 工作流结构

工作应该分为六个工作流并按此顺序构建。

#### 工作流 A — 语义内核

Slice 16 更新：`claim` / `settle` 的输出关系义务现在会按 verifier 覆盖情况细分；如果 operation-tagged `create_set` 输出字段已被固定字段 verifier 完整检查，则 `claim-output:<T>` / `settle-output:<T>` 标记为 `checked-runtime`，不再作为 unresolved runtime-required 输出义务重复出现。真正的 `claim-conditions:<Receipt>`（见证/签名/时间条件）默认仍保持 `runtime-required`，但带显式 20-byte signer 字段且无额外时间/业务谓词的原生 `claim receipt` 现在可被标为 `checked-runtime`；`settle-finalization:<T>`（最终化/准入语义）只在受限 lifecycle final-state 子条件和同一 settle-created output admission 都被 verifier 覆盖时升级为 `checked-runtime`，没有被泛化弱化。

Slice 17 更新：`&mut shared` 参数现在不只影响调度器元数据。编译器会为可变 shared 参数暴露 `shared-state` 类 verifier obligation，例如 AMM 中 `swap_a_for_b` / `add_liquidity` / `remove_liquidity` 都带有 `shared-mutation:Pool` / `runtime-required`。这表示 Pool 输入到替换输出的状态转换还没有被证明；它现在是显式策略门控项，而不是隐藏在 `touches_shared` 之后的语义缺口。

Slice 18 更新：同样的显式义务现在扩展到非 shared 的可变 Cell 参数。`mint(auth: &mut MintAuthority, ...)` 会暴露 `cell-state` / `mutable-cell:MintAuthority` / `runtime-required`，表示 MintAuthority 的替换输出 cell、权限状态和供应量更新仍需要运行时/交易构造器证明。这样 `launch` / `mint` 的普通 action 模拟路径不会被误报为 v1 后 launch builder 已完成。

Slice 19 更新：IR / metadata 现在增加 `mutate_set`，用于记录可变 Cell 参数的直接字段写入摘要。示例：`mint(auth: &mut MintAuthority, ...)` 暴露 `binding=auth, ty=MintAuthority, fields=[minted]`；AMM 的 `&mut Pool` 路径暴露 `reserve_a`、`reserve_b`、`total_lp` 等被写字段。这只是审计和调度输入，不能替代 consumed input 到 replacement output 的交易级证明；后续还必须定义 replacement output index ABI、type/lock identity 保留和字段转换 verifier。

Slice 20 更新：`mutate_set` 现在带有 replacement-output ABI：`input_source/input_index`、`output_source/output_index`、`preserve_type_hash`、`preserve_lock_hash`、`fields`（需要转换证明的字段）、`preserved_fields`（需要等值保留的字段）、`field_equality_status` 和 `field_transition_status`。编译器还把这些绑定暴露为 `ckb_runtime_accesses` 中的 `mutate-input` / `mutate-output` 记录，并进入 scheduler witness 输入。当前状态仍是 `runtime-required`：ABI 已稳定暴露，真正的 TypeHash/LockHash 和字段 transition verifier 将在后续 slice 中执行化。

Slice 21 更新：`mutate_set` 的 replacement-output ABI 现在开始进入可执行 verifier 路径。对要求保留身份的可变 Cell，生成的 RISC-V assembly 会通过 `LOAD_CELL_BY_FIELD` 分别加载 Input 和 Output 的 `TypeHash` / `LockHash`，精确检查长度为 32 字节，并逐字节比较。metadata schema v10 当时新增 `type_hash_preservation_status` / `lock_hash_preservation_status`，当前已随 Pool primitive 结构化元数据推进到 schema v14。字段等值保留（`preserved_fields`）和 transition 字段公式仍保持 `runtime-required`，没有被误报为已完成。

Slice 22 更新：`preserved_fields` 中固定宽度、能放入当前 verifier scratch buffer 的字段现在也进入可执行等值检查。生成器会加载 replacement Input / Output 的完整 cell bytes，检查 schema 固定大小，对每个 preserved field 做 bounds check 和逐字节比较。当前示例中 `MintAuthority.max_supply` / `token_symbol` 以及 AMM `Pool` 的 `fee_rate_bps`、`token_a_symbol`、`token_b_symbol`、`total_lp` 等保留字段会报告 `field_equality_status=checked-runtime`。真正的 transition 字段公式，例如 `MintAuthority.minted = old + amount` 和 Pool 储备量更新，仍保持 `runtime-required`。

Slice 23 更新：第一类 transition 字段公式已经可执行化。IR 会记录简单的 `field = field + operand`、`field = field - operand` 和 `field += operand` 形式；codegen 会加载 replacement Input / Output cell bytes，并验证 `new_field == old_field +/- operand`。当前落地范围覆盖 `token.cell` 中 `MintAuthority.minted = old + amount`，因此 `mint` 的 `mutable-cell:MintAuthority` obligation 现在是 `checked-runtime`。AMM Pool 的储备量公式涉及更多中间值、除法和非参数 operand，仍保持 `runtime-required`。

Slice 24 更新：transition 字段公式的 operand 覆盖面扩展到可由 verifier 重新加载的 schema-backed 参数字段。AMM 中 `input.amount`、`token_a.amount`、`token_b.amount`、`receipt.lp_amount` 这类参数 Cell 字段现在可以作为 `old +/- operand` 的 delta 被检查，因此 `swap_a_for_b` 的 `reserve_a`、`add_liquidity` 的 `reserve_a/reserve_b`、`remove_liquidity` 的 `total_lp` 都有 executable transition check。AMM Pool 的 `field_transition_status` 现在是 `checked-partial`：已经覆盖的字段不会再被混同为完全 runtime-required，但依赖计算局部值的 `output`、`lp_amount`、`amount_a`、`amount_b` 仍等待下一步 prelude 公式重算。

Slice 25 更新：AMM 控制样例中的计算局部值也已进入 verifier prelude 重算路径。编译器会把可证明的 u64 表达式传播为 transition operand，当前覆盖 add/sub/mul/div 和 `min(...)`，所以 `output`、`lp_amount`、`amount_a`、`amount_b` 不再从 action body 栈值中取信，而是由 verifier 从 schema-backed 输入字段重新计算。`swap_a_for_b`、`add_liquidity`、`remove_liquidity` 的 `Pool` replacement 现在都报告 `field_transition_status=checked-runtime`，普通 `shared-mutation:Pool` 字段转换义务也变为 `checked-runtime`。这仍不等于 池语言原语完成；池不变量、准入规则、调度器/交易构造器层的池特化语义仍需要单独显式化。

Slice 26 更新：Pool 专属语义缺口现在已从普通 `shared-mutation:Pool` 义务中拆出来。metadata 新增 `pool-pattern` 类 runtime-required obligations：`seed_pool` 暴露 `pool-create:Pool`，AMM 的 `swap_a_for_b` / `add_liquidity` / `remove_liquidity` 暴露 `pool-mutation-invariants:Pool`，`launch_token -> seed_pool -> Pool` 的组合路径暴露 `pool-composition:Pool`。因此，普通 Pool replacement 的 TypeHash/LockHash、preserved fields 和 source-level field transitions 可以是 `checked-runtime`，但审计/策略工具仍会明确看到 池模式准入规则、AMM 不变量、LP supply consistency、fee accounting 和 launch/pool composition 语义还没有执行化。

Slice 27 更新：Pool 专属义务现在不只是字符串。metadata schema v11 新增 runtime/action/fn/lock 级 `pool_primitives[]`，每条记录包含 `operation`、`feature`、`ty`、`status`、`source`、`checked_components`、`runtime_required_components`、`source_invariant_count`、可选 `binding` / `callee` / Input/Output index，以及 transition/preserved field 列表。当前 `seed_pool` 的 `pool-create:Pool` 会记录 create source、Output index、source invariant guard 数量和 token-pair/reserve/fee/LP runtime 组件；AMM mutation 会记录 replacement ABI、transition/preserved fields、checked general mutation components 和 reserve/fee/LP/admission runtime 组件；`launch_token` 会记录 `seed_pool` callee 和 launch-pool atomicity 债务。docgen 的 lowering audit 也会输出 Pool Pattern Metadata 表。

Slice 28 更新：Pool pattern metadata 现在进入 schema v12，并新增 `invariant_families[]`：每个 family 记录 `name`、`status` 和 `source`。受控 AMM/launch 样例中的源码 `assert_invariant` CFG guard 会被命名为 checked component，例如 `seed_pool` 的 `token-pair-distinct` / `positive-reserves`，`swap_a_for_b` 的 input-token match、minimum-output 和 reserve-output bounds，`add_liquidity` 的 deposit-token matches，`remove_liquidity` 的 LP receipt pool-id match，以及 `launch_token` 的 mint/seed/distribution cap。与此同时，fee policy、LP supply、constant-product pricing、proportional liquidity/withdrawal accounting、pool admission 和 launch-pool atomicity 仍保持 `runtime-required`，没有被误报为 池模式语义已完成。

Slice 29 更新：`invariant_families[]` 现在不只是审计输出，也进入 CLI policy 面。`cellc build --json`、`cellc check --json` 和 `cellc verify-artifact --json` 会输出 checked/runtime-required Pool invariant family 计数；`--deny-runtime-obligations` 会拒绝 runtime-required Pool invariant families。受控 `seed_pool` 路径中，`positive-reserve-admission` 只有在 `positive-reserves` 源码 guard 和 create-output field verifier 都覆盖时才被提升为 `checked-runtime`。`token-pair-admission`、`fee-policy`、`lp-supply-invariant` 仍是 `runtime-required`。

Slice 30 更新：受控 `seed_pool` 现在新增 `fee_rate_bps <= 10000` 的源码 invariant guard，并在 Pool pattern metadata 中命名为 `fee-bps-bound`。当该 guard 与 create-output field verifier 同时覆盖时，`fee-policy` 会被重分类为 `checked-runtime`，并从 `runtime_required_components` / CLI `--deny-runtime-obligations` 的 Pool invariant family 失败列表中移除。`token-pair-admission` 和 `lp-supply-invariant` 仍保持 `runtime-required`，因此 池模式准入仍未完成。

Slice 31 更新：`seed_pool` 的 LP supply admission 现在有受控可执行覆盖。metadata 会要求 `Pool` create fields 可验证、同一 action 中存在可验证的 `LPReceipt` create fields，并且 `Pool.total_lp` 与 `LPReceipt.lp_amount` 来自同一个固定宽度 verifier source；满足这些条件时，`lp-supply-invariant` 被标记为 `checked-runtime`，source 为 `create-output-field-coupling`，并从 CLI runtime-required Pool family 失败列表中移除。`token-pair-admission` 仍保持 `runtime-required`，因为仅凭符号字段还不足以表达完整资产身份/type-id 语义。

Slice 32 更新：粗粒度 `token-pair-admission` 被拆成两个 family。`token-pair-symbol-admission` 在受控 `seed_pool` 中可被标记为 `checked-runtime`：它要求 `token-pair-distinct` 源码 guard 存在，并且 created `Pool.token_a_symbol` / `Pool.token_b_symbol` 字段都由 verifier 覆盖且来自不同 token symbol source。完整资产身份/type-id 准入当时仍以 `token-pair-identity-admission=runtime-required` 暴露给 CLI policy 和审计工具；Slice 60 已把受控 `seed_pool` 的 Input TypeHash 不等式路径执行化。

Slice 33 更新：Pool pattern metadata 当时进入 schema v13，并新增 `runtime_input_requirements[]`。受控 `seed_pool` 中，`token-pair-identity-admission` 当时仍是 `runtime-required`，但不再只是宽泛的 Pool admission 字符串：它的 invariant family source 改为 `token-input-type-id-abi`，并且 `pool_primitives[]` 明确记录 `Input#0:token_a` 与 `Input#1:token_b` 都需要 `input-type-id-32` ABI。Slice 60 已把这个受控 ABI 路径升级为 executable `LOAD_CELL_BY_FIELD` TypeHash 比较；docgen 的 Pool Pattern Metadata 表仍会输出未覆盖 Pool family 的 runtime input requirements。

Slice 34 更新：Pool pattern metadata 现在进入 schema v14，`runtime_input_requirements[]` 每项新增可选 `field`，可以把运行时 ABI/source 要求指到具体 cell 字段。受控 `swap_a_for_b` 中，`fee-accounting` 和 `constant-product-pricing` 仍是 `runtime-required`，但它们的 source 分别变为 `swap-fee-accounting-abi` 和 `swap-constant-product-abi`，并显式记录 `Input#0:input.amount`、`Input#1:pool.fee_rate_bps`、`Input#1:pool.reserve_a`、`Input#1:pool.reserve_b`、`Output#1:pool.reserve_a`、`Output#1:pool.reserve_b` 等字段来源。这一步只暴露 verifier/交易构造器需要验证的字段 ABI，没有把 AMM fee 或 constant-product 经济语义误标为 checked。

Slice 35 更新：schema v14 的 field-aware `runtime_input_requirements[]` 继续覆盖 AMM add/remove 路径。受控 `add_liquidity` 中，`proportional-liquidity-accounting` 和 `lp-supply-consistency` 仍是 `runtime-required`，但 source 现在分别为 `add-liquidity-proportional-abi` 和 `pool-lp-supply-consistency-abi`，并显式记录 `token_a.amount`、`token_b.amount`、Pool `reserve_a/reserve_b/total_lp` 的 Input/Output 字段，以及创建出的 `LPReceipt.lp_amount`。受控 `remove_liquidity` 中，`proportional-withdrawal-accounting` 和 `lp-supply-consistency` 仍是 `runtime-required`，但 source 现在分别为 `remove-liquidity-proportional-withdrawal-abi` 和 `pool-lp-supply-consistency-abi`，并显式记录 `receipt.lp_amount`、Pool reserve/total_lp Input/Output 字段，以及创建出的两个 Token `amount` 字段。这一步继续只暴露运行时 ABI/source 义务，没有把比例铸造、比例赎回或 LP supply 经济语义误标为 checked。

Slice 36 更新：AMM mutation 的 `reserve-conservation` family 也被收窄到字段级 runtime ABI/source。`swap_a_for_b`、`add_liquidity` 和 `remove_liquidity` 仍把 `reserve-conservation` 保持为 `runtime-required`，但 invariant family source 现在是 `pool-reserve-conservation-abi`，并记录 Pool `reserve_a` / `reserve_b` 的 Input/Output 字段。各 action 还会记录对应的金额来源：swap 记录 `input.amount` 与 created Token `amount`，add 记录 `token_a.amount` / `token_b.amount`，remove 记录两个 created Token `amount` 字段。这一步继续只让运行时/交易构造器知道要读取哪些字段，不把 reserve conservation 经济语义误标为 checked。

Slice 37 更新：AMM mutation 的 `pool-specific-admission` family 也被收窄到字段级 runtime ABI/source。`swap_a_for_b`、`add_liquidity` 和 `remove_liquidity` 仍把 Pool admission 语义保持为 `runtime-required`，但 invariant family source 现在是 `pool-specific-admission-abi`。受控 swap 记录 `input.symbol`、Pool `token_a_symbol/token_b_symbol` 和 created Token `symbol`；add 记录 `token_a.symbol`、`token_b.symbol`、Pool token symbols、Pool `type_hash` 和 created `LPReceipt.pool_id`；remove 记录 `receipt.pool_id`、Pool `type_hash`、Pool token symbols 和两个 created Token `symbol` 字段。这一步只暴露运行时/交易构造器要读取的准入字段，没有把池特化 token/type-id admission 语义误标为 checked。

Slice 38 更新：`swap_a_for_b` 剩余的 `lp-supply-consistency` 运行时要求也进入 field-aware metadata。swap 不铸造或销毁 LPReceipt，但 Pool primitive 仍把 LP supply 一致性保持为 `runtime-required`；现在它的 runtime input requirements 明确记录 Pool `total_lp` 的 Input/Output 字段，source 仍是 `pool-lp-supply-consistency-abi`。这一步只暴露 LP supply 检查所需字段，没有把 LP 经济语义误标为 checked。

Slice 39 更新：`launch_token -> seed_pool -> Pool` 组合路径也开始暴露运行时 ABI/source 要求。`callee-pool-admission` 的 source 现在是 `pool-composition-callee-admission-abi`，并记录池种子 created Token 的 `Output#5:type_hash/symbol`、配对 token 参数的 `Param#4:type_hash/symbol` 和 `fee_rate_bps` 参数；`launch-pool-atomicity` 的 source 现在是 `launch-pool-atomicity-abi`，并记录 `initial_mint`、`pool_seed_amount`、`distribution`、created `MintAuthority.minted/token_symbol` 和池种子 Token `amount/symbol`；`pool-id-continuity` 的 source 现在是 `pool-id-continuity-abi`，并记录 tuple `CallReturn#0:Pool.type_hash` 与 `CallReturn#1:LPReceipt.pool_id`。这些 family 仍保持 `runtime-required`，这一步只把组合语义需要读取的 `Param` / `Output` / `CallReturn` ABI 显式化，没有把 launch builder 或 pool-pattern 组合语义误标为 checked。

Slice 40 更新：Pool runtime input requirements 现在进入更直接的报告/策略面。`cellc build --json`、`cellc check --json` 和 `cellc verify-artifact --json` 会输出 `pool_runtime_input_requirements` 计数和 `pool_runtime_input_requirement_summaries`；`--deny-runtime-obligations` 除了列出 runtime-required Pool invariant families，也会列出对应的 runtime input requirement 摘要。docgen 的 Markdown/HTML lowering audit 现在新增独立的 `Pool Runtime Input Requirements` 表，docgen JSON 也新增扁平化 `pool_runtime_input_requirements` 数组。Slice 40 没有把任何 Pool family 升级为 checked；它只把 Slice 33-39 收集到的 ABI 债务接到审计和 policy 输出上。下一步第一个可执行候选被限定为 `launch-pool-atomicity` 的局部字段耦合：先证明 `Param#2 initial_mint -> Output#0 MintAuthority.minted`、`Param#3 pool_seed_amount -> Output#5 Token.amount` 和 symbol 一致性，仍不关闭完整 launch/pool 原子性 family。

Slice 41 更新：`launch-pool-atomicity` 的第一批字段耦合已经作为 checked subcomponents 暴露。受控 `launch_token` 组合 primitive 现在会在 `checked_components` 中报告 `launch-pool-atomicity:minted-equals-initial-mint=checked-runtime`、`launch-pool-atomicity:seed-token-amount=checked-runtime` 和 `launch-pool-atomicity:symbol-consistency=checked-runtime`，条件是对应 create output fields 已被 verifier 覆盖且与参数 source 一致。完整 `launch-pool-atomicity` invariant family 仍保持 `runtime-required`，因为 distribution 总量耦合、callee seed_pool admission、池实例身份连续性和交易构造器原子性还没有全部执行化。

Slice 42 更新：`launch-pool-atomicity` 的 distribution allocation coupling 也进入 checked subcomponents。编译器现在会追踪受控 `launch_token` IR 中固定 tuple-array 参数 `distribution[i].1` 的全量求和来源，并确认该求和加 `pool_seed_amount` 后通过 `<= initial_mint` 的 runtime 分支检查；满足这些条件时，Pool composition metadata 会报告 `launch-pool-atomicity:distribution-sum-plus-seed-lte-initial-mint=checked-runtime`。完整 `launch-pool-atomicity` family 仍保持 `runtime-required`，因为 callee Pool admission、Pool/LPReceipt identity continuity 和交易构造器原子性还没有全部执行化。

Slice 43 更新：`launch_token -> seed_pool` 的 callee Pool admission 也开始拆出可证明 handoff 子组件。编译器现在检查 direct call 实参是否把最后创建的池种子 Token 传给 `seed_pool` 的 token_a，把 `pool_paired_token` 参数传给 token_b，并把 `fee_rate_bps` 参数传给 callee 的 fee 参数；在 seed token `symbol` 已由 create-output verifier 证明、paired token `symbol` 字段布局可由 schema 参数 ABI 覆盖、fee 参数是 verifier-coverable `u16` 时，metadata 会报告 `callee-pool-admission:seed-token-symbol-handoff=checked-runtime`、`callee-pool-admission:paired-token-symbol-handoff=checked-runtime` 和 `callee-pool-admission:fee-bound-handoff=checked-runtime`。完整 `callee-pool-admission` family 仍保持 `runtime-required`，因为 token type-id/asset identity admission 还没有执行化。

Slice 58 更新：`claim` 授权路径现在有一个受限的可执行签名验证约定。若 receipt 暴露固定 `[u8; 20]` 字段 `signer_pubkey_hash`、`claim_pubkey_hash`、`owner_pubkey_hash`、`beneficiary_pubkey_hash` 或 `pubkey_hash`，codegen 会在消费该 receipt 后检查字段 bounds，复用已验证的 65/66 字节 witness envelope 和 `LOAD_ECDSA_SIGNATURE_HASH` canonical sighash，并调用 `SECP256K1_VERIFY` syscall `3002`。metadata 会把该路径的 `claim-witness-signature` 与 `claim-signer-key-binding` 标为 `checked-runtime`；对没有额外时间/业务谓词的原生 `claim receipt`，`claim-conditions:<Receipt>` 顶层也会标为 `checked-runtime`，并只暴露 checked witness/signature runtime input requirements。若同样带 signer 字段的 claim action 还包含源级 `assert_invariant` / DAA 谓词，编译器不会把顶层 `claim-conditions:<Receipt>` 升级为 checked，而是保留 `runtime-required` 并追加 `source-predicate=runtime-required`，同时保留签名/密钥绑定子条件 checked。没有这种 20 字节 signer 字段的 receipt，例如当前 `VestingGrant`，仍保持 `claim-witness-signature=runtime-required` 和 `witness-verification-gap`，避免把 generalized claim 授权误报为完成。

Slice 59 更新：`settle` 最终态路径现在有一个受限的可执行生命周期约定。若被 settle 的类型有 lifecycle metadata 且暴露 fixed-scalar `state` 字段，codegen 会在 settle-created output verification 中检查 consumed Input 和 created Output 的 `state` 都等于最后一个 lifecycle 状态索引，并在 metadata 中把该路径的 `settle-final-state-context` 标为 `checked-runtime`。非 lifecycle 类型或无法由 fixed-field verifier 覆盖的 settle 仍保持 `finalization-policy-gap`，避免把 generalized settle finalization 误报为完成。

Slice 64 更新：受限 lifecycle `settle-finalization:<T>` 顶层分类现在也收敛了。若同一个 `settle` 的 final-state 检查已由 fixed-scalar `state` verifier 覆盖，且 settle-created output relation/admission 也已由 operation-tagged output verifier 覆盖，metadata 会把 `settle-finalization:<T>` 本身标为 `checked-runtime`，并继续输出 checked `settle-final-state-context` 与 `settle-output-admission` runtime input requirements。非 lifecycle、缺少固定 `state` 字段、或输出 relation 不完整的 settle 仍保持 `runtime-required` / `finalization-policy-gap`。

Slice 60 更新：受控 `seed_pool` 的 `token-pair-identity-admission` 现在有可执行 verifier 路径。codegen 会用 `LOAD_CELL_BY_FIELD Source::Input field=5` 分别加载 `Input#0:token_a` 和 `Input#1:token_b` 的 TypeHash，精确检查 32 字节长度，并拒绝二者完全相等的 token pair。metadata 把该 family 标为 `checked-runtime`，source 为 `input-type-id-abi+load-cell-by-field`，并从 Pool runtime input requirement 摘要中移除旧的 `token-input-type-id-abi` runtime-required 条目。更广义的 Pool admission、swap/add/remove 经济不变量和 launch-pool composition 原子性仍保持 runtime-required，避免把 pool-pattern 误报为完整语言原语。

Slice 61 更新：受控 `launch_token -> seed_pool` tuple 返回路径现在有真实的 return-register ABI 支撑。IR 新增 tuple aggregate 指令，callee 返回 tuple 时把字段放入 `a0..a7`，caller 的 tuple field projection 从对应返回寄存器落栈。metadata 在 `Pool` 与 `LPReceipt` 返回字段都被投影且 `LPReceipt.pool_id` 是固定 32 字节字段时，将 `pool-id-continuity` family 标为 `checked-runtime`，source 为 `callee-output-field-coupling+tuple-return-abi`，并移除旧的 `CallReturn#0:Pool.type_hash`、`CallReturn#1:LPReceipt.pool_id` 和 `CallReturnPair#0` equality runtime input requirement。完整一等 `launch` builder、广义 Pool admission 和 AMM 经济不变量仍未关闭。

Slice 62 更新：受控 `swap_a_for_b` 的 `lp-supply-consistency` 现在从 field-aware runtime requirement 收敛为 `checked-runtime`。该 action 不创建或销毁 LPReceipt，且 `Pool.total_lp` 已在 `mutate_set.preserved_fields` 中通过 Input/Output fixed-width preserved-field equality verifier 覆盖；metadata 因此把 `lp-supply-consistency` source 标为 `mutate-preserved-field-equality`，并移除旧的 Pool `total_lp` Input/Output runtime input requirement。`add_liquidity` / `remove_liquidity` 的 LP 供应变化、fee accounting、constant-product、proportional-liquidity/withdrawal、reserve conservation 和 pool-specific admission 仍保持 runtime-required，避免把完整 AMM 经济语义误报为完成。

Slice 63 更新：schema v18 为 Pool `invariant_families[]` 和 Pool `runtime_input_requirements[]` 增加可选 `blocker` / `blocker_class`。剩余 runtime-required Pool family 现在按稳定类别暴露：generalized Pool admission 为 `phase2-deferred-pool-admission`，fee policy/accounting 为 `phase2-deferred-pool-fee-policy`，LP supply 为 `phase2-deferred-lp-supply-policy`，AMM reserve/pricing/liquidity/withdrawal 分别为 `phase2-deferred-amm-reserve-conservation`、`phase2-deferred-amm-pricing`、`phase2-deferred-amm-liquidity-accounting`、`phase2-deferred-amm-withdrawal-accounting`，launch/pool atomicity 为 `phase2-deferred-launch-atomicity`，generalized pool-id continuity 为 `phase2-deferred-pool-id-continuity`。CLI JSON、`--deny-runtime-obligations` 诊断和 docgen Markdown/HTML 都会显示这些 blocker class。Slice 63 没有把 AMM 经济语义或 launch builder 误标为 checked；它把 Phase 2 不关闭的 generalized Pool/launch 语义变成 policy-visible 的稳定边界。

Slice 64 更新：非 Pool 的 mutable state 缺口现在也进入 transaction runtime input blocker 分类。`shared-state` / `cell-state` obligation 中如果 `field transition` 或 `field equality` 不是 `checked-runtime`，metadata 会分别暴露 `mutate-field-transition` / `mutate-field-equality` requirement，并给出 `state-transition-formula-gap` / `state-field-equality-gap` blocker class。这样 `u128` 等当前 verifier 不能覆盖的状态转换不会被误报完成，同时 `--json`、`--deny-runtime-obligations` 和审计工具能稳定区分是字段转换公式缺口还是 preserved-field equality 缺口。

Slice 65 更新：带显式 20-byte signer 字段但仍包含源级 `assert_invariant` / DAA 谓词的 guarded claim 现在有独立 blocker surface。编译器继续保留 `claim-conditions:<Receipt>` 顶层 `runtime-required` 和 `source-predicate=runtime-required`，同时额外生成 `transaction_runtime_input_requirements[]` 中的 `claim-source-predicate` 组件，source 为 `Transaction`、field 为 `source-predicate`、ABI 为 `claim-source-predicate-cfg`，blocker class 为 `claim-source-predicate-gap`。这样 CLI/CI 不再只能从 claim 条件长文本里推断未覆盖的源谓词，而可以直接按 blocker class 拒绝或统计该类差距。

Slice 66 更新：受限 `amount: u64` resource split 守恒现在进入 `checked-runtime`。当一个同类型 resource Input 被消费，并创建多个同类型 Output，且其中一个 Output 的 `amount` 是 verifier 可重算的 `input.amount - split_terms`，同时每个扣减项都由一个 sibling created Output 的 `amount` 精确匹配时，metadata 会把 `resource-conservation:<T>` 标为 `checked-runtime`。重复 split 输出、不匹配扣减项、额外字段、丢失扣减项或更广义跨 Cell accounting 仍保持 `runtime-required`，并继续通过 `resource-conservation-proof-gap` 暴露。

Slice 67 更新：`claim-output:<T>` 和 `settle-output:<T>` 输出关系现在进入 `transaction_runtime_input_requirements[]` 的独立 component surface。verifier 覆盖的输出关系会分别生成 checked `claim-output-relation` / `settle-output-relation` 组件；不支持的输出形状仍 fail closed，并通过 `claim-output-relation-gap` / `settle-output-relation-gap` blocker class 暴露。这样 `claim` / `settle` 的输出关系缺口不再只藏在 verifier obligation 文本里，而可以被 CLI/CI 按 component 统计或拒绝。

Slice 68 更新：已覆盖的 `resource-conservation:<T>` 现在也进入 `transaction_runtime_input_requirements[]`。direct field alias、`amount: u64` additive merge、matched amount split 会生成 checked `resource-conservation-proof` 组件；未覆盖的 generalized conservation 继续生成 runtime-required `resource-conservation-proof` 并带 `resource-conservation-proof-gap`。这样 resource 守恒的已覆盖和未覆盖形态共享同一个审计字段，只通过 `status` 和 blocker 区分。

Slice 69 更新：pure helper `fn` 的 runtime 边界收紧。`fn` 现在不仅不能包含 `create` / `consume` / `transfer` / `destroy` / `read_ref` / `claim` / `settle`，也不能调用 `env::*` runtime builtin 或 `type_hash()` Cell identity builtin；这些必须留在 `action` / `lock` / runtime-visible path 中。`Address::zero`、`Hash::zero`、`min` / `max` / `isqrt` 等纯 helper 不受影响。

Slice 70 更新：线性 `let` 绑定现在执行 move 语义。`let moved = token` 会先把原绑定 `token` 标记为已移动，再引入新绑定 `moved`；因此 `let copied = token` 后继续 `transfer token` / `destroy copied` 这类复制同一个 resource 的代码会被类型检查器拒绝。字段读取如 `let amount = token.amount` 仍是非线性标量读取，不会消费整个 Cell 值。

Slice 71 更新：显式分支 `return` 也进入线性所有权合并。`if flag { return token } else { return token }` 现在可通过，因为两个 terminal 分支都移动同一个 resource；如果一个分支直接返回标量、另一个分支消费/返回 resource，类型检查器会报线性状态不一致。这样线性检查不再只覆盖继续执行的分支，也覆盖所有分支都终止的路径。

Slice 72 更新：优化器从孤立原型进入受限主编译链。`src/optimize/` 现在作为 `pub mod optimize` 编译，`opt_level > 0` 时会在原始 AST 通过类型/生命周期检查后执行保守 AST 优化，再对优化后的 AST 重新类型/生命周期检查，然后进入 IR lowering。当前覆盖字面量常量折叠、保守代数简化、字面量 `if` 分支折叠和 `while false` 删除；不会做跨作用域常量传播、纯表达式 DCE、Cell/runtime 操作消除、SSA 优化或内联。

Slice 73 更新：tail-if 返回路径也进入线性所有权合并。`action choose(token: Token, flag: bool) -> Token { if flag { token } else { token } }` 现在可通过，因为两个 tail 分支都移动同一个 resource；`if flag { left } else { right }` 会被拒绝，因为任一执行路径都会留下另一个 resource 未处理。实现上，类型检查器在检查尾部语句前保留 tail base env，对尾部 `if` 的 then/else 分支分别重放检查和 tail move，再用同一套 branch linear-state merge 规则合并。

Slice 74 更新：普通 `if` expression 也使用同一套线性分支合并规则。`let moved = if flag { token } else { token }` 现在可通过，因为两个 expression 分支移动同一个 resource；`let moved = if flag { left } else { right }` 会被拒绝，因为分支只移动了不同 resource。带状态操作的 expression 分支也按路径合并，例如 `if flag { destroy token } else { destroy token }` 可通过，而只在单边 destroy/consume/transfer/claim/settle 会报分支线性状态不一致。实现上，`Expr::If` 的类型推断和 move 标记都改为使用独立分支环境，并且线性名称收集覆盖父环境链，避免 block/branch 子环境漏掉外层 resource 状态。

Slice 75 更新：普通 block expression 现在会把父作用域中已有 resource 的线性状态从块内子环境写回父环境，同时保留块内局部绑定的词法作用域。`{ destroy token }`、`{ token }`、`{ let inner = token; inner }` 这类包在 block 里的状态操作或 move 不再被块作用域吞掉；block-local 线性绑定如果没有被处理或通过尾表达式移出，例如 `{ let out = create Token { ... }; 1 }`，会被类型检查器拒绝。尾部线性值在 block 自身类型推断阶段标记，外层 move 标记不再重新执行整段 block，避免对已经类型检查过的语句二次 consume/destroy。这样 block expression 和 `if` expression 使用同一套父作用域线性状态传播模型。

Slice 76 更新：block expression 的尾部 `if` 语句现在也可以作为值尾表达式参与类型检查、线性合并和 IR lowering。`let moved = { let inner = token; if flag { inner } else { inner } }` 现在可通过，因为两个 block-tail-if 分支移动同一个 block-local resource；`let moved = { if flag { left } else { right } }` 会被拒绝，因为分支只移动不同的父作用域 resource。纯值路径如 `let value = { if flag { 1 } else { 2 } }` 会 lowering 成 then/else 分支写入 join 临时变量，再由外层绑定或 return 使用。这样 block expression 不再只识别尾部 `Stmt::Expr`，也识别带 `else` 的尾部 `Stmt::If`。

Slice 77 更新：`match` expression 现在也进入线性所有权分支合并。每个 arm 会在独立 child env 中类型检查和 move 标记，所有 arm 对父作用域 resource 的最终状态必须一致后才写回父环境。`let moved = match flag { Flag::On => token, Flag::Off => token }` 现在可通过；`Flag::On => left, Flag::Off => right` 会被拒绝，因为不同 arm 只移动不同 resource。状态操作也按同一规则处理，例如所有 arm 都 `destroy token` 可通过，只有单个 arm destroy 会报 `match arms` 线性状态不一致。

Slice 78 更新：`for` / `while` 循环体现在补上了保守线性边界。循环体内创建的 loop-local resource 必须在循环体作用域内被消费、转移、销毁或移出，否则会报未处理线性资源；同时循环体禁止改变父作用域已有 resource 的 ownership 状态，因为循环可能执行零次或多次，不能把一次性的 `consume` / `destroy` / `transfer` 语义安全写回父作用域。`for i in 0..n { let out = create Token { ... }; destroy out }` 可通过；`for i in 0..n { destroy token } destroy token` 和对应 `while` 形态会被拒绝，避免循环子环境吞掉父 resource 状态后产生双用。

Slice 79 更新：线性类型判断现在递归覆盖 tuple / array 聚合。`(Token, Token)` 和 `[Token; N]` 本身会被视为线性值，不能通过 `let pair = (left, right)` 或 `let items = [left, right]` 把 resource 藏进普通局部变量后静默丢弃；tuple destructuring 仍然可用，但每个线性元素都会被显式绑定并继续参与线性检查。同时 wildcard 绑定不能接收线性值，包括 parser 传入的 `_` 名称和 tuple pattern 中的 `_`，因此 `let _ = token` / `let (_, kept) = (left, right)` 会被拒绝。

Slice 80 更新：线性聚合的 field/index 投影现在也 fail-closed。`let first = pair.0` 或 `let first = items[0]` 不能把 `Token` 这类线性元素从 tuple / array 中取出，同时又让父聚合保持可用；在没有 partial move / field ownership 语义前，这会制造重复使用窗口。受支持的路径仍是 tuple destructuring，让每个线性元素显式绑定并由线性检查器继续跟踪。普通标量 tuple field 和数组 index 读取不受影响。

Slice 81 更新：`action` / `fn` / `lock` 的参数名现在进入稳定身份检查。重复参数名会在类型检查阶段报错，`_` 也不能作为 callable 参数名，因为参数会进入 ABI、IR 和 metadata，必须可稳定引用；`_` 只保留给局部 wildcard binding。这样不会再出现参数覆盖或匿名 ABI 参数在后续 lowering 中被误解释的情况。

Slice 82 更新：schema 字段名也进入稳定身份检查。`resource` / `shared` / `receipt` / `struct` 定义中的重复字段名会被拒绝，`_` 也不能作为字段名；字段会进入 layout、IR、metadata 和 verifier field source，不能靠后续 `HashMap` 收集时覆盖前一个字段。这样 `Token { amount: u64, amount: u128 }` 这类定义不会再污染后续布局语义。

Slice 83 更新：局部绑定现在也采用稳定单赋值身份。`let x = ...; let x = ...`、`let (x, x) = ...`、以及 block/loop/branch 子作用域里遮蔽外层可见绑定都会在类型检查阶段失败；callable 参数也走同一条新绑定路径，因此不会通过 `TypeEnv` 或 IR `vars` map 静默覆盖已有名字。内部非线性 `insert` 同时会清理同名旧线性状态，避免 stale linear state 污染后续检查。

Slice 84 更新：field/index 赋值目标现在必须有命名 local/parameter 根。`point().x = 3` 和 `read_ref<Config>().threshold = 2` 这类临时值写入会在类型检查阶段失败，不能因为找不到 `assignment_root_name` 就绕过 mutability、ownership 和 IR 变量身份检查；合法路径仍是先把值绑定到稳定的 `let mut name` 或 `&mut` 参数，再对 `name.field` / `name[index]` 写入。

Slice 85 更新：只读引用根现在不能被字段或索引赋值。`let mut cfg = read_ref<Config>()` 后执行 `cfg.threshold = 2`、或 `let mut view = &point` 后执行 `view.x = 2` 都会失败；`mut` 修饰的是引用变量本身，不会把 `&T` 升级成 `&mut T`。字段/索引写入现在只允许 mutable owned local 或显式 `&mut T` 根。

Slice 86 更新：mutable Cell 参数形态现在收紧。`mut token: Token`、`mut cfg: read_ref Config`、`mut view: &Config`、`mut pool: &mut Pool` 都会在类型检查阶段失败，避免通过前置 `mut` 把 owned Cell、只读 CellDep、只读引用或冗余 mutable 引用伪装成 mutable ABI 参数；Cell 状态写入必须显式使用 `param: &mut T`。同时 owned linear/resource 根即使是 `let mut token = create ...` 也不能直接 `token.amount = ...`，必须走 `&mut T` 状态更新或 consume/create 所有权转换。

Slice 87 更新：本地只读引用别名现在不能根在 linear Cell 值上。`let view = &token` 或 `let amount = &token.amount` 会在类型检查阶段失败，避免非线性 `&T` 局部别名跨越后续 `destroy token` / `consume token` / `transfer token` 存活；短生命周期的直接调用借用如 `helper(&token)` 仍是当前支持形态。这个规则不是完整 borrow checker，而是 Phase 4 中针对线性 Cell 本地别名复制窗口的 fail-closed 收敛。

Slice 88 更新：引用类型的作用域边界进一步收紧。`&T` / `&mut T` / `read_ref T` 仍可作为 callable 参数和短生命周期表达式借用使用，但不能作为 `action` / `fn` 返回类型，也不能进入 `resource` / `shared` / `receipt` / `struct` 字段或 enum payload 字段。原因是 CellScript 目前没有 lifetime 模型，schema storage 也必须是 owned serializable value；让引用跨 callable 边界或进入持久布局会制造无来源的悬垂引用。

Slice 89 更新：本地只读引用别名检查现在覆盖被存储的嵌套结果。`let pair = (&token, 0)`、`let refs = [&token]`、`let view = if flag { &token } else { &token }`，以及等价的 `match` arm 或 block tail 返回值都会被拒绝，不能把根在 linear Cell 上的 `&T` 藏进 tuple / array / branch result 后再 `destroy` / `consume` / `transfer` 原值。普通 `helper(&token)` 仍不受影响，因为 call 参数不是被保存的本地别名。

Slice 90 更新：同一条本地引用别名规则现在也覆盖 assignment RHS。`let mut view = read_ref<Token>(); view = &token`、`pair = (&token, 0)`、`pair.0 = &token` 都会失败，避免先创建一个可变本地容器/引用变量，再通过赋值把 `&linear Cell` 保存进去。短生命周期的 call 参数借用仍保持可用。

目标：
- 在后端工作扩展之前冻结最小语言核心。

范围：
- 定义声明类：
  - `resource`
  - `shared`
  - `receipt`
  - `object`
- 定义原始操作类：
  - `mint`
  - `burn`
  - `transfer`
  - `wrap`
  - `unwrap`
  - `claim`
  - `settle`
- 定义协议模式 metadata：
  - launch builder
  - pool/AMM flow
  - `seed_pool`
  - `swap`
- 定义所有权、线性和生命周期规则
- 定义 `touches` 语法和效果声明语法
- 明确定义 `touches` 是：
  - 默认推断
  - 仅在需要共享写入或效果域清晰时才显式

退出标准：
- 一个简短的语义规范
- 一个规范的 AST 模式
- 一个用于线性/生命周期失败的规范错误模型

#### 工作流 B — Spora IR

目标：
- 创建将源语义与 `ckbvm` 代码生成分离的层。

范围：
- 定义：
  - `consume_set`
  - `read_refs`
  - `write_intents`
  - `mutate_set`（当前实现中的保守 replacement ABI + 字段级 mutation 摘要，尚不等价于完整 `write_intents`）
  - `create_set`
  - `effect_class`
  - `lifecycle_rules`
  - `scheduler_hints`
- 在 IR 中定义对象身份和共享对象版本处理
- 以不需要通用协议级对象头的方式定义这些
- 定义验证器计划表示

退出标准：
- IR 模式版本 `v0`
- 从源示例到 IR JSON 或 Borsh 形式的往返 fixtures
- 共识相关 IR 与建议性调度器元数据之间的清晰区分

#### 工作流 C — 编译器前端

目标：
- 从源代码到经过验证的 IR。

范围：
- 手写解析器
- AST 构建
- 符号解析
- 类型检查
- 线性检查
- 生命周期检查
- `touches` 推断和验证

推荐选择：
- 首选手写递归下降解析器作为编译器解析器，而不是 Tree-sitter
- Tree-sitter 可以稍后添加用于编辑器工具，不作为规范前端

退出标准：
- 将示例程序编译为经过验证的 Spora IR
- 以确定性诊断拒绝无效程序

#### 工作流 D — `ckbvm` 后端

目标：
- 将经过验证的 IR 降级为当前链可以实际执行的工件。

范围：
- 规范类型化 Cell 布局
- 选定模式的可选对象布局模板
- 见证清单编码
- 生成的验证器代码
- 锁定/类型脚本发出
- RISC-V ELF 生成和链接

推荐策略：
- 不要从自定义 SSA 优化器开始
- 从简单的结构化 IR 降级路径和可预测的代码生成开始
- 在 v1 中优先选择正确性和可检查性而不是花哨的后端架构

退出标准：
- 将最小资产模块编译为 ELF
- 使用生成的工件构建有效的 CellTx
- 在当前 `ckbvm` 集成下成功执行

#### 工作流 E — 运行时集成

目标：
- 使编译器输出成为 Spora 执行和调度的第一类参与者。

范围：
- 交易构建器支持
- 见证打包支持
- 对象发现 / 类型化解码
- 效果清单提取
- 调度器元数据提取
- 内存池和模板构建器对效果元数据的消费

重要规则：
- 建议性调度器元数据可以从非共识开始
- 对象有效性、生命周期检查和验证器逻辑保持共识强制执行

退出标准：
- 节点可以摄取和模拟编译器产生的交易
- 内存池可以在不解编译脚本代码的情况下检查效果表面

#### 工作流 F — 工具和 SDK

目标：
- 使系统超越核心协议工程师可用。

范围：
- 格式化程序
- 检查器
- 测试
- CLI 编译器
- Rust SDK
- TypeScript SDK
- 示例包模板

退出标准：
- 外部开发者可以在不阅读原始系统调用文档的情况下编写、编译、构建和提交交易

### 11.5 阶段计划

上述工作流应通过四个阶段交付。

阶段推进规则：

- 维护一个独立阶段表，而不是只维护百分比。
- `go on` 表示继续执行阶段表中第一个未关闭阶段，直到该阶段退出门槛满足或出现真实阻塞。
- 阶段收尾时必须更新阶段表：状态、证据、剩余工作和阻塞项。
- 如果当前阶段在同一轮工作中收尾完成，应自动将下一阶段设为 active，并开始下一阶段第一个可执行事项。
- 只有缺少凭据、外部依赖、破坏性操作或需要产品/协议决策时才暂停等待用户。

### 阶段 0 — 冻结系统契约

目标：
- 定义你实际愿意支持的窄语言。

构建：
- 语义内核
- 标准操作列表
- 对象头格式
- 效果清单格式
- Spora IR 初稿

还不构建：
- 优化器
- LSP
- 可执行源码中的用户自定义泛型
- 高级宏系统

门槛：
- 核心团队可以阅读规范并明确回答 `resource`、`shared`、`receipt`、`transfer`、`destroy`、`claim` 和 `settle` 的含义，并说明为什么 `launch` 和池 flow 不属于 v1 语言核心。

### 阶段 1 — 编译器 MVP

目标：
- 从源代码到经过验证的 IR 到工作 `ckbvm` 工件。

构建：
- 解析器
- AST
- 类型检查器
- 线性检查器
- IR 发出
- 后端代码生成
- 最小标准库

支持的表面：
- `resource`
- `action`
- `consume`
- `create`
- `transfer`
- `mint`
- `burn`
- 类型化负载

里程碑演示：
- 具有铸造、转移和燃烧的可替代资产通过当前 `ckbvm` 端到端执行

### 阶段 2 — 资产生命周期和共享状态核心

当前执行状态（2026-04-19）：Phase 2 和 Phase 3 operational exit gate 已关闭，Phase 4 生产强化已开始。`vesting.cell` 是当前受控目标；`read_ref` 参数调度器可见，schema-backed `Address` / `Hash` / `[u8; N]` 输出字段保存已进入 verifier 覆盖；create 的固定字节常量、`[u8; N<=8]` 参数、32 字节 `Address` / `Hash` 指针+长度参数输出验证已落地；一进一出、同类型、直接字段别名的 resource conservation、单字段 `amount: u64` 资源的多 Input 加法合并，以及单 Input `amount - split_terms` 拆分到多个同类型 Output 且每个扣减项都有 sibling Output 精确匹配的受限拆分，现在都通过 `resource-conservation:<T>` 标为 `checked-runtime`，并生成 checked `resource-conservation-proof` transaction input component；已有 duplicate amount leaf、missing consumed input leaf、duplicate/unmatched split output、extra field 负向测试防止误标；不匹配扣费/净额、额外字段和更广义跨 Cell 守恒仍是 `runtime-required`，并通过 transaction runtime input metadata 暴露 `resource-conservation-proof-gap` blocker class；可覆盖的 transfer 输出关系现在会把 `transfer-output-relation` 标为 `checked-runtime`，不可覆盖的 generalized transfer 输出关系则通过 `transfer-output-relation-gap` blocker class 显式暴露；可覆盖的 claim/settle 输出关系现在也会生成 checked `claim-output-relation` / `settle-output-relation` transaction input component，不可覆盖输出形状通过 `claim-output-relation-gap` / `settle-output-relation-gap` 显式暴露；不可覆盖的 mutable state transition / preserved-field equality 现在通过 `state-transition-formula-gap` / `state-field-equality-gap` blocker class 显式暴露；带显式 20-byte signer 字段且不含额外时间/业务谓词的原生 receipt claim 现在能把 `claim-conditions:<Receipt>` 顶层标为 `checked-runtime`，而没有该 signer ABI 或带额外时间/业务谓词的 generalized claim 仍是 `runtime-required`，其中 signer-backed guarded claim 的源级谓词缺口已通过 `claim-source-predicate-gap` blocker class 单独暴露；受限 lifecycle settle final-state + output admission 现在能把 `settle-finalization:<T>` 顶层标为 `checked-runtime`，而 generalized finalization 仍是 `runtime-required`；固定宽度 aggregate 参数（例如 `[u64; N]`、`[(Address, u64); N]`）现在也有指针+长度 ABI、exact-size/bounds check、静态 foreach 展开、固定索引 lowering 和 tuple field projection；已知 tuple 返回类型的调用现在可通过真实 RISC-V 返回寄存器 ABI 返回并投影 `.0` 到 `.7`；受控 `launch_token -> seed_pool` 的 `pool-id-continuity` 已标为 `checked-runtime`；受控 `swap_a_for_b` 的 LP supply 不变式已通过 preserved `Pool.total_lp` equality 标为 `checked-runtime`；新建 Output 的 `type_hash()` 现在可通过 `LOAD_CELL_BY_FIELD Source::Output field=5` 读取实例 TypeHash，并作为固定字节 verifier source；命名 schema 参数的 `type_hash()` 现在要求可信的 32 字节指针+长度 ABI，而不是把 Pool 实例身份简化为编译期类型名 hash；可证明的 `with_lock(...)` 绑定现在会通过 `LOAD_CELL_BY_FIELD` 读取输出 `LockHash` 并做 32 字节比较；命名 cell-backed `destroy` 现在会通过 `LOAD_CELL_BY_FIELD Source::GroupOutput field=5` 扫描 grouped outputs，区分 `INDEX_OUT_OF_BOUND` 扫描结束和 `ITEM_MISSING` 无 type script，并把 destroy output absence / group boundary 标为 `checked-runtime`。剩余真实差距集中在 post-v1 launch builder、更广义的池特化 admission/经济不变量、launch-pool 原子性，以及 generalized `claim` 授权策略和 `settle` 生命周期/最终化验证；其中 generalized transfer relation、generalized claim/settle output relation、generalized mutable state formulas、generalized claim/settle conditions/finalization 和 generalized resource conservation 已通过 transaction runtime input blocker class 表示，Pool/launch 剩余义务已通过 schema v20 `pool_primitives[]` blocker class 表示。池仍是 shared-state 协议模式，不是 v1 语言原语。Phase 3 已完成 CellTx witness placement helper、Borsh envelope decode/admission、effect/operation/source class 校验、transaction Input/CellDep/Output index bounds 校验、trusted operation/source/index/binding_hash access-set multiset 对比、compiled-metadata producer helper、wallet transaction generator witness 自动附加与 `PendingTransaction` trusted summary 暴露、producer-returned summary 通过 mining sidecar insertion 进入 selector exposure、selector-provided builder summary 的 strict template prefilter 接收/拒绝测试、consensus MPE access-summary consumption、mempool/template admission policy gate、mempool-entry/template selector producer sidecar 保存与传递，以及 malformed/illegal/out-of-bounds/underreported/forged/missing/mismatched/transaction-shape-incompatible witness summary、malformed/missing/mismatched policy metadata 和 selector sidecar 传递的第一批对抗测试；schema v20 还会把 claim witness/signature 这类 runtime-only 访问从 scheduler witness 中过滤出去。Phase 4 当前差距是外部提交路径 trusted summary 认证/传递策略、更完整的调度器/状态转换 adversarial/property 测试，以及 release-grade 格式化/检查/审计门禁。

目标：
- 使语言对真正的 Spora 原生协议有用。

构建：
- `shared`
- `receipt`
- 生命周期转换
- `claim`
- `settle`
- 调度器提示发出
- 池示例的协议模式 metadata

里程碑演示：
- 一个完整的启动流程：
  - 部署资产
  - 铸造供应
  - 播种第一个池
  - 创建归属收据
  - 从收据认领
  - 结算共享状态

### 阶段 3 — 节点/调度器集成

目标：
- 使效果表面对链重要，而不仅仅是对编译器重要。

构建：
- 内存池效果检查
- 调度器提示消费
- 共享写入争用分析
- 效果驱动的模板构建钩子
- IR/效果与顺序执行的等价性测试

里程碑演示：
- 编译器生成的交易参与区块构建，具有调度器可见的冲突域

### 阶段 4 — 生产强化

目标：
- 降低运营和安全风险。

构建：
- 优化器通道
- 广泛的差异测试
- 线性和生命周期的基于属性的测试
- 外部安全审计
- 格式化程序、检查器、文档生成器、SDK 稳定化

里程碑演示：
- 经过审计的编译器工具链，具有稳定的示例包和发布流程

### 11.6 按层的具体交付物

| 层 | v1 中必须构建 | 可以等待 |
|---|---|---|
| 语言 | 解析器、类型检查器、线性、生命周期核心、推断的 `touches` 和可选显式注释 | post-v1 template/codegen 泛型、宏系统 |
| IR | 效果模型、对象模型、调度器提示 | 优化器级 SSA |
| 后端 | 类型化布局降级、见证格式、ELF 输出 | 激进优化通道、全局对象头方案 |
| 运行时 | 清单解析、对象验证、交易构建器集成 | 完整的链上效果 VM |
| 调度器 | 元数据摄取、冲突域支持 | 深度自动跨块调度 |
| 工具 | CLI、测试、示例 | LSP 优化、调试器、文档生成器 |

### 11.7 什么编码在哪里

这种分割必须保持清晰。

| 关注点 | 属于哪里 | 注释 |
|---|---|---|
| 交易线结构 | `CellTx` 信封（保持不变） | 保持不变 |
| 对象负载模式 | 对象模型 + 编译器输出 | Borsh 编码的类型化负载 |
| 对象身份/版本/生命周期 | 类型脚本定义的布局，在有用的地方具有可选标准化头模板 | 不要对所有 Cell 强制通用协议范围的头 |
| 资源线性 | 编译器 | 编译时保证 |
| 生命周期合法性 | 编译器 + 验证器 | 尽可能静态，需要时运行时 |
| 共享写入意图 | 信封元数据 + IR | 调度器可见，但 v1 中不是共识声明契约 |
| 最终有效性 | 生成的验证器 + 现有运行时 | 仍在执行路径上强制执行 |
| 调度器提示 | 编译器输出、建议性见证元数据 | 仅优化，除非以后的协议工作明确提升 |
| mass 估计 | 编译器估计 + 共识权威 | 编译器是近似的，链是最终的 |

### 11.8 推荐的技术选择

推荐：
- 解析器：手写递归下降
- 编码：Borsh
- 用于测试/调试的 IR 格式：JSON 镜像加上规范二进制形式
- 后端：首先简单的自定义降级，除非以后有正当理由，否则没有 LLVM 依赖
- 标准库：薄而显式，具有系统调用包装器和类型化布局助手

不要过度构建：
- 不要从 LLVM 开始
- 不要从 Cranelift 开始，除非你有明确证明它降低风险
- 不要使编译器架构依赖于尚不存在的未来优化器

### 11.9 验收里程碑

执行计划应该按里程碑管理，而不是按"编译器完成百分比"。

#### 里程碑 M1

- 解析和类型检查最小资产模块
- 为 `mint`、`transfer`、`burn` 发出 Spora IR

#### 里程碑 M2

- 生成 `ckbvm` 兼容验证器工件
- 在集成测试内运行编译的代币流程

#### 里程碑 M3

- 支持 `shared`、`receipt` 和生命周期转换
- 编译池和归属示例

#### 里程碑 M4

- 编译器发出调度器可见的效果元数据
- 节点工具可以读取和检查它

#### 里程碑 M5

- 端到端启动/池/认领/结算示例在受控测试环境中工作

### 11.10 风险和缓解措施

| 风险 | 严重性 | 可能性 | 缓解 |
|---|---|---|---|
| 后端成为伪装的新 VM 工作 | 严重 | 中等 | 保持 `ckbvm` 固定，将 DSL 视为仅编译器 + 验证器生成器 |
| 语言表面漂回通用智能合约设计 | 高 | 中等 | 尽早冻结语义内核并拒绝偏离模型的特性 |
| 共享对象语义 underspecified | 严重 | 中等 | 在池/结算工作之前在 IR 中定义版本控制和写入规则 |
| 编译器/运行时边界变得模糊 | 高 | 中等 | 在规范和测试中保持信封/对象/编译器/运行时分割显式 |
| 调度器元数据不准确或不诚实 | 高 | 中等 | 在验证器路径中保持有效性；在元数据和执行对象集之间使用差异检查 |
| 代码大小或周期成本增长过快 | 高 | 中等 | 保持标准库最小，尽早分析生成的 ELF，仅在端到端流程工作后才优化 |

### 11.11 明确的禁止列表

在 v1 中不要做这些：
- 通用合约平台特性
- 任意运行时调用图
- 合约继承
- 作为默认抽象的动态存储集合
- 完整的 Move 能力系统
- 完整的 Sway/Fuel 执行语义
- 仅为了让编译器感觉优雅的协议更改

### 11.12 最终执行建议

如果立即开始执行，最可防御的路径是：

1. 使后端接近当前 Spora 和 `ckbvm`
2. 尽早冻结 Hypha 风格语义
3. 在雄心勃勃的代码生成之前构建 Spora IR
4. 交付一个处理资产启动、收据、池和结算良好的窄编译器
5. 让通用性等待

---

## 12. 示例程序

### 示例 1：具有池播种的 Meme 资产启动

此示例显示完整流程：定义资产、用初始供应启动它、播种 AMM 池、启用交易。

```cellscript
// meme_launch.cell — 启动具有即时 AMM 交易的 meme 代币

module spora::meme

use spora::fungible_token::{Token, MintAuthority}
use spora::amm::{Pool, LPReceipt}

// ─── 资产定义 ───

resource MemeToken has store, transfer, destroy {
    amount: u64
    symbol: [u8; 8]    // 例如，b"DOGE\0\0\0\0"
}

// ─── 启动：创建代币 + 播种池 ───

/// 启动 meme 代币。原子的：要么所有事情发生，要么什么都不发生。
///
/// 流程：
///   1. 创建具有上限供应的 MintAuthority
///   2. 铸造初始供应
///   3. 分割：80% 到池，10% 给创建者，10% 给社区
///   4. 用 80% 的供应 + 配对的 SOL/USDC 播种 AMM 池
///   5. 返回权限 + 池 + LP 收据
///
action launch_meme(
    symbol: [u8; 8],
    paired_token: Token,         // 例如，包装的 SOL
    creator: Address,
    community_addr: Address
) -> (MintAuthority, Pool, LPReceipt) {
    
    let total_supply: u64 = 1_000_000_000_00  // 10 亿代币，2 位小数
    let pool_share: u64   = total_supply * 80 / 100
    let creator_share: u64 = total_supply * 10 / 100
    let community_share: u64 = total_supply - pool_share - creator_share

    // 1. 创建铸造权限（锁定到创建者）
    let auth = create MintAuthority {
        token_symbol: symbol,
        max_supply: total_supply,
        minted: total_supply    // 启动时全部铸造
    } with_lock(creator)

    // 2. 创建分发代币
    create MemeToken {
        amount: creator_share,
        symbol: symbol
    } with_lock(creator)

    create MemeToken {
        amount: community_share,
        symbol: symbol
    } with_lock(community_addr)

    // 3. 创建池种子代币
    let pool_tokens = create MemeToken {
        amount: pool_share,
        symbol: symbol
    } with_lock(creator)

    // 4. 播种 AMM 池（恒定乘积：x * y = k）
    consume pool_tokens
    consume paired_token

    let initial_lp = math::isqrt(pool_share * paired_token.amount)

    let pool = create Pool {
        token_a_symbol: symbol,
        token_b_symbol: paired_token.symbol,
        reserve_a: pool_share,
        reserve_b: paired_token.amount,
        total_lp: initial_lp,
        fee_rate_bps: 30       // 0.3% 手续费
    }

    let lp_receipt = create LPReceipt {
        pool_id: pool.type_hash(),
        lp_amount: initial_lp,
        provider: creator
    } with_lock(creator)

    (auth, pool, lp_receipt)
}

// ─── 交易 ───

/// 使用配对代币通过池购买 meme 代币。
action buy_meme(
    pool: &mut Pool,
    payment: Token,
    min_meme_out: u64,
    buyer: Address
) -> MemeToken {
    assert_invariant(payment.symbol == pool.token_b_symbol, "wrong payment token")
    
    let fee = payment.amount * pool.fee_rate_bps as u64 / 10000
    let net_input = payment.amount - fee
    let output = pool.reserve_a * net_input / (pool.reserve_b + net_input)
    
    assert_invariant(output >= min_meme_out, "slippage exceeded")
    assert_invariant(output < pool.reserve_a, "insufficient pool reserves")
    
    consume payment
    pool.reserve_b = pool.reserve_b + payment.amount
    pool.reserve_a = pool.reserve_a - output

    create MemeToken {
        amount: output,
        symbol: pool.token_a_symbol
    } with_lock(buyer)
}

/// 通过池出售 meme 代币以换取配对代币。
action sell_meme(
    pool: &mut Pool,
    meme: MemeToken,
    min_paired_out: u64,
    seller: Address
) -> Token {
    assert_invariant(meme.symbol == pool.token_a_symbol, "wrong meme token")
    
    let fee = meme.amount * pool.fee_rate_bps as u64 / 10000
    let net_input = meme.amount - fee
    let output = pool.reserve_b * net_input / (pool.reserve_a + net_input)
    
    assert_invariant(output >= min_paired_out, "slippage exceeded")
    assert_invariant(output < pool.reserve_b, "insufficient pool reserves")
    
    consume meme
    pool.reserve_a = pool.reserve_a + meme.amount
    pool.reserve_b = pool.reserve_b - output

    create Token {
        amount: output,
        symbol: pool.token_b_symbol
    } with_lock(seller)
}
```

### 示例 2：归属收据 / 认领流程

此示例显示完整的归属协议：创建归属计划、发行收据、时间锁定认领和最终结算。

```cellscript
// vesting_protocol.cell — 具有悬崖 + 线性释放的员工代币归属

module spora::vesting_protocol

use spora::fungible_token::Token

// ─── 归属类型 ───

/// 归属计划参数，存储在共享配置 Cell 中。
shared VestingConfig has store {
    admin: Address
    token_symbol: [u8; 8]
    cliff_period: u64          // 第一次认领前的 DAA 分数增量
    total_vesting_period: u64  // 完全归属的 DAA 分数增量
    revocable: bool            // 管理员可以撤销未归属的代币
}

/// 归属授予收据。每个员工一个。
#[lifecycle(Granted -> Claimable -> FullyClaimed)]
receipt VestingGrant has store {
    state: u8                  // 0=Granted, 1=Claimable, 2=FullyClaimed
    beneficiary: Address
    total_amount: u64
    claimed_amount: u64
    grant_daa_score: u64       // 授予创建时间
    cliff_daa_score: u64       // 悬崖到达时间
    end_daa_score: u64         // 完全归属时间
    token_symbol: [u8; 8]
}

// ─── 操作 ───

/// 管理员创建归属配置。
action create_vesting_config(
    admin: Address,
    token_symbol: [u8; 8],
    cliff_period: u64,
    total_period: u64,
    revocable: bool
) -> VestingConfig {
    assert_invariant(cliff_period < total_period, "cliff >= total")
    
    create VestingConfig {
        admin: admin,
        token_symbol: token_symbol,
        cliff_period: cliff_period,
        total_vesting_period: total_period,
        revocable: revocable
    } with_lock(admin)
}

/// 管理员向员工授予归属代币。
/// 代币被锁定——员工在悬崖之前无法访问。
action grant_vesting(
    config: read_ref VestingConfig,
    tokens: Token,
    beneficiary: Address
) -> VestingGrant {
    assert_invariant(tokens.symbol == config.token_symbol, "wrong token")
    assert_invariant(tokens.amount > 0, "zero grant")
    
    let now = env::current_daa_score()
    
    consume tokens  // 在归属中锁定代币
    
    create VestingGrant {
        state: 0,  // Granted
        beneficiary: beneficiary,
        total_amount: tokens.amount,
        claimed_amount: 0,
        grant_daa_score: now,
        cliff_daa_score: now + config.cliff_period,
        end_daa_score: now + config.total_vesting_period,
        token_symbol: config.token_symbol
    } with_lock(beneficiary)
}

/// 员工认领已归属的代币。允许部分认领。
action claim_vested(grant: VestingGrant) -> (Token, VestingGrant) {
    let now = env::current_daa_score()
    
    assert_invariant(now >= grant.cliff_daa_score, "cliff not reached")
    assert_invariant(grant.state < 2, "already fully claimed")
    
    // 计算已归属金额（线性插值）
    let vested_total = if now >= grant.end_daa_score {
        grant.total_amount
    } else {
        let elapsed = now - grant.cliff_daa_score
        let period = grant.end_daa_score - grant.cliff_daa_score
        grant.total_amount * elapsed / period
    }
    
    let claimable = vested_total - grant.claimed_amount
    assert_invariant(claimable > 0, "nothing to claim")
    
    consume grant
    
    // 确定新状态
    let new_state: u8 = if vested_total == grant.total_amount { 2 } else { 1 }
    
    let tokens = create Token {
        amount: claimable,
        symbol: grant.token_symbol
    } with_lock(grant.beneficiary)
    
    let updated_grant = create VestingGrant {
        state: new_state,
        beneficiary: grant.beneficiary,
        total_amount: grant.total_amount,
        claimed_amount: grant.claimed_amount + claimable,
        grant_daa_score: grant.grant_daa_score,
        cliff_daa_score: grant.cliff_daa_score,
        end_daa_score: grant.end_daa_score,
        token_symbol: grant.token_symbol
    } with_lock(grant.beneficiary)
    
    (tokens, updated_grant)
}

/// 管理员撤销未归属的代币（如果配置允许）。
action revoke_grant(
    config: read_ref VestingConfig,
    grant: VestingGrant,
    admin: Address
) -> (Token, Token) {
    assert_invariant(config.revocable, "not revocable")
    assert_invariant(grant.state < 2, "already fully claimed")
    
    let now = env::current_daa_score()
    
    // 计算员工已赚取的金额
    let vested = if now >= grant.end_daa_score {
        grant.total_amount
    } else if now >= grant.cliff_daa_score {
        let elapsed = now - grant.cliff_daa_score
        let period = grant.end_daa_score - grant.cliff_daa_score
        grant.total_amount * elapsed / period
    } else {
        0
    }
    
    let unclaimed_vested = vested - grant.claimed_amount
    let unvested = grant.total_amount - vested
    
    consume grant
    
    // 员工获得他们的已归属部分
    let employee_tokens = create Token {
        amount: unclaimed_vested,
        symbol: grant.token_symbol
    } with_lock(grant.beneficiary)
    
    // 管理员恢复未归属部分
    let admin_tokens = create Token {
        amount: unvested,
        symbol: grant.token_symbol
    } with_lock(admin)
    
    (employee_tokens, admin_tokens)
}
```

### 示例 3：共享状态结算流程

此示例显示具有累积状态、最终结算和清理的多方存款池。

```cellscript
// settlement.cell — 具有结算的多方存款池

module spora::settlement

use spora::fungible_token::Token

// ─── 类型 ───

/// 共享存款池。从多方累积存款。
shared DepositPool has store {
    token_symbol: [u8; 8]
    total_deposited: u64
    num_depositors: u32
    settlement_daa: u64          // 结算成为可能的时间
    admin: Address
    is_settled: bool
}

/// 存款收据 — 个人存款证明。
receipt DepositReceipt has store {
    pool_type_hash: Hash
    depositor: Address
    amount: u64
    deposited_at_daa: u64
}

/// 结算记录 — 最终分发证明。
struct SettlementEntry {
    recipient: Address
    amount: u64
    share_bps: u16              // 总份额的基点
}

// ─── 池创建 ───

/// 创建新存款池。
action create_pool(
    token_symbol: [u8; 8],
    settlement_daa: u64,
    admin: Address
) -> DepositPool {
    assert_invariant(settlement_daa > env::current_daa_score(), 
        "settlement must be in future")
    
    create DepositPool {
        token_symbol: token_symbol,
        total_deposited: 0,
        num_depositors: 0,
        settlement_daa: settlement_daa,
        admin: admin,
        is_settled: false
    } with_lock(admin)
}

// ─── 存款 ───

/// 将代币存入池。返回收据。
action deposit(
    pool: &mut DepositPool,
    tokens: Token,
    depositor: Address
) -> DepositReceipt {
    assert_invariant(!pool.is_settled, "pool already settled")
    assert_invariant(tokens.symbol == pool.token_symbol, "wrong token")
    assert_invariant(tokens.amount > 0, "zero deposit")
    assert_invariant(env::current_daa_score() < pool.settlement_daa,
        "deposit window closed")
    
    consume tokens
    
    pool.total_deposited = pool.total_deposited + tokens.amount
    pool.num_depositors = pool.num_depositors + 1

    create DepositReceipt {
        pool_type_hash: pool.type_hash(),
        depositor: depositor,
        amount: tokens.amount,
        deposited_at_daa: env::current_daa_score()
    } with_lock(depositor)
}

// ─── 结算 ───

/// 结算池。管理员在 settlement_daa 之后触发。
/// 这将池标记为已结算并按比例分发代币。
action settle_pool(
    pool: &mut DepositPool,
    receipts: [DepositReceipt; 8]  // 每笔结算交易最多 8 个存款人
) {
    assert_invariant(!pool.is_settled, "already settled")
    assert_invariant(env::current_daa_score() >= pool.settlement_daa,
        "settlement time not reached")
    
    // 计算每个存款人的份额并分发
    let mut distributed: u64 = 0
    
    for receipt in receipts {
        assert_invariant(receipt.pool_type_hash == pool.type_hash(),
            "receipt from wrong pool")
        
        // 按比例分发
        let share = receipt.amount  // 最简单的情况：拿回你存入的
        // 更复杂：share = receipt.amount * pool.total_reward / pool.total_deposited
        
        consume receipt
        
        settle create Token {
            amount: share,
            symbol: pool.token_symbol
        } with_lock(receipt.depositor)
        
        distributed = distributed + share
    }
    
    pool.total_deposited = pool.total_deposited - distributed
    
    if pool.total_deposited == 0 {
        pool.is_settled = true
    }
}

// ─── 紧急提款 ───

/// 结算前紧急提款（放弃任何奖励）。
action emergency_withdraw(
    pool: &mut DepositPool,
    receipt: DepositReceipt
) -> Token {
    assert_invariant(!pool.is_settled, "already settled")
    
    consume receipt
    
    pool.total_deposited = pool.total_deposited - receipt.amount
    pool.num_depositors = pool.num_depositors - 1

    create Token {
        amount: receipt.amount,
        symbol: pool.token_symbol
    } with_lock(receipt.depositor)
}
```

---

## 13. 最终建议

### 13.1 强烈建议：构建 CellScript

应该构建 CellScript。理由是：

1. **Spora 的 Cell 模型对于生态系统增长来说太低层了。** 手写具有原始系统调用的 RISC-V 脚本对核心开发者来说是可行的，但对更广泛的协议设计者社区来说不行。CellScript 在不牺牲 Cell 模型任何功能的情况下弥合了这一差距。

2. **没有现有语言适合。** 如第 1.3 节所分析，Solidity/Move/Sway 每个都需要对 Spora 的执行模型进行根本性修改才能目标。从头设计的 CellScript 避免了适配不合适语言的技术债务。

3. **编译目标已经存在。** ckbvm 正在运行。系统调用接口稳定。RISC-V 工具链成熟。CellScript 只需要生成有效的 ELF 二进制文件，仅此而已。这是一个编译器项目，不是 VM 项目。

4. **DAG 调度需要语言级支持。** MPE 并行化设计需要 `BlockAccessSummary` 和 `BlockExecutionEffect` 元数据。CellScript 的调度器提示自动提供此元数据，加速 MPE 路线图。

### 13.2 名称理由

**CellScript** 因为：
- Cell 是 Spora 的基本状态单元。该语言是关于编程 Cell 行为的。
- "Script" 与 CKB/Bitcoin 传统（锁定脚本、类型脚本）一致。它传达范围：这不是通用语言。
- `.cell` 扩展名干净、独特，不太可能碰撞现有工具。

### 13.3 为什么这个设计强大

- **最小表面积**：CellScript 做一件事（Cell 生命周期管理）并做好。该语言可以在一天内被任何知道 Rust 的人学会。
- **零阻抗不匹配**：每个 CellScript 概念直接映射到 Spora 运行时概念。没有翻译层，没有适配器模式，没有"但底层模型实际上并不那样工作"。
- **编译器强制执行的安全**：线性类型在编译时防止双重花费错误。生命周期属性在编译时防止无效状态转换。这些是原始脚本编程无法提供的保证。
- **与 MPE 向前兼容**：调度器提示系统今天为明天正在构建的并行化模型设计，但它保持建议性。当 MPE 落地时，CellScript 编译的脚本可以受益，而不必将调度声明移入共识信任边界。
- **对 Cell 层透明**：只有当开发者仍然可以看到源代码如何降级为 `inputs`、`outputs`、`deps` 和见证时，该语言才有用。CellScript 保持该映射可检查。
- **低注释负担**：开发者不应该必须手写完整的接触状态清单。编译器推断明显的部分，显式 `touches` 保留给共享写入和模糊情况。

### 13.4 什么应该与当前 Spora 保持兼容

**不要更改**：
- CellTx 信封格式（`ver: 0xC001`、`inputs`、`outputs`、`deps`、`header_deps`、`outputs_data`、`witnesses`）
- ckbvm 执行模型（RISC-V ELF、系统调用编号 2061–2177、加上 Blake3 扩展）
- 脚本模型（`code_hash`、`hash_type`、`args`）
- CellMeta 索引（`lock_hash`、`type_hash`、`data_hash`）
- 容量模型（占用容量计算）
- 3 维 mass 模型（计算 + 瞬态 + 存储）
- 具有域分离的 Blake3 哈希（`spora-cell/txid`、`spora-cell/wtxid`、`spora-cell/sig`、`spora-cell/data`）
- 基于 MuHash 的 CellStateTree 和 cell_commitment
- GhostDAG 共识模型（蓝色/红色/选定父级/合并集）

CellScript 是一个针对现有基础设施的编译器。它在不修改基础的情况下增加能力。

### 13.5 什么应该与 CKB 设计决裂

**故意偏离**：

1. **原始字节级编程**：CKB 鼓励用 C/汇编编写脚本。CellScript 用类型化语言取代这一点。原始脚本保持支持，但不是推荐路径。

2. **手动见证编码**：CKB 脚本逐字节解析见证。CellScript 的编译器自动生成基于 Borsh 的见证编码/解码。标准库处理序列化。

3. **无类型 Cell 数据**：CKB Cell 数据是具有无强制模式的 `Vec<u8>`。CellScript 在编译时强制执行类型化数据布局，并为每个资源类型生成 Borsh 模式。类型脚本验证数据布局转换。

   重要限制：
   CellScript 不应该用强制通用对象头取代这一点。类型化布局应该保持编译器和脚本定义，仅在抽象明显值得的地方具有可选标准化布局模板。

4. **无生命周期感知**：CKB 没有资源生命周期状态的概念。CellScript 添加 `#[lifecycle(...)]` 属性，生成用于状态机强制执行的类型脚本逻辑。

5. **无调度器提示**：CKB 脚本不提供并行执行的元数据。CellScript 的设计目标是在见证字段中发出 `SchedulerWitness` 数据，使区块模板构建器和虚拟处理器能够做出明智的调度决策。当前实现先将该数据作为编译 metadata sidecar 暴露。

   重要信任边界：
   这些提示仅是建议性的。链在 v1 中不能依赖它们进行共识有效性。

6. **Molecule 序列化**：CKB 使用 Molecule 进行链上编码。Spora 已经采用 Borsh（代码大小小 80%）。CellScript 专门生成 Borsh 编码。

### 13.6 CellScript 如何取代 CoBuild / OTX / 交易构建器心智模型

CellScript 不应该被误解为消除交易、见证、签名或协作交易构建的提案。这些功能仍然是必要的。改变的是它们所在的位置以及它们如何暴露。

在旧模型中，应用操作不直接映射到链上状态转换。中间的东西仍然必须：

1. 查询实时 Cell
2. 选择输入
3. 物化输出和找零
4. 解析 deps 和 header_deps
5. 组织输入组
6. 布局见证
7. 估计费用和存储义务
8. 产生可签名消息
9. 跨多个参与者协调部分构建

这就是为什么生态系统增长出定制的 `tx-builder` 层、见证助手、OTX 包、CoBuild 风格协调格式、钱包特定签名流程和脚本特定胶水代码。这些不是假复杂性。它们是对缺乏受祝福的高级协调契约的补偿。

CellScript 的价值是将此协调栈内部化为一个规范管道：

```text
应用意图
 └─ CellScript 编译器
     └─ Cell 计划
         └─ 交易计划
             └─ 见证义务
                 └─ 授权 / 钱包适配器
                     └─ 执行证明 / 签名
                         └─ 链
```

这改变了每个遗留组件的角色：

| 遗留工具关注点 | CellScript 替代 |
|---|---|
| `tx-builder` | 标准 `Cell 计划 -> 交易计划` 规划器阶段 |
| 见证助手 | `见证义务`层 |
| CoBuild 风格协调格式 | 规范 IR 加上标准义务协议 |
| OTX 包 | 部分意图、未解决的义务和可合并的交易计划 |

关键的转变是，见证处理不再主要是直接呈现给应用开发者的字节布局问题。它变成了一个义务问题：

- 哪个参与者必须授权哪个操作
- 哪个授权适配器负责
- 证明是 Schnorr、ECDSA、多签、通行密钥还是其他什么
- 实际正在授权哪些交易字段

钱包和签名者仍然重要。它们仍然处理密钥托管、用户同意、硬件签名、多签协调、通行密钥流程和证明生成。CellScript 不取代授权适配器，它也不移除费用支付者、存储赞助商或验证器面向的数据提供者。它所做的是用标准的内部栈取代碎片化的生态系统协议，为这些组件提供语义清晰的输入，而不是临时的交易骨架和见证约定。

精确地说：CellScript 不会抹去 CoBuild、OTX 或交易构建器背后的底层功能。它消除了它们保持前台开发者负担和碎片化外部协调格式的需要。

### 13.7 什么放在哪里

| 关注点 | 位置 | 原理 |
|---|---|---|
| 交易结构 | CellTx 信封（不变） | CellScript 编译成有效的 CellTx。信封是协议的线格式。 |
| 对象数据布局 | Cell 数据（outputs_data） | CellScript 生成 Borsh 编码的类型化数据。类型脚本验证布局。 |
| 可选对象头模板 | 选定模式的编译器约定 | 对某些 `shared` / `receipt` / `settle` 模式有用，但不是全局强制的 |
| 状态转换规则 | 类型脚本 ELF（编译器输出） | 类型脚本就是编译的 CellScript 操作。它在 ckbvm 中运行。 |
| 授权逻辑 | 锁定脚本 ELF（编译器输出） | 锁定脚本从 CellScript 锁定函数编译。 |
| 调度器元数据 | metadata sidecar（当前实现）；见证字段（设计目标） | 模板构建器 / 虚拟处理器的建议性数据。当前不是共识强制执行的。 |
| 线性强制执行 | 编译器类型检查器 | 编译时保证。生产中线性检查没有运行时成本。 |
| 生命周期强制执行 | 类型脚本逻辑（运行时） | 执行期间类型脚本验证生命周期转换。 |
| mass 估计 | 编译器 + 共识 MassCalculator | 编译器估计 mass；共识层计算权威 mass。 |
| CellStateTree 承诺 | 共识层（不变） | MuHash 累加器、cell_commitment 哈希。CellScript 不触及这一点。 |

### 13.8 结束语

CellScript 不是试图构建"Spora 的 Solidity"。它是试图构建 Spora 架构暗示但尚未拥有的语言。Cell 模型、DAG 共识、RISC-V 执行、3 维 mass——这些是强大、设计良好的基础。缺失的是让协议设计者以资产、池、收据和生命周期的术语思考，而不是系统调用编号、见证字节偏移和原始 ELF 二进制文件的层。

CellScript 填补了这一空白。它编译到 ckbvm 已经在运行的相同 RISC-V。它尊重网络已经在处理的相同 CellTx 信封。它在不需要任何共识级更改的情况下增加安全性（线性、生命周期强制执行）和能力（调度器提示、类型化数据）。

同样重要的是，它应该在不执行以下操作的情况下做到这一点：
- 对所有 Cell 强加通用协议级对象头
- 将调度器声明转变为共识法律
- 模糊源代码构造如何降级为实际 Cell 操作

该语言是故意窄域的。它不渴望成为通用语言。它渴望成为表达 Spora 被构建来执行的操作的最佳可能语言：创建资产、管理其生命周期、确保其完整性、结算其最终状态——所有这些都发生在基于 DAG 并行、基于 Cell、RISC-V 执行的区块链中。

构建它。

---

*文档结束。*
