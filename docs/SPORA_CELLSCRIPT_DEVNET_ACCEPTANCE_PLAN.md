# Spora + CellScript Devnet Acceptance Test Plan

## 1. 目标

本计划用于验收 Spora 在接入 CellScript 后的端到端可用性。验收重点不是单元测试覆盖率，而是在真实 devnet 节点、真实 RPC、真实 mempool、真实 block template、真实 VM 校验路径上证明以下能力：

- devnet 能从干净 appdir 启动，创建可控的创世/预分配资金环境。
- 测试工具能生成 devnet 助记词、钱包、地址，并把该地址用于 `sporad --num-prealloc-cells` 的预分配。
- 节点能挖矿、维护 virtual chain、更新 cellindex、同步 mempool/block/cell state。
- 预分配资金能被实际签名、提交、进入 mempool、进入 block template、被挖出，并在 cellindex 中可查询。
- VM 脚本在真实交易验证路径中被执行，成功和失败路径都被覆盖。
- CellScript 合约能编译为 Spora artifact，部署为 code cell，通过 cell dep 被真实交易使用，并且 action metadata / Molecule scheduler witness / wallet generator 集成路径被验证。
- 同一套验收明确区分 Spora profile 和 CKB profile，不因为 CellScript 增强破坏 Spora 原有能力。

## 2. 当前命令需要修正

用户给出的启动命令：

```bash
cargo run --bin sporad --features devnet-prealloc -- \
  --devnet \
  --num-prealloc-cells=101 \
  --cellindex \
  --nodnsseed \
  --disable-upnp \
  --rpclisten=0.0.0.0:16610 \
  --rpclisten-borsh=0.0.0.0:17610 \
  --enable-unsynced-mining \
  --yes
```

目前不完整。`sporad` 的 `devnet-prealloc` 功能只会把资金分配给一个已有地址；它不会生成钱包，也不会生成助记词。只传 `--num-prealloc-cells` 而不传 `--prealloc-address` 会被 `validate_args` 拒绝。

验收入口应改成两步：

1. 生成 devnet 助记词、钱包、地址和 manifest。
2. 用生成出来的地址启动 `sporad`。

建议最终命令形态：

```bash
RUN_DIR="${TMPDIR:-/tmp}/spora-devnet-acceptance-$(date +%Y%m%d-%H%M%S)"

# 非交互 devnet bootstrap helper。
cargo run -p spora-testing-integration --bin spora-devnet-bootstrap -- \
  --network devnet \
  --wallet-dir "$RUN_DIR/wallet" \
  --wallet-name acceptance \
  --out "$RUN_DIR/bootstrap.json"

DEVNET_ADDRESS="$(python3 - "$RUN_DIR/bootstrap.json" <<'PY'
import json
import sys
with open(sys.argv[1], "r", encoding="utf-8") as fh:
    print(json.load(fh)["wallet"]["default_address"])
PY
)"

cargo run --bin sporad --features devnet-prealloc -- \
  --devnet \
  --appdir "$RUN_DIR/sporad" \
  --num-prealloc-cells=101 \
  --prealloc-address="$DEVNET_ADDRESS" \
  --prealloc-amount=10000000000 \
  --cellindex \
  --nodnsseed \
  --disable-upnp \
  --rpclisten=0.0.0.0:16610 \
  --rpclisten-borsh=0.0.0.0:17610 \
  --rpclisten-json=0.0.0.0:18610 \
  --enable-unsynced-mining \
  --skip-proof-of-work \
  --unsaferpc \
  --yes
```

真实外部进程验收还必须在节点启动后运行 probe，而不是只检查日志：

```bash
cargo run -p spora-testing-integration --bin spora-devnet-probe -- \
  --grpc grpc://127.0.0.1:16610 \
  --wrpc-borsh ws://127.0.0.1:17610 \
  --wrpc-json ws://127.0.0.1:18610 \
  --address "$DEVNET_ADDRESS" \
  --expected-prealloc-cells 101 \
  --expected-prealloc-amount-sau 10000000000 \
  --out "$RUN_DIR/probe-report.json"
```

说明：

- `--prealloc-address` 必须存在，并且网络前缀必须匹配 devnet。
- `--prealloc-amount` 的单位是 SAU，不是 SPORA。
- `--rpclisten` 是 gRPC；`--rpclisten-borsh` 和 `--rpclisten-json` 是 wRPC 入口。
- `--unsaferpc` 用于验收环境中需要影响节点状态的 RPC 路径。
- `--skip-proof-of-work` 只用于本地 devnet/simnet 验收；否则 `get_block_template` 返回的块仍需要外部 miner 求解 nonce。
- 测试脚本必须把 devnet mnemonic 明确标记为测试资产，写入临时目录，不允许默认写入用户真实钱包目录。

## 3. 交付物

本轮已经新增/规划以下文件：

| 文件 | 作用 | 状态 |
| --- | --- | --- |
| `docs/SPORA_CELLSCRIPT_DEVNET_ACCEPTANCE_PLAN.md` | 本验收计划 | 已新增 |
| `scripts/devnet_acceptance.sh` | 外部进程级真实 devnet 验收入口 | 已新增，当前支持 `smoke`/`external-boot`/`propagation`/`cellscript`/`full` |
| `testing/integration/src/devnet_acceptance_tests.rs` | in-process devnet/simnet 验收测试 | 已新增，当前覆盖 devnet prealloc -> mining -> signed transfer -> parent/child mempool -> scheduler tamper 负例 -> mempool -> template -> block -> cellindex -> code cell deploy -> VM cell dep spend -> CellScript ELF deploy/use |
| `testing/integration/src/common/devnet_bootstrap.rs` | 生成助记词、keypair、地址、钱包 manifest 的测试工具 | 已新增 |
| `testing/integration/src/common/cellscript_contracts.rs` | 编译 CellScript Spora ELF 并校验 artifact hash / Molecule VM ABI metadata 的辅助工具 | 已新增，当前提供 no-op、fixed-output schema verifier、parameterized amount 合约 smoke |
| `testing/integration/src/bin/spora-devnet-bootstrap.rs` | 非交互 bootstrap CLI，供真实 devnet 脚本使用 | 已新增 |
| `testing/integration/src/bin/spora-devnet-probe.rs` | 外部进程 probe，连接 gRPC、wRPC Borsh、wRPC JSON，验证 prealloc/cellindex，并提交模板块推进 DAA | 已新增 |

如不希望给 `spora-testing-integration` 增加 bin，也可以把 bootstrap CLI 放在 `tools/devnet-bootstrap` 或 `xtask`。验收标准不依赖具体位置，但必须满足非交互、可重复、输出机器可读 manifest。

当前已经落地的基础设施还包括：

- `sporad --skip-proof-of-work`，仅允许 devnet/simnet 使用，便于本地验收直接提交模板块。
- devnet prealloc genesis `cell_root`/`cell_commitment` 修正。
- prealloc/imported cell 的完整 lock/type/data 元数据保留，覆盖 mempool、body validation、virtual replay 和 cellindex 查询路径。
- RPC compact cell entry 使用 consensus 一致的 canonical data hash，避免签名端和验证端的 input material 不一致。
- RPC/gRPC transaction model 保留 `cell_deps` 和 `header_deps`，避免合约交易通过 RPC 后丢失 code cell 依赖。
- Rust wRPC client 已补齐 Spora handshake，external probe 当前会真实连接 Borsh 和 JSON wRPC endpoint，验证网络、cellindex 和 DAG 信息。
- CellsChanged 通知链路已修正为跨网络可用：address tracker 不再把索引地址强制解释成 Mainnet，index 层按订阅地址 lock hash 过滤 CellDiff，index -> RPC 转换用节点网络 prefix 从完整 lock script 还原地址。
- in-process smoke 已经部署真实 VM code cell，验证缺失 `CellDep::Code` 时被拒绝，提供正确 dep 时能执行 VM 脚本并把输出写入 cellindex。
- in-process smoke 已经把 CellScript no-op action 编译成 Spora RISC-V ELF，校验 SPORABI/Molecule VM ABI metadata，把 artifact 部署为 code cell，并通过 cell dep 真实消费 CellScript-locked cell。
- in-process smoke 已经把 CellScript fixed-output schema verifier 编译成 Spora RISC-V ELF，链上 spend 必须用 `LOAD_CELL_DATA` 校验 Output cell data，并通过 cellindex 查询到 8-byte data hash。
- in-process smoke 已经把 CellScript parameterized amount verifier 编译成 Spora RISC-V ELF，链上 spend 通过 GroupInput witness 的 `CSARGv1\0 + u64_le` 入口参数 ABI 传入 `amount`，输出 data 必须与 witness-bound amount 一致。
- CellScript public metadata API 已提供 `ActionMetadata::entry_witness_args` / `LockMetadata::entry_witness_args` / `encode_entry_witness_args_for_params`，交易构造器可从 action/lock metadata 生成 `_cellscript_entry` 所需的 `cellscript-entry-witness-v1` bytes；devnet 参数化 amount 正例已改为使用该公共 API，避免在测试里手写 ABI bytes。
- 已清理暴露的 deprecated Borsh scheduler witness generator：CellScript 标准库不再提供 `generate_legacy_borsh`，`cellscript` crate 也不再直接依赖 `borsh`；旧 metadata sidecar 字段只保留为 crate 内 serde 兼容路径，不能作为公开 scheduler witness ABI。
- entry witness/CLI/simulate 中重复的 hex/type-width helper 已合并为 crate 内共享实现，避免公共 builder 和 CLI builder 规则漂移。
- in-process smoke 已经覆盖 parent/child mempool 依赖：child 消费 parent 的 mempool 输出；如果 template 同时选择 parent 和 child，必须按 parent -> child 拓扑顺序打包；如果 template 只选择 ready parent，则 parent accepted 后下一块必须提升 child，最终 child 输出可被 cellindex 查询。
- in-process smoke 已经覆盖 scheduler witness tamper 负例：带 malformed CellScript scheduler witness bytes 的 VM spend 必须在 mempool scheduler policy 阶段被拒绝，不能落到普通 VM 成功路径。
- CI 已新增 `.github/workflows/spora-devnet-acceptance.yml`：PR/push 跑 `scripts/devnet_acceptance.sh --profile smoke`，nightly 和手动 release 验收跑 `--profile full`。

## 4. 测试分层

### 4.1 快速 in-process 验收

用途：本地开发和 CI PR gate。使用现有 `Daemon` harness 启动节点，避免进程管理开销，但必须走真实 consensus、mempool、RPC、cellindex 和 VM 路径。

命令：

```bash
cargo test -p spora-testing-integration --lib \
  --features "integration-tests devnet-prealloc vm" \
  devnet_acceptance_smoke \
  -- --nocapture --test-threads=1
```

覆盖：

- 生成 devnet/simnet 地址和 keypair。
- 启动带 `initial_cell_set` 的节点。
- 开启 cellindex。
- 挖若干块推进 DAA。
- 查询预分配 cell。
- 签名并提交标准转账。
- 验证 mempool entry、block template、submit block、cellindex 输出。
- 提交 parent/child 交易链，child 消费 parent 的 mempool 输出，验证 mempool 接受、template 同块拓扑顺序或 parent accepted 后下一块 promotion，以及 child 输出索引。
- 部署 always-success VM fixture 为 code cell。
- 缺失 code cell dep 的 VM-locked spend 必须失败。
- 带 malformed CellScript scheduler witness 的 VM-locked spend 必须被 scheduler metadata policy 拒绝。
- 带正确 code cell dep 的 VM-locked spend 必须成功进入 mempool、template、block，并被 cellindex 查询到。
- 编译 CellScript Spora ELF no-op 合约，单独部署 code cell，创建 CellScript-locked cell。
- 缺失 CellScript code cell dep 的 spend 必须失败。
- 带正确 CellScript code cell dep 的 spend 必须成功进入 mempool、template、block，并被 cellindex 查询到。

### 4.2 外部进程真实 devnet 验收

用途：发布前验收。用真实 `cargo run --bin sporad` 或已构建的 `sporad` 二进制启动节点。

命令：

```bash
scripts/devnet_acceptance.sh --profile full
```

覆盖：

- 真实进程启动/关闭。
- 真实 appdir、日志、数据库、RPC 端口。
- 非交互 devnet 钱包生成。
- 预分配资金写入节点配置并被 cellindex 发现。
- probe 真实连接 gRPC、wRPC Borsh、wRPC JSON，确认 devnet network id、cellindex 状态和 DAG 状态。
- probe 通过 gRPC 获取 block template 并提交 block，确认 virtual DAA 从 0 推进到 1。
- 挖矿、转账、复杂交易、VM code cell 部署和使用。
- acceptance 明确启用 relaxed mass policy：`--relaynonstd` 跳过 standard relay 的单笔 100k mass 限制，`--blockmaxmass=100000000` 提高验收节点区块模板容量；该策略是显式 operator opt-in，适用于所有网络，默认网络策略不变。
- CellScript no-op 合约部署/使用已进入 smoke；所有 bundled examples (`amm_pool.cell`、`launch.cell`、`multisig.cell`、`nft.cell`、`timelock.cell`、`token.cell`、`vesting.cell`) 必须编译为 Spora ELF、逐个部署为 code cell、进入 block template、出块后可被 cellindex 查询。
- 对每个 bundled example，smoke 还会创建对应 locked probe cell，并提交带正确 code dep 但缺业务 witness/参数的 malformed spend；这些交易必须失败，用于确认脚本确实被加载执行且不会错误放行空业务路径。
- 输出完整验收报告 JSON。`acceptance-report.json` 必须指向 `smoke-report.json`；脚本会校验 smoke report 结构、parent/child、scheduler tamper、7 个示例清单、relaxed mass policy、部署索引结果和 malformed spend 拒绝原因。

当前 `cellscript` profile 已挂入 focused 工具验收：

- bundled examples 编译到 ELF。
- local path dependency package 编译。
- registry dependency fail-closed。
- `cellc check` / `cellc build` package flow。
- `cellc init` / `cellc info` JSON。
- `cellc add` / `remove` dev path。
- `cellc install --path` 写入 `Cell.lock`，`remove` 后剪枝 lockfile。

`full` profile 当前会把 smoke、external boot/probe、two-node propagation、CellScript/package manager suite 四段结果汇总到 `acceptance-report.json`，并把 CellScript 子结果写入 `cellscript-report.json`。
每次脚本运行的 artifact 目录使用 timestamp + process id，避免本地或 CI 并行 profile 在同一秒启动时覆盖彼此报告。

`propagation` profile 可单独运行：

```bash
scripts/devnet_acceptance.sh --profile propagation
```

该 profile 跑 `daemon_cells_propagation_test`，用于快速复验两节点 block relay、transaction acceptance、cellindex 一致性和 address-scoped CellsChanged 通知链路。

### 4.3 Relaxed mass policy 精确定义

当前验收放宽到以下边界：

- `--relaynonstd`：显式跳过 mempool standard relay 检查。被跳过的 standard 检查包括单笔交易 `compute_mass`、`transient_mass`、`storage_mass` 的 `100_000` 上限，以及同一 standardness 阶段的 witness size、dust、standard script shape、minimum relay fee 检查。
- `--blockmaxmass=100000000`：把 block template 的 maximum mass 从默认 `10_000_000` 提高到 `100_000_000`。
- 该放宽是运行时显式 opt-in，适用于 mainnet、testnet、devnet、simnet 所有网络；不传参数时所有网络仍使用默认 standard relay policy 和默认 block mass。
- `--rejectnonstd` 与 `--relaynonstd` 冲突；如果显式使用 `--rejectnonstd`，不会启用 non-standard relay。
- 验收脚本会在 `smoke-report.json` 中记录：
  - `relay_non_standard: true`
  - `block_max_mass: 100000000`
  - `applies_to_all_networks_when_explicitly_enabled: true`
  - `standard_policy_preserved_by_default: true`
  - `parent_child_mempool_confirmed: true`
  - `scheduler_tamper_rejected: true`

注意：这不是共识规则放宽，而是 mempool/template relay policy 的 operator opt-in。生产网络是否应该使用该策略，需要另行评估 DoS 风险、fee policy、artifact 发布策略和节点资源配置。

### 4.4 夜间完整验收

用途：长耗时和复杂合约路径，不阻塞普通 PR。

命令：

```bash
SPORA_DEVNET_ACCEPTANCE_FULL=1 cargo test -p spora-testing-integration --lib \
  --features "integration-tests devnet-prealloc vm" \
  devnet_acceptance_full \
  -- --nocapture --test-threads=1
```

覆盖：

- 两节点 P2P。
- 多输入/多输出复杂交互。
- 交易冲突、double spend、错误 witness、错误 cell dep 的负例。
- 多个 CellScript 案例合约。
- scheduler/MPE trusted summary 行为。

## 5. Bootstrap 和钱包验收

### 5.1 Bootstrap manifest

bootstrap helper 输出：

```json
{
  "network": "devnet",
  "wallet": {
    "name": "acceptance",
    "mnemonic": "test only ...",
    "default_address": "spora...",
    "public_key": "hex",
    "secret_key": "hex",
    "derivation_path": "m/44'/..."
  },
  "node": {
    "prealloc_cells": 101,
    "prealloc_amount_sau": 10000000000
  }
}
```

要求：

- 默认写入 `target/devnet-acceptance/<run-id>/bootstrap.json` 或脚本提供的临时目录。
- manifest 必须包含足够信息让测试重建签名 key。
- manifest 必须在顶部标注只用于 devnet/simnet。
- 不得默认修改用户真实钱包。

### 5.2 钱包验收项

- 能生成 12 或 24 词 devnet mnemonic。
- 能生成和网络匹配的标准地址。
- 能用同一个 mnemonic 重建同一个地址。
- 能把该地址传给 `--prealloc-address` 启动节点。
- 能用该地址对应 key 签名预分配 cell 的消费交易。
- 负例：错误网络地址、缺失 `--prealloc-address`、缺失 `--num-prealloc-cells` 都必须失败。

## 6. Genesis / Prealloc / Cellindex 验收

### 6.1 启动成功

启动后通过 RPC 检查：

- `get_info` 成功。
- 节点网络为 devnet。
- `is_cell_indexed == true`。
- `mempool_size == 0`。
- 初始 block count 和 virtual DAA 状态符合 devnet 预期。

### 6.2 预分配资金可见

对 bootstrap 地址调用 cellindex RPC：

- `get_cells_by_address` 返回 101 个 cell。
- 总 capacity 等于 `101 * prealloc_amount_sau`。
- 每个 outpoint 唯一。
- lock hash 与地址标准 lock script 匹配。
- cell data hash 为空数据的预期 hash 或当前实现的默认 data hash。

### 6.3 预分配资金可花费

- 挖足够块推进 virtual DAA。
- 使用 `fetch_spendable_cells` 或等价逻辑过滤可花费 cell。
- 至少消费 1 个预分配 cell。
- 至少消费多个预分配 cell 构造多输入交易。

## 7. Mining 验收

### 7.1 单节点挖矿

步骤：

1. 调用 `get_block_template(miner_address, extra_data)`。
2. 校验 coinbase 输出地址是 miner address。
3. 调用 `submit_block(template.block, false)`。
4. 等待 `VirtualDaaScoreChanged`。
5. 校验 block count、virtual selected parent、accepted transaction ids。

通过标准：

- 连续挖 10 块无错误。
- 每个 block 都能被 virtual chain 接受。
- event 通知和最终 RPC 状态一致。

### 7.2 Coinbase maturity

步骤：

- 挖到当前网络 `coinbase_maturity`。
- 查询 miner address 的 coinbase cell。
- maturity 前不可花费，maturity 后可花费。

通过标准：

- 未成熟 coinbase 消费交易被拒绝。
- 成熟 coinbase 消费交易可进入 mempool 并被挖出。

### 7.3 模板包含交易

步骤：

1. 提交一笔合法转账。
2. `get_mempool_entry(txid)` 成功。
3. `get_block_template` 返回的非 coinbase 交易包含该 txid。
4. `submit_block` 后 mempool 中不再存在该 entry。

## 8. 转账和复杂交易验收

### 8.1 标准转账

步骤：

- 从预分配地址转账到 recipient A。
- 输出 change 回预分配地址。
- 挖出交易。
- 查询 recipient A 的 cell。

通过标准：

- recipient A 的新增 capacity 精确匹配转账金额。
- sender 总余额减少金额和 fee。
- change cell 出现在 sender 地址下。
- 交易进入 block 后，输入 cell 不再出现在可花费集合。

### 8.2 多输入/多输出

步骤：

- 选择多个预分配 cell 作为输入。
- 构造至少 4 个输出：recipient A、recipient B、recipient C、change。
- 提交并挖出。

通过标准：

- 每个 recipient 都能通过 cellindex 查到正确输出。
- 输入输出 capacity 守恒，fee 非负。
- block template 和 accepted transaction ids 都包含该交易。

### 8.3 交易链和 mempool 依赖

步骤：

- 构造 parent tx 消费预分配 cell。
- 构造 child tx 消费 parent 输出。
- 先提交 parent，再提交 child。
- 挖出一个 block。

通过标准：

- mempool 接受 parent/child 依赖。
- 如果 template 同时选择 parent 和 child，则 block 内交易拓扑顺序正确；如果当前 template 只选择 ready parent，则 parent accepted 后下一块 template 必须提升 child。
- child 输出最终被 cellindex 索引。

当前状态：已进入 `devnet_acceptance_smoke`，并由 `smoke-report.json.parent_child_mempool_confirmed` 记录。当前实现允许 child 进入 mempool；block template 至少必须在 parent accepted 后的下一块提升 child，同块 parent+child CPFP 打包可作为后续 template 优化。

### 8.4 负例

必须覆盖：

- double spend。
- 错误签名。
- 输入不存在。
- 输出 capacity 超过输入 capacity。
- lock script 与签名 key 不匹配。
- cell dep 缺失或 code hash 错误。

所有负例都必须在 mempool 或 block validation 阶段被拒绝，并断言错误类型或错误文本中的稳定关键字。

## 9. VM 验收

验收目标是证明 VM 不是只在单元测试中运行，而是在真实交易验证路径中运行。

### 9.1 基础 VM fixture

优先使用仓库内已有 fixture 或测试 helper：

- always success。
- always failure。
- timelock / since。
- load input。
- load output。
- load cell data。
- load dep cell data。
- load header timestamp / DAA。
- secp standard lock。

### 9.2 VM 成功路径

当前 smoke 已实现：

- 用预分配资金部署 always-success ELF 为 code cell。
- 创建使用该 code hash 的 VM-locked cell。
- 后续交易通过 `CellDep::Code` 引用 code cell，执行 VM lock script。
- 交易完整经历 submit -> mempool -> template -> block -> cellindex。
- 用 CellScript 编译 no-op Spora ELF，部署为独立 code cell，创建 CellScript-locked cell，并用 `CellDep::Code` 在真实交易验证路径中执行该 artifact。

步骤：

1. 部署或引用 VM code cell。
2. 构造使用该 code hash 的 lock/type script。
3. 提供正确 witness / args / dep。
4. 提交交易并挖出。

通过标准：

- mempool 接受。
- block template 包含。
- submit block 成功。
- 输出 cell 被索引。

### 9.3 VM 失败路径

当前 smoke 已实现：

- 同一 VM-locked spend 在缺失 code cell dep 时必须被 RPC/mempool 拒绝。
- 同一 CellScript-locked spend 在缺失 compiled artifact code cell dep 时必须被 RPC/mempool 拒绝。

步骤：

- 对每类 VM fixture 构造至少一个错误 witness、错误 args 或错误 dep。

通过标准：

- 交易不能进入 mempool，或 block submit 被拒绝。
- 错误能定位到 script execution / VM validation，不是测试 harness panic。

## 10. CellScript 编译验收

### 10.1 Spora profile artifact

每个案例合约必须执行：

```bash
cargo run -p cellscript --bin cellc -- \
  cellscript/examples/token.cell \
  --target riscv64-elf \
  --target-profile spora \
  -o "$RUN_DIR/artifacts/token.elf"

cargo run -p cellscript --bin cellc -- \
  verify-artifact "$RUN_DIR/artifacts/token.elf" \
  --metadata "$RUN_DIR/artifacts/token.elf.meta.json"
```

通过标准：

- 产物是 Spora profile。
- metadata 中 `target_profile.name == "spora"`。
- scheduler witness 使用 Molecule ABI。
- 不接受 legacy Borsh scheduler witness 作为公开 metadata。
- artifact verify 通过。

### 10.2 CKB profile 非回归检查

虽然本计划重点是 Spora devnet，仍要保证 CellScript 的 CKB profile 没被 Spora devnet 改动破坏：

```bash
cargo run -p cellscript --bin cellc -- \
  cellscript/examples/token.cell \
  --target riscv64-elf \
  --target-profile ckb
```

通过标准：

- CKB profile 编译/元数据检查不因 Spora devnet 验收代码退化。
- CKB artifact/package 相关 fail-closed 行为保持明确，不被 Spora artifact 路径错误复用。

### 10.3 CKB 本地集成 devnet 验收

CKB profile 还必须用父目录 CKB 仓库跑真实本地节点验收。入口：

```bash
scripts/ckb_cellscript_acceptance.sh
```

默认行为：

- 使用 `../ckb/test/template` 复制出临时 CKB integration devnet；该模板是 Dummy PoW、本地测试链，开启 `IntegrationTest` RPC，并带 always-success system cell。
- 如果 `CKB_BIN` 未指定且 `../ckb/target/debug/ckb` 不存在，脚本会在父目录执行 `cargo build --bin ckb`。
- 编译一个纯 no-arg CellScript baseline，以及固定 7 个 bundled examples：`amm_pool.cell`、`launch.cell`、`multisig.cell`、`nft.cell`、`timelock.cell`、`token.cell`、`vesting.cell`。
- bundled examples 的通用链上 spend 使用 acceptance-only smoke entry，因为大多数 strict business artifact 仍需要 action-specific witness、Input cell data、Output cell data 和依赖 wiring。smoke 路径会复制完整 examples 目录并追加 `action main() -> u64 { 0 }`，同时设置 `CELLSCRIPT_CKB_ACCEPTANCE_SMOKE_ALLOW_UNPORTABLE_EXAMPLES=1`。编译器只在 env 值精确为 `1`、target profile 是 `ckb`、且模块存在 no-arg `main() -> u64` 时接受该 bypass。这只证明 artifact packaging、CKB-VM 入口、code-cell dep resolution 和 lock-script invocation；原始业务 action 的 strict CKB lowering/sidecar verification 仍单独记录。
- `token.cell` 已升级为 action-specific CKB on-chain harness：脚本为 `mint`、`transfer_token`、`burn`、`merge` 分别生成 strict CKB artifact，部署真实 code cell，构造 `CSARGv1\0` witness、typed input cell data、typed output cell data、type/dependency wiring，并提交 valid transaction；每个 action 还构造 malformed transaction，必须被脚本逻辑拒绝。
- 调用 `cellc verify-artifact --expect-target-profile ckb` 校验每个 sidecar。
- 硬校验每个 artifact 是 ELF，且没有 Spora `SPORABI` trailer。
- 启动真实 CKB 节点，通过 JSON-RPC `generate_block` 出块，使用 CKB `send_test_transaction` 构造三段链上流程：
  - always-success cellbase -> CellScript ELF code cell；
  - always-success cellbase -> CellScript-locked probe cell；
  - 缺失 CellScript code cell dep 的 malformed spend 必须被 `dry_run_transaction` 拒绝为脚本解析失败；
  - 带 CellScript code cell dep 的合法 spend 必须先通过 `dry_run_transaction` 并记录 cycles；
  - CellScript-locked probe cell + code cell dep -> always-success recipient cell。
- 生成 `target/ckb-cellscript-acceptance/<run-id>/ckb-cellscript-acceptance-report.json`，记录 CKB repo/bin、每个 artifact metadata、CKB Blake2b data hash、部署 tx、创建 tx、spend tx、live-cell 查询、malformed spend 负例和 tip header。

可选 compile-only 兜底：

```bash
scripts/ckb_cellscript_acceptance.sh --compile-only
```

compile-only 模式不要求父目录存在 CKB checkout，也不会解析或构建 CKB binary；它用于 CI/release gate 中证明 CKB-profile artifact packaging、sidecar、固定 7 个 example smoke matrix 和 `SPORABI` trailer 边界。完整链上模式仍必须使用父目录 CKB 本地 devnet。

通过标准：

- `bundled_examples_count == 7`，且 `bundled_examples_exact_order == ["amm_pool.cell", "launch.cell", "multisig.cell", "nft.cell", "timelock.cell", "token.cell", "vesting.cell"]`。
- 每个 artifact 的 `target_profile == "ckb"`，`verify.expected_target_profile_verified == true`。
- 每个 artifact 以 ELF magic 开头，且 `artifact_has_sporabi_trailer == false`。
- compile-only 和完整模式都必须记录 bundled examples 的 `strict_original_ckb_compile.status`；strict original 通过的 example 必须进入 `bundled_examples_strict_admitted` 且其 strict sidecar 必须通过 `verify-artifact`，仍 fail-closed 的 example 必须进入 `strict_original_ckb_compile_policy_fail_closed`。通用链上 spend 仍使用 smoke artifact，并记录在 `bundled_examples_smoke_bypass`。`strict_original_ckb_compile_unexpected_failures == []`，避免 backend/import/panic 类问题被 smoke bypass 掩盖。
- 完整模式下 `onchain.status == "passed"`，`onchain.all_artifacts_deployed_and_spent == true`，并且每个 artifact 的 code cell、locked cell、spend recipient 都通过 CKB `get_live_cell` 查询为 live。
- 完整模式下每个 artifact 的 `malformed_spend_without_code_dep.status == "rejected"`，`policy_or_capacity_reason == false`，且拒绝后 `locked_cell_live_after_malformed_spend == true`。
- 完整模式下每个 artifact 的 `valid_spend_dry_run` 必须存在，证明合法 CellScript locked-cell spend 在提交前可被 CKB-VM 预执行。

边界：

- 这是 CKB 本地 integration devnet 验收，不是 mainnet/testnet 兼容声明。
- 当前证明 v1 pure baseline 和 bundled example smoke artifact 能被真实 CKB 节点加载、作为 code cell 依赖、执行并花费；strict-admitted bundled examples 另行证明 strict original CKB compile + sidecar verification。`token.cell` 的 `mint`、`transfer_token`、`burn`、`merge` 已有真实 CKB action transaction harness；其余复杂 stateful CellScript 业务合约的原始 action 链上执行仍以 fail-closed/post-v1 builder 范围管理。
- 该脚本只修改 Spora `target/` 临时目录，不修改父目录 CKB 仓库；父目录只用于读取模板和构建/运行 `ckb`。
- 该验收不能替代 Spora profile 验收；每次修复 CKB 路径后仍需跑 Spora `cellscript`/smoke 回归，确保双 profile 没有互相污染。

2026-04-21 首次完整运行结果：

- CKB repo：`/Users/arthur/RustroverProjects/ckb`
- CKB binary：`/Users/arthur/RustroverProjects/ckb/target/debug/ckb`
- CKB version：`ckb 0.206.0 (5ebbc39 2026-04-10)`
- compile-only report：`/Users/arthur/RustroverProjects/Spora/target/ckb-cellscript-acceptance/20260421-101222-13883/ckb-cellscript-acceptance-report.json`
- full on-chain report：`/Users/arthur/RustroverProjects/Spora/target/ckb-cellscript-acceptance/20260421-101345-16333/ckb-cellscript-acceptance-report.json`
- artifact size：`5576` bytes
- CKB data hash：`0x7e475809edfbdb2affb7b87afb891f7407af90297bdedbda8e67d423ae3d5259`
- code cell deploy tx：`0x0cb86bb39b56b2bc722c36c84faf672d6524459867cc7ffcd655ea2b534b438d`
- locked probe cell create tx：`0x1d8516b13b7c7dde9b45dbdd3a8436f8fa94af922b436ab9e8137d72493e552d`
- CellScript locked-cell spend tx：`0x8869f7e24e2993ed046256846527272bd9540b352ff63be59522eb24ccc43cf4`
- report 关键字段：`status = passed`、`onchain.status = passed`、`artifact_has_sporabi_trailer = false`、`code_cell_live = true`、`locked_cell_live = true`、`spend_recipient_live = true`

本轮寻找问题式验收暴露并修复了一个脚本构造问题：

- 初版脚本用单个 CKB integration cellbase reward 部署 5.5KB CellScript ELF code cell，被 CKB tx-pool 正确拒绝为 `InsufficientCellCapacity(Outputs[0])`。这说明本地 CKB 节点确实在执行 CKB occupied-capacity 规则，而不是只做空转 RPC。
- 修复后脚本按 CKB occupied-capacity 需求自动收集多个 always-success cellbase 输入来部署 code cell；创建小的 CellScript-locked probe cell 仍使用单个输入。该修复只在验收脚本内构造 CKB 测试交易，不改变 Spora/CellScript 编译器或 Spora profile 行为。
- 同轮回归：`scripts/devnet_acceptance.sh --profile cellscript --keep-artifacts` 通过，artifact 目录为 `/Users/arthur/RustroverProjects/Spora/target/devnet-acceptance/20260421-101613-23089`，`cellscript = passed`。

2026-04-21 收口：

- `scripts/cellscript_phase4_release_gate.sh quick/full/v1` 已接入 `scripts/ckb_cellscript_acceptance.sh --compile-only`。CI/release gate 现在会检查 CKB-profile ELF、sidecar target profile、无 `SPORABI` trailer；但不会要求 CI runner 旁边有父目录 CKB checkout。脚本现在从 Cargo JSON artifact 读取真实 `cellc` executable，避免因 target-dir、并发构建或路径猜测导致假阴性。
- `.github/workflows/cellscript-v1.yml` 已把 `scripts/ckb_cellscript_acceptance.sh` 和本计划文档加入触发路径。
- `scripts/cellscript_phase4_release_gate.sh quick` 通过。
- `scripts/cellscript_phase4_release_gate.sh v1` 通过。
- `CKB_REPO=/tmp/nonexistent-ckb-for-compile-only scripts/ckb_cellscript_acceptance.sh --compile-only` 通过，证明 compile-only 不依赖父目录 CKB。
- `scripts/ckb_cellscript_acceptance.sh` 完整链上模式复跑通过，报告为 `/Users/arthur/RustroverProjects/Spora/target/ckb-cellscript-acceptance/20260421-104620-88056/ckb-cellscript-acceptance-report.json`，`onchain.status = passed`。
- 补充 malformed 缺失 code-cell dep 负例后，`scripts/ckb_cellscript_acceptance.sh` 完整链上模式再次通过，报告为 `/Users/arthur/RustroverProjects/Spora/target/ckb-cellscript-acceptance/20260421-105139-823/ckb-cellscript-acceptance-report.json`。CKB `dry_run_transaction` 返回 `ScriptNotFound`，报告中 `malformed_spend_without_code_dep.status = rejected`、`policy_or_capacity_reason = false`、`locked_cell_live_after_malformed_spend = true`。
- 扩展到全部 bundled examples 并硬化 strict-original 边界后，`scripts/cellscript_phase4_release_gate.sh v1` 内的 CKB compile-only 验收通过，报告为 `/Users/arthur/RustroverProjects/Spora/target/ckb-cellscript-acceptance/20260421-115713-21130/ckb-cellscript-acceptance-report.json`。
- 扩展到全部 bundled examples 并硬化 strict-original 边界后，`scripts/ckb_cellscript_acceptance.sh` 完整链上模式通过，报告为 `/Users/arthur/RustroverProjects/Spora/target/ckb-cellscript-acceptance/20260421-115555-16636/ckb-cellscript-acceptance-report.json`。该报告中 8 个 artifact 均完成 code-cell deploy、locked probe create、malformed missing-dep dry-run reject、valid spend dry-run、actual spend commit；7 个 bundled examples 均出现在 `onchain.bundled_examples_deployed_and_spent`。
- strict original example 编译边界已经硬化：报告必须给出 `strict_original_ckb_compile_policy_fail_closed`，并保持 `strict_original_ckb_compile_unexpected_failures = []`。这保证原始复杂业务 action 暂未进入 CKB v1 admitted subset 时只能按 target-profile policy fail-closed，不能因为 import/backend/codegen/panic 类非预期错误被 smoke artifact 路径掩盖。
- 本轮 CKB example matrix 暴露并修复了真实问题：CKB GroupInput 64-bit source 常量需要内置 assembler 支持 64-bit `li`；ELF `_start` 必须通过 `_cellscript_entry` tail-call 选择 no-arg `main`，不能误执行第一个带参业务 action；跨 example helper 调用必须生成 fail-closed unresolved-call stub，避免 `launch.cell` 因外部 linker undefined symbol 落入内置 assembler 异常路径；内置 ELF assembler 的 LOAD segment 布局改为 CKB/GNU linker 风格。以上修复不改变 Spora profile 的 syscall、hash、scheduler witness 或 `SPORABI` packaging。
- 后续 strict-admission 收口：`token.cell` 的 strict original artifact 已不再被 CKB profile policy 拒绝，报告会把它列入 `bundled_examples_strict_admitted` 并校验 strict sidecar。`token.cell` business action 的链上执行已由 action-specific CKB harness 覆盖 `mint`、`transfer_token`、`burn`、`merge`。修复点包括 resource conservation classifier 能识别 `amount` 求和合并加固定 identity 字段复制，并要求 source 中有显式 equality guard，例如 `a.symbol == b.symbol`；没有 equality guard 的多字段资源合并仍保持 `runtime-required` 并被 CKB profile 拒绝。

2026-04-21 action-specific CKB harness 暴露并修复：

- `&mut` schema 参数的 prelude 只验证 mutate input/output，没有把参数绑定到 loaded Input cell data，导致后续字段访问仍使用 null entry ABI length；现已在 mutate prelude 中重绑真实 input data。
- `destroy` 操作没有进入 consumed schema pointer binding，`burn(token)` 中的 `token.amount` 会读取 null ABI length；现已把 `Destroy` 纳入 consumed operand 绑定。
- destroy absence scan 使用静态 CellScript type hash 且 syscall loop 结束后保留 `INDEX_OUT_OF_BOUND` 返回码；现已改为读取 consumed Input 的真实 CKB TypeHash，扫描 transaction Outputs，并在成功结束时清零返回码。

## 11. CellScript 合约部署和使用验收

当前 smoke 已覆盖最小闭环：

- `testing/integration/src/common/cellscript_contracts.rs` 编译 no-op `action main() -> u64 { return 0 }` 为 Spora RISC-V ELF。
- helper 校验 artifact hash、Spora profile、SPORABI/Molecule VM ABI metadata、standalone runner compatibility 和 fail-closed feature 集。
- devnet smoke 将 artifact bytes 写入 code cell，另建 CellScript-locked cell，并验证缺 dep 失败、带 dep 成功。
- devnet smoke 另部署 fixed-output `Marker { amount: 42 }` schema verifier，并提交真实成功 spend；输出 data 必须是 `42u64` little-endian bytes，cellindex 必须记录 8-byte data hash。
- devnet smoke 另部署 parameterized `Marker { amount }` schema verifier，并提交真实成功 spend；witness 使用 `CSARGv1\0` magic 加 little-endian u64 payload，生成的 ELF wrapper 必须把该 payload 放入 action ABI register，输出 data 必须是同一个 u64。
- 参数化 witness bytes 不能再由验收测试局部手写；应通过 CellScript metadata 公共 API 生成。当前公共 API 会省略 schema/cell-backed 参数，按源码顺序编码 scalar/fixed-byte payload，校验 fixed-byte 长度和 a0-a7 ABI 上限。
- `cellc entry-witness` 已提供命令行 witness builder：可按 `--action`/`--lock` 选择入口，`--arg` 按 metadata 参数类型推断编码，默认输出 hex，`--json` 输出 `cellscript-entry-witness-v1` 摘要，`--output` 写 raw witness bytes；focused CLI 测试覆盖 u64 参数和 schema-backed 参数省略。
- deprecated `SchedulerMetadata::generate_legacy_borsh` 已移除；scheduler witness public path 只保留 Molecule。保留的 `scheduler_witness_molecule_hex` / `scheduler_witness_borsh_hex` sidecar 字段仅用于 crate 内旧 metadata 兼容校验，不再作为 public Rust API 字段或标准库生成入口。
- smoke 在 relaxed mass policy 下逐个部署全部 bundled example ELF，并验证每个 code cell 被 cellindex 查到。
- `all_spora_examples_compile_metadata_acceptance` 会用 file/package-aware `compile_file` 路径编译所有 bundled examples，覆盖跨示例类型依赖；此前裸 source 编译会暴露 `amm_pool.cell` 找不到 `Token` 的问题。
- focused `cellscript --test examples` 已覆盖 token mint/transfer/burn/merge、NFT mint/transfer/burn、timelock release/extend、vesting grant/claim/revoke、multisig propose/sign/execute/cancel 等 action-specific metadata、Molecule scheduler witness、runtime input requirement 和 verifier obligation。当前业务示例的 malformed on-chain spend 预期失败；完整经济学成功路径仍需要 post-v1 action transaction/witness builder。
- `smoke-report.json` 记录每个 bundled example 的 artifact size、部署 tx id、code cell outpoint、locked probe outpoint、malformed spend 拒绝原因，并断言拒绝原因不是 standard/mass policy，也不是 VM cycles limit。
- 带参数 action/lock 的 ELF `_start` 当前通过 `_cellscript_entry` wrapper 支持最小 witness 参数 ABI：标量和固定字节参数从 GroupInput witness 解出，schema/cell-backed 参数仍由运行时 cell 数据路径加载；不支持的参数形状、ABI register 超过 a0-a7、缺失/错误 magic 的 malformed spend 都必须 fail-fast 退出。当前所有 bundled examples 的 malformed spend 均以 `Script exited with code 25` 拒绝，避免把 VM 进程 argv/未初始化寄存器误当业务参数执行。
- schema verifier/output data 校验必须使用 `LOAD_CELL_DATA`，不能把完整 `LOAD_CELL` / CellOutput bytes 按 schema offset 解释；compiler assembly tests 和 smoke 正例都覆盖该约束。
- 内置 ELF assembler 的 text user-section PC 计算必须扣除 `_start` trampoline bias；2026-04-21 smoke 曾暴露该 bug 会把 wrapper branch 编成错误方向并让 `launch.cell` malformed spend 跑到 cycles limit，现已修复并由 smoke 禁止回归。

### 11.1 通用部署模型

当前 no-op CellScript 合约和 bundled examples 使用以下部署模型：

1. 编译 `.cell` 为 Spora ELF artifact 和 metadata。
2. 用预分配资金创建 code cell，data 为 artifact bytes。
3. 挖出部署交易。
4. 通过 cellindex 查询 code cell。
5. 计算 code hash / data hash。
6. 后续合约交易通过 `CellDep` 引用该 code cell。
7. 合约 action 交易带上 metadata 生成的 Molecule scheduler witness。

通过标准：

- 部署交易是真实交易，不直接注入 state。
- code cell 可通过 RPC 查询。
- no-op 合约后续 action 交易缺失 cell dep 时失败，提供正确 cell dep 时成功。
- bundled examples 的 malformed action probe 提供正确 cell dep 但缺业务 witness/参数，必须失败。

### 11.2 Token 合约

文件：`cellscript/examples/token.cell`

场景：

1. `mint`
   - 创建 MintAuthority。
   - 铸造 Token cell 给 owner。
   - 验证 supply、symbol、owner、capacity。
2. `transfer_token`
   - owner 把一部分 token 转给 recipient。
   - 验证 sender/recipient token amount 和 cell data。
3. `merge`
   - 合并两个同 symbol/token id 的 Token cell。
   - 验证 amount 求和。
4. `burn`
   - 销毁部分 token。
   - 验证 supply 或 receipt 状态变化。

负例：

- 非 authority mint。
- 超过 max supply。
- 不同 symbol merge。
- transfer amount 大于 balance。
- scheduler witness 被篡改。

v1 当前状态：

- `token_mint_authority_mutation_is_explicit` 覆盖 `mint` 的 `MintAuthority` mutate input/output、`minted` transition、type/lock hash preservation、Molecule scheduler witness mutate access。
- `compile_token_spora_example_contract` 覆盖 `transfer_token` 的 consume/create output relation、lock rebinding、destination address binding 和 resource conservation metadata。
- `burn` / `merge` 的 destroy / conservation lowering 已由 CellScript compiler 单测和 bundled metadata 编译路径覆盖；完整 token economic transaction builder 仍属于 post-v1。

### 11.3 NFT 合约

文件：`cellscript/examples/nft.cell`

场景：

- mint 一个 NFT receipt/resource。
- transfer 给新 owner。
- update metadata 或证明 metadata immutable 规则。
- burn 或 revoke，如果合约支持。

负例：

- 重复 mint 同一 NFT id。
- 非 owner transfer。
- 错误 metadata hash。

v1 当前状态：

- `nft_core_actions_expose_action_specific_builder_metadata` 覆盖 `mint` 创建 `NFT` 并 mutate `Collection.total_supply`、`transfer` mutate `NFT.owner`、`burn` destroy `NFT`。
- 测试断言对应 runtime requirements：create-output fields、mutable-cell transition、destroy input data、destroy output absence。
- CKB acceptance 已补充 fixed-width NFT `transfer` / `burn` 的 action-specific on-chain harness：真实部署 strict CKB artifact，构造 NFT cell data、witness、Input/Output、CellDep，成功路径必须 commit，malformed owner / recreated TypeHash 必须被脚本拒绝。`mint` 仍依赖 `Collection` 的动态 `String` 字段和完整 action transaction/witness builder，保留 post-v1。

### 11.4 Timelock 合约

文件：`cellscript/examples/timelock.cell`

场景：

- 创建 timelock cell。
- maturity 前 claim 失败。
- 挖矿推进 DAA 或 header/time。
- maturity 后 claim 成功。

负例：

- 错误 unlock path。
- header dep 缺失。
- witness 中的时间条件与链上状态不一致。

v1 当前状态：

- `timelock_core_actions_expose_time_and_release_metadata` 覆盖 `create_absolute_lock` 创建 `TimeLock`、`execute_release` destroy `TimeLock`/`LockedAsset`/`ReleaseRequest` 并创建 `ReleaseRecord`、`extend_lock` mutate `TimeLock.unlock_height`。
- devnet smoke 真实推进 DAA，并用 scheduler tamper 负例确认 witness policy 生效。
- 完整 maturity 前失败 / maturity 后成功的 action transaction builder 仍属于 post-v1。

### 11.5 Vesting 合约

文件：`cellscript/examples/vesting.cell`

场景：

- 创建 vesting config。
- 创建 grant。
- cliff 前 claim 失败。
- 挖矿推进到 cliff 后部分 claim。
- 到期后 full claim。
- 如支持 revoke，则测试 revoke 后 recipient 和 admin 的余额关系。

注意：

- 该合约依赖时间/DAA 语义。Spora profile 必须用 Spora 当前 VM/env API 验证；CKB profile 的 DAA 语义不能直接复用。
- `vesting_phase2_remaining_obligations_are_explicit` 当前覆盖 `create_vesting_config`、`grant_vesting`、`claim_vested`、`revoke_grant` 的 action metadata、DAA/time context、claim witness runtime input requirement、生命周期 transition 和 fail-closed debt。
- 完整 cliff 前失败 / cliff 后部分 claim / 到期 full claim 的链上成功交易仍属于 post-v1 builder。

### 11.6 Multisig 合约

文件：`cellscript/examples/multisig.cell`

场景：

- 创建 threshold config。
- 少于 threshold 的签名提交失败。
- 满足 threshold 的签名提交成功。
- signer 顺序变化不影响合法性，重复签名不能绕过 threshold。

v1 当前状态：

- `multisig_core_actions_expose_threshold_lifecycle_metadata` 覆盖 `create_wallet`、`propose_transfer`、`add_signature`、`execute_proposal`、`cancel_proposal` 的 create/mutate/destroy metadata 和 runtime input requirements。
- threshold signature 顺序、重复签名拒绝和执行路径仍需要 post-v1 action transaction/witness builder。

### 11.7 AMM / Launch 合约

文件：`cellscript/examples/amm_pool.cell`、`cellscript/examples/launch.cell`

v1 验收建议先做 bounded coverage：

- 编译和 metadata verify。
- 部署 code cell。
- 构造最小 create/init action。
- 验证 scheduler witness 和 resource/shared touch set。

完整经济学路径保留为 post-v1 full suite：

- add liquidity。
- swap。
- remove liquidity。
- launch claim/settle。
- conservation 和 price invariant 检查。

## 12. CellScript Scheduler / MPE 验收

### 12.1 Trusted summary 正例

步骤：

- 从 CellScript metadata 中提取 Spora Molecule scheduler witness。
- 通过 wallet generator 或测试 helper 附加到 final transaction。
- 提交两个互不冲突的 action transaction。

通过标准：

- trusted summary 被解析。
- mempool/template 接受。
- 两笔交易能在同一个 block 或合理顺序中被接受。

### 12.2 冲突检测

步骤：

- 构造两个触碰同一 shared/resource key 的 action。
- 第一个进入 mempool。
- 第二个提供冲突 summary。

通过标准：

- 当前策略若要求串行，第二个不能并行进入同一 batch。
- 若策略允许 mempool 共存，block template 必须给出可执行顺序。
- 不允许错误地把冲突交易标记为独立并行。

### 12.3 Witness 篡改

步骤：

- 修改 scheduler witness 的 version、effect class、access source 或 key hash。

通过标准：

- metadata/wallet generator 层拒绝无效 Molecule。
- 若绕过 generator 直接提交交易，mempool 或 VM validation 层拒绝。

## 13. 包管理工具验收

CellScript 已有 package manager 能力后，devnet 验收必须覆盖“合约项目”而不是只编译单文件。

### 13.1 Package init/build/check

步骤：

```bash
mkdir -p "$RUN_DIR/packages"
cd "$RUN_DIR/packages"

cargo run -p cellscript --bin cellc -- init acceptance-token --lib
cd acceptance-token
cargo run -p cellscript --bin cellc -- check --target-profile spora
cargo run -p cellscript --bin cellc -- build --target-profile spora
cargo run -p cellscript --bin cellc -- info --json
cargo run -p cellscript --bin cellc -- entry-witness --action main --arg 77 --json
```

通过标准：

- `Cell.toml` 被创建。
- `check` 通过。
- `build` 输出 artifact 和 metadata。
- `info --json` 可被验收脚本解析。
- `entry-witness --json` 可从 package metadata 生成 `_cellscript_entry` witness bytes，交易构造脚本不需要手写 `CSARGv1\0` payload。

### 13.2 Local dependency

步骤：

- 创建 `token-lib` package。
- 创建 `vesting-app` package。
- `vesting-app` 通过 `cellc add --path ../token-lib token-lib` 引用本地依赖。
- build 后检查 lockfile。

通过标准：

- lockfile 记录 local dependency。
- source hash 或 package hash 稳定。
- 修改依赖源码后，build/check 能检测变化。

### 13.3 Policy

通过标准：

- devnet 验收默认禁止 registry dependency。
- 允许 local path dependency。
- git dependency 若启用，必须 pin revision。

## 14. 两节点和传播验收

### 14.1 P2P block propagation

步骤：

- 启动 node A 和 node B。
- node B `--connect` 到 node A。
- node A 挖 5 个 block。
- node B 等待同步。

通过标准：

- 两节点 selected tip 一致。
- block count 一致。
- virtual DAA 一致。

当前状态：`scripts/devnet_acceptance.sh --profile propagation` 运行 `daemon_cells_propagation_test`，覆盖两节点连接、block relay、selected tip/block count/virtual DAA 对齐。该测试从第一块真实生成的 block 开始查询 virtual chain，不把 simnet genesis 当作普通 status-visible block 计数。

### 14.2 Transaction propagation

步骤：

- 向 node A 提交一笔合法交易。
- 等待 node B mempool 出现该 txid。
- node B 挖 block。
- node A 接受该 block。

通过标准：

- 交易能跨节点传播。
- node B 接受 node A 挖出的交易块后，双方 cellindex 和余额查询一致。

当前状态：同一 propagation profile 覆盖 node A 提交/挖出交易、node B 接受 block、双方 CellsChanged 通知与余额一致。验收不假设 coinbase 出现在 `accepted_transaction_ids` 中，只要求 acceptance data 的 block hash 与新增 block 一致。
- 任一节点挖出的 block 都能被另一节点接受。
- cellindex 最终一致。
- address-scoped CellsChanged 必须只返回订阅地址相关的 CellDiff，且 devnet/simnet/testnet/mainnet 前缀不应导致过滤误伤。

## 15. 报告和产物

每次 full acceptance 生成：

```text
target/devnet-acceptance/<timestamp>-<pid>/
  bootstrap.json
  bootstrap.stdout.json
  sporad.log
  probe-report.json
  probe.stdout.json
  smoke-report.json
  propagation-report.json
  cellscript-report.json
  acceptance-report.json
  wallet/
    wallet.json
```

`acceptance-report.json` 当前包含：

- profile。
- run id。
- run directory。
- generated UTC timestamp。
- completed UTC timestamp。
- invocation command。
- git revision。
- git dirty flag。
- bootstrap manifest path。
- generated prealloc address。
- smoke report path。
- external boot/probe report path。
- propagation report path。
- cellscript report path。
- sporad log path。
- smoke / external_boot / propagation / cellscript pass/fail summary。
- structured `profiles` summary。
- overall result。
- failed step、exit code 和脚本行号；成功时这些字段为 `null`。

失败路径也必须写出 partial `acceptance-report.json`。例如 `external_boot` 中 sporad 启动失败时，报告仍会保留已经存在的 bootstrap/probe/log 路径、各 profile 状态、`result: failed`、`failed_step: external_boot`、`failure_exit_code` 和 `failure_line`，便于 CI artifact 和本地验收复盘。

`smoke-report.json` 承载交易级细节，包括 signed transfer、多输入多输出、parent/child mempool、scheduler tamper、VM spend、CellScript no-op spend、CellScript schema-output spend，以及每个 bundled example 的 artifact size、deployment tx、code cell outpoint、locked probe outpoint、indexing 状态和 malformed spend 拒绝原因。

`propagation-report.json` 承载两节点 propagation focused suite 清单，包括 block relay、transaction acceptance、cellindex consistency 和 address-scoped CellsChanged notifications。

`bootstrap.json` 和 `bootstrap.stdout.json` 承载 generated mnemonic、derivation path、public/secret key、prealloc cell count 和 amount。该钱包只用于 devnet/simnet 验收，不能导入或资助主网。

`cellscript-report.json` 承载 focused CellScript/package-manager 测试清单。

## 16. CI 策略

### 16.1 PR gate

PR gate 只跑快速套件：

```bash
scripts/devnet_acceptance.sh --profile smoke
./scripts/cellscript_phase4_release_gate.sh quick
```

当前 `.github/workflows/spora-devnet-acceptance.yml` 已把 smoke profile 接入 PR/push gate。
path filter 覆盖 CellScript、consensus、exec、indexes、mining、notify、protocol、RPC、scripts、simpa、sporad、state、testing、treasure_boy、wallet 等会影响 devnet/VM/cellindex/notification 验收的路径。

### 16.2 Nightly gate

Nightly 跑 full suite：

```bash
scripts/devnet_acceptance.sh --profile full
```

当前 `.github/workflows/spora-devnet-acceptance.yml` 已配置 nightly full profile。

### 16.3 Release gate

Release 前必须跑：

```bash
scripts/devnet_acceptance.sh --profile full --keep-artifacts
cargo test -p spora-testing-integration --lib \
  --features "integration-tests devnet-prealloc vm" \
  -- --test-threads=1
```

手动 release 验收可通过 GitHub Actions `Spora Devnet Acceptance` workflow_dispatch 选择 `full` profile；需要局部复验时也可以选择 `smoke`、`external-boot`、`propagation` 或 `cellscript`。

## 17. 验收通过标准

v1 通过必须同时满足：

- bootstrap helper 能非交互生成 devnet mnemonic、地址、manifest。
- 用 manifest 地址启动的真实 devnet 节点能通过 gRPC、wRPC Borsh、wRPC JSON 查询。
- 101 个预分配 cell 被 cellindex 正确索引。
- 节点能连续挖矿并推进 virtual DAA。
- 至少一笔标准预分配转账完整经历 submit -> mempool -> template -> block -> cellindex。
- 至少一笔多输入/多输出复杂交易成功。
- 至少一条 parent/child mempool 交易链成功；若 template 同时包含 parent 和 child，parent 必须排在 child 前面；若当前 template 只包含 parent，parent accepted 后下一块必须提升 child，最终 child 输出被 cellindex 索引。
- 至少一个 VM 成功脚本和一个 VM 失败脚本走真实验证路径。
- scheduler witness tamper 负例必须被 CellScript scheduler metadata policy 拒绝。
- 至少一个 CellScript Spora ELF 真实部署为 code cell，并通过 cell dep 完成 VM spend。
- 所有 bundled CellScript examples 都必须在 relaxed mass policy 下真实部署为 code cell，进入 block template，出块后可被 cellindex 查询。
- 所有 bundled CellScript examples 都必须覆盖 malformed spend 负例：带正确 code dep 但缺业务 witness/参数时不得被 mempool 接受。
- 至少一个时间类合约或 fixture 覆盖 DAA/header/since。
- CellScript Spora metadata 中 scheduler witness 是 Molecule，不接受 Borsh witness。
- focused CellScript examples suite 必须覆盖 token/NFT/timelock/vesting/multisig 的 action-specific metadata 和 runtime input requirements。
- full suite 必须包含两节点 block/transaction propagation 验收。
- 包管理工具完成 init/check/build/info/local dependency 的最小真实流程。
- 所有负例按预期失败，而不是测试工具 panic。
- full suite 输出机器可读报告。

## 18. 最近验收结果

2026-04-21 在 CellScript 独立仓库/submodule 切换后，按本文档重新完成一次 Spora + CKB 双 profile 验收，确认修复没有破坏任一侧兼容：

```bash
CARGO_TARGET_DIR=/tmp/spora-acceptance-full-target \
CARGO_INCREMENTAL=0 \
CARGO_BUILD_JOBS=1 \
./scripts/devnet_acceptance.sh --profile full --keep-artifacts
```

Spora full 结果：

- artifact 目录：`/Users/arthur/RustroverProjects/Spora/target/devnet-acceptance/20260421-232523-19363`
- run id：`20260421-232523-19363`
- git revision：`b8c35804`
- generated_at_utc：`2026-04-21T15:25:23Z`
- completed_at_utc：`2026-04-21T15:41:46Z`
- command：`./scripts/devnet_acceptance.sh --profile full --keep-artifacts`
- `smoke`: passed
- `external_boot`: passed
- `propagation`: passed
- `cellscript`: passed
- `result`: passed

同轮已完成 CKB 本地开发网验收：

```bash
CARGO_TARGET_DIR=/tmp/spora-acceptance-ckb-fixed-target \
CARGO_INCREMENTAL=0 \
CARGO_BUILD_JOBS=1 \
./scripts/ckb_cellscript_acceptance.sh --ckb-repo ../ckb
```

CKB 结果：

- report：`/Users/arthur/RustroverProjects/Spora/target/ckb-cellscript-acceptance/20260421-231905-5630/ckb-cellscript-acceptance-report.json`
- CKB repo：`/Users/arthur/RustroverProjects/ckb`
- CKB bin：`/Users/arthur/RustroverProjects/ckb/target/debug/ckb`
- CellScript compiler：`/tmp/spora-acceptance-ckb-fixed-target/debug/cellc`
- `status`: passed
- `onchain.status`: passed
- `bundled_examples_count`: 7
- `bundled_examples_exact_order`: `amm_pool.cell`、`launch.cell`、`multisig.cell`、`nft.cell`、`timelock.cell`、`token.cell`、`vesting.cell`
- `bundled_examples_strict_admitted`: `token.cell`
- `strict_original_ckb_compile_unexpected_failures`: `[]`
- `all_artifacts_deployed_and_spent`: true
- `all_token_actions_exercised`: true，覆盖 `mint`、`transfer_token`、`burn`、`merge`
- `all_nft_actions_exercised`: true，覆盖 `transfer`、`burn`
- `all_timelock_actions_exercised`: true，覆盖 `extend_lock`

本轮 CKB 验收暴露并修复了一个真实兼容问题：fixed-width NFT `transfer` 的合法 CKB dry-run 一度返回 `ValidationFailure error code 1`。根因是 CellScript codegen 对 `Return(None)` 的 void action 成功路径没有清空 `a0`，导致前一个 helper/comparison 留下的寄存器值被 CKB VM 当成脚本退出码。修复后 void action epilogue 前显式生成 `li a0, 0`，并补充 CKB profile 回归断言；随后 Spora full 与 CKB full acceptance 均通过。该修复已进入 CellScript 独立仓库 commit `cb0f697`，Spora submodule pointer 已更新到 `b8c35804`。

2026-04-21 针对测试暴露问题完成一次 post-fix release-style full 验收：

```bash
scripts/devnet_acceptance.sh --profile full --keep-artifacts
```

结果：

- artifact 目录：`/Users/arthur/RustroverProjects/Spora/target/devnet-acceptance/20260421-053133-12071`
- run id：`20260421-053133-12071`
- git revision：`85b9749d`
- command：`scripts/devnet_acceptance.sh --profile full --keep-artifacts`
- `smoke`: passed
- `external_boot`: passed
- `propagation`: passed
- `cellscript`: passed
- `result`: passed

关键报告：

- `acceptance-report.json`：full profile 汇总，含 run id、UTC timestamp、command、git revision、dirty flag、各子报告路径和 `profiles` summary，四段均 passed。
- `smoke-report.json`：`signed_transfer_confirmed`、`multi_input_multi_output_confirmed`、`parent_child_mempool_confirmed`、`scheduler_tamper_rejected`、`always_success_vm_spend_confirmed`、`noop_cellscript_spend_confirmed`、`cellscript_schema_output_spend_confirmed`、`cellscript_parameterized_amount_spend_confirmed` 全部为 true。
- `cellscript_schema_output_spend_confirmed` 覆盖 fixed-output CellScript schema verifier：真实链上 spend 通过 `LOAD_CELL_DATA` 校验 Output cell data，并由 cellindex 查询到 8-byte domain-separated data hash。
- `cellscript_parameterized_amount_spend_confirmed` 覆盖 parameterized CellScript entry wrapper：真实链上 spend 从 GroupInput witness 解出 u64 amount，并通过 schema verifier 校验 Output cell data。
- `smoke-report.json.relaxed_mass_policy`：`relay_non_standard: true`，`block_max_mass: 100000000`，`applies_to_all_networks_when_explicitly_enabled: true`，`standard_policy_preserved_by_default: true`。
- `propagation-report.json`：`daemon_cells_propagation_test` passed，覆盖 two-node block relay、transaction acceptance、cellindex consistency、address-scoped CellsChanged notifications。
- bundled examples 全部真实部署并被 cellindex 查询到：`amm_pool.cell`、`launch.cell`、`multisig.cell`、`nft.cell`、`timelock.cell`、`token.cell`、`vesting.cell`。
- bundled examples 的 malformed spend 全部被拒绝，且拒绝原因不是 standard/mass policy，也不是 VM cycles limit；当前全部 fail-fast 为 `VM error: Script exited with code 25`。
- `cellscript-report.json`：examples action metadata suite、package manager local dependency、registry fail-closed、check/build/init/info/add/remove/install path、entry-witness CLI builder 流程均 passed。
- failure-report path 已用受控环境故障验证：`PATH=/usr/bin:/bin scripts/devnet_acceptance.sh --profile propagation` 在 `cargo` 缺失时写出 `result: failed`、`failed_step: propagation`、`failure_exit_code: 127`、`failure_line` 的 partial `acceptance-report.json`。

2026-04-21 已完成一次针对参数化 entry 公共 witness builder 和内置 ELF assembler 修复的 smoke 复验：

```bash
scripts/devnet_acceptance.sh --profile smoke --keep-artifacts
```

结果：

- artifact 目录：`/Users/arthur/RustroverProjects/Spora/target/devnet-acceptance/20260421-055032-47089`
- run id：`20260421-055032-47089`
- git revision：`85b9749d`
- generated_at_utc：`2026-04-20T21:50:32Z`
- completed_at_utc：`2026-04-20T21:51:46Z`
- command：`scripts/devnet_acceptance.sh --profile smoke --keep-artifacts`
- `smoke`: passed
- `cellscript_parameterized_amount_spend_confirmed`: true
- devnet 参数化 amount spend 的 GroupInput witness 由 `ActionMetadata::entry_witness_args(&[EntryWitnessArg::U64(77)])` 生成，确认 public metadata API 与 ELF `_cellscript_entry` wrapper ABI 对齐。
- `launch.cell` malformed spend 已从此前暴露的 `script cycles exceeded limit` 回归修复为 `VM error: Script exited with code 25`。
- `result`: passed
- bundled examples 的 malformed spend 全部 fail-fast 为 `VM error: Script exited with code 25`；`scripts/devnet_acceptance.sh` 和 Rust smoke test 均禁止 `cycles exceeded` / `cycles limit` 出现在拒绝原因中。

2026-04-21 已完成一次 focused CellScript/package/tooling profile 复验，确认 `cellc entry-witness` 纳入验收：

```bash
scripts/devnet_acceptance.sh --profile cellscript --keep-artifacts
```

结果：

- artifact 目录：`/Users/arthur/RustroverProjects/Spora/target/devnet-acceptance/20260421-060539-74702`
- run id：`20260421-060539-74702`
- git revision：`85b9749d`
- generated_at_utc：`2026-04-20T22:05:39Z`
- completed_at_utc：`2026-04-20T22:13:08Z`
- command：`scripts/devnet_acceptance.sh --profile cellscript --keep-artifacts`
- `cellscript`: passed
- `cellscript-report.json` tests 包含 `cellc_entry_witness_subcommand`，覆盖 u64 参数 witness JSON/raw 输出和 schema-backed 参数省略。
- `result`: passed

2026-04-21 又完成一次 cleanup 后的 focused CellScript/package/tooling profile 复验，确认验收过程中暴露的 redundant/deprecated code 已修复且没有破坏 Spora/CellScript 兼容：

```bash
scripts/devnet_acceptance.sh --profile cellscript --keep-artifacts
```

结果：

- artifact 目录：`/Users/arthur/RustroverProjects/Spora/target/devnet-acceptance/20260421-095248-52171`
- run id：`20260421-095248-52171`
- generated_at_utc：`2026-04-21T01:52:48Z`
- completed_at_utc：`2026-04-21T02:00:39Z`
- command：`scripts/devnet_acceptance.sh --profile cellscript --keep-artifacts`
- `cellscript`: passed
- `cellscript-report.json` tests 包含 `cellscript_examples_action_metadata_suite`、package manager init/check/build/info/add/remove/install/local dependency、`cellc_entry_witness_subcommand`、`all_spora_examples_compile_metadata_acceptance`。
- `cargo test -p cellscript --tests -- --test-threads=1` 同轮通过：lib 323 个、CLI 64 个、examples 10 个全部 passed。
- `cargo check -p cellscript --tests` 同轮无 warning；`cargo fmt --check` 和 `git diff --check` 通过；`cargo tree -p cellscript | rg "borsh"` 无匹配。

本轮验收暴露并修复的问题：

- 移除了暴露的 deprecated `SchedulerMetadata::generate_legacy_borsh`，并移除 `cellscript` crate 对 `borsh` 的直接依赖。公开 scheduler witness ABI 只保留 Molecule；legacy Borsh sidecar decode 不再挂在 `ActionMetadata` public/crate API 上，只在 crate 内测试 helper 中覆盖旧 metadata 拒绝/迁移语义。
- `scheduler_witness_molecule_hex` / `scheduler_witness_borsh_hex` 从 `ActionMetadata` public Rust 字段收敛为私有 serde 兼容字段。新 metadata 继续不输出这些 alias；公开 `scheduler_witness_bytes()` 会拒绝 legacy Borsh 字段和冲突 Molecule alias。
- entry witness 的 magic/count 常量从 public API 收敛为 crate-internal。外部构造器应使用 `ActionMetadata::entry_witness_args` / `LockMetadata::entry_witness_args` / `encode_entry_witness_args_for_params` 或 `cellc entry-witness`，而不是依赖内部 magic 常量。integration acceptance 也改掉了对 internal magic 常量的 public API 依赖。
- 合并了重复的 hex/type-width helper：CLI、simulate 和 public builder 不再维护各自的编码/宽度推断副本，降低 `_cellscript_entry` wrapper、public metadata API 和 `cellc entry-witness` 规则漂移风险。
- 全量 CellScript 测试暴露了旧 `LOAD_CELL reason=...` 断言和 codegen fallback path 仍会在无字段访问的 consume/read_ref/destroy 上走完整 `LOAD_CELL`。已统一为 `LOAD_CELL_DATA`，并删除 `RuntimeSyscallAbi.load_cell` 字段和旧 `emit_load_cell_syscall*` helper，避免把完整 CellOutput bytes 按 schema data offset 解释。stdlib 低层 `__syscall_load_cell` wrapper 仍保留，不影响显式 full-cell syscall 支持。
- `cellscript --tests` 暴露的 warning/dead code 已清掉；最终 `cargo check -p cellscript --tests` 无 warning。

正确性结论：

- 对 Spora + CellScript v1 验收范围，这些修复是正确实现：public scheduler witness surface 是 Molecule-only；legacy Borsh 不再作为公开生成/读取 API；entry witness 的公开构造路径是 metadata/CLI builder；schema verifier 使用 `LOAD_CELL_DATA` 读取 cell data；所有 bundled examples 继续通过 metadata/ELF 编译验收。
- 这些修复没有放宽验收标准：脚本仍硬校验 7 个 bundled examples 的固定清单和顺序、所有 code cell indexed、所有 malformed spend rejected、拒绝原因不能是 standard/mass/transient/cycles policy、smoke 关键布尔字段全为 true。
- 这些修复之后已完成 CKB 本地开发网验收；`scripts/ckb_cellscript_acceptance.sh` 用父目录 CKB 节点做真实 artifact/deployment/spend 兼容测试。当前 CKB 结论覆盖 v1 pure baseline、bundled example smoke artifact 链上执行、`token.cell` strict original compile/verify，以及 token/NFT/timelock 的 v1 action-specific CKB harness；原始复杂 stateful 业务 action 链上执行仍按明确的 post-v1 builder/full-suite 范围管理。

## 19. 当前风险和边界

- 现有 `wallet create` 是交互式，不适合作为 CI 验收入口；当前已用非交互 bootstrap helper 覆盖验收路径，钱包 CLI 后续仍可单独改造。
- 目前高层 CellScript “按 action 自动构造完整业务交易”的能力不能假设已经完备；v1 已用明确 helper 构造基础链上交易，并用 focused metadata tests 验证 action-specific metadata/witness/generator 输入。完整经济学 action 交易生成器仍是 post-v1。
- `token.cell`、`vesting.cell`、`multisig.cell` 等业务示例编译出的 ELF 大于 no-op smoke artifact；验收已显式使用 `--relaynonstd` 和 `--blockmaxmass=100000000` 放宽 mass 策略。该显式 opt-in 适用于所有网络；默认不传参数时仍保留 standard mass 策略。生产部署是否需要 artifact 分片/压缩/code dep 发布策略要单独决策。
- AMM/launch 等复杂经济合约已纳入编译和部署 smoke；完整经济学 action 成功路径放入 post-v1 builder/full suite。
- 带参数 CellScript entry 的最小链上成功路径已覆盖标量/固定字节 witness ABI；复杂业务 action 的交易构造器仍需要 post-v1 完成。缺失 witness、错误 magic、不支持的参数形状或 ABI 超出 a0-a7 时，ELF `_start` 必须 fail-fast，防止 malformed spend 误进入业务逻辑或消耗到 cycles limit。
- devnet coinbase maturity 可能导致测试耗时；PR gate 优先使用 prealloc cell 做转账，coinbase maturity 放入 full suite。
- CKB profile 不是 Spora devnet 的运行目标，但必须保留 check/fail-closed 非回归测试，并用 `scripts/ckb_cellscript_acceptance.sh` 做父目录 CKB 本地集成 devnet 验收，避免 Spora devnet 工具错误污染 CKB artifact 行为。

## 20. 实施顺序

1. 已完成：写入本计划文档。
2. 已完成：新增 `devnet_bootstrap` helper 和 `spora-devnet-bootstrap` bin。
3. 已完成：新增 `devnet_acceptance_smoke`，覆盖 prealloc、cellindex、mining、标准转账。
4. 已完成：扩展 smoke，覆盖多输入/多输出、double-spend 负例、parent/child mempool 依赖和 template 拓扑顺序。
5. 已完成：新增 VM fixture 真实交易测试，覆盖 code cell deploy、缺 dep 拒绝、带 dep 成功执行。
6. 已完成：新增 `scripts/devnet_acceptance.sh` 外部进程入口，当前支持 smoke、external-boot、propagation、cellscript、full。
7. 已完成：新增 CellScript compile/deploy/use helper，并在 smoke 中覆盖 no-op Spora ELF 部署和真实 VM spend。
8. 已完成：新增 `spora-devnet-probe`，external-boot 会真实验证 gRPC、wRPC Borsh、wRPC JSON、prealloc cellindex 和模板块提交。
9. 已完成：relaxed mass policy 接入 `--relaynonstd` / `--blockmaxmass=100000000`，显式 opt-in 适用于所有网络，smoke 覆盖全部 bundled examples 的真实 code-cell 部署和 malformed spend 负例。
10. 已完成 v1 范围：接入 `token.cell` mint/transfer/burn/merge 的真实 CKB action harness，接入 fixed-width `nft.cell` transfer/burn 的真实 CKB action harness，接入 fixed-width `timelock.cell` extend_lock 的真实 CKB action harness，以及 nft/timelock/vesting/multisig 的 action-specific metadata/runtime input requirement 验收；动态集合、动态字符串和完整 on-chain economic action builder 保留 post-v1。
11. 已完成：`scripts/devnet_acceptance.sh --profile cellscript` 已覆盖 package manager init/check/build/info/add/remove/install/local dependency，并已汇总进 full profile JSON 报告。
12. 已完成：full profile 接入两节点传播验收，smoke 接入 scheduler tamper 负例；scheduler conflict policy 继续由 consensus/mining release-gate 单测覆盖。
13. 已完成：新增 `Spora Devnet Acceptance` CI workflow，PR/push 跑 smoke，nightly/manual release 跑 full。
14. 已完成：CellScript 参数化 entry 最小 witness ABI 接入 `_cellscript_entry` wrapper，smoke 覆盖 parameterized amount 成功 spend；同步修复内置 ELF assembler text PC bias，避免 wrapper branch 误编码导致 cycles limit。
15. 已完成：CellScript 公共 metadata API 暴露 `entry_witness_args`/`encode_entry_witness_args_for_params`，devnet 参数化 amount 正例改用公共 builder 生成 `CSARGv1\0 + payload`，并用单测锁定 u64、fixed-byte Address、schema-backed 参数省略规则。
16. 已完成：新增 `cellc entry-witness` CLI，把参数化 entry witness builder 暴露给 shell/wallet/package flow；`scripts/devnet_acceptance.sh --profile cellscript` 已纳入该 CLI 测试。
17. 已完成：清理暴露的 deprecated/redundant code，移除 CellScript 标准库 Borsh scheduler witness generator 和 `cellscript` crate 的直接 borsh 依赖，合并重复 hex/type-width helper。
18. 已完成：新增 `scripts/ckb_cellscript_acceptance.sh`，用父目录 CKB integration devnet 编译/校验 CKB profile artifact，并在真实 CKB RPC 上部署 code cell、创建 CellScript lock cell、花费 CellScript lock cell；该路径必须与 Spora devnet/profile 回归一起维护。
