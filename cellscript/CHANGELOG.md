# Changelog

所有 notable 变更都将记录在此文件中。

格式基于 [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)，
并且本项目遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [Unreleased]

## [0.1.0] - 2026-04-15

### Added

#### 核心编译器
- 词法分析器 (Lexer) - 完整的 CellScript 词法分析
- 解析器 (Parser) - 递归下降解析器，生成 AST
- AST 定义 - 完整的抽象语法树节点
- 类型检查器 - 静态类型检查，支持所有内置类型
- 线性检查器 - 编译时资源使用验证
- Spora IR - 中级表示，包含效果类别和调度器提示
- 优化器 - 常量折叠、代数简化、死代码消除
- RISC-V 代码生成 - 基础 RISC-V 汇编生成
- WebAssembly 目标 - Wasm 编译支持

#### 标准库
- Borsh 序列化/反序列化 (u8/u16/u32/u64/u128/bool/address/hash)
- ckbvm 系统调用包装器 (2061-2177)
- 数学函数 (min/max/isqrt/abs_diff)
- 哈希函数 (BLAKE3)
- 环境函数 (current_daa_score/remaining_cycles)
- 集合类型 (Vec, HashMap, HashSet, Option, Result)

#### 开发工具
- CLI (15+ 子命令)
  - `init` - 初始化新项目
  - `build` - 构建项目
  - `test` - 运行测试
  - `doc` - 生成文档
  - `fmt` - 格式化代码
  - `repl` - 交互式解释器
  - `check` - 快速检查
  - `run` - 运行程序
  - `add/remove` - 管理依赖
  - `clean` - 清理构建产物
  - `publish` - 发布包
  - `install` - 安装包
  - `update` - 更新依赖
  - `info` - 显示包信息
  - `login` - 登录注册表
- REPL 交互式解释器 - 实时编译和执行
- 代码格式化器 - 自动格式化 CellScript 代码
- 文档生成器 - 生成 HTML/Markdown/JSON 文档
- LSP 服务器 - IDE 支持（补全、跳转、悬停）
- 包管理器 - Cell.toml 依赖管理
- 测试框架 - 单元测试、集成测试、文档测试
- 增量编译 - 缓存和依赖追踪
- 调试信息生成 - DWARF 格式支持

#### 示例程序
- `token.cell` - 可替代代币合约
- `amm_pool.cell` - AMM 流动性池
- `vesting.cell` - 代币归属协议
- `launch.cell` - 代币启动平台
- `nft.cell` - 非同质化代币
- `multisig.cell` - 多签钱包
- `timelock.cell` - 时间锁合约

#### 语言特性
- 资源类型 (`resource`) - 线性资源管理
- 共享状态 (`shared`) - 可突变共享状态
- 收据 (`receipt`) - 带有生命周期的临时对象
- 结构体 (`struct`) - 复合数据类型
- Action - 可执行操作，支持效果标记
- Lock - 验证函数
- 能力系统 (`#[capability]`) - store/transfer/destroy
- 生命周期 (`#[lifecycle]`) - 状态机验证
- 调度器提示 (`#[scheduler_hint]`) - 并行化提示
- 模块系统 (`module`/`use`) - 代码组织
- 泛型支持 - Vec<T>, Option<T>, Result<T, E>

### Changed
- N/A (初始版本)

### Deprecated
- N/A (初始版本)

### Removed
- N/A (初始版本)

### Fixed
- N/A (初始版本)

### Security
- N/A (初始版本)

## 版本说明

### 版本号格式
`MAJOR.MINOR.PATCH`

- **MAJOR**: 不兼容的 API 变更
- **MINOR**: 向下兼容的功能添加
- **PATCH**: 向下兼容的问题修复

### 标签说明

- `[BREAKING]` - 破坏性变更
- `[FEATURE]` - 新功能
- `[BUGFIX]` - 问题修复
- `[PERF]` - 性能优化
- `[DOCS]` - 文档更新
- `[REFACTOR]` - 代码重构
- `[TEST]` - 测试相关
- `[CHORE]` - 构建/工具链

[Unreleased]: https://github.com/spora/cellscript/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/spora/cellscript/releases/tag/v0.1.0
