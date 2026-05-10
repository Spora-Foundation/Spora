# CellScript 与 Spora 适配执行方案

Branch: `spora-typed`
日期：2026-05-10
状态：方案形成，CellScript typed-cell profile MVP、profile-gated conflict_key/identity metadata、live scheduler witness plan/builder、Spora compile-metadata acceptance、wallet/action-builder typed-cell scheduler plan 解析、Generator live witness/typed output/WASM 入参桥接、invoice financing live witness action-builder matrix、base/cellscript/production acceptance profiles 已落地

## 结论

CellScript 已重新作为 submodule 接入 Spora，并切到 0.20-based 独立适配分支。0.20 基线的 `cellscript-ckb-adapter` 已从本地 `../../../ckb-sdk-rust` 路径改为 `ckb-sdk-rust` git tag 依赖，因此 submodule 自身可以独立 `cargo check --workspace`。Spora root workspace 仍暂不直接纳入 CellScript workspace；先保持 submodule 边界，避免把 CellScript 0.20 的 adapter/dev-test 依赖面扩散到 Spora root。

1. Spora integration 的 compile-metadata acceptance 已从旧 `target_profile = "spora"` 迁到 `typed-cell`。
2. CellScript 0.20-based 分支已新增 `TargetProfile::TypedCell` 的 MVP，不恢复旧 `spora` profile。
3. `metadata.constraints.spora` 的硬依赖已从 integration helper 中移除；typed-cell contract 迁移到 action scheduler witness / typed-cell metadata，不做假兼容字段。
4. 双方已经具备可对接的核心桥：CellScript 有 `ActionMetadata::scheduler_witness_bytes()`；Spora runtime 有 `CellTx::push_cellscript_compiled_scheduler_witness()`、trusted summary、`BlockAccessSummary` 和 conflict-hash 调度路径。
5. Spora devnet builder matrix 已按 typed-cell ABI 重新对齐：cell-bound input/output 的 source/index 顺序、read-ref `CellDep#0`、新增 schema 字段（例如 `wallet_id`、`lock_id`、receipt `state`）均改为以 CellScript metadata 为准。
6. CellScript typed-cell metadata schema 已升级到 v43，`#[identity(...)]` / `#[conflict_key(...)]` 只在 `target_profile = "typed-cell"` 下生成嵌套 `typed_cell` metadata，并校验 conflict key 字段必须存在且为 fixed-width；CKB profile 不暴露该字段。
7. CellScript action metadata 现在输出 `typed_cell_scheduler_plan`，并提供 Spora-compatible live witness builder：wallet/tx-builder 传入 type script、conflict_key_value、typed data 后，可按 Spora runtime 固定 vector 派生 `conflict_hash` / `typed_data_hash`。
8. Spora wallet/action-builder 已开始消费该 plan：typed-cell metadata 解析会校验 scheduler plan ABI/hash domain/source-operation/data-source 组合，补齐 effect/cycles 与 conflict-key field slice，并把 plan 存入 `GeneratorSettings`。
9. Wallet 已新增 live typed-cell scheduler witness helper，并接入 `Generator` final tx 路径：当 `typed_cell_scheduler_plan` 存在时，Generator 优先用显式 typed output 配置或 resolved Input/CellDep sidecar 生成 live Molecule witness，不再把 compile-time shape witness 当作最终调度凭证；WASM generator 也已能接收这些 typed-cell output/sidecar 参数。
10. Spora devnet invoice financing matrix 已从静态 `scheduler_witness_hex` 切到 live witness：测试按 CellScript `typed_cell_scheduler_plan` 读取 source/index/type，通过 scoped action artifact 中的 `TypeMetadata.fields` 自动按 offset/encoded_size/fixed_width 抽取 conflict key，并优先使用真实 Output type script 或 sidecar type script 生成 Molecule scheduler witness。

因此执行策略是：先闭合 Spora runtime 消费侧，再在 CellScript 增加 typed-cell profile，恢复端到端 acceptance，最后把 metadata plan 下沉到 wallet live tx builder；当前断点已转向具体业务 tx skeleton 自动填充 typed output 配置与 Input/CellDep sidecar。

## 当前状态

已完成：

| 项目 | 状态 |
|------|------|
| `cellscript` submodule | 已接入 `https://github.com/a19q3/CellScript_Private.git` |
| submodule branch | `arthur/typed-cell-profile-v020` |
| submodule base | `origin/v0.20.0` |
| submodule base commit | `d9f8ec63d400eda0c17b6391cc51032baa515217` |
| submodule typed-cell commit | `ece9387415108c55f9f4577f7f421883087730e5` |
| submodule worktree | typed-cell MVP、scheduler witness vector、invoice financing example、typed-cell conflict key metadata、live scheduler witness plan/builder 已提交，当前分支 ahead `origin/v0.20.0` 5 commits |
| 0.20 local prerequisite | 已将 `cellscript-ckb-adapter` 的 `ckb-sdk` 改为 git tag `v5.1.0` |
| typed-cell profile MVP | 已新增 profile、metadata、ELF trailer、7-field scheduler witness、70-byte access record、profile-gated conflict key metadata、live scheduler witness plan/builder |
| root workspace member | 暂缓；保持 submodule 边界，避免 CellScript 0.20 workspace 依赖面扩散 |
| workspace dependency | root workspace 不直接纳入 CellScript；integration crate 通过 path dependency 显式接入 |
| `spora-testing-integration` dependency | 已接入本地 `cellscript` path dependency，root workspace 显式 exclude nested CellScript workspace |
| wallet/action-builder metadata | 已解析并校验 `typed_cell_scheduler_plan`，存入 `GeneratorSettings`，并补齐 live witness 所需 effect/cycles/field slices |
| wallet live witness helper | 已支持 Output 真实 tx data、Input/CellDep resolved sidecar、single/composite fixed conflict key extraction、Molecule witness append；Generator final tx 会在 typed-cell plan 存在时优先生成 live witness |
| wallet typed output config | 已支持 action-specific builder 显式配置 final output 的 typed-cell type script/data，并与 CKB TYPE_ID 输出冲突 fail-closed |
| WASM generator typed-cell bridge | 已新增 `cellscriptTypedCellOutputs` / `cellscriptTypedCellResolvedCells` 入参，支持 JS 侧传入 typed output type script/data 与 Input/CellDep sidecar |
| devnet live witness helper | 已把 invoice financing 5 个 action 的 valid/malformed action-builder matrix 切到 concrete cell data -> metadata-driven live typed-cell scheduler witness，支持真实 Output/sidecar type script、single/composite fixed conflict key |
| production evidence live coverage | 已新增 typed-cell scheduler plan 覆盖数、live typed-cell scheduler witness 覆盖数和 per-action live witness 标记 |
| acceptance script | `cellscript` profile 已改为通过 submodule manifest 跑 CellScript 测试 |
| base devnet acceptance | 已恢复，typed-cell action builder matrix 覆盖 token/AMM/NFT/launch/vesting/multisig/timelock/invoice financing |

当前验证：

```bash
cargo test --locked -p cellscript typed_cell --lib
cargo test --locked -p cellscript \
  test_parse_typed_cell_identity_and_conflict_key_attributes --lib -- --nocapture
cargo test --locked -p cellscript compile_typed_cell_conflict_key --lib -- --nocapture
cargo test --locked -p cellscript typed_cell_live_hash_helpers_match_spora_vectors --lib -- --nocapture
cargo test --locked -p cellscript action_typed_cell_scheduler_witness_builder_matches_spora_vector --lib -- --nocapture
cargo check --locked -p cellscript
cargo check --locked --workspace
cargo test --locked --manifest-path /Users/arthur/RustroverProjects/Spora/cellscript/Cargo.toml \
  -p cellscript --test examples -- --nocapture --test-threads=1
cargo test --locked -p cellscript --test examples \
  invoice_financing_actions_expose_scheduler_metadata -- --nocapture
cargo check --locked -p spora-testing-integration --features "integration-tests devnet-prealloc vm"
cargo test --locked -p spora-testing-integration --lib \
  --features "integration-tests devnet-prealloc vm" \
  devnet_acceptance_tests::devnet_acceptance_base \
  -- --nocapture --test-threads=1
cargo test --locked -p spora-testing-integration --lib \
  --features "integration-tests devnet-prealloc vm" \
  common::cellscript_contracts::tests::all_spora_examples_compile_metadata_acceptance \
  -- --nocapture --test-threads=1
cargo check --locked -p spora-wallet-core
cargo test --locked -p spora-wallet-core typed_cell --lib -- --nocapture
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

CellScript 检查在 `/Users/arthur/RustroverProjects/Spora/cellscript` / submodule manifest 下通过；Spora compile-metadata、wallet/action-builder metadata parser、wallet live witness helper、consensus、mining、base/full/production devnet 检查在 root 内通过。Focused acceptance profile 通过并生成报告：

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

最新 production evidence 覆盖 8 个 bundled examples、48 个 required action-specific builders、48 个 scoped action artifacts、48 个 valid builder probes、48 个 malformed rejection probes、48 个 scheduler witness shape probes，并开始输出 `typed_cell_scheduler_plan_count`、`live_typed_cell_scheduler_witness_count` 与每个 action 的 live witness 覆盖标记。

当前断点已不再是 `constraints.spora` 编译错误，也不再是 action builder ABI/schema 偏移。runtime fixed vector 与首个业务调度 demo 已补上：

1. `spora-exec` 新增 typed-cell `conflict_hash`、`typed_data_hash`、composite key encoding、scheduler witness Molecule hex 固定向量。
2. CellScript typed-cell profile 新增完整 `scheduler_witness_hex` 固定向量，防止 profile/schema/hash 规则漂移。
3. `ExecutionDAG` 新增 invoice financing 场景：同 invoice 写写串行、不同 invoice 写写并行、同 invoice 读读并行、读写串行。
4. CellScript 新增 bundled `invoice_financing` source example，并纳入 example compile、ELF budget、backend shape baseline、schema manifest 和 scheduler metadata 测试。
5. Spora devnet action-builder matrix 新增 invoice financing 端到端覆盖：`register_invoice`、`approve_drawdown`、`inspect_invoice`、`settle_invoice`、`cancel_invoice`，包含 valid path、malformed rejection、scheduler witness shape 和 scoped action artifact coverage；当前 invoice matrix 已使用 concrete cell data、真实可用的 type script 和 CellScript type field layout 生成 live typed-cell scheduler witness。
6. CellScript 新增 `#[identity(field(...))]` / `#[conflict_key(...)]` typed-cell attribute 语义，AST/IR/metadata 全链路 profile-gated，invoice financing example 已声明 `invoice_id` 作为 shared/receipt conflict key。
7. CellScript 新增 `typed_cell_scheduler_plan` 和 live scheduler witness builder，hash helper 已与 Spora `typed_cell_vectors` 中的 conflict hash、typed data hash、Molecule witness 固定向量对齐。

下一阶段重点转为把具体 action-builder tx skeleton 的 typed-cell cell 构造补齐：在 Rust/JS builder 层自动提供 Input/CellDep resolved sidecar 与 typed output 配置，并把 live witness 覆盖从 invoice financing 推广到更多 typed-cell examples；production evidence 已能追踪 scheduler plan / live witness 覆盖缺口。

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
| B3 | profile-gated typed-cell attributes | parser / AST / IR | 已完成：`#[conflict_key(...)]`, `#[identity(...)]` parse + metadata，不污染 CKB |
| B4 | conflict key canonical encoding | CellScript lowering / wallet builder | 部分完成：metadata/schema canonical fields、composite helper、live witness builder 已对齐 Spora fixed vector；wallet/Generator 已解析 scheduler plan 并支持 fixed field-slice 自动抽取，具体业务 tx skeleton 自动 sidecar 仍需接入 |
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
5. profile-gated `conflict_key` / `identity` attribute metadata 已完成，包含 fixed-width 字段校验与 CKB profile 隔离。
6. 声明式 `conflict_key` 已进入 `typed_cell_scheduler_plan`，live scheduler witness builder 已与 Spora runtime 固定 vector 对齐。
7. wallet/action-builder 已解析并保存 plan，live witness helper 已能从真实 Output data 与 resolved Input/CellDep data 自动抽取 conflict key 和 typed data。
8. 下一步把 helper 接入具体 action-builder tx skeleton，自动提供 resolved sidecar。

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
5. wallet metadata parser 接受并校验 `typed_cell_scheduler_plan`。已完成。
6. wallet live witness helper 生成并 append typed-cell Molecule witness。已完成基础 API。
7. focused `cellscript` acceptance profile。已通过。
8. base devnet profile。已通过。
9. full devnet profile。已通过。
10. production devnet profile 与 production evidence。已通过。

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

MVP 已超过原始最小可交付，typed-cell attribute metadata 与 live scheduler witness builder 已落地。下一步最小可交付只需要继续闭合：

1. wallet/action-builder API 已能从 CellScript metadata 解析并保存 `typed_cell_scheduler_plan`，包含 effect/cycles 与 conflict-key field slices。
2. wallet live helper 已能从真实 Output data 与 resolved Input/CellDep data 自动抽取 `conflict_key_value` 与 typed data，并 append live scheduler witness。
3. Generator 已在 final tx 路径优先使用 live typed-cell scheduler witness，并支持 native/WASM typed output type script/data 与 resolved sidecar 配置；下一步是 invoice financing 等具体 tx skeleton 自动提供这些配置。
4. production evidence 下一步固定 typed-cell metadata、builder matrix、scheduler plan 和 witness shape 的发布口径。

这能把当前 parse/metadata 层能力推进到 tx 构造和调度层，再逐步补 `settlement`、accounting、ProofPlan 和更复杂业务约束。
