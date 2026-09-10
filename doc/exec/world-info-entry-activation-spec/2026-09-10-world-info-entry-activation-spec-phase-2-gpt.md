# World Info Entry Activation — Phase 2 Spec

> **Model**: GPT-5.6 Sol
> **Date**: 2026-09-10
> **Status**: Proposed
> **Source Design**: [World Info Entry Activation Refactor — Design](../../design/2026-09-05-world-info-entry-activation-design-gpt.md)
> **Phase**: Phase 2 — read-only Activation Preview tooling

---

## 1. Goal

Expose the Phase 1 activation engine as a side-effect-free author Preview API and bounded visual trace without changing activation semantics.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Add an application service that loads one committed Story/Knowledge/Activation snapshot and executes the Phase 1 engine in `ActivationRunMode::Preview`.
- Add a read-only HTTP endpoint for previewing activation from a proposed Player Contribution and optional exact external targets.
- Return activated Source IDs, knowledge kind, authorized deliveries, activation class, bounded match evidence, round, rank, token cost, consumed budget, and rejection summaries.
- Add a compact author-facing trace view to the existing server web client.
- Guarantee that Preview performs no Story generation, LLM call, commit, timed-state write, overlay mutation, cache-authority write, outbox write, or continuation reuse.
- Add request/response bounds, authorization checks, deterministic ordering, structured tracing, tests, and API error mapping.

### 2.2 Non-Goals

- Does not change matching, recursion, group, probability, timing, ranking, authorization, or budget behavior from Phase 1.
- Does not add editable World Book rules, Pack mutation, Story mutation, or a “commit preview” action.
- Does not return Fact/Rumor/Memory body text, Story fragments, Player Contribution echoes, regex text, literal key text, Prompt content, or raw cache records.
- Does not implement a vector provider, embedding call, semantic search, BM25, or provider configuration.
- Does not persist or reuse Preview `ActivationContinuation`.
- Does not guarantee that a later real Turn uses the same base revision when another Turn commits between Preview and execution.
- Does not add a new authentication model; deployment access control remains owned by the server boundary.
- Does not add a standalone frontend framework or dependency.

### 2.3 Implementation Constraints

- Phase 1 is a hard prerequisite. Phase 2 must call the same `KnowledgeActivationCoordinator` and `KnowledgeActivationEngine`; it must not copy or branch the algorithm.
- Preview differences are limited to request source, response projection, and suppression of side effects.
- Preview cannot be accepted as a `PendingActivationStateDelta`, `ActivationContinuation`, `ValidatedChangeSet`, or `TurnCommitSpec` input.
- The endpoint must apply trusted server limits before allocating request collections or loading snapshots.
- API and UI treat all Story, Entry, evidence, and request values as untrusted data.
- No lock or database transaction crosses `.await`; no trace/event/I/O occurs while holding a write guard.
- Code follows `AGENTS.md`: no ordinary code comments, no inline test modules, index-only `mod.rs`/`lib.rs`, compact imports, format and Clippy clean.
- Do not add a frontend package manager, build step, or runtime dependency; update the existing static HTML/JavaScript/CSS only.

### 2.4 Required Implementation Order

1. Add Preview request/result Domain projections and trusted limits.
2. Add `KnowledgeActivationPreviewService` using Phase 1 snapshots, Scan Buffer builder, and coordinator.
3. Add API DTOs, route, error mapping, and endpoint tests.
4. Add the bounded static-web trace view and browser rendering tests where the repository already supports them.
5. Add side-effect and Preview/Turn equivalence tests.
6. Run Phase 1 regression, server API, format, Clippy, and workspace tests.

---

## 3. Contracts

### 3.1 File and Route Layout

```text
crates/aise/src/context/
├── activation/
│   ├── preview.rs
│   └── tests/
│       └── preview_tests.rs
└── mod.rs

crates/aise-server/src/api/
├── activation_preview.rs
├── mod.rs
└── routes.rs

crates/aise-server/assets/
├── index.html
├── app.js
└── style.css

crates/aise-server/tests/
└── activation_preview_api_tests.rs
```

Route:

```text
POST /api/stories/{story_id}/knowledge-activation/preview
Content-Type: application/json
```

The route is read-only at the application level. It must not call `Store::commit_turn` or any asset/knowledge mutation method.

### 3.2 Core Preview Contract

```rust
#[derive(Debug, Clone)]
pub struct ActivationPreviewSpec {
    pub story_id: StoryId,
    pub player_contribution: BoundedText,
    pub generation_trigger: GenerationTrigger,
    pub external_targets: Vec<ActivationPreviewTarget>,
}

#[derive(Debug, Clone)]
pub struct ActivationPreviewTarget {
    pub source_id: KnowledgeSourceId,
    pub delivery: KnowledgeDelivery,
    pub mandatory: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ActivationPreviewLimits {
    pub max_player_contribution_bytes: usize,
    pub max_external_targets: usize,
    pub max_response_entries: usize,
    pub max_evidence_per_entry: usize,
    pub max_response_evidence: usize,
    pub max_response_bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivationPreviewResult {
    pub story_id: StoryId,
    pub base_revision: StoryRevision,
    pub evaluated_turn_number: TurnNumber,
    pub pack_digest: Sha256Digest,
    pub overlay_version: u64,
    pub generation_trigger: GenerationTrigger,
    pub activated: Vec<ActivationPreviewEntry>,
    pub rejection_counts: BTreeMap<ActivationRejectionReason, u32>,
    pub stop_reason: ActivationStopReason,
    pub usage: ActivationWorkUsage,
    pub truncated: bool,
}
```

```rust
pub struct KnowledgeActivationPreviewService {
    store: Arc<dyn Store>,
    activation_index: Arc<dyn ActivationIndexPort>,
    timed_state: Arc<dyn ActivationTimedStateReadPort>,
    coordinator: Arc<KnowledgeActivationCoordinator>,
    content_limits: TurnContentLimitsConfig,
    context_config: ContextPreparationConfig,
    activation_config: ActivationConfig,
    preview_limits: ActivationPreviewLimits,
}

impl KnowledgeActivationPreviewService {
    pub async fn preview(
        &self,
        spec: ActivationPreviewSpec,
    ) -> Result<ActivationPreviewResult, ActivationPreviewError>;
}
```

The service performs these steps exactly:

1. Validate request bounds and reject `GenerationTrigger::Repair` from the client.
2. Load one committed Story Snapshot and derive `evaluated_turn_number` as the next logical Turn number without reserving or writing it.
3. Load matching committed timed state and Activation Index Snapshot.
4. Run the same Narrative projection and Scan Buffer builder used by Phase 1.
5. Convert requested targets into `PreviewOverride` external seeds after the same kind/delivery authorization used by a real Turn.
6. Execute the coordinator with `ActivationRunMode::Preview` and evaluate Entry scope against the requested trigger.
7. Project bounded metadata into `ActivationPreviewResult`.
8. Drop continuation and pending timed-state delta.

### 3.3 Preview Entry and Evidence

```rust
#[derive(Debug, Clone, Serialize)]
pub struct ActivationPreviewEntry {
    pub source_id: KnowledgeSourceId,
    pub knowledge_kind: KnowledgeKind,
    pub deliveries: Vec<KnowledgeDelivery>,
    pub activation_class: ActivationSeedKind,
    pub round: u16,
    pub recursion_level: u16,
    pub rank: u32,
    pub token_cost: u64,
    pub budget_class: ActivationBudgetClass,
    pub evidence: Vec<ActivationPreviewEvidence>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivationPreviewEvidence {
    pub pattern_kind: ActivationPatternKind,
    pub pattern_ordinal: u16,
    pub fragment_kind: ScanFragmentKind,
    pub recency_depth: u16,
    pub match_count: u16,
    pub group_score_contribution: u16,
}
```

Evidence identifies rule position and fragment category but does not include matched text, offsets into secret text, literal patterns, regex source, Story content, or Entry content. Evidence is ordered by pattern kind, pattern ordinal, fragment kind, recency depth, then stable fragment order.

Activated entries use the exact engine rank order. Rejection counts are aggregate only; Phase 2 does not return a per-Entry rejected list because it can expose the existence of unauthorized knowledge.

### 3.4 HTTP Request and Response

Request:

```json
{
  "player_contribution": "I ask Bai Suzhen whether Fahai knows her identity.",
  "generation_trigger": "normal",
  "external_targets": [
    {
      "source_id": {
        "kind": "fact",
        "id": "fact_0001"
      },
      "delivery": {
        "kind": "writer"
      },
      "mandatory": false
    }
  ]
}
```

`generation_trigger` accepts `normal`, `continue`, `regenerate`, or `dry_run_preview`. It defaults to `normal`. `external_targets` defaults to `[]`.

Success response:

```json
{
  "story_id": "story-id",
  "base_revision": 7,
  "evaluated_turn_number": 8,
  "pack_digest": "sha256",
  "overlay_version": 3,
  "generation_trigger": "normal",
  "activated": [
    {
      "source_id": {
        "kind": "fact",
        "id": "fact_0001"
      },
      "knowledge_kind": "fact",
      "deliveries": [
        {
          "kind": "writer"
        }
      ],
      "activation_class": "text_match",
      "round": 0,
      "recursion_level": 0,
      "rank": 1,
      "token_cost": 24,
      "budget_class": "normal",
      "evidence": [
        {
          "pattern_kind": "primary_literal",
          "pattern_ordinal": 0,
          "fragment_kind": "player_contribution",
          "recency_depth": 0,
          "match_count": 1,
          "group_score_contribution": 1
        }
      ]
    }
  ],
  "rejection_counts": {
    "secondary_condition": 2,
    "group_loser": 1
  },
  "stop_reason": "complete",
  "usage": {
    "scan_fragments": 4,
    "scan_bytes": 1024,
    "pattern_matches": 3,
    "candidate_evaluations": 6,
    "recursion_steps": 0,
    "activated_entries": 1,
    "knowledge_tokens": 24
  },
  "truncated": false
}
```

The API serializes typed IDs using their existing canonical wire representations. Numeric revisions and Turn numbers serialize as positive integers.

### 3.5 HTTP Status and Error Contract

```rust
#[derive(Debug, thiserror::Error)]
pub enum ActivationPreviewError {
    #[error("activation preview request is invalid: {code}")]
    InvalidRequest { code: &'static str },
    #[error("activation preview story was not found")]
    StoryNotFound,
    #[error("activation preview target is unauthorized")]
    UnauthorizedTarget,
    #[error("activation preview snapshot changed")]
    SnapshotConflict,
    #[error("activation preview response limit exceeded")]
    ResponseLimitExceeded,
    #[error("activation preview failed")]
    Activation(ActivationError),
    #[error("activation preview store operation failed")]
    Store(StoreError),
}
```

Mapping:

| Condition | HTTP status | Stable code |
|---|---:|---|
| malformed JSON, unknown field, invalid ID, unsupported trigger, or request bound | 400 | `activation_preview_invalid_request` |
| Story missing | 404 | `story_not_found` |
| target kind/delivery unauthorized | 403 | `activation_preview_target_unauthorized` |
| revision/digest/index mismatch during one Preview | 409 | `activation_preview_snapshot_conflict` |
| response bound exceeded | 422 | `activation_preview_response_limit` |
| activation work/mandatory/invariant failure | 422 | Phase 1 activation code |
| Store unavailable | 503 | `store_unavailable` |

Error bodies use the server's existing error envelope and contain no request text, Entry text, pattern text, regex, evidence, or raw Store error.

### 3.6 Response Bounding

Projection applies limits in this order:

1. Keep activated entries in engine rank order up to `max_response_entries`.
2. Keep each Entry's first `max_evidence_per_entry` evidence records in stable evidence order.
3. Stop adding evidence when `max_response_evidence` total records is reached.
4. Serialize the typed response once.
5. If serialized bytes exceed `max_response_bytes`, remove evidence from the end while retaining all admitted Entry summary rows.
6. If the summary-only response still exceeds `max_response_bytes`, return `ResponseLimitExceeded`.

`truncated` is true when any successful Entry or evidence record was omitted. Truncation never changes engine execution or rejection counts.

### 3.7 Side-Effect Boundary

Preview may read:

- Story Snapshot and current committed Turn number
- Knowledge Snapshot metadata
- Activation Index Snapshot
- committed timed state
- bounded Fact/Rumor bodies required for recursion and token cost
- fragment-match cache

Preview must not:

- call an LLM or reserve LLM budget
- invoke StoryGenerator, StoryStateExtractor, Validation, StoryRepairer, or TurnCommitter
- persist pending timed state
- mutate Story overlay or overlay version
- write Story, knowledge, Narrative, idempotency, outbox, or ledger rows
- publish events or return a resumable continuation

Fragment cache population is permitted because it is rebuildable derived data. Cache write failure must degrade to an uncached Preview with a structured cache error count; it must not alter the activation result.

### 3.8 Visual Trace

The existing static web client adds one Activation Preview section containing:

- Story ID input or the currently bound Story ID
- Player Contribution text area with live byte count
- generation trigger select with exactly `normal`, `continue`, `regenerate`, and `dry_run_preview`
- optional repeatable exact-target rows for Source ID, delivery, and mandatory flag
- Run Preview action
- evaluated revision/Turn/Pack/overlay identity
- activated Entry list in rank order
- per-Entry evidence rows grouped by round and recursion level
- aggregate rejection counts
- work/token usage against configured maxima
- stop reason and truncation indicator

The renderer uses `textContent` or DOM node construction for every untrusted value and never assigns response/request values through `innerHTML`. It does not render hidden Entry body text or fetch any endpoint other than the explicit Preview request.

### 3.9 Observability

Wrap each request in:

```text
knowledge.activation.preview {
    story_id,
    base_revision,
    evaluated_turn_number,
    generation_trigger,
    external_target_count,
    activated_count,
    rejection_count,
    response_bytes,
    truncated,
    status,
    error_code,
    latency_ms
}
```

The nested Phase 1 activation spans remain enabled with `mode = "preview"`. Production traces contain no Player Contribution, Story fragment, key, regex, Entry content, evidence text, or request/response body.

---

## 4. Behavior Rules

1. **WIA2-01 — Shared Engine**: Preview calls the Phase 1 coordinator and engine; no Preview-specific matcher, selector, probability, recursion, or budget implementation exists.
2. **WIA2-02 — Committed Snapshot**: Preview evaluates exactly one committed Story/Knowledge/Index snapshot identity and reports it.
3. **WIA2-03 — Next Turn Seed**: Deterministic probability uses the reported next logical Turn number without reserving it.
4. **WIA2-04 — Scope Projection**: Entry generation scope is evaluated against the requested trigger; `ActivationRunMode::Preview` controls side effects independently.
5. **WIA2-05 — No Side Effects**: Preview cannot change any authoritative Story, knowledge, activation, Narrative, outbox, idempotency, or ledger state.
6. **WIA2-06 — No Continuation Reuse**: Preview continuation and pending delta are dropped before returning and cannot be submitted later.
7. **WIA2-07 — Authorization**: External targets pass the same kind/delivery authorization as a real Turn.
8. **WIA2-08 — No Secret Enumeration**: Unauthorized or rejected Entry IDs are not returned; rejection output is aggregate.
9. **WIA2-09 — Evidence Redaction**: Preview evidence identifies pattern ordinal and fragment category only, never matched or source text.
10. **WIA2-10 — Stable Order**: Equivalent requests against the same snapshot return byte-equivalent ordered Domain results before transport metadata.
11. **WIA2-11 — Bounded Response**: Request and response limits are applied exactly as §3.6; truncation is explicit.
12. **WIA2-12 — Cache Neutrality**: Cache hit, miss, insertion, eviction, or write failure cannot change Preview semantics.
13. **WIA2-13 — Race Visibility**: A concurrent commit yields either one internally consistent old/new Preview or `409`; it never combines revisions.
14. **WIA2-14 — UI Safety**: The browser renders all untrusted values as text and provides no mutation action.
15. **WIA2-15 — No LLM**: Preview makes zero completion, streaming, or embedding calls.

### 4.1 Error Handling

- Unknown JSON fields and unsupported client trigger values return `400`.
- Missing Story returns `404` without distinguishing hidden knowledge.
- Unauthorized external target returns `403` before body loading.
- Snapshot drift returns `409` and no partial activation list.
- Phase 1 typed activation failures retain their stable activation code.
- UI request failures render status and stable code only; raw HTML or response bodies are not injected.

### 4.2 Concurrency

- Snapshot, index, timed-state, and bounded body reads use short operations with no transaction crossing engine execution.
- Preview does not acquire the Story Turn write coordinator or reserve a Turn number.
- A revision recheck before response projection detects cross-read drift when the Store cannot provide one immutable read snapshot.
- The endpoint adds no background work, polling loop, detached task, queue, or per-Entry fan-out.

### 4.3 Observability

- One Preview request emits one `knowledge.activation.preview` span and bounded nested activation spans.
- Preview-specific status fields distinguish invalid request, unauthorized target, snapshot conflict, activation failure, response truncation, and success.
- No production log includes request or knowledge content.

---

## 5. Acceptance Criteria

### 5.1 Service and Equivalence

- [ ] The service uses `ActivationRunMode::Preview` and the Phase 1 coordinator; static search finds no duplicate engine implementation.
- [ ] For the same committed snapshot, Turn number, contribution, trigger, targets, and limits, Preview and a real pre-commit activation run return identical activated IDs, authorized deliveries, order, ranks, token costs, evidence metadata, and rejection counts.
- [ ] Repeated Preview calls against unchanged state return byte-equivalent typed results.
- [ ] Preview probability and weighted-group results equal the corresponding real Turn seed.
- [ ] Repair is rejected; normal, continue, regenerate, and dry-run-preview scope values are accepted.

### 5.2 Side Effects

- [ ] A full database snapshot before and after Preview is identical for Story, Turn, knowledge, overlay version, timed state, Narrative, outbox, idempotency, and LLM ledger tables.
- [ ] Mock call counts are zero for every mutation Store method and every LLM Gateway method.
- [ ] Returned results expose no continuation or pending timed-state delta.
- [ ] Fragment cache hit/miss/write-failure cases return identical results.

### 5.3 API

- [ ] `POST /api/stories/{story_id}/knowledge-activation/preview` accepts and returns the shapes in §3.4.
- [ ] Unknown fields, malformed IDs, oversized contribution, too many targets, and unsupported triggers return exact §3.5 statuses/codes.
- [ ] Missing Story, unauthorized target, snapshot conflict, activation failure, and Store outage map exactly to §3.5.
- [ ] Response ordering and bounding follow §3.6 and set `truncated` correctly.
- [ ] Response JSON contains no Entry body, Story fragment, matched text, pattern text, regex, Prompt, continuation, or pending delta field.
- [ ] Concurrent commit tests yield a consistent response identity or `409`, never mixed revisions.

### 5.4 Visual Trace

- [ ] Existing static client can submit a bounded Preview request and render identity, entries, evidence metadata, rejection counts, usage, stop reason, and truncation.
- [ ] Client trigger options are exactly normal/continue/regenerate/dry-run-preview.
- [ ] Adversarial Story ID, error code, and response values are rendered via `textContent`/DOM APIs and cannot create DOM elements or event handlers.
- [ ] UI contains no commit, apply, save, mutate, or “continue from preview” action.
- [ ] Network tests show one explicit Preview request and no implicit polling.

### 5.5 Observability and Toolchain

- [ ] Preview span contains every §3.9 metadata field and no content field.
- [ ] `cargo test -p aise --test world_info_activation_tests` passes Phase 1 regression cases.
- [ ] `cargo test -p aise-server --test activation_preview_api_tests` passes.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo test --workspace --all-features` passes.
- [ ] `cargo +1.85 fmt --all -- --check` passes.
- [ ] `cargo +1.85 clippy --workspace --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo +1.85 test --workspace --all-features` passes.
- [ ] `git diff --check` passes.

---

## 6. Out of Scope / Future Work

- Editable authoring and Pack mutation require a separate trust, validation, versioning, and concurrency design.
- A vector activation provider requires an independent Source Design and Spec before implementing `ActivationSeedProvider`.
- Role-owned Memory requires an independent Source Design and Spec.
- Authentication/authorization policy for exposing author tools outside the current deployment boundary belongs to the server security design.

---

## 7. References

- Source design: [World Info Entry Activation Refactor — Design](../../design/2026-09-05-world-info-entry-activation-design-gpt.md)
- Phase 1 prerequisite: [Hard Cutover and Activation Runtime](./2026-09-10-world-info-entry-activation-spec-phase-1-gpt.md)
- Existing server routes: `crates/aise-server/src/api/routes.rs`
- Existing static client: `crates/aise-server/assets/index.html`, `crates/aise-server/assets/app.js`, `crates/aise-server/assets/style.css`
- Guardrails: `AGENTS.md` and `doc/agents/guardrails/`
