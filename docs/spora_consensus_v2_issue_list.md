# Spora 共识架构 V2 Issue 清单

- 日期: 2026-04-11
- 状态: Draft
- 主文档: [spora_consensus_architecture_v2.md](/Users/arthur/RustroverProjects/Spora/docs/spora_consensus_architecture_v2.md)
- 差距清单: [spora_consensus_v2_gap_analysis.md](/Users/arthur/RustroverProjects/Spora/docs/spora_consensus_v2_gap_analysis.md)
- 评审摘要: [spora_consensus_v2_rfc_summary.md](/Users/arthur/RustroverProjects/Spora/docs/spora_consensus_v2_rfc_summary.md)
- P2b 设计: [P2B_VIRTUAL_PROCESSOR_PARALLELIZATION_DESIGN.md](/Users/arthur/RustroverProjects/Spora/docs/P2B_VIRTUAL_PROCESSOR_PARALLELIZATION_DESIGN.md)

## 1. 用法

这份文档不是协议正文，而是执行清单。

每一条 issue 都尽量回答五件事：

- 为什么要做
- 改哪些代码
- 完成标准是什么
- 依赖什么前置条件
- 不做会留下什么风险

建议团队按 `P0 -> P1 -> P2` 顺序推进，不要跳着修。

## 2. P0

### V2-P0-01 把 `body_validation_in_context` 接到真实 Cell 状态机

**为什么要做**

V2 要求 `body validation in context` 不只是 isolation 检查，而要共享正式的 Cell context / DAG / script 语义。现在这层还没有真正调用共享状态机。

**主要代码路径**

- `consensus/src/pipeline/body_processor/body_validation_in_context.rs`
- `consensus/src/processes/cell_validator/mod.rs`
- `consensus/src/processes/cell_validator/cell_validation_in_context.rs`
- `consensus/src/processes/cell_validator/cell_validation_in_dag.rs`

**完成标准**

- 非 coinbase 交易在 `validate_body_in_context` 中执行真实 context 校验
- `POV`、maturity、capacity、输入可花费性不再只在后续 virtual/state 路径拒绝
- body 阶段和后续状态阶段对同一交易给出一致结论
- 补充覆盖缺失输入、immature coinbase、重复花费的上下文测试

**依赖**

- 可以先不接 VM，先把 context / DAG 校验接上

**不做的风险**

- 协议仍然不是单一状态机
- “更晚才拒绝”会继续掩盖入口语义分裂

### V2-P0-02 用真实 consensus-backed data provider 替换 `SimpleDataProvider`

**为什么要做**

脚本执行只有在使用真实共识存储作为 data provider 时，才算正式进入共识。当前 `verify_scripts` 仍然是 placeholder。

**主要代码路径**

- `consensus/src/processes/cell_validator/mod.rs`
- `spora_exec` 中 VM verifier 接口对应的数据提供者实现
- 可能新增 `consensus/src/processes/cell_validator/data_provider.rs`

**完成标准**

- `verify_scripts` 不再使用 `SimpleDataProvider::new()`
- data provider 能从共识状态读取输入 Cell、header 上下文、依赖数据
- `validate_full_with_scripts` 可以在主路径安全调用
- 至少补一组 lock / type script 正反例测试

**依赖**

- 建议和 `V2-P0-01` 同步设计接口

**不做的风险**

- Spora 还不能算“CKB-VM 已正式入共识”
- 文档和实现会继续错位

### V2-P0-03 把 mempool admission 切到共享状态机

**为什么要做**

mempool 不是共识，但它不能跑一套和正式验块完全不同的逻辑。当前实现仍是明显 placeholder。

**主要代码路径**

- `consensus/src/pipeline/virtual_processor/processor.rs`
- `consensus/src/consensus/mod.rs`
- `consensus/core/src/api/mod.rs`

**完成标准**

- `validate_mempool_transaction` 使用与正式验块相同的 Cell 可花费性和 capacity 规则
- `validate_mempool_transactions_in_parallel` 不再直接返回全 `Ok`
- `populate_mempool_transaction` 不再是空实现
- mempool 和正式验块在同一 POV 下对交易是否可接受给出一致结论

**依赖**

- 依赖共享状态转移接口先稳定下来

**不做的风险**

- 本地池子会持续接受正式区块不会接受的交易
- template 选择和主共识结论容易漂移

## 3. P1

### V2-P1-01 把 template validation 切到正式状态机

**为什么要做**

template 已经能生成大部分承诺，但交易筛选和 fee 语义仍是简化实现。V2 要求 template 和正式验块共享交易可接受性判断。

**主要代码路径**

- `consensus/src/pipeline/virtual_processor/processor.rs`
- `consensus/src/pipeline/virtual_processor/test_block_builder.rs`

**完成标准**

- `validate_block_template_transaction` 不再是简化 fee 路径
- `validate_block_template_transactions` 不再带 `Simplified - full Cell validation pending`
- template 中交易准入与正式验块使用同一状态转移入口
- 模板构造测试覆盖 commitment、accepted root、fee 结果一致性

**依赖**

- 依赖 `V2-P0-03`

**不做的风险**

- template 虽然能出块，但仍不保证和正式状态机严格同构

### V2-P1-02 正式降级 `get_cell_at_daa` 为非共识接口

**为什么要做**

协议正文已经切到 `POV-aware`，但状态索引、测试和旧文档里还保留大量 `get_cell_at_daa` 痕迹，容易误导后续实现。

**主要代码路径**

- `state/src/index/cell_db.rs`
- `consensus/src/pipeline/virtual_processor/cell_tests.rs`
- `docs/spora_ghostdag_cell_architecture.md`
- `docs/spora_audit_progress.md`
- `docs/spora_audit_session_summary.md`
- `docs/todo_categorization.md`

**完成标准**

- `get_cell_at_daa` 注释明确声明为索引 / 调试接口
- 所有协议文档不再把它写成共识语义
- 测试名称和注释不再暗示它是正式共识视角
- 新代码 review 基准中禁止在共识路径引入 `DAA-only` 历史判断

**依赖**

- 无强依赖，可以和 P0 并行

**不做的风险**

- 团队容易把旧语义重新带回主路径

### V2-P1-03 收敛 `CellValidator` 和 `virtual_processor` 的状态机边界

**为什么要做**

当前“正确逻辑”分散在 `CellValidator` 和 `virtual_processor/cell_processing.rs` 两边。V2 需要一个明确的单一 `State Transition Engine`。

**主要代码路径**

- `consensus/src/processes/cell_validator/mod.rs`
- `consensus/src/pipeline/virtual_processor/cell_processing.rs`
- `consensus/src/pipeline/virtual_processor/processor.rs`

**完成标准**

- 明确哪个模块是唯一状态转移入口
- 去掉重复实现或“一个模块定义规则，另一个模块真正执行”的分裂结构
- 文档和代码命名一致，例如真正落出 `state_transition` 模块或等价入口

**依赖**

- 建议在 `V2-P0-01` 完成后推进

**不做的风险**

- 后续每次改规则都要改两套地方
- 再次引入分层漂移的概率很高

### V2-P1-04 把 legacy 时间锁脚本面彻底迁移到 `ScriptRef + CKB-VM`

**为什么要做**

Cell 模型已经把时间锁语义切到每输入 `since`，而且共识侧 `CellValidator::validate_in_dag` 已经通过 `cell_validation_in_dag::validate_time_locks` 覆盖了四类 Cell-native 约束：

- 绝对 DAA 锁
- 相对 DAA 锁
- 绝对时间戳锁
- 相对时间戳锁

因此，这一条 issue 的重点不是“再发明一次时间锁校验”，也不是“给 CLTV/CSV 打一个 Cell 兼容补丁”，而是把仍暴露在 txscript / SDK / wallet 上层的 legacy 时间锁脚本面彻底迁到 `ScriptRef + CKB-VM`。当前这些 legacy 入口和 Cell 模型不兼容，而且下游仍在使用，属于会误导调用方的真实 P1 风险。

当前 txscript 中两个 legacy 时间锁 opcode 在 Cell 模型下都已实质失效：

- `OpCheckLockTimeVerify`（CLTV）：比较栈上值与 `tx.lock_time()`，但 `CellTx::lock_time()` 恒返回 0。任何 `lock_time > 0` 的脚本检查必败，HTLC 超时退款路径不可达，**资金将永久锁死**。
- `OpCheckSequenceVerify`（CSV）：沿用 legacy `sequence` 语义。对 Cell `since` 来说，bit63 表示“相对锁”，但 CSV 把它当成“disabled bit”，因此相对锁会被立即判错；即便是绝对锁，CSV 也只比较低 32 位，忽略 bit62 模式位和高位值域，语义同样是**错误的**。

此外，依赖这两个 opcode 的标准脚本构建函数（HTLC、时间锁支付）仍在对外暴露，下游调用面包括 wallet generator、WASM SDK、`consensus/client` 包装层和 `treasure_boy`。这意味着风险不是“仓库里留了几段死代码”，而是“公开 API 仍在引导用户生成会失败或锁死资金的脚本”。

主架构文档已经把终态写清楚了：Spora 的规范脚本面应当与 CKB 对齐，即由 `ScriptRef + CKB-VM + since + header_deps` 组成，txscript 只应作为迁移期兼容层，不能继续承载正式时间锁能力。

Cell-native 的替代方向已经存在：`exec/src/scripts/` 中已有通过 CKB-VM 系统调用读取 `since` / header timestamp 的 fixture；后续应将对外时间锁能力统一收敛到“输入 `since` + VM lock script”这一条规范路径。

**主要代码路径**

- `consensus/src/processes/cell_validator/mod.rs`
- `consensus/src/processes/cell_validator/cell_validation_in_dag.rs`
- `crypto/txscript/src/opcodes/mod.rs`（CLTV: L800-L850, CSV: L852-L891）
- `crypto/txscript/src/standard.rs`（`pay_to_pub_key_with_lock_time`, `pay_to_address_with_lock_time_script`, `htlc_script`, `htlc_script_ecdsa`）
- `crypto/txscript/src/script_builder.rs`（`add_lock_time`, `add_sequence`）
- `crypto/txscript/src/wasm/builder.rs`
- `exec/src/celltx/types.rs`（`CellTx::lock_time()` 恒返回 0 的兼容方法）
- `exec/src/scripts/mod.rs`
- `exec/src/vm/`
- `crypto/txscript/src/lib.rs`（`LOCK_TIME_THRESHOLD`, `MAX_TX_IN_SEQUENCE_NUM` 等仅服务旧 opcode 的常量）
- `consensus/client/src/utils.rs`
- `wallet/core/src/tx/generator/generator.rs`
- `wallet/core/src/tx/generator/pending.rs`
- `treasure_boy/src/lib.rs`

**完成标准**

- 文档先明确：Cell 模型的规范时间锁语义是输入 `since`，不是 tx-level `lock_time`
- CLTV / CSV 在 CellTx 上不再“静默沿用 legacy 语义”
- 如果暂时不能删除 opcode，实现至少要改成显式返回“Cell 模型不支持”的确定性错误
- 对外脚本能力的终态明确为 `ScriptRef + CKB-VM`，而不是修补 txscript helper 继续沿用
- `pay_to_pub_key_with_lock_time`、`pay_to_address_with_lock_time_script`、`htlc_script`、`htlc_script_ecdsa` 不再作为可用标准脚本对外暴露
- `consensus/client`、WASM SDK、wallet generator、`treasure_boy` 不再调用上述 legacy helper
- `consensus/client` / wallet / SDK 对时间锁脚本的公开入口改成 Cell-native 方案，必要时直接暴露 `ScriptRef`、code hash、args、`header_deps` 等构造能力
- `ScriptBuilder` 中 `add_lock_time()` / `add_sequence()` 及对应 wasm builder 包装层删除，或降级为仅限 legacy/测试 feature
- `CellTx::lock_time()` 兼容方法删除，或保留为内部桥接但不再被共识/SDK/标准脚本调用
- txscript 中仅服务旧 opcode 的常量与示例清理完成；保留哪些共识常量、哪些 bridge 常量，需要在文档里明确切分
- `exec/src/scripts/` 或等价位置提供清晰的 Cell-native 时间锁示例、脚本包产物和迁移说明
- 增加回归测试，至少覆盖以下事实：
- 现有 `validate_time_locks` 继续覆盖四类 `since` 语义
- 旧 helper 若仍保留，会稳定报错而不是生成“看似成功、实则不可花费”的脚本
- wallet / SDK 不再能构造 CLTV/CSV 风格的 Cell 时间锁输出
- 真实 VM data provider + script execution 能覆盖迁移后的时间锁脚本样例

**依赖**

- 建议与 `V2-P0-02` 并轨推进，因为“彻底迁到新 VM”必须以真实 consensus-backed data provider 为前提
- 依赖对外替代路径先明确，否则直接删除会打断 wallet / SDK / `treasure_boy`
- 建议分两阶段推进：
- 第一阶段先把 legacy helper 改成显式不可用，并补 `ScriptRef + CKB-VM` 迁移文档与样例
- 第二阶段在下游调用清零后删除实现和兼容常量
- `legacy_sequence_to_cell_since()` 作为 legacy 交易输入桥接可暂时保留，但不得再被包装成 Cell-native 时间锁能力

**不做的风险**

- 用户可能构造使用 CLTV 的脚本，导致资金永久锁死
- CSV 对 Cell `since` 的解释错误，导致相对锁直接误判、绝对锁按错误位宽比较
- 标准脚本库、WASM SDK 和 wallet helper 名义可用但语义错误，继续误导开发者和工具作者
- 文档如果只写成“清理旧 opcode”，会掩盖真正的终态要求：Spora 需要和 CKB 对齐，彻底收敛到 `ScriptRef + CKB-VM`
- 如果继续维持两套脚本世界，协议外围会长期卡在“共识用 VM、钱包/SDK 还在 txscript”的分裂状态

## 4. P2

### V2-P2-01 建立 DAG 顺序无关性和 reorg replay 测试集

**为什么要做**

复杂 DAG 协议不能只靠普通单元测试证明正确。V2 明确要求 proof-oriented 测试。

**主要代码路径**

- `consensus/src/pipeline/virtual_processor/tests.rs`
- `consensus/src/pipeline/virtual_processor/cell_tests.rs`
- 可能新增 `consensus/tests/` 下的模型测试

**完成标准**

- 同一 DAG 的不同导入顺序得到相同 `accepted_id_merkle_root`
- split/reorg replay 后得到相同 `cell_root`
- created-then-spent、duplicate spend、red/blue reward 等场景有系统性覆盖

**依赖**

- 依赖 P0/P1 主路径先稳定

**不做的风险**

- “测试通过” 仍然不代表协议在复杂导入顺序下稳定

### V2-P2-02 设计 second implementation / model checker 对拍方案

**为什么要做**

协议要接近成熟，仅靠单实现自测不够。需要一个更小、更可验证的参考实现或模型。

**主要代码路径**

- 不一定先改生产代码
- 可以新增 `tools/`、`scripts/` 或 `research/` 目录下的参考模型

**完成标准**

- 有一个独立于主实现的状态机模型
- 能对拍 `accepted set`、`cell_root`、`commitment`
- 明确哪些输入空间是模型覆盖范围

**依赖**

- 依赖 V2 规范先冻结

**不做的风险**

- 无法有效防止“单实现自洽但协议其实有坑”

### V2-P2-03 评估增量状态树和历史证明扩展

**为什么要做**

当前 `cell_root` 路径能工作，但距离轻客户端证明和高性能实现还有差距。

**主要代码路径**

- `state/src/cell_tree.rs`
- `docs/cell_commitment_evolution.md`

**完成标准**

- 输出一份技术选型结论
- 明确是否引入增量状态树
- 明确 `cell_commitment v1/v2` 是否扩展历史证明或多命名空间承诺

**依赖**

- 建议在协议和主路径稳定后推进

**不做的风险**

- 主网后期会更难演进证明层

## 5. 建议的执行顺序

1. `V2-P0-01`
2. `V2-P0-02`
3. `V2-P0-03`
4. `V2-P1-01`
5. `V2-P1-03`
6. `V2-P1-04`
7. `V2-P1-02`
8. `V2-P2-01`
9. `V2-P2-02`
10. `V2-P2-03`

这个顺序的原则是：

- 先修主共识入口
- 再收口外围入口
- 最后补证明和长期演进层

## 6. 最终目标

这些 issue 全部完成后，Spora V2 才接近下面这个状态：

- 有唯一有效的协议文档
- 有唯一的状态转移引擎
- 有全入口一致的共识语义
- 有脚本执行的正式共识接线
- 有可证明、可重放、可升级的实现基础
