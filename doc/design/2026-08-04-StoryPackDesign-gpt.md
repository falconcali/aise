# AISE 故事包 / 卡片体系设计

> 参考：[SillyTavern Character Card V2/V3](https://github.com/SillyTavern/SillyTavern) 的
> `spec: chara_card_v2` 卡片、World Info 世界书条目（key / position / order / depth /
> constant / selective）与 Context Builder 的上下文组装顺序。
>
> 本文是设计文档，只定义目标格式、语义与集成点，不约束实现细节。与
> `Architecture.md` 冲突时以 `Architecture.md` 为准。

## 1. 设计目标

SillyTavern 的资产模型是"聊天工具 + 可交换资产"：角色卡（V2/V3）、世界书（World
Info）、上下文预设三件套，靠 `extensions` 字段做跨平台兼容。AISE 是 Turn 驱动的
叙事引擎，角色卡只是引擎输入，不是聊天界面的一部分。因此直接照搬 ST 的资产模型
会缺两块 ST 没有的东西：

- **故事级设定**（Story Instructions、约束、知识边界、开场场景）——ST 靠场景文本
  和用户手动填，AISE 需要结构化输入。
- **权威性语义**（`Canonical World Fact` / `Character Belief` / `Character Memory` /
  `Retrieved Lore`，见 `Architecture.md` §10.2）——世界书条目必须带来源标签，不能
  全部当世界事实，否则违反 `R-AISE-07`。

设计原则：

- 三层资产：`aise_char_v1`（人物卡）→ `aise_world_v1`（世界书）→ `aise_story_v1`
  （故事包）。前两者可独立发布和复用，故事包是"开箱即玩"的完整单元。
- **世界书是内容资产，故事包是组合资产，二者不是同一层级，不合并为单一类型。
  世界书是故事包的一部分（`world_book ⊂ story_pack`），不是反过来。** 共享同一
  `WorldBook` 数据模型：故事包的 `world_book` 字段就是一份完整、独立可导出的
  世界书（§4），作者写"知识"只有一个心智模型。
- **知识（进上下文）与规则（驱动逻辑）必须分离，规则不扁平化成世界书条目。**
  `instructions.rules`、`boundaries`、`config`、角色卡 `think_style` /
  `knowledge_boundary` 是 Validator / Planner / Character Think 的确定性输入，
  需要类型化字段供确定性校验（`Architecture.md` §13.1），文本条目无法承担。
- 参考 ST 字段语义（`first_mes`、`alternate_greetings`、`mes_example`、世界书
  `constant` / `selective` / `order` / `position` / `depth`），但明确标注权威性与
  知识边界，供 Validation 与 Retrieval 使用。
- 单一 JSON 文件承载完整故事包，导入即玩；可选 PNG 封面（复用 ST 的 tEXt chunk
  嵌入约定）。
- 种子数据（Seed）与运行时状态（Committed）分离：卡片/世界书只做一次种子导入，
  运行时只能通过 `ValidatedChangeSet` 产生 `FactSource::CommittedTurn` 事实。
- **故事包是唯一必选格式，人物卡与角色卡、世界书均为可选增强。** 缺失时引擎在
  运行时依赖 LLM 即兴生成角色设定与世界细节，不要求作者手动填充所有字段。
- **玩家必须有角色。** 故事包可提供默认玩家卡（`player`），缺失时玩家在开局时
  从 `characters` 中选择一个角色，或创建一个新角色卡。

## 2. 人物卡 aise_char_v1

与 ST `chara_card_v2` 同构，保留生态里熟悉的字段名，只增不删。字段全部可选，导入
时按默认值补齐。

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
    "first_mes": "…",                       // 开场白
    "alternate_greetings": ["…"],           // 备选开场白
    "mes_example": "…",                     // 示例对话（few-shot，注入 dialogueExamples）
    "system_prompt": "…",                   // 角色级系统提示，覆盖 main
    "post_history_instructions": "…",       // 角色级后置指令，覆盖 jailbreak
    "creator": "…",
    "character_version": "1.0",
    "tags": ["elves", "guardian"],
    "extensions": {
      "talkativeness": 0.5,
      "fav": false,
      "world": "Eldoria",                   // 关联世界书（可选）
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
    "character_book": { … }                 // 内嵌世界书（V2 兼容，可选）
  }
}
```

说明：

- `spec` / `spec_version` 顶层标记与 ST 完全一致，便于导入器识别。
- `extensions.aise` 是 AISE 私有扩展命名空间；未知 `extensions` 字段透传保留，
  不参与运行时，发布时可丢弃。
- 卡片是"模板"，导入后由引擎物化为 `CharacterState`（`bio` = `description` +
  `personality` + `scenario` 渲染结果，`internal_state` 来自种子）。

## 3. 世界书 aise_world_v1

结构与 ST World Info 对齐，`entries` 为 keyed map（string uid），每条 entry 语义
兼容，新增 AISE 权威性字段。

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
      "constant": false,        // true = 无条件注入（ST constant）
      "selective": true,        // true = 关键词触发（ST selective）
      "position": "before",     // before | after | atDepth（对齐 ST 0/1/4）
      "order": 100,             // 同位置内排序（越小越靠前，ST order）
      "depth": 4,               // atDepth 时的注入深度（ST depth）
      "enabled": true,
      "group": "",              // 条目分组（ST group，可选）
      "authority": "lore",      // AISE 扩展：lore | canonical
      "role": null              // 可选系统角色（ST role）
    }
  },
  "extensions": {
    "aise": {
      "scan_scope": ["recent_story", "player_input", "current_scene"],
      "budget": { "max_entries": 8, "max_tokens": 1200 }
    }
  }
}
```

权威性语义（对齐 `Architecture.md` §10.2 上下文分类）：

- `authority: canonical` 条目 → `ContextSource::WorldKnowledge`，可被视作世界事实，
  但不能自动写回 `WorldState`，写回必须走提案与 `ValidatedChangeSet`。
- `authority: lore`（默认）→ `ContextSource::LoreBook`，只进入上下文，永远不升级为
  世界事实。
- `constant: true` 条目进入 Baseline（无条件），`selective: true` 条目进入
  Retrieval（关键词扫描），两者互斥，`constant` 优先。

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
  "config": { "genre": "fantasy", "tone": "mysterious" },  // 对应 StoryConfig
  "player": { "name": "旅人", "bio": "…" },  // 可选默认玩家卡（aise_char_v1 子集）；缺失时玩家开局时选择或创建角色卡
  "characters": [ … ],                       // aise_char_v1[]，可选，无则引擎即兴生成 NPC
  "world_book": { … },                       // aise_world_v1，可选，无则依赖 LLM 常识
  "start": {
    "scene": "灰林入口的黄昏…",               // 初始场景 → current_scene
    "first_mes": "你站在灰林边缘…"            // 开场白
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
- `characters` / `player` → 物化为 `CharacterState`。玩家必须有角色：`player` 为
  可选默认玩家卡，提供时作为 `player_character` 候选；缺失时玩家在开局时从
  `characters` 中选择一个角色或创建一个新角色卡，`player_character` 在首个 Turn
  开始时确定。`characters` 可选，缺失时 NPC 由 LLM 在叙事中即兴生成。
- `world_book` → 可选。提供时导入为世界书条目；`constant` 条目由 Baseline 无条件
  加载，`selective` 条目由 Retrieval 关键词扫描。缺失时无种子知识注入，检索
  pipeline 跳过，世界细节由 LLM 在叙事过程中自行构建。
- `boundaries` 交给 Validation（`R-AISE-06` 预算内完成）。

## 5. 与 Context Builder 的配合

### 5.1 组装顺序

`BaselineContextBuilder` 的加载顺序对齐 ST 的
`main → wiBefore → persona → description → personality → scenario →
wiAfter → dialogueExamples`，但按 AISE 的 Baseline 字段重组：

```text
story_instructions        <- story.instructions.system_prompt + 角色 system_prompt
active_constraints        <- story.constraints
story_config              <- story.config
player_character          <- story.player（可选默认卡；缺失时开局时玩家选择/创建）
relevant_characters       <- story.characters（可选，缺失时 LLM 生成 NPC）
current_scene             <- story.start.scene
world knowledge           <- world_book constant entries（authority=canonical，缺失时为空）
retrieved lore            <- ContextRetrievalPipeline（selective 条目，无种子条目时跳过）
recent_story              <- store.load_story（历史 Turn）
story_summary             <- store（可重建投影）
player_input              <- TurnRequest
```

`ContextSource` 分类与权威性映射（`ctx_model.rs` 已预留 `LoreBook`）：

| 来源 | ContextSource | 权威性 | 是否可写回 WorldState |
| --- | --- | --- | --- |
| 世界书 canonical 条目 | `WorldKnowledge` | Canonical World Fact | 只能经提案+验证 |
| 世界书 lore 条目 | `LoreBook` | Retrieved Lore | 永不 |
| 角色记忆 | `CharacterMemory` | Character Memory | 永不（保留为主观认知） |
| 角色思考 | 无（`thoughts`） | Character Thought | 永不（`R-AISE-07`） |
| 历史 Turn | `HistoricalStory` | Narrative History | 已提交，属权威历史 |
| 摘要 | 无（`story_summary`） | Narrative Summary | 可重建 |

### 5.2 Selective 检索

`ContextRetrievalPipeline` 实现 ST 的关键词扫描逻辑，但受 `TurnBudget` 硬约束：

- 扫描范围 `scan_scope`（默认 `recent_story` + `player_input` +
  `current_scene`），文本先做 `truncate`，再对启用条目做 key 匹配。
- 命中按 `position` 分组（`before` / `after` / `atDepth`），组内按 `order`
  稳定排序。
- 输出合并入 Context，但**保留来源标签与权威性**，不扁平化成一个裸文本段。
- 上界：条目数 ≤ `budget.max_entries`，token ≤ `budget.max_tokens`，并受
  `TurnBudget.max_retrieved_items` 总裁剪（`R-ARCH-04`）。
- 未来接入 embedding 时（`R-CONC-04`），semantic 触发作为 `selective` 的补充
  方式，仍走同一限流器。

### 5.3 与 Character Think / Validation 的配合

- `extensions.aise.think_style` 与 `knowledge_boundary` 供 `CharacterThinkPipeline`
  构造提示（`R-OBS-02` 要求包 span，结构化字段）。
- `boundaries` 与 `knowledge_boundary` 供 `ValidationPipeline` 做
  `Knowledge Boundary` / `Player Control Boundary` 检查（确定性部分为硬门槛，
  语义部分走 Narrative Validation，`Architecture.md` §13）。

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
  派生）重复导入返回原 story，不重复种入（对齐 `Architecture.md` §4.3 幂等语义）。
- `aise pack export --story <id>`：从权威状态导出标准故事包，可带封面图。

### 6.2 导入落库（种子 vs 运行状态）

新增种子表，与运行状态分离，避免覆盖 `FactSource` 语义（修复
`当前架构代码与设计对比.md` §10.2 的 `world == None` 覆盖问题）：

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
    entry_key     TEXT NOT NULL,        -- ST key 的扁平化文本
    key_secondary TEXT NOT NULL DEFAULT '',
    content       TEXT NOT NULL,
    constant      INTEGER NOT NULL DEFAULT 0,
    selective     INTEGER NOT NULL DEFAULT 1,
    position      TEXT NOT NULL DEFAULT 'before',
    ord           INTEGER NOT NULL DEFAULT 100,
    depth         INTEGER NOT NULL DEFAULT 4,
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

### 6.3 Store 端口扩展

`Store` trait（`persistence/store.rs`）新增只读入口与导入入口：

- `import_story_pack(&pack) -> StoryId`：原子导入 story + characters + lore
  （单事务）。
- `load_world_book(&story_id, limit) -> Vec<LoreEntry>`：Baseline 与 Retrieval
  共用。
- `load_story_pack_meta(&story_id) -> Option<PackMeta>`：供 export 与目录展示。

导入/导出属于管理面（`aise-server`），不进入 Turn 固定工作流，不触碰
`TurnRuntime`（`R-AISE-01`）。

## 7. 与 SillyTavern 的兼容对照

| AISE | SillyTavern | 差异说明 |
| --- | --- | --- |
| `aise_char_v1` | `chara_card_v2` | 字段同名透传；新增 `extensions.aise` 权威/边界语义 |
| `aise_world_v1` | World Info JSON | entry 字段同名；新增 `authority`、`scan_scope`、`budget` |
| `aise_story_v1` | 无直接对应 | 故事级设定 + 组合资产，ST 靠手工拼 |
| `character_book` 内嵌书 | V2 CharacterBook | 双向兼容（导入/导出） |
| Baseline 组装顺序 | Prompt Manager 顺序 | 见 §5.1 |
| selective 检索 | World Info 关键词扫描 | 增加预算上界与权威性保留 |
| PNG 封面/tEXt | PNG tEXt chunk | 复用约定，格式层对齐 |
| `extensions.*` | 扩展字段 | 未知字段透传保留 |

## 8. 实施建议（按依赖排序）

1. 领域模型：在 `domain/` 新增卡片/世界书/故事包的 `Card`、`WorldBook`、
   `StoryPack` 类型与 `pack` 模块；`StoryConfig`、`BaselineContext` 字段已有承接。
2. 校验与迁移：新增 pack validator（spec 识别 + 字段校验 + 默认值补齐），复用
   `validation` 的 issue 结构。
3. 种子持久化：迁移 `0002` 增加 `story_packs` / `lore_entries`，`Store` 扩展
   import / load 入口（见 §6.3），一次性修复 `world == None` 覆盖问题。
4. Baseline 装配：`BaselineContextBuilder` 加载故事包与 constant 世界书条目。
5. Selective 检索：`ContextRetrievalPipeline` 关键词扫描 + 预算裁剪。
6. 管理面：`aise-server` 增加 `/packs/validate|import|export` 与 CLI 命令。
7. 示例：`assets/packs/` 放一个随附示例故事包（对齐 ST 的 `Eldoria.json` 示例
   定位）。

## 9. 验收标准

1. 一份 `aise_story_v1` JSON 可以"校验 → 导入 → 开玩"全流程跑通，不要求任何手工
   数据库操作。
2. 世界书 `constant` / `selective` / `position` / `order` / `depth` 语义在上下文
   中生效，且所有条目保留权威性标签。
3. `lore` 条目永不写回 `WorldState`；只有 `ValidatedChangeSet` 能产生新世界事实。
4. 重复导入同一故事包幂等，不重复种入。
5. 导入不覆盖已提交的运行状态（种子与运行状态分离）。
6. 上下文大小、检索条目数与 token 均受 `TurnBudget` 限制。
7. ST 的 `chara_card_v2` / World Info JSON 可直接导入为 `aise_char_v1` /
   `aise_world_v1`（字段映射兼容）。
