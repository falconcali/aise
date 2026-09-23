# Langfuse Trace System Phase 2 — Spec

> **Model**: GPT-5.6 Sol
> **Date**: 2026-09-23
> **Status**: Proposed
> **Source Design**: [Langfuse Trace 系统重构](../../design/2026-09-23-langfuse-trace-system-design-gpt.md)
> **Phase**: Phase 2 of 3

---

## 1. Goal

Delete the legacy Trace system and product surface, finalize runtime wiring and environment configuration, and pass performance plus Cloud/self-hosted Langfuse acceptance gates.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Delete the custom Trace DTO, recorder, sinks, local JSON/JSONL writer, hand-written OTLP conversion, and all dependent tests.
- Remove `include_trace`, `TurnEvent::TraceCompleted`, SSE `trace`/`trace_completed`, and the browser Trace viewer.
- Remove all legacy Trace limits and TOML Langfuse configuration.
- Separate ordinary log storage into `log_dir` / `AISE_LOG_DIR`.
- Finalize `main` ownership, subscriber filters, Turn task drain, and bounded blocking observability shutdown.
- Update configuration examples and documents that describe the deleted path.
- Add reproducible disabled, metadata-only, redacted-content, blocked-exporter, and memory benchmarks.
- Complete fake-server, Langfuse Cloud, and OTLP-v4 self-hosted smoke tests.

### 2.2 Non-Goals

- Does not restore Validation/Repair execution.
- Does not preserve old Trace JSON files, schemas, retention behavior, APIs, UI, or configuration.
- Does not migrate old local Trace files into Langfuse.
- Does not add a compatibility alias for `LANGFUSE_ENABLED`, `[langfuse]`, or old trace environment variables.
- Does not add a Collector, telemetry disk queue, or crash-durable final batch.
- Does not add prompt management, datasets, scores, evaluators, metrics export, or log export.

### 2.3 Implementation Constraints

- This phase completes a hard refactor. No fallback branch, adapter, deprecated alias, dual-write, dead feature flag, or commented legacy code may remain.
- The old path and all tests/docs/config that assert it MUST be deleted in the same change.
- `LangfuseExportAdapter` via OTLP/HTTP protobuf MUST be the only Trace persistence path.
- Ordinary logging MUST remain available when Observation export is disabled or broken.
- Performance gates MUST use the same deterministic representative Turn fixture for baseline and instrumented runs.

---

## 3. Contracts

### 3.1 Final Turn Request API

The request DTO becomes:

```rust
#[derive(Debug, Deserialize)]
pub struct TurnRequest {
    pub player_contribution: String,
}
```

Requests containing `include_trace` MUST be accepted only according to the existing Serde unknown-field policy; the field has no effect and MUST NOT appear in generated requests, tests, or documentation. Do not add a compatibility field.

### 3.2 Final Turn Event Protocol

The final event enum is:

```rust
#[derive(Debug, Clone)]
pub enum TurnEvent {
    StageStarted {
        turn_number: Option<TurnNumber>,
        stage: TurnStage,
    },
    ValidationCompleted {
        turn_number: Option<TurnNumber>,
        attempt: u32,
        decision: ValidationDecision,
        issue_codes: Vec<ValidationIssueCode>,
    },
    Committed {
        result: CommittedTurnResult,
        replayed: bool,
    },
    Failed {
        turn_number: Option<TurnNumber>,
        code: &'static str,
    },
    Cancelled {
        turn_number: Option<TurnNumber>,
        code: &'static str,
    },
    Conflict {
        turn_number: Option<TurnNumber>,
        code: &'static str,
    },
}
```

The SSE event names are exactly:

```text
stage
validation
committed
failed
cancelled
conflict
```

No event payload may contain a Trace, Span tree, prompt, provider response, or Langfuse credential.

### 3.3 Final SSE Sink API

```rust
pub struct SseSink {
    progress_tx: tokio::sync::mpsc::Sender<axum::response::sse::Event>,
    terminal_tx: tokio::sync::mpsc::Sender<axum::response::sse::Event>,
    terminal_sent: std::sync::atomic::AtomicBool,
    dropped: std::sync::atomic::AtomicUsize,
}

impl SseSink {
    pub fn new(
        progress_tx: tokio::sync::mpsc::Sender<axum::response::sse::Event>,
        terminal_tx: tokio::sync::mpsc::Sender<axum::response::sse::Event>,
    ) -> Self;

    pub fn new_shared(
        tx: tokio::sync::mpsc::Sender<axum::response::sse::Event>,
    ) -> Self;
}
```

`include_trace` MUST be removed from fields, constructors, handlers, and call sites.

### 3.4 Final Server Configuration

`ServerConfig` MUST contain ordinary log storage but no Trace persistence or Langfuse fields:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub listen_addr: std::net::SocketAddr,
    pub assets_dir: Option<std::path::PathBuf>,
    pub max_sessions: usize,
    pub max_concurrent_turns: usize,
    pub admission_capacity: usize,
    pub admission_timeout_ms: u64,
    pub shutdown_grace_ms: u64,
    pub log_dir: std::path::PathBuf,
    pub aise: AiseConfig,
}
```

`AISE_LOG_DIR` overrides `log_dir`; the default is `log`. Langfuse and Observation configuration MUST be loaded only by `ObservabilityConfig::load_from_env`.

Delete these `ServerConfig` fields and all defaults, parsers, validators, and tests for them:

```text
trace_dir
trace_channel_capacity
trace_max_record_bytes
trace_rotation_bytes
trace_retention_files
trace_shutdown_grace_ms
langfuse
```

Delete these core configuration fields:

```text
LlmConfig::trace_content
TurnConfig::max_trace_spans
ContentConfig::max_trace_field_bytes
TurnBudget::max_trace_spans
```

### 3.5 Final Environment Contract

`.env.example` MUST document exactly the active Observation variables:

```dotenv
LANGFUSE_TRACING_ENABLED=false
LANGFUSE_BASE_URL=https://cloud.langfuse.com
LANGFUSE_PUBLIC_KEY=
LANGFUSE_SECRET_KEY=
LANGFUSE_TRACING_ENVIRONMENT=development
LANGFUSE_RELEASE=
LANGFUSE_SAMPLE_RATE=1.0
AISE_TRACE_CONTENT_POLICY=metadata_only
AISE_TRACE_FULL_CONTENT_ALLOWED=false
AISE_TRACE_MAX_FIELD_BYTES=16384
AISE_TRACE_MAX_OBSERVATION_BYTES=32768
OTEL_BSP_MAX_QUEUE_SIZE=2048
OTEL_BSP_MAX_EXPORT_BATCH_SIZE=256
OTEL_BSP_SCHEDULE_DELAY=1000
AISE_TRACE_HTTP_TIMEOUT_MS=3000
AISE_TRACE_SHUTDOWN_TIMEOUT_MS=5000
AISE_LOG_DIR=log
```

The following legacy variables MUST have zero parser or documentation matches:

```text
LANGFUSE_ENABLED
LANGFUSE_ENVIRONMENT
LANGFUSE_MAX_QUEUE_SIZE
LANGFUSE_MAX_EXPORT_BATCH_SIZE
LANGFUSE_SCHEDULED_DELAY_MS
LANGFUSE_EXPORT_TIMEOUT_MS
LANGFUSE_SHUTDOWN_TIMEOUT_MS
LANGFUSE_MAX_REQUEST_BYTES
AISE_TRACE_DIR
AISE_TRACE_CHANNEL_CAPACITY
AISE_TRACE_MAX_RECORD_BYTES
AISE_TRACE_ROTATION_BYTES
AISE_TRACE_RETENTION_FILES
AISE_TRACE_SHUTDOWN_GRACE_MS
```

### 3.6 Final Application Composition

`AppState` MUST expose the submission service instead of allowing the Turn handler to assemble the engine/task path:

```rust
pub struct AppState {
    pub turn_submission: std::sync::Arc<TurnSubmissionService>,
    pub registry: std::sync::Arc<SessionRegistry>,
    pub config: ServerConfig,
    pub pack_service: Option<std::sync::Arc<PackService>>,
    pub character_card_service: Option<std::sync::Arc<CharacterCardService>>,
    pub instance_factory: Option<std::sync::Arc<StoryInstanceFactory>>,
    pub story_history_reader: Option<std::sync::Arc<dyn StoryHistoryReadPort>>,
    pub activation_preview: Option<std::sync::Arc<KnowledgeActivationPreviewService>>,
}
```

The application startup/shutdown order is:

```text
1. Load ServerConfig.
2. Load ObservabilityConfig from environment.
3. Build ordinary logging layers and optional Observation layer.
4. Initialize the global subscriber once.
5. Build engine services, SessionRegistry, TurnTaskSupervisor, and TurnSubmissionService.
6. Serve HTTP.
7. Stop admission and drain Turn tasks.
8. Stop HTTP.
9. Move ObservabilityRuntime into spawn_blocking.
10. Flush/shutdown within the configured timeout.
11. Drop the ordinary log appender guard and exit.
```

### 3.7 Mandatory Deletions

Delete these files and directories:

```text
crates/aise/src/turn/turn_trace.rs
crates/aise/tests/turn_trace_tests.rs
crates/aise-server/src/trace/
crates/aise-server/tests/trace_sink_tests.rs
```

Delete all imports, exports, fields, constructors, methods, variants, payloads, and tests containing these legacy symbols:

```text
TurnTrace
TraceId
TraceSpan
TraceRecord
TraceRecorder
PendingSpan
TraceSpanSink
SpanPayload
TurnData
PipelineData
LlmCallData
LlmCallContent
StructuredCallData
MessageData
ToolCallData
ValidationData
PersistData
CompositeTraceSink
LangfuseTraceSink
TraceWriter
TraceCompleted
include_trace
```

If a payload type is still needed for non-Trace business behavior, replace it with a domain-owned type in the owning module; do not retain the legacy name or module.

### 3.8 Browser UI Deletion

Remove from `crates/aise-server/assets/`:

```text
include_trace request construction
trace/trace_completed SSE handling
Trace tab or panel
Trace tree rendering
Trace JSON rendering
Trace-specific CSS selectors
Trace state and storage
```

The remaining UI MUST continue to show stage progress, validation events, committed story text, and terminal failures.

### 3.9 Documentation Updates

Update these documents where they describe current behavior:

```text
doc/design/2026-08-04-Architecture-gpt.md
doc/exec/2026-08-06-turn-runtime-review-remediation-spec-gpt.md
doc/exec/2026-08004-Turn-Runtime-Codegen-Spec-gpt.md
doc/exec/CSI-RC-FTI/2026-08-18-turn-runtime-contract-alignment-spec-gpt.md
```

Historical specs that are not normative MAY retain historical statements only when they are explicitly labeled superseded and link to the source design plus this spec. Current architecture/configuration documentation MUST contain no claim that local Trace JSON, `TraceCompleted`, or `TurnTrace` is active.

### 3.10 Benchmark Contract

Add a reproducible benchmark or ignored integration benchmark with these modes:

```rust
pub enum TraceBenchmarkMode {
    Disabled,
    MetadataOnly,
    RedactedContent,
    BlockedExporter,
}

pub struct TraceBenchmarkResult {
    pub mode: TraceBenchmarkMode,
    pub completed_turns: u64,
    pub p50_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub peak_rss_bytes: u64,
    pub exported_spans: u64,
    pub dropped_spans: u64,
}
```

The harness MUST use:

```text
warm-up completed Turns: 100
measured completed Turns: at least 1000 per mode
concurrency: 8
sampling: 100% for enabled modes
same deterministic provider/store fixture for every mode
blocked-exporter duration: at least 2 × schedule delay
```

Regression formulas are:

```text
disabled_regression = (disabled_p95 - no_facade_baseline_p95) / no_facade_baseline_p95
metadata_regression = (metadata_p95 - disabled_p95) / disabled_p95
redacted_regression = (redacted_p95 - disabled_p95) / disabled_p95
```

### 3.11 Deployment Smoke Contract

Add an opt-in smoke command or ignored test accepting:

```text
LANGFUSE_BASE_URL
LANGFUSE_PUBLIC_KEY
LANGFUSE_SECRET_KEY
LANGFUSE_TRACING_ENVIRONMENT
```

It MUST submit:

```text
1 successful Turn
1 early semantic failure
1 replayed Turn
1 conditional-skip Turn
1 repair Turn only when the business Validation/Repair loop is enabled
```

It MUST query the Langfuse API/CLI for the emitted Trace IDs and verify root input/output, hierarchy, types, names, model, usage, cost when available, Session, environment, release, and metadata. The same command MUST run against Cloud and one self-hosted Langfuse v4 endpoint by changing environment variables only.

---

## 4. Behavior Rules

1. **P2-R1**: The repository MUST have one runtime Trace model: `tracing`/OpenTelemetry spans.
2. **P2-R2**: The repository MUST have one Trace persistence path: OTLP/HTTP protobuf through `LangfuseExportAdapter`.
3. **P2-R3**: No local Trace JSON or JSONL file may be created.
4. **P2-R4**: No HTTP request, response, SSE event, or frontend state may expose the Observation tree.
5. **P2-R5**: Legacy config names MUST be rejected or ignored by the existing generic unknown-field policy; no alias parser is allowed.
6. **P2-R6**: `log_dir` controls ordinary log files only and MUST NOT be used as a Trace directory.
7. **P2-R7**: Disabled Observation mode MUST create no exporter, provider worker, batch queue, or telemetry HTTP client.
8. **P2-R8**: Exporter blockage, queue overflow, endpoint failure, and shutdown timeout MUST not delay or alter Turn outcomes.
9. **P2-R9**: Batch queue, batch size, content fields, per-Observation content, and shutdown time MUST remain hard-bounded by typed configuration.
10. **P2-R10**: Default worst-case queued content MUST remain at or below `2048 × 32 KiB = 64 MiB`, excluding fixed Span overhead.
11. **P2-R11**: Total measured observability memory increment MUST remain at or below 96 MiB under the benchmark contract.
12. **P2-R12**: Disabled p95 latency regression MUST be less than 1%.
13. **P2-R13**: Metadata-only p95 latency regression relative to disabled MUST be less than 3%.
14. **P2-R14**: Redacted-content p95 latency regression relative to disabled MUST be less than 5%.
15. **P2-R15**: Memory after warm-up MUST not grow linearly with completed Turn count.
16. **P2-R16**: A blocked exporter MUST not add network wait time to Turn latency.
17. **P2-R17**: Cloud/self-host switching MUST require environment changes only.
18. **P2-R18**: Service shutdown MUST drain Turn tasks before observability flush.
19. **P2-R19**: Blocking observability shutdown MUST run only in `spawn_blocking`.
20. **P2-R20**: Crash/abort may lose the final queued batch; no disk fallback may be introduced.

### 4.1 Error Handling

- Missing credentials, malformed URL, invalid environment, invalid queue values, and exporter construction failure MUST disable Observation export and leave startup successful.
- Legacy Trace writer initialization errors no longer exist and MUST not be replaced.
- Shutdown timeout MUST log a structured warning with dropped count and exit without indefinite wait.
- UI removal MUST not convert unknown SSE events into fatal client errors.

### 4.2 Concurrency

- `ObservabilityRuntime` is the sole owner of provider and batch worker lifetime.
- Turn task cancellation and client disconnect MUST NOT cancel the exporter worker.
- Queue overflow MUST drop spans through SDK behavior without blocking a Turn.
- No replacement queue, local cache, retry task, or sidecar writer may be added.

### 4.3 Observability

- Startup logs MUST report Observation enabled state and non-secret effective limits.
- Queue/export/shutdown diagnostics MUST remain in ordinary logs and MUST not recurse into OpenTelemetry.
- Ordinary logs MUST use `log_dir`; startup MUST no longer emit a `trace_dir` field.
- Saved-view-compatible names and schema version from Phase 1 MUST remain unchanged.

---

## 5. Acceptance Criteria

- [ ] `crates/aise/src/turn/turn_trace.rs`, `crates/aise/tests/turn_trace_tests.rs`, `crates/aise-server/src/trace/`, and `crates/aise-server/tests/trace_sink_tests.rs` do not exist.
- [ ] `rg 'TurnTrace|TraceId|TraceSpan|TraceRecord|TraceRecorder|PendingSpan|TraceSpanSink|SpanPayload|CompositeTraceSink|LangfuseTraceSink|TraceWriter|TraceCompleted' crates/` returns zero matches.
- [ ] `rg 'include_trace|trace_completed' crates/` returns zero matches.
- [ ] `rg 'max_trace_spans|max_trace_field_bytes|trace_content' crates/ config/` returns zero matches.
- [ ] `rg 'LANGFUSE_ENABLED|LANGFUSE_ENVIRONMENT|LANGFUSE_MAX_QUEUE_SIZE|LANGFUSE_MAX_EXPORT_BATCH_SIZE|LANGFUSE_SCHEDULED_DELAY_MS|LANGFUSE_EXPORT_TIMEOUT_MS|LANGFUSE_SHUTDOWN_TIMEOUT_MS|LANGFUSE_MAX_REQUEST_BYTES|AISE_TRACE_DIR|AISE_TRACE_CHANNEL_CAPACITY|AISE_TRACE_MAX_RECORD_BYTES|AISE_TRACE_ROTATION_BYTES|AISE_TRACE_RETENTION_FILES|AISE_TRACE_SHUTDOWN_GRACE_MS' crates/ config/ .env.example` returns zero matches.
- [ ] `config/aise_config.toml` has no `[langfuse]` table and no Trace content/size setting.
- [ ] `.env.example` contains every active variable from §3.5.
- [ ] UI browser test proves no Trace controls exist and normal Turn progress/terminal rendering still works.
- [ ] Filesystem integration test proves a Turn creates no Trace JSON/JSONL files.
- [ ] Disabled benchmark passes `<1%`; metadata-only passes `<3%`; redacted-content passes `<5%`.
- [ ] Blocked-exporter benchmark proves no network-duration coupling and unchanged business results.
- [ ] Long-run benchmark reports no Turn-count-proportional memory growth and at most 96 MiB observability increment.
- [ ] Fake OTLP server tests pass for endpoint, auth, ingestion header, protobuf, queue overflow, and shutdown timeout.
- [ ] Cloud smoke test passes when credentials are provided.
- [ ] Self-hosted Langfuse v4 smoke test passes with the same binary and only environment changes.
- [ ] Current architecture documentation contains no active local Trace writer, `TurnTrace`, or `TraceCompleted` contract.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo test --workspace` passes.

---

## 6. Out of Scope / Future Work

- Restoring Validation/Repair execution requires a separate business-flow spec.
- Content-enabled production evaluation requires a separate data classification, retention, and access-control review.
- A future OpenTelemetry Collector deployment may replace the endpoint topology without changing core instrumentation.

---

## 7. References

- Source design: [Langfuse Trace 系统重构](../../design/2026-09-23-langfuse-trace-system-design-gpt.md)
- Phase 0 contracts: [Phase 0](./2026-09-23-langfuse-trace-system-spec-phase-0-gpt.md)
- Phase 1 instrumentation: [Phase 1](./2026-09-23-langfuse-trace-system-spec-phase-1-gpt.md)
- Guardrails: [Agent guardrails](../../agents/README.md)
- Langfuse OpenTelemetry integration: <https://langfuse.com/integrations/native/opentelemetry>
- Langfuse best practices: <https://langfuse.com/docs/observability/best-practices>
