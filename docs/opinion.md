The fake requirements are basically all the things that make people say “wow, this feels like a real modern language,” while quietly destroying the reason CellScript should exist in the first place.
最危险的伪需求，本质上就是那些会让人短期惊呼“哇，这像一门真正现代的语言了”，但同时悄悄把 CellScript 存在理由掏空的东西。

If CellScript’s strategic edge is:

* explicit cell/effect semantics
* predictable lowering
* auditability
* thin consensus surface
* easy static analysis
* good fit for DAG scheduling / conflict detection

then every feature that weakens those should be treated as guilty until proven innocent.
如果 CellScript 的战略优势是：

* cell / effect 语义显式
* lowering 可预测
* 易审计
* 共识面薄
* 易做静态分析
* 非常适合 DAG 调度 / 冲突检测

那任何会削弱这些优势的功能，都应该先被当成“有罪嫌疑”看待，而不是默认欢迎。

---

# 先给你最核心的一句话

**Anything that hides effects, hides state, hides cost, or hides control flow is probably a fake requirement.**
**凡是会隐藏 effect、隐藏状态、隐藏成本、隐藏控制流的功能，十有八九都是伪需求。**

That is the knife.
这就是刀口。

---

# 第一类伪需求：为了“像 Rust / Swift / Kotlin”而引入的通用语言豪华件

This is the most common death path.
这是一条最常见的死亡路线。

People start saying:

* we need traits
* we need rich generics
* we need operator overloading
* we need async
* we need closures everywhere
* we need object-like interfaces
* we need nice containers
* we need standard collections

And then suddenly CellScript is no longer a contract DSL with explicit semantics; it becomes a bad general-purpose language.
人们会开始说：

* 我们需要 traits
* 我们需要丰富 generics
* 我们需要 operator overloading
* 我们需要 async
* 我们需要到处都是 closures
* 我们需要对象式接口
* 我们需要漂亮的容器
* 我们需要标准集合库

然后突然之间，CellScript 就不再是一个语义显式的合约 DSL，而变成了一门蹩脚的通用语言。

## 具体危险件

### 1) Rich trait/typeclass system

Why dangerous:

* implicit dispatch paths
* harder reasoning about called implementation
* bloats compiler complexity
* invites abstraction towers instead of explicit logic

### 1）很重的 trait / typeclass 系统

危险点：

* 引入隐式派发路径
* 更难推理到底调用了哪个实现
* 编译器复杂度暴涨
* 鼓励抽象高塔，而不是显式逻辑

A tiny interface mechanism may be okay. A full trait universe is usually poison.
一个很薄的 interface 机制也许可以；完整 trait 宇宙通常是毒药。

### 2) Powerful higher-kinded generics / advanced polymorphism

Why dangerous:

* enormous implementation complexity
* harder errors
* less readable audits
* encourages library cleverness over contract clarity

### 2）强力高阶泛型 / 高级多态

危险点：

* 实现复杂度极高
* 错误信息恶化
* 审计可读性下降
* 鼓励“库技巧”压过“合约清晰度”

For chain code, generics should mostly be a code deduplication device, not a philosophy.
对链上代码来说，泛型最好主要只是减少重复代码的手段，而不是一种哲学。

### 3) Operator overloading

Looks harmless. Often not.
看起来无害，实际上经常有害。

Why dangerous:

* hides semantics
* makes audits slower
* lets “business syntax” disguise real effects or costs

### 3）操作符重载

看似小功能，实则很容易带毒。

危险点：

* 隐藏语义
* 让审计变慢
* 让“业务写法”掩盖真实 effect 或成本

If `+` can mean hash merge, amount combine, witness concat, or lazy object construction, you’ve already lost.
如果 `+` 既可能表示数值加法，也可能表示 hash merge、amount combine、witness concat、甚至某种惰性对象构造，那你已经输了。

---

# 第二类伪需求：隐藏状态与 effect 的“友好抽象”

This is even more dangerous than fancy type systems.
这类比花哨类型系统还危险。

CellScript should make state interaction painfully visible in a good way.
CellScript 应该让状态交互以一种“健康的痛感”变得明显。

Anything that turns:

```text
read input cell X
check lock hash
read witness Y
declare output mutation Z
```

into:

```text
wallet.transfer(to, amount)
```

is suspicious by default.
任何把：

```text
读 input cell X
检查 lock hash
读 witness Y
声明 output mutation Z
```

包装成：

```text
wallet.transfer(to, amount)
```

的设计，都默认可疑。

### 4) Implicit storage access

Why dangerous:

* kills auditability
* kills parallel/conflict analysis
* hides gas/cycle expectations
* encourages magical SDK thinking

### 4）隐式 storage 访问

危险点：

* 直接破坏可审计性
* 破坏并行 / 冲突分析
* 隐藏 gas / cycle 预期
* 鼓励“魔法 SDK 思维”

If reading a cell is not obvious in source, the language is drifting in the wrong direction.
如果源码里读了一个 cell 却不明显，那语言已经开始跑偏。

### 5) Implicit witness / dependency injection

Very seductive. Very bad if overdone.
非常诱人，但一旦过度，后果很差。

Example bad pattern:
“Just ask for `Context.currentUser()` and the runtime figures it out.”

### 5）隐式 witness / dependency 注入

典型坏味道是：
“你只管写 `Context.currentUser()`，剩下运行时自己懂。”

That is exactly how hidden consensus surfaces grow.
这正是隐藏共识面的典型生长方式。

A contract language should not behave like a web backend framework.
合约语言不应该像一个 web backend framework。

### 6) Auto-derived effects

If effects are inferred too aggressively and not surfaced clearly, developers stop thinking in effect space.
如果 effect 被过度自动推导，而且没有被清楚展示出来，开发者就会停止用 effect 维度思考。

You may still do inference internally, but the source-facing model should keep effects visible.
你内部当然可以做 inference，但面向源码的模型一定要让 effect 可见。

---

# 第三类伪需求：会把控制流搞脏的高级便利特性

CellScript needs control flow that is easy to reason about statically.
CellScript 需要一种在静态上容易推理的控制流。

### 7) Exceptions / try-catch style unwinding

This is almost always a bad trade in consensus code.
这在共识代码里几乎总是个坏交易。

Why dangerous:

* non-local control flow
* difficult auditing
* hidden cleanup semantics
* hard effect reasoning

### 7）异常 / try-catch 式栈展开

危险点：

* 非局部控制流
* 审计困难
* 隐藏清理语义
* effect 推理困难

Prefer explicit `Result`-style failure or simple abort semantics.
更好的方式是显式 `Result` 风格失败，或者非常简单的 abort 语义。

### 8) Closures everywhere / capturing semantics

Small lambdas for local transforms may be okay.
Tiny local lambdas maybe. Full closure culture, no.
局部小 lambda 也许还能接受。
但完整 closure 文化，最好别来。

Why dangerous:

* hidden environment capture
* harder lowering predictability
* more opaque call graph
* more machinery for little real chain value

### 8）到处可用的闭包 / capture 语义

危险点：

* 隐藏环境捕获
* 降低 lowering 可预测性
* 调用图更模糊
* 引入很多 machinery，却没有太多链上实际价值

### 9) Async / await

Almost pure poison unless your model has a very special need.
除非你的执行模型有极特殊需求，否则几乎纯毒。

Consensus execution is not an app runtime. It does not need “pleasant concurrency syntax.”
共识执行不是应用运行时，不需要“优雅并发语法”。

Async brings:

* state machines
* hidden suspension semantics
* more compiler complexity
* user confusion about execution order

### 9）async / await

它会带来：

* 状态机转换
* 隐藏挂起语义
* 编译器复杂度上升
* 用户对执行顺序的误解

For DAG chains, the concurrency story should be at the transaction/effect scheduling level, not inside the contract language runtime fantasy.
对 DAG 链来说，并发故事应该发生在交易 / effect 调度层，而不是合约语言内部的 runtime 幻觉里。

---

# 第四类伪需求：会让成本模型失真的内存与容器系统

This one kills predictability fast.
这类会很快杀死成本可预测性。

### 10) Hidden heap allocation

Bad unless extremely constrained and obvious.
除非极其受限且非常显式，否则都不好。

Why dangerous:

* unpredictable cost
* bigger runtime assumptions
* harder determinism reasoning
* pressure to add allocator semantics everywhere

### 10）隐藏式 heap allocation

危险点：

* 成本不可预测
* runtime 假设变多
* 确定性推理更难
* 会逼着你到处补 allocator 语义

A good CellScript should prefer:

* fixed layouts
* explicit buffers
* bounded vectors if absolutely needed
* compile-time known shapes where possible

一个好的 CellScript 更应该偏好：

* 固定布局
* 显式 buffer
* 实在需要时才给 bounded vector
* 尽量让 shape 在编译期已知

### 11) Rich dynamic collections as a standard comfort layer

Maps, sets, nested dynamic arrays, arbitrary iterators — these look productive, but often import a huge semantic cost.
Map、set、嵌套动态数组、任意 iterator —— 这些看起来高效，其实常常偷偷引入巨大的语义成本。

Especially dangerous if they:

* allocate
* reorder implicitly
* rely on hidden equality/hash semantics
* have variable iteration cost

### 11）作为“舒适层”的丰富动态容器

尤其当它们会：

* 分配内存
* 隐式重排
* 依赖隐藏 equality/hash 语义
* 迭代成本不稳定

then they are terrible defaults for chain code.
那它们就非常不适合作为链上代码默认件。

---

# 第五类伪需求：让 ABI / lowering 不再透明的“智能编译器帮忙”

This is the most deceptively reasonable category.
这是最“看起来很讲道理”的危险类别。

People say:
“Can’t the compiler just optimize / infer / rewrite that for us?”

Sometimes yes. But if the user-visible semantics become fuzzy, you pay forever.
人们会说：
“编译器不能顺手帮我们优化 / 推断 / 改写吗？”

有时候可以。
但只要用户可见语义开始发糊，你就会付一辈子的代价。

### 12) Large-scale magical desugaring

A little sugar is great. Huge semantic desugaring layers are not.
少量语法糖很好；大规模语义糖化非常危险。

If surface syntax and lowered semantics are too far apart, audits become archaeology.
如果表层语法和最终 lowered semantics 距离太远，审计就会变成考古。

### 12）大规模魔法 desugaring

危险点就是：
表层看起来像 A，底层实际跑的是 B。

That is exactly what CellScript should avoid.
这正是 CellScript 最不该变成的样子。

### 13) Clever auto-optimization that changes effect shape

Compiler optimizations are fine when semantics stay obviously identical.
But if optimization changes apparent read/write/effect boundaries in ways users cannot intuit, that is bad.

### 13）会改变 effect 形状的聪明自动优化

编译器优化当然可以做。
但如果优化会在用户难以直觉理解的情况下改变读写边界 / effect 边界，那就很糟。

For this language, effect transparency is more valuable than squeezing the last 3% prettiness out of generated code.
对这门语言来说，effect 透明度比榨出最后 3% 的生成代码漂亮度更重要。

---

# 第六类伪需求：把“库生态繁荣”误当成语言目标

This is a common founder trap.
这也是常见的 founder trap。

Someone says:
“We need a rich stdlib so people can build anything.”

But on a contract DSL, “can build anything” is often the wrong target.
有人会说：
“我们需要一个丰富标准库，这样大家什么都能做。”

可对 contract DSL 来说，“什么都能做”往往就是错目标。

### 14) Huge standard library

Bad because:

* expands semantic surface
* creates hidden dependencies
* becomes governance burden
* makes upgrades politically painful

### 14）巨大的标准库

坏处：

* 扩大语义面
* 制造隐藏依赖
* 带来治理负担
* 让升级变得政治痛苦

A contract language often benefits more from a **tiny frozen prelude** than from a “real language ecosystem.”
合约语言往往更适合 **小而冻结的 prelude**，而不是“像样的大语言生态”。

### 15) Framework-ification

The moment CellScript starts shipping “application frameworks” that hide cell plumbing, validation logic, or effect declaration, it starts becoming another smart-contract theatre language.
一旦 CellScript 开始发布那种会隐藏 cell plumbing、验证逻辑、effect 声明的“应用框架”，它就开始滑向另一种智能合约戏台语言。

Frameworks are where explicit semantics go to die.
Framework 往往就是显式语义的墓地。

---

# 第七类伪需求：为了“AI 友好”而做的错事

This one matters because your use case clearly involves AI coding.
这类尤其重要，因为你的使用场景明显会有大量 AI 编码参与。

People may think:
“AI writes code now, so difficulty doesn’t matter. Let’s just make the language expressive.”

That is exactly backwards.
人们会想：
“反正现在 AI 写代码了，难不难无所谓，不如把语言做得更有表达力。”

这恰恰是反的。

AI benefits most from:

* explicit grammar
* stable semantics
* low ambiguity
* obvious effect model
* bounded abstractions

AI 最吃香的，其实是：

* 语法显式
* 语义稳定
* 歧义低
* effect 模型明显
* 抽象边界清楚

### 16) Features that increase syntactic flexibility but reduce semantic regularity

Examples:

* too many equivalent spellings
* too much optional magic
* too many ways to express same effect
* DSL-style over-customization

### 16）提高语法自由度、却降低语义规律性的功能

比如：

* 太多等价写法
* 太多 optional magic
* 同一 effect 有太多表达方式
* 过度 DSL 化定制

Humans tolerate this; AI often amplifies it into inconsistency.
人类能勉强容忍；AI 往往会把这种松散放大成不一致。

So “AI-friendly” does **not** mean “more magical.” It usually means “more regular.”
所以“AI 友好”**不等于**“更魔法”。通常恰恰意味着“更规整”。

---

# 真正值得做的，不是这些伪需求，而是这些硬价值

If you want the opposite list — the stuff worth spending LOC on — I’d rank these very high:

* crisp effect declarations
* excellent diagnostics
* readable IR / lowered form dump
* deterministic, obvious ABI rules
* strong static checks around cell access
* bounded data structures
* source-to-lowered traceability
* audit tooling hooks
* compile-time explain mode

如果你要看“什么才值得花代码量”，我会把这些排得很高：

* 清晰的 effect 声明
* 极好的 diagnostics
* 可读的 IR / lowered form dump
* 确定且显然的 ABI 规则
* 针对 cell access 的强静态检查
* 有界数据结构
* 源码到 lowered 结果的可追踪性
* 审计工具钩子
* compile-time explain mode

That’s where the real moat is.
那才是真正的护城河。

---

# 我会给 CellScript 立的几条红线

Here’s my blunt red-line set:
这是我会立下的几条硬红线：

1. **No hidden storage reads/writes.**
   **不允许隐藏 storage 读写。**

2. **No effectful operations disguised as ordinary expressions.**
   **不允许把有 effect 的操作伪装成普通表达式。**

3. **No dynamic dispatch in consensus-critical paths unless brutally explicit.**
   **共识关键路径里，不允许不显式的动态派发。**

4. **No exceptions / unwinding semantics.**
   **不要异常 / 栈展开语义。**

5. **No implicit allocation model that users can forget exists.**
   **不要那种用户会忘记其存在的隐式分配模型。**

6. **No giant stdlib that becomes a second protocol.**
   **不要巨型标准库，不要把 stdlib 做成第二协议层。**

7. **No syntax sugar that obscures lowered behaviour.**
   **不要掩盖 lowered 行为的语法糖。**

8. **No “framework first” developer story.**
   **不要走“框架优先”的开发叙事。**

If a proposed feature violates one of those, it needs an extremely strong case.
任何候选功能只要撞到这些红线，就必须拿出极强的正当性。

---

# 一句话总收束

**CellScript dies the moment it starts optimising for feeling like a modern general-purpose language instead of being an explicit contract semantics language.**
**CellScript 一旦开始优先追求“像一门现代通用语言”，而不是“成为一门显式的合约语义语言”，它就开始走向死亡。**

The fake requirements are mostly prestige features.
The real requirements are semantic sharpness features.
伪需求大多是面子功能。
真需求大多是语义锋利度功能。

下一条我可以直接给你做一个更狠的版本：
**把 CellScript 功能分成 “必须有 / 可以有 / 坚决别有” 三栏清单。**
