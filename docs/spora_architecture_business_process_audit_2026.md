# Spora 架构业务流程完备性审计报告

**日期**: 2026-04-13  
**审计方式**: 从用户可用性角度倒序审计，从应用层向底层检查各业务流程的完备性  
**审计范围**: 全链业务流程，涵盖钱包、RPC、索引、共识、VM、P2P、挖矿等核心模块

---

## 执行摘要

### 核心结论

> **Spora 架构已从"共识原型"阶段进入"基础设施收尾"阶段。**

最关键的判断变化：
- ❌ **早期审计（2026-04-11）**: 共识未成立，查询层断路，VM 是骨架
- ✅ **当前状态（2026-04-13）**: 共识主路径已闭环，CellIndex 已接入，VM 真实执行
- ✅ **本轮修复（2026-04-13）**: `get_headers` / `get_cell_return_address` / `resolve_finality_conflict` 已接通，钱包 `set_name` / gen1 导入 / `watch-only` 创建链路已闭环
- ⚠️ **复审补充（2026-04-13）**: 标准地址锁默认执行链路仍未闭环；钱包生成的 canonical `CellTx` 默认不自动补脚本 `cell_deps`，生产级签名锁 ELF 也尚未落地

### 总体完备度评级

| 层级 | 完备度 | 评级 | 状态 |
|------|--------|------|------|
| 共识核心 | 90% | A- | ✅ 生产就绪 |
| 数据/状态管理 | 85% | B+ | ✅ 可用 |
| 挖矿/Mempool | 85% | B+ | ✅ 可用 |
| P2P 网络层 | 85% | B+ | ✅ 可用 |
| VM/脚本执行 | 78% | B | ⚠️ 真实执行已接通，但生产锁脚本仍未收口 |
| 索引层 (CellIndex) | 80% | B+ | ✅ 已接入 |
| RPC 查询层 | 78% | B | ⚠️ 主体可用 |
| 钱包 SDK | 72% | B- | ⚠️ 主体可用，但默认签名交易链路仍有缺口 |

**加权总体评级: B (约 80%)**

---

## 1. 共识层（Consensus）

### 1.1 状态: 90% 完备 ⭐⭐⭐⭐

**结论**: 共识主路径已闭环，核心机制完整实现。

### 已完成的组件

| 组件 | 状态 | 说明 |
|------|------|------|
| GhostDAG 共识算法 | ✅ | 完整实现，包括蓝/红块分类、k-确认 |
| Cell 模型核心 | ✅ | OutPoint、Script、CellOutput、CellInput、CellDep 全部实现 |
| 四层验证架构 | ✅ | 隔离验证 → 上下文验证 → DAG验证 → 脚本验证 |
| 时间锁 `since` | ✅ | 四类语义全部实现（绝对/相对 DAA/时间戳） |
| CellValidator | ✅ | 已接入虚拟处理器和区块正文验证 |
| 难度调整 (DAA) | ✅ | 完整实现 |
| Pruning 机制 | ✅ | 状态修剪完整 |

### 关键代码路径

```rust
// consensus/src/pipeline/virtual_processor/cell_processing.rs
// - Cell 状态计算与 diff 应用
// - mergeset 处理

// consensus/src/processes/cell_validator/mod.rs
// - 四层验证入口
// - VM 脚本验证调用

// consensus/src/pipeline/body_processor/body_validation_in_context.rs
// - 区块正文上下文验证
// - block cycles 累计检查
```

### 剩余问题

| 问题 | 优先级 | 影响 |
|------|--------|------|
| CellDiff 组合语义在复杂场景需验证 | P2 | 边缘情况处理 |
| CellStateTree::root() O(n) 性能 | P2 | 大规模状态性能 |
| 历史查询 POV-aware 增强 | P2 | 复杂重组场景 |

---

## 2. VM/脚本执行层（VM/Script）

### 2.1 状态: 78% 完备 ⭐⭐⭐

**结论**: 执行闭环已完成，测试强度高于表面印象，共识主路径已接入；`LoadHeader` 的 header/runtime 视图已明显补强。但生产级签名锁 ELF 仍未落地，标准地址锁的默认执行链路也未完全收口。

### 已完成的组件

| 组件 | 状态 | 说明 |
|------|------|------|
| ckb-vm 集成 | ✅ | 真实执行 ELF，非 placeholder |
| TransactionScriptVerifier | ✅ | 脚本分组、并行验证、cycles 汇总 |
| CellValidator 接入 | ✅ | 共识验证路径已调用 |
| Block cycles 累计 | ✅ | 区块级别 cycles 上限检查 |
| 资源限制落地 | ✅ | tx/block cycles、script size、VM memory 已接入 |

### Syscall 实现状态

| Syscall | 状态 | 说明 |
|---------|------|------|
| CurrentCycles | ✅ | 已返回真实 cycles |
| Debugger | ✅ | 内存问题已修复 |
| LoadInput | ✅ | 错误码语义已对齐 |
| LoadCell | ✅ | 共享枚举解析 |
| LoadCellData | ✅ | 共享 Source 解析 |
| LoadWitness | ✅ | 共享 Source 解析 |
| LoadHeader | ✅ | 完整 header/runtime 字段已返回，`by-field` 已扩展，source 语义仍为子集 |

### 测试覆盖

```bash
$ cargo test -p spora-exec --features vm
# 结果: 119 passed, 0 failed
# 覆盖: always_success, load_input_since, load_header_timestamp, 
#       load_dep_cell_data, timelock_absolute, timelock_relative, htlc, htlc_minimal
```

### 剩余问题

| 问题 | 优先级 | 说明 |
|------|--------|------|
| 标准地址锁默认执行链路 | P1 | 生产 VM provider 仅从 `cell_deps` 注入脚本字节码，默认地址锁交易未自动补 dep |
| 生产级签名锁 | P1 | HTLC 仅为 fixture-only 签名规则，`secp256k1_blake3_lock.c` 仍未接真 secp256k1 |
| Syscall 语义子集 | P1 | 当前为子集实现，需补齐 CKB 完整语义 |
| Header source 语义 | P1 | `LoadHeader` 目前仍主要覆盖 `HeaderDep` 路径 |
| Mempool cycles 前置裁剪 | P2 | block 预算预筛能力有限 |

---

## 3. 数据/状态管理层（State）

### 3.1 状态: 85% 完备 ⭐⭐⭐⭐

**结论**: Cell 状态管理主路径完整，持久化机制就绪。

### 已完成的组件

| 组件 | 状态 | 说明 |
|------|------|------|
| CellStateTree | ✅ | 完整实现 |
| CellDB | ✅ | 状态持久化 |
| CellDiff | ✅ | 状态差异管理（基础语义正确） |
| Pruning 支持 | ✅ | 修剪安全恢复 |
| cell_root 计算 | ✅ | 默克尔根承诺 |

### 关键代码路径

```rust
// state/src/cell_tree.rs
// - CellStateTree 实现

// state/src/index/cell_db.rs
// - Cell 状态持久化查询

// consensus/core/src/cell_diff.rs
// - CellDiff 定义与组合
```

### 剩余问题

| 问题 | 优先级 | 说明 |
|------|--------|------|
| CellStateTree::root() O(n) | P2 | 大规模状态性能瓶颈 |
| 历史查询 POV-aware | P2 | 复杂 DAG 场景状态视角 |

---

## 4. 索引层（CellIndex）

### 4.1 状态: 80% 完备 ⭐⭐⭐⭐

**结论**: **关键进展** - CellIndex 已实现并接入 daemon，RPC 查询不再断路。

### 已完成的组件

| 组件 | 状态 | 说明 |
|------|------|------|
| CellIndexer | ✅ | 完整实现 |
| CellIndexProxy | ✅ | 代理接口 |
| daemon 接入 | ✅ | sporad 启动时初始化 |
| RPC 查询接入 | ✅ | get_cell_set_by_address 等已实现 |

### 关键代码路径

```rust
// sporad/src/daemon.rs:619
let cellindex = CellIndexProxy::new(Arc::new(CellIndexer::new(&cell_db_dir, &script_db_dir).unwrap()));

// rpc/service/src/service.rs:259-280
async fn get_cell_set_by_address(&self, address: &RpcAddress, start: u64, limit: u32) -> RpcResult<(CompactCellCollection, u64)> {
    let lock_script = pay_to_address_lock_script(address);
    let result = self.cellindex()?.query(&CellQuery::by_lock(lock_script.hash(), query_limit))...;
}
```

### 剩余问题

| 问题 | 优先级 | 说明 |
|------|--------|------|
| 大规模数据性能 | P2 | 需生产环境验证 |
| 复杂查询场景 | P2 | 多条件组合查询 |

---

## 5. RPC 查询层（RPC）

### 5.1 状态: 78% 完备 ⭐⭐⭐

**结论**: 接口定义完整，CellIndex 已接入，核心查询主体可用；`get_headers`、`get_cell_return_address`、`get_coin_supply`、`resolve_finality_conflict` 已接通，finality conflict 通知和确认链路不再只是空壳；`get_block` / `get_transaction` 的 accepted tx mass 也已提升到 resolved selection mass 路径。

### 已完成的组件

| 组件 | 状态 | 说明 |
|------|------|------|
| RPC 方法定义 | ✅ | 完整定义 |
| CellIndex 接入 | ✅ | 查询数据源已接入 |
| get_cell_set_by_address | ✅ | 已实现 |
| get_balance_by_addresses | ✅ | 已实现 |
| get_cells_by_address | ✅ | 已实现 |
| get_coin_supply | ✅ | 已接入真实供应量计算 |
| get_headers | ✅ | 已返回 header 列表 |
| get_cell_return_address | ✅ | 已返回首个 resolved input 的返还地址 |
| resolve_finality_conflict | ✅ | 已确认当前 finality point 并清理已记录冲突 |
| accepted tx mass 口径 | ✅ | `get_block` / `get_transaction` 已优先走 resolved transaction 的真实 selection mass |

### 关键代码路径

```rust
// rpc/service/src/service.rs:259-305
// - get_cell_set_by_address
// - get_balance_by_addresses  
// - get_cell_set_by_addresses

// rpc/service/src/service.rs:720-762
// - get_cells_by_address_call
// - get_balance_by_address_call
// - get_balances_by_addresses_call
```

### 剩余问题

| 问题 | 优先级 | 说明 |
|------|--------|------|
| context-free mass projection | P2 | `rpc-core` 的无上下文 `CellTx -> RpcTransaction` 转换仍只能做 deterministic fallback projection |

---

## 6. 钱包 SDK（Wallet）

### 6.1 状态: 72% 完备 ⭐⭐⭐

**结论**: 主体框架已可用，关键缺口已明显收敛；密钥元数据修改、gen1 导入和 `watch-only` 账户路径已闭环。但 canonical `CellTx` 默认不自动补脚本 `cell_deps`，所以“标准地址锁 + 钱包签名 + 共识脚本验证”的默认用户链路还不能算完全闭环。

### 已完成的组件

| 组件 | 状态 | 说明 |
|------|------|------|
| 钱包创建/打开 | ✅ | 完整实现 |
| 密钥管理 | ✅ | BIP32 派生、助记词 |
| 交易生成器 | ⚠️ | Generator 支持简单与批量交易，但 canonical `CellTx` 默认不自动补 script `cell_deps` |
| PSST 多签 | ✅ | 多签交易支持 |
| WASM/浏览器支持 | ✅ | 跨平台 SDK |
| 通知接口 | ✅ | register_notifications 已实现 |
| `PrvKeyData` 重命名 | ✅ | `set_name` 已实现并持久化 |
| gen1 钱包导入 | ✅ | 兼容解析、解密与账户导入已接通 |
| `watch-only` 账户 | ✅ | load / descriptor / 公开创建路径均已接通 |

### 关键代码路径

```rust
// wallet/core/src/wallet/api.rs:18-31
// - register_notifications: 已实现
// - unregister_notifications: 已实现

// wallet/core/src/tx/generator/generator.rs
// - 交易生成核心逻辑
```

### 剩余问题

| 问题 | 优先级 | 位置 | 说明 |
|------|--------|------|------|
| 标准地址锁 code dep 自动注入 | P1 | `wallet/core/src/tx/generator/generator.rs` | canonical `CellTx` 默认 `cell_deps` 为空，无法天然满足生产 VM 脚本加载 |
| 生产级签名锁对接 | P1 | `exec/src/scripts/` | 钱包默认地址锁尚无真实 secp256k1 ELF + dep 分发闭环 |
| 旧 `import_gen1_keydata` 入口 | P2 | `wallet/core/src/wallet/mod.rs` | 保留为显式迁移提示，真实导入入口已改为 `import_gen1_wallet_data` |
| 账户变体联调覆盖 | P2 | `wallet/core/src/` | `watch-only` / 多签 / 浏览器侧仍需更大范围集成测试 |

---

## 7. 挖矿与内存池（Mining/Mempool）

### 7.1 状态: 85% 完备 ⭐⭐⭐⭐

**结论**: 基本可运行，主路径完整。

### 已完成的组件

| 组件 | 状态 | 说明 |
|------|------|------|
| MiningManager | ✅ | 挖矿管理器 |
| BlockTemplateBuilder | ✅ | 区块模板构建 |
| CellPool | ✅ | Cell 交易池 |
| 交易 DAG 构建 | ✅ | 冲突检测与拓扑排序 |
| 挖矿规则引擎 | ✅ | 同步状态判断 |

### 关键代码路径

```rust
// mining/src/manager.rs
// - MiningManager 主逻辑

// mining/src/mempool/
// - CellPool 交易池管理
// - 交易选择与打包
```

### 剩余问题

| 问题 | 优先级 | 说明 |
|------|--------|------|
| Mempool cycles 前置裁剪 | P2 | block 预算预筛能力有限 |
| Template 路径优化 | P2 | 构块质量提升 |

---

## 8. P2P 网络层（P2P）

### 8.1 状态: 85% 完备 ⭐⭐⭐⭐

**结论**: 主路径完整，协议流处理就绪。

### 已完成的组件

| 组件 | 状态 | 说明 |
|------|------|------|
| 协议流处理 | ✅ | v5/v6/v7 版本支持 |
| 连接管理 | ✅ | ConnectionManager |
| 地址管理 | ✅ | AddressManager |
| 挖矿规则引擎 | ✅ | 网络状态判断 |
| IBD 流程 | ✅ | 初始区块下载 |

### 关键代码路径

```rust
// protocol/flows/src/
// - flow_trait.rs: 流接口定义
// - flow_context.rs: 流上下文
// - v6/mod.rs, v7/mod.rs: 协议版本注册

// components/connectionmanager/
// - 连接管理实现
```

### 剩余问题

| 问题 | 优先级 | 说明 |
|------|--------|------|
| 边缘网络场景 | P2 | 复杂网络拓扑测试 |

---

## 9. 已修复的关键问题（相对于早期审计）

### 9.1 P0 问题已解决 ✅

| 问题 | 早期状态 | 当前状态 | 修复说明 |
|------|----------|----------|----------|
| CellIndex 未接入 daemon | ❌ P0 断路 | ✅ 已解决 | `sporad` 启动时初始化 CellIndexer |
| RPC 查询返回 stub | ❌ P0 断路 | ✅ 已解决 | `get_cell_set_by_address` 等已实现 |
| txscript 依赖 | ❌ P0 误导 | ✅ 已解决 | `spora-txscript` 已从 workspace 删除 |
| VM placeholder | ❌ P0 骨架 | ✅ 已解决 | 真实 `ckb-vm` 已接入执行 ELF |

### 9.2 P1 问题已解决 ✅

| 问题 | 早期状态 | 当前状态 | 修复说明 |
|------|----------|----------|----------|
| CurrentCycles syscall | ❌ 返回 0 | ✅ 已修复 | 返回真实 `machine.cycles()` |
| Debugger 内存问题 | ❌ 破坏 guest 内存 | ✅ 已修复 | 改用 `load_bytes()` 读取 |
| LoadInput 错误码 | ❌ 语义错位 | ✅ 已修复 | 使用共享枚举解析 |
| 资源限制落地 | ❌ 未 enforce | ✅ 已修复 | tx/block cycles、script size、VM memory 已接入 |
| `GetCellReturnAddress` | ❌ 成功路径断路 | ✅ 已修复 | 现已返回 resolved input 的 lock address |
| `GetHeaders` | ❌ 返回未实现 | ✅ 已修复 | 现已返回从 `start_hash` 到 tip 的 header 列表 |
| `ResolveFinalityConflict` | ❌ 返回未实现 | ✅ 已修复 | 现已确认当前 finality point 并发出 resolved 通知 |
| 钱包 `set_name` | ❌ `NotImplemented` | ✅ 已修复 | rename API / transport / WASM 均已接通 |
| gen1 钱包导入 | ❌ placeholder | ✅ 已修复 | 新增兼容解析、解密与导入主路径 |
| `watch-only` 公开创建 | ❌ dormant/未接通 | ✅ 已修复 | factory / account kind / create API / WASM 已接通 |
| `LoadHeader` runtime | ❌ 最小字段子集 | ✅ 已扩展 | 已返回更完整的 header/runtime 字段 |
| 钱包通知链路 | ❌ 注册后断链 | ✅ 已修复 | `Wallet::notify()` / transport / WASM 事件面均已闭环 |

---

## 10. 剩余关键缺口与建议

### 10.1 P1 优先级（建议 2-4 周完成）

| 任务 | 影响 | 建议方案 |
|------|------|----------|
| 标准地址锁默认执行链路闭环 | 用户签名交易可用性 | 在 generator / mempool 提交流程中自动补齐脚本 `cell_deps` 或建立标准脚本 registry |
| 生产级签名锁落地 | 资金安全 | 提供真实 secp256k1/ecdsa ELF、稳定 code hash 与 dep 分发路径 |
| 补齐 VM syscall 语义 | CKB 兼容性 | 扩展 `LoadHeader` 等 syscall 字段覆盖 |
| 扩展钱包账户变体集成测试 | SDK 稳定性 | 覆盖 `watch-only`、多签、WASM 调用链 |

### 10.2 P2 优先级（建议 1-2 月完成）

| 任务 | 影响 | 建议方案 |
|------|------|----------|
| 更新历史审计文档 | 避免误导开发者 | 在文档顶部添加"当前状态"标注 |
| 扩展集成测试 | 系统稳定性 | 覆盖 consensus/mempool/template 联动场景 |
| CellStateTree 性能优化 | 大规模状态性能 | 优化 `root()` 计算复杂度 |
| Mempool cycles 前置裁剪 | 构块质量 | 增加 block 预算预筛逻辑 |

---

## 11. 业务流程端到端验证

### 11.1 交易生命周期流程

```
用户创建交易
    ↓
钱包 SDK 签名 (⚠️ 有条件可用)
    ↓
RPC 提交交易 (✅ 可用)
    ↓
Mempool 接收与验证 (⚠️ 标准地址锁默认链路未完全闭环)
    ↓
矿工打包区块 (✅ 可用)
    ↓
共识验证 (⚠️ VM 可执行，但生产签名锁仍未落地)
    - 隔离验证
    - 上下文验证
    - DAG 验证
    - VM 脚本验证
    ↓
状态更新 (✅ 可用)
    ↓
钱包通知 (✅ 可用)
```

### 11.2 查询流程

```
用户查询余额
    ↓
RPC get_balance_by_address (✅ 可用)
    ↓
CellIndex 查询 (✅ 已接入)
    ↓
返回真实余额 (✅ 可用)
```

---

## 12. 最终结论

### 12.1 状态判断

> **Spora 已从"能跑"进入"基础设施可用"阶段，但用户资金默认保护路径还未完全闭环；距离"生产级"大约还差最后 20-25% 的收尾工作。**

### 12.2 关键指标

| 指标 | 早期审计 | 当前状态 | 变化 |
|------|----------|----------|------|
| 共识成立性 | ❌ 不成立 | ✅ 已成立 | +++ |
| 查询层可用性 | ❌ 断路 | ✅ 已接通 | +++ |
| VM 执行 | ❌ 骨架 | ✅ 真实执行 | +++ |
| 钱包完整性 | ⚠️ 60% | ⚠️ 72% | ++ |
| 文档一致性 | ⚠️ 60% | ⚠️ 80% | ++ |

### 12.3 建议的下一步

1. **立即（本周）**: 闭环标准地址锁默认执行链路，明确脚本 `cell_deps` 自动注入策略
2. **短期（2-4 周）**: 落地真实 secp256k1/ecdsa 锁 ELF，并补齐 VM syscall 完整语义
3. **中期（1-2 月）**: 扩展钱包账户变体与浏览器/WASM 集成测试，优化性能瓶颈
4. **长期（3-6 月）**: 生产环境验证，安全审计

---

## 附录 A: 关键文件映射

| 组件 | 关键文件 |
|------|----------|
| 共识核心 | `consensus/src/pipeline/virtual_processor/processor.rs` |
| Cell 验证 | `consensus/src/processes/cell_validator/mod.rs` |
| VM 执行 | `exec/src/vm/machine.rs`, `exec/src/vm/verifier.rs` |
| CellIndex | `sporad/src/daemon.rs`, `indexes/cellindex/` |
| RPC 服务 | `rpc/service/src/service.rs` |
| 钱包 API | `wallet/core/src/wallet/api.rs` |
| 交易生成 | `wallet/core/src/tx/generator/generator.rs` |
| 挖矿 | `mining/src/manager.rs` |
| P2P 协议 | `protocol/flows/src/flow_context.rs` |

## 附录 B: 审计方法说明

本次审计采用**倒序审计法**：
1. 从用户视角出发，检查应用层功能可用性
2. 向下追溯至 RPC、索引层
3. 最后验证共识、VM 等底层实现
4. 对比早期审计文档，识别进展与剩余缺口

审计工具：
- 静态代码分析
- 文档对比分析
- 历史审计交叉验证
- 关键路径代码审查

---

*报告完成时间: 2026-04-13*  
*下次建议审计时间: 修复 P1 问题后（约 2-4 周后）*
