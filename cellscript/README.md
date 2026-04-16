# CellScript

CellScript 是 Spora 区块链的领域特定语言 (DSL)，当前处于 **可工作的编译器/工具链推进阶段**：核心编译链、部分 CKB-style runtime lowering、元数据、CLI、格式化、docgen、package、test、Wasm scaffold 和 IDE/LSP scaffold 均已落地，但还不能称为完整生产级 stateful contract language。

实现状态快照见：
[CELLSCRIPT_IMPLEMENTATION_STATUS.md](/Users/arthur/RustroverProjects/Spora/docs/CELLSCRIPT_IMPLEMENTATION_STATUS.md)

## 特性

- **资源语法模型**: 支持 `resource`、`shared`、`receipt`、`action`、`lock` 的基础语法与编译路径
- **基础线性/资源检查**: 主编译链已接入类型检查与线性检查，但仍包含启发式和未完整覆盖的语义
- **效果与调度标注**: 支持 `#[effect]`、`#[scheduler_hint]` 等前端语法并进入部分 lowering
- **包感知编译**: 支持单文件、包目录、`Cell.toml`、本地 `path` 依赖和 `source_roots`
- **RISC-V 产物**: 支持 `riscv64-asm` 和 `riscv64-elf`
- **审计元数据**: 编译时输出 lowering/runtime/scheduler JSON sidecar，也可通过 `cellc metadata` 直接查看；metadata 会区分 CKB runtime access、symbolic runtime feature 和 fail-closed runtime feature
- **Schema 布局元数据**: 输出类型字段 offset / fixed encoded size；命名入参、`consume` 输入和 `read_ref<T>()` 上的固定标量字段 (`bool/u8/u16/u32/u64`) 可 lowered 到无对齐要求的 little-endian byte-load 组合；携带 length 的固定 schema source 会做 exact-size check 和字段 bounds check
- **Create 输出字段验证**: 简单 fixed-scalar `create Type { ... }` 会生成 `LOAD_CELL Source::Output`、exact-size check、bounds check 和字段相等性检查；`u64` 字段额外支持 consumed-input alias 和左结合 `+/-` 链
- **Effect 约束**: `action` effect 会从 `read_ref` / `consume` / `create` / `destroy` / `transfer` / `claim` / `settle` 推断，并传播同模块普通函数调用和本地 `path` 依赖导入函数的 effect；显式 `#[effect(...)]` 低于真实行为时编译失败，避免调度器 metadata 低报
- **`fn` 边界**: `fn` 被强制为 pure helper；任何 `read_ref` 或 Cell runtime 操作出现在 `fn` 内都会编译失败

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
- `build` / `check` / `doc` / `fmt` / `metadata` / compile-test 子命令
- feature-gated `cellc run` 无参纯 ELF CKB-VM runner

当前还不能视为完成的有：

- WebAssembly executable 主目标
- 完整生产级 LSP / 优化器 / 包注册表生态
- `consume` 加载 cell bytes 后的完整 resource conservation / state-transition verification
- `create` 的完整 resource-handle / lock / type script / state-transition verification
- `read_ref` 的广义 schema decoding，目前只支持固定标量字段；nested/dynamic schema 仍未完成
- 资源副作用、witness binding 和所有 stateful 构造的完整 executable lowering
- runtime/property/fuzz/invariant 测试执行器

说明：

- `publish` / `install` / `update` / `login` 等注册表命令仍会明确拒绝执行，而不是伪装成成功。
- `cellc test` 当前是 compile-test harness，会发现并编译 `tests/**/*.cell`；它还不是可信 runtime/property 测试执行器。
- `src/wasm/` 现在参与编译和测试，但对 `action` / `lock` executable lowering 明确 fail-closed。
- 当前 schema lowering 只覆盖命名 action/lock 入参、`consume` 输入、`read_ref<T>()` 和简单 `create` output 上的固定宽度标量字段。字段读取使用 byte-wise little-endian 组合，避免 Borsh 紧凑布局导致的非对齐 load；输入到输出的守恒仍只覆盖 `u64` 字段别名和左结合 `+/- const_or_param_or_local_const` 链，并支持简单 move/alias 传播。其他 cell-derived 字段访问仍然 fail-closed，不能当作完整状态 decoding。
- `create` output 只有在 fixed-scalar schema 的所有字段都被 verifier 覆盖时才继续执行；带 lock、动态字段、缺失字段或其它未完整证明的 output verifier 会显式 fail-closed。
- 未完成真实 verifier lowering 的 symbolic runtime 操作会显式 fail-closed，包括 `transfer` / `destroy` / `claim` / `settle`、动态 collection、`type_hash`、未预加载的 `read_ref`、以及未 lower 到 concrete schema bytes 的 field/index 访问；这些路径不会再返回占位成功值。
- 显式 action effect 声明必须覆盖编译器推断出的 effect；`ReadOnly` 不能声明在 `create`/`consume` 路径上，`Creating` 不能覆盖 destroy-only 路径，`Destroying` 不能覆盖 create-only 路径。
- `fn` 不允许隐藏状态访问或资源操作；同模块调用链和本地 `path` 依赖导入函数上的 impure action/fn 也会污染 `fn` 纯度。需要访问 Cell/runtime 的入口必须是 `action` 或 `lock`。
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
| `cellc build` | 编译当前包并写入 artifact + metadata |
| `cellc check` | 类型检查 / lowering 检查，不写 artifact |
| `cellc doc --format markdown|html|json` | 从包源生成文档 |
| `cellc fmt [--check]` | 格式化包源或指定文件 |
| `cellc metadata [INPUT]` | 输出 lowering/runtime/scheduler JSON |
| `cellc test [--no-run]` | 发现并编译 `tests/**/*.cell` |
| `cellc run` | 需要 `vm-runner` feature；仅支持无参纯 ELF 路径 |
| `publish/install/update/login` | 注册表生态未完成，fail-closed |

## 编辑器支持

仓库现在包含一个薄层 VS Code 扩展骨架，用于 `.cell` 语法高亮、语言配置和基础 snippets：

- [cellscript/editors/vscode-cellscript](/Users/arthur/RustroverProjects/Spora/cellscript/editors/vscode-cellscript)

它当前覆盖：

- `.cell` 文件关联
- TextMate 语法高亮
- 注释 / 括号 / 自动闭合配置
- 基础模板片段
- 本地 `npm run validate` / `npm run package` 骨架
- 基于 `cellc` 的基础诊断
- 与 in-crate LSP 对齐的格式化 / hover / definition / references 方向
- action hover 中展示 lowering metadata、ELF 兼容性、symbolic runtime features、fail-closed runtime features 和 CKB access summary
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
receipt VestingGrant {
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

- `Optimizer`/`Wasm` 相关模块目前不属于稳定 executable 主路径；`wasm` 模块会 fail-closed，避免隐藏过期后端。
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
