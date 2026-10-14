# Plan Knowledge Retrieval Verification and Cleanup — Spec

> **Model**: GPT-5.6 Sol
> **Date**: 2026-10-14
> **Status**: Proposed
> **Source Design**: [Plan 阶段知识召回选择](../../design/2026-10-14-plan-knowledge-retrieval-design-gpt.md)
> **Phase**: Phase 3

---

## 1. Goal

Verify the complete Plan-to-Retrieval-to-Generate flow, prove truncation observability, and remove every superseded contract and plain-text path before the hard refactor lands.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Add end-to-end maintained-crate tests for Plan selection, optional Retrieval routing, ID resolution, and Generate prompt delivery.
- Add deterministic tests for candidate truncation warning inputs and trace metadata.
- Verify strict JSON failure behavior and no fallback to plain-text Plan output.
- Verify no activated entry appears in the retrieval candidate sections.
- Verify all old Hint-bearing retrieval contracts and string-query placeholders are removed.
- Run formatting, clippy with warnings denied, and maintained-crate tests.
- Update the source design status from `Draft` to `Accepted` only after every acceptance criterion passes.

### 2.2 Non-Goals

- Does not add new production behavior beyond Phases 0–2.
- Does not tune the candidate limit from runtime metrics.
- Does not implement relevance prefiltering, rotating windows, BM25, embedding, Memory, or Character audience retrieval.
- Does not modify frozen reference crates.
- Does not add integration compatibility for old plain-text Plan responses.

### 2.3 Implementation Constraints

- Phases 0–2 are required.
- All phases MUST be reviewed and landed as one hard-refactor change.
- Tests follow `tests/<source>_tests.rs`; no inline test modules.
- Tests MUST assert typed fields and stable codes, not full human-readable error strings.
- No `#[allow]`, ignored test, snapshot update without review, or waiver is permitted.

---

## 3. Contracts

### 3.1 End-to-end data contract

The tested flow MUST have this exact shape:

```text
PlanInput
  player_contribution
  directions
  constraints
  ActivatedKnowledge

PlanOutput
  BoundedText story_goal
  FactId[] facts_to_retrieve
  RumorId[] rumors_to_retrieve

RetrievalInput
  &[FactId]
  &[RumorId]

RetrievalOutput
  FactEntry[]
  RumorEntry[]

GenerateInput
  story_goal
  player_contribution
  directions
  constraints
  ActivatedKnowledge
  &[FactEntry] retrieved_facts
  &[RumorEntry] retrieved_rumors
```

No Hint-bearing value crosses from Plan output into Retrieval input.

### 3.2 Testable trace metadata projection

Expose this pure helper inside `plan_trace.rs` for its external test module:

```rust
fn candidate_trace_metadata(metrics: &PlanTraceMetrics) -> Vec<Attribute>;
```

For:

```rust
PlanTraceMetrics {
    fact_candidates_total: 5,
    fact_candidates_offered: 2,
    rumor_candidates_total: 1,
    rumor_candidates_offered: 1,
    facts_selected: 1,
    rumors_selected: 0,
}
```

the returned attributes MUST encode:

```json
{
  "aise.plan.retrieval.fact_candidates_total": 5,
  "aise.plan.retrieval.fact_candidates_offered": 2,
  "aise.plan.retrieval.fact_candidates_dropped": 3,
  "aise.plan.retrieval.fact_candidates_truncated": true,
  "aise.plan.retrieval.rumor_candidates_total": 1,
  "aise.plan.retrieval.rumor_candidates_offered": 1,
  "aise.plan.retrieval.rumor_candidates_dropped": 0,
  "aise.plan.retrieval.rumor_candidates_truncated": false,
  "aise.plan.retrieval.facts_selected": 1,
  "aise.plan.retrieval.rumors_selected": 0
}
```

`finish_plan` MUST use this helper unchanged.

### 3.3 Required test cases

Create or extend dedicated test files with these exact test names:

```text
pipeline/plan/tests/plan_candidates_tests.rs
  candidates_exclude_activated_facts_and_rumors
  candidates_preserve_stable_id_order
  candidates_truncate_each_kind_independently
  zero_limit_truncates_all_eligible_candidates

pipeline/plan/tests/plan_output_tests.rs
  valid_output_preserves_selected_id_order
  output_rejects_plain_text_story_goal
  output_rejects_json_with_surrounding_prose
  output_rejects_unknown_fields
  output_rejects_unknown_fact_id
  output_rejects_unknown_rumor_id
  output_rejects_duplicate_fact_id
  output_rejects_duplicate_rumor_id
  output_rejects_empty_story_goal
  output_rejects_oversized_story_goal

pipeline/plan/tests/plan_trace_tests.rs
  candidate_trace_metadata_reports_fact_truncation
  candidate_trace_metadata_reports_rumor_truncation
  candidate_trace_metadata_reports_no_truncation

pipeline/retrieval/tests/retrieval_tests.rs
  retrieval_resolves_selected_entries_in_request_order
  retrieval_rejects_missing_fact_id
  retrieval_rejects_missing_rumor_id

pipeline/generate/tests/generate_prompt_tests.rs
  retrieved_knowledge_follows_activated_knowledge

pipeline/runtime/tests/runtime_tests.rs
  runtime_skips_retrieval_for_empty_plan_selection
  runtime_runs_retrieval_for_fact_selection
  runtime_runs_retrieval_for_rumor_selection
  runtime_delivers_retrieved_knowledge_to_generate
```

### 3.4 Forbidden residuals

These patterns MUST have zero matches under `crates/aise-core`:

```text
RetrievalFactEntry
RetrievalRumorEntry
RetrievalFacts
RetrievalRumors
RetrievalInput { query:
Return only the story goal as plain text
result: input.query.to_owned()
```

These files MUST NOT exist:

```text
crates/aise-core/src/core/knowledge/retrieval.rs
```

### 3.5 Verification commands

Run from the repository root:

```powershell
cargo fmt -p aise-core -p aise-service -- --check
cargo clippy -p aise-core -p aise-service --all-targets -- -D warnings
cargo test -p aise-core -p aise-service
rg "RetrievalFactEntry|RetrievalRumorEntry|RetrievalFacts|RetrievalRumors" crates/aise-core
rg "RetrievalInput\s*\{\s*query|result:\s*input\.query\.to_owned" crates/aise-core
rg "Return only the story goal as plain text" crates/aise-core/assets/prompts
```

The three `rg` commands MUST exit with no matches.

---

## 4. Behavior Rules

1. **R-P3-1**: End-to-end tests use a Story Pack containing at least one activated Fact, one activated Rumor, one retrievable Fact, and one retrievable Rumor.
2. **R-P3-2**: The Plan prompt seen by the fake provider includes activated bodies in Known sections and excludes their IDs from Retrievable sections.
3. **R-P3-3**: The fake Plan response selects only the non-activated candidate IDs.
4. **R-P3-4**: Runtime executes Retrieval once and Generate receives both selected bodies.
5. **R-P3-5**: The Generate prompt preserves Fact/Rumor semantics: Facts appear only under Known Facts and Rumors only under Rumors.
6. **R-P3-6**: Empty Plan selections skip Retrieval and still execute Generate with activated knowledge.
7. **R-P3-7**: Candidate overflow continues the Turn with the stable prefix; the test MUST NOT expect an error.
8. **R-P3-8**: Candidate overflow produces dropped counts and `truncated = true` in Plan trace metadata.
9. **R-P3-9**: Strict JSON violations fail Plan before Retrieval and Generate execute.
10. **R-P3-10**: Missing selected IDs fail Retrieval before Generate executes.
11. **R-P3-11**: No test or implementation retains a plain-text compatibility branch.
12. **R-P3-12**: Frozen crates remain byte-for-byte unmodified by this change.

### 4.1 Error Handling

- Tests assert `PipelineError` variant, stage, and stable code.
- Invalid Plan output asserts `InvalidOutput { stage: Plan, .. }`.
- Missing retrieval entries assert `InconsistentData { stage: Retrieval, .. }`.
- No acceptance test may pass by matching only `error.to_string()`.

### 4.2 Concurrency

- End-to-end tests assert at most one Plan LLM call and no Retrieval LLM call.
- No test relies on timing, sleeps, background tasks, or unordered execution.
- Fake providers and stores remain bounded and are owned by the test fixture.

### 4.3 Observability

- The metadata helper is tested as a pure function.
- Truncation remains a successful Plan observation, not an error status.
- The warning message is exactly `plan retrieval candidates truncated`.
- Warning fields are numeric and contain no IDs or story content.
- Retrieval trace requested/resolved counts are covered by unit tests.

---

## 5. Acceptance Criteria

- [ ] Every test named in §3.3 exists in a dedicated external test module.
- [ ] End-to-end tests prove Plan → Retrieval → Generate delivery for both knowledge kinds.
- [ ] Overflow tests prove stable truncation, continued execution, and trace metadata.
- [ ] Plain-text and malformed JSON Plan outputs fail with stable codes.
- [ ] Missing Retrieval IDs stop before Generate.
- [ ] All forbidden patterns in §3.4 have zero matches.
- [ ] No file under `crates/aise` or `crates/aise-server` is modified.
- [ ] `cargo fmt -p aise-core -p aise-service -- --check` exits `0`.
- [ ] `cargo clippy -p aise-core -p aise-service --all-targets -- -D warnings` exits `0`.
- [ ] `cargo test -p aise-core -p aise-service` exits `0`.
- [ ] The source design status is changed to `Accepted` only after all checks above pass.

---

## 6. Out of Scope / Future Work

- Candidate relevance prefiltering or rotation requires a new source design backed by observed truncation frequency.
- Provider-level structured output belongs to an LLM Gateway design.
- Dynamic knowledge, Memory, and Character audience isolation require separate pipeline designs.

---

## 7. References

- Source design: [Plan 阶段知识召回选择](../../design/2026-10-14-plan-knowledge-retrieval-design-gpt.md)
- Phase 0: [Contracts and Config](./2026-10-14-plan-knowledge-retrieval-spec-phase-0-gpt.md)
- Phase 1: [Plan Candidate Selection](./2026-10-14-plan-knowledge-retrieval-spec-phase-1-gpt.md)
- Phase 2: [Retrieval and Generation Handoff](./2026-10-14-plan-knowledge-retrieval-spec-phase-2-gpt.md)
- Toolchain requirements: `doc/agents/guardrails/toolchain.md`
