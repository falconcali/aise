# AISE Turn Runtime 代码生成规范 v1.0

## 1. 文档定位

本文用于指导 AI 按阶段生成、重构和验证 AISE 的 Rust 代码。

适用基线：`main@c14f84e`。

本文不是新的业务架构，而是将 [2026-08-04-Architecture-gpt.md](./2026-08-04-Architecture-gpt.md) 中的目标架构收敛为可执行的文件、类型、接口、迁移顺序和验收标准。

实现必须同时遵守：

1. 根目录 `AGENTS.md` 及其按任务路由的 Guardrails。
2. `2026-08-04-Architecture-gpt.md` 中未被本文明确修订的架构约束。
3. 本文定义的实现边界、命名、顺序和验收条件。

本文明确修订 `2026-08-04-Architecture-gpt.md v3.0` 的以下内容，并要求在第一阶段同步形成 `2026-08-04-Architecture-gpt.md v3.1`：

- Story 串行化由 `AiseEngine` 内部的 `StoryTurnCoordinator` 强制，不再依赖 `Session::lock_turn`。
- Session 是临时连接资源，Story 是持久化领域对象，两者不构成一对一架构不变量。
- 新增 `turn` Turn Contracts 层，Runtime、Engine、LLM Gateway 和所有 Pipeline 单向依赖它。
- `TurnExecutionContext` 创建时必须已有有效的 Identity、Request、Budget、Deadline、Cancellation 和 Trace；不得存在空 ID 或半初始化对象。
- `TurnInitializer` 只负责 Turn 内部对象准备和状态初始化，不加载 Story、World、Character、Memory 或历史；请求规范化由 `TurnRequest::try_new` 在 Context 构造前完成。
- `StoryDraft` 改为不可信的 `StoryProposal`；只有 `ValidatedChangeSet` 可以进入 Commit。
- 状态表是当前权威状态，Canonical Events 是不可变审计记录；本阶段不实现完整 Event Sourcing。
- 所有 LLM 横切事务统一由 `LlmGateway` 处理。

## 2. 实现目标

本次系列重构完成后，系统必须满足：

1. 模块依赖不存在环，Pipeline Contract 不再定义在 Runtime 或业务 Pipeline 模块中。
2. 同一 Story 的所有公开执行入口都严格串行，不同 Story 可以并行。
3. Turn 工作流固定，Validation 未通过时在类型和控制流两层都无法 Commit。
4. LLM 只能生成 Proposal，不能直接生成或修改权威领域状态。
5. 所有 completion、streaming、embedding 和未来 LLM 调用都经过同一个 `LlmGateway`。
6. LLM 的限流、排队、deadline、取消、token usage、计费和 tracing 不再由调用 Pipeline 重复实现。
7. Commit 具备 revision compare-and-swap、幂等、事务和恢复语义。
8. 所有循环、集合、队列、外部调用和后台任务都有明确上限与失败语义。

## 3. 非目标

在安全闭环完成前，不实现以下业务增强：

- 完整 Writer Planner Prompt。
- Vector Retrieval、Lore Book 或 Narrative Graph 检索。
- 多角色并发思考。
- 高级 Story Prompt Builder。
- 完整 Narrative Critic。
- 多实例分布式 Story 锁。
- 完整 Event Sourcing。
- 提交前的 provisional token streaming。

这些能力只能在本文第 14 节的基础阶段全部通过后开始。

## 4. 目标依赖方向

依赖只能沿以下方向存在：

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
          turn contracts
               |
               v
             domain

persistence adapter -> persistence port -> turn contracts / domain
```

强制规则：

- `turn` 可以依赖 `domain` 和基础错误类型，不得依赖 `runtime`、任何 Pipeline、`llm`、`persistence`、`aise-server`。
- `runtime` 只能编排，不定义被业务模块反向引用的数据模型。
- Pipeline 之间不得互相导入、持有或调用。
- Pipeline 可以依赖 `turn`、`domain` 以及被注入的 Port/Gateway。
- `llm` 可以依赖 `turn` 中受限的 Turn LLM Scope，不得依赖 Runtime 或具体 Pipeline。
- Provider Adapter 不得依赖 Turn Context；它只处理供应商协议。
- Store Adapter 不得被 turn 或 domain 导入。

禁止重新出现以下依赖：

```text
turn -> runtime
turn -> planning / story / validation / character / context
llm -> runtime
pipeline A -> pipeline B
domain -> turn / runtime / adapter
```

## 5. 目标目录结构

`turn` 采用聚合文件，不为每个小结构创建独立文件：

```text
crates/aise/src/
├── turn/
│   ├── mod.rs
│   ├── turn_contract.rs
│   ├── turn_budget.rs
│   ├── turn_context.rs
│   ├── turn_data.rs
│   ├── story_proposal.rs
│   ├── turn_validation.rs
│   ├── turn_pipeline.rs
│   ├── turn_event.rs
│   └── turn_trace.rs
├── runtime/
│   ├── mod.rs
│   ├── initializer.rs
│   ├── story_turn_coordinator.rs
│   ├── turn_pipeline_set.rs
│   └── turn_runtime.rs
├── llm/
│   ├── mod.rs
│   ├── accounting.rs
│   ├── error.rs
│   ├── gateway.rs
│   ├── limiter.rs
│   ├── message.rs
│   ├── provider.rs
│   └── openai_compat.rs
├── context/
├── planning/
├── character/
├── story/
├── validation/
├── persistence/
└── domain/
```

`mod.rs` 和 `lib.rs` 只能声明模块和 re-export，不得放置类型或逻辑。

完成 turn 迁移时，必须在同一变更中删除旧定义及旧路径：

```text
runtime/pipeline.rs
runtime/turn_budget.rs
runtime/turn_execution_ctx.rs
runtime/event.rs
runtime/trace/**
context/ctx_model.rs
character/character_model.rs
story/story_model.rs
validation/validation_model.rs
```

`WriterPlan` 等类型从 `planning/writer_planner.rs` 移到 turn 后，该文件只保留 Planner 实现。不得保留 type alias、兼容 re-export、旧新双类型或转换适配层。

迁移映射：

| 当前定义 | 目标定义 |
| --- | --- |
| `runtime::pipeline::TurnExecutionPipeline` | `turn::turn_pipeline` |
| `runtime::turn_execution_ctx::TurnExecutionContext` | `turn::turn_context` |
| `runtime::turn_budget::TurnBudget` | `turn::turn_budget` |
| `runtime::event::*` | `turn::turn_event` |
| `runtime::trace::*` | `turn::turn_trace` |
| `context::ctx_model::*` | `domain::turn` |
| `planning::writer_planner::{WriterPlan, ContextRequest, StoryGoal}` | `domain::turn` |
| `character::character_model::CharacterThought` | `domain::turn` |
| `story::story_model::StoryDraft` | `domain::turn::proposal::StoryProposal` |
| `validation::validation_model::*` | `turn::turn_validation` |

## 6. turn Turn Contracts

### 6.1 文件职责

| 文件 | 必须包含 | 不得包含 |
| --- | --- | --- |
| `turn_contract.rs` | `ExecuteTurnSpec`、`TurnRequest`、`RequestDigest`、`TurnIdentity`、`IdempotencyKey`、`StoryRevision`、`TurnPhase`、`TurnStatus`、`TurnCancellation`、`TurnControl`、`CommittedTurnResult` | Store、LLM、Pipeline 实现 |
| `turn_budget.rs` | Turn 预算限制、实际消耗、Repair 消耗、LLM reservation/settlement contract | Provider 限流器、供应商计费协议 |
| `turn_context.rs` | 私有字段的 `TurnExecutionContext`、阶段化读写方法、`TurnLlmCallScope` | 外部服务、SQL、模型调用 |
| `turn_data.rs` | `StoryReadSnapshot`、`BaselineContext`、`ContextItem`、`WriterPlan`、`CharacterThought` 等跨阶段数据 | Builder、Planner、Retriever、Thinker 的执行逻辑 |
| `story_proposal.rs` | LLM 可生成的不可信 Proposal DTO | Canonical ID、权威 State、Store 类型 |
| `turn_validation.rs` | `ValidationIssue`、`ValidationDecision`、`ValidationResult`、`ValidatedChangeSet` | Validator 执行流程、LLM 调用 |
| `turn_pipeline.rs` | `TurnStage`、`TurnExecutionPipeline` | Pipeline 实现和 Runtime 编排 |
| `turn_event.rs` | Observer Event、Observer trait、终态事件数据 | SSE、HTTP Event 类型 |
| `turn_trace.rs` | 有界 Turn Trace 数据和 Recorder | LLM 调用流程、日志后端初始化 |

### 6.2 有效构造

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
- `StoryId`、`TurnId`、`IdempotencyKey` 和玩家输入不得为空。
- ID newtype 不得再实现可产生空值的 `Default`。
- 不得使用 `TurnId::from("")` 作为占位值。
- `TurnBudget` 必须来自 `TurnConfig`，不得在 Context 或 Pipeline 中再次使用另一份 Default。
- deadline 必须是绝对单调时钟时间点。
- Cancellation 必须能同时响应请求取消和服务 shutdown。
- Context 创建后即满足 `TurnPhase::Created` 的全部不变量。

建议使用 `tokio_util::sync::CancellationToken`。若引入 `tokio-util`，必须在 Workspace 统一声明版本，并记录其作为 Tokio 官方取消原语的依赖理由。

### 6.3 TurnInitializer 边界

`TurnInitializer` 的输入是已经有效的 Context，职责仅限：

- 确认已规范化 Request 满足 Turn 业务前置条件。
- 初始化只属于本 Turn 的临时槽位和执行状态。
- 将 Phase 从 `Created` 推进为 `Initialized`。

`TurnInitializer` 不得：

- 生成 `turn_id`。
- 创建或覆盖 Budget、Deadline、Cancellation、Trace。
- 调用 Store、LLM、Retriever 或其他 Pipeline。
- 加载 World、Character、Memory、History、Summary 或 Narrative Graph。

### 6.4 TurnExecutionContext

Context 字段全部私有。概念结构：

```rust
pub struct TurnExecutionContext {
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

这里的 `Option` 只表示阶段产物尚未产生；阶段正确性必须同时由私有字段、Phase 和有语义的方法保证，不能直接依赖 `Option::unwrap`。

必须提供具有业务语义的方法，不提供通用字段写接口：

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

每个写方法必须：

1. 验证当前 Phase。
2. 验证集合、字符串和 token 上限。
3. 只更新该阶段拥有的输出。
4. 清除已经失效的下游数据。
5. 推进到唯一允许的下一 Phase。
6. 在违反不变量时返回 typed error，不得 panic。

Repair 替换 Proposal 时必须清除旧 Validation 和旧 `ValidatedChangeSet`，并增加 `proposal_revision`。

### 6.5 TurnPhase

最低要求：

```text
Created
Initialized
Prepared
Planned
ContextReady
ProposalReady
RepairRequired
ReadyToCommit
Committed
Failed
Cancelled
Conflict
```

允许转换：

| 当前 Phase | 操作 | 下一 Phase |
| --- | --- | --- |
| `Created` | Initializer 成功 | `Initialized` |
| `Initialized` | Baseline Builder 成功 | `Prepared` |
| `Prepared` | Planner 成功 | `Planned` |
| `Planned` | Runtime 完成或跳过可选阶段 | `ContextReady` |
| `ContextReady` | Generator 成功 | `ProposalReady` |
| `ProposalReady` | Validation Pass | `ReadyToCommit` |
| `ProposalReady` | Validation Repair | `RepairRequired` |
| `RepairRequired` | Repairer 成功 | `ProposalReady` |
| `ReadyToCommit` | Commit 成功 | `Committed` |
| 任意非终态 | 失败、取消或冲突 | 对应终态 |

Validation Reject 不得进入 `ReadyToCommit`。

### 6.6 Pipeline Contract

统一接口保持不变，但 Stage 改为有穷枚举：

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

`TurnStage::as_str()` 返回稳定、低基数字符串。禁止 Pipeline 自定义动态 Stage 名称。

`Ok(())` 表示该 Pipeline 自身执行成功并写入了合法阶段结果。Validation 返回 `Repair` 或 `Reject` 是合法业务结果，可以返回 `Ok(())`，但 Runtime 必须立即读取 Decision 并分支；它不等于整个 Turn 成功。

## 7. 八步 Turn 工作流

固定顺序：

1. `TurnInitializer`
2. `BaselineContextBuilder`
3. `WriterPlanner`
4. `ContextRetrievalPipeline`，有请求时执行
5. `CharacterThinkPipeline`，有请求时执行
6. `StoryGenerator`
7. `ValidationPipeline`，内部由 Runtime 控制 Repair 循环
8. `TurnCommitter`

`StoryRepairer` 是第 7 步循环中的辅助 Pipeline，不是独立的第九个业务阶段。

阶段契约：

| 阶段 | 只读输入 | 唯一输出 |
| --- | --- | --- |
| Initializer | Identity、已规范化 Request、Budget、Control | Turn 临时状态、Initialized Phase |
| Baseline Builder | Identity、Request、Budget | `StoryReadSnapshot`、`BaselineContext` |
| Writer Planner | Baseline、Request | `WriterPlan` |
| Retrieval | Plan、Baseline、Budget | `ContextItem[]` |
| Character Think | Plan、Snapshot、Retrieved、Budget | `CharacterThought[]` |
| Story Generator | Baseline、Plan、Retrieved、Thoughts | `StoryProposal` |
| Validation | Snapshot、Proposal | `ValidationResult`；仅 Pass 时产生 `ValidatedChangeSet` |
| Story Repairer | Proposal、Validation Issues | 新版本 `StoryProposal` |
| Turn Committer | Identity、Snapshot、`ValidatedChangeSet` | `CommittedTurnResult` |

Runtime 根据 `retrieval_requests.is_empty()` 和 `character_requests.is_empty()` 决定是否执行可选阶段。不得同时保留 `need_retrieval`、`need_character_thinking` 之类可能与请求集合矛盾的布尔值。

跳过的阶段：

- 不调用 Pipeline。
- 不产生 `StageStarted`。
- 由 Runtime 通过 Context 的显式方法记录为空结果并推进到 `ContextReady`。

## 8. Story Proposal、Validation 与 Commit 边界

### 8.1 StoryProposal

`StoryProposal` 是不可信模型输出，替代 `StoryDraft`。

它可以包含：

- Story Text。
- Proposed Event DTO。
- Proposed Character Change DTO。
- Proposed World Change DTO。
- Proposed Memory Change DTO。
- Proposed Summary Delta。

它不得包含：

- `EventId`、Canonical Event 或已授权 Command。
- 完整 `CharacterState`、`WorldState` 或其他可直接覆盖权威状态的对象。
- 数据库字段、revision 更新或 Outbox Record。

Generator 和 Repairer 不得注入 Store、Unit of Work、ID Generator 或 Clock。

### 8.2 ValidationResult

最低数据结构：

```rust
pub enum ValidationDecision {
    Pass,
    Repair,
    Reject,
}

pub struct ValidationIssue {
    pub code: ValidationIssueCode,
    pub message: String,
    pub severity: ValidationSeverity,
    pub repairability: Repairability,
    pub location: Option<ValidationLocation>,
}

pub struct ValidationResult {
    decision: ValidationDecision,
    issues: Vec<ValidationIssue>,
}
```

`ValidationIssue` 数量和单条 message 长度必须受 Budget 限制。

Validation 顺序：

1. 解析和 Schema。
2. ID/引用完整性。
3. 修改权限。
4. Domain invariant。
5. Knowledge Boundary 和 Player Control Boundary 的确定性部分。
6. Narrative/Character Consistency。
7. 生成 `ValidatedChangeSet`。

确定性失败不得被 LLM Validator 覆盖。Narrative Validator 的所有 LLM 调用也必须经过 `LlmGateway`。

### 8.3 ValidatedChangeSet

`ValidatedChangeSet` 字段私有，至少包含：

```text
story_text
canonical_events
character_changes
world_change
memory_changes
summary_change
```

`world_change` 等可选变更不得使用含义模糊的 `Option<T>`。使用显式枚举：

```rust
pub enum StateChange<T> {
    Unchanged,
    Replace(T),
}
```

需要 Patch 时使用经过验证的 Domain Command，不得重新使用 LLM Proposal Patch。

只有 `ValidationDecision::Pass` 可以与 `ValidatedChangeSet` 同时存在。Context 进入 `ReadyToCommit` 前必须检查该不变量。

### 8.4 TurnCommitter

TurnCommitter：

- 只读取 `ValidatedChangeSet`。
- 不读取或提交 `StoryProposal`。
- 不再次解释 LLM 输出。
- 根据 Canonical Events 和确定性派生任务映射生成 Outbox Records。
- 不生成缺失的默认 World。
- 不吞掉 Store、revision 或幂等错误。
- Commit 成功后写入 `CommittedTurnResult`。

即使 Runtime 出现编排错误，Committer 在 Context 不是 `ReadyToCommit` 或缺少 `ValidatedChangeSet` 时也必须拒绝执行。这是第二道安全门。

## 9. LLM 模块规范

### 9.1 单一入口

所有业务 Pipeline 只能持有 `Arc<LlmGateway>`，不得持有或导入底层 `LlmProvider`。

```rust
pub struct StoryGenerator {
    llm: Arc<LlmGateway>,
}
```

以下调用全部经过 Gateway：

- completion。
- streaming completion。
- embedding。
- Planner、Character Think、Story Generate、Story Repair、Narrative Validation。
- 未来 Agent Loop 中的每一次模型调用。

`OpenAiCompatProvider` 只负责：

- 构建供应商 HTTP 请求。
- 认证 Header。
- 解析响应、SSE Delta、finish reason 和原始 token usage。
- 将供应商错误转换为 `LlmProviderError`。

Provider 不得持有 Limiter、Turn Budget、Cancellation、Turn Trace 或业务 Context。

### 9.2 Gateway API

概念接口：

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

Pipeline 通过 `ctx.llm_call_scope(stage)` 获得受限 Scope。Scope 只暴露：

- `story_id`、`turn_id`、Stage 和 trace correlation。
- Turn absolute deadline。
- Cancellation token。
- LLM Budget reservation/settlement 能力。
- Turn Trace 中的 LLM Call transaction。

Scope 不暴露 Baseline、Proposal、Validation 或其他 Pipeline 数据。

### 9.3 每次调用的固定事务

Gateway 必须按以下顺序执行，每条退出路径都要完整结算：

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

### 9.4 限流与背压

最低支持：

- 全局 `max_concurrent`。
- 有界等待时间 `queue_timeout_ms`。
- 可配置 `requests_per_minute`。
- 可配置 `tokens_per_minute`。
- Provider 或 Model 维度的限额键。

`0` 不得被解释为无限并发。无效配置必须在启动时失败。

Limiter 由 Gateway 单一拥有，Provider 不再各自创建 Semaphore。Permit 等待也必须受 Turn deadline 和 cancellation 控制。

### 9.5 Deadline 与取消

有效调用截止时间：

```text
effective_deadline = min(turn_absolute_deadline, call_started_at + provider_timeout)
```

Cancellation 必须覆盖：

- 等待 limiter。
- HTTP 请求。
- SSE response stream。
- embedding。
- Gateway 内的有限重试。

本版本默认不自动重试。未来若增加重试，只能由 Gateway 根据 typed retry policy 执行，并且必须受 attempt、deadline 和 token budget 限制。

错误必须区分：

```text
Cancelled
TurnDeadlineExceeded
ProviderTimeout
QueueTimeout
RateLimited
TokenBudgetExceeded
ProviderRejected
Transport
Protocol
```

### 9.6 Token usage 与计费

Provider 返回统一结构：

```rust
pub struct LlmTokenUsage {
    pub input_tokens: u64,
    pub cached_input_tokens: Option<u64>,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub accuracy: UsageAccuracy,
}

pub struct LlmCompletion {
    pub text: String,
    pub finish_reason: Option<FinishReason>,
    pub usage: LlmTokenUsage,
    pub charge: Option<LlmCharge>,
}
```

规则：

- 优先使用 Provider 返回的 usage。
- Provider 不返回 usage 时，由 Gateway 拥有的 Token Estimator 估算，并标记 `Estimated`；不得伪装为精确值。
- Turn Budget 使用实际值或保守估算值结算。
- 实际 usage 超过剩余 Turn Budget 时，Gateway 仍要记录已发生的 usage/charge，但返回 `TokenBudgetExceeded`，本 Turn 不得继续或 Commit。
- 价格使用整数最小货币单位计算，不使用浮点数累计金额。
- Pricing 未配置时仍记录 token usage，`charge` 为 `None`。
- 计费记录至少包含 provider、model、input、cached input、output、usage accuracy 和价格版本。
- 单次调用 usage 和 Turn 聚合 usage 都要进入已提交 Turn Result；持久化在 revision/idempotency 阶段完成。

### 9.7 LLM tracing

Gateway 是 LLM tracing transaction 的唯一所有者。每次调用至少记录：

```text
story_id
turn_id
stage
purpose
provider
model
attempt
queue_wait_ms
provider_latency_ms
total_latency_ms
input_tokens
cached_input_tokens
output_tokens
usage_accuracy
charge
finish_reason
status
error_kind
```

内容策略：

- 默认 `metadata_only`。
- Prompt 和 Response 正文不得进入普通结构化日志。
- 开发环境显式启用内容追踪时，必须先脱敏再按配置截断。
- API Key、Authorization Header、Secret Memory 和未经允许的角色私密信息永不记录。
- 成功、Provider 错误、deadline、cancel、queue timeout 和 budget failure 都必须关闭 span 并写入终态。

StoryGenerator、StoryRepairer 和其他调用方必须删除当前的 `Instant`、`tracing::info_span!`、`LlmCallData` 拼装和 `ctx.trace.begin/end` LLM 逻辑。

### 9.8 LLM 配置

最低配置：

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

`requests_per_minute` 和 `tokens_per_minute` 使用 `Option<NonZeroU32>`；省略表示未配置该 Provider quota。`max_concurrent` 必须存在且为正数。配置语义必须在 `LlmConfig::validate` 中一次性校验。

## 10. Engine、Coordinator 与 Runtime

### 10.1 StoryTurnCoordinator

`StoryTurnCoordinator` 注入 `AiseEngine`，由 `AiseEngine::run_turn` 内部调用。所有 HTTP、CLI、测试和任务消费者都无法绕过。

单进程实现要求：

- 每个 `StoryId` 一个容量为 1 的执行 permit。
- Permit 等待队列有全局和 per-Story 上限。
- 等待受 Turn deadline 和 cancellation 约束。
- Map 锁只用于同步获取或更新 Entry，不得跨 `.await` 持有。
- Turn 生命周期持有的是 owned permit，不是 `MutexGuard` 或 `RwLockGuard`。
- Entry 在无 owner、无 waiter 且空闲超过配置时间后回收。
- Shutdown 时拒绝新请求，取消等待者，并等待已拥有 Turn 在宽限期内结束。

`Session::turn_lock`、`Session::lock_turn` 及 HTTP handler 中的锁逻辑必须删除。

### 10.2 AiseEngine

概念入口：

```rust
pub async fn run_turn(
    &self,
    spec: ExecuteTurnSpec,
    observer: &dyn TurnEventObserver,
) -> Result<CommittedTurnResult, AiseError>;
```

固定所有权顺序：

```text
Validate and canonicalize ExecuteTurnSpec into TurnRequest
    -> Acquire StoryTurnPermit
    -> Check committed idempotency result by request digest
    -> Create TurnIdentity / Budget / Control / Trace
    -> Construct valid TurnExecutionContext
    -> TurnRuntime::run
    -> Publish committed result or typed terminal event
    -> Release permit
    -> Destroy Context
```

Turn ID、Clock、Budget Policy 和 shutdown cancellation 由 Engine 的注入依赖提供。Pipeline 不得直接调用 `Uuid::new_v4` 或 `SystemTime::now`。

### 10.3 TurnPipelineSet

删除 `TurnRuntime::new(Vec<Box<dyn TurnExecutionPipeline>>)`。

使用私有命名字段：

```text
initializer
baseline_builder
writer_planner
retrieval
character_think
story_generator
validation
story_repairer
committer
```

`TurnPipelineSet::new` 必须验证每个对象的 `TurnStage` 与字段匹配。不得提供任意 Vec、运行时插件拼接或重新排序 API。

### 10.4 Runtime 控制流

概念流程：

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

Repair 预算在调用 Repairer 之前消费。`max_repair_rounds = 0` 表示不允许 Repair，而不是无限次数。

Runtime 在每个 Pipeline 前后检查 cancellation、deadline、预期 Phase 和后置 Phase。失败后不得启动下一个 Pipeline。

## 11. Snapshot、权威状态与持久化

### 11.1 StoryReadSnapshot

使用 `StoryReadSnapshot` 表示一次 Turn 从 Store 原子读取的不可变视图，避免把 World State 本身称为 Story Snapshot。

最低内容：

```text
story_id
base_revision
story instructions/configuration
player character id
world state
current scene
relevant characters
bounded recent turns
story summary
active constraints
required memories
```

规则：

- Snapshot 由一次一致性读事务获得。
- Baseline 只能从该 Snapshot 构建，不再分别读取互相可能错版的数据。
- Recent Turns 从 Store 返回给业务层时必须是时间正序。
- Player Character 通过稳定 ID 指定，不得取 SQL 无序结果的第一项。
- `WorldState` 不再内嵌 Character 权威副本；Character 表是 Character 当前状态的唯一权威来源。
- `WorldFact` 使用稳定 `FactId`，删除操作不得使用数组下标。

### 11.2 Commit 语义

Store Port 的概念接口：

```rust
async fn load_story_snapshot(
    &self,
    story_id: &StoryId,
    limits: SnapshotLimits,
) -> Result<StoryReadSnapshot, StoreError>;

async fn commit_turn(
    &self,
    spec: &TurnCommitSpec,
) -> Result<CommittedTurnResult, StoreError>;
```

同一数据库事务内必须：

1. 查询相同 idempotency key 是否已有已提交结果；有则返回原结果。
2. 校验 `stored_revision == base_revision`。
3. 写 Turn Record。
4. 写 Canonical Events。
5. 应用经过验证的 World、Character、Memory、Scene、Constraint 和 Summary 变更。
6. 将 Story revision 原子推进一位。
7. 写 Outbox Records。
8. Commit transaction。

revision 更新必须使用 compare-and-swap，并检查 affected rows。失败返回 `RevisionConflict`，不得覆盖较新状态。

### 11.3 幂等

幂等唯一键为：

```text
(story_id, idempotency_key)
```

规则：

- 相同 key、相同规范化请求：返回原 `CommittedTurnResult`，不再次调用 LLM、不再次提交。
- 相同 key、不同请求摘要：返回 `IdempotencyConflict`。
- Commit 成功但响应丢失后，重试必须恢复原结果。
- `turn_id` 唯一约束不能替代 idempotency key。

HTTP 使用必填 `Idempotency-Key` Header，并更新前端调用方。不得由服务端为每次重试静默生成新 key。

### 11.4 World No-Change

`StateChange::Unchanged` 必须完全跳过 World state update。禁止把 No-Change 转换为空 `WorldState` 后 upsert。

必须添加回归测试：已有 World 非空时，提交一个不含 World Change 的 Turn，提交后 World 字节级语义保持不变，仅 Story revision 正常推进。

### 11.5 Outbox

Outbox 与权威变更同事务写入，至少保存：

```text
outbox_id
story_id
turn_id
event_type
payload
created_at
attempt_count
published_at
last_error
```

Embedding、向量索引、可重建 Summary Projection、分析和通知只能由 Outbox 消费者异步更新，不得加入 Turn 数据库事务之外的“先写后补”流程。

## 12. Budget、容量与配置

`TurnBudget` 至少包含：

```text
max_repair_rounds
max_llm_calls
max_input_tokens
max_output_tokens
max_total_tokens
max_retrieved_items
max_context_tokens
max_character_thoughts
max_validation_issues
max_trace_spans
```

预算分为 immutable limits 和 mutable usage，字段均私有。

配置必须只有一个权威来源：

- `TurnConfig` 定义 limits。
- Engine 将其转换为当前 Turn 的 `TurnBudget`。
- Pipeline 不保存重复的 `max_tokens`、`max_repair_rounds` 或 `max_retrieved_items`。
- `StoryGenerator` 从 Gateway reservation 获得本次允许的最大输出 token，不从自己的字段读取另一份配置。

多角色思考在本规范完成前按确定性顺序执行。未来增加并发时，必须先为每个调用从 Context 创建独立预算 reservation，并使用有界 batch API；不得 clone Context 或共享 `&mut TurnExecutionContext`。

## 13. Error、事件与生命周期

### 13.1 Typed errors

`AiseError` 至少可区分：

```text
InvalidRequest
StoryNotFound
Llm
Store
ValidationRejected
ValidationBudgetExhausted
TurnDeadlineExceeded
Cancelled
RevisionConflict
IdempotencyConflict
Backpressure
InvariantViolation
```

turn/Domain/LLM/Store Port 使用各自的 `thiserror` typed error。`sqlx::Error` 和 `reqwest::Error` 不得直接成为 turn API 的公开错误语义。

Turn 终态：

```text
Committed
Failed
Cancelled
Conflict
```

### 13.2 Observer events

Observer/SSE 不是权威结果存储。Observer 事件允许 best-effort，但失败必须产生 structured warning，不得静默丢弃。

最低事件：

```text
StageStarted
ValidationCompleted
Committed
Failed
Cancelled
Conflict
TraceCompleted
```

只有数据库 Commit 成功后才发送 `Committed`。当前 `Token` 若仍表示完整最终文本，应改成明确的 committed result event；不得伪装为实时 delta。

SSE channel 必须有界。客户端断开时触发 Cancellation；取消不得撤销已成功的数据库事务，客户端可用 idempotency key 查询结果。

### 13.3 Turn task owner

HTTP 层不得创建无所有者的 `tokio::spawn`。

必须由有界 `TurnTaskManager` 或等价组件拥有 JoinHandle/JoinSet：

- 有全局 admission limit。
- 服务 shutdown 时停止接收新任务。
- 取消等待中的任务。
- 给执行中的事务提供 shutdown grace period。
- 等待或明确终止全部 task。

## 14. 强制实施顺序

一次 AI 编码任务默认只执行一个 Phase。每个 Phase 必须独立完成迁移、删除旧路径、更新测试和文档，并通过第 16 节命令后再进入下一 Phase。

### Phase 0：Architecture v3.1 对齐

只修改文档：

1. 将本文第 1 节列出的架构修订同步到 `2026-08-04-Architecture-gpt.md`。
2. 更新模块图、Context 阶段表、串行化所有者和 LLM 章节。

验收：两份文档（`2026-08-04-Architecture-gpt.md` 与本文）之间不存在 Session lock、Initializer、Story Proposal、turn 依赖和 LLM 所有权冲突。

### Phase 1：turn Contracts 与有效 Context

1. 创建 `turn` 目录和第 5 节文件。
2. 移动全部跨 Pipeline Contract。
3. 删除旧 Contract 文件和旧 import path。
4. Context 字段私有化并加入 Phase 方法。
5. 修复 ID 空值、Clock 和 ID Generator 所有权。
6. 将 Turn Budget 统一为配置生成的单一实例。
7. 将 Turn Trace 移入 turn，仍保持现有非 LLM trace 行为。

本阶段是结构迁移：只创建已经有消费方的 Contract。`StoryDraft` 在迁移时直接改名为 `StoryProposal`，但其字段的信任边界在 Phase 3 一次性收敛；不得为了未来阶段加入未使用的占位类型。

验收：`runtime` 和所有 Pipeline 只从 `crate::turn` 导入 Turn Contract；代码可编译且不存在兼容层。

### Phase 2：Engine 执行权与统一 LlmGateway

1. 实现 `StoryTurnCoordinator` 并注入 `AiseEngine`。
2. 将 Story permit 的获取移动到 `AiseEngine::run_turn` 内部。
3. 删除 `Session::turn_lock`、`Session::lock_turn` 和 HTTP handler 锁逻辑。
4. Provider 中删除 Limiter。
5. 新增 Gateway、Accounting、Usage 和统一错误。
6. 所有 Pipeline 从 Provider 迁移到 Gateway。
7. Gateway 实现第 9 节固定事务。
8. 接入 Turn deadline、Cancellation、LLM Budget 和 Trace。
9. 默认切换到 metadata-only 内容策略。
10. completion 与 streaming 先完成；embedding 接口可先提供明确的 Unsupported typed error，但调用路径不得绕过 Gateway。

验收：直接调用 Engine 也无法绕过 Story 串行化；业务目录中不存在 `LlmProvider` import；所有 LLM 成功和失败路径都有准确 usage、budget 和 trace 终态。

### Phase 3：安全 Proposal → Validation → Commit 闭环

1. 将 `StoryProposal` 字段收敛为纯 Proposal DTO，不再复用 Canonical Event 和权威 Patch 类型。
2. Validation 产生明确 Decision。
3. 只有 Pass 产生并消费 `ValidatedChangeSet`。
4. TurnCommitter 只接受 `ValidatedChangeSet`。
5. Committer 加入 Phase 二次门禁。
6. 立即修复 `world == None` 覆盖已有 World。

本阶段可以暂时不 Repair；Repairable Validation 先安全失败，绝不能提交。

验收：任何 Validation 非 Pass、缺少 ChangeSet 或伪造 Proposal 的路径都无法修改 Store。

### Phase 4：Revision、幂等、原子 Snapshot 与 Outbox

1. 实现 `StoryReadSnapshot` 单事务读取。
2. 增加 Story revision 和 commit compare-and-swap。
3. 增加 `(story_id, idempotency_key)` 唯一约束和请求摘要。
4. 返回并持久化 `CommittedTurnResult`。
5. 增加 Outbox 表和同事务写入。
6. 明确 World、Character、Memory 的单一权威来源。
7. 修复 Player Character 与 recent history 的确定性加载。

验收：revision conflict、重复请求、响应丢失恢复和事务回滚测试全部通过。

### Phase 5：固定 TurnPipelineSet 与有界 Repair Loop

1. 用命名字段 `TurnPipelineSet` 替换任意 Pipeline Vec。
2. Runtime 控制可选阶段，不再由可选 Pipeline 自行 no-op。
3. 装配 `StoryRepairer`。
4. 实现 Validation Decision 分支。
5. Repair 前消费预算，每次 Repair 后完整重验。
6. 预算耗尽返回 `ValidationBudgetExhausted`，不调用 Committer。

验收：工作流不可重排，跳过阶段无事件，Repair 次数严格不超过配置。

### Phase 6：并发、失败、背压与恢复测试

1. 增加 same-Story serial、different-Story parallel 测试。
2. 增加 LLM queue timeout、deadline、cancel 和 limiter 测试。
3. 增加 Commit 失败、崩溃恢复、重复请求和 Outbox 测试。
4. 增加有界 SSE、Turn task owner 和 shutdown 测试。
5. 增加 Story coordinator wait queue、回收和 shutdown 测试。
6. 增加依赖方向静态检查。

验收：第 15 节测试矩阵全部通过。

### Phase 7：业务 Pipeline 完善

严格按以下顺序：

1. Baseline Context Builder。
2. Prompt Builder / Context Merger。
3. Writer Planner。
4. Context Retrieval。
5. Character Think。
6. Story Generator / Repairer。
7. Narrative Validation。

每新增一个 LLM 调用，必须先证明它经过 Gateway 并受 Turn Budget 限制。

## 15. 必须实现的测试矩阵

### 15.1 turn 与阶段

- `context_rejects_empty_identity`
- `context_rejects_invalid_phase_transition`
- `initializer_does_not_access_external_services`
- `repair_invalidates_previous_validation_and_change_set`
- `bounded_outputs_reject_over_limit_values`

### 15.2 Runtime

- `pipeline_error_stops_following_stages`
- `runtime_skips_empty_retrieval_without_stage_event`
- `runtime_skips_empty_character_think_without_stage_event`
- `validation_reject_never_invokes_committer`
- `repair_revalidates_full_pipeline`
- `repair_budget_is_consumed_before_repair_call`
- `repair_budget_exhaustion_never_commits`
- `pipeline_set_rejects_wrong_stage_binding`

### 15.3 LLM Gateway

- `all_calls_wait_for_shared_permit`
- `permit_wait_respects_queue_timeout`
- `permit_wait_respects_turn_deadline`
- `provider_call_respects_cancellation`
- `stream_respects_cancellation`
- `budget_is_reserved_before_provider_dispatch`
- `actual_usage_settles_reserved_tokens`
- `missing_provider_usage_is_marked_estimated`
- `pricing_uses_integer_units`
- `llm_trace_closes_on_success`
- `llm_trace_closes_on_provider_error`
- `llm_trace_closes_on_timeout_and_cancel`
- `default_trace_does_not_store_prompt_or_response_text`

### 15.4 Validation 与 Commit

- `proposal_cannot_be_committed_directly`
- `deterministic_failure_cannot_be_overridden_by_narrative_validator`
- `pass_is_the_only_decision_that_produces_change_set`
- `committer_rejects_non_ready_context`
- `world_unchanged_does_not_overwrite_existing_world`
- `character_thought_cannot_become_world_fact`

### 15.5 Persistence

- `snapshot_is_revision_consistent`
- `recent_turns_are_returned_in_chronological_order`
- `player_character_is_selected_by_stable_id`
- `revision_conflict_rolls_back_every_change`
- `same_idempotency_key_returns_original_result`
- `same_key_with_different_request_returns_conflict`
- `response_loss_retry_does_not_call_llm_again`
- `outbox_is_atomic_with_turn_commit`
- `transaction_failure_persists_nothing`

### 15.6 并发与生命周期

- `same_story_turns_never_overlap_through_engine_api`
- `different_story_turns_can_overlap`
- `direct_engine_call_cannot_bypass_coordination`
- `story_wait_queue_rejects_over_capacity`
- `client_disconnect_cancels_uncommitted_turn`
- `bounded_sse_channel_applies_backpressure`
- `shutdown_waits_for_owned_turn_tasks`

## 16. 每个 Phase 的完成检查

每个 Phase 完成后必须运行：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

还必须检查：

```bash
rg "crate::runtime::(pipeline|turn_budget|turn_execution_ctx|event|trace)" crates/aise/src
rg "LlmProvider" crates/aise/src/context crates/aise/src/planning crates/aise/src/character crates/aise/src/story crates/aise/src/validation
rg "StoryDraft" crates
rg "lock_turn|turn_lock" crates
```

最终状态要求以上命令无旧路径匹配；底层 Provider 的定义和 `llm` 模块内部使用不计入第二项业务目录检查。

## 17. AI 生成代码规则

执行本 Spec 的 AI 必须：

1. 开始前完整读取 `AGENTS.md` 和当前 Phase 对应的 Guardrails。
2. 一次只执行一个 Phase，不提前实现后续业务能力。
3. 修改前列出该 Phase 的文件增删清单和依赖方向。
4. 优先迁移现有类型，不创建同义新类型。
5. 同一 Phase 中删除旧代码、旧配置、旧测试和旧文档路径。
6. 不添加 fallback、compatibility shim、adapter bridge、dual path 或 dead flag。
7. 不在代码中添加普通注释或文档注释。
8. 不在 Pipeline 内创建 Provider、Limiter、Store、Clock、UUID Generator 或后台任务。
9. 不用 `unwrap`、`expect` 或 panic 表达业务错误。
10. 不持有 Mutex/RwLock Guard 跨 `.await`。
11. 不新增无界 Vec、channel、queue、retry、loop 或 fan-out。
12. 报告实际运行的 fmt、clippy、test 和静态检查结果；未运行不得声称通过。

若当前代码与本文不一致，AI 应按当前 Phase 迁移到本文目标，不得修改 Spec 来迁就旧实现。若发现本文内部矛盾、数据迁移不可恢复或需要扩大权威状态范围，应停止编码并提出具体问题。

## 18. 最终 Definition of Done

只有同时满足以下条件，Turn Runtime 重构才算完成：

1. `turn` 成为唯一 Turn Contract 定义层，依赖方向单向且无兼容旧路径。
2. Context 从创建开始有效，所有阶段输出私有、受限、可验证。
3. Engine 内部强制 Story 串行化，Session 不再拥有执行锁。
4. Runtime 固定八步工作流并控制可选阶段和 Repair Loop。
5. Proposal 无法绕过 Validation 转换成权威变更。
6. Committer 只能提交 `ValidatedChangeSet`。
7. Snapshot、revision、幂等、事务和 Outbox 形成恢复闭环。
8. 所有 LLM 调用只经过 Gateway，统一处理限流、deadline、取消、token usage、计费和 tracing。
9. 成功结果只在 Commit 后发布，失败和取消具有明确终态。
10. 所有资源有界，全部验收测试和工具链检查通过。
