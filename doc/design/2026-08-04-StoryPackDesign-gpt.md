# AISE 故事包 / 卡片体系设计

> 参考：[SillyTavern Character Card V2/V3](https://github.com/SillyTavern/SillyTavern) 的
> `spec: chara_card_v2` 卡片、World Info 世界书条目（key / constant / selective）
> 与 Context Builder 的上下文组装顺序。ST 的 `position` / `order` / `depth`（按位置
> 注入）与 `first_mes`（角色开场白）在 AISE 中被注入模型与故事开场取代（§3、§4）。
>
> 本文是设计文档，只定义目标格式、语义与集成点，不约束实现细节。与
> `2026-08-04-Architecture-gpt.md` 冲突时以 `2026-08-04-Architecture-gpt.md` 为准。

## 1. 设计目标

SillyTavern 的资产模型是"聊天工具 + 可交换资产"：角色卡（V2/V3）、世界书（World
Info）、上下文预设三件套，靠 `extensions` 字段做跨平台兼容。AISE 是 Turn 驱动的
叙事引擎，角色卡只是引擎输入，不是聊天界面的一部分。因此直接照搬 ST 的资产模型
会缺两块 ST 没有的东西：

- **故事级设定**（Story Instructions、约束、知识边界、开场场景）——ST 靠场景文本
  和用户手动填，AISE 需要结构化输入。**开场白是故事级概念，只存在于故事包
  （`start`），角色卡不承载开场白。** AISE 不是跟角色聊天的工具，而是创造故事的
  工具，开场锚定故事起点，不绑定到某个角色。
- **权威性语义**（`Canonical World Fact` / `Character Belief` / `Character Memory` /
  `Retrieved Lore`，见 `2026-08-04-Architecture-gpt.md` §10.2）——世界书条目必须带来源标签，不能
  全部当世界事实，否则违反 `R-AISE-07`。

设计原则：

- 三层资产：`aise_char_v1`（人物卡）→ `aise_world_v1`（世界书）→ `aise_story_v1`
  （故事包）。前两者可独立发布和复用，故事包是"开箱即玩"的完整单元。
- **世界书是内容资产，故事包是组合资产，二者不是同一层级，不合并为单一类型。
  世界书是故事包的一部分（`world_book ⊂ story_pack`），不是反过来。** 共享同一
  `WorldBook` 数据模型：故事包的 `world_book` 字段就是一份完整、独立可导出的
  世界书（§4），作者写"知识"只有一个心智模型。
- **知识只有一个入口：故事包的 `world_book`。角色卡不关联、不内嵌世界书**
  （`character_book` 仅用于 ST V2 卡片兼容导入），也不承担知识职责。世界书与
  角色卡都通过故事包统一、共享：同一份 `world_book` 供所有角色与整个故事共用，
  避免"每个角色各自挂一本书"导致的知识碎片化与注入重复。
- **知识（进上下文）与规则（驱动逻辑）必须分离，规则不扁平化成世界书条目。**
  `instructions.rules`、`boundaries`、`config`、角色卡 `think_style` /
  `knowledge_boundary` 是 Validator / Planner / Character Think 的确定性输入，
  需要类型化字段供确定性校验（`2026-08-04-Architecture-gpt.md` §13.1），文本条目无法承担。
- 参考 ST 字段语义（`mes_example`、世界书 `constant` / `selective`），但明确标注
  权威性与知识边界，供 Validation 与 Retrieval 使用。`first_mes` /
  `alternate_greetings`（角色开场白）与 `position` / `depth`（按位置注入）等聊天
  工具语义不采用：前者由故事包 `start` 取代，后者由注入模型取代。
- 单一 JSON 文件承载完整故事包，导入即玩；可选 PNG 封面（复用 ST 的 tEXt chunk
  嵌入约定）。
- 种子数据（Seed）与运行时状态（Committed）分离：卡片/世界书只做一次种子导入，
  运行时只能通过 `ValidatedChangeSet` 产生 `FactSource::CommittedTurn` 事实。
- **故事包是唯一必选格式，人物卡、世界书均为可选增强。** 缺失时引擎在运行时
  依赖 LLM 即兴生成角色设定与世界细节，不要求作者手动填充所有字段。
- **玩家必须有角色。** 故事包可提供默认玩家卡（`player`），缺失时玩家在开局时
  从 `characters` 中选择一个角色，或创建一个新角色卡。

## 2. 人物卡 aise_char_v1

与 ST `chara_card_v2` 同构，保留生态里熟悉的字段名；删除聊天工具语义的
`first_mes` / `alternate_greetings`（开场白）与 `extensions.world`（关联世界书），
两者分别由故事包的 `start` 与 `world_book` 统一承担。字段全部可选，导入时按默认
值补齐。

```jsonc
{
  "spec": "aise_char_v1",
  "spec_version": "1.0",
  "data": {
    "name": "Seraphina",                    // 必填，唯一业务标识
    "title": "灰林守护者",                   // 可选，称号
    "description": "…",                     // 角色描述（注入 description）
    "personality": "…",                     // 性格（注入 personality）
    "scenario": "…",                        // 角色视角的场景设定（注入 scenario）
    "mes_example": "…",                     // 示例对话（few-shot，注入 dialogueExamples）
    "system_prompt": "…",                   // 角色级系统提示，覆盖 main
    "post_history_instructions": "…",       // 角色级后置指令，覆盖 jailbreak
    "creator": "…",
    "character_version": "1.0",
    "tags": ["elves", "guardian"],
    "extensions": {
      "talkativeness": 0.5,
      "fav": false,
      "aise": {
        "internal_state": {                 // 种子内部状态（对应 InternalState）
          "goals": ["守护灰林"],
          "health": 100,
          "relationships": { "player": 50 }
        },
        "seed_memory": [                    // 种子记忆（对应 MemoryKind::Observed）
          { "kind": "Observed", "content": "…" }
        ],
        "knowledge_boundary": [             // 角色不该知道的信息（供 Validation 用）
          "玩家是转世者"
        ],
        "think_style": "谨慎多疑",           // Character Think 提示片段
        "visibility": {                     // 角色对世界事实的可见性过滤
          "canonical": true,                // 是否能看到 canonical fact
          "retrieved_lore": true            // 是否能看到检索到的 lore
        }
      }
    },
    "character_book": { … }                 // 仅 ST V2 导入兼容；知识统一并入故事包 world_book，不参与 AISE 运行时
  }
}
```

说明：

- `spec` / `spec_version` 顶层标记与 ST 完全一致，便于导入器识别。
- **不包含开场白与关联世界书**：`first_mes` / `alternate_greetings` 由故事包的
  `start`（故事开场）取代（§4），`extensions.world` / `character_book` 的关联
  语义由故事包的 `world_book` 统一承载（§1）。从 ST 导入时，卡片的开场白与
  内嵌世界书由故事包打包流程吸收，不写入角色卡本体。角色卡只描述"这个角色是谁、
  怎么行动"，不描述"故事从哪里开始"。
- `extensions.aise` 是 AISE 私有扩展命名空间；未知 `extensions` 字段透传保留，
  不参与运行时，发布时可丢弃。
- 卡片是"模板"，导入后由引擎物化为 `CharacterState`（`bio` = `description` +
  `personality` + `scenario` 渲染结果，`internal_state` 来自种子）。

## 3. 世界书 aise_world_v1

世界书是纯内容资产：只描述"知识条目"，不描述"知识如何进入上下文"。注入策略由
故事包的 `config` 统一声明（§4），匹配策略按 `match` 模式逐条声明（本节的匹配
设计）。结构与 ST World Info 的 `entries` 对齐（keyed map，string uid），新增
AISE 权威性字段。

```jsonc
{
  "spec": "aise_world_v1",
  "spec_version": "1.0",
  "name": "Eldoria",
  "entries": {
    "0": {
      "key": ["eldoria", "灰林", "forest"],
      "keysecondary": ["守护者"],
      "comment": "灰林设定",
      "content": "灰林是精灵族守护的古老森林…",
      "constant": false,        // true = 无条件注入
      "selective": true,        // true = 触发式注入
      "match": "keyword",       // AISE 匹配模式：keyword（默认）| embedding | llm | hybrid
      "enabled": true,
      "group": "",              // 条目分组（可选，供 group 级注入控制）
      "authority": "lore",      // AISE 扩展：lore | canonical
      "role": null              // 可选系统角色（ST role 兼容保留）
    }
  }
}
```

不再采用 ST 的 `position` / `order` / `depth` 按位置注入语义：

- ST 的 `depth` 依赖聊天历史在上下文中的物理位置，AISE 上下文按语义段组织
  （§5.1），没有"离当前消息 N 轮"的位置概念。
- AISE 的注入模型按**语义段**与**优先级**声明（§4 `config`），注入位置稳定、
  可预测，适合 Turn 驱动的确定性组装。

权威性语义（对齐 `2026-08-04-Architecture-gpt.md` §10.2 上下文分类）：

- `authority: canonical` 条目 → `ContextSource::WorldKnowledge`，可被视作世界事实，
  但不能自动写回 `WorldState`，写回必须走提案与 `ValidatedChangeSet`。
- `authority: lore`（默认）→ `ContextSource::LoreBook`，只进入上下文，永远不升级为
  世界事实。
- `constant: true` 条目进入 Baseline（无条件），`selective: true` 条目进入
  Retrieval，两者互斥，`constant` 优先。

### 3.1 匹配模式与动态 LLM 召回

匹配是检索侧的概念，决定"当前上下文该带哪些条目"。分两步落地：

**第一步（当前阶段）：确定性匹配（keyword 模式）。**

- `keyword` 模式沿用 ST 的关键词扫描：对 `key` / `keysecondary` 做规范化
  （大小写、空白、同义词表）后，在候选文本范围内做子串/词形匹配。
- 确定性、可离线测试、零 LLM 成本，作为兜底与冷启动召回。
- 扫描范围与预算由故事包 `config` 声明（§4），默认候选范围
  `recent_story` + `player_input` + `current_scene`，受 `TurnBudget`
  硬约束（条目数与 token）。

**第二步（动态 LLM 召回）：LLM 语义匹配（llm / hybrid 模式）。**

- `embedding` 模式：条目预计算向量，检索时对候选文本做向量相似度召回
  （embedding 调用统一走 `LlmGateway`，`R-CONC-04`）。
- `llm` 模式：把候选条目（key + 摘要或全文）交给 LLM，结合当前上下文做语义
  相关性判定与去重，适合概念难以用关键词表达的条目（隐喻、跨条目引用、
  模糊关联）。
- `hybrid` 模式：keyword 粗筛产生候选集 → LLM 精排；或 keyword 与 embedding
  并集后 LLM 精排。
- 动态召回必须：有界（候选集上界、单次召回条目上限、token 预算）、经
  `LlmGateway`、结果保留来源与权威性标签（`R-AISE-07`）、每次调用都有 trace
  与失败降级路径。LLM 召回失败时降级为 keyword 结果，不得让召回错误中断 Turn。
- **匹配设计对作者的承诺**：`match` 模式是逐条声明的，作者可以按条目选择
  代价与精确度的平衡；默认 `keyword` 保证任何世界书在第二步之前都可直接用。

匹配结果与注入分离：匹配只产出"命中条目集合"，注入阶段（§5.1）再按
`config.inject` 的段位与优先级放置。这样动态召回接入时，注入行为不变。

## 4. 故事包 aise_story_v1

"开箱即玩"的完整单元：一份文件 = 设定 + 玩家 + 角色 + 世界 + 开场 + 约束 + 边界。

与世界书的关系（超集关系）：故事包的 `world_book` 字段是一份完整、独立可导出的
`aise_world_v1`（共享同一 `WorldBook` 数据模型）。抽取 `world_book` + `name` 即得
独立世界书；反向把独立世界书作为 `world_book` 嵌入即成故事包。作者写知识只需学
世界书一种模型。不能反向合并的原因是 `instructions` / `config` / `boundaries` /
角色 `think_style` 等是驱动引擎逻辑的类型化规则，不是可注入的知识条目（§1）。

```jsonc
{
  "spec": "aise_story_v1",
  "spec_version": "1.0",
  "meta": {
    "title": "灰林的低语",
    "author": "SeraphinaStudio",
    "version": "1.0.0",
    "tags": ["fantasy", "mystery"],
    "description": "…",
    "cover": "data:image/png;base64,…"      // 可选封面
  },
  "instructions": {                          // 故事指令
    "system_prompt": "你是一位沉浸式叙事作者…",
    "tone": "…",
    "language": "zh-CN",
    "rules": ["玩家输入不改变已确立的世界事实"]
  },
  "config": {                               // 对应 StoryConfig
    "genre": "fantasy",
    "tone": "mysterious",
    "inject": {                             // 世界书注入模型（AISE 专用，替代 ST position/depth）
      "constant_segment": "baseline",       // constant 条目注入段：baseline（默认）
      "lore_segment": "lore",               // 命中条目注入段：lore
      "priority": 100                       // 段内优先级，越小越靠前
    },
    "retrieval": {                          // 匹配配置（供 ContextRetrievalPipeline）
      "scan_scope": ["recent_story", "player_input", "current_scene"],
      "budget": { "max_entries": 8, "max_tokens": 1200 },
      "llm": {                              // 第二步动态 LLM 召回（可选）
        "enabled": false,                   // 默认关闭，保证离线可玩
        "max_candidates": 20,
        "max_entries": 4
      }
    }
  },
  "player": { "name": "旅人", "bio": "…" },  // 可选默认玩家卡（aise_char_v1 子集）；缺失时玩家开局时选择或创建角色卡
  "characters": [ … ],                       // aise_char_v1[]，可选，无则引擎即兴生成 NPC
  "world_book": { … },                       // aise_world_v1（唯一知识入口，供全故事与所有角色共享）；缺失时依赖 LLM 常识
  "start": {
    "scene": "灰林入口的黄昏…",               // 初始场景 → current_scene
    "opening": "你站在灰林边缘…"              // 故事开场（唯一开场；角色卡不再有开场白）
  },
  "constraints": ["回合内只推进一个场景"],      // active_constraints
  "boundaries": {                            // 知识边界（供 Validation）
    "canonical_only": ["玩家身份"],
    "player_control": ["玩家角色的行动选择"]
  },
  "extensions": { "aise": { … } }            // 引擎调优：预算、验证项开关等
}
```

导入语义：

- `meta` 不进运行时，只用于目录、索引和发布清单。
- `instructions` / `config` / `constraints` / `start` → `BaselineContext`
  的 `story_instructions` / `story_config` / `active_constraints` / `current_scene`。
  **`start` 是唯一开场来源**：`scene` → `current_scene`，`opening` → 故事开场
  文本。角色卡不带开场白（§2），开场只由故事包声明一次。
- `config.inject` → Baseline 装配的注入模型：`constant_segment` /
  `lore_segment` 决定世界书条目注入哪个语义段，`priority` 决定段内排序
  （§5.1）。`config.retrieval` → `ContextRetrievalPipeline` 的匹配范围与预算
  （§5.2），`retrieval.llm` 控制第二步动态召回开关。
- `characters` / `player` → 物化为 `CharacterState`。玩家必须有角色：`player` 为
  可选默认玩家卡，提供时作为 `player_character` 候选；缺失时玩家在开局时从
  `characters` 中选择一个角色或创建一个新角色卡，`player_character` 在首个 Turn
  开始时确定。`characters` 可选，缺失时 NPC 由 LLM 在叙事中即兴生成。
- `world_book` → 可选，**全故事唯一知识入口**。提供时导入为世界书条目；
  `constant` 条目由 Baseline 无条件加载，`selective` 条目由
  `ContextRetrievalPipeline` 按 `config.retrieval` 匹配（第一步 keyword，第二步
  可开启动态 LLM 召回）。角色卡不关联世界书，角色所需知识与全故事共享同一份
  `world_book`。缺失时无种子知识注入，检索 pipeline 跳过，世界细节由 LLM 在
  叙事过程中自行构建。
- `boundaries` 交给 Validation（`R-AISE-06` 预算内完成）。

## 5. 与 Context Builder 的配合

### 5.1 组装顺序

`BaselineContextBuilder` 按语义段组装，不再采用 ST 的 `wiBefore` / `wiAfter`
按位置注入：

```text
story_instructions        <- story.instructions.system_prompt + 角色 system_prompt
active_constraints        <- story.constraints
story_config              <- story.config
player_character          <- story.player（可选默认卡；缺失时开局时玩家选择/创建）
relevant_characters       <- story.characters（可选，缺失时 LLM 生成 NPC）
current_scene             <- story.start.scene（唯一开场来源）
world knowledge           <- world_book constant entries，按 config.inject.constant_segment 注入
retrieved lore            <- ContextRetrievalPipeline 命中条目，按 config.inject.lore_segment 注入
recent_story              <- store.load_story（历史 Turn）
story_summary             <- store（可重建投影）
player_input              <- TurnRequest
```

注入模型：命中集合（constant 与 selective 的并集）按 `config.inject` 声明分到
`constant_segment` / `lore_segment` 两个语义段，段内按 `priority` 稳定排序。
匹配（§3.1）与注入解耦：`ContextRetrievalPipeline` 只产出命中条目集合，注入位置
与顺序完全由 `config.inject` 决定，因此第二步动态召回接入时注入行为不变。

`ContextSource` 分类与权威性映射（`ctx_model.rs` 已预留 `LoreBook`）：

| 来源 | ContextSource | 权威性 | 是否可写回 WorldState |
| --- | --- | --- | --- |
| 世界书 canonical 条目 | `WorldKnowledge` | Canonical World Fact | 只能经提案+验证 |
| 世界书 lore 条目 | `LoreBook` | Retrieved Lore | 永不 |
| 角色记忆 | `CharacterMemory` | Character Memory | 永不（保留为主观认知） |
| 角色思考 | 无（`thoughts`） | Character Thought | 永不（`R-AISE-07`） |
| 历史 Turn | `HistoricalStory` | Narrative History | 已提交，属权威历史 |
| 摘要 | 无（`story_summary`） | Narrative Summary | 可重建 |

### 5.2 检索：两阶段匹配

`ContextRetrievalPipeline` 按条目 `match` 模式（§3.1）分两步执行，受
`TurnBudget` 硬约束：

**第一阶段（确定性粗筛，keyword 模式，当前实现）。**

- 扫描范围 `config.retrieval.scan_scope`（默认 `recent_story` + `player_input` +
  `current_scene`），文本先做 `truncate`，再对 `match = keyword` 的启用条目做
  key 匹配。
- 产出候选集合，保留来源标签与权威性，不扁平化成一个裸文本段。
- 上界：条目数 ≤ `config.retrieval.budget.max_entries`，token ≤
  `budget.max_tokens`，并受 `TurnBudget.max_retrieved_items` 总裁剪
  （`R-ARCH-04`）。

**第二阶段（动态 LLM 召回，llm / hybrid / embedding 模式，第二步）。**

- `config.retrieval.llm.enabled = true` 时执行；`embedding` 条目的向量召回与
  `llm` 条目的语义判定都统一走 `LlmGateway`（`R-CONC-04`）。
- `hybrid` 条目：先 keyword 粗筛 → LLM 精排；`embedding` 条目：向量相似度召回；
  `llm` 条目：候选（key + 摘要）交 LLM 判定。
- 每次 LLM 召回有界：`max_candidates` 限制送入判定的候选数，`max_entries`
  限制最终新增条目数；失败降级为第一阶段结果，不得中断 Turn。
- 两阶段命中合并在注入阶段（§5.1）统一去重与排序。

无种子条目或 `scan_scope` 为空时整个检索跳过，不产生 Stage 事件。

### 5.3 与 Character Think / Validation 的配合

- `extensions.aise.think_style` 与 `knowledge_boundary` 供 `CharacterThinkPipeline`
  构造提示（`R-OBS-02` 要求包 span，结构化字段）。
- `boundaries` 与 `knowledge_boundary` 供 `ValidationPipeline` 做
  `Knowledge Boundary` / `Player Control Boundary` 检查（确定性部分为硬门槛，
  语义部分走 Narrative Validation，`2026-08-04-Architecture-gpt.md` §13）。

## 6. 制作与发布工作流

目标：让"写一个故事给别人玩"收敛到三步——**写一份 JSON → 校验 → 导入/分享**。

```text
作者编辑 story.aise.json（或卡片、世界书单体）
      |
      v
aise pack validate story.aise.json   # 离线校验，无 LLM
      |
      v
aise pack import story.aise.json     # 创建 story + 种子导入（幂等）
      |
      v
玩家通过 Turn API 游玩（故事卡只读，运行状态走 TurnCommit）
      |
      v
aise pack export --story <id>        # 导出标准包（可携带封面 PNG）
      |
      v
发布（仓库 / 分享）
```

### 6.1 CLI 命令面

- `aise pack validate <file>`：校验 `spec` / `spec_version` / 必填字段 /
  世界书条目引用完整性；输出诊断（复用 `validation` 目录的 issue 结构）。
- `aise pack import <file>`：幂等导入。同 `story_id`（由 `meta.title` + 版本哈希
  派生）重复导入返回原 story，不重复种入（对齐 `2026-08-04-Architecture-gpt.md` §4.3 幂等语义）。
- `aise pack export --story <id>`：从权威状态导出标准故事包，可带封面图。

### 6.2 导入落库（种子 vs 运行状态）

新增种子表，与运行状态分离，避免覆盖 `FactSource` 语义（修复
`world == None` 覆盖问题）：

```sql
CREATE TABLE IF NOT EXISTS story_packs (
    id         TEXT PRIMARY KEY,
    story_id   TEXT NOT NULL REFERENCES worlds(id),
    spec       TEXT NOT NULL,
    spec_version TEXT NOT NULL,
    payload    TEXT NOT NULL,          -- 原始包 JSON（可重建、可重导入）
    imported_at INTEGER NOT NULL,
    UNIQUE (story_id, spec, spec_version)
);

CREATE TABLE IF NOT EXISTS lore_entries (
    id            TEXT PRIMARY KEY,
    world_id      TEXT NOT NULL REFERENCES worlds(id),
    entry_key     TEXT NOT NULL,        -- key 的扁平化文本
    key_secondary TEXT NOT NULL DEFAULT '',
    content       TEXT NOT NULL,
    constant      INTEGER NOT NULL DEFAULT 0,
    selective     INTEGER NOT NULL DEFAULT 1,
    match_mode    TEXT NOT NULL DEFAULT 'keyword',  -- keyword | embedding | llm | hybrid（§3.1）
    priority      INTEGER NOT NULL DEFAULT 100,     -- 注入段内优先级（替代 ST position/order/depth）
    enabled       INTEGER NOT NULL DEFAULT 1,
    grp           TEXT NOT NULL DEFAULT '',
    authority     TEXT NOT NULL DEFAULT 'lore',
    role          TEXT
);
```

`worlds.state` 中的 `WorldFact` 仍以 `FactSource::Seed` 标注故事包种入的事实；
`lore_entries` 是检索索引，`WorldState.facts` 是权威事实，两者不重复写。
导入只种种子；运行时新增事实必须 `source = CommittedTurn`，由
`TurnCommitter` 事务写入（`R-AISE-05`）。

注入与匹配配置（`config.inject` / `config.retrieval`，§4）随 `story_packs.payload`
原子导入，运行时由 Baseline Builder 解析进 `StoryReadSnapshot`，不在
`lore_entries` 中逐行复制。第二步动态召回所需的 embedding 向量属于可重建的派生
状态（`2026-08-04-Architecture-gpt.md` §14.2），通过 outbox 在事务提交后更新，不落入
`lore_entries`。

### 6.3 Store 端口扩展

`Store` trait（`persistence/store.rs`）新增只读入口与导入入口：

- `import_story_pack(&pack) -> StoryId`：原子导入 story + characters + lore
  （单事务）。
- `load_world_book(&story_id, limit) -> Vec<LoreEntry>`：Baseline 与 Retrieval
  共用。
- `load_story_pack_meta(&story_id) -> Option<PackMeta>`：供 export 与目录展示。
- `load_pack_config(&story_id) -> Option<PackConfig>`：读取 `config.inject` /
  `config.retrieval`，供 Baseline 装配与 `ContextRetrievalPipeline` 使用。

导入/导出属于管理面（`aise-server`），不进入 Turn 固定工作流，不触碰
`TurnRuntime`（`R-AISE-01`）。

## 7. 与 SillyTavern 的兼容对照

| AISE | SillyTavern | 差异说明 |
| --- | --- | --- |
| `aise_char_v1` | `chara_card_v2` | 移除开场白（`first_mes` / `alternate_greetings`）与 `world` 关联；开场与知识统一由故事包承担 |
| `aise_world_v1` | World Info JSON | entry 字段同名；`position` / `order` / `depth` 替换为 `match` 模式与 `priority`；新增 `authority` |
| `aise_story_v1` | 无直接对应 | 故事级设定 + 组合资产，ST 靠手工拼 |
| `character_book` 内嵌书 | V2 CharacterBook | 仅导入兼容，知识并入故事包 `world_book`，不参与 AISE 运行时 |
| Baseline 组装顺序 | Prompt Manager 顺序 | 按语义段 + `config.inject` 注入模型，见 §5.1 |
| selective 检索 | World Info 关键词扫描 | 两阶段匹配：keyword 粗筛（第一步）+ 动态 LLM 召回（第二步） |
| PNG 封面/tEXt | PNG tEXt chunk | 复用约定，格式层对齐 |
| `extensions.*` | 扩展字段 | 未知字段透传保留 |

## 8. 实施建议（按依赖排序）

1. 领域模型：在 `domain/` 新增卡片/世界书/故事包的 `Card`、`WorldBook`、
   `StoryPack` 类型与 `pack` 模块；`StoryConfig`、`BaselineContext` 字段已有承接；
   注入模型与匹配配置进入 `StoryConfig`（`inject` / `retrieval`）。
2. 校验与迁移：新增 pack validator（spec 识别 + 字段校验 + 默认值补齐，含
   `match` 模式与 `config.inject` / `config.retrieval` 的合法性），复用
   `validation` 的 issue 结构。
3. 种子持久化：迁移 `0002` 增加 `story_packs` / `lore_entries`（`match_mode` /
   `priority`，无 `position` / `depth`），`Store` 扩展 import / load 入口
   （见 §6.3），一次性修复 `world == None` 覆盖问题。
4. Baseline 装配：`BaselineContextBuilder` 加载故事包、`config.inject` 注入模型
   与 constant 世界书条目；`start` 作为唯一开场来源。
5. 检索第一步：`ContextRetrievalPipeline` keyword 粗筛 + 预算裁剪，注入按
   `config.inject` 语义段装配。
6. 管理面：`aise-server` 增加 `/packs/validate|import|export` 与 CLI 命令。
7. 示例：`assets/packs/` 放一个随附示例故事包（对齐 ST 的 `Eldoria.json` 示例
   定位），内含故事开场与共享世界书，展示角色卡无开场白、无知识字段的形态。
8. 检索第二步（后续迭代）：动态 LLM 召回——`embedding` / `llm` / `hybrid`
   匹配经 `LlmGateway` 接入，支持降级与预算控制（§3.1、§5.2）。

## 9. 验收标准

1. 一份 `aise_story_v1` JSON 可以"校验 → 导入 → 开玩"全流程跑通，不要求任何手工
   数据库操作。
2. 世界书 `constant` / `selective` / `match` / `priority` 语义在上下文中生效，
   条目按 `config.inject` 注入到指定语义段且保留权威性标签；ST 的 `position` /
   `order` / `depth` 不进入 AISE 运行时。
3. `lore` 条目永不写回 `WorldState`；只有 `ValidatedChangeSet` 能产生新世界事实。
4. 重复导入同一故事包幂等，不重复种入。
5. 导入不覆盖已提交的运行状态（种子与运行状态分离）。
6. 上下文大小、检索条目数与 token 均受 `TurnBudget` 限制。
7. ST 的 `chara_card_v2` / World Info JSON 可直接导入为 `aise_char_v1` /
   `aise_world_v1`（字段映射兼容；卡片开场白与内嵌世界书被故事包打包流程吸收）。
8. 角色卡不含开场白与关联世界书；故事开场只来自故事包 `start`，知识只来自
   `world_book`（全故事共享）。
9. 开启动态 LLM 召回后：召回有界（`max_candidates` / `max_entries`），LLM 失败
   自动降级为 keyword 结果且不中断 Turn，注入行为与关闭召回时一致。
