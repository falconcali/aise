# Observation Future Instrumentation Removal — Spec

> **Model**: GPT-5.6 Sol
> **Date**: 2026-09-25
> **Status**: Proposed
> **Source Design**: [Langfuse Trace 系统重构](../design/2026-09-23-langfuse-trace-system-design-gpt.md)
> **Source Refactor**: [Langfuse Observability 数据模型对齐](../refactor/2026-09-25-langfuse-observability-model-refactor-gpt.md)
> **Supersedes**: Future-instrumentation contracts in [Langfuse Observability Model Alignment](./2026-09-25-langfuse-observability-model-spec-gpt.md)
> **Phase**: Remediation

---

## 1. Goal

Remove Future-based Observation instrumentation and execute every observed asynchronous operation through direct `.await` while preserving explicit `&Trace` and `&Observation` parent propagation.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Delete `Trace::trace` and `Observation::trace`.
- Delete every business Observation wrapper method that accepts a generic `Future`.
- Remove Observation lifecycle calls shaped as `.trace(future)`, `.in_scope(future)`, or `future.instrument(observation_span)`.
- Change every affected call site to `begin` → direct business `.await` → `finish`.
- Preserve parent-child relationships through the existing explicit `&Trace` and `&Observation` parameters.
- Preserve existing Observation names, kinds, metadata, content capture, status mapping, error mapping, and module ownership.
- Remove stale Future-instrumentation requirements from active observability documents and examples.

### 2.2 Non-Goals

- Does not redesign or relocate existing business observability facades.
- Does not change facade-owned `ObservationSpec`, `ObservationOutcome`, metadata, content, or error mapping.
- Does not change `TurnExecutionPipeline::execute` other than retaining its existing explicit `&Observation` parameter.
- Does not add new Observation types, names, attributes, or hierarchy levels.
- Does not change Session or Trace ownership.
- Does not change HTTP, SSE, persistence, prompts, budgets, retries, cancellation, deadlines, or LLM concurrency limiting.
- Does not remove unrelated diagnostic `tracing` spans used only for structured logs.
- Does not add compatibility wrappers or retain the removed API under another name.

### 2.3 Implementation Constraints

- This is a hard removal under `R-REFACTOR-01` and `R-REFACTOR-02`.
- Delete superseded methods, imports, call sites, tests, and active document claims in the same change.
- Do not introduce `run`, `scope`, `instrument`, `observe`, or another generic Future wrapper as a replacement.
- Do not use `Span::current()`, task-local state, thread-local state, global state, or `TurnExecutionContext` to recover an Observation parent.
- Top-level `observability` remains a business-agnostic leaf module.
- Existing module observability facades and their ownership boundaries remain authoritative.
- Existing bounded content, queue, and concurrency behavior MUST remain unchanged.

---

## 3. Contracts

### 3.1 Top-Level Lifecycle API

The final API MUST expose no method generic over `Future`:

```rust
impl Trace {
    pub fn root(&self) -> &Observation;
    pub fn begin_observation(&self, spec: ObservationSpec) -> Observation;
    pub fn bind(&mut self, attributes: Vec<Attribute>);
    pub fn content_capture(&self) -> &ContentCapture;
    pub fn finish(self, outcome: TraceOutcome);
}

impl Observation {
    pub fn begin(&self, spec: ObservationSpec) -> Observation;
    pub fn content_capture(&self) -> &ContentCapture;
    pub fn finish(self, outcome: ObservationOutcome);
    pub fn is_recording(&self) -> bool;
}
```

These APIs MUST be deleted:

```rust
pub async fn trace<F: Future>(&self, future: F) -> F::Output;
pub async fn in_scope<F: Future>(&self, future: F) -> F::Output;
```

`Observation::begin` MUST continue to set the receiver's stored OpenTelemetry context as the explicit parent. It MUST NOT enter the span or instrument a Future.

### 3.2 Leaf Operation Execution

Replace:

```rust
let observation = begin_load_story_snapshot_observation(parent, ctx);
let outcome = observation
    .trace(self.store.load_story_snapshot(&story_id, limits))
    .await;
finish_observation(observation, &outcome);
```

With:

```rust
let observation = begin_load_story_snapshot_observation(parent, ctx);
let outcome = self.store.load_story_snapshot(&story_id, limits).await;
finish_observation(observation, &outcome);
```

The business Future MUST be awaited directly by the orchestration function.

### 3.3 Nested Operation Execution

Replace:

```rust
let observation = begin_pipeline_stage_observation(parent, stage);
let outcome = pipeline.execute(ctx, &observation).await;
finish_observation(observation, &outcome);
```

With:

```rust
let observation = begin_pipeline_stage_observation(parent, stage);
let outcome = pipeline.execute(ctx, &observation).await;
finish_observation(observation, &outcome);
```

The nested function MUST receive the existing explicit parent parameter:

```rust
async fn execute(
    &self,
    ctx: &mut TurnExecutionContext,
    observation: &Observation,
) -> Result<(), TurnExecutionError>;
```

### 3.4 Trace-Level Execution

Replace:

```rust
let observation = begin_run_turn_pipelines_observation(trace);
let result = self.run_inner(ctx, sink, &observation).await;
finish_observation(observation, ctx, &result);
```

With:

```rust
let observation = begin_run_turn_pipelines_observation(trace);
let result = self.run_inner(ctx, sink, &observation).await;
finish_observation(observation, ctx, &result);
```

Engine and submission operations MUST use the same direct-await shape.

### 3.5 LLM Execution

LLM generation, streaming, and embedding Observations MUST be started before limiter/provider execution and finished after the typed result is available:

```rust
let observation = begin_generation_observation(parent, request);
let result = self.execute_completion(request, &observation).await;
finish_observation(observation, &result);
```

Every actual LLM call MUST continue through the existing shared injected concurrency limiter. Removing Future instrumentation MUST NOT move, duplicate, or bypass limiter acquisition.

### 3.6 Mandatory Deletions

Delete:

```text
Trace::trace
Observation::trace
*Observation::trace
ObservationSpan::in_scope
observe_result when used as an Observation Future wrapper
observation.trace(future)
observation.in_scope(future)
future.instrument(observation_span)
```

Remove `std::future::Future` and `tracing::Instrument` imports when they become unused after these deletions.

Unrelated diagnostic spans MAY retain `tracing::Instrument`, but they MUST NOT use target `aise::observation`, establish Langfuse Observation parentage, or replace explicit parent parameters.

### 3.7 Documentation Precedence

This specification supersedes:

- `trace(Future)` and Future-instrumentation requirements in `doc/refactor/2026-09-25-langfuse-observability-model-refactor-gpt.md`.
- `Trace::trace`, `Observation::trace`, wrapper `trace(Future)`, and `tracing::Instrument` requirements in `doc/exec/2026-09-25-langfuse-observability-model-spec-gpt.md`.
- Any active observability example that requires `.trace(future)` or `.in_scope(future)`.

The affected documents MUST be updated to point to this remediation and MUST NOT remain contradictory active instructions.

---

## 4. Behavior Rules

1. **OFIR-R1**: Observation ancestry MUST be determined only by the supplied `&Trace` or `&Observation`.
2. **OFIR-R2**: No Observation lifecycle type or business Observation wrapper may expose a method generic over `Future`.
3. **OFIR-R3**: No Observation lifecycle type or wrapper may poll, enter, scope, or instrument a business Future.
4. **OFIR-R4**: Every affected business Future MUST be awaited directly at its existing orchestration call site.
5. **OFIR-R5**: Starting an Observation before the direct `.await` and finishing it afterward MUST define the measured lifecycle.
6. **OFIR-R6**: Existing explicit parent parameters MUST remain unchanged and MUST be forwarded to nested Observation creators.
7. **OFIR-R7**: `TurnExecutionContext` MUST contain no Observation, OpenTelemetry context, or hidden parent state.
8. **OFIR-R8**: Explicit finish and `Drop` MUST continue to emit exactly one terminal state.
9. **OFIR-R9**: Dropping an unfinished Trace or Observation MUST continue to record `Incomplete`.
10. **OFIR-R10**: Removing Future instrumentation MUST NOT change any business result, error, state transition, cancellation, or deadline behavior.
11. **OFIR-R11**: Existing facade-owned names, kinds, metadata, content, status, usage, cost, and error mapping MUST remain unchanged.
12. **OFIR-R12**: A skipped operation MUST continue to create no child Observation.
13. **OFIR-R13**: Parallel children MUST continue to derive their parent IDs from the same explicitly supplied parent.
14. **OFIR-R14**: Every actual LLM operation MUST remain protected by the shared concurrency limiter.
15. **OFIR-R15**: Telemetry-disabled, exporter-failing, queue-full, and serialization-failing paths MUST preserve the original business result.
16. **OFIR-R16**: No active design, refactor, spec, guardrail example, or code example may prescribe Future instrumentation for Observation lifecycle.

### 4.1 Error Handling

- Existing typed business errors MUST pass through unchanged.
- Existing facade error-to-Observation mapping MUST remain unchanged.
- Early returns MUST finish their owned Observation or preserve the existing `Incomplete` drop behavior.
- No ambient-parent fallback is permitted.

### 4.2 Concurrency

- `Trace` and `Observation` MUST remain non-`Clone` lifecycle owners.
- A shared `&Observation` MAY be borrowed by concurrent children only while the parent remains alive.
- No write guard may be held across a business `.await`, Observation finish, event emission, or I/O.
- This change MUST add no task, queue, channel, cache, history, or fan-out.

### 4.3 Observability

- Observation spans MUST retain target `aise::observation`.
- `Observation::begin` MUST retain explicit OpenTelemetry parent assignment.
- Parent-ID and lifecycle tests MUST pass without entering or instrumenting the Observation span.
- Existing Langfuse tree shape and schema version MUST remain unchanged.

---

## 5. Acceptance Criteria

- [ ] `Trace` and `Observation` expose no Future-generic method.
- [ ] No business Observation wrapper exposes a Future-generic method.
- [ ] `crates/aise/src/context/baseline_ctx_builder.rs` directly awaits snapshot loading and baseline preparation.
- [ ] `crates/aise/src/context/retrieval_pipeline.rs` directly awaits retrieval execution.
- [ ] `crates/aise/src/planning/` directly awaits planner operations.
- [ ] `crates/aise/src/validation/validation_pipeline.rs` directly awaits validation execution.
- [ ] `crates/aise/src/runtime/turn_runtime.rs` directly awaits runtime and Pipeline execution.
- [ ] `crates/aise/src/engine/service.rs` directly awaits coordination, story loading, and idempotency lookup.
- [ ] `crates/aise-server/src/turn_submission/service.rs` directly awaits session resolution and task admission.
- [ ] LLM completion, streaming, and embedding paths use direct `.await` and retain limiter coverage.
- [ ] Explicit parent IDs remain correct — verified by `observation_begin_uses_explicit_parent_context`.
- [ ] Direct-awaited operations retain duration and terminal status — verified by `direct_await_observation_records_lifecycle`.
- [ ] The canonical Turn tree remains unchanged — verified by `pipeline_tree_uses_explicit_parent_parameters`.
- [ ] Parallel children retain their explicit common parent — verified by `parallel_children_use_explicit_common_parent`.
- [ ] Finish and incomplete drop remain exactly-once — verified by `observation_finish_is_exactly_once` and `unfinished_observation_is_incomplete`.
- [ ] Core lifecycle Future APIs are gone — `rg 'pub async fn (trace|in_scope)<|std::future::Future|Future::instrument' crates/aise/src/observability` returns zero matches.
- [ ] Business wrapper Future APIs are gone — `rg 'pub async fn (trace|in_scope)<' crates/aise/src/*/observability crates/aise-server/src/*/observability` returns zero matches.
- [ ] Observation Future call sites are gone — `rg '\.(trace|in_scope)\(' crates/aise/src crates/aise-server/src -g '*.rs'` returns no Observation lifecycle call sites.
- [ ] Hidden parent lookup remains absent — `rg 'Span::current\(\)' crates/aise/src crates/aise-server/src` returns zero matches in Observation lifecycle code.
- [ ] The source refactor and prior execution spec contain no active Future-instrumentation requirement.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo test --workspace` passes.

---

## 6. Out of Scope / Future Work

- Correlating ordinary diagnostic logs with Observation span IDs without entering spans requires a separate logging-context design.
- Historical Langfuse traces remain unchanged.

---

## 7. References

- Source design: [Langfuse Trace 系统重构](../design/2026-09-23-langfuse-trace-system-design-gpt.md)
- Source refactor: [Langfuse Observability 数据模型对齐](../refactor/2026-09-25-langfuse-observability-model-refactor-gpt.md)
- Superseded execution spec: [Langfuse Observability Model Alignment](./2026-09-25-langfuse-observability-model-spec-gpt.md)
- Architecture: [AISE Architecture](../design/2026-08-04-Architecture-gpt.md)
- Langfuse data model: <https://langfuse.com/docs/observability/data-model>
