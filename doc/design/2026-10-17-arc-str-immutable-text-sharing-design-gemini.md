# 基于 `Arc<str>` 的不可变文本零拷贝共享 — Design

> **Date**: 2026-10-17
> **Author**: Gemini 4 Argon
> **Status**: Draft
> **Prior doc**: [Pipeline 框架与编排](./2026-10-13-pipeline-framework-design-opus.md)、[Seed 与 Runtime 角色及故事上下文解耦](./2026-10-17-role-and-context-seed-runtime-decoupling-design-gemini.md)

---

## Context

### 现状

`crates/aise-core` 的核心领域模型（`StoryPack`、`CharacterCard`、`StoryContext`、`RoleEntry`、`FactEntry`、`RumorEntry`、`MemoryEntry`、`NarrativeGraph` 等）广泛使用值类型 `BoundedText` 与各类强类型 ID（`define_id!`）承载不可变文本，但底层全部基于独占堆内存的 `String`：

1. **`BoundedText(String)` 导致每次 `.clone()` 都触发深拷贝与堆内存分配**（[`crates/aise-core/src/core/bounded_text.rs:5`](../../crates/aise-core/src/core/bounded_text.rs)）：
   - 在 `AiseFactory::freeze_story`（[`crates/aise-core/src/engine/factory.rs:150-214`](../../crates/aise-core/src/engine/factory.rs)）将 `StoryPack` 实例化为 `StoryContext` 时，`opening`、所有角色的 `name`、`alias`、`backstory`、`retrieval`、`profile` 以及所有 `seed_facts`、`seed_rumors`、`seed_memories` 的文本全部被深拷贝一份。若同一个 `StoryPack` 创建 $N$ 个并发故事实例，完全相同的静态 Seed 文本会在堆上重复分配 $N + 1$ 份。
2. **Turn 热路径上的快照读取与原子提交反复深拷贝全量文本**（违反 `R-ARCH-03` 热路径分配预算精神）：
   - `StoryStoreMem::load`（[`crates/aise-core/src/persistence/story_store_mem.rs:306-312`](../../crates/aise-core/src/persistence/story_store_mem.rs)）在每轮 Turn 开始时执行 `story.context.clone()`；
   - `StoryStoreMem::commit`（[`crates/aise-core/src/persistence/story_store_mem.rs:361`](../../crates/aise-core/src/persistence/story_store_mem.rs)）在构造 `next_context` 时再次执行 `story.context.clone()`；
   - Pipeline 内部（如 `baseline_knowledge::collect_knowledge`、`retrieval::resolve_facts` / `resolve_rumors`、`baseline_narrative::build_projection`）在阶段间传递条目时也频繁调用 `.clone()`。
   - 由于 `BoundedText` 内部是 `String`，上述每一次 `.clone()` 都会将整个故事的 `facts`、`rumors`、`roles`、`memories` 文本在堆上全量复制一遍。
3. **试图在业务结构体层面“引用 Seed”来省内存会破坏领域模型**：
   - 若为了避免重复分配而在 `StoryContext` / `RoleEntry` 中保留指向 `StoryPack` 或 `CharacterCard` 的间接引用（如 `RoleActor::Card(card_ref)`），会导致每个 Turn 被迫异步查询 `PackStore`、引入 `ResolvedRole` 等中间类型，并且无法与运行时动态修改（`Update`）或 LLM 动态创建的数据同构处理。

### 为什么现在做

配合 [Seed 与 Runtime 角色及故事上下文解耦](./2026-10-17-role-and-context-seed-runtime-decoupling-design-gemini.md)，我们需要在不牺牲上层领域模型清晰度（`SeedRole` 与 `RoleEntry` 逻辑分离、`StoryContext` 直接持有 `opening` 与生效 `profile`）的前提下，从底层消除 `StoryPack` → `StoryContext` 实例化以及每轮 Turn 快照克隆时的重复堆内存分配。

### Constraints & assumptions

- `BoundedText` 与 `define_id!` 生成的 ID 类型在创建后均为不可变值（Immutable Value Object），不存在原地修改（`&mut str`）需求。
- 保持 `BoundedText` 与各 ID 类型的 `Serialize` / `Deserialize` JSON 格式、校验规则与公开读取接口（`as_str(&self) -> &str`、`Deref<Target = str>`）不变。
- 硬重构（`R-REFACTOR-01` / `R-REFACTOR-02`）：直接替换底层存储表示，不引入第二套文本包装类型。

---

## Principles

1. **逻辑模型解耦与物理内存共享正交**：上层结构体按业务语义直接持有值类型（如 `BoundedText`），底层通过引用计数不可变切片（`Arc<str>`）自动实现跨结构体的物理内存共享。
2. **天然写时复制（Copy-on-Write / Structural Sharing）**：从未被修改的 Seed 文本在 `PackStore`（`Arc<StoryPack>`）与所有 `StoryContext` 实例之间共享同一块底层堆内存；仅当某条知识或角色属性在某轮 Turn 被 `Update` 时，被更新的字段才指向新分配的 `Arc<str>`。
3. **更小的栈体积与 $O(1)$ 克隆开销**：`Arc<str>` 胖指针仅占 16 字节（比 `String` 的 24 字节少 8 字节），`.clone()` 仅执行一次原子引用计数递增，0 次堆内存分配。

---

## Options

### Option A：维持 `BoundedText(String)`，在外层结构体用 `Arc<RoleBody>` 或查表引用省内存

- **Idea**：保持 `BoundedText(String)` 不变，在 `RoleEntry`、`FactEntry` 等外层结构上套 `Arc`，或继续通过 `CharacterCardRef` / `PackRef` 每次从 `PackStore` 查表。
- **Pros**：
  - 不改动 `BoundedText` 内部实现。
- **Cons**：
  - 一旦某个结构体中的单个字段被覆盖（例如 `CharacterCard` 只覆盖 `Role` 的 `profile`，保留 `name` 和 `backstory`），外层 `Arc<RoleBody>` 就无法复用，或者被迫拆出大量碎片化的 `Arc<...>` 包装类型。
  - 未包在 `Arc` 里的 `BoundedText`（如 `FactBody.content`、`RetrievalHint`、`ActivationKey`、`opening`）在 `StoryContext::clone()` 时仍然触发全量 `String` 深拷贝。
- **Risk**：外层类型签名被 `Arc` 污染，且无法彻底解决热路径 `StoryContext` 克隆的堆分配问题。

### Option B：将 `BoundedText`（及高频不可变文本包装类型）底层改为 `Arc<str>`

- **Idea**：
  - 将 `BoundedText` 的内部字段从 `String` 改为 `Arc<str>`（`pub struct BoundedText(Arc<str>)`）。
  - 同步评估将 `define_id!`（`RoleId`、`FactId`、`RumorId`、`MemoryId` 等）以及 `ActivationKey` / `ActivationGroupKey` 底层从 `String` 改为 `Arc<str>`。
  - 上层领域模型直接按值持有 `BoundedText` 与 ID，在 `freeze_story`、`StoryStore::load`、`StoryStore::commit` 及各 Pipeline 阶段之间 `.clone()` 时自动共享底层字符串内存。
- **Pros**：
  - **对上层完全透明**：所有结构体定义（`RoleSpec`、`RoleEntry`、`CharacterProfile`、`FactEntry`、`StoryContext`）保持干净的值语义，无需到处包裹 `Arc<...>`。
  - **跨实例与跨 Turn 零拷贝共享**：1000 个 `StoryContext` 实例与 `PackStore` 中的 `Arc<StoryPack>` 共享同一份 Seed 文本堆内存；`StoryStoreMem::load` 和 `commit` 克隆 `StoryContext` 时不再拷贝任何字符串字节。
  - **栈内存同步缩减**：每个 `BoundedText` 字段从 24 字节（`ptr + len + cap`）缩减为 16 字节（`ptr + len`）。
- **Cons**：
  - `BoundedText::into_string(self)` 若在引用计数 $> 1$ 时调用，需通过 `Arc::unwrap_or_clone`（或 `self.0.as_ref().to_owned()`）转为 `String`；但代码中几乎所有消费处均只需 `&str`。
- **Risk**：单次创建 `BoundedText` 时从 `String` 转换为 `Arc<str>`（`Arc::from(string)`）若原 `String` 存在多余 `capacity` 会发生一次收缩拷贝；可通过在校验通过后直接 `Arc::<str>::from(value.as_str())` 一步分配到位来消除。

### Choice

**Adopt Option B.**

**Rationale**：`BoundedText` 和各类 ID 在整个 `aise-core` 中具有 100% 的不可变语义（创建即校验定型，后续只读不改）。将其底层替换为 `Arc<str>`，能够以零心智负担的方式同时实现“Seed 与 Runtime 结构体解耦”和“跨实例/跨 Turn 物理内存零拷贝共享”。

---

## Design

### 1. Target structure

#### 1.1 物理内存共享拓扑

```mermaid
flowchart TD
  subgraph Heap["共享不可变文本堆内存 (Arc&lt;str&gt; Allocations)"]
    T_OPEN["Arc&lt;str&gt;: opening"]
    T_NAME["Arc&lt;str&gt;: role name & backstory"]
    T_PROF_CARD["Arc&lt;str&gt;: card profile texts"]
    T_FACT_SEED["Arc&lt;str&gt;: seed fact content"]
    T_FACT_TURN["Arc&lt;str&gt;: turn-1 updated fact content"]
  end

  subgraph PackStore["PackStore"]
    SP["Arc&lt;StoryPack&gt;<br/>(SeedRole, seed_facts, opening)"]
  end

  subgraph StoryCtx1["StoryContext (Turn 0 快照)"]
    SC0["opening, roles, facts, rumors"]
  end

  subgraph StoryCtx2["StoryContext (Turn 1 Commit 后快照)"]
    SC1["opening, roles, facts (含更新), rumors"]
  end

  SP -.->|指针共享| T_OPEN
  SP -.->|指针共享| T_NAME
  SP -.->|指针共享| T_FACT_SEED

  SC0 -.->|指针共享| T_OPEN
  SC0 -.->|指针共享| T_NAME
  SC0 -.->|指针共享| T_PROF_CARD
  SC0 -.->|指针共享| T_FACT_SEED

  SC1 -.->|指针共享 (未变)| T_OPEN
  SC1 -.->|指针共享 (未变)| T_NAME
  SC1 -.->|指针共享 (未变)| T_PROF_CARD
  SC1 -->|仅被 Update 的条目指向新分配| T_FACT_TURN
```

### 2. Core types & responsibilities

| Type / Macro | Location | Target Representation | Responsibility | Out of scope |
|---|---|---|---|---|
| `BoundedText` | [`crates/aise-core/src/core/bounded_text.rs`](../../crates/aise-core/src/core/bounded_text.rs) | `pub struct BoundedText(Arc<str>);` | 承载所有经过字节上限校验的不可变业务文本（角色名、背景、画像、知识正文、RetrievalHint、叙事焦点與约束等），提供 $O(1)$ 克隆 | 原地可变字符串拼接 |
| `define_id!` | [`crates/aise-core/src/core/ids.rs`](../../crates/aise-core/src/core/ids.rs) | `pub struct $name(Arc<str>);` | 承载 `RoleId`、`FactId`、`RumorId`、`MemoryId`、`StoryId` 等高频作为 `BTreeMap` / `HashMap` Key 克隆的标识符，消除集合操作时的 Key 堆分配 | — |
| `ActivationKey` / `ActivationGroupKey` | [`crates/aise-core/src/core/activation/rule.rs`](../../crates/aise-core/src/core/activation/rule.rs) | `pub struct ActivationKey(Arc<str>);`<br/>`pub struct ActivationGroupKey(Arc<str>);` | 承载知识与记忆条目中的激活匹配关键字与分组名，使 `ActivationRule::clone` 零字符串拷贝 | — |

### 3. Key flows

#### 3.1 `BoundedText` 构造与一步分配

1. 调用 `BoundedText::try_new(value: impl AsRef<str>, field: &'static str, maximum_bytes: usize)`：
   - 先通过 `let text = value.as_ref();` 读取字节长度 `text.len()` 并与 `maximum_bytes` 比较；
   - 若超出上限，直接返回 `Err(CoreError::BoundedTextLimitExceeded { ... })`（甚至无需先分配 `String`）；
   - 若校验通过，调用 `Arc::<str>::from(text)` 一次性精确分配紧凑的引用计数字符串块。
2. `Default` 实现：返回 `Self(Arc::from(""))`（空切片静态共享）。
3. `Clone` 实现：派生 `#[derive(Clone)]`，仅对内部 `Arc<str>` 做原子引用计数递增。

#### 3.2 开局 `freeze_story` 零拷贝实例化

1. `AiseFactory::freeze_story` 接收反序列化好的 `StoryPack` 与 `CharacterCard`。
2. 在构造初始 `StoryContext` 时：
   - `opening: story_pack.opening.clone()` 仅复制 16 字节 `Arc<str>` 指针；
   - `RoleEntry` 的 `spec`（`name`、`alias`、`backstory`、`retrieval`）从 `SeedRole.spec` 克隆，所有文本指针直接指向 `StoryPack` 中的同一份 `Arc<str>`；
   - 若绑定了 `CharacterCard`，`RoleEntry.profile = card.profile.clone()` 将卡片画像的 `Arc<str>` 指针赋给 `RoleEntry`，随后 `CharacterCard` 外壳即可丢弃，底层画像文本由 `RoleEntry` 接管持有；
   - `FactEntry`、`RumorEntry`、`MemoryEntry` 从 Seed 转换时，其 `content`、`retrieval`、`activation` 中的所有文本与 ID 均为 `Arc<str>` 指针拷贝。

#### 3.3 Turn 运行期快照克隆与增量更新

1. **Turn 开始（`StoryStore::load`）**：克隆 `StoryContext` 快照传入 `Runtime::run_turn`，仅克隆外层 `BTreeMap` 容器节点与 `Arc<str>` 指针，0 字节文本重分配。
2. **Stage 间传递（`baseline` → `plan` → `retrieval` → `think` / `generate`）**：激活或召回的 `FactEntry`、`RumorEntry`、`RoleEntry` 在克隆或投影时，文本与 ID 均保持指针级共享。
3. **Turn 提交（`StoryStore::commit`）**：
   - `let mut next_context = story.context.clone();` 零文本拷贝构造下一状态基线；
   - 应用 `KnowledgeChange` 时，仅本轮新增（`Add`）或修改（`Update`）的 `Fact` / `Rumor` / `Memory` / `Role` 字段持有新创建的 `Arc<str>`，其余数以百计未变更的历史条目继续与旧快照及 `StoryPack` 共享底层内存。

### 4. Key decisions

- **将 `BoundedText` 底层从 `String` 改为 `Arc<str>`，而非在外层业务结构体套 `Arc`** → `BoundedText` 是 `core` 层所有业务文本的统一底座 → 改动点高度收敛在 `bounded_text.rs` 一处，上层所有结构体（`RoleEntry`、`FactEntry`、`RumorEntry`、`MemoryEntry`、`StoryContext`）无需修改字段类型即可自动获得字段级细粒度的结构共享（Structural Sharing）。
- **`BoundedText::try_new` 参数签名接纳 `impl AsRef<str>` 并直接 `Arc::from(&str)`** → 避免调用方先把 `&str` 转成带多余容量的 `String` 再由 `Arc::from(String)` 触发二次 realloc 收缩 → 确保单次构造最多只有一次精确长度的堆分配。
- **同步将 `define_id!` 与 `ActivationKey` / `ActivationGroupKey` 底层改为 `Arc<str>`** → `RoleId`、`FactId`、`RumorId`、`MemoryId` 在 `BTreeMap` 索引、候选列表、激活器 `ActivationOutcome` 中被高频 `.clone()` → 将 ID 也纳入 `Arc<str>` 后，克隆整棵 `StoryContext` 的 `facts` / `rumors` / `roles` 时连 Map Key 和激活规则关键字也实现零堆分配。

---

## Impact

- **Code**：
  - `crates/aise-core/src/core/bounded_text.rs`：内部表示改为 `Arc<str>`；更新 `try_new`、`Default`、`Deserialize`；移除或调整未使用的 `into_string`（若保留则通过 `self.0.as_ref().to_owned()` 实现）。
  - `crates/aise-core/src/core/ids.rs`：`define_id!` 宏内部表示改为 `Arc<str>`。
  - `crates/aise-core/src/core/activation/rule.rs`：`ActivationKey` 与 `ActivationGroupKey` 内部表示改为 `Arc<str>`。
- **Config**：无变更。
- **Data**：JSON 序列化与反序列化格式 100% 保持不变。
- **External interface**：`BoundedText::as_str()`、`Deref<Target = str>`、`Display`、`AsRef<str>` 等读取接口完全兼容；若外部调用了 `BoundedText::into_string()`，其行为保持返回 `String`。

---

## Risks & mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| `From<String>` 转 `Arc<str>` 时若使用 `Arc::from(owned_string)`，当 `String.capacity() > String.len()` 时标准库会重新分配内存以收缩多余容量 | med | low | 在 `try_new` 与 `Deserialize` 中统一使用 `&str` 切片构造 `Arc::<str>::from(s)`，或在反序列化时直接基于 `&str`（`Deserialize` visitor / `&str` 借用）构造，避免中间 `String` 分配 |
| 部分调用点依赖 `String::from(id)` 移动所有权避免拷贝 | low | low | ID 转 `String` 仅发生在错误消息构造等冷路径，热路径全部使用 `&RoleId` / `RoleId::clone()`（变为 $O(1)$）或 `id.as_str()` |

---

## Roadmap

- **Phase 0（`BoundedText`、ID 与激活 Key 的 `Arc<str>` 化）**：修改 `bounded_text.rs`、`ids.rs` 与 `activation/rule.rs`，跑通全量单元与集成测试 → spec `TBD`
