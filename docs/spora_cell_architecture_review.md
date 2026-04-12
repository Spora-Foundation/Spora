# Spora 共识 × Cell 模型 架构评审

## 文档定位

这份文档评审的是 Spora 当前“GHOSTDAG 共识 + Cell 模型”这套架构设计本身，而不是单纯的实现 bug 列表。

它关注的问题是：

- 这套抽象是否优雅
- 协议对象是否统一
- 当前实现是否已经进入真正的 Cell-native 完成态

结论先行：

> **共识内核已经具备很强的 Cell-native 设计感，但系统外围仍被 legacy Transaction 语义牵制。**

因此，这套系统今天更准确的描述是：

> **Cell 共识内核 + legacy 适配外壳**

而不是“从钱包到共识都已经完全 Cell-native”。

## 总体评分

综合评价：

**70 / 100**

判断依据：

- 骨架优雅
- 内核方向正确
- 关键协议对象仍未统一
- 交易对象、签名、coinbase、mass、脚本系统仍在过渡态

## 架构上的优雅之处

### 1. DAG 与 Cell 状态语义天然契合

相比传统 legacy txout，CellDiff 的 `add/remove` 组合在 DAG 中更适合表达：

- selected parent 状态
- mergeset 的增量变化
- reorg 时的前进 / 回滚

`CellDiff` 与确定性有序结构结合后，状态演化比传统 legacy txout diff 更自然，也更容易做一致性推导。

### 2. Header 承诺分层是正确方向

当前头部同时拥有：

- `cell_root`
- `cell_commitment = H("spora/cell_commitment/v0" || cell_root)`

这比把所有含义压进一个 commitment 里更干净。

好处是：

- `cell_root` 可用于状态证明
- `cell_commitment` 可用于协议版本化演进
- 未来可以更平滑地升级到带 history root / multi-commitment 的格式

### 3. VirtualState 的 Cell 语义已经成型

当前共识主路径里的核心状态对象已经围绕 Cell 模型构建：

- `cell_state_tree`
- `cell_diff`
- `accepted_tx_ids`
- `mergeset_rewards`

这说明系统的真正共识核心已经从旧 legacy txout 语义迁移出来，不再只是“外面包了一层 Cell 名字”。

### 4. POV-aware 状态视角是对的

在 DAG 中，Cell 是否 live 不能只靠 DAA score 决定。

当前设计已经逐步收敛到：

- 以 `selected_parent` / `POV block` 为状态视角
- 以 `cell_diff` 重建视角状态
- 以 `cell_root` 和 commitment 验证状态承诺

这是协议层正确方向，也是相比旧设计最重要的进步之一。

## 最大的架构债

### F1. Transaction / CellTx 双轨制

这是当前最不优雅、也最值得优先消灭的一层。

系统今天同时存在两套交易身份：

- 共识 / block / 持久化使用 `CellTx`
- 钱包 / mempool / 兼容层仍残留 legacy `Transaction`

这直接导致：

- transaction id 与 cell tx id 并存
- mempool 和 accepted block 之间需要做映射和 relink
- orphan / unorphan 要靠兼容桥接而不是统一状态机
- 许多本不该存在的转换函数成为系统关键路径

这不是“兼容设计得很优雅”，而是“迁移仍未完成”。

执行上也不应继续打磨这层桥接。正确做法是直接删掉主路径对 legacy `Transaction` 的依赖，把断点留给调用方修复，而不是再做一层“薄兼容壳”。

### F2. Coinbase 仍走 legacy 路径

当前 coinbase 不是原生生成 `CellTx`，而是先生成 legacy `Transaction`，再转换成 `CellTx`。

这带来两层问题：

1. 结构上不干净  
2. 协议上容易埋唯一性风险

尤其 coinbase `CellTx id` 的唯一性，必须由协议对象本身保证，而不是长期依赖模拟器、extra data 或外围拼装习惯去“凑不同”。

如果 coinbase 的 identity 不是协议内生定义，那么它迟早会成为重复 outpoint 或模板构造歧义的来源。

### F3. Mass / fee 模型尚未 Cell-native

Cell 模型允许 data 承载状态，因此它比 legacy txout 更需要完整的费用 / 资源模型。

当前问题在于：

- mass 语义仍不完整
- storage 维度约束不足
- RPC / 兼容层也还残留 workaround 逻辑

如果长期没有 Cell-native mass，那么系统会出现：

- data-carrying cell 的经济约束不足
- fee market 与状态占用不匹配

### F4. 签名路径仍未统一

这是协议完成态之前必须处理的一项。

当前系统里仍存在：

- legacy signing 路径
- CellTx sighash 路径
- wallet / SDK / VM 脚本验证之间的迁移痕迹

即便当前主路径已经比之前收口很多，只要交易对象和签名对象还不是一个统一真相，系统就仍处于过渡态，而不是规范闭环态。

### F5. Type script 仍未真正成为系统能力

如果系统目标是“CKB 风格 Cell 模型”，那么只拥有：

- lock
- capacity
- data

还不够。

Cell 模型最强的协议能力之一，是把状态转移规则也作为 Cell 的一部分。

如果 type script 长期缺位，那么这套设计更接近：

> 带 data 的 legacy txout / Cell-like output

而不是完整意义上的 Cell 协议。

## 这套设计现在到底算什么阶段

当前不是“架构失败”，也不是“已经完美”。

更准确的分层判断是：

### 已经成立的部分

- 共识主路径已经 Cell-native
- virtual state / cell commitment / reorg 主逻辑已经围绕 Cell 运转
- DAG 里的状态承诺与状态视角已经不再依赖旧 legacy txout 模型

### 仍处于迁移态的部分

- 钱包交易对象
- mempool / orphan 语义
- coinbase 生成
- mass / fee 规则
- 签名真相
- type script / 完整 VM 能力

因此，当前最准确的判断是：

> **协议内核是新的，系统边界还是旧的。**

## 与 CKB / Kaspa 的关系

### 相比 CKB

Spora 继承的是：

- Cell 状态对象
- lock / type / data 的抽象方向
- Cell-based 状态转移语义

但又明显不同于 CKB：

- 它的链不是 linear chain，而是 DAG
- 状态视角必须显式依赖 `selected_parent` / `POV`
- accepted order 和 mergeset 参与状态构造

所以它不是“CKB 搬到 DAG”，而是“把 CKB 风格状态机重写到 GhostDAG 上”。

### 相比 Kaspa

Spora 继承的是：

- GhostDAG / selected parent / virtual block 语义
- accepted set / mergeset reward 这套 DAG 共识结构

但又明显比 Kaspa 更重：

- 状态对象不再只是 legacy txout 集
- 交易执行和脚本验证语义更复杂
- 状态承诺比单一 legacy txout commitment 更强

因此它也不是“Kaspa 换个交易格式”，而是“在 Kaspa 式 DAG 共识上挂了一个更强的状态机”。

## 通向更完整协议的顺序

### 第一阶段: 统一唯一交易对象

目标：

- 共识、钱包、mempool、coinbase 全部只认 `CellTx`

必须做掉的事：

- 删除 `legacy Transaction -> CellTx` 关键路径依赖
- 让 accepted block、mempool、wallet 使用同一 transaction identity
- 迁移过程中不再新增桥接 helper；任何遗留调用点都直接改到 `CellTx`

这是当前收益最高的一步。

### 第二阶段: coinbase 原生 CellTx 化

目标：

- coinbase 直接生成为 `CellTx`
- 把唯一性来源写进协议对象本身

完成后，coinbase 不应再依赖外围兼容层或临时 extra data 习惯来避免碰撞。

### 第三阶段: 统一 sighash / witness / wallet signing

目标：

- 钱包签名对象与共识验证对象一致
- 删除 legacy 签名真相

这是脚本系统和钱包系统真正闭环的前提。

### 第四阶段: 建立 Cell-native mass / fee 语义

目标：

- 把 compute cost 和 state occupancy 同时纳入约束
- 明确 data-carrying cell 的经济成本

没有这一步，Cell 模型的资源定价是不完整的。

### 第五阶段: 启用完整 type script / VM 协议能力

目标：

- 让 type script 成为第一等协议对象
- 明确 provider、dep、hash_type、script execution 的完整规则

做到这一步之后，Spora 才真正具备“Cell 协议”的全部能力，而不是只有其状态外形。

## 最终结论

如果问题是：

> “这套共识 × Cell 架构是不是设计得很差？”

答案是否定的。

它的内核其实相当好，而且方向正确。

如果问题是：

> “它是不是已经进入了完美的 Cell-native 完成态？”

答案也是否定的。

今天这套系统最准确的判断是：

> **共识核心已经 Cell-native，系统外围仍在从 legacy 交易体系向 Cell 协议迁移。**

所以接下来的重点不该是继续为旧对象缝补桥接层，而是要系统性地消灭双轨制，统一：

1. 交易对象
2. coinbase 对象
3. 签名对象
4. 费用模型
5. 脚本执行对象

当这几项全部统一之后，Spora 才会从“一个优秀的过渡态设计”进入“真正完整的 Cell DAG 协议”。
