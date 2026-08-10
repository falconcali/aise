# Turn Runtime Review Remediation — Spec

> **Model**: GPT-5
> **Date**: 2026-08-06
> **Status**: Proposed
> **Source Design**: [AISE Technical Architecture v3.1](../design/2026-08-04-Architecture-gpt.md)
> **Source Review**: [Turn Runtime Code Review](../review/2026-08-05-Turn-Runtime-Code-Review-gpt.md)
> **Supersedes for remediation work**: [Turn Runtime Codegen Spec v1.0](./2026-08004-Turn-Runtime-Codegen-Spec-gpt.md)
> **Reviewed baseline**: `main@bf653e5439ff04a563d50fb8be8f8492a6bd8bee`
> **Phase**: Review remediation

---

## 1. Goal

Close every P1 and P2 defect identified by the Turn Runtime review so that terminal delivery, trust boundaries, resource limits, production lifecycle, authoritative Story state, LLM accounting, and recovery are mechanically verifiable end to end.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Validate the complete `ExecuteTurnSpec` before coordinator, Store, Trace, task, or Story side effects.
- Route every post-validation Engine exit through one finalizer that normalizes errors, sets exactly one Context terminal phase, emits exactly one terminal event, and closes Trace.
- Make idempotency replay observable through SSE and recoverable through a persistent result API.
- Split turn, Gateway, Provider, Store port, and SQLite adapter errors while preserving one-way dependencies.
- Seal `ValidatedChangeSet`, make `ValidationResult` structurally consistent, and enforce permission, domain, knowledge, and player-control validators.
- Require verifiable evidence for proposed World Facts and reject Character Thought as authority.
- Enforce count, byte, token, protocol-buffer, queue, trace, and retention limits from one validated configuration tree.
- Make the LLM Gateway transaction cover pre-dispatch rejection, reservation, quota, queueing, provider execution, settlement, Trace, and release.
- Parse OpenAI-compatible streaming usage and finish reasons and classify HTTP/provider failures.
- Persist a bounded per-call usage and charge ledger atomically with the committed Turn result.
- Add graceful shutdown, a service cancellation tree, bounded Turn admission, a single-owner task supervisor, and bounded asynchronous Trace I/O.
- Persist and atomically load/commit Story instructions, configuration, current scene, authoritative summary, and active constraints.
- Separate persistent Story APIs from ephemeral Session binding.
- Validate all configuration at startup and enforce the workspace MSRV and all-features CI commands.

### 2.2 Non-Goals

- Does not change the fixed eight-stage `TurnRuntime` topology or permit Pipelines to call one another.
- Does not add multi-instance distributed Story coordination, leases, or fencing tokens.
- Does not implement full Event Sourcing; state tables remain authoritative and canonical events remain the audit log.
- Does not add provisional token streaming, advanced retrieval, Lore Book, Narrative Graph, or multi-character fan-out.
- Does not add automatic LLM retries.
- Does not redesign Prompt assets except to remove sensitive content from parse errors and Trace payloads.
- Does not preserve automatic Story creation from Turn execution.
- Does not retain compatibility endpoints, legacy constructors, dual schemas, or dual-write behavior.

### 2.3 Implementation Constraints

- Implement the final form in one remediation change. Do not retain fallback paths, compatibility shims, adapter bridges, dual-write logic, deprecated constructors, or dead flags (`R-REFACTOR-01`, `R-REFACTOR-02`).
- Preserve the already-correct fixed `TurnPipelineSet`, bounded Repair loop, Story-level serialization, revision CAS, idempotent transaction, Outbox atomicity, deterministic recent-Turn ordering, and `StateChange::Unchanged` semantics.
- `turn` and `domain` must not import `llm`, `persistence`, `runtime`, Pipeline modules, adapters, or `aise-server` (`R-LAYER-01`).
- Every runtime object and background task must have one owner, a bounded lifetime, and a shutdown path (`R-ARCH-02`).
- No `MutexGuard` or `RwLockGuard` may cross `.await`, channel send, event emission, or I/O (`R-CONC-01`, `R-CONC-03`).
- Every completion, streaming, embedding, and narrative-validation call must use the injected shared `LlmGateway` limiter (`R-CONC-04`).
- `mod.rs` and `lib.rs` remain index-only; tests use dedicated `tests/<source>_tests.rs`; generated code contains no ordinary comments; configuration uses typed `*Config` types (`R-CODE-01`, `R-CODE-02`, `R-CODE-05`, `R-CODE-06`).
- Use existing workspace dependencies where possible. Any new dependency requires an MSRV, license, maintenance, and footprint justification (`R-DEP-01`).

### 2.4 Required Implementation Order

1. Terminal and recovery boundary: request validation, error normalization, Context terminal phases, Engine finalizer, SSE terminal delivery, result lookup.
2. Trust and resource boundary: layered errors, sealed validation, deterministic validators, Fact evidence, Context/Snapshot/Retrieval limits.
3. Gateway and production lifecycle: call transaction, streaming protocol, usage ledger, content policy, task supervisor, shutdown, Trace writer.
4. Authoritative Story state: schema migration, atomic Snapshot/Commit, Story API, Session binding.
5. Startup and delivery verification: configuration validation, CI, static dependency checks, full test matrix.

No later item may weaken or bypass an earlier boundary.

---

## 3. Contracts

### 3.1 Validated Turn Input

`StoryId`, `TurnId`, `IdempotencyKey`, and `SessionId` must be non-empty newtypes with fallible public constructors. Remove public `From<String>`, `From<&str>`, and any `Default` implementation that can construct an invalid ID.

```rust
pub struct ExecuteTurnSpec {
    pub story_id: StoryId,
    pub idempotency_key: IdempotencyKey,
    pub player_input: String,
    pub cancellation: TurnCancellation,
}

pub struct ValidatedExecuteTurnSpec {
    story_id: StoryId,
    idempotency_key: IdempotencyKey,
    request: TurnRequest,
    cancellation: TurnCancellation,
}

impl ExecuteTurnSpec {
    pub fn try_into_validated(self) -> Result<ValidatedExecuteTurnSpec, TurnInputError>;
}

impl StoryId {
    pub fn try_new(value: impl Into<String>) -> Result<Self, TurnInputError>;
}

impl TurnId {
    pub fn try_new(value: impl Into<String>) -> Result<Self, TurnInputError>;
}

impl IdempotencyKey {
    pub fn try_new(value: impl Into<String>) -> Result<Self, TurnInputError>;
}

#[derive(Debug, thiserror::Error)]
pub enum TurnInputError {
    EmptyStoryId,
    EmptyTurnId,
    EmptyIdempotencyKey,
    IdempotencyKeyTooLong { actual: usize, maximum: usize },
    EmptyPlayerInput,
    PlayerInputTooLong { actual: usize, maximum: usize },
}
```

`ExecuteTurnSpec::try_into_validated` performs all normalization and digest generation. It must complete before `StoryTurnCoordinator::acquire`, Store lookup, Story creation, Trace creation, or observer registration.

The HTTP handler performs the same fallible parsing before returning an SSE `200 OK`. Invalid path IDs, missing/invalid `Idempotency-Key`, and invalid player input return a synchronous JSON error response and never create an SSE stream.

### 3.2 Layered Errors and Terminal Classification

turn owns only transport-free Turn execution errors.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnFailureKind {
    InvalidRequest,
    StoryNotFound,
    Cancelled,
    DeadlineExceeded,
    RevisionConflict,
    IdempotencyConflict,
    Backpressure,
    ValidationRejected,
    ValidationBudgetExhausted,
    Llm,
    Store,
    Io,
    InvariantViolation,
}

#[derive(Debug, thiserror::Error)]
pub struct TurnExecutionError {
    kind: TurnFailureKind,
    code: &'static str,
    stage: Option<TurnStage>,
    message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnTerminalKind {
    Failed,
    Cancelled,
    Conflict,
}

impl TurnExecutionError {
    pub fn terminal_kind(&self) -> TurnTerminalKind;
    pub fn kind(&self) -> TurnFailureKind;
    pub fn code(&self) -> &'static str;
    pub fn stage(&self) -> Option<TurnStage>;
}
```

The Pipeline contract becomes independent of the application error aggregator.

```rust
#[async_trait]
pub trait TurnExecutionPipeline: Send + Sync {
    fn stage(&self) -> TurnStage;

    async fn execute(
        &self,
        ctx: &mut TurnExecutionContext,
    ) -> Result<(), TurnExecutionError>;
}
```

Outer layers keep separate typed errors.

```rust
pub enum LlmProviderError {
    RateLimited { retry_after_ms: Option<u64> },
    Rejected { status: u16, code: Option<String> },
    Transport { kind: LlmTransportErrorKind },
    Protocol { kind: LlmProtocolErrorKind },
    ResponseLimitExceeded { limit: LlmResponseLimit },
}

pub enum LlmError {
    Cancelled,
    TurnDeadlineExceeded,
    ProviderTimeout,
    QueueTimeout,
    RateLimited { retry_after_ms: Option<u64> },
    TokenBudgetExceeded,
    ProviderRejected { status: u16 },
    Transport { kind: LlmTransportErrorKind },
    Protocol { kind: LlmProtocolErrorKind },
    ResponseLimitExceeded { limit: LlmResponseLimit },
    EmbeddingUnsupported,
}

pub enum StoreError {
    NotFound,
    RevisionConflict,
    IdempotencyConflict,
    ConstraintViolation { constraint: String },
    Serialization { kind: StoreSerializationErrorKind },
    Unavailable,
}

enum SqliteStoreError {
    Database(sqlx::Error),
    Migration(sqlx::migrate::MigrateError),
    Json(serde_json::Error),
    Io(std::io::Error),
}
```

`reqwest::Error`, `sqlx::Error`, `serde_json::Error`, and `std::io::Error` must not appear in turn, Domain, Gateway, or Store port public variants. Provider and SQLite adapters map concrete library errors at their own boundary. Pipelines map `LlmError` or `StoreError` to `TurnExecutionError`; Engine maps preflight, coordinator, Runtime, and commit errors to the same terminal classifier.

Required classification:

| Source error | `TurnFailureKind` | Context phase | Event |
| --- | --- | --- | --- |
| `LlmError::Cancelled` | `Cancelled` | `Cancelled` | `Cancelled` |
| `LlmError::TurnDeadlineExceeded` | `DeadlineExceeded` | `Failed` | `Failed` |
| `StoreError::RevisionConflict` | `RevisionConflict` | `Conflict` | `Conflict` |
| `StoreError::IdempotencyConflict` | `IdempotencyConflict` | `Conflict` | `Conflict` |
| `ValidationBudgetExhausted` | `ValidationBudgetExhausted` | `Failed` | `Failed` |
| all other typed failures | matching non-conflict kind | `Failed` | `Failed` |

### 3.3 Context Terminal Transitions and Engine Finalizer

```rust
impl TurnExecutionContext {
    pub fn mark_failed(&mut self, failure: &TurnExecutionError) -> Result<(), TurnExecutionError>;
    pub fn mark_cancelled(&mut self, failure: &TurnExecutionError) -> Result<(), TurnExecutionError>;
    pub fn mark_conflict(&mut self, failure: &TurnExecutionError) -> Result<(), TurnExecutionError>;
    pub fn terminal_phase(&self) -> Option<TurnPhase>;
}

pub enum TurnRunOutcome {
    Committed {
        result: CommittedTurnResult,
        replayed: bool,
    },
    Failed(TurnExecutionError),
}
```

`AiseEngine::run_turn` must have one post-validation finalization path. The finalizer owns these operations in order:

```text
normalize result
-> set exactly one Context terminal phase when a Context exists
-> emit exactly one terminal TurnEvent
-> emit TraceCompleted exactly once when a Trace exists
-> return the committed result or typed error
-> release StoryTurnPermit
-> destroy TurnExecutionContext
```

An idempotency replay is `TurnRunOutcome::Committed { replayed: true }`; it emits `Committed` with the original persisted result and does not construct a Runtime, call an LLM, or commit again.

### 3.4 Turn Events and SSE Protocol

```rust
#[derive(Debug, Clone)]
pub enum TurnEvent {
    StageStarted { turn_id: TurnId, stage: TurnStage },
    ValidationCompleted {
        turn_id: TurnId,
        attempt: u32,
        decision: ValidationDecision,
        issue_codes: Vec<ValidationIssueCode>,
    },
    Committed { result: CommittedTurnResult, replayed: bool },
    Failed { turn_id: TurnId, code: &'static str },
    Cancelled { turn_id: TurnId, code: &'static str },
    Conflict { turn_id: TurnId, code: &'static str },
    TraceCompleted { turn_id: TurnId, trace_id: TraceId },
}

#[derive(Debug, thiserror::Error)]
pub enum TurnEventDeliveryError {
    ProgressBackpressure,
    TerminalAlreadySent,
    ClientDisconnected,
}

pub trait TurnEventSink: Send + Sync {
    fn emit(&self, event: TurnEvent) -> Result<(), TurnEventDeliveryError>;
}
```

`ValidationCompleted` is emitted for every validation attempt, including Repair and Reject. `issue_codes` is bounded by `max_validation_issues` and contains no issue message or model content.

The server uses `tokio::sync::mpsc`, not `futures::channel::mpsc` behind `std::sync::Mutex`. Each SSE connection has a bounded progress lane and a terminal lane of capacity one. Progress backpressure produces a structured warning; the first terminal event owns the terminal lane and cannot be displaced by progress events. Sending never occurs while any mutex or read/write guard is held.

SSE names and payloads are fixed:

```json
{"event":"committed","data":{"turn_id":"string","story_revision":1,"replayed":false}}
{"event":"failed","data":{"turn_id":"string","code":"string"}}
{"event":"cancelled","data":{"turn_id":"string","code":"cancelled"}}
{"event":"conflict","data":{"turn_id":"string","code":"revision_conflict"}}
```

### 3.5 Persistent Result Recovery API

```rust
#[async_trait]
pub trait Store: Send + Sync {
    async fn find_committed_turn(
        &self,
        story_id: &StoryId,
        idempotency_key: &IdempotencyKey,
    ) -> Result<Option<CommittedTurnResult>, StoreError>;
}
```

```http
GET /api/stories/{story_id}/turn-results/{idempotency_key}
```

Responses:

| Condition | Status | Body |
| --- | --- | --- |
| persisted committed result exists | `200` | full `CommittedTurnResult` |
| Story exists but key has no committed result | `404` | `{"code":"turn_result_not_found"}` |
| invalid path ID/key | `400` | typed validation error |
| Store unavailable | `503` | `{"code":"store_unavailable"}` |

The endpoint is read-only, does not acquire a Story execution permit, does not create a Story, and never calls an LLM.

### 3.6 Sealed Validation Contract

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationIssueCode {
    SchemaInvalid,
    ReferenceMissing,
    ModificationForbidden,
    DomainInvariantViolated,
    KnowledgeBoundaryViolated,
    PlayerControlViolated,
    WorldFactEvidenceMissing,
    WorldFactEvidenceInvalid,
    NarrativeInconsistent,
    CharacterInconsistent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Repairability {
    Repairable,
    Fatal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationLocation {
    pub path: String,
    pub item_index: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub code: ValidationIssueCode,
    pub message: String,
    pub repairability: Repairability,
    pub location: Option<ValidationLocation>,
}

pub enum ValidationResult {
    Pass(ValidatedChangeSet),
    Repair(BoundedValidationIssues),
    Reject(BoundedValidationIssues),
}

pub struct ValidatedChangeSet {
    story_text: String,
    events: Vec<StoryEvent>,
    character_changes: Vec<CharacterState>,
    world_change: StateChange<WorldState>,
    memory_changes: Vec<MemoryEntry>,
    scene_change: StateChange<CurrentScene>,
    constraint_change: StateChange<Vec<StoryConstraint>>,
    summary_change: StateChange<StorySummary>,
}
```

`ValidatedChangeSet` has no public constructor and is not `Deserialize`. Its only constructor is `pub(crate)` and is called only by the final deterministic conversion step in `validation/validation_pipeline.rs`. `ValidationResult` exposes constructors that enforce these invariants:

```rust
impl ValidationResult {
    pub(crate) fn pass(change_set: ValidatedChangeSet) -> Self;
    pub fn repair(issues: BoundedValidationIssues) -> Result<Self, TurnExecutionError>;
    pub fn reject(issues: BoundedValidationIssues) -> Result<Self, TurnExecutionError>;
    pub fn decision(&self) -> ValidationDecision;
    pub fn issues(&self) -> &[ValidationIssue];
    pub(crate) fn into_change_set(self) -> Option<ValidatedChangeSet>;
}
```

`Pass` always owns a `ValidatedChangeSet` and exposes an empty issue slice. `Repair` contains at least one repairable issue and no fatal issue. `Reject` contains at least one fatal issue. Remove `with_issue`, the public `ValidatedChangeSet::new`, and all direct construction in integration tests.

Deterministic validators execute in this fixed order:

```text
Schema
-> Reference Consistency
-> Modification Permission
-> Domain Invariant
-> Knowledge Boundary
-> Player Control Boundary
-> World Fact Evidence
-> Narrative and Character validation
-> ValidatedChangeSet conversion
```

Any deterministic Reject stops later narrative validation. Narrative validation cannot replace, downgrade, or remove deterministic issues.

### 3.7 World Fact Evidence

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorldFactEvidenceRef {
    SnapshotFact(FactId),
    ProposedEvent { event_index: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedWorldFact {
    pub text: String,
    pub evidence: Vec<WorldFactEvidenceRef>,
}
```

Every new proposed World Fact has at least one bounded evidence reference. `SnapshotFact` must resolve to a canonical fact in the same `StoryReadSnapshot`; `ProposedEvent` must resolve to an event in the same Proposal that passed schema, permission, reference, and domain validation. `CharacterThought`, `CharacterBelief`, `CharacterMemory`, `PlannerHypothesis`, free text, and raw model reasoning are not evidence variants and cannot be converted into one.

### 3.8 Unified Limits and Bounded Data

All limits originate in validated `AiseConfig`; runtime limit views are derived values with no independent `Default`.

```rust
pub struct TurnContentLimitsConfig {
    pub max_story_instructions_bytes: usize,
    pub max_story_config_bytes: usize,
    pub max_scene_bytes: usize,
    pub max_summary_bytes: usize,
    pub max_constraints: usize,
    pub max_constraint_bytes: usize,
    pub max_characters: usize,
    pub max_character_bytes: usize,
    pub max_world_facts: usize,
    pub max_world_fact_bytes: usize,
    pub max_recent_turns: usize,
    pub max_recent_turn_bytes: usize,
    pub max_memories: usize,
    pub max_memory_bytes: usize,
    pub max_plan_bytes: usize,
    pub max_retrieval_candidates: usize,
    pub max_retrieved_items: usize,
    pub max_retrieved_item_bytes: usize,
    pub max_retrieved_tokens: u64,
    pub max_character_thoughts: usize,
    pub max_character_thought_bytes: usize,
    pub max_proposal_bytes: usize,
    pub max_validation_issues: usize,
    pub max_validation_issue_bytes: usize,
    pub max_trace_spans: usize,
    pub max_trace_field_bytes: usize,
}

pub struct SnapshotLimits {
    max_story_instructions_bytes: usize,
    max_story_config_bytes: usize,
    max_scene_bytes: usize,
    max_summary_bytes: usize,
    max_constraints: usize,
    max_constraint_bytes: usize,
    max_characters: usize,
    max_character_bytes: usize,
    max_world_facts: usize,
    max_world_fact_bytes: usize,
    max_recent_turns: usize,
    max_recent_turn_bytes: usize,
    max_memories: usize,
    max_memory_bytes: usize,
}

impl SnapshotLimits {
    pub fn from_config(config: &TurnContentLimitsConfig) -> Self;
}
```

`SnapshotLimits` and `TurnBudgetLimits` have no `Default`. `TurnBudgetLimits::from(&TurnConfig)` is the only production constructor. Test fixtures may use explicit constructors under test-only modules.

Every `TurnExecutionContext` setter validates counts, UTF-8 byte lengths, and token totals before mutating state or advancing phase. Rejection leaves the previous Context state unchanged. Store queries apply SQL `LIMIT` before decoding; JSON documents are rejected after a bounded byte read and before full semantic expansion. Retrieval keeps a bounded Top-K heap of at most `max_retrieval_candidates` and never collects all candidates into an unbounded `Vec`.

### 3.9 Authoritative Story State

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryConfig {
    pub style: Option<String>,
    pub point_of_view: Option<String>,
    pub tense: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentScene {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorySummary {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryConstraint {
    pub id: ConstraintId,
    pub text: String,
}

pub struct StoryReadSnapshot {
    story_id: StoryId,
    base_revision: StoryRevision,
    story_instructions: String,
    story_config: StoryConfig,
    player_character_id: Option<CharacterId>,
    world: Option<WorldState>,
    current_scene: CurrentScene,
    characters: Vec<CharacterState>,
    recent_turns: Vec<StoryTurn>,
    story_summary: StorySummary,
    active_constraints: Vec<StoryConstraint>,
    player_memories: Vec<MemoryEntry>,
}

pub struct StoryCreateSpec {
    pub story_id: StoryId,
    pub story_instructions: String,
    pub story_config: StoryConfig,
    pub player_character_id: Option<CharacterId>,
    pub initial_world: Option<WorldState>,
    pub current_scene: CurrentScene,
    pub story_summary: StorySummary,
    pub active_constraints: Vec<StoryConstraint>,
    pub created_at_ms: i64,
}

#[async_trait]
pub trait Store: Send + Sync {
    async fn create_story(&self, spec: &StoryCreateSpec) -> Result<StoryInfo, StoreError>;
    async fn get_story(&self, story_id: &StoryId) -> Result<Option<StoryInfo>, StoreError>;
    async fn load_story_snapshot(
        &self,
        story_id: &StoryId,
        limits: SnapshotLimits,
    ) -> Result<StoryReadSnapshot, StoreError>;
    async fn commit_turn(
        &self,
        spec: &TurnCommitSpec,
    ) -> Result<CommittedTurnResult, StoreError>;
}
```

Add migration `crates/aise/assets/persistence/mig/0005_authoritative_story_state.sql`. It adds authoritative Story instructions, configuration, current scene, summary, and constraints without deriving them from `story_turns.summary_delta` or the most recent `story_text`. Existing Story rows are migrated to explicit empty typed state once; the old approximation code is deleted in the same change.

`TurnCommitSpec` carries `scene_change`, `constraint_change`, and `summary_change`. The SQLite transaction applies them after revision CAS and before Outbox insertion. `StateChange::Unchanged` performs no update. Snapshot loading reads Story metadata/state, revision, World, Characters, history, and memories in one read transaction.

### 3.10 Story and Session HTTP API

```http
POST /api/stories
GET /api/stories/{story_id}
POST /api/sessions
PUT /api/sessions/{session_id}/story
DELETE /api/sessions/{session_id}
```

```json
{"story_id":"string","story_instructions":"string","story_config":{"style":null,"point_of_view":null,"tense":null},"player_character_id":null,"initial_world":null,"current_scene":{"text":""},"story_summary":{"text":""},"active_constraints":[]}
```

```json
{"name":"string","story_id":"existing-story-id"}
```

```json
{"story_id":"existing-story-id"}
```

`POST /api/stories` is the only transport operation that creates persistent Stories. `POST /api/sessions` requires an existing `story_id`; `PUT /api/sessions/{session_id}/story` switches the binding to another existing Story. Multiple Sessions may bind the same Story. Session deletion or server restart does not delete or hide a Story.

### 3.11 LLM Call Transaction and Accounting

```rust
pub struct LlmCallUsage {
    pub call_id: LlmCallId,
    pub purpose: LlmCallPurpose,
    pub provider: String,
    pub model: String,
    pub input_tokens: u64,
    pub cached_input_tokens: Option<u64>,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub accuracy: UsageAccuracy,
    pub pricing_version: Option<String>,
    pub charge: Option<LlmCharge>,
    pub finish_reason: Option<FinishReason>,
}

pub struct LlmUsageLedger {
    calls: Vec<LlmCallUsage>,
    aggregate: LlmUsageAggregate,
}

pub struct EmbeddingOutput {
    pub vectors: Vec<Vec<f32>>,
    pub usage: LlmTokenUsage,
    pub charge: Option<LlmCharge>,
}

pub struct CommittedTurnResult {
    pub turn_id: TurnId,
    pub story_revision: StoryRevision,
    pub story_text: String,
    pub llm_usage: LlmUsageAggregate,
    pub llm_calls: Vec<LlmCallUsage>,
}
```

`LlmUsageLedger.calls.len()` never exceeds `TurnConfig.max_llm_calls`. The ledger and aggregate are serialized into the committed result and stored in the same transaction as the Turn. Idempotency replay returns byte-equivalent accounting data.

Gateway opens a standard `tracing` span and a Turn Trace call transaction at method entry, before cancellation, deadline, budget, quota, or queue checks. A call transaction has exactly one final status and releases all reservations on every exit.

```rust
pub enum LlmCallStatus {
    Succeeded,
    Cancelled,
    TurnDeadlineExceeded,
    ProviderTimeout,
    QueueTimeout,
    RateLimited,
    TokenBudgetExceeded,
    ProviderRejected,
    TransportFailed,
    ProtocolFailed,
    ResponseLimitExceeded,
}

pub struct LlmBudgetReservation {
    call_id: LlmCallId,
    reserved_input_tokens: u64,
    reserved_output_tokens: u64,
}

impl TurnBudget {
    pub fn reserve_llm(
        &mut self,
        input_tokens: u64,
        maximum_output_tokens: u64,
    ) -> Result<LlmBudgetReservation, TurnExecutionError>;

    pub fn settle_llm(
        &mut self,
        reservation: LlmBudgetReservation,
        usage: LlmCallUsage,
    ) -> Result<(), TurnExecutionError>;

    pub fn release_llm(&mut self, reservation: LlmBudgetReservation);
}
```

Pending reservations count against remaining budget. `settle_llm` consumes the reservation, records actual usage, releases unused capacity, and returns `TokenBudgetExceeded` when actual usage exceeds the allowed budget. The Trace status must then be `TokenBudgetExceeded`, never `Succeeded`.

Rate quota and concurrency permit acquisition share:

```text
queue_deadline = min(turn_deadline, call_started_at + queue_timeout)
```

Provider execution uses:

```text
provider_deadline = min(turn_deadline, provider_started_at + provider_timeout)
```

### 3.12 OpenAI-Compatible Streaming Protocol

```rust
pub struct LlmProtocolLimitsConfig {
    pub max_sse_line_bytes: usize,
    pub max_stream_buffer_bytes: usize,
    pub max_content_bytes: usize,
    pub max_reasoning_bytes: usize,
    pub max_embedding_items: usize,
    pub max_embedding_dimensions: usize,
}
```

The provider requests streaming usage, parses provider usage, cached tokens, and finish reason, and never substitutes `FinishReason::Stop` when the provider supplied another value. HTTP mapping is fixed:

| Provider response | `LlmProviderError` |
| --- | --- |
| `429` | `RateLimited` with parsed `Retry-After` when valid |
| other `400..499` | `Rejected` |
| `500..599` | `Transport { kind: Server }` |
| malformed JSON/SSE or unknown finish reason | `Protocol` |
| line/buffer/content/reasoning limit exceeded | `ResponseLimitExceeded` |

The provider stops reading immediately when a protocol limit is exceeded. Partial content is never returned as success.

### 3.13 Trace Content and Retention

```rust
pub enum TraceContentPolicy {
    MetadataOnly,
    RedactedContent,
}

pub struct TraceWriterConfig {
    pub channel_capacity: usize,
    pub max_record_bytes: usize,
    pub rotation_bytes: u64,
    pub retention_files: usize,
    pub shutdown_grace_ms: u64,
}

pub enum TraceRecord {
    Span { trace_id: TraceId, span: TraceSpan },
    Completed(TurnTrace),
}

pub trait TraceSink: Send + Sync {
    fn try_write(&self, record: TraceRecord) -> Result<(), TraceSinkError>;
}
```

`MetadataOnly` records no player input, Prompt, Response, model-output preview, secret memory, API key, authorization header, or private Character Thought. Parse errors record only parse kind, schema path, response hash, and byte length.

`RedactedContent` is accepted only when the runtime environment is explicitly `development`; content passes through one injected `TraceRedactor` before byte truncation. The file writer owns one bounded channel and one background task, uses async file I/O off the request path, rotates before `rotation_bytes`, retains at most `retention_files`, and drains within shutdown grace. Queue overflow emits a structured warning with `trace_id`, `record_kind`, and `error`, without content.

### 3.14 Turn Task Supervisor and Graceful Shutdown

```rust
pub struct TurnTaskSupervisorConfig {
    pub max_active_turns: usize,
    pub admission_capacity: usize,
    pub admission_timeout_ms: u64,
    pub shutdown_grace_ms: u64,
}

pub struct TurnTaskSupervisorHandle {
    command_tx: tokio::sync::mpsc::Sender<TurnTaskCommand>,
    service_cancellation: CancellationToken,
}

enum TurnTaskCommand {
    Spawn(TurnTaskSpec),
    Shutdown(tokio::sync::oneshot::Sender<()>),
}

impl TurnTaskSupervisorHandle {
    pub async fn spawn(&self, task: TurnTaskSpec) -> Result<(), TurnTaskError>;
    pub async fn shutdown_with_grace(&self) -> Result<(), TurnTaskError>;
    pub fn service_cancellation(&self) -> CancellationToken;
}
```

One supervisor task owns `JoinSet`; no `JoinSet` is stored behind a mutex. Admission is bounded by `admission_capacity` and `admission_timeout_ms`. Shutdown closes admission, cancels waiters, cancels the service token, waits for owned Turn tasks until the grace deadline, aborts remaining tasks, joins them, drains the Trace writer, and then completes.

Each Turn cancellation is a child of both client-disconnect cancellation and service cancellation. Cancellation stops new external calls but never rolls back a transaction that already committed.

`main.rs` wires `axum::serve(...).with_graceful_shutdown(...)` to SIGINT and SIGTERM where supported and invokes the supervisor shutdown sequence exactly once.

### 3.15 Configuration and CI

```rust
impl ServerConfig {
    pub fn load() -> Result<Self, ConfigError>;
    pub fn validate(&self) -> Result<(), ConfigError>;
}

impl AiseConfig {
    pub fn validate(&self) -> Result<(), ConfigError>;
}

impl TurnConfig {
    pub fn validate(&self) -> Result<(), ConfigError>;
}

impl CoordinatorConfig {
    pub fn validate(&self) -> Result<(), ConfigError>;
}

impl LlmConfig {
    pub fn validate(&self) -> Result<(), ConfigError>;
}
```

Invalid TOML, invalid environment overrides, zero capacities, inconsistent token limits, invalid retention, invalid timeouts, and `RedactedContent` outside development fail startup. No invalid setting logs a warning and falls back to defaults.

The workspace keeps its documented `rust-version = "1.85"`. CI runs exact MSRV and stable jobs with the commands in §5.

### 3.16 File and Directory Layout

```text
crates/aise/src/
├── turn/
│   ├── mod.rs
│   ├── story_proposal.rs
│   ├── turn_budget.rs
│   ├── turn_context.rs
│   ├── turn_contract.rs
│   ├── turn_data.rs
│   ├── turn_error.rs
│   ├── turn_event.rs
│   ├── turn_pipeline.rs
│   ├── turn_trace.rs
│   └── turn_validation.rs
├── domain/
│   └── story_state.rs
├── llm/
│   ├── error.rs
│   ├── gateway.rs
│   ├── openai_compat.rs
│   ├── provider.rs
│   └── tests/
├── persistence/
│   ├── sqlite_error.rs
│   ├── sqlite_store.rs
│   ├── store.rs
│   └── turn_committer.rs
└── validation/
    ├── validation_pipeline.rs
    └── validators/
        ├── consistency.rs
        ├── domain_invariant.rs
        ├── knowledge_boundary.rs
        ├── modification_permission.rs
        ├── player_control.rs
        ├── schema.rs
        └── world_fact_evidence.rs

crates/aise-server/src/
├── api/
│   ├── session.rs
│   ├── story.rs
│   └── turn.rs
├── shutdown.rs
├── tasks/
│   ├── mod.rs
│   ├── supervisor.rs
│   └── tests/
└── trace/
    ├── mod.rs
    ├── redactor.rs
    ├── writer.rs
    └── tests/

crates/aise/assets/persistence/mig/
└── 0005_authoritative_story_state.sql
```

Move code from `crates/aise-server/src/tasks.rs` to `tasks/supervisor.rs` and from `crates/aise-server/src/trace.rs` to the `trace/` directory. Delete the old sibling files in the same change. Every `mod.rs` remains declarations, re-exports, and attributes only.

---

## 4. Behavior Rules

1. **R-1 — Side-effect-free validation**: Invalid `ExecuteTurnSpec` returns `TurnInputError` before acquiring a permit, reading or writing Store, creating a Story, creating Trace, spawning a task, or returning SSE `200`.
2. **R-2 — No implicit Story creation**: `AiseEngine::run_turn` returns `StoryNotFound` for an unknown Story; only `Store::create_story` through `POST /api/stories` creates one.
3. **R-3 — Single finalizer**: Every Engine exit after validated input uses the finalizer in §3.3; direct early returns that bypass terminal classification, event emission, Trace closure, or permit release are forbidden.
4. **R-4 — Exact terminal state**: A Context reaches exactly one of `Committed`, `Failed`, `Cancelled`, or `Conflict`; no result may leave it in an intermediate phase.
5. **R-5 — Nested error classification**: Gateway cancellation and Store conflicts retain their semantic terminal class through Pipeline and Engine mappings.
6. **R-6 — Replay delivery**: Same key and same digest returns and emits the original result with `replayed = true`; it performs zero LLM and commit calls.
7. **R-7 — Recovery is authoritative**: SSE is an observer, not result storage; the lookup API returns only persisted committed results.
8. **R-8 — Terminal SSE cannot be displaced**: Progress saturation cannot consume or overwrite the single terminal lane.
9. **R-9 — Validation structural consistency**: `Pass`, `Repair`, and `Reject` cannot be constructed with contradictory issues or ChangeSet state.
10. **R-10 — Deterministic validation dominates**: Any deterministic fatal issue stops narrative validation and prevents `ValidatedChangeSet` construction.
11. **R-11 — Thought is non-authoritative**: No type conversion, validator, Generator, Repairer, or Committer may treat Character Thought, Belief, Memory, Planner Hypothesis, or raw model reasoning as World Fact evidence.
12. **R-12 — Commit gate**: `TurnCommitter` rejects any Context not in `ReadyToCommit` or without the sealed `ValidatedChangeSet`.
13. **R-13 — Bounded mutation**: A Context setter validates the complete proposed value before mutation; an over-limit error preserves the old value and phase.
14. **R-14 — Bounded reads**: Snapshot and retrieval code applies bounds before materializing collections and rejects oversized individual records.
15. **R-15 — Single configuration source**: Runtime limits are derived from validated config; no production `Default` supplies a second limit set.
16. **R-16 — Full LLM transaction**: Every Gateway call produces one final Trace status from method entry through settlement, including pre-cancel, deadline, budget, quota, queue timeout, provider error, and settlement overflow.
17. **R-17 — Reserved budget counts**: Concurrent pending reservations reduce available call/token budget until settled or released.
18. **R-18 — Provider isolation**: Provider code handles HTTP/SSE protocol only; it owns no Turn Context, budget, limiter, pricing, or Turn Trace.
19. **R-19 — No content leakage**: `MetadataOnly` prevents player input, Prompt, Response, raw parse preview, secret memory, private Thought, and credentials from entering errors, logs, SSE, or persistent Trace.
20. **R-20 — Atomic accounting**: Per-call usage, aggregate usage, charge, pricing version, and committed result are written in the Turn transaction and replayed unchanged.
21. **R-21 — Atomic Story state**: Scene, constraints, summary, World, Characters, Memories, canonical events, revision, Turn result, usage ledger, and Outbox changes commit or roll back together.
22. **R-22 — Snapshot consistency**: All authoritative Story fields in one Snapshot reflect the same base revision under a concurrent commit.
23. **R-23 — Session independence**: Session lifetime never determines Story lifetime; multiple Sessions may bind one persistent Story.
24. **R-24 — Bounded task admission**: Turn task admission rejects or times out at configured capacity; it never waits indefinitely.
25. **R-25 — One task owner**: The supervisor task is the sole `JoinSet` owner and joins or aborts every spawned Turn before shutdown completes.
26. **R-26 — No lock side effects**: No lock guard crosses `.await`, channel send, event emission, Trace write, or I/O.
27. **R-27 — Graceful server exit**: Shutdown rejects new work, cancels waiters and uncommitted external work, honors grace for owned work, drains Trace, and exits without orphan tasks.
28. **R-28 — Startup is strict**: Invalid files, environment overrides, capacities, budgets, timeouts, content policy, and retention return `ConfigError`; no silent fallback is permitted.
29. **R-29 — Structured observability**: Logs and spans use structured `story_id`, `turn_id`, `stage`, `error_kind`, `provider`, `model`, `base_revision`, and `committed_revision` fields; identifiers are not interpolated into messages.
30. **R-30 — Hard refactor completion**: Old constructors, old task/trace modules, implicit Story creation, summary approximation, raw adapter errors, and duplicate defaults are deleted with their old tests and config paths.

### 4.1 Error Handling

- External input and domain failures return typed errors; no `unwrap`, `expect`, or panic represents a business failure.
- Critical parse, LLM, Store, task, Trace, and event delivery failures return a diagnosable typed error or emit a structured error/warning before unwinding (`R-OBS-01`).
- Event delivery failure does not alter a committed database result. It emits a structured warning and leaves the result available through the recovery API.
- Error display strings and serialized API errors contain stable codes and bounded metadata, never raw Prompt, Response, player text, SQL, credentials, or adapter error bodies.

### 4.2 Concurrency

- Story execution remains serialized by owned Story permits; different Stories remain parallel within global admission and LLM limits.
- Every channel, queue, task set, coordinator waiter set, Trace buffer, SSE lane, and LLM queue has a positive configured capacity and timeout or retention policy.
- Character Thought generation remains deterministic and sequential in this remediation; no new fan-out is introduced.
- All LLM paths use the one application-root `LlmGateway` and its shared limiter.

### 4.3 Observability

- Wrap every Pipeline, LLM call, validation attempt, Store snapshot, commit, result recovery, and task shutdown operation in a structured `tracing` span (`R-OBS-02`).
- Each LLM span records `status` and `error_kind` exactly once, including pre-dispatch failures.
- Each validation attempt records `attempt`, `decision`, bounded issue codes, and elapsed time.
- Track counters for terminal kinds, replay recovery, SSE progress drops, terminal delivery failures, validation decisions, budget rejection, task admission rejection, Trace drops, revision conflicts, and shutdown aborts.

---

## 5. Acceptance Criteria

### 5.1 Terminal, Replay, and Recovery

- [ ] `invalid_story_id_has_no_store_or_coordinator_side_effects` proves R-1 and R-2.
- [ ] `http_preflight_error_does_not_open_sse` returns JSON `400`, not SSE `200`.
- [ ] `idempotency_replay_emits_original_committed_event` observes the persisted result with `replayed = true`.
- [ ] `idempotency_conflict_emits_conflict_terminal` observes Context `Conflict` and one `Conflict` event.
- [ ] `nested_llm_cancel_sets_cancelled_phase_and_event` proves the nested mapping in §3.2.
- [ ] `nested_store_conflict_sets_conflict_phase_and_event` proves the nested mapping in §3.2.
- [ ] `pipeline_failure_sets_failed_phase_and_closes_trace` leaves no intermediate Context phase.
- [ ] `repair_exhaustion_sets_failed_phase_and_never_commits` proves finalizer and commit gating.
- [ ] `terminal_event_survives_saturated_progress_lane` proves the reserved terminal lane.
- [ ] `get_turn_result_recovers_after_sse_disconnect` returns the committed result without an LLM call.

### 5.2 Validation and Resource Boundaries

- [ ] `rg "pub fn new" crates/aise/src/turn/turn_validation.rs` returns no public `ValidatedChangeSet` constructor.
- [ ] `rg "with_issue" crates` returns zero matches.
- [ ] `pass_cannot_contain_issues` is enforced by construction and compile-fail coverage.
- [ ] `repair_cannot_contain_fatal_issue` and `reject_requires_fatal_issue` pass.
- [ ] `deterministic_failure_skips_narrative_validator` proves deterministic dominance.
- [ ] `character_thought_proposed_as_world_fact_is_rejected` places Thought-derived content in `world_change` and receives `KnowledgeBoundaryViolated` or `WorldFactEvidenceInvalid`.
- [ ] `world_fact_requires_resolvable_evidence` covers missing, invalid, and valid evidence references.
- [ ] `bounded_outputs_reject_plan_snapshot_retrieval_thought_proposal_and_validation_limits` covers every §3.8 limit class.
- [ ] `snapshot_query_limits_before_decode` proves Store does not materialize an unbounded collection.
- [ ] `retrieval_uses_bounded_top_k_candidates` proves candidate memory never exceeds `max_retrieval_candidates`.
- [ ] `rg "impl Default for (TurnBudgetLimits|SnapshotLimits)" crates/aise/src` returns zero matches.

### 5.3 Layering and Error Isolation

- [ ] `turn_has_no_outer_transitive_dependency` parses Rust imports or module dependencies and rejects `turn -> llm|persistence|runtime|server|pipeline` paths.
- [ ] `provider_public_error_hides_reqwest` proves `reqwest::Error` is adapter-private.
- [ ] `store_port_public_error_hides_sqlx` proves `sqlx::Error` is adapter-private.
- [ ] `rg "reqwest::Error|sqlx::Error" crates/aise/src/turn crates/aise/src/domain crates/aise/src/persistence/store.rs crates/aise/src/llm/error.rs` returns zero matches.
- [ ] Business Pipeline directories contain no `LlmProvider` import.

### 5.4 LLM Gateway and Accounting

- [ ] `llm_trace_closes_on_pre_cancel` verifies a pre-dispatch Cancelled span.
- [ ] `llm_trace_closes_on_turn_deadline_before_queue` verifies a deadline span.
- [ ] `llm_trace_closes_on_queue_timeout` verifies configured queue deadline.
- [ ] `pending_reservation_reduces_available_budget` proves reserve/release semantics.
- [ ] `settlement_overflow_marks_trace_budget_exceeded` never records `Succeeded`.
- [ ] `stream_parses_finish_reason_and_usage` covers non-Stop finish reasons and cached input.
- [ ] `stream_rejects_line_buffer_content_and_reasoning_overflow` covers all protocol limits.
- [ ] `provider_classifies_429_4xx_and_5xx` matches §3.12.
- [ ] `metadata_only_parse_error_contains_no_model_output` checks logs, SSE, and persisted Trace.
- [ ] `content_trace_redacts_before_truncation` verifies redaction ordering.
- [ ] `committed_result_persists_bounded_per_call_ledger` verifies per-call and aggregate accounting.
- [ ] `idempotency_replay_preserves_usage_and_charge` returns byte-equivalent accounting.

### 5.5 Production Lifecycle and Trace

- [ ] `task_admission_rejects_over_capacity_without_unbounded_wait` passes.
- [ ] `task_supervisor_owns_join_set_without_mutex` is enforced by static check and test behavior.
- [ ] `shutdown_cancels_waiters_and_waits_for_owned_turns` passes.
- [ ] `shutdown_aborts_only_after_grace_and_joins_all_tasks` passes.
- [ ] `service_shutdown_reaches_running_turn_cancellation` passes.
- [ ] `trace_writer_applies_bounded_backpressure` passes.
- [ ] `trace_writer_rotates_and_enforces_retention` leaves at most configured files.
- [ ] `shutdown_drains_trace_writer_within_grace` passes.
- [ ] `rg "Mutex<JoinSet|RwLock<JoinSet|std::sync::Mutex" crates/aise-server/src/tasks crates/aise-server/src/api/sse.rs` returns zero matches.

### 5.6 Authoritative Story State and Session Binding

- [ ] Migration `0005_authoritative_story_state.sql` upgrades an existing database without losing committed Turns.
- [ ] `snapshot_is_revision_consistent_under_concurrent_commit` verifies all authoritative fields share one base revision.
- [ ] `baseline_uses_authoritative_scene_summary_and_constraints` does not derive state from latest Turn text or `summary_delta`.
- [ ] `commit_atomically_updates_scene_summary_constraints_and_revision` passes.
- [ ] `authoritative_state_rolls_back_on_commit_failure` leaves every state table and Outbox unchanged.
- [ ] `turn_execution_never_creates_missing_story` passes through Engine and HTTP entry points.
- [ ] `session_binds_existing_story_after_restart` locates a persistent Story after recreating the Session registry.
- [ ] `multiple_sessions_can_bind_same_story` passes.
- [ ] `deleting_session_does_not_delete_story` passes.

### 5.7 Configuration, Static Checks, and Toolchain

- [ ] Invalid TOML and invalid environment overrides fail `ServerConfig::load`.
- [ ] Every `validate` method in §3.15 is called once before binding the HTTP listener or creating runtime services.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo test --workspace --all-features` passes.
- [ ] `cargo +1.85 fmt --all -- --check` passes in the MSRV CI job.
- [ ] `cargo +1.85 clippy --workspace --all-targets --all-features -- -D warnings` passes in the MSRV CI job.
- [ ] `cargo +1.85 test --workspace --all-features` passes in the MSRV CI job.
- [ ] `rg "crate::runtime::(pipeline|turn_budget|turn_execution_ctx|event|trace)" crates/aise/src` returns zero matches.
- [ ] `rg "StoryDraft|lock_turn|turn_lock" crates/aise/src crates/aise-server/src` returns zero matches.
- [ ] `rg "create_story" crates/aise/src/engine.rs crates/aise-server/src/api/turn.rs` returns zero matches.
- [ ] `rg "summary_delta.*story_summary|story_text.*current_scene" crates/aise/src/context crates/aise/src/persistence` returns zero approximation matches.
- [ ] `git diff --check` passes.

---

## 6. Out of Scope / Future Work

- Multi-instance Story ownership requires a separate distributed coordination design and spec.
- Advanced Retrieval, Narrative Graph, Lore Book, and multi-character parallel inference require separate bounded Pipeline specs after this document passes.
- Provisional token streaming requires a separate protocol spec with explicit provisional/retraction semantics.
- Full Event Sourcing requires a separate persistence design; this remediation keeps current state tables authoritative.

---

## 7. References

- Source design: [AISE Technical Architecture v3.1](../design/2026-08-04-Architecture-gpt.md)
- Source review: [Turn Runtime Code Review](../review/2026-08-05-Turn-Runtime-Code-Review-gpt.md)
- Prior execution spec: [Turn Runtime Codegen Spec v1.0](./2026-08004-Turn-Runtime-Codegen-Spec-gpt.md)
- Review evidence: `crates/aise/src/engine.rs:91`, `crates/aise/src/turn/turn_context.rs:161`, `crates/aise/src/turn/turn_validation.rs:20`, `crates/aise/src/validation/validation_pipeline.rs:19`, `crates/aise/src/llm/gateway.rs:176`, `crates/aise/src/llm/openai_compat.rs:119`, `crates/aise/src/persistence/sqlite_store.rs:112`, `crates/aise-server/src/api/turn.rs:48`, `crates/aise-server/src/tasks.rs:36`, `crates/aise-server/src/trace.rs:19`.
- Guardrails: [Architecture and Refactor](../agents/guardrails/architecture-refactor.md), [Layer Dependencies](../agents/guardrails/layer-dependencies.md), [Concurrency](../agents/guardrails/concurrency.md), [Code Organization](../agents/guardrails/code-organization.md), [Observability](../agents/guardrails/observability.md), [Toolchain](../agents/guardrails/toolchain.md).
