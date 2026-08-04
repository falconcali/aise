# AISE（AI Story Engine）技术架构设计 v3.1

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

## 1.1 v3.1 修订说明

本版本相对 v3.0 的修订（与 `2026-08004-Turn-Runtime-Codegen-Spec-gpt.md` 同步，适用基线 `main@c14f84e`）：

1. Story 串行化由 `AiseEngine` 内部的 `StoryTurnCoordinator` 强制，不再依赖 `Session::lock_turn`。
2. Session 是临时连接资源，Story 是持久化领域对象，两者不构成一对一架构不变量。
3. 新增 `core` Turn Contracts 层，Runtime、Engine、LLM Gateway 和所有 Pipeline 单向依赖它。
4. `TurnExecutionContext` 创建时必须已有有效的 Identity、Request、Budget、Deadline、Cancellation 和 Trace；不得存在空 ID 或半初始化对象。
5. `TurnInitializer` 只负责 Turn 内部对象准备和状态初始化，不加载 Story、World、Character、Memory 或历史；请求规范化由 `TurnRequest::try_new` 在 Context 构造前完成。
6. `StoryDraft` 改为不可信的 `StoryProposal`；只有 `ValidatedChangeSet` 可以进入 Commit。
7. 状态表是当前权威状态，Canonical Events 是不可变审计记录；本阶段不实现完整 Event Sourcing。
8. 所有 LLM 横切事务统一由 `LlmGateway` 处理。

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
AiseEngine::run_turn
      |   (内部 StoryTurnCoordinator 强制 Story 级串行化)
      v
TurnRuntime::run
      |
      +-- TurnInitializer
      +-- BaselineContextBuilder
      +-- WriterPlanner
      +-- ContextRetrievalPipeline    when requested
      +-- CharacterThinkPipeline      when requested
      +-- StoryGenerator
      +-- ValidationPipeline / StoryRepairer   (bounded repair loop)
      +-- TurnCommitter
      |
      v
Committed Turn Result
      |
      v
TurnEventSink
```

`StoryTurnCoordinator` 注入 `AiseEngine`，由 `AiseEngine::run_turn` 内部调用，负责 Story 级 Turn 串行化。`AiseEngine::run_turn` 管理一次 Turn 的入口、串行化和结果发布，`TurnRuntime` 管理固定工作流和 Turn 生命周期。所有 HTTP、CLI、测试和任务消费者都无法绕过 Coordinator。

横切能力由应用组合根创建并注入：

- Store / Repository。
- `LlmGateway`（统一限流、排队、deadline、取消、token usage、计费和 tracing）。
- `StoryTurnCoordinator`（每 Story 一个容量为 1 的执行 permit）。
- 配置与预算策略。
- Trace、Metrics 和 Event Sink。
- Clock、ID Generator 等运行时能力。

新增 `core` Turn Contracts 层：Runtime、Engine、LLM Gateway 和所有 Pipeline 单向依赖 `core`，`core` 不得反向依赖任何业务模块或外层类型。

Pipeline 不得自行创建这些长生命周期服务。

---

## 4. Turn 串行化与入口控制

### 4.1 Story 级串行化

同一个 `StoryId` 在任意时刻最多只能有一个正在执行的 Turn。串行化范围覆盖完整生命周期，而不只是数据库提交：

```text
StoryTurnCoordinator::acquire
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

Story 串行化是应用级不变量。`StoryTurnCoordinator` 注入 `AiseEngine`，`AiseEngine::run_turn` 内部完成 Story permit 获取与释放。HTTP、CLI、任务消费者、测试入口和未来新增的传输层调用 `AiseEngine::run_turn` 时都无法绕过串行化。

单进程实现要求：

- 每个 `StoryId` 一个容量为 1 的执行 permit。
- Permit 等待队列有全局和 per-Story 上限。
- 等待受 Turn deadline 和 cancellation 约束。
- Map 锁只用于同步获取或更新 Entry，不得跨 `.await` 持有。
- Turn 生命周期持有的是 owned permit，不是 `MutexGuard` 或 `RwLockGuard`。
- Entry 在无 owner、无 waiter 且空闲超过配置时间后回收。
- Shutdown 时拒绝新请求，取消等待者，并等待已拥有 Turn 在宽限期内结束。

Session 是临时连接资源，Story 是持久化领域对象，两者不构成一对一架构不变量。`Session` 不再拥有执行锁，`Session::turn_lock` / `Session::lock_turn` 已删除。

多实例部署时，`StoryTurnCoordinator` 背后的实现必须扩展为能够覆盖所有实例的协调机制，并提供 fencing token、租约或等价的所有权校验。无论采用何种协调机制，提交阶段仍必须校验故事版本。

### 4.3 版本与幂等

Turn 开始时记录 `base_revision`。提交时必须满足：

```text
stored_revision == base_revision
```

提交成功后原子地推进 Story revision。版本校验是串行化之外的最终一致性防线，用于发现锁失效、进程切换和非标准入口造成的冲突。

客户端或入口层必须提供稳定的幂等键。幂等唯一键为 `(story_id, idempotency_key)`：

- 相同 key、相同规范化请求：返回原 `CommittedTurnResult`，不再次调用 LLM、不再次提交。
- 相同 key、不同请求摘要：返回 `IdempotencyConflict`。
- Commit 成功但响应丢失后，重试必须恢复原结果。
- `turn_id` 唯一约束不能替代 idempotency key。

请求规范化（长度检查、稳定 request digest）在 `TurnRequest::try_new` 中完成，先于 Context 构造。

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
      +-- Pass -----------------------------------+
      |                                           |
      +-- Repair (budget available)               |
      |         |                                 |
      |         v                                 |
      |    Story Repairer (循环内辅助 Pipeline)   |
      |         |                                 |
      |         +----> Validation Pipeline        |
      |                                           |
      +-- Reject or budget exhausted -> Fail      |
      |                                           |
      +-------------------------------------------+
                                                  |
                                                  v
                                           Turn Committer
                                                  |
                                                  v
                                           Committed Turn Result
```

工作流固定为八步：Initializer -> Baseline Builder -> Writer Planner -> [Retrieval] -> [Character Think] -> Story Generator -> Validation/Repair loop -> Committer。`StoryRepairer` 是 Validation 循环内的辅助 Pipeline，不是独立的第九个业务阶段。

Runtime 根据 `WriterPlan` 推导的请求集合决定是否执行可选阶段：`retrieval_requests.is_empty()` 表示跳过 Retrieval，`character_requests.is_empty()` 表示跳过 Character Think。不得保留 `need_retrieval`、`need_character_thinking` 之类可能与请求集合矛盾的布尔值。Runtime 不能重排阶段，也不支持运行时任意组合 Pipeline。条件判断由 Runtime 负责，已跳过阶段不得产生 `StageStarted` 事件，由 Runtime 通过 Context 的显式方法记录空结果并推进 Phase。

若未来需要另一种工作流，应定义新的带版本固定工作流，并在应用启动时验证完整性。不得通过任意 Pipeline 插件拼接改变当前工作流的不变量。

---

## 6. Pipeline 契约与失败语义

Pipeline 接口：

```rust
#[async_trait]
pub trait TurnExecutionPipeline: Send + Sync {
    fn stage(&self) -> TurnStage;

    async fn execute(
        &self,
        ctx: &mut TurnExecutionContext,
    ) -> Result<(), AiseError>;
}
```

`TurnStage` 是有穷枚举，`stage()` 返回稳定、低基数的阶段标识，用于事件、日志、指标和 Trace；禁止 Pipeline 自定义动态 Stage 名称。

`execute()` 的语义：

- `Ok(())` 表示该 Pipeline 自身执行成功并写入了合法阶段结果。Validation 返回 `Repair` 或 `Reject` 是合法业务结果，可以返回 `Ok(())`，但 Runtime 必须立即读取 Decision 并分支；它不等于整个 Turn 成功。
- `Err(AiseError)` 表示当前执行路径不能继续，Runtime 立即停止后续 Pipeline。
- 错误必须包含足够的阶段和原因信息，不得只返回布尔值或静默降级。
- Pipeline 不得吞掉错误，不得在内部进行无界重试。

Runtime 使用 fail-fast 协议：任何 Pipeline 返回 `Err`，本 Turn 执行立即以错误结束且不会执行后续 Committer。若引入有限重试或允许降级，重试与降级策略仍由 Runtime 或明确的策略对象控制，不能隐藏在 Pipeline 内部。

错误模型必须能够区分：

- InvalidRequest：请求或领域数据错误。
- StoryNotFound。
- Llm / Store / I/O 错误。
- ValidationRejected：Validation 不可修复。
- ValidationBudgetExhausted：Validation 预算耗尽。
- TurnDeadlineExceeded。
- Cancelled。
- RevisionConflict：Story revision 冲突。
- IdempotencyConflict。
- Backpressure。
- InvariantViolation：内部不变量破坏。

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

- 执行固定八步工作流（Initializer -> Baseline Builder -> Writer Planner -> [Retrieval] -> [Character Think] -> Story Generator -> Validation/Repair loop -> Committer）。
- 检查阶段前置条件和后置条件。
- 根据请求集合控制可选 Pipeline。
- 管理 Validation / Repair 循环，Repair 预算在调用 Repairer 之前消费。
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
execute(initializer, ctx).await?;
execute(baseline_builder, ctx).await?;
execute(writer_planner, ctx).await?;

if ctx.requires_retrieval()? {
    execute(retrieval, ctx).await?;
} else {
    ctx.skip_retrieval()?;
}

if ctx.requires_character_thinking()? {
    execute(character_think, ctx).await?;
} else {
    ctx.skip_character_thinking()?;
}

ctx.complete_context_preparation()?;
execute(story_generator, ctx).await?;

loop {
    execute(validation, ctx).await?;
    match ctx.validation_decision()? {
        ValidationDecision::Pass => break,
        ValidationDecision::Repair => {
            ctx.consume_repair_round()?;
            execute(story_repairer, ctx).await?;
        }
        ValidationDecision::Reject => return Err(ctx.validation_error()?),
    }
}

execute(committer, ctx).await?;
ctx.committed_result()
```

Repair 预算在调用 Repairer 之前消费。`max_repair_rounds = 0` 表示不允许 Repair，而不是无限次数。Runtime 在每个 Pipeline 前后检查 cancellation、deadline、预期 Phase 和后置 Phase；失败后不得启动下一个 Pipeline。该代码用于表达控制语义，不要求实现逐字一致。

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
    control: TurnControl,
    budget: TurnBudget,
    trace: TurnTraceRecorder,
    snapshot: Option<StoryReadSnapshot>,
    baseline: Option<BaselineContext>,
    plan: Option<WriterPlan>,
    retrieved: Vec<ContextItem>,
    thoughts: Vec<CharacterThought>,
    proposal: Option<StoryProposal>,
    proposal_revision: u32,
    validation: Option<ValidationResult>,
    change_set: Option<ValidatedChangeSet>,
    committed_result: Option<CommittedTurnResult>,
}
```

字段全部私有。`Option` 只表示阶段产物尚未产生；阶段正确性由私有字段、Phase 状态机和有语义的方法共同保证，不依赖 `Option::unwrap`。

Context 创建时即持有有效的 Identity、Request、Budget、Control（deadline + cancellation）和 Trace，满足 `TurnPhase::Created` 的全部不变量。`StoryReadSnapshot` 包含 `base_revision`，表示本 Turn 从 Store 原子读取的权威状态版本。

Engine 必须先创建以下有效对象，再构造 Context：

```rust
pub struct ExecuteTurnSpec {
    pub story_id: StoryId,
    pub idempotency_key: IdempotencyKey,
    pub player_input: String,
    pub cancellation: TurnCancellation,
}

pub struct TurnRequest {
    player_input: String,
    request_digest: RequestDigest,
}

pub struct TurnIdentity {
    story_id: StoryId,
    turn_id: TurnId,
    idempotency_key: IdempotencyKey,
    started_at_ms: i64,
}

pub struct TurnControl {
    deadline: Instant,
    cancellation: TurnCancellation,
}
```

`TurnExecutionContext::new` 的概念签名：

```rust
pub fn new(
    identity: TurnIdentity,
    request: TurnRequest,
    budget: TurnBudget,
    control: TurnControl,
    trace: TurnTraceRecorder,
) -> Result<Self, AiseError>;
```

构造规则：

- Engine 先通过 `TurnRequest::try_new` 完成输入规范化、长度检查和稳定 request digest，再创建 Context。
- `StoryId`、`TurnId`、`IdempotencyKey` 和玩家输入不得为空；ID newtype 不得再实现可产生空值的 `Default`。
- `TurnBudget` 必须来自 `TurnConfig`，不得在 Context 或 Pipeline 中再次使用另一份 Default。
- deadline 必须是绝对单调时钟时间点；Cancellation 必须能同时响应请求取消和服务 shutdown。
- `TurnId` 由 Engine 注入的 ID 依赖生成，Pipeline 不得直接调用 `Uuid::new_v4` 或 `SystemTime::now`。

必须提供具有业务语义的阶段化读写方法，不提供通用字段写接口：

```text
complete_initialization
set_prepared_context
set_writer_plan
set_retrieved_context
set_character_thoughts
set_story_proposal
set_validation_result
replace_story_proposal
set_committed_result
llm_call_scope
```

每个写方法必须：验证当前 Phase；验证集合、字符串和 token 上限；只更新该阶段拥有的输出；清除已经失效的下游数据；推进到唯一允许的下一 Phase；在违反不变量时返回 typed error，不得 panic。Repair 替换 Proposal 时必须清除旧 Validation 和旧 `ValidatedChangeSet`，并增加 `proposal_revision`。

### 8.3 阶段契约

| 阶段 | 只读输入 | 唯一业务输出 |
| --- | --- | --- |
| Initializer | Identity、已规范化 Request、Budget、Control | Turn 临时状态、`Initialized` Phase |
| Baseline Builder | Identity、Request、Budget | `StoryReadSnapshot`、`BaselineContext` |
| Writer Planner | Baseline、Request | `WriterPlan` |
| Retrieval | Plan、Baseline、Budget | `ContextItem[]` |
| Character Think | Plan、Snapshot、Retrieved、Budget | `CharacterThought[]` |
| Story Generator | Baseline、Plan、Retrieved、Thoughts | `StoryProposal` |
| Validation | Snapshot、Proposal | `ValidationResult`；仅 Pass 时产生 `ValidatedChangeSet` |
| Story Repairer | Proposal、Validation Issues | 新版本 `StoryProposal` |
| Turn Committer | Identity、Snapshot、`ValidatedChangeSet` | `CommittedTurnResult` |

`TurnInitializer` 的输入是已经有效的 Context，只负责初始化本 Turn 的临时槽位和执行状态，并将 Phase 从 `Created` 推进为 `Initialized`。它不得生成 `turn_id`、不得创建或覆盖 Budget/Deadline/Cancellation/Trace、不得调用 Store/LLM/Retriever/其他 Pipeline、不得加载 World/Character/Memory/History/Summary 或 Narrative Graph。请求规范化由 `TurnRequest::try_new` 在 Context 构造前完成。

`TurnPhase` 最低包含：`Created`、`Initialized`、`Prepared`、`Planned`、`ContextReady`、`ProposalReady`、`RepairRequired`、`ReadyToCommit`、`Committed`、`Failed`、`Cancelled`、`Conflict`。允许转换固定为：`Created -> Initialized -> Prepared -> Planned -> ContextReady -> ProposalReady`；Validation Pass 时 `ProposalReady -> ReadyToCommit -> Committed`；Validation Repair 时 `ProposalReady -> RepairRequired -> ProposalReady`；Reject 不得进入 `ReadyToCommit`；任意非终态可因失败、取消或冲突进入对应终态。

---

## 9. 预算、超时与取消

`TurnBudget` 是整个 Turn 的统一资源预算，分为 immutable limits 和 mutable usage，字段私有，至少覆盖：

- `max_repair_rounds`。
- `max_llm_calls`。
- `max_input_tokens`。
- `max_output_tokens`。
- `max_total_tokens`。
- `max_retrieved_items`。
- `max_context_tokens`。
- `max_character_thoughts`。
- `max_validation_issues`。
- `max_trace_spans`。

配置必须只有一个权威来源：`TurnConfig` 定义 limits，Engine 将其转换为当前 Turn 的 `TurnBudget`，Pipeline 不保存重复的 `max_tokens`、`max_repair_rounds` 或 `max_retrieved_items`。Story Generator 从 Gateway reservation 获得本次允许的最大输出 token，不从自己的字段读取另一份配置。

`TurnControl` 携带绝对单调时钟 deadline 和 `TurnCancellation`。Cancellation 必须同时响应请求取消和服务 shutdown，建议使用 `tokio_util::sync::CancellationToken`。

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

`StoryReadSnapshot` 是一次 Turn 从 Store 原子读取的不可变视图，最低包含：`story_id`、`base_revision`、story instructions/configuration、player character id、world state、current scene、relevant characters、bounded recent turns、story summary、active constraints、required memories。

规则：

- Snapshot 由一次一致性读事务获得。
- Baseline 只能从该 Snapshot 构建，不再分别读取互相可能错版的数据。
- Recent Turns 从 Store 返回给业务层时必须是时间正序。
- Player Character 通过稳定 ID 指定，不得取 SQL 无序结果的第一项。
- `WorldState` 不再内嵌 Character 权威副本；Character 表是 Character 当前状态的唯一权威来源。
- `WorldFact` 使用稳定 `FactId`，删除操作不得使用数组下标。

Baseline Builder 从 Snapshot 构建上下文，不负责剧情生成，也不能更新持久化状态。

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

Story Generator 的输出是 `StoryProposal`（替代 `StoryDraft`），其语义是不可信的模型 Proposal，而不是可以直接提交的领域状态。

Proposal 可以包含：

- Story Text。
- Proposed Event DTO。
- Proposed Character Change DTO。
- Proposed World Change DTO。
- Proposed Memory Change DTO。
- Proposed Summary Delta。

Proposal 不得包含：

- `EventId`、Canonical Event 或已授权 Command。
- 完整 `CharacterState`、`WorldState` 或其他可直接覆盖权威状态的对象。
- 数据库字段、revision 更新或 Outbox Record。

Generator 和 Repairer 不得注入 Store、Unit of Work、ID Generator 或 Clock。

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

确定性校验是提交的硬门槛，LLM Validator 不能覆盖其结论。确定性失败不得被 Narrative Validator 覆盖。

### 13.2 Narrative Validation

- Character Consistency。
- Narrative Consistency。
- 风格和语气约束。
- 需要语义判断的 Knowledge Boundary。
- 需要语义判断的 Player Control Boundary。

Narrative Validator 的所有 LLM 调用也必须经过 `LlmGateway`。

Validation Issue 至少具有：

```text
code
message
severity
repairability
location
```

`ValidationIssue` 数量和单条 message 长度必须受 Budget 限制。

Validation 的数据结构：

```rust
pub enum ValidationDecision {
    Pass,
    Repair,
    Reject,
}

pub struct ValidationResult {
    decision: ValidationDecision,
    issues: Vec<ValidationIssue>,
}
```

Validation 的决策为：

- `Pass`：生成 `ValidatedChangeSet`，允许进入 Commit。只有 Pass 可以与 `ValidatedChangeSet` 同时存在。
- `Repair`：存在可修复问题且仍有修复预算。
- `Reject`：存在不可修复问题，立即失败，不得进入 `ReadyToCommit`。
- 修复预算耗尽返回 `ValidationBudgetExhausted`，不得调用 Committer。

每次 Repair 后必须重新执行完整验证。修复轮数在调用 Repairer 之前消费，任何路径都不能绕过预算检查。`max_repair_rounds = 0` 表示不允许 Repair。

---

## 14. Turn Commit 与恢复

### 14.1 Turn Committer 定位

Turn Committer 是应用层的提交协调者。它只读取 `ValidatedChangeSet`，不读取或提交 `StoryProposal`，不再次解释 LLM 输出。它依赖 Store / Unit of Work port，不包含具体数据库实现。

提交输入至少包含：

- `story_id`。
- `turn_id` 和幂等键。
- `base_revision`。
- Story Turn。
- Canonical Events。
- Validated Character / World / Memory Changes。
- 需要投递的 Outbox Records。

`ValidatedChangeSet` 字段私有，至少包含：`story_text`、`canonical_events`、`character_changes`、`world_change`、`memory_changes`、`summary_change`。可选变更不使用含义模糊的 `Option<T>`，使用显式枚举：

```rust
pub enum StateChange<T> {
    Unchanged,
    Replace(T),
}
```

需要 Patch 时使用经过验证的 Domain Command，不得重新使用 LLM Proposal Patch。只有 `ValidationDecision::Pass` 可以与 `ValidatedChangeSet` 同时存在；Context 进入 `ReadyToCommit` 前必须检查该不变量。

即使 Runtime 出现编排错误，Committer 在 Context 不是 `ReadyToCommit` 或缺少 `ValidatedChangeSet` 时也必须拒绝执行。这是第二道安全门。

### 14.2 权威状态与派生状态

状态表是当前权威状态，Canonical Events 是不可变审计记录；本阶段不实现完整 Event Sourcing。

以下内容必须在同一数据库事务内原子提交：

- Turn Record。
- Canonical Story Events。
- 影响后续叙事决策的 Character / World / Memory 权威状态。
- Story revision。
- Outbox Records。

同一事务内顺序：查询同幂等键是否已有已提交结果（有则返回原结果）-> 校验 `stored_revision == base_revision` -> 写 Turn Record -> 写 Canonical Events -> 应用经过验证的 World/Character/Memory/Scene/Constraint/Summary 变更 -> 将 Story revision 原子推进一位 -> 写 Outbox Records -> Commit transaction。revision 更新必须使用 compare-and-swap 并检查 affected rows；失败返回 `RevisionConflict`，不得覆盖较新状态。

以下内容属于可重建的派生状态，不要求与外部系统进行分布式事务：

- Embedding。
- 向量索引。
- 搜索索引。
- 可重新生成的 Summary Projection。
- 分析和通知事件。

派生状态通过 transactional outbox 在事务提交后更新。消费者必须幂等，失败可以重试和重建。Outbox 与权威变更同事务写入，至少保存 `outbox_id`、`story_id`、`turn_id`、`event_type`、`payload`、`created_at`、`attempt_count`、`published_at`、`last_error`。

`StateChange::Unchanged` 必须完全跳过 World state update，禁止把 No-Change 转换为空 `WorldState` 后 upsert。

### 14.3 提交保证

Turn Committer 必须保证：

- 原子性：权威变更全部成功或全部失败。
- 一致性：`base_revision` 与当前 revision 一致才能提交。
- 幂等性：`(story_id, idempotency_key)` 重复提交不会重复应用变更；相同 key 与相同请求摘要返回原 `CommittedTurnResult`，相同 key 与不同请求摘要返回 `IdempotencyConflict`。
- 可恢复性：进程在提交前、中、后崩溃时都能判断 Turn 是否已经提交。
- 可诊断性：冲突、约束失败和存储错误具有不同错误类型。

数据库事务提交成功是不可逆边界。事务成功但响应丢失时，客户端重试必须查询并返回原 Turn 结果，不能生成新 Turn、不再次调用 LLM。

---

## 15. 对外结果与事件

草稿、Validation 中间结果和未提交的 token 都不是最终事实。

默认协议为：

```text
Commit Success
    -> Publish Committed Event
    -> Return Turn Result
    -> Release Story Execution Ownership
```

对外成功结果必须包含稳定的 `turn_id`、已提交的 `story_revision` 和持久化的聚合 LLM usage。只有数据库 Commit 成功后才发送 `Committed` 事件。

Observer / SSE 不是权威结果存储。Observer 事件允许 best-effort，但失败必须产生 structured warning，不得静默丢弃。SSE channel 必须有界；客户端断开时触发 Cancellation；取消不得撤销已成功的数据库事务，客户端可用 idempotency key 查询结果。

最低事件：`StageStarted`、`ValidationCompleted`、`Committed`、`Failed`、`Cancelled`、`Conflict`、`TraceCompleted`。

Turn 终态只有：`Committed`、`Failed`、`Cancelled`、`Conflict`。

若未来支持提交前流式预览，事件必须明确标记为 provisional，并在失败时发送撤销/失败终态；客户端不得把 provisional 内容当作已提交历史。该能力不改变“只有提交后才成功”的语义。

---

## 16. 分层与依赖方向

目标依赖方向为：

```text
aise-server transport / composition
                |
                v
        engine / runtime
          |          |
          v          v
      pipelines    llm gateway
          |          |
          +----+-----+
               v
          core contracts
               |
               v
             domain

persistence adapter -> persistence port -> core contracts / domain
```

强制规则：

- `core` 可以依赖 `domain` 和基础错误类型，不得依赖 `runtime`、任何 Pipeline、`llm`、`persistence`、`aise-server`。`core` 是唯一 Turn Contract 定义层。
- `runtime` 只负责编排，不定义被业务模块反向引用的数据模型。
- Pipeline 之间不得互相导入、持有或调用。
- Pipeline 可以依赖 `core`、`domain` 以及被注入的 Port/Gateway。
- `llm` 可以依赖 `core` 中受限的 Turn LLM Scope，不得依赖 Runtime 或具体 Pipeline。
- Provider Adapter 不得依赖 Turn Context；它只处理供应商协议。
- Store Adapter 不得被 Core 或 Domain 导入；具体实现只在组合根装配。
- 反向通知通过注入的 trait 完成，内层模块不得导入外层具体类型。

禁止出现反向依赖：`core -> runtime`、`core -> planning/story/validation/character/context`、`llm -> runtime`、`pipeline A -> pipeline B`、`domain -> core/runtime/adapter`。

`TurnCommitter` 虽位于 `persistence` 目录，但其角色是提交协调者；数据库连接、SQL 和事务实现属于 Store adapter。

---

## 17. 模块目录结构

模块目录采用以下结构：

```text
crates/
├── aise/
│   └── src/
│       ├── core/
│       │   ├── turn_contract.rs
│       │   ├── turn_budget.rs
│       │   ├── turn_context.rs
│       │   ├── turn_data.rs
│       │   ├── story_proposal.rs
│       │   ├── turn_validation.rs
│       │   ├── turn_pipeline.rs
│       │   ├── turn_event.rs
│       │   └── turn_trace.rs
│       ├── engine.rs
│       ├── config.rs
│       ├── error.rs
│       ├── runtime/
│       │   ├── initializer.rs
│       │   ├── story_turn_coordinator.rs
│       │   ├── turn_pipeline_set.rs
│       │   └── turn_runtime.rs
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
│       │   ├── accounting.rs
│       │   ├── error.rs
│       │   ├── gateway.rs
│       │   ├── limiter.rs
│       │   ├── message.rs
│       │   ├── provider.rs
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

`mod.rs` 和 `lib.rs` 只能声明模块和 re-export，不得放置类型或逻辑。目录只表示职责归属，真正的架构边界以依赖规则和 trait port 为准。

---

## 18. LLM 调用与并发

所有 completion、streaming、embedding 和未来 Agent Loop 中的模型调用都必须经过同一个 `LlmGateway`。所有业务 Pipeline 只能持有 `Arc<LlmGateway>`，不得持有或导入底层 `LlmProvider`。

```rust
pub struct StoryGenerator {
    llm: Arc<LlmGateway>,
}
```

Gateway 的概念接口：

```rust
impl LlmGateway {
    pub async fn complete(
        &self,
        scope: TurnLlmCallScope<'_>,
        spec: CompletionSpec,
    ) -> Result<LlmCompletion, LlmError>;

    pub async fn complete_stream(
        &self,
        scope: TurnLlmCallScope<'_>,
        spec: CompletionSpec,
        sink: BoundedDeltaSink,
    ) -> Result<LlmCompletion, LlmError>;

    pub async fn embed(
        &self,
        scope: TurnLlmCallScope<'_>,
        spec: EmbeddingSpec,
    ) -> Result<EmbeddingOutput, LlmError>;
}
```

Pipeline 通过 `ctx.llm_call_scope(stage)` 获得受限 Scope。Scope 只暴露 story_id、turn_id、Stage、trace correlation、Turn absolute deadline、cancellation token、LLM Budget reservation/settlement 能力和 Turn Trace 中的 LLM Call transaction；不暴露 Baseline、Proposal、Validation 或其他 Pipeline 数据。

`OpenAiCompatProvider` 只负责构建供应商 HTTP 请求、认证 Header、解析响应/SSE Delta/finish reason/原始 token usage，并将供应商错误转换为 `LlmProviderError`。Provider 不得持有 Limiter、Turn Budget、Cancellation、Turn Trace 或业务 Context。

Gateway 是每次 LLM 调用的固定事务所有者，按以下顺序执行，每条退出路径都要完整结算：

1. 检查 Turn cancellation 和 absolute deadline。
2. 估算输入 token，并预留 Turn 的 LLM call、输入 token 和最大输出 token 预算。
3. 根据输入和 `max_output_tokens` 预留全局 RPM/TPM 配额。
4. 创建标准 `tracing` span 和 Turn Trace LLM span。
5. 在 cancellation、deadline 和 queue timeout 共同约束下等待并发 permit。
6. 使用 `min(turn_deadline, now + provider_timeout)` 约束 Provider 请求。
7. 收集 response、finish reason、provider usage 和 latency。
8. 由 Token Accountant 结算实际 usage、释放多余预留并计算 charge。
9. 记录结构化 trace/metrics。
10. 释放 permit，返回结果或 typed error。

任何 Pipeline 不得自行执行上述任一步骤。

限流最低支持：全局 `max_concurrent`、有界 `queue_timeout_ms`、可配置 `requests_per_minute`、可配置 `tokens_per_minute`、Provider/Model 维度限额键。`0` 不得被解释为无限并发，无效配置必须在启动时失败。Limiter 由 Gateway 单一拥有。

有效调用截止时间为 `min(turn_absolute_deadline, call_started_at + provider_timeout)`。Cancellation 必须覆盖 limiter 等待、HTTP 请求、SSE response stream、embedding 和 Gateway 内有限重试。本版本默认不自动重试。

错误必须区分：`Cancelled`、`TurnDeadlineExceeded`、`ProviderTimeout`、`QueueTimeout`、`RateLimited`、`TokenBudgetExceeded`、`ProviderRejected`、`Transport`、`Protocol`。

Token usage 优先使用 Provider 返回值；Provider 不返回时由 Gateway 的 Token Estimator 估算并标记 `Estimated`，不得伪装为精确值。实际 usage 超过剩余 Turn Budget 时仍记录已发生的 usage/charge，但返回 `TokenBudgetExceeded`，本 Turn 不得继续或 Commit。价格使用整数最小货币单位计算；Pricing 未配置时仍记录 token usage，`charge` 为 `None`。单次调用 usage 和 Turn 聚合 usage 都要进入已提交 Turn Result。

Gateway 是 LLM tracing transaction 的唯一所有者。默认 `metadata_only` 内容策略，Prompt 和 Response 正文不得进入普通结构化日志；开发环境显式启用内容追踪时先脱敏再按配置截断。API Key、Authorization Header、Secret Memory 和未经允许的角色私密信息永不记录。成功、Provider 错误、deadline、cancel、queue timeout 和 budget failure 都必须关闭 span 并写入终态。

LLM 配置：

```toml
[aise.llm]
provider = "openai_compat"
base_url = "..."
model = "..."
max_concurrent = 4
queue_timeout_ms = 5000
provider_timeout_ms = 30000
requests_per_minute = 120
tokens_per_minute = 100000
trace_content = "metadata_only"
```

`requests_per_minute` 和 `tokens_per_minute` 使用 `Option<NonZeroU32>`；省略表示未配置该 Provider quota。`max_concurrent` 必须存在且为正数。配置语义在 `LlmConfig::validate` 中一次性校验。

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
| StoryTurnCoordinator 协调记录 | 空闲回收、租约或关闭路径 |
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

1. 并发请求无法让同一 Story 同时执行两个 Turn，直接调用 `AiseEngine::run_turn` 也无法绕过串行化。
2. 不同 Story 可以在全局并发预算内并行。
3. 任一 Pipeline 失败都会停止后续阶段且不会提交。
4. Repair 次数、上下文大小和所有 LLM 调用均受预算约束。
5. Pipeline 无法越权修改其他阶段拥有的数据。
6. Character Thought 无法作为 World Fact 直接提交。
7. 未通过确定性验证的 Proposal 无法生成 `ValidatedChangeSet`，只有 `Pass` 决策可以进入 Commit。
8. 重复提交同一个 Turn 不会重复应用状态。
9. revision 不匹配时提交失败而不是覆盖新状态。
10. Commit 成功但响应丢失后可以通过幂等键恢复原结果。
11. 外部派生系统失败不会破坏已经提交的权威状态。
12. `core` 是唯一 Turn Contract 定义层，API、Runtime、Domain 和 adapter 之间不存在反向依赖。
13. 所有 LLM 调用只经过同一个 `LlmGateway`，统一处理限流、deadline、取消、token usage、计费和 tracing。

---

## 22. 架构总结

AISE 是一个固定 Pipeline 工作流驱动的 Turn-based Narrative Engine：

```text
Story-level Serialization (StoryTurnCoordinator)
          |
          v
Bounded Turn Runtime
          |
          v
Valid TurnExecutionContext (Core Contracts)
          |
          v
Story Proposal (untrusted)
          |
          v
Deterministic and Narrative Validation
          |
          v
ValidatedChangeSet
          |
          v
Atomic Versioned Commit
          |
          v
Committed Turn Result
```

架构的核心不是任意组合 Pipeline，而是在固定且可验证的工作流中，使每个阶段保持单一职责，并通过严格的状态、预算、并发和事务边界保证故事连续性、角色稳定性和系统可恢复性。
