# CKB 时代历史包袱报告与 Spora 现代化建议

- 日期: 2026-04-13
- 状态: Draft
- 目的: 识别 CKB 社区长期反复承认、并已通过 OTX / CoBuild / CCC / Fee abstraction / Light client 讨论持续修补的历史包袱；区分哪些是应删除的兼容负担，哪些是应保留但要重新包装的资源约束；给出 Spora 的现代化建议。

## 1. 结论

如果只压成一句话：

**CKB 时代最大的历史包袱，不是 Cell 模型本身，而是“没有一套被强力收敛的高层协作合同”，导致交易构造、脚本依赖、witness 组织、地址语义和客户端职责长期裸露给钱包、SDK 和应用层自己拼。**

更具体地说，CKB 真正反复暴露出来的问题是：

1. **交易构造的机械细节长期外溢**
2. **地址/账户/认证没有被干净拆开**
3. **个别协议特例会反向污染工具链心智**
4. **资源约束是合理的，但被直接暴露成 UX 税**
5. **部分客户端架构默认桌面环境，而不是 mobile-first**

我的判断是：

- Cell 不是该被“去掉”的东西
- `Script { code_hash, hash_type, args }` 这种底层表达力也不是该被“简化掉”的东西
- 真正该被删掉的，是历史上把这些能力直接推给应用层处理的那一圈机械接口

换句话说，Spora 不该做“更现代的 CKB 工具堆”，而该做“把 CKB 值钱的底层保留，把 CKB 历史性外溢的复杂度收回系统内部”。

## 2. 为什么最大包袱不是 Cell 本身

CKB 官方与社区材料本身已经给出一个很清晰的信号：

1. 官方词汇表仍把地址定义为对 Lock Script 的封装，而不是账户对象。
2. 官方签名文档仍要求开发者理解 input group、witness 对齐、`WitnessArgs`、占位锁字段和不同锁脚本的签名组织。
3. CoBuild / OTX 系列讨论反复强调：CKB 应用的状态在链下生成、链上验证，客户端要深度参与交易处理，协作式交易构造几乎就是应用执行过程本身。
4. CCC 被官方明确推荐为主 JS/TS SDK，并提供补 input、补 fee 的高级接口，说明生态正在主动把底层交易机械细节往 SDK 内部回收。

这些迹象共同说明：

- 社区真正要修的不是 Cell 的表达力
- 而是“如何不让每个钱包、SDK、应用都各自实现一套 Cell 时代的交易工程学”

## 3. 核心包袱一：交易构造、脚本依赖、witness 组织外溢到应用层

这是我认为最重、最系统性的历史包袱。

### 3.1 包袱的具体表现

在 CKB 的传统心智里，一个“普通转账”不是一个高层动作，而是一连串低层工程步骤：

1. 查询 live cells
2. 选择 inputs
3. 计算 fee
4. 生成 change
5. 处理 cell deps / header deps
6. 对输入做 script grouping
7. 组织 witness
8. 根据具体 lock/type script 的要求填不同字段

官方签名文档至今仍在直接教授这些细节，包括：

1. 以 `script_hash` 为单位分 input group
2. 在每个 group 的首 witness 里塞占位 lock
3. 对首 witness 和组内其他 witness 分别参与哈希
4. 用 `WitnessArgs` 解决 lock/type script 争用同一 witness 的冲突

这不是“文档写得详细”而已，而是说明这些低层结构长期属于开发者必学内容。

### 3.2 社区为什么会往 OTX / CoBuild / CCC 方向收敛

社区后续几个关键动作，本质上是在修同一个洞：

1. **OTX**
   - 承认交易可以由多方协作构造
   - 把用户动作和最终完整交易分开
2. **CoBuild**
   - 承认 CKB 客户端深度参与应用交易处理
   - 尝试标准化 Building / Signing / WitnessLayout / Message / ScriptInfo
3. **CCC**
   - 试图把“补 input / 补 fee / 发交易”收口成统一 SDK 接口

这里最关键的一点不是“又多了几个工具”，而是：

**CKB 生态自己已经默认接受：旧时代让每个项目各写一套 tx skeleton / signing pipeline / witness 组装的方式不可持续。**

### 3.3 真正的旧税

旧税不在于 CKB 有 off-chain construction。

旧税在于：

1. 链下构造没有一个足够强的 blessed contract
2. 中间表示长期碎片化
3. witness 结构过于接近底层验证逻辑
4. 应用开发者过早接触 input / witness / deps / fee / change 这些机械对象

所以最大的历史包袱可以更精确地表达为：

**低层自由度存在，但高层协作合同长期缺位。**

## 4. 核心包袱二：地址、账户、认证三者长期混在一起

### 4.1 这不是能力问题，而是抽象问题

CKB 的地址本质上就是 lock script 的封装。这个设计本身没错，甚至很强。

问题在于：

1. 地址天然更接近“认证方法”
2. 用户和应用却常把地址当“账户身份”
3. 钱包和工具经常默认“一账号一地址”的使用方式

这会带来两个后果：

1. 丢掉 UTXO / Cell 世界本来有的多地址隐私优势
2. 让认证方式、接收端点、账户身份绑死在一起

2025 年的社区讨论已经把这个问题说得很直白：

1. CKB 现在常被以 1:1 account-address 的方式使用
2. 这种做法牺牲隐私，掩盖 UTXO 优势
3. 更合理的方向是把 account、authentication、address 拆开

### 4.2 我的判断

“地址只是 lock script 的包装”本身不是包袱。

真正的包袱是：

**把这个底层事实直接当成默认产品心智。**

也就是：

1. 协议层是 script-centric
2. 钱包层却没有给出稳定的 account-centric / identity-centric 视图
3. 结果开发者和用户都不得不直接接触脚本时代的残留语义

对 Spora 来说，最该吸取的教训是：

1. 协议层保留 script tuple 抽象
2. 钱包层默认暴露 account / auth / endpoint，而不是 script packaging
3. 地址应是可轮换接收端点，不应天然承担账户身份角色

## 5. 核心包袱三：NervosDAO 这类协议特例会反向污染工具链

### 5.1 为什么 NervosDAO 是“沉积层”问题

NervosDAO 的问题不在于“有一个存款/收益产品”。

真正的问题是：

1. 它是一个高度协议化、历史负担很重的特殊流程
2. 它需要特殊 type script 语义
3. 在 CoBuild 语境下，如果不通过硬分叉升级原脚本，就必须在工具链里专门硬编码对 DAO 的预处理和 Action 校验逻辑

这说明什么？

说明 **协议特例已经不仅存在于链上，还会反向污染链下标准化过程**。

### 5.2 我的判断

Spora 如果要吸取教训，应避免引入这种形态的“一次性神圣特例”：

1. 如果确实需要协议级储蓄、锁仓、收益或治理模块
2. 应把它设计成 generic capability / builtin action / standard interface
3. 不要让钱包、builder、indexer 以后都额外背一层“遇到这个协议对象就走特殊分支”的工具链税

一句话：

**DAO 不是因为有金融功能而成为包袱，而是因为它让工具链必须知道它是“那个特殊对象”。**

## 6. 核心包袱四：61 CKB 最小 cell 不是该删除的包袱，而是该重新包装的约束

### 6.1 必须区分“约束”和“包袱”

61 CKB 最小 cell 常被用户体验问题放大，但它和前面几类问题不完全一样。

它首先是一个**资源经济约束**：

1. 每个 cell 占据链上状态空间
2. 容量与字节绑定
3. 状态空间必须有经济成本

社区近年的讨论也明确指出：

1. 这不是 bug
2. 这是把状态和经济成本绑定的设计

所以从协议哲学上说，61 CKB 这一类“occupied capacity floor”并不是应该被简单删除的对象。

### 6.2 真正的问题是什么

真正的问题是，这个合理约束长期被直接暴露成应用层 UX 税：

1. 微支付不自然
2. 交易所和钱包需要为接收体验预付大量 cell 成本
3. UDT 接收经常要求对方或自己先准备 CKB 容量
4. ACP、relay cell、集中托管池、聚合中转这些方案不断出现，本质上都在修“谁来承担最小 cell 的前台成本”

因此这里应当得出的结论不是“把状态成本删掉”，而是：

**保留状态成本，但绝不把它原封不动地暴露成默认用户交互。**

### 6.3 对 Spora 的建议

Spora 应保留“状态有成本”这条原则，但可以把 61 CKB 时代的历史痛点拆开处理：

1. 保留 occupied-capacity / storage-rent / state-cost 一类抽象
2. 不要求每个接收动作都显式创建独立最小状态单元
3. 默认支持聚合接收、共享容器、延迟结算、赞助初始化、fee payer / storage payer
4. 对小额接收默认给出 pooled / rollup / off-chain credit / deferred claim 模式

也就是说：

**应保留资源约束，不保留“每次交互都让普通用户直面最小 cell 经济学”的旧体验。**

## 7. 核心包袱五：客户端架构一度不够 mobile-first

社区关于 Android / iOS light client 的讨论说明，CKB 一些历史实现默认的是：

1. 桌面级网络模型
2. 桌面级存储模型
3. 浏览器 / WASM 的强运行时前提
4. 长连接、后台运行和较高资源预算

而移动端实践暴露了：

1. Tentacle/ckb-network 在 Android 上存在适配问题
2. 浏览器版 light client 依赖 `SharedArrayBuffer`、`Atomics.wait`、Worker + IndexedDB、跨域隔离等环境条件，无法直接嵌入 Android WebView
3. RocksDB 和 full gossip 对移动端资源预算过重
4. 社区后来又明确把 SQLite 替换 RocksDB 视为移动端改进方向

这说明历史包袱不是“没有移动端”，而是：

**早期默认前提不是 constrained-environment-first。**

对 Spora 来说，这意味着现代化不能只改地址和 SDK，还必须从一开始决定：

1. 传输层是否 mobile-friendly
2. 存储层是否轻量化
3. 同步模型是否可恢复、可暂停、可分层
4. 验证核心是否能嵌入多环境

## 8. 哪些是该删的兼容负担，哪些是必须保留的约束

### 8.1 应删除的兼容负担

Spora 应主动删除以下历史负担：

1. **把 raw tx construction 当成应用层默认接口**
2. **让 witness 细节成为大多数开发者必须理解的主路径心智**
3. **让地址承担账户身份**
4. **让认证方式直接暴露成地址族分叉**
5. **让个别协议对象要求工具链写特殊分支**
6. **允许多个 builder / wallet / SDK 长期维持彼此不兼容的中间表示**
7. **把 fee 补齐、change 生成、input 选择当成临时脏活而不是标准阶段**
8. **把桌面 full-node 假设带到移动端和受限环境**

### 8.2 应保留但重新包装的约束

Spora 应保留以下原则，但必须换一种上层表达方式：

1. **Cell / UTXO 式状态对象**
2. **Script tuple / capability-based verification**
3. **状态生成链下、验证链上**
4. **状态空间有成本**
5. **交易需要显式资源支付**
6. **不同验证能力可以共存**

重点不是删掉这些约束，而是：

**不能让普通应用直接感知这些约束的底层机械形状。**

## 9. 我对 Spora 的具体建议

### 9.1 定义唯一 blessed pipeline

从第一天开始就收敛成一条主路径：

1. `Intent / Action`
2. `Cell Plan`
3. `Tx Plan`
4. `Witness Obligations`
5. `Execution Proof / Signatures`

不要再允许：

1. builder 一套中间格式
2. wallet 一套中间格式
3. signer 一套中间格式
4. RPC / indexer 再隐式发明一套“准交易格式”

### 9.2 把 witness 从“拼装对象”升级为“义务对象”

不要让应用自己思考：

1. 第几个 witness
2. 哪个 input group
3. 首 witness 对齐
4. placeholder lock

应用只应该知道：

1. 哪个 capability 需要谁证明
2. 证明对象是什么
3. 签名/证明的可读语义是什么

也就是把 `witness` 从“字节布局问题”提升成“证明义务问题”。

### 9.3 account / auth / address 三层分离

建议主路径直接这样定义：

1. **Account**
   - 应用识别和持有关系的对象
2. **Auth**
   - Schnorr / ECDSA / hardware / social recovery / passkey / multisig 等认证适配器
3. **Address / Endpoint**
   - 可轮换、可一次性、可场景化的接收端点

这样你就不会再掉回“地址看起来像账户”的旧坑。

### 9.4 fee payer / storage payer 一等公民化

CKB 时代一个典型问题是，手续费和容量准备常常是 builder 最后偷偷补上的脏活。

Spora 应直接协议化或标准化以下角色：

1. fee payer
2. storage payer
3. sponsor
4. conversion payer

这会直接抹掉一大块“交易构造工程税”。

### 9.5 协议特例只能走 generic capability

如果 Spora 未来要做：

1. 锁仓
2. 储蓄
3. 收益
4. 治理
5. 赞助支付

都应走：

1. 标准 action
2. 标准 script descriptor
3. 标准 capability interface

不要再制造第二个“DAO 必须被工具链特别认识”的对象。

### 9.6 mobile-first 不是附加优化，而是架构前提

Spora 若要避免重蹈覆辙，需要从一开始假设：

1. QUIC / WebSocket 友好传输
2. SQLite 级别本地存储
3. 可中断可恢复的同步
4. 可裁剪的 verifier core
5. 钱包可以在弱设备、弱网络和弱权限环境下工作

## 10. 最终判断

我的最终判断是：

**CKB 时代最深的包袱，不是 Cell 太复杂，而是“低层表达力很强，但高层合同长期不够收敛”，于是复杂度从协议边界渗到了钱包、SDK、应用和用户心智里。**

因此，Spora 的 modernize 方向不应是“去 Cell 化”，而应是：

1. **保留 Cell 的语义力量**
2. **保留 Script 的可编程性**
3. **保留状态成本约束**
4. **删除交易工程细节对外暴露**
5. **删除地址即账户的旧心智**
6. **删除协议特例反向污染工具链的空间**
7. **从第一天就给出唯一 blessed 的高层协作合同**

一句话总结：

**要 modernize 的不是 Cell 本体，而是围绕 Cell 长出来的历史接口。**

## 11. 对当前仓库的直接启发

这份判断与仓库里已经出现的几条本地结论是相互强化的：

1. [SPORA_ADDRESS_LOCK_MODERNIZATION_PROPOSAL.md](/Users/arthur/RustroverProjects/Spora/docs/SPORA_ADDRESS_LOCK_MODERNIZATION_PROPOSAL.md)
   - 方向正确：协议层保留 CKB 式脚本抽象，产品层隐藏脚本细节
2. [CELL_MASS_UNIFICATION_PROPOSAL.md](/Users/arthur/RustroverProjects/Spora/docs/CELL_MASS_UNIFICATION_PROPOSAL.md)
   - 方向正确：交易对象不应再承担散落资源真相
3. [spora_consensus_v2_gap_analysis.md](/Users/arthur/RustroverProjects/Spora/docs/spora_consensus_v2_gap_analysis.md)
   - 方向正确：`body validation` / `mempool` / `template` / VM data provider 应继续收敛为单一状态转移引擎

如果要继续推进，我建议接下来直接把工作拆成四条线：

1. `Intent / Action / Witness Obligation` 统一中间表示
2. `Account / Auth / Address` 分层
3. `Fee / Storage / Sponsorship` 支付策略标准化
4. `Mobile / Light Client / Constrained Runtime` 架构前置化

## 12. 参考材料

以下材料直接支持本文判断：

1. 官方文档：CCC 已被明确写成主 JS/TS SDK，并提供统一接口完成 input 和 fee 补齐  
   https://docs.nervos.org/docs/sdk-and-devtool/ccc
2. 官方文档：签名教程仍要求开发者理解 P2PKH / Multisig、input group、`WitnessArgs`、group 首 witness 等底层组织方式  
   https://docs.nervos.org/docs/how-tos/how-to-sign-a-tx
3. 官方词汇表：地址是对 Lock Script 的封装，Lock Script 与地址一一对应  
   https://docs.nervos.org/docs/tech-explanation/glossary
4. OTX 原型总结：基础转账也要处理 live cells、fee、change；`ckb-cli` JSON 与 Lumos `TransactionSkeleton` 等专有中间格式阻碍协作  
   https://talk.nervos.org/t/exploring-the-ckb-otx-paradigm-accomplishments-and-insights-from-building-a-transaction-streaming-prototype/7346
5. CoBuild 协议概览：CKB 客户端深度参与交易处理，协作式交易构造近似应用执行过程；目标是降低开发门槛、标准化 witness layout 与 building packet  
   https://talk.nervos.org/t/ckb-transaction-cobuild-protocol-overview/7702
6. OTX / CoBuild 概览：多方协作与 off-chain building packet 被标准化，fee/packing 也成为显式角色  
   https://talk.nervos.org/t/ckb-open-transaction-otx-cobuild-protocol-overview/7739
7. CoBuild 对 NervosDAO 的讨论：若不升级原 DAO 脚本，工具链需要硬编码特殊处理  
   https://talk.nervos.org/t/ckb-transaction-cobuild-protocol-overview/7702/11
8. 地址/账户/认证讨论：社区明确指出 1:1 account-address 使用方式牺牲隐私，并倡议将 account 与 authentication 解耦  
   https://talk.nervos.org/t/account-authentication-and-addresses/8983
9. 61 CKB 相关讨论：这是状态成本约束，但在微支付、交易所充提、UDT 接收等场景造成明显 UX 税  
   https://talk.nervos.org/t/microtipping-on-ckb-an-elegant-solution-to-the-cell-minimum-problem/10087  
   https://talk.nervos.org/t/rfc-anyone-can-pay-lock/4438  
   https://talk.nervos.org/t/a-resource-economic-solution-for-exchange-deposit/3804
10. 移动端 light client 讨论：现有 light client 架构在 Android / WebView / RocksDB / P2P 假设上暴露 desktop-first 包袱，后续又通过 SQLite 等改进修补  
    https://talk.nervos.org/t/bringing-ckb-light-client-to-mobile-devices-android-and-ios-current-limitations-findings-and-a-path-toward-a-mobile-ready-p2p-standard/9693  
    https://talk.nervos.org/t/update-ckb-light-client-on-mobile/9865
