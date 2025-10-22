# UTXO Cleanup Progress Report

**Branch**: `spora`  
**Date**: 2025-10-22  
**Status**: 🚧 In Progress

---

## Completed ✅

### 1. Core Infrastructure Removal
- ✅ **删除 indexes/utxoindex/** (16 files removed via `git rm`)
  ```bash
  git rm -rf indexes/utxoindex/
  ```

- ✅ **移除 Cargo.toml 依赖**
  - Removed `"indexes/utxoindex"` from workspace members
  - Removed `tondi-utxoindex` from workspace.dependencies

- ✅ **创建 Cell Validator** (替代 Transaction Validator)
  - `consensus/src/processes/cell_validator/mod.rs`
  - `consensus/src/processes/cell_validator/errors.rs`
  - `consensus/src/processes/cell_validator/cell_validation_in_isolation.rs`
  - `consensus/src/processes/cell_validator/cell_validation_in_context.rs`

- ✅ **重命名旧验证器** (保留历史)
  ```bash
  git mv consensus/src/processes/transaction_validator \
         consensus/src/processes/transaction_validator.deprecated
  ```

---

## Remaining UTXO References (by Category)

### Category 1: Core Consensus (High Priority) 🔴

**需要重写或删除的文件** (17 files):

```
consensus/core/src/tx.rs                    # VerifiableTransaction trait
consensus/core/src/utxo/                    # 整个UTXO模块（5个文件）
  ├── mod.rs
  ├── utxo_diff.rs
  ├── utxo_collection.rs
  ├── utxo_view.rs
  └── utxo_inquirer.rs
consensus/core/src/errors/utxo/mod.rs       # UTXO错误定义
consensus/src/model/stores/utxo_set.rs      # UTXO存储
consensus/src/model/stores/utxo_diffs.rs    # UTXO差异
consensus/src/model/stores/pruning_utxoset.rs
consensus/src/pipeline/virtual_processor/utxo_validation.rs
consensus/src/pipeline/virtual_processor/utxo_inquirer.rs
consensus/src/consensus/utxo_set_override.rs
```

**修复策略**:
1. 创建 `consensus/core/src/cell/` 模块（对应 utxo/）
2. 实现 `CellDiff`, `CellCollection`, `CellView`
3. 更新 stores 使用 Cell 模型

---

### Category 2: Wallet Layer (Medium Priority) 🟡

**需要适配Cell模型的文件** (28 files):

```
wallet/core/src/utxo/                       # UTXO钱包核心（3个文件）
  ├── test.rs
  ├── sync.rs
  └── stream.rs
wallet/core/src/wasm/utxo/                  # WASM绑定（2个文件）
  ├── processor.rs
  └── context.rs
wallet/pstt/src/                            # PSTT格式（6个文件）
  ├── pstt.rs
  ├── input.rs
  ├── bundle.rs
  └── ...
```

**修复策略**:
1. 等待 Cell 模型稳定后重写
2. 创建 `wallet/core/src/cell/` 模块
3. 实现 Cell-based PSTT (PSCT?)

---

### Category 3: Mining & RPC (Medium Priority) 🟡

**需要更新的文件** (12 files):

```
mining/src/mempool/model/utxo_set.rs
mining/src/mempool/                         # Mempool UTXO集合
mining/errors/src/mempool.rs
rpc/core/src/                               # RPC UTXO响应
indexes/core/src/indexed_utxos.rs
indexes/core/src/notification.rs
```

**修复策略**:
1. Mining: 已有新的 `mempool/` crate（Cell版本）
2. RPC: 定义 Cell 查询响应类型

---

### Category 4: Examples & Tests (Low Priority) 🟢

**需要更新的文件** (33 files):

```
wasm/examples/                              # WASM示例（11个文件）
  ├── web/utxo-context.html
  ├── nodejs/javascript/transactions/utxo-*.js
  └── ...
testing/integration/                        # 集成测试
consensus/benches/check_scripts.rs          # 性能测试
```

**修复策略**:
1. 后期更新示例代码
2. 创建新的 Cell 示例

---

## 统计

| 类别 | 文件数 | 优先级 | 状态 |
|-----|--------|--------|------|
| 已删除 | 16 | - | ✅ 完成 |
| Core Consensus | 17 | 🔴 High | ⏳ 待处理 |
| Wallet | 28 | 🟡 Medium | ⏳ 待处理 |
| Mining & RPC | 12 | 🟡 Medium | ⏳ 待处理 |
| Examples & Tests | 33 | 🟢 Low | ⏳ 待处理 |
| **总计** | **106** | - | **15% 完成** |

---

## 下一步计划

### Phase 1: Core Consensus (3-5天)
- [ ] 创建 `consensus/core/src/cell/` 模块
- [ ] 实现 `CellDiff`, `CellCollection`, `CellView`
- [ ] 更新 `virtual_processor` 使用 Cell 验证
- [ ] 删除 `consensus/core/src/utxo/` 目录

### Phase 2: Storage & Indexes (2-3天)
- [ ] 更新 `consensus/src/model/stores/` 为 Cell 版本
- [ ] 更新 `indexes/core/` 通知类型
- [ ] 删除 UTXO 相关 stores

### Phase 3: Wallet Adaptation (5-7天)
- [ ] 重写 `wallet/core/src/cell/` 模块
- [ ] 适配 PSTT → PSCT (Partially Signed Cell Transaction)
- [ ] 更新 WASM 绑定

### Phase 4: Examples & Documentation (2-3天)
- [ ] 更新示例代码
- [ ] 更新文档
- [ ] 创建迁移指南

---

## Git History Preservation ✅

所有删除操作使用 `git rm` 以保留历史记录：

```bash
# 已执行
git rm -rf indexes/utxoindex/
git mv consensus/src/processes/transaction_validator \
       consensus/src/processes/transaction_validator.deprecated

# 待执行
git rm -rf consensus/core/src/utxo/
git rm -rf consensus/src/model/stores/utxo_*.rs
...
```

---

## 验证清单

- ✅ `cargo check --workspace` 通过（移除 utxoindex 后）
- ✅ Cell validator 基础实现完成
- ⏳ 所有共识测试通过
- ⏳ 钱包功能可用
- ⏳ RPC 兼容性验证

---

## 风险评估

| 风险 | 影响 | 缓解措施 |
|-----|------|---------|
| 钱包功能不可用 | 高 | 分阶段迁移，保留旧代码直到新版本稳定 |
| RPC 兼容性破坏 | 中 | 提供 v2 API，标记 v1 deprecated |
| 测试覆盖不足 | 中 | 每个模块强制 TDD |
| 共识分叉风险 | 低 | SPORA 分支独立，不影响主网 |

---

**最后更新**: 2025-10-22 15:30 UTC

