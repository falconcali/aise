# Langfuse Trace System Phase 0 — Spec

> **Model**: GPT-5.6 Sol
> **Date**: 2026-09-23
> **Status**: Proposed
> **Source Design**: [Langfuse Trace 系统重构](../../design/2026-09-23-langfuse-trace-system-design-gpt.md)
> **Phase**: Phase 0 of 3

---

## 1. Goal

Establish the final Observation contracts, traced Turn submission boundary, fail-open OpenTelemetry runtime, bounded content handling, and Langfuse OTLP export chain required by later instrumentation.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Add the core `turn/observability/` facade and its unit tests.
- Add `TurnSubmissionService` as the only Turn-attempt submission boundary for HTTP and non-HTTP callers.
- Add environment-only `ObservabilityConfig` parsing with fail-open initialization.
- Add the filtered tracing/OpenTelemetry subscriber layer, baggage propagation processor, batch processor, Langfuse export adapter, and bounded shutdown owner.
- Add bounded content encoding, final export-stage masking, Langfuse attribute mapping, and deterministic diagnostics.
- Pin the OpenTelemetry dependency set and features defined in §3.8.
- Prove queue-after-mapping/masking placement and parent propagation across spawned tasks with tests.

### 2.2 Non-Goals

- Does not instrument every Pipeline, LLM, retrieval, validation, or persistence call; Phase 1 does that.
- Does not restore the disabled Validation/Repair business loop.
- Does not delete the legacy `TurnTrace` path, local trace files, SSE trace payload, or trace UI; Phase 2 deletes them after Phase 1 coverage exists.
- Does not add Langfuse prompt management, datasets, scores, evaluators, metrics export, or log export.
- Does not add a disk WAL or OpenTelemetry Collector requirement.
- Does not make Langfuse availability part of service readiness or Turn success.

### 2.3 Implementation Constraints

- This is one phase of a single hard refactor. The final merged change MUST contain Phases 0–2; no phase may be deployed independently.
- Phase 0 may compile beside the legacy trace path only as an unmerged implementation state. It MUST NOT add a runtime fallback from the new Observation path to the legacy path.
- All new Rust modules MUST use directory entry `mod.rs` files containing declarations and re-exports only.
- Unit tests MUST be placed in `tests/<source>_tests.rs`; do not add inline test modules.
- New code MUST contain no comments or doc comments.
- The core facade MUST NOT contain Langfuse endpoint, credential, queue, or HTTP types.
- No write lock, channel send, exporter call, masking pass, serialization pass, or I/O may occur while a write guard is held.

---

## 3. Contracts

### 3.1 Core Observation Types

Implement these public contracts under `crates/aise/src/turn/observability/`:

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObservationStep {
    ExecuteStoryTurn,
    ResolveInteractionSession,
    ValidateRequest,
    AdmitTurnTask,
    CoordinateStoryTurn,
    LoadStory,
    CheckIdempotency,
    RunTurnPipelines,
    InitializeTurn,
    PrepareContext,
    LoadStorySnapshot,
    ActivateWorldInfo,
    PlanTurn,
    ProjectNarrative,
    GenerateWriterPlan,
    RetrieveContext,
    ThinkCharacters,
    ThinkCharacter,
    GenerateStory,
    DraftStoryText,
    ExtractStoryState,
    InferStoryState,
    ValidateStory,
    RepairStory,
    ReviseStoryText,
    CommitTurn,
    PersistTurn,
}

impl ObservationStep {
    pub const ALL: &'static [Self];
    pub const SCHEMA_VERSION: &'static str = "2";
    pub const fn name(self) -> &'static str;
    pub const fn kind(self) -> ObservationKind;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationStatus {
    Ok,
    Error,
    Cancelled,
    DeadlineExceeded,
    Conflict,
    Incomplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentCapturePolicy {
    MetadataOnly,
    RedactedContent,
    FullContent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationError {
    pub code: String,
    pub failure_kind: String,
    pub stage: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct ObservationFields {
    pub metadata: Vec<ObservationAttribute>,
    pub input: Option<BoundedContent>,
}

#[derive(Debug, Clone, Default)]
pub struct ObservationFinish {
    pub status: ObservationStatus,
    pub metadata: Vec<ObservationAttribute>,
    pub output: Option<BoundedContent>,
    pub error: Option<ObservationError>,
    pub usage: Option<GenerationUsage>,
    pub cost: Option<GenerationCost>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObservationAttribute {
    pub key: &'static str,
    pub value: ObservationValue,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ObservationValue {
    String(String),
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(f64),
    StringList(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationUsage {
    pub input: u64,
    pub input_cached_tokens: u64,
    pub output: u64,
    pub output_reasoning_tokens: u64,
    pub total: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GenerationCost {
    pub currency: &'static str,
    pub input: Option<f64>,
    pub input_cached_tokens: Option<f64>,
    pub output: Option<f64>,
    pub output_reasoning_tokens: Option<f64>,
    pub total: f64,
}
```

`ObservationAttribute::key` MUST come from constants in `fields.rs`; call sites MUST NOT create dynamic attribute names.

### 3.2 Bounded Content Contracts

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentCaptureLimits {
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

pub struct BoundedContentEncoder {
    policy: ContentCapturePolicy,
    limits: ContentCaptureLimits,
}

impl BoundedContentEncoder {
    pub fn new(policy: ContentCapturePolicy, limits: ContentCaptureLimits) -> Self;
    pub fn encode<T: serde::Serialize>(&self, value: &T, remaining_observation_bytes: usize) -> Option<BoundedContent>;
}
```

`encode` MUST return `None` for `MetadataOnly`. It MUST write directly into a bounded UTF-8-safe buffer of at most `min(max_field_bytes + detector_overlap_bytes, remaining_observation_bytes + detector_overlap_bytes)` bytes and MUST NOT first create an unbounded JSON string.

### 3.3 Observation Lifecycle API

```rust
pub struct ObservationSpan {
    span: tracing::Span,
    finished: bool,
}

impl ObservationSpan {
    pub fn begin(step: ObservationStep, fields: ObservationFields) -> Self;
    pub async fn in_scope<F: std::future::Future>(&self, future: F) -> F::Output;
    pub fn finish(self, finish: ObservationFinish);
    pub fn is_recording(&self) -> bool;
}

pub struct ObservationTrace {
    root: ObservationSpan,
    context: opentelemetry::Context,
    finished: bool,
}

impl ObservationTrace {
    pub fn begin(fields: ObservationFields) -> Self;
    pub fn context(&self) -> &opentelemetry::Context;
    pub fn span(&self) -> tracing::Span;
    pub fn bind_session(&mut self, session_id: &str, story_id: &str);
    pub fn bind_request(&mut self, idempotency_key_digest: &str);
    pub fn bind_turn(&mut self, turn_number: u64);
    pub fn finish(self, finish: ObservationFinish);
}

pub async fn observe_result<T, E, F, M>(
    step: ObservationStep,
    fields: ObservationFields,
    future: F,
    map_error: M,
) -> Result<T, E>
where
    F: std::future::Future<Output = Result<T, E>>,
    M: FnOnce(&E) -> ObservationError;
```

`ObservationSpan::Drop` and `ObservationTrace::Drop` MUST finish an unfinished recording span with `ObservationStatus::Incomplete`. The API MUST use `tracing::Instrument`; it MUST NOT hold `span.enter()` across `.await`. All begin, bind, and finish methods MUST have no error return and MUST never change the wrapped business result.

### 3.4 Turn Submission Boundary

Implement under `crates/aise-server/src/turn_submission/`:

```rust
pub struct TurnSubmissionRequest {
    pub raw_session_id: String,
    pub raw_idempotency_key: Option<String>,
    pub player_contribution: String,
    pub cancellation: TurnCancellation,
}

#[derive(Debug, thiserror::Error)]
pub enum TurnSubmissionError {
    #[error("invalid session id")]
    InvalidSession,
    #[error("session not found")]
    SessionNotFound,
    #[error("invalid turn request: {0}")]
    InvalidRequest(String),
    #[error("missing Idempotency-Key header")]
    MissingIdempotencyKey,
    #[error("invalid idempotency key: {0}")]
    InvalidIdempotencyKey(String),
    #[error("turn task admission failed: {0}")]
    Admission(String),
}

pub struct TurnSubmissionService {
    engine: std::sync::Arc<AiseEngine>,
    registry: std::sync::Arc<SessionRegistry>,
    tasks: std::sync::Arc<TurnTaskSupervisor>,
}

impl TurnSubmissionService {
    pub fn new(
        engine: std::sync::Arc<AiseEngine>,
        registry: std::sync::Arc<SessionRegistry>,
        tasks: std::sync::Arc<TurnTaskSupervisor>,
    ) -> Self;

    pub async fn submit(
        &self,
        request: TurnSubmissionRequest,
        sink: std::sync::Arc<dyn TurnEventSink>,
    ) -> Result<(), TurnSubmissionError>;
}
```

The service MUST create `ObservationTrace` before Session parsing or semantic request validation. On successful admission it MUST move the trace exactly once into the spawned Turn task. On admission failure it MUST finish the trace in the request task. Axum extraction failures remain outside this service.

### 3.5 Environment Configuration

Implement under `crates/aise-server/src/observability/config.rs`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ObservabilityConfig {
    pub enabled: bool,
    pub base_url: String,
    pub public_key: Option<String>,
    pub secret_key: Option<String>,
    pub environment: String,
    pub release: String,
    pub sample_rate: f64,
    pub content_policy: ContentCapturePolicy,
    pub full_content_allowed: bool,
    pub max_field_bytes: usize,
    pub max_observation_bytes: usize,
    pub max_queue_size: usize,
    pub max_export_batch_size: usize,
    pub schedule_delay_ms: u64,
    pub http_timeout_ms: u64,
    pub shutdown_timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservabilityConfigIssue {
    pub field: &'static str,
    pub error_kind: &'static str,
}

pub struct ObservabilityConfigLoad {
    pub config: ObservabilityConfig,
    pub issues: Vec<ObservabilityConfigIssue>,
}

impl ObservabilityConfig {
    pub fn load_from_env() -> ObservabilityConfigLoad;
    pub fn load_with(get: impl Fn(&str) -> Option<String>) -> ObservabilityConfigLoad;
    pub fn endpoint(&self) -> Option<String>;
}
```

The environment contract is:

| Variable | Default |
|---|---|
| `LANGFUSE_TRACING_ENABLED` | `false` |
| `LANGFUSE_BASE_URL` | `https://cloud.langfuse.com` |
| `LANGFUSE_PUBLIC_KEY` | unset |
| `LANGFUSE_SECRET_KEY` | unset |
| `LANGFUSE_TRACING_ENVIRONMENT` | `development` |
| `LANGFUSE_RELEASE` | `CARGO_PKG_VERSION` |
| `LANGFUSE_SAMPLE_RATE` | `1.0` |
| `AISE_TRACE_CONTENT_POLICY` | `metadata_only` |
| `AISE_TRACE_FULL_CONTENT_ALLOWED` | `false` |
| `AISE_TRACE_MAX_FIELD_BYTES` | `16384` |
| `AISE_TRACE_MAX_OBSERVATION_BYTES` | `32768` |
| `OTEL_BSP_MAX_QUEUE_SIZE` | `2048` |
| `OTEL_BSP_MAX_EXPORT_BATCH_SIZE` | `256` |
| `OTEL_BSP_SCHEDULE_DELAY` | `1000` |
| `AISE_TRACE_HTTP_TIMEOUT_MS` | `3000` |
| `AISE_TRACE_SHUTDOWN_TIMEOUT_MS` | `5000` |

Invalid enabled configuration MUST produce issues and return `enabled = false`. `full_content` outside environment `development`, or without `AISE_TRACE_FULL_CONTENT_ALLOWED=true`, MUST become `metadata_only` and produce one issue. `http_timeout_ms` MUST be less than `shutdown_timeout_ms`.

### 3.6 Runtime, Processor, Exporter, and Diagnostics

```rust
pub type ObservationLayer = Box<
    dyn tracing_subscriber::Layer<tracing_subscriber::Registry> + Send + Sync
>;

pub struct ObservabilityComponents {
    pub layer: Option<ObservationLayer>,
    pub runtime: ObservabilityRuntime,
}

pub struct ObservabilityRuntime {
    provider: Option<opentelemetry_sdk::trace::SdkTracerProvider>,
    shutdown_timeout: std::time::Duration,
}

impl ObservabilityRuntime {
    pub fn initialize(
        load: ObservabilityConfigLoad,
        diagnostics: TelemetryDiagnostics,
    ) -> ObservabilityComponents;

    pub fn is_enabled(&self) -> bool;
    pub fn shutdown_with_timeout(self) -> ShutdownReport;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShutdownReport {
    pub completed: bool,
    pub dropped_span_count: u64,
    pub error_kind: Option<&'static str>,
}

pub struct TraceAttributePropagationProcessor<P> {
    inner: P,
}

impl<P: opentelemetry_sdk::trace::SpanProcessor> TraceAttributePropagationProcessor<P> {
    pub fn new(inner: P) -> Self;
}

pub struct LangfuseExportAdapter<E> {
    inner: E,
    masker: StreamingMasker,
    diagnostics: TelemetryDiagnostics,
}

impl<E: opentelemetry_sdk::trace::SpanExporter> LangfuseExportAdapter<E> {
    pub fn new(inner: E, masker: StreamingMasker, diagnostics: TelemetryDiagnostics) -> Self;
}

#[derive(Clone)]
pub struct TelemetryDiagnostics;

impl TelemetryDiagnostics {
    pub fn initialization(&self, error_kind: &'static str, endpoint_host: Option<&str>);
    pub fn export(&self, error_kind: &'static str, status: Option<u16>, endpoint_host: &str);
    pub fn dropped(&self, dropped_span_count: u64, queue_capacity: usize);
    pub fn shutdown(&self, report: &ShutdownReport);
}
```

The provider MUST register exactly one outer `TraceAttributePropagationProcessor`, which owns one SDK `BatchSpanProcessor`, which owns one `LangfuseExportAdapter`, which owns the OTLP exporter:

```text
SdkTracerProvider
└── TraceAttributePropagationProcessor
    └── BatchSpanProcessor
        └── LangfuseExportAdapter
            └── OTLP HTTP/protobuf SpanExporter
```

The OTLP endpoint MUST be `{trimmed_base_url}/api/public/otel/v1/traces`. Headers MUST contain `Authorization: Basic base64(public_key:secret_key)` and `x-langfuse-ingestion-version: 4`. Diagnostic output MUST expose only `endpoint_host`, never the full URL or credentials.

### 3.7 Attribute Protocol

Core spans MUST emit only these temporary key families:

```text
aise.observation.type
aise.observation.input
aise.observation.output
aise.observation.model.name
aise.observation.model.parameters
aise.observation.usage_details
aise.observation.cost_details
aise.observation.level
aise.observation.status_message
aise.observation.metadata.*
aise.trace.name
aise.trace.tags
aise.trace.environment
aise.trace.release
aise.trace.metadata.*
aise.schema.version
```

`LangfuseExportAdapter` MUST map them to the corresponding `langfuse.*` keys from the source design and remove every `aise.*` key before export. Unknown `aise.*` keys MUST be dropped.

The baggage allowlist is exactly:

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

### 3.8 Dependencies and Layout

Add workspace dependencies:

```toml
opentelemetry = { version = "0.33.0", default-features = false, features = ["trace"] }
opentelemetry_sdk = { version = "0.33.0", default-features = false, features = ["trace", "internal-logs"] }
opentelemetry-otlp = { version = "0.33.0", default-features = false, features = ["trace", "http-proto", "reqwest-blocking-client", "reqwest-rustls", "internal-logs"] }
tracing-opentelemetry = { version = "0.33.0", default-features = false, features = ["tracing-log"] }
```

Final Phase 0 additions:

```text
crates/aise/src/turn/observability/
├── mod.rs
├── fields.rs
├── span.rs
├── step.rs
├── trace.rs
└── tests/
    ├── fields_tests.rs
    ├── span_tests.rs
    ├── step_tests.rs
    └── trace_tests.rs

crates/aise-server/src/observability/
├── mod.rs
├── baggage_processor.rs
├── config.rs
├── diagnostics.rs
├── langfuse_exporter.rs
├── propagation.rs
├── runtime.rs
└── tests/
    ├── baggage_processor_tests.rs
    ├── config_tests.rs
    ├── langfuse_exporter_tests.rs
    ├── propagation_tests.rs
    └── runtime_tests.rs

crates/aise-server/src/turn_submission/
├── mod.rs
├── service.rs
└── tests/
    └── service_tests.rs
```

---

## 4. Behavior Rules

1. **P0-R1**: `ObservationStep::ALL` MUST contain each enum variant exactly once; each name MUST match `^[a-z0-9-]+ \(.*\)$`; each name and kind MUST match Phase 1 §3.1.
2. **P0-R2**: Observation names, kinds, and schema version MUST be compile-time constants; IDs, model names, attempts, and Turn numbers MUST be attributes, never names.
3. **P0-R3**: Disabled spans MUST skip content serialization and heap allocation.
4. **P0-R4**: A dropped unfinished `ObservationSpan` or `ObservationTrace` MUST record status `incomplete` without panicking.
5. **P0-R5**: `observe_result` MUST return the future's original `Result<T, E>` unchanged.
6. **P0-R6**: `TurnSubmissionService::submit` MUST start the root before Session parsing and semantic validation.
7. **P0-R7**: Axum path, header, and JSON extraction failures MUST NOT create a Turn Trace.
8. **P0-R8**: Successful admission MUST transfer root ownership once; failed admission MUST finish it in the caller task.
9. **P0-R9**: `AiseEngine::execute_turn` MUST NOT create a second root Observation.
10. **P0-R10**: `TraceAttributePropagationProcessor::on_start` MUST copy only the fixed baggage allowlist and MUST perform no JSON serialization, masking, regex work, blocking call, or I/O.
11. **P0-R11**: Mapping, final masking, protobuf encoding, and HTTP export MUST execute after BatchSpanProcessor enqueue on its worker.
12. **P0-R12**: Queue insertion from span end MUST be non-blocking; a full queue MUST drop telemetry and leave the business result unchanged.
13. **P0-R13**: Sampling MUST be `ParentBased(TraceIdRatioBased(sample_rate))`.
14. **P0-R14**: Initialization failure MUST disable only the OpenTelemetry layer; normal logging and service startup MUST continue.
15. **P0-R15**: The tracing filter MUST route `aise::observation` only to the OpenTelemetry layer, route normal logs only to fmt layers, and prevent `aise::telemetry` and OpenTelemetry internal logs from re-entering OpenTelemetry.
16. **P0-R16**: The runtime MUST be created once in `main`, owned only by `main`, and shut down after Turn task drain.
17. **P0-R17**: `main` MUST invoke blocking flush/shutdown through `tokio::task::spawn_blocking`.
18. **P0-R18**: Shutdown MUST stop waiting at `shutdown_timeout_ms`; timeout MUST be diagnostic-only.
19. **P0-R19**: Every exported string attribute MUST pass through the final streaming masker, including metadata and error/status strings.
20. **P0-R20**: The masker overlap MUST be at least the maximum detector cross-block width, and masking MUST occur before final byte truncation.
21. **P0-R21**: API keys, Authorization values, cookies, and secret-key patterns MUST be masked in every content policy.
22. **P0-R22**: Telemetry diagnostics MUST use structured `error_kind`, `status`, `dropped_span_count`, `queue_capacity`, and `endpoint_host` fields and MUST be rate-limited by error kind.
23. **P0-R23**: Generation usage buckets MUST be mutually exclusive and `total` MUST equal their sum.
24. **P0-R24**: Cost details MUST be emitted only when currency is confirmed as USD and bucket semantics are known.

### 4.1 Error Handling

- Observation facade methods MUST not return telemetry errors.
- Invalid enabled configuration MUST disable tracing and emit one structured warning per distinct issue.
- Export failures for DNS, TLS, timeout, 401/403, 429, and 5xx MUST be diagnostic-only.
- When a typed HTTP status is unavailable, diagnostics MUST use `status = "unknown"` and MUST NOT parse error strings.
- Serialization failure MUST omit the affected content field, add `content_encode_failed=true`, and preserve the business result.

### 4.2 Concurrency

- The SDK batch queue capacity is `max_queue_size`; no second application queue or side cache is permitted.
- Provider, processor, exporter, and worker have one owner: `ObservabilityRuntime`.
- No Observation context, history, or content buffer may be shared across Turns.
- The root context MUST be attached explicitly to every spawned Turn task.
- No mutex or write guard may be held across `.await`, channel send, span finish, or diagnostic emission.

### 4.3 Observability

- Root span target MUST be `aise::observation`.
- Telemetry self-diagnostics target MUST be `aise::telemetry`.
- Resource attributes MUST include `service.name = "aise-server"`, crate version, environment, release, and Observation schema version.
- Initialization MUST log `enabled`, `environment`, `release`, `sample_rate`, `content_policy`, `queue_capacity`, and `endpoint_host` without credentials.

---

## 5. Acceptance Criteria

- [ ] All files in §3.8 exist and each `mod.rs` contains declarations/re-exports only.
- [ ] `ObservationStep::ALL`, names, kinds, and schema version pass `cargo test -p aise step_tests`.
- [ ] Disabled begin/finish performs zero heap allocations in the dedicated allocation-count test.
- [ ] RAII incomplete and explicit finish behavior pass `cargo test -p aise span_tests`.
- [ ] Cross-task parent propagation and late binding pass `cargo test -p aise trace_tests`.
- [ ] Invalid/missing credentials, invalid URL, invalid environment, invalid ranges, and production full-content cases disable tracing without returning an error from config load.
- [ ] Provider construction test proves exactly one outer propagation processor and one inner BatchSpanProcessor/export adapter chain.
- [ ] A blocking masker test proves `ObservationSpan::finish` latency does not include masker delay.
- [ ] A saturated queue test proves Turn completion does not wait for exporter progress.
- [ ] Fake OTLP server verifies `/api/public/otel/v1/traces`, Basic Auth, ingestion version `4`, and protobuf content.
- [ ] Secret tests cover every byte boundary around truncation and every masker block boundary.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo test --workspace` passes.

---

## 6. Out of Scope / Future Work

- Full business-flow instrumentation is specified in [Phase 1](./2026-09-23-langfuse-trace-system-spec-phase-1-gpt.md).
- Legacy trace deletion, API/UI cleanup, performance gates, and deployment smoke tests are specified in [Phase 2](./2026-09-23-langfuse-trace-system-spec-phase-2-gpt.md).

---

## 7. References

- Source design: [Langfuse Trace 系统重构](../../design/2026-09-23-langfuse-trace-system-design-gpt.md)
- Architecture: [AISE Architecture](../../design/2026-08-04-Architecture-gpt.md)
- Guardrails: [Agent guardrails](../../agents/README.md)
- Langfuse OpenTelemetry integration: <https://langfuse.com/integrations/native/opentelemetry>
