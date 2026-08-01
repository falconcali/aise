# AI Story Engine 技术架构设计 v2.0

## 1. 概述

AI Story Engine 是一个基于 Turn 的互动叙事生成系统。

一次玩家输入触发一个完整故事回合：

    玩家输入
        ↓
    Turn 执行
        ↓
    故事生成
        ↓
    验证
        ↓
    状态提交
        ↓
    Turn Result

一个 Turn 内部包含多个 AI 推演阶段：

    上下文理解
        ↓
    信息召回
        ↓
    角色推演
        ↓
    故事生成
        ↓
    验证 / 修复
        ↓
    提交

系统采用模块化 Pipeline 架构，每个 Step 都由独立模块负责。

------------------------------------------------------------------------

# 2. 总体架构

    Story Client
          |
          v
    Story Turn API
          |
          v
    Turn Runtime
          |
          +-- 1. Turn Initializer
          |
          +-- 2. Baseline Context Builder
          |
          +-- 3. Writer Planner
          |
          +-- 4. Context Retrieval Pipeline
          |
          +-- 5. Character Think Pipeline
          |
          +-- 6. Story Generator
          |
          +-- 7. Validation Pipeline
          |
          +-- 8. Turn Committer
          |
          v
    Turn Result

Turn Runtime 负责一次 Turn 的流程编排。

每个 Step 负责单一业务能力。

------------------------------------------------------------------------

# 3. Turn Runtime

## 职责

Turn Runtime 是一次故事回合的 Workflow Orchestrator。

负责：

-   按顺序执行各个 Step。
-   管理 TurnExecutionContext 生命周期。
-   控制 Optional Pipeline。
-   控制 Validation / Repair Loop。
-   管理执行预算。
-   触发最终提交。

不负责：

-   决定剧情内容。
-   生成故事文本。
-   修改世界状态。

------------------------------------------------------------------------

## 调用流程

``` rust
async fn execute_turn(request: TurnRequest) -> TurnResult {

    let mut ctx = TurnExecutionContext::new(request);

    turn_initializer.init_turn(&mut ctx).await;

    baseline_context_builder
        .build(&mut ctx)
        .await;

    writer_planner
        .plan(&mut ctx)
        .await;


    if ctx.plan.need_retrieval {
        context_retrieval_pipeline
            .execute(&mut ctx)
            .await;
    }


    if ctx.plan.need_character_thinking {
        character_think_pipeline
            .execute(&mut ctx)
            .await;
    }


    story_generator
        .generate_story(&mut ctx)
        .await;


    loop {

        let validation =
            validation_pipeline
                .validate(&ctx)
                .await;


        if validation.pass {
            break;
        }


        story_generator
            .repair_story(
                &mut ctx,
                validation
            )
            .await;
    }


    turn_committer
        .commit(&ctx)
        .await
}
```

------------------------------------------------------------------------

# 4. 核心数据结构

## TurnExecutionContext

TurnExecutionContext 是一次 Turn 执行期间的核心上下文对象。

生命周期：

    Turn 开始创建
        ↓
    各 Step 持续修改
        ↓
    最终提交

示例：

``` rust
struct TurnExecutionContext {

    request: TurnRequest,

    snapshot: StorySnapshot,

    story_context: StoryContext,

    plan: Option<WriterPlan>,

    retrieved_context: Vec<ContextItem>,

    character_thoughts: Vec<CharacterThought>,

    draft: Option<StoryDraft>,

    validation: Option<ValidationResult>,

    budget: TurnBudget,

    trace: ExecutionTrace,
}
```

该对象只存在于当前 Turn 生命周期，不直接持久化。

------------------------------------------------------------------------

# 5. Step 1：Turn Initializer

## 职责

初始化一次 Turn 执行环境。

负责：

-   校验 TurnRequest。
-   加载 StorySnapshot。
-   检查 Session Version。
-   初始化预算。
-   创建 Trace。

接口：

``` rust
trait TurnInitializer {

    async fn init_turn(
        &self,
        ctx: &mut TurnExecutionContext
    );

}
```

------------------------------------------------------------------------

# 6. Step 2：Baseline Context Builder

## 职责

构建初始 Story Context。

关系：

    Story Snapshot

            ↓

    Baseline Context Builder

            ↓

    Baseline Story Context

Snapshot 表示世界状态。

Context 表示提供给 AI 的认知信息。

该阶段不调用 LLM。

接口：

``` rust
trait BaselineContextBuilder {

    async fn build(
        &self,
        ctx: &mut TurnExecutionContext
    );

}
```

基础 Context 包含：

-   Story Instructions
-   Story Configuration
-   Player Character
-   Current Scene
-   Relevant Characters
-   Recent Story
-   Story Summary
-   Active Constraints
-   Player Input

------------------------------------------------------------------------

# 7. Step 3：Writer Planner

## 职责

分析当前情况并决定故事生成所需信息。

这是第一次 LLM 调用。

负责：

-   理解玩家输入。
-   判断上下文缺口。
-   决定需要召回的信息。
-   决定需要推演的角色。
-   定义故事生成目标。

接口：

``` rust
trait WriterPlanner {

    async fn plan(
        &self,
        ctx: &mut TurnExecutionContext
    );

}
```

输出：

``` rust
struct WriterPlan {

    retrieval_requests:
        Vec<ContextRequest>,

    character_requests:
        Vec<CharacterId>,

    story_intent:
        StoryIntent,

    risk_level:
        RiskLevel,
}
```

------------------------------------------------------------------------

# 8. Step 4：Context Retrieval Pipeline

## 职责

根据 Writer Planner 的请求补充上下文。

这是一个可选步骤。

接口：

``` rust
trait ContextRetrievalPipeline {

    async fn execute(
        &self,
        ctx: &mut TurnExecutionContext
    );

}
```

内部组件：

    Character Retriever

    World Retriever

    Memory Retriever

    History Retriever

    Constraint Retriever

    Context Merger

支持：

-   Entity 查询。
-   Keyword 查询。
-   Tag 查询。
-   Vector Retrieval（未来）。

------------------------------------------------------------------------

# 9. Step 5：Character Think Pipeline

## 职责

推演当前重要角色的认知和反应。

这是一个可选步骤。

输出是角色视角信息，不是故事事实。

接口：

``` rust
trait CharacterThinkPipeline {

    async fn execute(
        &self,
        ctx: &mut TurnExecutionContext
    );

}
```

流程：

    角色选择

    ↓

    角色上下文构建

    ↓

    NPC 推演

    ↓

    结果合并

数据：

``` rust
struct CharacterThought {

    character_id: CharacterId,

    perception: String,

    emotion: String,

    goal: String,

    possible_action: String,

    knowledge_boundary: Vec<String>,
}
```

------------------------------------------------------------------------

# 10. Step 6：Story Generator

## 职责

负责生成和修复 Story Draft。

接口：

``` rust
trait StoryGenerator {

    async fn generate_story(
        &self,
        ctx: &mut TurnExecutionContext
    );


    async fn repair_story(
        &self,
        ctx: &mut TurnExecutionContext,
        validation: ValidationResult
    );

}
```

------------------------------------------------------------------------

## Generate Story

输入：

    Story Context

    +

    Writer Plan

    +

    Character Thoughts

输出：

``` rust
struct StoryDraft {

    story_text: String,

    events: Vec<StoryEvent>,

    character_updates:
        Vec<CharacterPatch>,

    world_updates:
        Vec<WorldPatch>,

    memory_updates:
        Vec<MemoryPatch>,

    scene_update:
        Option<ScenePatch>,
}
```

------------------------------------------------------------------------

## Repair Story

当 Validation 失败：

    Story Draft

    ↓

    Validation Issues

    ↓

    repair_story()

    ↓

    新的 Story Draft

修复属于 Story Generator 的职责。

------------------------------------------------------------------------

# 11. Step 7：Validation Pipeline

## 职责

判断 Story Draft 是否可以提交。

接口：

``` rust
trait ValidationPipeline {

    async fn validate(
        &self,
        ctx: &TurnExecutionContext
    ) -> ValidationResult;

}
```

内部：

    Deterministic Validator

    +

    Story Critic

Deterministic Validator：

-   Schema 检查。
-   ID 合法性。
-   状态修改合法性。
-   约束检查。
-   权限检查。

Story Critic：

-   角色一致性。
-   剧情合理性。
-   知识泄露。
-   叙事质量。
-   玩家控制边界。

------------------------------------------------------------------------

# 12. Step 8：Turn Committer

## 职责

提交最终故事结果。

接口：

``` rust
trait TurnCommitter {

    async fn commit(
        &self,
        ctx: &TurnExecutionContext
    ) -> TurnResult;

}
```

提交：

-   Story Turn。
-   Story Event。
-   Character State。
-   World State。
-   Character Memory。
-   Scene State。
-   Story Constraint。
-   Summary。

提交必须保证原子性。

------------------------------------------------------------------------

# 13. 模块结构

    ai_story_engine/

    runtime/
        turn_runtime.rs
        turn_initializer.rs
        turn_execution_context.rs

    context/
        baseline_context_builder.rs
        retrieval_pipeline.rs

    planning/
        writer_planner.rs

    character/
        character_think_pipeline.rs

    generation/
        story_generator.rs

    validation/
        validation_pipeline.rs

    persistence/
        turn_committer.rs

    domain/
        story.rs
        character.rs
        world.rs
        memory.rs

------------------------------------------------------------------------

# 14. 架构总结

AI Story Engine 是一个模块化 Turn Pipeline。

职责划分：

    Turn Runtime
        负责流程编排。

    Turn Initializer
        初始化执行环境。

    Baseline Context Builder
        构建基础上下文。

    Writer Planner
        分析需求并制定生成计划。

    Context Retrieval Pipeline
        提供额外知识。

    Character Think Pipeline
        提供角色推演。

    Story Generator
        生成和修复故事。

    Validation Pipeline
        保证故事质量和一致性。

    Turn Committer
        保存最终故事状态。

最终目标：

> 通过模块化、可观测、可扩展的 Turn Pipeline，让 AI Story Engine
> 能够稳定地产生连续、可控、一致的互动故事。
