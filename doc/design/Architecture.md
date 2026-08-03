# AI Story Engine 技术架构设计 v2.0

## 1. 概述

AI Story Engine 是一个基于 Turn 的互动叙事生成系统。

一次玩家输入触发一个完整 Story Turn：

```
Player Input
      ↓
Turn Runtime
      ↓
Context Preparation
      ↓
Story Planning
      ↓
Story Generation
      ↓
Validation / Repair Loop
      ↓
Turn Commit
      ↓
Turn Result
```

系统采用 Pipeline 架构。

每个执行步骤都是独立模块，并实现统一 Pipeline 接口，由 Turn Runtime 按流程编排执行。

核心目标：

- 模块职责单一。
- 流程可扩展。
- 每个阶段可独立替换。
- 支持复杂 AI 推理流程。
- 保证故事连续性、一致性和可控性。

---

# 2. 总体架构

```
Story Client
      |
      v
Story Turn API
      |
      v
Turn Runtime
      |
      +-- Turn Initializer
      |
      +-- Baseline Context Builder
      |
      +-- Writer Planner
      |
      +-- Context Retrieval Pipeline
      |
      +-- Character Think Pipeline
      |
      +-- Story Generator
      |
      +-- Validation Pipeline
      |
      +-- Story Repairer
      |
      +-- Turn Committer
      |
      v
Turn Result
```

Turn Runtime 负责一次 Turn 的生命周期管理。

所有业务步骤均通过 Pipeline 接口执行。

---



# 3. Turn Execution Pipeline

所有 Turn 执行步骤统一实现：

```rust
#[async_trait]
trait TurnExecutionPipeline {

    async fn execute(
        &self,
        ctx: &mut TurnExecutionContext
    );

}
```

Pipeline 职责：

- 接收共享 TurnExecutionContext。
- 修改当前 Turn 执行状态。
- 输出下一阶段所需数据。

优势：

- Runtime 不需要关注具体业务实现。
- 新增步骤只需要实现 Pipeline。
- 支持动态组合 Pipeline。

---



# 4. Turn Runtime



## 职责

Turn Runtime 是 Turn Workflow Orchestrator。

负责：

- 创建 TurnExecutionContext。
- 按顺序执行 Pipeline。
- 控制条件 Pipeline。
- 管理 Validation / Repair Loop。
- 控制 Turn 生命周期。

不负责：

- 生成故事。
- 判断剧情方向。
- 修改持久化状态。



## 调用流程

```rust
async fn execute_turn(
    request: TurnRequest
) -> TurnResult {

    let mut ctx =
        TurnExecutionContext::new(request);


    turn_initializer
        .execute(&mut ctx)
        .await;


    baseline_ctx_builder
        .execute(&mut ctx)
        .await;


    writer_planner
        .execute(&mut ctx)
        .await;


    if ctx.plan.need_retrieval {

        ctx_retrieval_pipeline
            .execute(&mut ctx)
            .await;
    }


    if ctx.plan.need_character_thinking {

        character_think_pipeline
            .execute(&mut ctx)
            .await;
    }


    story_generator
        .execute(&mut ctx)
        .await;


    loop {

        validation_pipeline
            .execute(&mut ctx)
            .await;


        if ctx.validation.pass {
            break;
        }


        story_repairer
            .execute(&mut ctx)
            .await;
    }


    turn_committer
        .execute(&mut ctx)
        .await
}
```

---



# 5. 核心数据结构



## TurnExecutionContext

TurnExecutionContext 是一次 Turn 执行期间的共享上下文。

生命周期：

```
Turn Start
    ↓
Create Context
    ↓
Pipeline 修改
    ↓
Commit
    ↓
Destroy
```

数据结构：

```rust
struct TurnExecutionContext {

    request: TurnRequest,


    // AI 基础认知上下文
    baseline_ctx: BaselineContext,


    // Planner 输出
    plan: Option<WriterPlan>,


    // Retrieval 结果
    retrieved_ctx:
        Vec<ctxItem>,


    // NPC 推演结果
    character_thoughts:
        Vec<CharacterThought>,


    // 当前故事结果
    draft:
        Option<StoryDraft>,


    // 验证结果
    validation:
        ValidationResult,


    // 执行控制
    budget:
        TurnBudget,


    trace:
        TraceRecorder,
}
```

TurnExecutionContext：

- 只存在于当前 Turn。
- 不直接持久化。
- 所有 Pipeline 共享。

---



# 6. Turn Initializer



## 职责

初始化 Turn 执行环境。

只负责准备必要对象，不负责业务数据加载。

负责：

- 初始化 Context。
- 初始化 Trace。
- 初始化 Budget。
- 初始化运行参数。
- 校验基础请求。

不负责：

- 加载世界状态。
- 构建 Story Context。
- 获取 Memory。

接口：

```rust
struct TurnInitializer;

impl TurnExecutionPipeline for TurnInitializer {

    async fn execute(
        &self,
        ctx: &mut TurnExecutionContext
    ) {

    }

}
```

---



# 7. Baseline Context Builder



## 职责

构建 Story Generator 所需的基础 Context。

它负责将已有世界信息整理成 AI 可理解的数据结构。

不负责剧情生成。

接口：

```rust
struct BaselineContextBuilder;


impl TurnExecutionPipeline
for BaselineContextBuilder {

    async fn execute(
        &self,
        ctx: &mut TurnExecutionContext
    );

}
```



## Story Context

基础 Context：

```
Story Instructions

Story Configuration

Player Character

Current Scene

Relevant Characters

Recent Story

Story Summary

Active Constraints

Player Input
```

说明：

- 这些属于 AI 当前认知输入。
- 类似 SillyTavern 的角色卡、世界书、聊天历史组合。
- 后续可以通过 Context Builder 插入额外 Lore / World Book 数据。

---



# 8. Writer Planner



## 职责

分析当前 Turn，并规划故事生成需要的信息。

负责：

- 理解 Player Input。
- 判断故事目标。
- 判断上下文缺口。
- 决定是否需要 Retrieval。
- 决定是否需要 NPC Think。

接口：

```rust
struct WriterPlanner;


impl TurnExecutionPipeline
for WriterPlanner {

    async fn execute(
        &self,
        ctx: &mut TurnExecutionContext
    );

}
```

输出：

```rust
struct WriterPlan {

    need_retrieval: bool,

    need_character_thinking: bool,


    retrieval_requests:
        Vec<ContextRequest>,


    character_requests:
        Vec<CharacterId>,


    story_goal:
        StoryGoal,
}
```

---



# 9. Context Retrieval Pipeline



## 职责

根据 Planner 请求补充额外上下文。

可能来源：

- Character Memory
- World Knowledge
- Narrative Graph
- Historical Story
- Lore Book

接口：

```rust
struct ContextRetrievalPipeline;


impl TurnExecutionPipeline
for ContextRetrievalPipeline {

    async fn execute(
        &self,
        ctx: &mut TurnExecutionContext
    );

}
```

内部：

```
Retriever

    ↓

Context Merger

    ↓

Context Items
```

---



# 10. Character Think Pipeline



## 职责

模拟关键角色当前认知。

输出：

- 角色感知。
- 情绪。
- 目标。
- 行动倾向。

注意：

Character Thought 不是世界事实。

它只是角色视角信息。

接口：

```rust
struct CharacterThinkPipeline;


impl TurnExecutionPipeline
for CharacterThinkPipeline {

    async fn execute(
        &self,
        ctx: &mut TurnExecutionContext
    );

}
```

数据：

```rust
struct CharacterThought {

    character_id: CharacterId,

    perception: String,

    emotion: String,

    goal: String,

    possible_action: String,
}
```

---



# 11. Story Generator



## 职责

生成新的故事结果。

接口：

```rust
struct StoryGenerator;


impl TurnExecutionPipeline
for StoryGenerator {

    async fn execute(
        &self,
        ctx: &mut TurnExecutionContext
    );

}
```

输入：

```
Story Context

+

Writer Plan

+

Retrieved Context

+

Character Thoughts
```

输出：

```rust
struct StoryDraft {

    story_text: String,


    events:
        Vec<StoryEvent>,


    character_updates:
        Vec<CharacterPatch>,


    world_updates:
        Vec<WorldPatch>,


    memory_updates:
        Vec<MemoryPatch>,
}
```

---



# 12. Story Repairer



## 职责

负责修复 Validation 未通过的 Story Draft。

独立于 Story Generator。

原因：

- Story Generation 和 Quality Repair 是不同能力。
- Repair 需要关注约束违反、逻辑错误、一致性问题。

接口：

```rust
struct StoryRepairer;


impl TurnExecutionPipeline
for StoryRepairer {

    async fn execute(
        &self,
        ctx: &mut TurnExecutionContext
    );

}
```

流程：

```
Story Draft

    ↓

Validation Result

    ↓

Story Repairer

    ↓

Updated Story Draft
```

---



# 13. Validation Pipeline



## 职责

验证 Story Draft 是否可以提交。

接口：

```rust
struct ValidationPipeline;


impl TurnExecutionPipeline
for ValidationPipeline {

    async fn execute(
        &self,
        ctx: &mut TurnExecutionContext
    );

}
```

验证：

## Deterministic Validation

- Schema。
- 数据合法性。
- 状态修改权限。
- Constraint。



## Story Validation

- Character Consistency。
- Narrative Consistency。
- Knowledge Boundary。
- Player Control Boundary。

输出：

```rust
struct ValidationResult {

    pass: bool,

    issues:
        Vec<ValidationIssue>,
}
```

---



# 14. Turn Committer



## 职责

提交最终 Turn 结果。

负责：

- 保存 Story Turn。
- 保存 Event。
- 更新 Character State。
- 更新 World State。
- 更新 Memory。
- 更新 Summary。

接口：

```rust
struct TurnCommitter;


impl TurnExecutionPipeline
for TurnCommitter {

    async fn execute(
        &self,
        ctx: &mut TurnExecutionContext
    );

}
```

提交必须保证：

- 原子性。
- 一致性。
- 可恢复。

---



# 15. 模块目录结构

目录结构体现模块依赖关系：

```
ai_story_engine/

├── runtime/
│
│   ├── turn_runtime.rs
│   ├── turn_execution_ctx.rs
│   ├── pipeline.rs
│   └── initializer.rs
│


├── context/
│
│   ├── baseline_ctx_builder.rs
│   ├── retrieval_pipeline.rs
│   └── ctx_model.rs
│


├── planning/
│
│   └── writer_planner.rs
│


├── character/
│
│   ├── character_think_pipeline.rs
│   └── character_model.rs
│


├── story/
│
│   ├── story_generator.rs
│   ├── story_repairer.rs
│   └── story_model.rs
│


├── validation/
│
│   ├── validation_pipeline.rs
│   └── validators/
│


├── persistence/
│
│   └── turn_committer.rs
│


└── domain/

    ├── world.rs
    ├── character.rs
    ├── memory.rs
    └── narrative.rs
```

---



# 16. 架构总结

AI Story Engine 是一个 Pipeline 驱动的 Turn-based Narrative Engine。

核心设计：

```
Turn Runtime
        |
        |
TurnExecutionContext
        |
        |
Pipeline Chain
        |
        |
Story Result
```

职责：


| 模块                         | 职责         |
| -------------------------- | ---------- |
| Turn Runtime               | 流程编排       |
| Turn Initializer           | 初始化执行环境    |
| Baseline Context Builder   | 构建 AI 基础认知 |
| Writer Planner             | 制定生成计划     |
| Context Retrieval Pipeline | 补充信息       |
| Character Think Pipeline   | NPC 推演     |
| Story Generator            | 创建故事       |
| Story Repairer             | 修复故事       |
| Validation Pipeline        | 保证质量       |
| Turn Committer             | 提交状态       |


最终目标：

通过模块化 Pipeline、结构化 Context 和可控生成流程，让 AI Story Engine 能够持续生成：

- 连续的故事。
- 稳定的角色。
- 可控的剧情。
- 可扩展的 AI 叙事体验。

