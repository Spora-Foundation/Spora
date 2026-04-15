# CellScript

CellScript 是 Spora 区块链的领域特定语言 (DSL)，当前处于 **MVP 编译器** 阶段。

实现状态快照见：
[CELLSCRIPT_IMPLEMENTATION_STATUS.md](/Users/arthur/RustroverProjects/Spora/docs/CELLSCRIPT_IMPLEMENTATION_STATUS.md)

## 特性

- **资源语法模型**: 支持 `resource`、`shared`、`receipt`、`action`、`lock` 的基础语法与编译路径
- **基础线性/资源检查**: 主编译链已接入类型检查与线性检查，但仍包含启发式和未完整覆盖的语义
- **效果与调度标注**: 支持 `#[effect]`、`#[scheduler_hint]` 等前端语法并进入部分 lowering
- **包感知编译**: 支持单文件、包目录、`Cell.toml`、本地 `path` 依赖和 `source_roots`
- **RISC-V 产物**: 支持 `riscv64-asm` 和 `riscv64-elf`

## 当前状态

当前真实可用的是：

- `cellc` 主入口编译器
- `lex / parse / compile` 主链
- `RISC-V assembly` 与 `RISC-V ELF` 产物
- 本地包加载与 examples / CLI / library 回归测试

当前还不能视为完成的有：

- 完整子命令工作流 (`cellc build/test/doc/fmt/...`)
- WebAssembly 主目标
- 完整 LSP / 优化器 / 文档生成器 / 包管理器生态
- 复杂控制流、资源副作用和所有语言构造的完整 lowering

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

`cellscript/src/cli/` 下存在更完整的子命令骨架，但当前主入口仍是 [cellscript/src/main.rs](/Users/arthur/RustroverProjects/Spora/cellscript/src/main.rs) 这条编译路径。

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

- `Optimizer`/`Wasm` 相关模块目前不属于稳定主路径。
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
