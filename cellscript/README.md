# CellScript

CellScript 是 Spora 区块链的领域特定语言 (DSL)，当前处于 **可工作的编译器/工具链推进阶段**：核心编译链、部分 CKB-style runtime lowering、元数据、CLI、格式化、docgen、package、test、受限 Wasm 元数据路径和基础 IDE/LSP 路径均已落地，但还不能称为完整生产级 stateful contract language。

实现状态快照见：
[CELLSCRIPT_IMPLEMENTATION_STATUS.md](/Users/arthur/RustroverProjects/Spora/docs/CELLSCRIPT_IMPLEMENTATION_STATUS.md)

## 特性

- **资源语法模型**: 支持 `resource`、`shared`、`receipt`、`action`、`lock` 的基础语法与编译路径
- **基础线性/资源检查**: 主编译链已接入类型检查与线性检查，但仍包含启发式和未完整覆盖的语义
- **效果与调度标注**: 支持 `#[effect]`、`#[scheduler_hint]` 等前端语法并进入部分 lowering
- **包感知编译**: 支持单文件、包目录、`Cell.toml`、本地 `path` 依赖和 `source_roots`
- **RISC-V 产物**: 支持 `riscv64-asm` 和 `riscv64-elf`
- **审计元数据**: 编译时输出 lowering/runtime/scheduler JSON sidecar，也可通过 `cellc metadata` 直接查看；metadata 会区分 CKB runtime access、symbolic runtime feature、fail-closed runtime feature 和 verifier obligation，并记录路径绑定的 source set BLAKE3、路径无关的 source content BLAKE3、源文件单元与 Input/CellDep/Output cell access operation provenance；schema v19 的 scheduler witness v1 携带 operation/source/index/binding-hash 访问记录，并过滤 claim witness/signature 等 runtime-only syscall 访问，使 witness 只描述调度器可见的 CellStateTree source；`spora-exec` 可 admission 这些 witness，共识 MPE `BlockAccessSummary` 已开始消费 admitted shared touch 争用域，并提供 strict trusted-access-set 路径在 merge 前对照 builder/metadata summary
- **资源操作审计**: `verifier_obligations` 会把 capability/type 层已经证明的 `transfer` / `destroy` / `claim` / `settle` 标为 `checked-static`；命名 `destroy` 的 grouped-output TypeHash absence scan 可标为 `checked-runtime`，claim condition、claim output、settle output、settle finalization 等交易级条件按覆盖度标为 `checked-runtime` 或 `runtime-required`，同时保留未完成 runtime lowering 的 `fail-closed` 义务
- **Receipt claim 输出类型**: `receipt Grant -> Token { ... }` 是一等语义；`claim grant` 的返回类型、IR `create_set`、Output access provenance、scheduler access witness 和 type metadata 都使用声明的输出类型。同名同类型 fixed-scalar 字段会从 consumed receipt 映射到 claim-created output verifier checks；未声明 `-> Type` 的 legacy receipt 仍按 non-cell `u64` claim 结果处理。
- **Schema 布局元数据**: 输出类型字段 offset / fixed encoded size；命名入参、`consume` / `transfer` / `destroy` / `claim` / `settle` 输入和 `read_ref<T>()` 上的固定标量字段 (`bool/u8/u16/u32/u64`) 可 lowered 到无对齐要求的 little-endian byte-load 组合；携带 length 的固定 schema source 会做 exact-size check 和字段 bounds check
- **Create / transfer / claim / settle 输出字段验证**: 简单 fixed-scalar `create Type { ... }` 会生成 `LOAD_CELL Source::Output`、exact-size check、bounds check 和字段相等性检查；`u64` 字段额外支持 consumed-input alias 和左结合 `+/-` 链。`transfer asset to addr`、`claim receipt` 和 `settle value` 会把同名同类型 fixed-scalar 字段从 consumed cell 映射到对应 output verifier checks；transfer lock rebinding、claim witness envelope / ECDSA authorization-domain sighash、显式 20 字节 signer pubkey hash 字段上的 `SECP256K1_VERIFY`、lifecycle-backed fixed-scalar `state` final-state settle 检查、以及 checked DAA source predicates 可在覆盖路径标为 `checked-runtime`，但没有该 signer 字段约定的 claim 授权和非 lifecycle/generalized settle finalization 仍是 runtime-required 义务。
- **Pool pattern 审计**: Pool 仍不是一等语言原语，而是 `shared` + action + metadata 模式；受控 `seed_pool` 现在会通过 `LOAD_CELL_BY_FIELD Source::Input field=5` 加载 `token_a` / `token_b` TypeHash 并拒绝相同 32 字节 identity，metadata 将 `token-pair-identity-admission` 标为 `checked-runtime`。受控 `launch_token -> seed_pool` tuple 返回路径会通过真实 RISC-V return-register ABI 标记 `pool-id-continuity` 为 `checked-runtime`；受控 `swap_a_for_b` 的 LP supply 不变式会通过 preserved `Pool.total_lp` equality 标为 `checked-runtime`。更广义的 Pool admission、AMM 经济不变量、launch/pool atomicity 和一等 Pool/launch 语言语义仍作为 runtime-required obligations 或 post-v1 边界暴露，并带有稳定 `blocker_class` 供 CLI policy 和 docgen 审计使用。
- **Create 目标边界**: `create` 只能作用于 cell-backed `resource` / `shared` / `receipt` 类型；普通 `struct` 必须使用 struct literal，不能伪装成 transaction output。
- **ReadRef 目标边界**: `read_ref<T>()` 只能作用于 cell-backed `resource` / `shared` / `receipt` 类型；普通 `struct` 不能伪装成 CellDep 读取。
- **Stateful 操作数边界**: `consume` / `transfer` / `destroy` / `claim` / `settle` 只能作用于具名的 cell-backed linear value；匿名表达式或普通 `struct` 不能绕过线性状态追踪。
- **分支线性状态合并**: `if` 分支会保守合并 linear ownership；只有部分继续路径 consume/transfer/destroy 同一资源会编译失败，避免分支内资源状态变化丢失。
- **Effect 约束**: `action` effect 会从 `read_ref` / `consume` / `create` / `destroy` / `transfer` / `claim` / `settle` 推断，并传播同模块普通函数调用和本地 `path` 依赖导入函数的 effect；显式 `#[effect(...)]` 低于真实行为时编译失败，避免调度器 metadata 低报
- **Capability 约束**: `#[capability(...)]` 与 `has ...` 声明会合并；`transfer` 必须声明 `transfer`，`destroy` 必须声明 `destroy`，`claim` 只能作用于 `receipt`，`settle` 只能作用于 cell-backed linear value
- **`fn` 边界与调用签名**: `fn` 是独立 pure helper 类别并进入 `functions[]` metadata；`fn` 只能调用 `fn`，不能调用 `action` 或 `lock`，任何 `read_ref` 或 Cell runtime 操作出现在 `fn` 内都会编译失败；无返回 helper 使用内部 `Unit`，只能作为语句调用，不能绑定或返回成值；本地、同模块限定和本地 path 依赖调用会校验参数个数和参数类型，`&mut T` 可传给只读 `&T`
- **返回语义**: 有返回值的 `action` / `fn` 必须在所有路径返回；显式 `return`、类型正确的尾表达式和两边都完整返回的 terminal `if` 都会 lowered 成真实 `Return(Some(...))` terminator
- **不可达代码拒绝**: `return` 或两边都 guaranteed-return 的 `if` 之后不能继续写语句；编译器会拒绝这种审计上不可见但 source 中存在的 dead code
- **断言语义**: `assert_invariant` lowered 成失败时返回非零错误码的 CFG，并被类型化为 `Unit`；它不能被 `let` 绑定，也不能伪装成尾返回值；message 必须是静态字符串字面量
- **局部集合语义**: 固定数组要求同质元素，空数组必须有显式零长度类型标注；`Vec::new()` 可由首次 `push(T)` 推断为 `Vec<T>`，后续不兼容 `push` 会编译失败；`len`、`push`、`extend_from_slice` 等方法调用会先经过 arity/type gate 再 lowering
- **Lifecycle 静态/运行时守门**: `#[lifecycle(...)]` receipt 现在进入主编译路径、LSP 诊断和 metadata；metadata 显式输出 lifecycle states 与相邻 transition 边；重复状态、非法 `state` 字段类型、缺失 `state` 的 lifecycle create、静态越界状态值、非初始状态创建和静态重置到初始状态都会编译失败；可完整验证的 consume-to-create fixed-scalar output 会生成 `old_state < state_count`、`new_state < state_count`、`old_state + 1 == new_state` verifier prelude

## 当前状态

当前真实可用的是：

- `cellc` 主入口编译器
- `lex / parse / compile` 主链
- `RISC-V assembly` 与 `RISC-V ELF` 产物
- 命名 schema 入参的固定标量字段访问 ELF lowering；该 ABI 使用 `aN=borsh_ptr, aN+1=borsh_len`，字段访问带 runtime bounds check
- `consume token` 的输入 Cell 预加载，以及 `token.scalar_field` 的 loaded-byte bounds check
- `read_ref<T>().scalar_field` 的 CKB-runtime ELF lowering，包含 `LOAD_CELL Source::CellDep` 和 loaded-byte bounds check
- 简单 `create` 输出固定标量字段的 assembly verifier prelude
- 简单 `consume input.u64_field -> create output.u64_field` 等值守恒检查的 assembly verifier prelude
- 简单 `consume input.u64_field +/- const_or_param_or_local_const +/- ... -> create output.u64_field` 左结合算术链检查的 assembly verifier prelude
- 本地包加载与 examples / CLI / library 回归测试
- `build` / `check` / `doc` / `fmt` / `metadata` / `verify-artifact` / compile-test 子命令；`build` / `check` 支持命令行和 manifest `[policy]` production / symbolic-runtime / CKB-runtime / runtime-obligation policy gate，`verify-artifact` 支持对已生成 artifact 执行同类 policy gate
- feature-gated `cellc run` 无参纯 ELF CKB-VM runner
- lifecycle declaration / create-state 静态检查、transition metadata 和 LSP 诊断
- CellScript scheduler witness 的低层 CellTx 放置/发现/admission helper，以及共识 MPE `BlockAccessSummary` 对 admitted witness shared read/write 争用域的首条消费路径
- no-return helper 的内部 `Unit` 类型、destinationless call lowering、`assert_invariant` 的 Unit/value-less 语义、不可达语句拒绝、未知调用返回类型拒绝、尾表达式返回 lowering、空数组类型标注和 `Vec.push` 类型传播

当前还不能视为完成的有：

- WebAssembly executable 主目标
- 完整生产级 LSP / 优化器 / 包注册表生态
- `consume` 加载 cell bytes 后的完整 resource conservation / state-transition verification
- `create` 的完整 resource-handle / lock / type script / state-transition verification
- `read_ref` 的广义 schema decoding，目前只支持固定标量字段；nested/dynamic schema 仍未完成
- 资源副作用、witness binding 和所有 stateful 构造的完整 executable lowering
- RPC/提交路径 trusted summary 认证/传递策略，以及更完整的 producer-backed 恶意 scheduler metadata 测试；wallet transaction generator 已能自动附加 compiled scheduler witness 并在 `PendingTransaction` 上暴露 trusted access summary，focused mining/consensus 测试已证明 producer-returned summary 能通过 sidecar insertion 进入 selector exposure，并被 strict template prefilter 接收或拒绝
- runtime/property/fuzz/invariant 测试执行器
- 完整 consume-to-create lifecycle transition verifier；当前 lifecycle 已做声明、静态 create-state、部分静态 reset，以及可完整 verified create output 的 `old_state + 1 == new_state` prelude，但尚未覆盖动态/nested/locked output 等所有路径

说明：

- `publish` / `install` / `update` / `login` 等注册表命令仍会明确拒绝执行，而不是伪装成成功。
- `cellc test` 当前是 compiler-test harness，会发现 `tests/**/*.cell`，支持正向编译测试、`// cellscript-test: expect-success` / `expect-fail` / `expect-error: ...` 诊断测试、`target: ...` 目标选择、`production` / `deny-*` policy 指令、standalone/CKB/symbolic/fail-closed runtime metadata 断言、runtime feature / verifier obligation / runtime-required obligation 断言，以及 action/function/lock metadata 分类断言；测试指令严格解析，拼写错误会失败；`deny-runtime-obligations` 可用于拒绝仍需外部 runtime/scheduler 兑现的 verifier obligations；`--json` 输出 CI 可解析 test summary；它还不是可信 runtime/property 测试执行器。
- `src/wasm/` 现在参与编译和测试，但仅提供受限 metadata-only 路径；`action` / `lock` executable lowering 会明确 fail-closed。
- 当前 schema lowering 只覆盖命名 action/lock 入参、`consume` 输入、`read_ref<T>()` 和简单 `create` output 上的固定宽度标量字段。字段读取使用 byte-wise little-endian 组合，避免 Borsh 紧凑布局导致的非对齐 load；本地固定数组 literal 支持静态索引读写、静态 foreach 展开、`len()` 常量折叠，并拒绝异构元素/不可变元素赋值；本地 tuple literal 支持静态字段投影/赋值和 destructuring，数组内 tuple 的静态索引投影及本地 array-of-tuples foreach destructuring 也不会退回 symbolic runtime；输入到输出的守恒仍只覆盖 `u64` 字段别名和左结合 `+/- const_or_param_or_local_const` 链，并支持简单 move/alias 传播。其他 cell-derived 字段访问仍然 fail-closed，不能当作完整状态 decoding。
- `create` output 只有在 fixed-scalar schema 的所有字段都被 verifier 覆盖时才继续执行；带 lock、动态字段、缺失字段或其它未完整证明的 output verifier 会显式 fail-closed。
- 未完成真实 verifier lowering 的 symbolic runtime 操作会显式 fail-closed，包括 `transfer` / `claim` / `settle`、不支持的 destroy operand、动态 collection、`type_hash`、未预加载的 `read_ref`、以及未 lower 到 concrete schema bytes 的 field/index 访问；命名 cell-backed `destroy` 已有受限 `GroupOutput` TypeHash absence scan，这些路径不会再返回静默成功值。
- 显式 action effect 声明必须覆盖编译器推断出的 effect；`ReadOnly` 不能声明在 `create`/`consume` 路径上，`Creating` 不能覆盖 destroy-only 路径，`Destroying` 不能覆盖 create-only 路径。
- `fn` 不允许隐藏状态访问或资源操作；同模块调用链和本地 `path` 依赖导入函数上的 impure action/fn 也会污染 `fn` 纯度。`action` 和 `lock` 可以调用 `fn`，但 `fn` 不能调用 `action` 或 `lock`；无返回 `fn` 在 IR 中不会生成调用目标，不能被 `let` 绑定或从有返回入口 `return`；需要访问 Cell/runtime 的入口必须是 `action` 或 `lock`。
- `cellc run` 不会运行带 entrypoint 参数或 CKB syscall runtime 需求的 ELF；这类 artifact 需要真实交易/ABI/syscall 上下文。

## 快速开始

### 安装

```bash
git clone https://github.com/spora/cellscript
cd cellscript
cargo install --path .
```

### 编写合约

```cellscript
module math_demo;

action add(x: u64, y: u64) -> u64 {
    let sum = x + y;
    sum
}

lock is_zero(value: u64) -> bool {
    value == 0
}
```

### 编译

```bash
# 编译单文件为汇编
cellc examples/token.cell

# 编译单文件为 ELF
cellc examples/token.cell --target riscv64-elf

# 编译包目录
cellc .

# 仅词法分析
cellc examples/token.cell --lex

# 仅解析
cellc examples/token.cell --parse
```

## 当前 CLI

| 入口 / 选项 | 描述 |
|------|------|
| `cellc <input>` | 编译 `.cell`、包目录或 `Cell.toml` |
| `--target riscv64-asm` | 生成汇编文本 |
| `--target riscv64-elf` | 生成 ELF |
| `-o <path>` | 指定输出路径 |
| `--lex` | 仅词法分析 |
| `--parse` | 仅语法分析 |
| `-i` / `--interactive` | 启动 REPL |
| `--gen-stdlib` | 输出标准库汇编 |

| 子命令 | 状态 |
|------|------|
| `cellc build [--json] [--production] [--deny-fail-closed] [--deny-symbolic-runtime] [--deny-ckb-runtime] [--deny-runtime-obligations]` | 编译当前包并写入 artifact + metadata；写入前执行 metadata policy gate；`--json` 输出 CI 可解析摘要 |
| `cellc check [--all-targets] [--json] [--production] [--deny-fail-closed] [--deny-symbolic-runtime] [--deny-ckb-runtime] [--deny-runtime-obligations]` | 类型检查 / lowering 检查，不写 artifact；`--all-targets` 同时验证 asm 与 ELF lowering；可按 metadata policy 拒绝 fail-closed、symbolic runtime、CKB runtime 或 runtime-required verifier obligations；`--json` 输出 CI 可解析摘要 |
| `cellc doc --format markdown|html|json [--json]` | 从包源生成 API 文档，并附带 lowering audit report / verifier obligations；`--json` 输出 CI 可解析 doc summary |
| `cellc fmt [--check] [--json]` | 格式化包源或指定文件；`--json` 输出 CI 可解析 changed-file summary |
| `cellc init [NAME] [PATH] [--lib] [--json]` | 创建包目录、manifest 和入口文件；`--json` 输出 package/path/manifest/entry summary |
| `cellc add CRATE... [--dev] [--build] [--git URL] [--path PATH] [--json]` | 写入普通/dev/build 依赖；支持 git/path dependency source；`--json` 输出 dependency mutation summary |
| `cellc remove CRATE... [--dev] [--build] [--json]` | 从普通/dev/build dependency section 删除依赖；`--json` 输出 removed/missing summary |
| `cellc clean [--json]` | 删除本地构建缓存；`--json` 输出 removed-path summary |
| `cellc info [--json]` | 读取 `Cell.toml` 包信息；`--json` 输出 manifest/package/dependency/policy summary |
| `cellc metadata [INPUT]` | 输出 lowering/runtime/scheduler/source provenance JSON |
| `cellc verify-artifact ARTIFACT [--metadata FILE] [--verify-sources] [--json] [--expect-artifact-hash HASH] [--expect-source-content-hash HASH] [--production] [--deny-fail-closed] [--deny-symbolic-runtime] [--deny-ckb-runtime] [--deny-runtime-obligations]` | 校验 artifact、metadata sidecar、可选磁盘源文件绑定、供应链 hash pin，以及已生成 artifact 的上线 policy gate；`--json` 输出 CI 可解析摘要 |
| `cellc test [--no-run]` | 发现 `tests/**/*.cell`，执行正向编译测试、注释驱动的负向诊断测试、per-file target/policy/runtime-metadata compiler tests |
| `cellc run` | 需要 `vm-runner` feature；仅支持无参纯 ELF 路径 |
| `publish/install/update/login` | 注册表生态未完成，fail-closed |

## Manifest Policy

`cellc build` 和 `cellc check` 会读取 `Cell.toml` 的 `[policy]` 默认值，并与命令行 flags 做 OR 合并；命令行只能进一步收紧，不能放宽仓库策略。

```toml
[policy]
production = true
deny_fail_closed = true
deny_symbolic_runtime = false
deny_ckb_runtime = false
deny_runtime_obligations = false
```

建议生产合约至少启用 `production = true`，让 CI 拒绝当前仍 fail-closed 的 lowering 路径；如果部署环境没有独立 runtime/scheduler obligation consumer，还应启用 `deny_runtime_obligations = true`。

## 编辑器支持

仓库现在包含一个薄层 VS Code 扩展，用于 `.cell` 语法高亮、语言配置和基础 snippets：

- [cellscript/editors/vscode-cellscript](/Users/arthur/RustroverProjects/Spora/cellscript/editors/vscode-cellscript)

它当前覆盖：

- `.cell` 文件关联
- TextMate 语法高亮
- 注释 / 括号 / 自动闭合配置
- 基础模板片段
- 本地 `npm run validate` / `npm run package` 校验入口
- 基于 `cellc` 的基础诊断
- 与 in-crate LSP 对齐的格式化 / hover / definition / references 方向
- action hover 中展示 lowering metadata、ELF 兼容性、symbolic runtime features、fail-closed runtime features、verifier obligations 和 CKB access summary
- diagnostics 中提示 ELF-incompatible symbolic runtime action
- code actions 中给出查看 `cellc metadata` 和临时使用 asm target 的建议

它当前仍不承诺：

- rename
- 完整跨包语义索引
- 调试器或完整 LSP 体验

`src/lsp/` 现在有最小真实路径并参与测试，但仍不是成熟语言服务器。

## 语言特性

### 资源类型

```cellscript
// 基础资源声明
resource Token {
    amount: u64,
}

// 基础共享状态声明
shared LiquidityPool {
    reserve_a: u64,
    reserve_b: u64,
}

// 基础收据声明
receipt VestingGrant -> Token {
    beneficiary: Address,
    total_amount: u64,
}
```

### Action

```cellscript
#[effect(Pure)]
action add(
    x: u64,
    y: u64,
) -> u64 {
    let sum = x + y;
    sum
}
```

### Lock

```cellscript
lock is_zero(value: u64) -> bool {
    value == 0
}
```

## 项目结构

```
cellscript/
├── src/           # 编译器源码
├── examples/      # 示例合约
│   ├── token.cell
│   ├── amm_pool.cell
│   ├── vesting.cell
│   └── ...
└── tests/         # 测试用例
```

## 当前编译链

```
CellScript 源码
    ↓
Lexer (词法分析)
    ↓
Parser (语法分析)
    ↓
AST (抽象语法树)
    ↓
Type Checker (类型检查)
    ↓
Linear Checker (线性检查)
    ↓
Minimal Spora IR (最小中间表示)
    ↓
Code Generator (代码生成)
    ↓
RISC-V Assembly / RISC-V ELF
```

说明：

- `Optimizer`/`Wasm` 相关模块目前不属于稳定 executable 主路径；`wasm` 模块对未支持 executable lowering 会 fail-closed，避免隐藏过期后端。
- 当前最可信的链路是 `Lexer -> Parser -> Type Checker -> Linear Check -> 最小 IR -> Codegen -> asm/elf`。

## 贡献

欢迎贡献！请阅读 [CONTRIBUTING.md](CONTRIBUTING.md) 了解如何参与。

## 许可证

本项目采用 MIT 或 Apache-2.0 双许可证。详见 [LICENSE-MIT](LICENSE-MIT) 和 [LICENSE-APACHE](LICENSE-APACHE)。

## 相关项目

- [Spora](https://github.com/spora/spora) - Spora 区块链核心
- [ckb-vm](https://github.com/nervosnetwork/ckb-vm) - RISC-V 虚拟机

## 社区

- Discord: [Spora Community](https://discord.gg/spora)
- Twitter: [@SporaChain](https://twitter.com/SporaChain)
- Forum: [forum.spora.io](https://forum.spora.io)
