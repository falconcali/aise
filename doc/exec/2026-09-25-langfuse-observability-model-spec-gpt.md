# Langfuse Observability Model Alignment — Spec

> **Model**: GPT-5.6 Sol
> **Date**: 2026-09-25
> **Status**: Proposed
> **Source Design**: [Langfuse Trace 系统重构](../design/2026-09-23-langfuse-trace-system-design-gpt.md)
> **Source Refactor**: [Langfuse Observability 数据模型对齐](../refactor/2026-09-25-langfuse-observability-model-refactor-gpt.md)
> **Phase**: N/A — single hard-refactor merge

---

## 1. Goal

Replace `turn::observability` with explicit top-level `ObservationSession`, `Trace`, and `Observation` lifecycles that match the Langfuse data model, pass parents explicitly through Turn execution, and move all business-specific observation assembly into owning-module facades.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Add the business-agnostic `aise::observability` leaf module.
- Model one application interaction as an `ObservationSession`, one Turn attempt as one `Trace`, and every Pipeline, LLM, Retriever, Tool, or Evaluator operation as an `Observation`.
- Change `TurnExecutionPipeline::execute` to receive an explicit `&Observation` parent separately from `&mut TurnExecutionContext`.
- Add module-owned `observability/` facades for submission, engine, runtime, context, planning, LLM, character, story, validation, and persistence.
- Move stable names, business metadata keys, content projection, status mapping, and error mapping out of the top-level observability module.
- Remove hidden parent propagation through `Span::current()`, `TurnExecutionContext`, `TurnLlmCallScope`, and the global `ObservationStep` registry.
- Remove the `ObservationTrace` oneshot transfer and keep each Trace owned by the task that executes and finishes the Turn.
- Increment the exported Observation schema version from `1` to `2`.
- Delete the superseded `turn::observability` implementation, compatibility imports, tests, and documentation in the same merge.

### 2.2 Non-Goals

- Does not change HTTP or SSE business protocols.
- Does not change database schemas, persistence semantics, prompts, Turn budgets, retry policies, or LLM concurrency limits.
- Does not migrate historical Langfuse data.
- Does not introduce Langfuse prompt management, datasets, scores, evaluators, metrics export, or log export.
- Does not restore business stages that are not executed by the current `TurnRuntime`.
- Does not redesign the existing OpenTelemetry exporter, batching, masking, sampling, or environment-variable contract except where imports must follow the new top-level types.
- Does not add a runtime rollback flag, compatibility adapter, dual instrumentation path, or `Span::current()` fallback.

### 2.3 Implementation Constraints

- This spec generates final-form code. Do **not** keep fallback paths, compatibility shims, re-export aliases, or dual-write logic.
- Delete every superseded type, function, module, test, config reference, and document claim in the same merged change.
- Intermediate commits may be incomplete, but the final merge MUST compile with only the new path.
- `mod.rs` and `lib.rs` MUST contain declarations, re-exports, and attributes only.
- Unit tests MUST live in `tests/<source>_tests.rs`; do not add inline test bodies.
- Rust code added or rewritten by this change MUST contain no comments or doc comments.
- `observability` MAY depend only on std, serde, tracing, OpenTelemetry, and other external telemetry libraries.
- `domain` and `config` MUST NOT depend on `observability`.
- Business orchestration files MUST NOT construct `ObservationSpec`, `ObservationOutcome`, protocol keys, usage JSON, cost JSON, or telemetry error mappings directly.
- Every LLM completion, streaming call, and embedding call MUST continue through the shared injected concurrency limiter.

---

## 3. Contracts

### 3.1 Top-Level Model Types

Implement these contracts under `crates/aise/src/observability/`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObservationKind {
    Chain,
    Span,
    Generation,
    Retriever,
    Tool,
    Evaluator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ObservationStatus {
    Ok,
    Error,
    Cancelled,
    DeadlineExceeded,
    Conflict,
    #[default]
    Incomplete,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttributeValue {
    String(String),
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(f64),
    StringList(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    pub key: &'static str,
    pub value: AttributeValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationError {
    pub code: String,
    pub failure_kind: String,
    pub stage: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct GenerationUsage {
    pub input: u64,
    pub input_cached_tokens: u64,
    pub output: u64,
    pub output_reasoning_tokens: u64,
    pub total: u64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct GenerationCost {
    pub currency: &'static str,
    pub input: Option<f64>,
    pub input_cached_tokens: Option<f64>,
    pub output: Option<f64>,
    pub output_reasoning_tokens: Option<f64>,
    pub total: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservabilityContentConfig {
    pub policy: ContentCapturePolicy,
    pub max_field_bytes: usize,
    pub max_observation_bytes: usize,
    pub detector_overlap_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedContent {
    pub json: String,
    pub original_bytes: usize,
    pub captured_bytes: usize,
    pub truncated: bool,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub struct SessionSpec {
    pub id: Option<String>,
    pub user_id: Option<String>,
    pub metadata: Vec<Attribute>,
}

#[derive(Debug, Clone)]
pub struct TraceSpec {
    pub name: &'static str,
    pub input: Option<BoundedContent>,
    pub metadata: Vec<Attribute>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ObservationSpec {
    pub name: &'static str,
    pub kind: ObservationKind,
    pub input: Option<BoundedContent>,
    pub metadata: Vec<Attribute>,
}

#[derive(Debug, Clone, Default)]
pub struct ObservationOutcome {
    pub status: ObservationStatus,
    pub metadata: Vec<Attribute>,
    pub output: Option<BoundedContent>,
    pub error: Option<ObservationError>,
    pub usage: Option<GenerationUsage>,
    pub cost: Option<GenerationCost>,
}

pub type TraceOutcome = ObservationOutcome;

#[derive(Debug, Clone, Default)]
pub struct SessionOutcome {
    pub status: ObservationStatus,
    pub metadata: Vec<Attribute>,
}
```

`Attribute` is a transport-neutral value carrier. Top-level `observability` MUST define only Langfuse/OpenTelemetry protocol keys. Story, Turn, character, Pipeline, retry, retrieval, and persistence keys MUST be constants in the owning business module's `observability/`.

### 3.2 Content Capture API

Replace `ContentCaptureLimits` with `ObservabilityContentConfig` and replace direct encoder storage in Turn state with this top-level API:

```rust
#[derive(Clone)]
pub struct ContentCapture {
    config: ObservabilityContentConfig,
}

impl ContentCapture {
    pub fn new(config: ObservabilityContentConfig) -> Self;

    pub fn encode<T: serde::Serialize>(
        &self,
        value: &T,
        remaining_observation_bytes: usize,
    ) -> CaptureResult;

    pub fn max_observation_bytes(&self) -> usize;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureResult {
    pub content: Option<BoundedContent>,
    pub encode_failed: bool,
}
```

`ContentCapture::encode` MUST return `content = None` for `MetadataOnly`, MUST serialize directly into a bounded UTF-8-safe writer, and MUST NOT first allocate an unbounded JSON string. Each Observation has an independent `max_observation_bytes` budget.

### 3.3 Session, Trace, and Observation Lifecycles

Implement these lifecycle APIs:

```rust
pub struct ObservationSession {
    context: SessionContext,
    content: ContentCapture,
    finished: bool,
}

pub struct Trace {
    root: Observation,
    context: TraceContext,
    finished: bool,
}

pub struct Observation {
    span: tracing::Span,
    context: opentelemetry::Context,
    content: ContentCapture,
    finished: bool,
}

impl ObservationSession {
    pub fn begin(spec: SessionSpec, content: ContentCapture) -> Self;
    pub fn begin_trace(&self, spec: TraceSpec) -> Trace;
    pub fn finish(self, outcome: SessionOutcome);
}

impl Trace {
    pub fn root(&self) -> &Observation;
    pub fn begin_observation(&self, spec: ObservationSpec) -> Observation;
    pub fn bind(&mut self, attributes: Vec<Attribute>);
    pub fn content_capture(&self) -> &ContentCapture;
    pub async fn trace<F: std::future::Future>(&self, future: F) -> F::Output;
    pub fn finish(self, outcome: TraceOutcome);
}

impl Observation {
    pub fn begin(&self, spec: ObservationSpec) -> Observation;
    pub fn content_capture(&self) -> &ContentCapture;
    pub async fn trace<F: std::future::Future>(&self, future: F) -> F::Output;
    pub fn finish(self, outcome: ObservationOutcome);
    pub fn is_recording(&self) -> bool;
}
```

`SessionContext` and `TraceContext` are private implementation types. No public method may return `tracing::Span` or `opentelemetry::Context`.

`ObservationSession` MUST NOT create a tracing or OpenTelemetry Span. `Trace` MUST create exactly one root Observation and one OpenTelemetry Trace ID. `Observation::begin` MUST use the receiver's stored OpenTelemetry context as the explicit parent and MUST NOT query `Span::current()`.

`finish` consumes its lifecycle owner. Dropping an unfinished `Trace` or `Observation` MUST record `Incomplete` exactly once. Dropping an unfinished `ObservationSession` MUST emit bounded structured diagnostics but MUST NOT export a Session Observation.

### 3.4 Generic Trace Binding

`Trace::bind` replaces `bind_session`, `bind_request`, and `bind_turn`. It MUST:

```text
1. Record each supplied attribute on the root Observation.
2. Update the Trace's immutable current OpenTelemetry context.
3. Propagate only allowlisted trace attributes to Observations created afterward.
4. Never rewrite an Observation that has already ended.
5. Never call Span::current().
```

The fixed propagation allowlist remains:

```text
aise.trace.name
aise.trace.tags
aise.trace.environment
aise.trace.release
aise.schema.version
langfuse.session.id
aise.trace.metadata.story_id
aise.trace.metadata.idempotency_key_digest
aise.trace.metadata.turn_number
```

All newly emitted roots MUST set `aise.schema.version = "2"`.

### 3.5 Turn Execution Contracts

Replace the Pipeline trait with:

```rust
#[async_trait::async_trait]
pub trait TurnExecutionPipeline: Send + Sync {
    fn stage(&self) -> TurnStage;

    async fn execute(
        &self,
        ctx: &mut TurnExecutionContext,
        observation: &Observation,
    ) -> Result<(), TurnExecutionError>;
}
```

`TurnRuntime` MUST use:

```rust
impl TurnRuntime {
    pub async fn run(
        &self,
        ctx: &mut TurnExecutionContext,
        sink: &dyn TurnEventSink,
        trace: &Trace,
    ) -> Result<(), TurnExecutionError>;
}
```

`runtime::observability` MUST create `run-turn-pipelines` from `&Trace`, create each executed stage Observation from that parent, pass `&Observation` to the Pipeline, and finish the stage after the Pipeline result is known.

The Pipeline contract is strict:

```text
Turn business state: &mut TurnExecutionContext only
Observation parent: &Observation only
Persisted or cross-Turn state: neither object
```

`TurnExecutionContext` MUST NOT contain `Observation`, `Trace`, `opentelemetry::Context`, `tracing::Span`, `ContentCapture`, or an observation encoder.

### 3.6 Engine and Submission Contracts

Move `crates/aise/src/engine.rs` to directory layout:

```text
crates/aise/src/engine/
├── mod.rs
├── service.rs
├── observability/
│   ├── mod.rs
│   ├── turn.rs
│   └── tests/
└── tests/
```

The internal engine entry receives a borrowed Trace:

```rust
impl AiseEngine {
    pub async fn run_turn(
        &self,
        spec: ExecuteTurnSpec,
        sink: &dyn TurnEventSink,
        trace: &mut Trace,
    ) -> Result<CommittedTurnResult, TurnExecutionError>;

    pub async fn execute_turn(
        &self,
        spec: ExecuteTurnSpec,
        sink: &dyn TurnEventSink,
        trace: &mut Trace,
    ) -> TurnRunOutcome;
}
```

`TurnSubmissionService` MUST create the `ObservationSession` and Turn `Trace`, resolve and bind late attributes through `turn_submission::observability`, move the Trace into the admitted Turn task, call the engine with `&mut Trace`, and finish the Trace and local Session lifecycle in that same task.

Admission MUST use a reserve-then-spawn boundary so ownership is deterministic:

```rust
impl TurnTaskSupervisor {
    pub async fn reserve(
        &self,
        cancellation: &TurnCancellation,
    ) -> Result<TurnTaskPermit, TurnTaskAdmissionError>;

    pub fn spawn_reserved(
        &self,
        permit: TurnTaskPermit,
        task: TurnTaskSpec,
    );
}
```

`reserve` is wrapped by the admission Observation. `spawn_reserved` MUST be non-fallible after a valid permit is returned. The task future captures the Trace directly. No `oneshot::channel::<Trace>()`, `oneshot::channel::<ObservationTrace>()`, or equivalent delayed ownership channel is permitted.

### 3.7 LLM Contracts

Remove `TurnLlmCallScope::observation_step` and `observation_step_for_stage`. Every gateway operation receives its explicit parent separately:

```rust
impl LlmGateway {
    pub async fn complete(
        &self,
        scope: TurnLlmCallScope<'_>,
        spec: CompletionSpec,
        reservation: LlmBudgetReservation,
        parent: &Observation,
    ) -> Result<LlmCompletion, LlmError>;

    pub async fn complete_stream(
        &self,
        scope: TurnLlmCallScope<'_>,
        spec: CompletionSpec,
        reservation: LlmBudgetReservation,
        sink: DeltaSink,
        parent: &Observation,
    ) -> Result<LlmCompletion, LlmError>;

    pub async fn embed(
        &self,
        scope: TurnLlmCallScope<'_>,
        inputs: Vec<String>,
        reservation: LlmBudgetReservation,
        parent: &Observation,
    ) -> Result<EmbeddingOutput, LlmError>;
}
```

The composed completion methods MUST also take `parent: &Observation` and forward it unchanged. `llm::observability` owns the `LlmCallPurpose` to stable name/kind mapping, input/output capture, model fields, provider metadata, usage, cost, and provider error mapping. Completion and streaming provider attempts use `ObservationKind::Generation`; embeddings use the accurate Langfuse kind selected by `llm::observability`.

The shared limiter acquisition remains authoritative and MUST occur before provider dispatch. Observation creation MUST NOT bypass, duplicate, or widen that limiter.

### 3.8 Business Observability Facades

Each facade returns a semantic wrapper that hides generic specs and outcomes:

```rust
pub struct LoadStorySnapshotObservation {
    observation: Observation,
}

pub fn begin_load_story_snapshot(
    parent: &Observation,
    ctx: &TurnExecutionContext,
) -> LoadStorySnapshotObservation;

impl LoadStorySnapshotObservation {
    pub async fn trace<F: std::future::Future>(&self, future: F) -> F::Output;
    pub fn parent(&self) -> &Observation;
    pub fn finish(
        self,
        ctx: &TurnExecutionContext,
        outcome: &Result<StoryReadSnapshot, StoreError>,
    );
}
```

Every module-specific wrapper MUST follow this shape:

```text
begin_*: project business input into ObservationSpec
trace: instrument only the supplied future
parent: expose &Observation only when a nested operation needs it
finish: project the typed business result into ObservationOutcome
```

The final ownership registry is:

| Owner | Stable operations |
|---|---|
| `turn_submission::observability` | session lifecycle, execute-story-turn, resolve-interaction-session, validate-request, admit-turn-task, root terminal outcome |
| `engine::observability` | coordinate-story-turn, load-story, check-idempotency |
| `runtime::observability` | run-turn-pipelines and Pipeline stage boundaries |
| `context::observability` | prepare-context, load-story-snapshot, activate-world-info, retrieve-context |
| `planning::observability` | plan-turn, project-narrative, generate-writer-plan |
| `character::observability` | think-characters, think-character |
| `story::observability` | generate-story, draft-story-text, extract-story-state, infer-story-state, repair-story, revise-story-text |
| `validation::observability` | validate-story |
| `persistence::observability` | commit-turn, persist-turn |
| `llm::observability` | provider generation/embedding fields, usage, cost, provider errors |

Only an operation that actually executes may create an Observation. Skip flags and skip reasons belong to the owning parent facade.

### 3.9 Final Directory Layout

```text
crates/aise/src/observability/
├── mod.rs
├── session.rs
├── trace.rs
├── observation.rs
├── model.rs
├── content.rs
└── tests/
    ├── session_tests.rs
    ├── trace_tests.rs
    ├── observation_tests.rs
    └── content_tests.rs

crates/aise/src/runtime/observability/
├── mod.rs
├── turn.rs
├── stage.rs
└── tests/

crates/aise/src/context/observability/
├── mod.rs
├── baseline.rs
├── retrieval.rs
└── tests/

crates/aise/src/llm/observability/
├── mod.rs
├── generation.rs
├── embedding.rs
└── tests/
```

`planning`, `character`, `story`, `validation`, `persistence`, `engine`, and `aise-server/src/turn_submission` MUST use the same directory-entry layout. `crates/aise-server/src/observability/` remains exporter/runtime infrastructure and MUST NOT own business names or Turn outcome mapping.

### 3.10 Mandatory Deletions and Renames

Delete:

```text
crates/aise/src/turn/observability/
crates/aise/src/context/baseline_observation.rs
ObservationStep
ObservationStep::ALL
observe_result
TurnExecutionContext.observation_encoder
TurnLlmCallScope.observation_step
observation_step_for_stage
LlmObservation
ObservationTrace ownership oneshot channel
```

Apply these final renames without aliases:

```text
ObservationTrace -> Trace
ObservationSpan -> Observation
ObservationFields -> ObservationSpec
ObservationFinish -> ObservationOutcome
ContentCaptureLimits -> ObservabilityContentConfig
```

---

## 4. Behavior Rules

1. **OM-R1**: One application interaction Session MAY own multiple Turn Traces; each Turn attempt MUST own exactly one distinct Trace ID.
2. **OM-R2**: `ObservationSession` MUST export zero OpenTelemetry Spans.
3. **OM-R3**: A Pipeline stage, Job, LLM call, Retriever, Tool, or Evaluator MUST be an Observation, not a Trace.
4. **OM-R4**: A child Observation MUST derive its parent only from the supplied `&Trace` or `&Observation`.
5. **OM-R5**: `Span::current()`, thread-local current-span lookup, global state, and `TurnExecutionContext` MUST NOT be used to infer an Observation parent.
6. **OM-R6**: `Trace` and `Observation` MUST NOT expose raw tracing or OpenTelemetry handles outside top-level `observability`.
7. **OM-R7**: `finish` MUST consume the lifecycle owner; explicit finish and `Drop` MUST never emit two terminal states.
8. **OM-R8**: Dropping an unfinished Trace or Observation MUST record `Incomplete` with bounded diagnostics and MUST NOT panic.
9. **OM-R9**: A Session finish or drop MUST NOT create a synthetic Session Span.
10. **OM-R10**: Top-level `observability` MUST contain no Story, Turn, character, Pipeline, retry, or persistence names, keys, DTOs, or errors.
11. **OM-R11**: A business orchestration file MAY begin, trace, pass, and finish a semantic facade; it MUST NOT assemble generic fields or telemetry outcomes.
12. **OM-R12**: `TurnExecutionContext` is the only Pipeline business-state exchange channel; `Observation` MUST carry no business state.
13. **OM-R13**: An Observation MUST NOT be persisted, stored globally, shared across Turns, or retained beyond its operation.
14. **OM-R14**: The Trace owner MUST remain in the admitted Turn task from creation through terminal finish.
15. **OM-R15**: A valid task permit MUST be acquired before the task future captures the Trace; no ownership-transfer channel is allowed.
16. **OM-R16**: Late-bound attributes MUST update the root and future descendants only; ended Observations MUST remain unchanged.
17. **OM-R17**: Every root and descendant MUST use schema version `2`.
18. **OM-R18**: Existing environment, release, Session ID, Story ID, request digest, Turn number, usage, cost, error, and content-policy semantics MUST remain observable after migration.
19. **OM-R19**: Conditional stages MUST create no child Observation and MUST record their stable skip facts on the owning parent.
20. **OM-R20**: Telemetry disabled, queue-full, exporter-failing, and serialization-failing paths MUST return the same business result as telemetry-enabled success.
21. **OM-R21**: No write lock may be held across `.await`, Observation finish, channel send, event emission, diagnostics, or I/O.
22. **OM-R22**: Content, queues, contexts, and lifecycles MUST retain their existing hard bounds; this refactor MUST add no queue, cache, history, or fan-out.
23. **OM-R23**: Every actual LLM call MUST remain inside both the shared concurrency limiter and a typed child Observation.
24. **OM-R24**: Validation/Repair loops MUST remain bounded by the existing Turn budget and MUST fail diagnostically on exhaustion.
25. **OM-R25**: Character thoughts MUST remain viewpoint data and MUST NOT be committed as world facts by observability projection or story generation.

### 4.1 Error Handling

- Turn/domain APIs MUST continue returning typed errors and MUST NOT expose `anyhow::Error`.
- Business facades MUST map typed errors to `ObservationError` without changing the original `Result`.
- Error metadata MUST include stable `code`, `failure_kind`, optional `stage`, and a bounded message.
- Telemetry serialization, finish, exporter, and Session lifecycle diagnostics MUST be non-fatal.
- Invalid usage or cost payloads MUST be omitted and diagnosed; they MUST NOT fail the business operation.
- Identifiers and errors in logs MUST use structured fields and MUST NOT be interpolated into message strings.

### 4.2 Concurrency

- A lifecycle object has one owner; `Trace` and `Observation` MUST NOT implement `Clone`.
- `&Observation` MAY be shared by concurrent child futures only for the duration of their common parent.
- Parallel character calls MUST create independent children from the same explicit parent and retain correct parent IDs.
- `Observation::trace` and `Trace::trace` MUST use `tracing::Instrument`; no `span.enter()` guard may cross `.await`.
- Export remains non-blocking on the business path and bounded by the existing SDK batch queue.
- LLM calls MUST acquire the existing application-root limiter before provider dispatch.

### 4.3 Observability

- Observation spans MUST use target `aise::observation`.
- Telemetry self-diagnostics MUST use target `aise::telemetry` and MUST not re-enter OpenTelemetry.
- Stable display names and kinds are owned and snapshot-tested by their business modules.
- Schema version `2` distinguishes the new Session/Trace/Observation model from version `1`.
- The canonical tree root is one Turn Trace; `run-turn-pipelines` and every executed stage are nested Observations.

---

## 5. Acceptance Criteria

- [ ] `aise::observability` publicly exposes `ObservationSession`, `Trace`, `Observation`, model types, and bounded content types.
- [ ] `crates/aise/src/observability/` imports no internal AISE module — verified by `cargo test -p aise domain_core_dependency_tests`.
- [ ] `ObservationSession` exports no Span — verified by `session_finish_exports_zero_observations`.
- [ ] Two Traces from one Session share the Session ID and have distinct Trace IDs — verified by `session_groups_distinct_turn_traces`.
- [ ] Recursive explicit parenting produces the expected parent IDs — verified by `observation_begin_uses_explicit_parent_context`.
- [ ] Cross-task explicit parenting remains correct — verified by `trace_parent_survives_task_move`.
- [ ] Explicit finish and `Drop` each emit one terminal state — verified by `observation_finish_is_exactly_once` and `unfinished_observation_is_incomplete`.
- [ ] `TurnExecutionPipeline::execute` matches §3.5 and every production implementation and test double compiles with it.
- [ ] `TurnExecutionContext` and `TurnLlmCallScope` contain none of the forbidden observability state from §3.5 and §3.10.
- [ ] `TurnSubmissionService` contains no Trace-transfer oneshot and a reservation failure finishes the same root Trace diagnostically.
- [ ] Every LLM completion, stream, and embedding test proves explicit parentage and shared-limiter acquisition.
- [ ] Business orchestration outside `**/observability/**` contains no generic assembly — `rg 'ObservationSpec|ObservationOutcome|Attribute::|aise\.observation\.|langfuse\.' crates/aise/src crates/aise-server/src -g '!**/observability/**'` returns zero matches.
- [ ] Old symbols are gone — `rg 'turn::observability|ObservationTrace|ObservationSpan|ObservationStep|ObservationFields|ObservationFinish|ContentCaptureLimits|observation_step_for_stage' crates/` returns zero matches.
- [ ] Hidden parent inference is gone — `rg 'Span::current\(\)' crates/aise/src crates/aise-server/src` returns zero matches in Observation lifecycle and business facade code.
- [ ] `crates/aise/src/turn/observability/` and `crates/aise/src/context/baseline_observation.rs` do not exist.
- [ ] Every added `mod.rs` and `lib.rs` is index-only.
- [ ] In-memory exporter tests cover success, replay, semantic early failure, admission failure, cancellation, deadline, conflict, and incomplete drop.
- [ ] A full executable Turn exports one Trace with the expected nested Observation tree and no synthetic Session node.
- [ ] Exporter-disabled, blocked, queue-full, and unreachable fixtures preserve the baseline business result and do not wait for network progress.
- [ ] Langfuse smoke verification groups multiple Turns by Session while retaining one distinct Trace per Turn.
- [ ] New roots export `aise.schema.version = "2"`.
- [ ] `AGENTS.md`, dependency guardrails, the architecture design, and the prior Langfuse design are synchronized with the final contract.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo test --workspace` passes.

---

## 6. Out of Scope / Future Work

- Historical Langfuse data remains under schema version `1`; no migration is planned.
- Production content-enabled evaluation still requires separate data classification, retention, and access-control review.
- A future OpenTelemetry Collector may replace the endpoint topology without changing these lifecycle contracts.

---

## 7. References

- Source design: [Langfuse Trace 系统重构](../design/2026-09-23-langfuse-trace-system-design-gpt.md)
- Source refactor: [Langfuse Observability 数据模型对齐](../refactor/2026-09-25-langfuse-observability-model-refactor-gpt.md)
- Prior phased specs: [Langfuse Trace System Phase 0](./langfuse-trace-system-spec/2026-09-23-langfuse-trace-system-spec-phase-0-gpt.md), [Phase 1](./langfuse-trace-system-spec/2026-09-23-langfuse-trace-system-spec-phase-1-gpt.md), [Phase 2](./langfuse-trace-system-spec/2026-09-23-langfuse-trace-system-spec-phase-2-gpt.md)
- Architecture: [AISE Architecture](../design/2026-08-04-Architecture-gpt.md)
- Guardrails: [Agent guardrails](../agents/README.md)
- Langfuse data model: <https://langfuse.com/docs/observability/data-model>
