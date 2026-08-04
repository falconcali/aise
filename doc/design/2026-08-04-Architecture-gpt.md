# AISE（AI Story Engine）技术架构设计 v3.0

## 1. 文档定位

本文是 AISE 最终架构的权威文档，定义目标架构、模块边界和运行时不变量。

本文关注：

- Turn 的生命周期与固定工作流。
- 同一 Story 的并发一致性。
- Pipeline 的输入、输出和失败语义。
- 上下文、预算和共享状态的边界。
- 生成结果成为权威状态前的验证过程。
- Turn 提交的事务、幂等和恢复语义。
- 模块依赖方向和横切能力的所有权。

具体模型提示词、数据库表结构、算法和供应商 SDK 不属于本文范围。

---

## 2. 核心架构决策

AISE 是一个基于 Turn 的互动叙事生成系统。

系统采用固定工作流。每个业务阶段实现统一的 `TurnExecutionPipeline` 接口，由 `TurnRuntime` 统一编排。Pipeline 之间不得直接调用，也不得自行改变工作流拓扑。

架构必须满足以下不变量：

1. 同一个 Story 的 Turn 严格串行执行，不同 Story 可以并行执行。
2. 一个 Turn 只能由一个 `TurnRuntime` 拥有，从创建到结束不得转移或跨 Turn 共享。
3. 所有 Pipeline 只通过当前 Turn 的 `TurnExecutionContext` 交换数据。
4. Pipeline 失败后立即把可诊断错误返回给 `TurnRuntime`，不得静默失败。
5. Validation / Repair 循环必须受预算限制，预算耗尽时 Turn 失败。
6. LLM 只能提出故事和状态变更建议，不能直接修改权威状态。
7. 只有通过确定性校验和叙事校验的变更集可以提交。
8. Turn 提交必须具备原子性、幂等性、一致性和崩溃恢复能力。
9. Turn 成功结果只能在提交成功后对外发布。
10. 所有集合、上下文、队列、并发、重试和外部调用都必须有明确上限。

## 3. 总体架构

```text
Story Client
      |
      v
Story Turn API
      |
      v
Session::lock_turn
  Story-level Serialization
      |
      v
AiseEngine::run_turn
      |
      v
TurnRuntime::run
      |
      +-- TurnInitializer
      +-- BaselineContextBuilder
      +-- WriterPlanner
      +-- ContextRetrievalPipeline
      +-- CharacterThinkPipeline
      +-- StoryGenerator
      +-- ValidationPipeline
      +-- StoryRepairer
      +-- TurnCommitter
      |
      v
Committed Turn Result
      |
      v
TurnEventSink
```

`Session::lock_turn` 负责 Story 级 Turn 串行化。`AiseEngine` 管理一次 Turn 的入口和结果发布，`TurnRuntime` 管理固定工作流和 Turn 生命周期。

横切能力由应用组合根创建并注入：

- Store / Repository。
- LLM Provider。
- 全局 LLM 并发限制器。
- 配置与预算策略。
- Trace、Metrics 和 Event Sink。
- Clock、ID Generator 等运行时能力。

Pipeline 不得自行创建这些长生命周期服务。

---

## 4. Turn 串行化与入口控制

### 4.1 Story 级串行化

同一个 `StoryId` 在任意时刻最多只能有一个正在执行的 Turn。串行化范围覆盖完整生命周期，而不只是数据库提交：

```text
Session::lock_turn
    -> Context Load
    -> Planning
    -> Generation
    -> Validation / Repair
    -> Commit
    -> Result Publication
    -> Release
```

这样可以保证后一个 Turn 构建上下文时一定能够观察到前一个 Turn 已提交的状态。

不同 `StoryId` 之间允许并行，但仍受全局请求上限和共享 LLM 并发限制器约束。

### 4.2 所有入口统一执行

Story 串行化是应用级不变量。HTTP、CLI、任务消费者、测试入口和未来新增的传输层在调用 `AiseEngine::run_turn` 前，都必须先获得对应 Story 所属 Session 的 Turn 执行权。

单进程部署由 `Session::lock_turn` 提供异步互斥。Session 与 Story 保持一对一关系，Session 数量和每个 Session 的等待请求必须有容量上限。

多实例部署时，`Session::lock_turn` 背后的实现必须扩展为能够覆盖所有实例的协调机制，并提供 fencing token、租约或等价的所有权校验。无论采用何种协调机制，提交阶段仍必须校验故事版本。

### 4.3 版本与幂等

Turn 开始时记录 `base_revision`。提交时必须满足：

```text
stored_revision == base_revision
```

提交成功后原子地推进 Story revision。版本校验是串行化之外的最终一致性防线，用于发现锁失效、进程切换和非标准入口造成的冲突。

客户端或入口层应提供稳定的幂等键。相同幂等键不得创建或应用两个 Turn。

---

## 5. 固定 Turn 工作流

工作流拓扑固定如下：

```text
Turn Initializer
      |
      v
Baseline Context Builder
      |
      v
Writer Planner
      |
      +-- Context Retrieval Pipeline    when requested
      |
      +-- Character Think Pipeline      when requested
      |
      v
Story Generator
      |
      v
Validation Pipeline
      |
      +-- pass ------------------------------+
      |                                      |
      +-- repairable and budget available    |
      |         |                            |
      |         v                            |
      |    Story Repairer                    |
      |         |                            |
      |         +----> Validation Pipeline   |
      |                                      |
      +-- non-repairable or exhausted -> Fail
                                             |
                                             v
                                      Turn Committer
                                             |
                                             v
                                      Turn Result
```

Runtime 根据 `WriterPlan` 跳过可选阶段，但不能重排阶段，也不支持运行时任意组合 Pipeline。条件判断由 Runtime 负责，已跳过阶段不得产生 `StageStarted` 事件。

若未来需要另一种工作流，应定义新的带版本固定工作流，并在应用启动时验证完整性。不得通过任意 Pipeline 插件拼接改变当前工作流的不变量。

---

## 6. Pipeline 契约与失败语义

Pipeline 接口：

```rust
#[async_trait]
pub trait TurnExecutionPipeline: Send + Sync {
    fn stage(&self) -> &'static str;

    async fn execute(
        &self,
        ctx: &mut TurnExecutionContext,
    ) -> Result<(), AiseError>;
}
```

`stage()` 返回稳定、低基数的阶段标识，用于事件、日志、指标和 Trace。

`execute()` 的语义：

- `Ok(())` 表示阶段后置条件已经满足，Runtime 可以进入下一阶段。
- `Err(AiseError)` 表示当前执行路径不能继续，Runtime 立即停止后续 Pipeline。
- 错误必须包含足够的阶段和原因信息，不得只返回布尔值或静默降级。
- Pipeline 不得吞掉错误，不得在内部进行无界重试。

Runtime 使用 fail-fast 协议：任何 Pipeline 返回 `Err`，本 Turn 执行立即以错误结束且不会执行后续 Committer。若引入有限重试或允许降级，重试与降级策略仍由 Runtime 或明确的策略对象控制，不能隐藏在 Pipeline 内部。

错误模型必须能够区分：

- 请求或领域数据错误。
- Story 不存在。
- LLM 或工具调用错误。
- Store / I/O 错误。
- Validation 不可修复。
- Validation 预算耗尽。
- Turn 超时或取消。
- Story revision 冲突。
- 幂等冲突。
- 内部不变量破坏。

Turn 终态只有：

```text
Committed
Failed
Cancelled
Conflict
```

只有 `Committed` 对外表示成功。

---

## 7. Turn Runtime

`TurnRuntime` 是 Turn Workflow Orchestrator，负责：

- 执行固定工作流。
- 检查阶段前置条件和后置条件。
- 控制可选 Pipeline。
- 管理 Validation / Repair 循环。
- 统一执行预算、deadline 和取消信号。
- 将阶段开始写入事件，并将阶段成功和失败写入 Trace。
- 在错误发生时停止工作流并返回诊断信息。

`TurnRuntime` 不负责：

- 生成故事内容。
- 决定剧情方向。
- 实现存储细节。
- 绕过 Committer 修改持久化状态。
- 自行持有跨 Turn 的可变故事状态。

概念调用流程：

```rust
async fn execute_pipeline(
    pipeline: &dyn TurnExecutionPipeline,
    ctx: &mut TurnExecutionContext,
    sink: &dyn TurnEventSink,
) -> Result<(), AiseError> {
    let stage = pipeline.stage();
    sink.emit(TurnEvent::StageStarted(stage));
    let pending = ctx.trace.begin_span("aise.pipeline", stage);
    let outcome = pipeline.execute(ctx).await;

    let payload = match &outcome {
        Ok(()) => SpanPayload::Pipeline(PipelineData {
            stage: stage.to_owned(),
            status: "ok".into(),
            error: None,
        }),
        Err(error) => SpanPayload::Pipeline(PipelineData {
            stage: stage.to_owned(),
            status: "error".into(),
            error: Some(error.to_string()),
        }),
    };
    ctx.trace.end_span_with(pending, &payload);

    match outcome {
        Ok(()) => Ok(()),
        Err(error) => Err(error),
    }
}

async fn execute_turn(
    request: TurnRequest,
    sink: &dyn TurnEventSink,
) -> Result<TurnResult, AiseError> {
    let mut ctx = TurnExecutionContext::new(request)?;

    execute_pipeline(&turn_initializer, &mut ctx, sink).await?;
    execute_pipeline(&baseline_ctx_builder, &mut ctx, sink).await?;
    execute_pipeline(&writer_planner, &mut ctx, sink).await?;

    if ctx.plan_requires_retrieval()? {
        execute_pipeline(&context_retrieval_pipeline, &mut ctx, sink).await?;
    }

    if ctx.plan_requires_character_thinking()? {
        execute_pipeline(&character_think_pipeline, &mut ctx, sink).await?;
    }

    execute_pipeline(&story_generator, &mut ctx, sink).await?;

    loop {
        execute_pipeline(&validation_pipeline, &mut ctx, sink).await?;

        match ctx.validation_decision()? {
            ValidationDecision::Pass => break,
            ValidationDecision::Repair => {
                ctx.budget.consume_repair_round()?;
                execute_pipeline(&story_repairer, &mut ctx, sink).await?;
            }
            ValidationDecision::Reject => {
                return Err(ctx.validation_error()?);
            }
        }
    }

    execute_pipeline(&turn_committer, &mut ctx, sink).await?;
    ctx.committed_result()
}
```

`execute_pipeline` 统一处理 Pipeline 返回值：成功时记录成功状态，失败时记录结构化错误并把原错误返回给 `execute_turn`。`execute_turn` 使用 `?` 立即终止当前 Turn，因此失败阶段之后的 Pipeline，包括 `TurnCommitter`，都不会继续执行。该代码用于表达控制语义，不要求实现逐字一致。

---

## 8. TurnExecutionContext

### 8.1 所有权与生命周期

`TurnExecutionContext` 由一个 Runtime 独占，只存在于当前 Turn：

```text
Construct
    -> Initialize
    -> Prepare
    -> Plan
    -> Generate
    -> Validate / Repair
    -> Commit or Fail
    -> Destroy
```

Context 不直接持久化，不跨 Turn 缓存，也不得放入全局共享状态。

Context 保存 Turn 数据和执行状态，不保存数据库连接、LLM Provider、并发限制器或后台任务等长生命周期服务。这些依赖由 Pipeline 自身通过组合根注入。

### 8.2 受控共享状态

Context 是统一的数据交换载体，但不能成为任意读写的 God Object。

架构要求：

- 字段保持私有，通过表达业务语义的方法访问。
- 每个 Pipeline 明确声明可读数据、可写数据、前置条件和后置条件。
- Pipeline 只能更新自己拥有的输出区域。
- Runtime 在阶段切换时检查不变量。
- `Option` 不能作为唯一的阶段正确性保障。
- Context 内的所有集合都有数量或字节上限。

Context 的概念结构为：

```rust
struct TurnExecutionContext {
    identity: TurnIdentity,
    phase: TurnPhase,
    request: TurnRequest,
    snapshot: StorySnapshot,
    baseline: BaselineContext,
    plan: Option<WriterPlan>,
    retrieved: BoundedContextItems,
    thoughts: BoundedCharacterThoughts,
    proposal: Option<StoryDraft>,
    validation: ValidationResult,
    change_set: Option<ValidatedChangeSet>,
    budget: TurnBudget,
    trace: TraceRecorder,
}
```

`StorySnapshot` 包含 `base_revision`，表示本 Turn 读取到的权威状态版本。

### 8.3 阶段契约

| 阶段 | 必要输入 | 唯一业务输出 |
| --- | --- | --- |
| Initializer | request | identity、运行参数、初始预算 |
| Baseline Builder | identity、request | snapshot、baseline |
| Writer Planner | baseline、request | plan |
| Retrieval | plan、baseline | retrieved context items |
| Character Think | plan、snapshot、retrieved | character thoughts |
| Story Generator | baseline、plan、retrieved、thoughts | story proposal |
| Validation | snapshot、proposal | validation result、validated change set |
| Story Repairer | proposal、validation issues | revised proposal |
| Turn Committer | snapshot、validated change set | committed revision/result |

Initializer 不再初始化一个已经被 Runtime 半构造的 Context。`TurnExecutionContext::new` 必须生成满足基础不变量的对象；Initializer 只准备需要外部运行时能力才能确定的数据。

---

## 9. 预算、超时与取消

`TurnBudget` 是整个 Turn 的统一资源预算，至少覆盖：

- 最大修复轮数。
- 最大 LLM 调用次数。
- 最大生成 token 数和总 token 数。
- 最大检索条目数。
- 最大参与思考的角色数。
- 最大上下文字节数或 token 数。
- Turn 总 deadline。
- 单次外部调用 timeout。

预算由 Runtime 拥有并由各阶段显式消费。预算不得在 Pipeline 内复制成互不一致的计数器。

预算耗尽必须返回可诊断错误。不得继续生成、无限修复或提交一个未通过验证的草稿。

用户取消、服务关闭或 deadline 到期时，Runtime 必须停止启动新的外部调用。已经启动的后台工作必须由明确的所有者取消或等待结束，不得形成孤儿任务。

---

## 10. Context Preparation 与 Retrieval

### 10.1 基础上下文

Baseline Context 包含：

- Story Instructions。
- Story Configuration。
- Player Character。
- Current Scene。
- Relevant Characters。
- Recent Story。
- Story Summary。
- Active Constraints。
- Player Input。

Baseline Builder 必须从同一个带 revision 的 `StorySnapshot` 构建上下文，不负责剧情生成，也不能更新持久化状态。

### 10.2 上下文分类

所有提供给模型的信息必须保留来源和语义类别，至少区分：

```text
Canonical World Fact
Character Belief
Character Memory
Narrative History
Narrative Summary
Retrieved Lore
Planner Hypothesis
Character Thought
```

Character Thought、Character Belief、Planner Hypothesis 和 Retrieved Lore 都不能自动升级为世界事实。

一个 Context Item 应具有以下概念元数据：

```text
source
scope
story_revision
timestamp
authority
visibility
relevance_score
token_cost
```

### 10.3 合并策略

Context Merger 必须使用确定性策略完成：

- 权威级别排序。
- 去重。
- 冲突保留和标记。
- 角色可见性过滤。
- token 预算裁剪。
- 稳定排序。

较新的 Canonical Fact 优先于摘要和角色记忆。角色记忆与世界事实冲突时应保留为角色的主观认知，不得覆盖世界事实。

检索数量、单条大小和最终上下文大小必须受 `TurnBudget` 限制。

---

## 11. Writer Planner 与 Character Think

Writer Planner 分析当前 Turn 并输出生成计划，包括：

- 故事目标。
- 上下文缺口。
- Retrieval 请求。
- 需要进行认知推演的角色集合。

是否执行可选阶段应从请求集合推导，避免布尔值和请求内容互相矛盾。例如，Retrieval 请求为空即表示跳过 Retrieval。

Character Think Pipeline 只模拟关键角色在当前可见信息下的：

- 感知。
- 情绪。
- 目标。
- 行动倾向。

Character Thought 是临时推理产物，只存在于当前 Turn。它不是权威世界事实，不得由 Generator 或 Committer 直接写入 World State。

角色数量和每个角色的 Thought 大小必须有上限。角色之间需要并发推演时，也必须使用有界并发且共享全局 LLM 限制器。

---

## 12. Story Proposal 与领域状态边界

Story Generator 的输出是 `StoryDraft`，其语义是待验证的 Proposal，而不是可以直接提交的领域状态。

Proposal 可以包含：

- Story Text。
- Proposed Events。
- Proposed Character Changes。
- Proposed World Changes。
- Proposed Memory Changes。

任何由 LLM 产生的 Patch 都是不可信输入。它必须经过解析、Schema 校验、权限校验和领域不变量校验，才能转换成 `ValidatedChangeSet`。

```text
LLM Story Proposal
      |
      v
Schema and Permission Validation
      |
      v
Domain Command / Proposed Event
      |
      v
Deterministic Domain Rules
      |
      v
ValidatedChangeSet
```

只有 `ValidatedChangeSet` 可以交给 Turn Committer。Generator 和 Repairer 都不得获得 Store 的写能力。

---

## 13. Validation / Repair

Validation 分为两个信任等级：

### 13.1 Deterministic Validation

- Schema 合法性。
- ID 和引用完整性。
- 状态修改权限。
- Story revision 前置条件。
- Domain invariant。
- Knowledge Boundary。
- Player Control Boundary 中可确定判断的部分。

确定性校验是提交的硬门槛，LLM Validator 不能覆盖其结论。

### 13.2 Narrative Validation

- Character Consistency。
- Narrative Consistency。
- 风格和语气约束。
- 需要语义判断的 Knowledge Boundary。
- 需要语义判断的 Player Control Boundary。

Validation Issue 至少具有：

```text
code
message
severity
repairability
location
```

Validation 的决策为：

- `Pass`：生成 `ValidatedChangeSet`，允许进入 Commit。
- `Repair`：存在可修复问题且仍有修复预算。
- `Reject`：存在不可修复问题，立即失败。
- `BudgetExhausted`：修复预算耗尽，立即失败。

每次 Repair 后必须重新执行完整验证。修复轮数在调用 Repairer 之前消费，任何路径都不能绕过预算检查。

---

## 14. Turn Commit 与恢复

### 14.1 Turn Committer 定位

Turn Committer 是应用层的提交协调者。它依赖 Store / Unit of Work port，不包含具体数据库实现，也不接受未验证的 StoryDraft。

提交输入至少包含：

- `story_id`。
- `turn_id` 和幂等键。
- `base_revision`。
- Story Turn。
- Canonical Events。
- Validated Character / World / Memory Changes。
- 需要投递的 Outbox Records。

### 14.2 权威状态与派生状态

以下内容必须在同一数据库事务内原子提交：

- Turn Record。
- Canonical Story Events。
- 影响后续叙事决策的 Character / World / Memory 权威状态。
- Story revision。
- Outbox Records。

以下内容属于可重建的派生状态，不要求与外部系统进行分布式事务：

- Embedding。
- 向量索引。
- 搜索索引。
- 可重新生成的 Summary Projection。
- 分析和通知事件。

派生状态通过 transactional outbox 在事务提交后更新。消费者必须幂等，失败可以重试和重建。

### 14.3 提交保证

Turn Committer 必须保证：

- 原子性：权威变更全部成功或全部失败。
- 一致性：`base_revision` 与当前 revision 一致才能提交。
- 幂等性：同一个 `turn_id` 或幂等键重复提交不会重复应用变更。
- 可恢复性：进程在提交前、中、后崩溃时都能判断 Turn 是否已经提交。
- 可诊断性：冲突、约束失败和存储错误具有不同错误类型。

数据库事务提交成功是不可逆边界。事务成功但响应丢失时，客户端重试必须查询并返回原 Turn 结果，不能生成新 Turn。

---

## 15. 对外结果与事件

草稿、Validation 中间结果和未提交的 token 都不是最终事实。

默认协议为：

```text
Commit Success
    -> Publish Finished Event
    -> Return Turn Result
    -> Release Story Execution Ownership
```

对外成功结果必须包含稳定的 `turn_id` 和已提交的 `story_revision`。

若未来支持提交前流式预览，事件必须明确标记为 provisional，并在失败时发送撤销/失败终态；客户端不得把 provisional 内容当作已提交历史。该能力不改变“只有提交后才成功”的语义。

---

## 16. 分层与依赖方向

逻辑依赖方向为：

```text
aise-server API / Session / Composition Root
                    |
                    v
              Engine / Runtime
                    |
          +---------+----------+
          |                    |
          v                    v
Context / Planning /      Store and LLM Ports
Character / Story /             ^
Validation                       |
          |                Concrete Adapters
          v
        Domain
```

规则：

- Domain 不依赖 Runtime、API、LLM 或 Persistence adapter。
- Pipeline 模块可以依赖 Domain 和抽象 port，不依赖 API concrete type。
- Runtime 负责编排，不依赖具体数据库或供应商 SDK。
- Persistence adapter 实现 Store port；具体实现只在组合根装配。
- LLM adapter 实现 LLM Provider port；所有调用经过同一个注入的限制器。
- 反向通知通过注入的 trait 完成，内层模块不得导入外层具体类型。
- Pipeline 之间不得直接依赖和调用。

`TurnCommitter` 虽位于 `persistence` 目录，但其角色是提交协调者；数据库连接、SQL 和事务实现属于 Store adapter。

---

## 17. 模块目录结构

模块目录采用以下结构：

```text
crates/
├── aise/
│   └── src/
│       ├── engine.rs
│       ├── config.rs
│       ├── error.rs
│       ├── runtime/
│       │   ├── turn_runtime.rs
│       │   ├── turn_execution_ctx.rs
│       │   ├── turn_budget.rs
│       │   ├── pipeline.rs
│       │   ├── initializer.rs
│       │   ├── event.rs
│       │   └── trace/
│       ├── context/
│       ├── planning/
│       ├── character/
│       ├── story/
│       ├── validation/
│       │   └── validators/
│       ├── persistence/
│       │   ├── store.rs
│       │   ├── sqlite_store.rs
│       │   └── turn_committer.rs
│       ├── llm/
│       │   ├── provider.rs
│       │   ├── limiter.rs
│       │   └── openai_compat.rs
│       └── domain/
└── aise-server/
    └── src/
        ├── app.rs
        ├── config.rs
        ├── api/
        │   ├── turn.rs
        │   ├── session.rs
        │   └── state.rs
        └── session/
            ├── model.rs
            └── registry.rs
```

目录只表示职责归属，真正的架构边界以依赖规则和 trait port 为准。

---

## 18. LLM 调用与并发

`aise-server::app::build_engine` 是 LLM 依赖的组合根，负责创建共享 `LlmLimiter` 并注入 `LlmProvider`。所有 completion、streaming 和未来的 embedding 调用都必须经过该共享限制能力，不得创建绕过限制器的 Provider 或调用点。

LLM 调用必须同时受 Turn budget、deadline、单次 timeout 和取消信号约束。并发 permit 的等待也必须受 deadline 限制，并在调用结束或取消时释放。

---

## 19. 可观测性与运行保障

一次 Turn 必须具有贯穿所有阶段的 `story_id`、`turn_id` 和 Trace。

每个 Pipeline、LLM 调用、工具调用、Validation 和持久化操作必须位于独立 span 中，并使用结构化字段记录：

- stage。
- story_id。
- turn_id。
- model / provider。
- attempt。
- token usage。
- latency。
- validation issue code。
- error type。
- base_revision / committed_revision。

不得把未经裁剪的故事全文、角色隐私信息或模型密钥写入日志。

关键指标至少包括：

- Turn 成功率与各终态数量。
- 各阶段延迟。
- 排队时间和每 Story 队列深度。
- LLM 并发、token 和失败率。
- Repair 次数及预算耗尽率。
- revision 冲突数。
- Commit 延迟和恢复次数。
- Outbox backlog。

---

## 20. 容量与生命周期约束

以下资源必须具有配置化上限和清理策略：

| 资源 | 必要约束 |
| --- | --- |
| Story Turn 等待队列 | 每 Story 和全局容量、超时、拒绝策略 |
| Story 锁或协调记录 | 空闲回收、租约或关闭路径 |
| Recent Story / Summary | 条数或 token 上限、压缩策略 |
| Retrieved Context | 条数、单项大小、总 token 上限 |
| Character Thoughts | 角色数和单项大小上限 |
| Validation Issues | 数量和消息大小上限 |
| Repair History | 最大轮数，不保留无界草稿副本 |
| Trace / Events | 数量、内容大小、采样和保留策略 |
| LLM 并发 | 全局共享限制器和等待超时 |
| Outbox | 重试、死信、告警和保留策略 |

任何后台任务、队列和 channel 都必须有单一所有者、背压方案和 shutdown 路径。

---

## 21. 架构验收条件

满足以下条件后，Turn 架构才视为完整：

1. 并发请求无法让同一 Story 同时执行两个 Turn。
2. 不同 Story 可以在全局并发预算内并行。
3. 任一 Pipeline 失败都会停止后续阶段且不会提交。
4. Repair 次数、上下文大小和所有 LLM 调用均受预算约束。
5. Pipeline 无法越权修改其他阶段拥有的数据。
6. Character Thought 无法作为 World Fact 直接提交。
7. 未通过确定性验证的 Proposal 无法生成 `ValidatedChangeSet`。
8. 重复提交同一个 Turn 不会重复应用状态。
9. revision 不匹配时提交失败而不是覆盖新状态。
10. Commit 成功但响应丢失后可以通过幂等键恢复原结果。
11. 外部派生系统失败不会破坏已经提交的权威状态。
12. API、Runtime、Domain 和 adapter 之间不存在反向依赖。

---

## 22. 架构总结

AISE 是一个固定 Pipeline 工作流驱动的 Turn-based Narrative Engine：

```text
Story-level Serialization
          |
          v
Bounded Turn Runtime
          |
          v
Controlled TurnExecutionContext
          |
          v
Story Proposal
          |
          v
Deterministic and Narrative Validation
          |
          v
Atomic Versioned Commit
          |
          v
Committed Turn Result
```

架构的核心不是任意组合 Pipeline，而是在固定且可验证的工作流中，使每个阶段保持单一职责，并通过严格的状态、预算、并发和事务边界保证故事连续性、角色稳定性和系统可恢复性。
