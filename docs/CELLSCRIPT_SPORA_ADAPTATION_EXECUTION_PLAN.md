# CellScript 与 Spora 适配执行方案

Branch: `spora-typed`
日期：2026-05-10
状态：方案形成，CellScript typed-cell profile MVP、Spora compile-metadata acceptance、invoice financing action-builder matrix、base/cellscript/production acceptance profiles 已落地

## 结论

CellScript 已重新作为 submodule 接入 Spora，并切到 0.20-based 独立适配分支。0.20 基线的 `cellscript-ckb-adapter` 已从本地 `../../../ckb-sdk-rust` 路径改为 `ckb-sdk-rust` git tag 依赖，因此 submodule 自身可以独立 `cargo check --workspace`。Spora root workspace 仍暂不直接纳入 CellScript workspace；先保持 submodule 边界，避免把 CellScript 0.20 的 adapter/dev-test 依赖面扩散到 Spora root。

1. Spora integration 的 compile-metadata acceptance 已从旧 `target_profile = "spora"` 迁到 `typed-cell`。
2. CellScript 0.20-based 分支已新增 `TargetProfile::TypedCell` 的 MVP，不恢复旧 `spora` profile。
3. `metadata.constraints.spora` 的硬依赖已从 integration helper 中移除；typed-cell contract 迁移到 action scheduler witness / typed-cell metadata，不做假兼容字段。
4. 双方已经具备可对接的核心桥：CellScript 有 `ActionMetadata::scheduler_witness_bytes()`；Spora runtime 有 `CellTx::push_cellscript_compiled_scheduler_witness()`、trusted summary、`BlockAccessSummary` 和 conflict-hash 调度路径。
5. Spora devnet builder matrix 已按 typed-cell ABI 重新对齐：cell-bound input/output 的 source/index 顺序、read-ref `CellDep#0`、新增 schema 字段（例如 `wallet_id`、`lock_id`、receipt `state`）均改为以 CellScript metadata 为准。

因此执行策略是：先闭合 Spora runtime 消费侧，再在 CellScript 增加 typed-cell profile，最后恢复端到端 acceptance。

## 当前状态

已完成：

| 项目 | 状态 |
|------|------|
| `cellscript` submodule | 已接入 `https://github.com/a19q3/CellScript_Private.git` |
| submodule branch | `arthur/typed-cell-profile-v020` |
| submodule base | `origin/v0.20.0` |
| submodule base commit | `d9f8ec63d400eda0c17b6391cc51032baa515217` |
| submodule typed-cell commit | `10535caa23b6a3860aba83010e5ae30a5e23ea92` |
| submodule worktree | typed-cell MVP、scheduler witness vector、invoice financing example 已提交，当前分支 ahead `origin/v0.20.0` 3 commits |
| 0.20 local prerequisite | 已将 `cellscript-ckb-adapter` 的 `ckb-sdk` 改为 git tag `v5.1.0` |
| typed-cell profile MVP | 已新增 profile、metadata、ELF trailer、7-field scheduler witness 和 70-byte access record |
| root workspace member | 暂缓；保持 submodule 边界，避免 CellScript 0.20 workspace 依赖面扩散 |
| workspace dependency | root workspace 不直接纳入 CellScript；integration crate 通过 path dependency 显式接入 |
| `spora-testing-integration` dependency | 已接入本地 `cellscript` path dependency，root workspace 显式 exclude nested CellScript workspace |
| acceptance script | `cellscript` profile 已改为通过 submodule manifest 跑 CellScript 测试 |
| base devnet acceptance | 已恢复，typed-cell action builder matrix 覆盖 token/AMM/NFT/launch/vesting/multisig/timelock/invoice financing |

当前验证：

```bash
cargo test --locked -p cellscript typed_cell --lib
cargo check --locked -p cellscript
cargo check --locked --workspace
cargo test --locked --manifest-path /Users/arthur/RustroverProjects/Spora/cellscript/Cargo.toml \
  -p cellscript --test examples -- --nocapture --test-threads=1
cargo check --locked -p spora-testing-integration --features "integration-tests devnet-prealloc vm"
cargo test --locked -p spora-testing-integration --lib \
  --features "integration-tests devnet-prealloc vm" \
  common::cellscript_contracts::tests::all_spora_examples_compile_metadata_acceptance \
  -- --nocapture --test-threads=1
cargo test --locked -p spora-wallet-core cellscript_action --lib
cargo test --locked -p spora-consensus execution_dag --lib -- --nocapture
cargo test --locked -p spora-consensus trusted_access_set --lib -- --nocapture
cargo test --locked -p spora-consensus template_scheduler_policy --lib -- --nocapture
cargo test --locked -p spora-mining cellscript_scheduler --lib -- --nocapture
cargo test --locked -p spora-consensus parallel --lib -- --nocapture
scripts/spora_cellscript_acceptance.sh --profile cellscript
scripts/spora_cellscript_acceptance.sh --profile base
scripts/spora_cellscript_acceptance.sh --profile full
scripts/spora_cellscript_acceptance.sh --profile production
cargo test --locked -p spora-exec --test typed_cell_vectors -- --nocapture
cargo test --locked -p spora-consensus invoice_financing --lib -- --nocapture
cargo test --locked --manifest-path /Users/arthur/RustroverProjects/Spora/cellscript/Cargo.toml \
  typed_cell_profile_emits_spora_scheduler_witness_shape --lib -- --nocapture
cargo test --locked --manifest-path /Users/arthur/RustroverProjects/Spora/cellscript/Cargo.toml \
  --test examples -- --nocapture --test-threads=1
```

CellScript 检查在 `/Users/arthur/RustroverProjects/Spora/cellscript` / submodule manifest 下通过；Spora compile-metadata、wallet、consensus、mining、base/full/production devnet 检查在 root 内通过。Focused acceptance profile 通过并生成报告：

```text
/Users/arthur/RustroverProjects/Spora/target/devnet-acceptance/20260510-162619-86051
/Users/arthur/RustroverProjects/Spora/target/devnet-acceptance/20260510-170309-58882
/Users/arthur/RustroverProjects/Spora/target/devnet-acceptance/20260510-172842-7647
```

Base devnet acceptance 通过并生成报告：

```text
/Users/arthur/RustroverProjects/Spora/target/devnet-acceptance/20260510-162538-85342
/Users/arthur/RustroverProjects/Spora/target/devnet-acceptance/20260510-172720-5305
```

Full devnet acceptance 通过并生成报告：

```text
/Users/arthur/RustroverProjects/Spora/target/devnet-acceptance/20260510-163000-89885
```

Production devnet acceptance 通过并生成 production evidence：

```text
/Users/arthur/RustroverProjects/Spora/target/devnet-acceptance/20260510-163653-95485
/Users/arthur/RustroverProjects/Spora/target/devnet-acceptance/20260510-163653-95485/production-evidence.json
/Users/arthur/RustroverProjects/Spora/target/devnet-acceptance/20260510-173321-15223
/Users/arthur/RustroverProjects/Spora/target/devnet-acceptance/20260510-173321-15223/production-evidence.json
```

最新 production evidence 覆盖 8 个 bundled examples、48 个 required action-specific builders、48 个 scoped action artifacts、48 个 valid builder probes、48 个 malformed rejection probes、48 个 scheduler witness shape probes。

当前断点已不再是 `constraints.spora` 编译错误，也不再是 action builder ABI/schema 偏移。runtime fixed vector 与首个业务调度 demo 已补上：

1. `spora-exec` 新增 typed-cell `conflict_hash`、`typed_data_hash`、composite key encoding、scheduler witness Molecule hex 固定向量。
2. CellScript typed-cell profile 新增完整 `scheduler_witness_hex` 固定向量，防止 profile/schema/hash 规则漂移。
3. `ExecutionDAG` 新增 invoice financing 场景：同 invoice 写写串行、不同 invoice 写写并行、同 invoice 读读并行、读写串行。
4. CellScript 新增 bundled `invoice_financing` source example，并纳入 example compile、ELF budget、backend shape baseline、schema manifest 和 scheduler metadata 测试。
5. Spora devnet action-builder matrix 新增 invoice financing 端到端覆盖：`register_invoice`、`approve_drawdown`、`inspect_invoice`、`settle_invoice`、`cancel_invoice`，包含 valid path、malformed rejection、scheduler witness shape 和 scoped action artifact coverage。

下一阶段重点转为 profile-gated typed-cell attribute 语义、wallet/action-builder API 产品化，以及 typed-cell production evidence 的发布口径固化。

## 目标

Phase 2 完成时，需要形成以下端到端链路：

```text
CellScript source
  -> typed-cell profile compile
  -> action metadata emits Molecule scheduler witness
  -> wallet / tx builder appends compiled scheduler witness
  -> CellTx admits witness and returns trusted summary
  -> mempool / block template carries trusted summary
  -> virtual processor validates trusted summary against tx witness
  -> BlockAccessSummary extracts conflict_hash read/write sets
  -> ExecutionDAG serializes conflicts and parallelizes independent txs
```

完成标准：

1. CellScript 可为 typed-cell action 生成合法 Molecule scheduler witness。
2. Witness 能通过 Spora `decode_cellscript_scheduler_witness()` 和 tx admission。
3. `conflict_hash` read/write 分类正确。
4. 同一 conflict key 的 write/write、read/write 被串行化。
5. 同一 conflict key 的 read/read 可并行。
6. block template 能使用 trusted summary 做冲突预过滤。
7. serial runner 与 parallel runner 得到一致 `cell_root`。
8. CellScript acceptance、Spora integration、workspace gate 全部恢复。

## 执行分层

### Layer A：Runtime 消费侧先闭合

目标：不等待 CellScript profile 完成，先证明 Spora runtime 能正确消费 typed-cell scheduler witness。

| Step | 内容 | 文件范围 | 验收 |
|------|------|----------|------|
| A1 | 补 `ExecutionDAG` conflict_hash 测试 | `consensus/src/pipeline/virtual_processor/execution_dag.rs` | `cargo test -p spora-consensus -- execution_dag` |
| A2 | 补 trusted summary -> BlockAccessSummary -> ExecutionDAG 测试 | `consensus/src/pipeline/virtual_processor/access_summary.rs` | `cargo test -p spora-consensus -- trusted_access_set` |
| A3 | 确认 `CellTx::push_cellscript_compiled_scheduler_witness()` summary 能被 template selector 传到 virtual processor | `wallet/core`, `mining`, `consensus` | 已由 wallet/mining/consensus scheduler tests 覆盖 |
| A4 | block template selector 接入 conflict_hash 预过滤 | `mining/src/block_template/selector.rs` 或 selector 下层模型 | `cargo test -p spora-mining cellscript_scheduler` |
| A5 | 并行/串行执行等价性测试 | `consensus/src/pipeline/virtual_processor/*` | `cargo test -p spora-consensus parallel` |

Layer A 不要求 CellScript 编译器产出真实 typed-cell witness。测试可以使用 runtime helper 构造合法 Molecule witness。

### Layer B：CellScript profile 生产侧闭合

目标：让 CellScript 成为 typed-cell scheduler witness 的生产者，而不是仅输出 CKB metadata。

| Step | 内容 | 文件范围 | 验收 |
|------|------|----------|------|
| B1 | 新增 profile 枚举 | `cellscript/src/lib.rs`, `cellscript/src/cli/commands.rs`, `cellscript/src/codegen/mod.rs` | `cellc --target-profile typed-cell` 可识别 |
| B2 | 定义 typed-cell target metadata | `TargetProfile::metadata()` | metadata 中 profile name、scheduler ABI、hash domain 稳定 |
| B3 | profile-gated typed-cell attributes | parser / AST / IR | `#[conflict_key(...)]`, `#[identity(...)]` 先 parse + metadata，不污染 CKB |
| B4 | conflict key canonical encoding | CellScript lowering | field/composite 编码与 Spora runtime 规则一致 |
| B5 | 生成 70-byte access record Molecule witness | CellScript metadata/lowering | `scheduler_witness_hex` 可被 Spora decode |
| B6 | metadata contract 对齐 | `ActionMetadata`, constraints metadata | Spora 不再依赖旧 `constraints.spora` |

建议 profile 名称先用：

```text
typed-cell
```

不要先做 `spora` 专名。原因是当前 governance 已把 typed-cell 作为共享中间层，后续 Spora / Hypha / Axone 可以是 deployment backend，而不是三套语言核心。

### Layer C：Spora integration 迁移

目标：把旧 Spora acceptance helper 从 `target_profile = "spora"` 迁到 typed-cell profile contract。

| Step | 内容 | 文件范围 | 验收 |
|------|------|----------|------|
| C1 | 替换 `target_profile: Some("spora")` | `testing/integration/src/common/cellscript_contracts.rs` | 测试能进入 CellScript compile |
| C2 | 删除对 `metadata.constraints.spora` 的硬依赖 | 同上 | 用 typed-cell constraints 或 action scheduler metadata |
| C3 | 使用 `ActionMetadata::scheduler_witness_bytes()` | integration/devnet tests | witness 能 append 到 `CellTx` |
| C4 | 保存 trusted summary | wallet / mempool / mining selector | virtual processor 可 strict validate |
| C5 | 恢复 CellScript acceptance profile | `scripts/spora_cellscript_acceptance.sh` | `--profile cellscript` 通过 |

迁移原则：

1. 不用空 `spora` constraints shim 假装兼容。
2. 不让 runtime 信任 JSON 字段本身；runtime 只信任 tx witness 和 producer-provided trusted summary 的一致性。
3. metadata 是 builder 输入，consensus validation 仍以 Molecule witness admission 为准。

### Layer D：端到端 demo

目标：用最小业务证明调度价值，不先做完整业务平台。

优先 demo：

```text
Invoice Financing
```

原因：

1. conflict key 清晰：`invoice_id`。
2. 业务冲突清晰：同一发票不能重复融资。
3. typed_data_hash 有审计价值：承诺 invoice payload。
4. 不依赖复杂 AMM 数学或 settlement 设计。

最小验收：

| 场景 | 预期 |
|------|------|
| 两个 tx 使用同一 `invoice_id` 写入 | 不在同一 DAG 层并行 |
| 两个 tx 使用不同 `invoice_id` 写入 | 可并行 |
| 同一 `invoice_id` read/read | 可并行 |
| witness conflict_hash 被篡改 | trusted summary validation 拒绝 |

## 立即执行清单

### 第 1 批：Spora runtime gate

1. 补 `ExecutionDAG` conflict_hash 单元测试。
2. 补 trusted summary 与 `BlockAccessSummary` 的端到端单元测试。
3. 确认 block template selector 的 selected trusted summaries 能传到 virtual processor。
4. 不改 CellScript compiler。

交付标准：

```bash
cargo test -p spora-consensus -- execution_dag
cargo test -p spora-consensus -- trusted_access_set
cargo test -p spora-mining
```

### 第 2 批：CellScript profile MVP

1. 新增 `TargetProfile::TypedCell`。已完成。
2. 新增 metadata target contract，不先引入 deployment backend。已完成。
3. 让简单 action 能输出合法 typed-cell scheduler witness。已完成。
4. 增加 CellScript 自测，验证 7-field Molecule table、70-byte access record、非零 conflict_hash / typed_data_hash。已完成。
5. 下一步补 profile-gated `conflict_key` / `identity` attribute 与 Spora runtime 固定 vector 对齐。

交付标准：

```bash
cargo test --locked -p cellscript typed_cell --lib
cargo check --locked --workspace
```

### 第 3 批：Spora integration restore

1. `cellscript_contracts.rs` 切到 typed-cell profile。已完成。
2. 移除 `constraints.spora` 依赖。已完成。
3. 使用 `scheduler_witness_bytes()` 生成 tx witness。已有路径，compile-metadata acceptance 与 focused acceptance profile 已覆盖 witness shape。
4. wallet metadata parser 接受 `typed-cell` scheduler witness metadata。已完成。
5. focused `cellscript` acceptance profile。已通过。
6. base devnet profile。已通过。
7. full devnet profile。已通过。
8. production devnet profile 与 production evidence。已通过。

交付标准：

```bash
cargo test --locked -p spora-testing-integration --lib \
  --features "integration-tests devnet-prealloc vm" \
  common::cellscript_contracts::tests::all_spora_examples_compile_metadata_acceptance \
  -- --nocapture --test-threads=1

scripts/spora_cellscript_acceptance.sh --profile cellscript
scripts/spora_cellscript_acceptance.sh --profile base
scripts/spora_cellscript_acceptance.sh --profile full
scripts/spora_cellscript_acceptance.sh --profile production
```

### 第 4 批：业务 demo

1. 新增 invoice financing runtime DAG demo。已完成。
2. 新增 invoice financing `.cell` example。已完成。
3. 生成 typed-cell scheduler witness。已由 CellScript example metadata 覆盖。
4. 构造同 conflict key / 不同 conflict key 的交易矩阵。runtime DAG demo 已完成。
5. 补 Spora action-specific builder + devnet acceptance。已完成，覆盖 invoice financing 5 个 action。
6. 跑 production devnet profile 与 evidence validation。已完成，production evidence 覆盖 8 个 bundled examples / 48 个 action-specific builders。

交付标准：

```bash
scripts/spora_cellscript_acceptance.sh --profile production
```

## 风险与处理

| 风险 | 影响 | 处理 |
|------|------|------|
| CellScript 0.20 基线与后续 upstream 差异变大 | patch 冲突、API 漂移 | typed-cell 适配固定在 `arthur/typed-cell-profile-v020`，定期从 0.20 后续基线 rebase |
| `constraints.spora` 旧字段诱导假兼容 | acceptance 通过但语义不对 | 不做 shim，直接迁移到 typed-cell metadata contract |
| block template 随机 selector 难以稳定测 conflict ordering | 测试 flaky | 单元测试使用 deterministic selector 或固定 candidate 顺序 |
| witness 生产者和 runtime hash 规则漂移 | consensus 风险 | hash domain/encoding 在 Spora runtime 定义，CellScript 测试引用固定 vectors |
| typed-cell profile 过早绑定 Spora deployment | 影响 Hypha/Axone | profile 使用 `typed-cell`，deployment backend 后置 |

## 决策点

需要明确的架构决策：

1. Profile 名称：建议 `typed-cell`，不要恢复旧 `spora`。
2. `metadata.constraints.spora`：建议废弃，不新增兼容字段。
3. CellScript branch：使用 `arthur/typed-cell-profile-v020`，基于 `origin/v0.20.0`。
4. Demo 顺序：先 invoice financing，再 AMM pool。

## 推荐排期

| 顺序 | 阶段 | 预估工作量 | 是否阻塞下一步 |
|------|------|------------|----------------|
| 1 | Layer A runtime gate | 小 | 不阻塞 CellScript，可立即做 |
| 2 | Layer B profile MVP | 中 | 阻塞 integration restore |
| 3 | Layer C integration restore | 中 | 阻塞 acceptance |
| 4 | Layer D invoice demo | 中 | 阻塞对外演示 |

## 最小可交付版本

MVP 不需要完成所有 typed-cell attribute 语义。最小可交付只需要：

1. `TargetProfile::TypedCell` 可编译。
2. 一个 action 可输出合法 scheduler witness。
3. `conflict_hash` 和 `typed_data_hash` 与 Spora runtime vector 一致。
4. Spora tx builder 能 append witness 并拿到 trusted summary。
5. 两笔同 conflict key 的 tx 在 DAG 中串行。

这能证明核心闭环，再逐步补 `identity`、`settlement`、accounting、ProofPlan 和更复杂业务约束。
