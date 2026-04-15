# 贡献指南

感谢您对 CellScript 的兴趣！我们欢迎各种形式的贡献。

## 如何贡献

### 报告问题

如果您发现了 bug 或有功能建议，请通过 GitHub Issues 提交。

提交 Issue 时，请包含：
- 问题的清晰描述
- 复现步骤（如果是 bug）
- 期望的行为
- 实际的行为
- 环境信息（操作系统、Rust 版本等）

### 提交代码

1. **Fork 仓库**
   ```bash
   git clone https://github.com/your-username/cellscript.git
   cd cellscript
   ```

2. **创建分支**
   ```bash
   git checkout -b feature/your-feature-name
   # 或
   git checkout -b fix/your-bug-fix
   ```

3. **进行更改**
   - 遵循现有的代码风格
   - 添加测试（如果适用）
   - 更新文档

4. **提交更改**
   ```bash
   git add .
   git commit -m "[FEATURE] 描述您的更改"
   ```

5. **推送到 Fork**
   ```bash
   git push origin feature/your-feature-name
   ```

6. **创建 Pull Request**
   - 在 GitHub 上创建 PR
   - 描述您的更改
   - 关联相关的 Issue（如果有）

## 开发环境

### 要求

- Rust 1.75+ 
- Cargo
- Git

### 构建

```bash
# 克隆仓库
git clone https://github.com/spora/cellscript.git
cd cellscript

# 构建
cargo build

# 运行测试
cargo test

# 安装本地版本
cargo install --path .
```

### 项目结构

```
cellscript/
├── src/
│   ├── main.rs          # CLI 入口
│   ├── lib.rs           # 库入口
│   ├── cli/             # CLI 子命令
│   ├── lexer/           # 词法分析器
│   ├── parser/          # 解析器
│   ├── ast/             # 抽象语法树
│   ├── types/           # 类型系统
│   ├── ir/              # 中级表示
│   ├── codegen/         # 代码生成
│   ├── stdlib/          # 标准库
│   ├── resolve/         # 模块系统
│   ├── lifecycle/       # 生命周期验证
│   ├── optimize/        # 优化器
│   ├── docgen/          # 文档生成器
│   ├── fmt/             # 代码格式化器
│   ├── lsp/             # LSP 服务器
│   ├── package/         # 包管理器
│   ├── test/            # 测试框架
│   ├── wasm/            # Wasm 目标
│   ├── incremental/     # 增量编译
│   └── debug/           # 调试信息生成
├── examples/            # 示例程序
└── tests/               # 集成测试
```

## 代码风格

### Rust 代码

- 使用 `rustfmt` 格式化代码
- 使用 `clippy` 检查代码
- 遵循 [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)

```bash
# 格式化
cargo fmt

# 检查
cargo clippy
```

### CellScript 代码

- 使用 4 空格缩进
- 最大行宽 100 字符
- 操作符周围加空格

## 测试

### 运行测试

```bash
# 所有测试
cargo test

# 特定测试
cargo test test_name

# 显示输出
cargo test -- --nocapture
```

### 测试类型

- **单元测试**: 测试单个函数/模块
- **集成测试**: 测试完整编译流程
- **文档测试**: 测试文档中的代码示例

### 添加测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature() {
        // 测试代码
        assert_eq!(expected, actual);
    }
}
```

## 文档

- 为所有公共 API 添加文档注释
- 使用示例代码说明用法
- 更新 README.md（如果适用）

```rust
/// 函数描述
///
/// # 示例
///
/// ```
/// let result = function_name(arg);
/// assert_eq!(result, expected);
/// ```
pub fn function_name(arg: Type) -> ReturnType {
    // ...
}
```

## 提交信息规范

使用以下前缀：

- `[FEATURE]` - 新功能
- `[BUGFIX]` - 问题修复
- `[PERF]` - 性能优化
- `[DOCS]` - 文档更新
- `[REFACTOR]` - 代码重构
- `[TEST]` - 测试相关
- `[CHORE]` - 构建/工具链

示例：
```
[FEATURE] 添加 Vec::sort 方法

实现快速排序算法，支持 u64 类型。

Closes #123
```

## 代码审查

所有 PR 都需要至少一个维护者的审查。审查时关注：

- 代码正确性
- 测试覆盖
- 文档完整性
- 性能影响
- 向后兼容性

## 发布流程

1. 更新 CHANGELOG.md
2. 更新版本号（Cargo.toml）
3. 创建 Git 标签
4. 发布到 crates.io

## 社区

- 尊重所有社区成员
- 欢迎新手，耐心回答问题
- 建设性地讨论技术问题

## 许可证

通过贡献代码，您同意您的贡献将采用与项目相同的许可证（MIT/Apache-2.0 双许可）。

## 联系方式

- GitHub Issues: [github.com/spora/cellscript/issues](https://github.com/spora/cellscript/issues)
- Discord: [Spora Community](https://discord.gg/spora)
- Email: dev@spora.io

感谢您的贡献！
