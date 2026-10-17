# 基于 `Arc<str>` 的不可变文本结构共享 — Design

> **Date**: 2026-10-17
> **Author**: Gemini 4 Argon
> **Status**: Draft
> **Prior doc**: [Pipeline 框架与编排](./2026-10-13-pipeline-framework-design-opus.md)、[Seed 与 Runtime 角色及故事上下文解耦](./2026-10-17-role-and-context-seed-runtime-decoupling-design-gemini.md)

---

## Context

### 现状

`crates/aise-core` 的核心领域模型（`StoryPack`、`CharacterCard`、`StoryContext`、`RoleEntry`、`FactEntry`、`RumorEntry`、`MemoryEntry`、`NarrativeGraph` 等）广泛使用不可变值类型 `BoundedText` 承载业务文本，但当前存储与建档流存在以下内存分配与深拷贝问题：

1. **`BoundedText(String)` 导致每次 `.clone()` 触发深拷贝与堆内存分配**（[`crates/aise-core/src/core/bounded_text.rs:5`](../../crates/aise-core/src/core/bounded_text.rs)）：
   - 在 `AiseFactory::freeze_story`（[`crates/aise-core/src/engine/factory.rs:78-214`](../../crates/aise-core/src/engine/factory.rs)）将 `StoryPack` 实例化为 `StoryContext` 时，`opening`、所有角色的 `name`、`alias`、`backstory`、`retrieval`、`profile` 以及所有 `seed_facts`、`seed_rumors`、`seed_memories` 的文本全部被深拷贝一份。
2. **建档顺序导致跨故事实例无法共享 Canonical `StoryPack` 内存**：
   - 在 `AiseFactory::create_story`（[`crates/aise-core/src/engine/factory.rs:44-46`](../../crates/aise-core/src/engine/factory.rs)）中，当前先对新反序列化的 `spec.story_pack` 执行 `freeze_story` 克隆生成 `StoryContext`，随后才调用 `store.pack_store().retain(&story_id, story.assets)`。
   - 而在 `PackStoreMem::retain`（[`crates/aise-core/src/persistence/pack_store_mem.rs:109-117`](../../crates/aise-core/src/persistence/pack_store_mem.rs)）中，当 `Sha256Digest` 已存在时，`or_insert_with` 直接丢弃后传入的 `StoryPack`。这导致即使 `BoundedText` 改为 `Arc<str>`，第 $2..N$ 个故事实例的 `StoryContext` 持有的仍是各自独立反序列化产生的 `Arc<str>`，并未与 `PackStore` 中的 canonical `Arc<StoryPack>` 共享底层内存。
3. **Turn 热路径在持锁期间与阶段传递中反复深拷贝文本**（违反 `R-ARCH-03` 热路径分配预算）：
   - `StoryStoreMem::load`（[`crates/aise-core/src/persistence/story_store_mem.rs:286-292`](../../crates/aise-core/src/persistence/story_store_mem.rs)）在读锁内执行 `story.context.clone()`；
   - `StoryStoreMem::commit`（[`crates/aise-core/src/persistence/story_store_mem.rs:341`](../../crates/aise-core/src/persistence/story_store_mem.rs)）在写锁内执行 `story.context.clone()`；
   - Pipeline 内部（如 `baseline_knowledge::collect_knowledge`、`retrieval::resolve_facts` / `resolve_rumors`、`baseline_narrative::build_projection`）在阶段间传递条目时也频繁调用 `.clone()`。
   - 此外，`StoryContext` 中的长文本字段 `StorySummary.text: String`（[`crates/aise-core/src/core/story/story_context.rs:10`](../../crates/aise-core/src/core/story/story_context.rs)）、`TurnSegment(String)` 与 `PlayerContribution { raw: String, processed: String }`（[`crates/aise-core/src/core/turn.rs:8-102`](../../crates/aise-core/src/core/turn.rs)）在每次克隆 `StoryContext` 时同样触发数 KB 级的堆深拷贝。
4. **`#[serde(try_from = "String", into = "String")]` 造成反序列化双重分配与序列化临时拷贝**：
   - `BoundedText::deserialize`（[`crates/aise-core/src/core/bounded_text.rs:38`](../../crates/aise-core/src/core/bounded_text.rs)）、`ActivationKey`（[`crates/aise-core/src/core/activation/rule.rs:104`](../../crates/aise-core/src/core/activation/rule.rs)）、`RetrievalHint`（[`crates/aise-core/src/core/knowledge/hint.rs:6`](../../crates/aise-core/src/core/knowledge/hint.rs)）等均先反序列化出临时 `String`，并在 `Serialize` 时通过 `into = "String"` 克隆并分配新的 `String`（影响 `StoryPack::validate_roles` 与 `Sha256Digest::canonical`）。

### 为什么现在做

配合 [Seed 与 Runtime 角色及故事上下文解耦](./2026-10-17-role-and-context-seed-runtime-decoupling-design-gemini.md)，我们需要在不牺牲上层领域模型值语义的前提下，通过 `Arc<str>` 不可变结构共享与 `PackStore` canonical 驻留（Interning），实质性消除跨实例与跨 Turn 快照克隆中的大块文本堆分配。

### Constraints & assumptions

- `BoundedText`、`StorySummary.text`、`TurnSegment`、`PlayerContribution` 在创建后均为不可变值（Immutable Value Object），不存在原地修改（`&mut str`）需求。
- Rust 标准库的 `Arc<str>` 堆布局包含 16 字节引用计数头（`ArcInner { strong, weak, data }`），因此：
  - 从已有 `String` 转为 `Arc<str>` 必定触发一次新堆分配与字节拷贝（无法复用 `String` 缓冲区）；
  - `Arc<str>` 转为 `String`（`into_string`）必定触发一次新堆分配与字节拷贝；
  - `Arc::<str>::from("")` 每次调用仍会在堆上分配 16 字节引用计数头，必须通过静态 `LazyLock<Arc<str>>` 才能实现零分配共享空字符串。
- 保持所有类型的 JSON 序列化与反序列化格式、校验错误语义、`Sha256Digest::canonical` 输出以及公开读取接口（`as_str(&self) -> &str`、`Deref<Target = str>`）100% 兼容。
- 硬重构（`R-REFACTOR-01` / `R-REFACTOR-02`）：直接替换目标类型的内部表示，不引入第二套文本包装类型。

---

## Principles

1. **逻辑模型解耦与物理内存共享正交**：上层结构体按业务语义直接持有值类型（如 `BoundedText`），底层通过引用计数不可变切片（`Arc<str>`）自动实现跨结构体的不可变结构共享（Structural Sharing）。
2. **先 Canonical 驻留、再实例化（Intern-then-Freeze）**：多实例共享同一剧本内存的前提是先通过 `PackStore::retain` 获取根据 `Sha256Digest` 去重后的 canonical `Arc<StoryPack>`，再从该 canonical 实例克隆构造 `StoryContext`。
3. **用单次构造/反序列化分配换取热路径 $O(1)$ 克隆**：反序列化时通过 `serde::de::Visitor::visit_str` 直接从借用的 `&str` 一步构造 `Arc<str>`，序列化时直接调用 `serializer.serialize_str(self.as_str())`，消除中间 `String` 分配。
4. **基于实测收益控制 `Arc<str>` 边界**：优先改造承载长文本与高频快照克隆的 `BoundedText` 及 Turn 历史长文本；对短小且具备 16 字节 `ArcInner` 头部开销的强类型 ID 与 `ActivationGroupKey`，通过独立基准测试（Phase 1）决定是否纳入。

---

## Options

### 问题 1：不可变文本的内存共享粒度与范围

#### Option 1A：维持 `BoundedText(String)`，在外层业务结构体套 `Arc<...>`

- **Idea**：保持 `BoundedText(String)` 不变，在 `RoleEntry`、`FactEntry` 等外层结构上套 `Arc`。
- **Pros**：
  - 不改动 `BoundedText` 内部实现。
- **Cons**：
  - 当 `SeedRole` 实例化为 `RoleEntry`（或角色卡覆盖 `profile`）时，外层结构体类型不同，无法直接复用外层 `Arc`。
  - 未包在 `Arc` 里的 `BoundedText`（如 `opening`、`RetrievalHint`、`ActivationKey`）在 `StoryContext::clone()` 时仍触发深拷贝。
- **Risk**：外层类型签名被 `Arc` 污染，且无法解决 `SeedRole` → `RoleEntry` 转换时的文本深拷贝。

#### Option 1B：将 `BoundedText`、所有 `define_id!` 及 `ActivationGroupKey` 全部一次性改为 `Arc<str>`

- **Idea**：把 `BoundedText`、13 个 `define_id!` 强类型 ID 以及 `ActivationGroupKey` 的内部字段全部从 `String` 改为 `Arc<str>`。
- **Pros**：
  - 所有文本与 ID 克隆均变为原子引用计数操作。
- **Cons**：
  - 每个唯一 `Arc<str>` 在堆上额外增加 16 字节 `ArcInner`（`strong` + `weak`）头部开销。对于短 ID（通常 8~24 字节）及仅单次使用的 ID（如 `IdempotencyKey`、`PlayerId`、`SemanticVersion`），总堆内存反而可能上升，且 `BTreeMap` 查找/克隆时引入不必要的原子指令开销。
- **Risk**：缺乏基准数据支撑全量短 ID `Arc` 化的净收益。

#### Option 1C：Phase 0 聚焦 `BoundedText` 与 `StoryContext` 历史长文本 `Arc<str>` 化；ID 留待 Phase 1 基准评估

- **Idea**：
  - **Phase 0**：将 `BoundedText`（含内嵌它的 `RetrievalHint`、`ActivationKey`）改为 `Arc<str>`；同步将 `StoryContext` 快照中的长文本字段（`StorySummary.text`、`TurnSegment`、`PlayerContribution`）改为 `Arc<str>`（或复用不可变文本表示）。
  - **Phase 1**：针对高频作为 `BTreeMap` Key 与 Entry 字段双重持有的领域 ID（`RoleId`、`FactId`、`RumorId`、`MemoryId`）及 `ActivationGroupKey` 编写基准测试，实测内存与克隆耗时收益后再决定是否切换。
- **Pros**：
  - 精准消除 `StoryPack` 与 `StoryContext` 中占比 $>95\%$ 的长文本深拷贝（角色设定、知识正文、记忆、开场白、滚动摘要、最近回合正文）。
  - 避免对短小或单次生命周期 ID 盲目引入 16 字节 `ArcInner` 堆头与原子计数开销。
- **Cons**：
  - `StoryContext::clone()` 时 `BTreeMap` 的短 ID Key 仍保持 `String` 克隆（待 Phase 1 实测决策）。
- **Risk**：无。

### 问题 2：跨故事实例的 `StoryPack` 内存共享机制

#### Option 2A：维持现有“先 `freeze_story`、后 `PackStore::retain`”顺序

- **Idea**：保持 `AiseFactory::create_story` 现有顺序不变。
- **Pros**：
  - 不修改 `PackStore::retain` 签名。
- **Cons**：
  - 每次 `create_story` 反序列化出的新 `StoryPack` 在 `freeze_story` 时将各自的 `Arc<str>` 克隆给 `StoryContext`，随后新 `StoryPack` 在 `PackStoreMem::retain` 中因 digest 已存在而被丢弃，导致多实例之间**完全没有共享**同一份 `StoryPack` 的 `Arc<str>`。
- **Risk**：跨故事实例内存共享名存实亡。

#### Option 2B：`PackStore::retain` 返回 Canonical `Arc<StoryPack>`，基于 Canonical 实例执行 `freeze_story`

- **Idea**：
  - 将 `PackStore::retain` 签名调整为返回 `Result<Arc<StoryPack>, PersistenceError>`（若 digest 已存在则增加 `holders` 引用并返回已驻留的 canonical `Arc<StoryPack>`，丢弃新传入的重复副本）。
  - 在 `AiseFactory::create_story` 中，先校验并调用 `pack_store.retain` 拿到 canonical `Arc<StoryPack>`，再基于 `&Arc<StoryPack>` 构造初始 `StoryContext`。
- **Pros**：
  - 即使调用方每次从磁盘/网络独立反序列化 `StoryPack` JSON，只要 `Sha256Digest` 相同，所有 `StoryContext` 实例的 Seed 文本都会真正指向 `PackStore` 中唯一存活的那份 canonical `StoryPack` 的 `Arc<str>` 堆内存，重复反序列化的临时 `Arc<str>` 立即释放。
- **Cons**：
  - 若后续 `story_store.create` 失败，需调用 `release_story_assets` 回滚 `PackStore` 引用（现有代码已具备该回滚分支）。
- **Risk**：无。

### Choice

**问题 1 采用 Option 1C；问题 2 采用 Option 2B。**

**Rationale**：Option 1C 兼顾了 `StoryContext` 快照（含 Seed 知识、角色设定、滚动摘要、最近回合正文）的长文本零拷贝共享，同时避免对短 ID 盲目增加 `ArcInner` 头部开销；Option 2B 修复了 `PackStore` 去重与 `freeze_story` 顺序颠倒导致的跨实例不共享漏洞。

---

## Design

### 1. Target structure

#### 1.1 Canonical 驻留与物理内存共享拓扑

```mermaid
flowchart TD
  subgraph Deser["第 N 次 create_story 输入"]
    IN_PACK["新反序列化的 StoryPack<br/>(临时 Arc&lt;str&gt; 集合)"]
  end

  subgraph PackStore["PackStore (按 Sha256Digest 去重驻留)"]
    CANON_PACK["Canonical Arc&lt;StoryPack&gt;<br/>(唯一常驻的 Seed Arc&lt;str&gt; 集合)"]
  end

  subgraph Heap["共享不可变文本堆内存 (ArcInner + str)"]
    T_OPEN["Arc&lt;str&gt;: opening"]
    T_ROLE["Arc&lt;str&gt;: role spec & profile"]
    T_FACT_SEED["Arc&lt;str&gt;: seed fact content"]
    T_FACT_UPD["Arc&lt;str&gt;: turn-1 updated fact"]
    T_TURN_SEG["Arc&lt;str&gt;: recent turn segment & summary"]
  end

  subgraph StoryCtx0["StoryContext (Turn 0 快照)"]
    SC0["opening, roles, facts, rumors"]
  end

  subgraph StoryCtx1["StoryContext (Turn 1 Commit 后快照)"]
    SC1["opening, roles, facts, recent_turns, summary"]
  end

  IN_PACK -->|1. retain(digest): 若已存在则丢弃临时 Pack| CANON_PACK
  CANON_PACK -->|2. freeze_story(&canonical_pack)| SC0

  CANON_PACK -.->|共享指针| T_OPEN
  CANON_PACK -.->|共享指针| T_ROLE
  CANON_PACK -.->|共享指针| T_FACT_SEED

  SC0 -.->|共享指针| T_OPEN
  SC0 -.->|共享指针| T_ROLE
  SC0 -.->|共享指针| T_FACT_SEED

  SC1 -.->|未变更字段继续共享| T_OPEN
  SC1 -.->|未变更字段继续共享| T_ROLE
  SC1 -->|被 Update 的条目指向新分配| T_FACT_UPD
  SC1 -.->|跨 Turn 快照共享历史正文| T_TURN_SEG
```

### 2. Core types & responsibilities

#### 2.1 Phase 0 目标类型（必改）

| Type | Location | Target Representation | Responsibility | Out of scope |
|---|---|---|---|---|
| `BoundedText` | [`crates/aise-core/src/core/bounded_text.rs`](../../crates/aise-core/src/core/bounded_text.rs) | `pub struct BoundedText(Arc<str>);` | 承载所有经过字节上限校验的不可变领域文本，提供静态空值 `Default`、单次分配反序列化与 $O(1)$ 克隆 | 原地可变字符串修改 |
| `ActivationKey` | [`crates/aise-core/src/core/activation/rule.rs`](../../crates/aise-core/src/core/activation/rule.rs) | `pub struct ActivationKey(BoundedText);`（保持包装 `BoundedText`） | 复用 `BoundedText(Arc<str>)` 实现激活关键字共享；将 `serde` 改为直接读写 `&str` 消除临时 `String` | — |
| `RetrievalHint` | [`crates/aise-core/src/core/knowledge/hint.rs`](../../crates/aise-core/src/core/knowledge/hint.rs) | `pub struct RetrievalHint(BoundedText);`（保持包装 `BoundedText`） | 复用 `BoundedText(Arc<str>)` 实现召回提示共享；将 `serde` 改为直接读写 `&str` 消除临时 `String` | — |
| `StorySummary` | [`crates/aise-core/src/core/story/story_context.rs`](../../crates/aise-core/src/core/story/story_context.rs) | `pub text: Arc<str>`（或只读包装） | 使 `StoryContext` 克隆时免于深拷贝滚动摘要长文本 | — |
| `TurnSegment` / `PlayerContribution` | [`crates/aise-core/src/core/turn.rs`](../../crates/aise-core/src/core/turn.rs) | `pub struct TurnSegment(Arc<str>);`<br/>`pub raw: Arc<str>, pub processed: Arc<str>` | 使 `StoryContext.recent_turns` 在 `load` / `commit` 克隆快照时免于深拷贝每轮故事正文与玩家输入 | — |
| `PackStore::retain` | [`crates/aise-core/src/persistence/pack_store.rs`](../../crates/aise-core/src/persistence/pack_store.rs) | 返回 `Result<Arc<StoryPack>, PersistenceError>` | 按 `Sha256Digest` 驻留并返回 canonical `Arc<StoryPack>`，支撑跨实例共享底层 `Arc<str>` | — |

#### 2.2 Phase 1 评估类型（基准驱动）

| Type | Location | Candidate Representation | Evaluation Criteria |
|---|---|---|---|
| `RoleId` / `FactId` / `RumorId` / `MemoryId`（及 `define_id!`） | [`crates/aise-core/src/core/ids.rs`](../../crates/aise-core/src/core/ids.rs) | `pub struct $name(Arc<str>);` | 对比 `BTreeMap` 克隆/查找耗时及 16 字节 `ArcInner` 头部带来的净内存变化 |
| `ActivationGroupKey` | [`crates/aise-core/src/core/activation/rule.rs`](../../crates/aise-core/src/core/activation/rule.rs) | `pub struct ActivationGroupKey(Arc<str>);` | 对比 `ActivationRule::clone` 耗时与短字符串头部开销 |

### 3. Key flows

#### 3.1 `BoundedText` 的构造、默认空值与 Serde 零中间分配

1. **静态空值共享（`Default`）**：
   - 使用模块级 `static EMPTY_ARC_STR: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from(""));`；
   - `BoundedText::default()` 返回 `Self(Arc::clone(&EMPTY_ARC_STR))`，确保所有默认空文本共享同一个堆头，0 次新堆分配。
2. **校验与构造（`BoundedText::try_new`）**：
   - 签名接纳 `value: impl AsRef<str>`；
   - 先对借用切片 `let text = value.as_ref();` 校验 `text.len() <= maximum_bytes`；
   - 若超限直接返回 `Err(CoreError::BoundedTextLimitExceeded { ... })`（若调用方传入 `&str`，失败路径 0 次堆分配）；
   - 若通过：当 `text.is_empty()` 时返回 `Self::default()`，否则调用 `Arc::<str>::from(text)` 执行一次精确长度的堆分配（`16B ArcInner + text.len()`）。
   - 注：若调用方已持有 `String` 并传入 `&str` / `String`，由于 `Arc<str>` 必须前置 16 字节引用计数头，此处固定发生一次从 `String` 到 `Arc<str>` 的分配与拷贝，以此换取后续整个生命周期内的 $O(1)$ 克隆。
3. **单次分配反序列化（`Deserialize`）**：
   - 废弃原有的 `let value = String::deserialize(deserializer)?;`（避免先分配 `String` 再分配 `Arc<str>` 的双重分配）；
   - `BoundedText`、`ActivationKey`、`RetrievalHint` 统一通过自定义 `serde::de::Visitor`（实现 `visit_str`）或借用反序列化直接拿到 `&str`，校验后一步构造 `Arc<str>`。
4. **无拷贝序列化（`Serialize`）**：
   - 移除 `ActivationKey`、`RetrievalHint`（及后续 ID）上的 `#[serde(into = "String")]`；
   - 统一使用 `serializer.serialize_str(self.as_str())` 直接写出切片，消除序列化时的 `String` 分配。
5. **显式转换（`into_string`）**：
   - 鉴于 `Arc<str>` 无法零拷贝退化为 `String`，且生产代码中除原先的 `#[serde(into = "String")]` 外无任何热路径消费 `BoundedText::into_string()`，移除 `into_string(self)`（或仅保留 `as_str(&self) -> &str`），防止调用方误以为存在零拷贝移动。

#### 3.2 先驻留后实例化的开局流（`AiseFactory::create_story`）

1. 校验传入的 `spec.story_pack` 与 `spec.cast`，计算 `pack_ref = spec.story_pack.pack_ref()?`。
2. 先调用 `let canonical_pack = store.pack_store().retain(&story_id, pack_ref, spec.story_pack).await?;`：
   - 若 `PackStore` 中尚无该 `digest`，将 `spec.story_pack` 封装为 `Arc<StoryPack>` 存入并返回；
   - 若 `PackStore` 中已存在该 `digest`，将 `story_id` 加入 `holders`，返回已有的 canonical `Arc<StoryPack>`，并当场释放新传入的重复 `StoryPack`。
3. 基于 `&canonical_pack`（而非已丢弃的临时副本）与 `spec.cast` 构造初始 `StoryContext`：
   - `opening`、`RoleSpec`、`CharacterProfile`、`FactEntry`、`RumorEntry`、`MemoryEntry` 中的所有 `BoundedText` 直接从 `canonical_pack` 克隆 `Arc<str>` 指针。
4. 调用 `store.story_store().create(...)` 写入初始 `StoryContext`（若失败则调用 `release_story_assets` 回滚 `PackStore` 引用）。

#### 3.3 Turn 运行期快照克隆与字段级替换

1. **Turn 读取（`StoryStoreMem::load`）**：在读锁内克隆 `StoryContext` 时，所有 `BoundedText`、`StorySummary.text`、`TurnSegment`、`PlayerContribution` 均仅递增 `Arc<str>` 引用计数，大幅缩短读锁持有时间。
2. **Turn 提交（`StoryStoreMem::commit`）**：
   - 在写锁内执行 `let mut next_context = story.context.clone();`；
   - 应用 `KnowledgeChange` 时，采用不可变结构共享 + 整字段替换：仅本轮 `Add` 或 `Update` 的条目指向新分配的 `Arc<str>`，未变更条目与历史 Turn 正文保持指针共享。

### 4. Key decisions

- **将 `BoundedText` 与 `StoryContext` 历史长文本改为 `Arc<str>`，暂缓全量短 ID `Arc` 化** → 长文本（设定、知识、记忆、摘要、回合正文）单条体积大且在每次 `load`/`commit` 中全量克隆，而短 ID 受到 16 字节 `ArcInner` 头部开销制约 → Phase 0 优先获取确定的大块内存与延迟收益，Phase 1 通过基准测试决定 ID 策略。
- **调整 `PackStore::retain` 返回 canonical `Arc<StoryPack>` 后再执行 `freeze_story`** → 修复重复反序列化同一 `StoryPack` 时因后传入副本被丢弃而无法跨实例共享内存的漏洞 → 确保 $N$ 个并发故事实例真正共享同一份 `StoryPack` 底层文本堆内存。
- **手写基于 `&str` 的 `Serialize` / `Deserialize` 替代 `#[serde(try_from = "String", into = "String")]`** → 消除反序列化时的 `String` → `Arc<str>` 双重堆分配，以及序列化（含 canonical digest 计算）时的临时 `String` 分配。
- **`BoundedText::try_new` 接纳 `impl AsRef<str>` 并使用 `LazyLock<Arc<str>>` 共享空切片** → 调用方传入 `&str` 时无需预分配 `String`（消除几十处 `.to_owned()` 样板代码），且所有空 `BoundedText` 零堆分配。

---

## Impact

- **Code**：
  - `crates/aise-core/src/core/bounded_text.rs`：内部改为 `Arc<str>`；使用 `LazyLock<Arc<str>>` 实现 `Default`；`try_new` 接收 `impl AsRef<str>`；`Deserialize` 使用 `Visitor::visit_str` 直接从 `&str` 构造；移除 `into_string`。
  - `crates/aise-core/src/core/activation/rule.rs`：`ActivationKey` 移除 `#[serde(try_from = "String", into = "String")]`，改为直接借用 `&str` 校验与 `serialize_str`；`try_new` 接收 `impl AsRef<str>`。
  - `crates/aise-core/src/core/knowledge/hint.rs`：`RetrievalHint` 移除 `#[serde(try_from = "String", into = "String")]` 与 `into_string`，改为直接借用 `&str` 校验与 `serialize_str`；`try_new` 接收 `impl AsRef<str>`。
  - `crates/aise-core/src/core/story/story_context.rs` 与 `crates/aise-core/src/core/turn.rs`：`StorySummary.text`、`TurnSegment`、`PlayerContribution` 内部文本改为 `Arc<str>`。
  - `crates/aise-core/src/persistence/pack_store*.rs` 与 `crates/aise-core/src/engine/factory.rs`：`PackStore::retain` 返回 `Arc<StoryPack>`，`AiseFactory::create_story` 基于返回的 canonical `Arc<StoryPack>` 构造 `StoryContext`。
- **Config**：无变更。
- **Data**：JSON 序列化与反序列化格式、`Sha256Digest::canonical` 摘要结果 100% 保持不变。
- **External interface**：
  - `BoundedText::try_new`、`ActivationKey::try_new`、`RetrievalHint::try_new` 参数从 `impl Into<String>` 拓宽为 `impl AsRef<str>`（同时兼容 `&str` 与 `String` / `&String`）；
  - 移除未在外部消费的 `into_string(self)`。

---

## Risks & mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| `serde_json::from_value` 或含转义字符的 JSON 字符串在反序列化时触发 `visit_string` 而非 `visit_str` | med | low | 在自定义 `Visitor` 中同时实现 `visit_str` 与 `visit_string`（两者均复用 `Self::try_new(&str)`），确保无论 JSON 是否包含转义都能正确反序列化 |
| 调用点从 `String` 构造 `BoundedText`（如 LLM DTO 解析后转换）仍需一次 `String` → `Arc<str>` 拷贝 | high | low | 让 LLM 输出 DTO 直接反序列化为 `BoundedText` / `RetrievalHint` / `ActivationKey`，使 `serde_json` 在解析阶段直接通过 `visit_str` 一步生成 `Arc<str>` |
| 短 ID 若改为 `Arc<str>` 可能因 16B `ArcInner` 头部增加内存 | med | low | 将 ID 与 `ActivationGroupKey` 拆分至 Phase 1，以基准测试数据作为准入条件 |

---

## Verification & Benchmarks

实施阶段需覆盖以下机械化验证与基准对比：

1. **行为与格式等价性测试**：
   - `StoryPack` 与 `CharacterCard` 的 `Sha256Digest::canonical` 输出在重构前后完全一致；
   - `BoundedText`、`ActivationKey`、`RetrievalHint` 的字节上限错误、空值拒绝及含转义字符 JSON 的反序列化测试全部通过。
2. **物理指针共享断言测试**：
   - **同实例共享**：验证 `freeze_story` 产出的 `StoryContext` 中，`opening`、`RoleEntry.spec.name`、`FactEntry.body.content` 与 `PackStore::get_pack` 返回的 `Arc<StoryPack>` 满足 `Arc::ptr_eq`（或 `as_str().as_ptr()` 相等）；
   - **跨实例独立反序列化共享**：两次独立调用 `serde_json::from_str::<StoryPack>(SNAKE_PACK_JSON)` 并分别 `create_story` 后，验证两个 `StoryContext` 的 `opening.as_str().as_ptr()` 完全相等（证明 `PackStore::retain` canonical 去重生效）。
3. **性能与内存基准关注项**：
   - `StoryContext::clone()` 在典型故事规模（如 `snake_pack` + 10 轮 `recent_turns` + `summary`）下的耗时、堆分配次数与分配字节数对比；
   - `StoryPack` JSON 反序列化耗时对比（验证移除双重分配后的效果）。

---

## Roadmap

- **Phase 0（`BoundedText`、Turn 历史长文本 `Arc<str>` 化与 `PackStore` Canonical 共享）**：改造 `BoundedText`、`ActivationKey`、`RetrievalHint`、`StorySummary`、`TurnSegment`、`PlayerContribution`，调整 `PackStore::retain` 返回 canonical `Arc<StoryPack>` 并先驻留后 `freeze_story`，补充指针共享断言测试 → spec `TBD`
- **Phase 1（领域 ID 与 `ActivationGroupKey` 基准评估）**：针对 `RoleId`、`FactId`、`RumorId`、`MemoryId`、`ActivationGroupKey` 开展内存与克隆基准测试，按实测收益决定是否切换为 `Arc<str>` → spec `TBD`
