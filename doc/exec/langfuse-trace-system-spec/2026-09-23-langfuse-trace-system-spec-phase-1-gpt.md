# Langfuse Trace System Phase 1 — Spec

> **Model**: GPT-5.6 Sol
> **Date**: 2026-09-23
> **Status**: Proposed
> **Source Design**: [Langfuse Trace 系统重构](../../design/2026-09-23-langfuse-trace-system-design-gpt.md)
> **Phase**: Phase 1 of 3

---

## 1. Goal

Instrument the actual Turn execution path with one stable bilingual OpenTelemetry tree covering submission, orchestration, Pipelines, LLM calls, retrieval, validation when executed, and persistence.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Make `TurnSubmissionService` create and bind the root Trace for every accepted Turn attempt.
- Transfer root ownership into the admitted background task and `AiseEngine`.
- Instrument all currently executed Turn phases and all LLM completions.
- Instrument retrieval, story coordination, idempotency, and persistence with accurate Langfuse Observation types.
- Implement late-bound Session, Story, request digest, Turn number, replay, terminal status, and failure-stage attributes.
- Emit bounded input, output, model, parameter, usage, cost, skip, attempt, and error fields.
- Add in-memory exporter integration tests for every terminal path and the canonical parent-child tree.

### 2.2 Non-Goals

- Does not restore the disabled Validation/Repair loop or the temporary state-extraction path in `TurnRuntime`.
- Does not create spans for stages that did not execute.
- Does not delete the legacy Trace DTO, sinks, files, SSE payload, UI, or configuration; Phase 2 performs deletion.
- Does not capture Axum transport extraction failures as Turn Traces.
- Does not add Langfuse scores, evaluators, datasets, prompt management, metrics, or logs.
- Does not change Turn business outcomes, retry policy, budgets, persistence semantics, or LLM concurrency limits.

### 2.3 Implementation Constraints

- Phase 1 MUST use only the Observation facade from Phase 0; business modules MUST NOT write `langfuse.*` attributes or construct OpenTelemetry exporters.
- Every LLM call MUST continue to pass through the existing shared `LlmGateway` concurrency limiter.
- Instrumentation MUST follow actual control flow. Commented-out Validation/Repair code MUST remain uninstrumented and MUST NOT be re-enabled by this phase.
- Dynamic values MUST be fields, never Observation names.
- No Span enter guard may cross `.await`; use `ObservationSpan::in_scope` or `observe_result`.
- No Observation object may outlive one Turn or be persisted in `TurnExecutionContext`.

---

## 3. Contracts

### 3.1 Stable Observation Registry

`ObservationStep` MUST return this exact registry:

| Variant | `ObservationKind` | Exact name |
|---|---|---|
| `ExecuteStoryTurn` | `Chain` | `execute-story-turn (执行故事回合)` |
| `ResolveInteractionSession` | `Retriever` | `resolve-interaction-session (解析交互会话)` |
| `ValidateRequest` | `Span` | `validate-request (校验请求)` |
| `AdmitTurnTask` | `Span` | `admit-turn-task (准入回合任务)` |
| `CoordinateStoryTurn` | `Span` | `coordinate-story-turn (协调故事回合)` |
| `LoadStory` | `Retriever` | `load-story (加载故事)` |
| `CheckIdempotency` | `Retriever` | `check-idempotency (检查幂等性)` |
| `RunTurnPipelines` | `Chain` | `run-turn-pipelines (执行回合流水线)` |
| `InitializeTurn` | `Chain` | `initialize-turn (初始化回合)` |
| `PrepareContext` | `Chain` | `prepare-context (准备上下文)` |
| `LoadStorySnapshot` | `Retriever` | `load-story-snapshot (加载故事快照)` |
| `ActivateWorldInfo` | `Retriever` | `activate-world-info (激活世界信息)` |
| `PlanTurn` | `Chain` | `plan-turn (规划回合)` |
| `ProjectNarrative` | `Span` | `project-narrative (投影叙事图)` |
| `GenerateWriterPlan` | `Generation` | `generate-writer-plan (生成写作计划)` |
| `RetrieveContext` | `Retriever` | `retrieve-context (检索上下文)` |
| `ThinkCharacters` | `Chain` | `think-characters (角色思考)` |
| `ThinkCharacter` | `Generation` | `think-character (角色思考)` |
| `GenerateStory` | `Chain` | `generate-story (生成故事)` |
| `DraftStoryText` | `Generation` | `draft-story-text (起草故事正文)` |
| `ExtractStoryState` | `Chain` | `extract-story-state (提取故事状态)` |
| `InferStoryState` | `Generation` | `infer-story-state (推断故事状态)` |
| `ValidateStory` | `Evaluator` | `validate-story (校验故事)` |
| `RepairStory` | `Chain` | `repair-story (修复故事)` |
| `ReviseStoryText` | `Generation` | `revise-story-text (修订故事正文)` |
| `CommitTurn` | `Chain` | `commit-turn (提交回合)` |
| `PersistTurn` | `Tool` | `persist-turn (持久化回合)` |

The registry is an analytics API. Any future name or kind change MUST increment `ObservationStep::SCHEMA_VERSION`.

### 3.2 Root Ownership and Engine API

Replace the engine entry contract with:

```rust
impl AiseEngine {
    pub async fn run_turn(
        &self,
        spec: ExecuteTurnSpec,
        sink: &dyn TurnEventSink,
        trace: ObservationTrace,
    ) -> Result<CommittedTurnResult, TurnExecutionError>;

    pub async fn execute_turn(
        &self,
        spec: ExecuteTurnSpec,
        sink: &dyn TurnEventSink,
        trace: ObservationTrace,
    ) -> TurnRunOutcome;
}
```

`TurnSubmissionService` MUST own `ObservationTrace` before admission. The admitted task MUST move it into `AiseEngine::execute_turn`. `AiseEngine` MUST finish it on every committed, replayed, failed, cancelled, deadline, and conflict return path.

The old `AiseEngine::with_trace_sink` contract remains present only until Phase 2 deletion and MUST NOT receive data from the new Observation path.

### 3.3 Root Trace Fields

The root MUST emit this logical payload:

```json
{
  "name": "execute-story-turn (执行故事回合)",
  "observation_type": "chain",
  "trace_name": "execute-story-turn (执行故事回合)",
  "tags": ["story-turn"],
  "input": {
    "player_contribution": "policy-controlled bounded content"
  },
  "output": {
    "status": "committed|failed|cancelled|deadline_exceeded|conflict",
    "story_text": "policy-controlled bounded content or absent",
    "error_code": "stable code or absent",
    "failure_kind": "stable kind or absent",
    "stage": "stable stage or absent"
  },
  "metadata": {
    "story_id": "bound after session resolution",
    "turn_number": "bound after story load",
    "idempotency_key_digest": "sha256 hex; raw key forbidden",
    "replayed": false,
    "schema_version": "2"
  }
}
```

`langfuse.session.id` MUST be set only after a valid application Session resolves. Root Trace ID MUST be the random OpenTelemetry 128-bit Trace ID.

### 3.4 Late-Bound Attribute API

Use the Phase 0 methods at these exact points:

```rust
trace.bind_session(session_id.as_str(), story_id.as_str());
trace.bind_request(request.request_digest().as_str());
trace.bind_turn(turn_number.get());
```

Terminal metadata MUST be supplied through `ObservationFinish`:

```rust
ObservationFinish {
    status,
    metadata: vec![
        ObservationAttribute::bool("replayed", replayed),
        ObservationAttribute::string("terminal_status", terminal_status),
        ObservationAttribute::optional_string("failure_stage", failure_stage),
    ],
    output,
    error,
    usage: None,
    cost: None,
}
```

Binding MUST update the root Span, the current context, and later descendants. It MUST NOT rewrite already-ended descendants.

### 3.5 Pipeline Execution Contract

`TurnRuntime::execute` MUST map `TurnStage` to `ObservationStep` through one total function:

```rust
pub const fn observation_step(stage: TurnStage) -> ObservationStep {
    match stage {
        TurnStage::TurnInitializer => ObservationStep::InitializeTurn,
        TurnStage::BaselineBuilder => ObservationStep::PrepareContext,
        TurnStage::WriterPlanner => ObservationStep::PlanTurn,
        TurnStage::ContextRetrieval => ObservationStep::RetrieveContext,
        TurnStage::CharacterThink => ObservationStep::ThinkCharacters,
        TurnStage::StoryGenerator => ObservationStep::GenerateStory,
        TurnStage::StoryStateExtractor => ObservationStep::ExtractStoryState,
        TurnStage::Validation => ObservationStep::ValidateStory,
        TurnStage::StoryRepairer => ObservationStep::RepairStory,
        TurnStage::TurnCommitter => ObservationStep::CommitTurn,
        TurnStage::Context => ObservationStep::PrepareContext,
    }
}
```

`TurnRuntime::run` MUST be wrapped by `RunTurnPipelines`. `TurnRuntime::execute` MUST wrap each actual `pipeline.execute(ctx)` call with the mapped step and MUST preserve the existing entry/exit phase validation and event behavior.

Skipped conditions MUST be recorded on `RunTurnPipelines`:

```json
{
  "retrieval_skipped": true,
  "character_thinking_skipped": true,
  "skip_reason": {
    "retrieval": "not_required",
    "character_thinking": "not_required"
  }
}
```

No child span may be created for a skipped stage.

### 3.6 Nested Pipeline Contracts

The following nested operations MUST create child Observations:

| Parent | Child | Required metadata |
|---|---|---|
| `PrepareContext` | `LoadStorySnapshot` | `story_id`, snapshot revision, role, relationship, constraint, and graph counts |
| `PrepareContext` | `ActivateWorldInfo` | request count, candidate count, activated count, skipped reason |
| `ActivateWorldInfo` | `ProjectNarrative` | graph revision, active node IDs and counts, intent, impulse, and effect counts |
| `PlanTurn` | `GenerateWriterPlan` | Generation fields from §3.7 |
| `ThinkCharacters` | one `ThinkCharacter` per actual completion | `character_id`, `attempt` |
| `GenerateStory` | `DraftStoryText` | Generation fields from §3.7 |
| `ExtractStoryState` | `InferStoryState` | Generation fields from §3.7 |
| `RepairStory` | `ReviseStoryText` | Generation fields plus `correction_round` |
| `CommitTurn` | `PersistTurn` | `story_id`, `turn_number`, commit status |

If an operation is not present as a distinct function boundary, instrumentation MUST be placed at the narrowest existing boundary that owns its result. Instrumentation MUST NOT duplicate the business operation.

Every currently executed non-Generation Observation MUST emit policy-controlled bounded input and output:

- Submission and engine nodes emit request validation, admission, coordination, story lookup, and idempotency outcomes.
- Pipeline Chain nodes emit only phase transitions and bounded domain summaries.
- Retriever nodes emit query keys and result IDs/counts without private content.
- `CommitTurn` and `PersistTurn` emit changeset and storage summaries without story text.
- Generation nodes exclusively own prompts and raw provider outputs.
- Character memories, private rumors, and character thoughts MUST NOT appear in non-Generation input or output.

### 3.7 LLM Generation Protocol

Every `LlmGateway` completion attempt MUST emit one Generation Observation. It MUST use the caller-provided `ObservationStep`; no purpose-to-name fallback is allowed.

Add the step to gateway request metadata:

```rust
#[derive(Debug, Clone, Copy)]
pub struct LlmObservation {
    pub step: ObservationStep,
    pub attempt: u32,
    pub correction_round: Option<u32>,
    pub character_id: Option<&str>,
}
```

Each Generation MUST export this shape:

```json
{
  "model": "provider model name",
  "model_parameters": {
    "temperature": 0.0,
    "max_tokens": 0,
    "thinking": "enabled|disabled|provider_default"
  },
  "usage_details": {
    "input": 0,
    "input_cached_tokens": 0,
    "output": 0,
    "output_reasoning_tokens": 0,
    "total": 0
  },
  "cost_details": {
    "currency": "USD",
    "input": 0.0,
    "input_cached_tokens": 0.0,
    "output": 0.0,
    "output_reasoning_tokens": 0.0,
    "total": 0.0
  },
  "metadata": {
    "provider": "string",
    "call_id": "string",
    "attempt": 1,
    "correction_round": null,
    "queue_wait_ms": 0,
    "provider_latency_ms": 0,
    "total_latency_ms": 0,
    "usage_accuracy": "exact|estimated",
    "finish_reason": "stable value",
    "reasoning_content_available": false
  }
}
```

Usage formulas are:

```text
input = input_tokens - min(input_cached_tokens, input_tokens)
input_cached_tokens = min(input_cached_tokens, input_tokens)
output = output_tokens - min(reasoning_tokens, output_tokens)
output_reasoning_tokens = min(reasoning_tokens, output_tokens)
total = input + input_cached_tokens + output + output_reasoning_tokens
```

Input MUST be the bounded request messages or prompt representation. Output MUST be the bounded provider response. `metadata_only` MUST emit neither. Reasoning text MUST be absent unless content policy permits it; reasoning token count MUST remain available.

### 3.8 Error Mapping

Map terminal kinds exactly:

| Business condition | `ObservationStatus` | Langfuse level |
|---|---|---|
| Success or replay | `Ok` | `DEFAULT` |
| Business failure | `Error` | `ERROR` |
| Client cancellation | `Cancelled` | `WARNING` |
| Turn/provider deadline | `DeadlineExceeded` | `ERROR` |
| Idempotency/commit conflict | `Conflict` | `WARNING` |
| Dropped unfinished span | `Incomplete` | `WARNING` |

Every error Observation MUST set:

```json
{
  "otel.status": "ERROR",
  "langfuse.observation.level": "ERROR|WARNING",
  "langfuse.observation.status_message": "bounded masked message",
  "langfuse.observation.metadata.error_code": "stable code",
  "langfuse.observation.metadata.failure_kind": "stable kind",
  "langfuse.observation.metadata.stage": "stable stage or absent"
}
```

Raw provider payloads, credentials, raw idempotency keys, and unbounded error chains are forbidden.

### 3.9 Canonical Runtime Tree

For a path that executes all available stages, the exported parent-child protocol is:

```text
execute-story-turn
├── resolve-interaction-session
├── validate-request
├── admit-turn-task
├── coordinate-story-turn
├── load-story
├── check-idempotency
└── run-turn-pipelines
    ├── initialize-turn
    ├── prepare-context
    │   ├── load-story-snapshot
    │   └── activate-world-info
    │       └── project-narrative
    ├── plan-turn
    │   └── generate-writer-plan
    ├── retrieve-context
    ├── think-characters
    │   └── think-character × N
    ├── generate-story
    │   └── draft-story-text
    ├── extract-story-state
    │   └── infer-story-state
    ├── validate-story
    ├── repair-story
    │   └── revise-story-text
    └── commit-turn
        └── persist-turn
```

The current runtime does not execute `ExtractStoryState`, `ValidateStory`, or `RepairStory`; those nodes MUST be absent until the business flow executes them.

### 3.10 Session and Story Identity

The final identity rules are:

```text
Story ID: persistent metadata, may span multiple Sessions
Application Session ID: Langfuse session.id, may contain multiple Turns
Turn attempt: one OpenTelemetry Trace
Observation: one actual operation within the attempt
```

`SessionRegistry` behavior MUST ensure a Session never binds to more than one Story. If existing `bind_story` behavior can change a Story, it MUST rotate to a new `SessionId`; reusing the old ID is forbidden.

### 3.11 Files Changed

Phase 1 MUST update at least these boundaries:

```text
crates/aise-server/src/turn_submission/service.rs
crates/aise-server/src/api/turn.rs
crates/aise-server/src/api/state.rs
crates/aise-server/src/session/
crates/aise/src/engine.rs
crates/aise/src/runtime/turn_runtime.rs
crates/aise/src/turn/turn_context.rs
crates/aise/src/llm/gateway.rs
crates/aise/src/context/
crates/aise/src/planning/
crates/aise/src/character/
crates/aise/src/story/
crates/aise/src/validation/
crates/aise/src/persistence/
```

Instrumentation helpers MAY be added within the owning module directory but MUST NOT create cross-layer imports from `aise` into `aise-server`.

---

## 4. Behavior Rules

1. **P1-R1**: One call to `TurnSubmissionService::submit` after successful transport extraction MUST create exactly one root Trace.
2. **P1-R2**: Invalid or missing Session, invalid request, missing or invalid idempotency key, and admission failure MUST end that root with a diagnosable error.
3. **P1-R3**: Invalid/missing Session MUST NOT set `langfuse.session.id`.
4. **P1-R4**: Admission success MUST move root ownership once into the background task; no clone may create a second lifecycle owner.
5. **P1-R5**: A Turn attempt MUST keep the same Trace ID through success, failure, cancellation, deadline, conflict, and replay.
6. **P1-R6**: A replay MUST emit `CheckIdempotency`, set `replayed=true`, finish successfully, and MUST NOT emit `RunTurnPipelines`.
7. **P1-R7**: Every executed Pipeline MUST be a child of `RunTurnPipelines`.
8. **P1-R8**: Every LLM completion attempt MUST be a distinct Generation child of its owning Pipeline.
9. **P1-R9**: Retry attempt and repair round MUST be numeric metadata and MUST NOT alter the stable name.
10. **P1-R10**: Character ID, Story ID, Turn number, model, provider, and call ID MUST be fields, never names.
11. **P1-R11**: Conditional skips MUST create no child and MUST record a stable skip flag and reason on the parent.
12. **P1-R12**: A business `Result` before and after instrumentation MUST compare equal for the same deterministic test fixture.
13. **P1-R13**: Telemetry failure MUST not consume a Turn retry, LLM retry, repair round, or token budget.
14. **P1-R14**: The actual LLM limiter MUST remain outside and authoritative over every provider call.
15. **P1-R15**: Generation usage MUST use mutually exclusive buckets and saturating subtraction.
16. **P1-R16**: Cost MUST be omitted unless the charge is confirmed in USD with compatible buckets.
17. **P1-R17**: `story_id` MUST be present on the root after Session resolution; `turn_number` MUST be present after allocation.
18. **P1-R18**: Late-bound fields MUST propagate only to the root, current Observation, and descendants created afterward.
19. **P1-R19**: Normal business logs MUST NOT become descendants in Langfuse.
20. **P1-R20**: Current commented Validation/Repair code MUST produce zero Observations.
21. **P1-R21**: A Session ID MUST never group Traces with different `story_id` values.
22. **P1-R22**: Observation content MUST obey the Phase 0 content policy and byte limits.

### 4.1 Error Handling

- `TurnSubmissionError` MUST map to the existing API status without exposing internal telemetry errors.
- Errors MUST preserve the existing stable business `code()` and terminal kind.
- Root output for failure MUST include only bounded status, code, kind, and stage.
- Span finish failure, serialization failure, and exporter failure MUST be diagnostic-only.

### 4.2 Concurrency

- Root context MUST be explicitly attached to the spawned Turn task before engine execution.
- Each Pipeline future and LLM future MUST be instrumented; no thread-local-only assumption is allowed.
- Parallel character calls MUST each inherit the `ThinkCharacters` parent and retain independent Generation spans.
- No Observation lifecycle object may be stored in global state, `AiseEngine`, a Pipeline instance, or across Turns.

### 4.3 Observability

- All Observation spans MUST use target `aise::observation`.
- Root resource and trace attributes MUST include service, environment, release, and schema version.
- Every Generation MUST include model name, provider, attempt, latency, finish status, and usage when available.
- Errors MUST include structured `error_code`, `failure_kind`, and `stage`.

---

## 5. Acceptance Criteria

- [ ] Registry snapshot matches §3.1 exactly.
- [ ] In-memory exporter test for a successful currently executable Turn matches §3.9 with non-executed Validation/Repair nodes absent.
- [ ] `success`, `invalid_session`, `invalid_request`, `missing_idempotency_key`, `admission_failure`, `cancelled`, `deadline`, `conflict`, and `replayed` tests each export exactly one root.
- [ ] Malformed JSON, invalid path extraction, and invalid content type tests export zero Turn roots.
- [ ] HTTP and direct service callers both enter through `TurnSubmissionService`.
- [ ] Two Turns in one application Session share one Langfuse Session ID.
- [ ] Two Sessions for one Story have different Session IDs and identical `story_id` metadata.
- [ ] Session Story switching rotates Session ID; a test proves no Session contains two Story IDs.
- [ ] Invalid/missing Session traces have no `langfuse.session.id`.
- [ ] Parallel character test proves all Generation spans have `ThinkCharacters` as parent.
- [ ] LLM retry test proves one Generation per attempt with a stable name and distinct `attempt`.
- [ ] Usage bucket tests cover cached input and reasoning output without double counting.
- [ ] Late-binding test proves ended early spans are unchanged and later spans receive new fields.
- [ ] `metadata_only` tests find no prompt, response, or reasoning text.
- [ ] Production `full_content` test exports metadata only.
- [ ] Telemetry-disabled and exporter-failing fixtures produce the same business result as the baseline fixture.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo test --workspace` passes.

---

## 6. Out of Scope / Future Work

- Restoring the Validation/Repair loop requires a separate business-flow spec; the existing Observation registry is ready for it.
- Legacy deletion and final performance/deployment acceptance are specified in [Phase 2](./2026-09-23-langfuse-trace-system-spec-phase-2-gpt.md).

---

## 7. References

- Source design: [Langfuse Trace 系统重构](../../design/2026-09-23-langfuse-trace-system-design-gpt.md)
- Phase 0 contracts: [Phase 0](./2026-09-23-langfuse-trace-system-spec-phase-0-gpt.md)
- Guardrails: [Agent guardrails](../../agents/README.md)
- Langfuse best practices: <https://langfuse.com/docs/observability/best-practices>
