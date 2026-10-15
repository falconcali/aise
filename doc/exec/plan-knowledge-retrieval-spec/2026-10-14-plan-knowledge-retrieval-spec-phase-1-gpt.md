# Plan Knowledge Candidate Selection — Spec

> **Model**: GPT-5.6 Sol
> **Date**: 2026-10-14
> **Status**: Proposed
> **Source Design**: [Plan 阶段知识召回选择](../../design/2026-10-14-plan-knowledge-retrieval-design-gpt.md)
> **Phase**: Phase 1

---

## 1. Goal

Make `PlanPipeline` offer bounded Fact/Rumor ID-and-hint candidates to one LLM call and return a strictly validated ID-only `PlanOutput`.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Load the current Story Pack once inside `PlanPipeline`.
- Collect all non-activated Seed Fact/Rumor candidates.
- Sort candidates by the existing `BTreeMap` ID order and truncate each kind independently.
- Emit one structured warning when either candidate list is truncated.
- Record candidate and selection counts in the Plan observation.
- Add `retrievable_facts` and `retrievable_rumors` to the Plan RC prompt.
- Replace plain-text Plan output with one strict JSON object.
- Parse, bound, and validate `story_goal`, Fact IDs, and Rumor IDs.
- Add focused unit tests for candidate collection, prompt projection, output validation, warning metadata, and `PlanOutput`.

### 2.2 Non-Goals

- Does not execute Retrieval or load selected knowledge bodies.
- Does not modify the Baseline activation algorithm.
- Does not rank candidates by relevance, rotate candidates between Turns, or randomize truncation.
- Does not retry or repair malformed LLM output.
- Does not add provider-level JSON Schema support to `LlmGateway`.
- Does not add reasons, free-text queries, or hints to `PlanOutput`.

### 2.3 Implementation Constraints

- Phase 0 contracts are required.
- Phases 0–3 MUST land as one hard-refactor change.
- The old plain-text Plan output path MUST be removed in the same change.
- `plan_llm.rs` remains a thin `LlmGateway::complete` wrapper.
- Candidate collection and output validation are pure functions.
- Prompt text lives only in Prompt assets.
- No code comments, fallback parser, Markdown-fence stripping, or “best effort” ID filtering.

---

## 3. Contracts

### 3.1 Candidate model

Create `crates/aise-core/src/pipeline/plan/plan_candidates.rs`:

```rust
use crate::core::{ActivatedKnowledge, FactId, RetrievalHint, RumorId, StoryPack};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct CandidateStats {
    pub total: usize,
    pub offered: usize,
}

impl CandidateStats {
    pub fn dropped(self) -> usize {
        self.total.saturating_sub(self.offered)
    }

    pub fn truncated(self) -> bool {
        self.dropped() > 0
    }
}

pub(super) struct RetrievalCandidate<'a, I> {
    pub id: &'a I,
    pub retrieval_hint: &'a RetrievalHint,
}

pub(super) struct PlanCandidates<'a> {
    pub facts: Vec<RetrievalCandidate<'a, FactId>>,
    pub rumors: Vec<RetrievalCandidate<'a, RumorId>>,
    pub fact_stats: CandidateStats,
    pub rumor_stats: CandidateStats,
}

pub(super) fn collect_candidates<'a>(
    story_pack: &'a StoryPack,
    activated: &ActivatedKnowledge,
    limit: usize,
) -> PlanCandidates<'a>;
```

`PlanCandidates` MUST also expose membership checks:

```rust
impl PlanCandidates<'_> {
    pub fn contains_fact(&self, id: &FactId) -> bool;
    pub fn contains_rumor(&self, id: &RumorId) -> bool;
}
```

### 3.2 LLM output DTO and validation

Create `crates/aise-core/src/pipeline/plan/plan_output.rs`:

```rust
use crate::core::{FactId, RumorId};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanLlmOutputDto {
    story_goal: String,
    facts_to_retrieve: Vec<FactId>,
    rumors_to_retrieve: Vec<RumorId>,
}

pub(super) fn parse_plan_output(
    raw: &str,
    candidates: &PlanCandidates<'_>,
    config: &PlanConfig,
) -> Result<PlanOutput, PipelineError>;
```

The exact output protocol is:

```json
{
  "story_goal": "one concise sentence within max_story_goal_bytes",
  "facts_to_retrieve": ["fact-id"],
  "rumors_to_retrieve": ["rumor-id"]
}
```

Use these stable `PipelineError::InvalidOutput.code` values:

```text
plan_output_invalid_json
plan_story_goal_empty
plan_story_goal_too_long
plan_duplicate_fact_id
plan_duplicate_rumor_id
plan_unknown_fact_id
plan_unknown_rumor_id
```

### 3.3 Plan orchestration

`PlanPipeline::process_story_plan` MUST return the final stage output:

```rust
async fn process_story_plan(
    &self,
    story_ctx: &StoryContext,
    input: &PlanInput<'_>,
    control: &TurnControl,
    observation: &Observation,
    metrics: &mut PlanTraceMetrics,
) -> Result<PlanOutput, PipelineError>;
```

Its primary flow is:

```text
load StoryPack
collect candidates
emit truncation warning if required
render Plan prompt
call LlmGateway once
parse and validate output
return PlanOutput and trace metrics
```

`execute` MUST use the standard lifecycle:

```text
plan_trace::begin_plan
process_story_plan
plan_trace::finish_plan
return PlanOutput
```

`PlanPipeline` MUST use the same loaded Story Pack for candidate collection and story opening projection. It MUST NOT perform a second `get_pack` call through `pipeline_prompt::load_story_opening`.

### 3.4 Prompt projection

Change `plan_prompt::process_plan` to accept the loaded Story Pack and candidates:

```rust
pub fn process_plan(
    prompt: &Prompt,
    story_ctx: &StoryContext,
    input: &PlanInput<'_>,
    story_pack: &StoryPack,
    candidates: &PlanCandidates<'_>,
    observation: &Observation,
) -> Result<RenderedPrompt, PipelineError>;
```

Add Prompt variables:

```text
retrievable_facts
retrievable_rumors
max_story_goal_bytes
```

Render each candidate as:

```text
- id: "<JSON-escaped ID>"
  retrieval_hint: "<JSON-escaped hint>"
```

The RC section order MUST be:

```text
Story Summary
Story Opening
Recent Story
Known Facts
Rumors
Retrievable Facts
Retrievable Rumors
Narrative Direction
Story Constraints
Pending Player Input
```

An empty candidate kind omits its entire section.

The FTI prompt MUST require the exact JSON protocol from §3.2 and MUST state:

- Select only IDs present in the corresponding candidate section.
- Set `story_goal` to one concise sentence containing only the immediate narrative transition, with no explanation, rationale, scene outline, or prose, and keep it within `max_story_goal_bytes` UTF-8 bytes.
- Return an empty array when no additional knowledge is needed.
- Do not return retrieval hints, reasons, prose, Markdown, or extra fields.
- Already known Facts/Rumors must not be requested again.

### 3.5 Trace metrics and warning

Extend `plan_trace.rs` with:

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlanTraceMetrics {
    pub fact_candidates_total: usize,
    pub fact_candidates_offered: usize,
    pub rumor_candidates_total: usize,
    pub rumor_candidates_offered: usize,
    pub facts_selected: usize,
    pub rumors_selected: usize,
}

pub fn warn_if_candidates_truncated(metrics: &PlanTraceMetrics);

pub fn finish_plan(
    observation: Observation,
    result: &Result<PlanOutput, PipelineError>,
    metrics: &PlanTraceMetrics,
);
```

Use these observation metadata keys:

```text
aise.plan.retrieval.fact_candidates_total
aise.plan.retrieval.fact_candidates_offered
aise.plan.retrieval.fact_candidates_dropped
aise.plan.retrieval.fact_candidates_truncated
aise.plan.retrieval.rumor_candidates_total
aise.plan.retrieval.rumor_candidates_offered
aise.plan.retrieval.rumor_candidates_dropped
aise.plan.retrieval.rumor_candidates_truncated
aise.plan.retrieval.facts_selected
aise.plan.retrieval.rumors_selected
```

If either kind is truncated, emit exactly one event:

```rust
tracing::warn!(
    target: "aise::pipeline::plan",
    fact_candidates_total = metrics.fact_candidates_total,
    fact_candidates_offered = metrics.fact_candidates_offered,
    fact_candidates_dropped = metrics.fact_candidates_total.saturating_sub(metrics.fact_candidates_offered),
    rumor_candidates_total = metrics.rumor_candidates_total,
    rumor_candidates_offered = metrics.rumor_candidates_offered,
    rumor_candidates_dropped = metrics.rumor_candidates_total.saturating_sub(metrics.rumor_candidates_offered),
    "plan retrieval candidates truncated"
);
```

No candidate ID or story ID is included in this warning.

### 3.6 File layout

```text
crates/aise-core/src/pipeline/plan/
  mod.rs
  config.rs
  plan.rs
  plan_candidates.rs
  plan_llm.rs
  plan_output.rs
  plan_prompt.rs
  plan_trace.rs
  tests/
    plan_candidates_tests.rs
    plan_output_tests.rs
    plan_prompt_tests.rs
    plan_tests.rs
    plan_trace_tests.rs

crates/aise-core/assets/prompts/
  csi/plan-process-story-plan.md.j2
  rc/plan-process-story-plan.md.j2
  fti/plan-process-story-plan.md.j2
```

---

## 4. Behavior Rules

1. **R-P1-1**: Candidate authority is `StoryContext.pack_ref -> Store.pack_store().get_pack`; `PlanInput.knowledge` is never treated as the complete candidate source.
2. **R-P1-2**: Activated Fact IDs are excluded only from Fact candidates; activated Rumor IDs are excluded only from Rumor candidates.
3. **R-P1-3**: Eligible candidates retain the stable ID order of `StoryPack.seed_facts` and `seed_rumors`.
4. **R-P1-4**: Each kind independently keeps the first `max_retrieval_candidates_per_kind` eligible entries.
5. **R-P1-5**: Truncation is non-fatal. Plan continues with the offered prefix and its observation status remains `Ok` if all later work succeeds.
6. **R-P1-6**: One warning is emitted per Plan execution when either kind is truncated; no warning is emitted when neither is truncated.
7. **R-P1-7**: A limit of `0` offers no candidates. Existing eligible entries produce a warning and truncated trace metadata.
8. **R-P1-8**: Candidate prompt data contains ID and Retrieval Hint only; it MUST NOT contain body content, activation rules, or entry origin.
9. **R-P1-9**: Known Facts/Rumors remain in the Plan prompt as full activated content and are not repeated in candidate sections.
10. **R-P1-10**: Plan calls `LlmGateway::complete` exactly once.
11. **R-P1-11**: The raw LLM response must deserialize as exactly one JSON object with no unknown fields or trailing non-whitespace content.
12. **R-P1-12**: `story_goal.trim()` must be non-empty and its original UTF-8 byte length must not exceed `max_story_goal_bytes`.
13. **R-P1-13**: Every selected ID must belong to the offered candidate list of the same kind.
14. **R-P1-14**: Duplicate selected IDs are invalid; do not deduplicate them.
15. **R-P1-15**: Preserve LLM selection order in `PlanOutput`.
16. **R-P1-16**: Do not retry, repair, strip Markdown fences, ignore unknown IDs, or fall back to the old plain-text Plan output.

### 4.1 Error Handling

- Story Pack load failures map to `PipelineError::Persistence { stage: Plan, .. }`.
- Prompt rendering failures map to `PipelineError::Prompt { stage: Plan, .. }`.
- Gateway failures map to `PipelineError::Llm { stage: Plan, .. }`.
- Output violations use exactly one code from §3.2; `plan_story_goal_too_long` reports the actual and maximum byte counts.
- `plan_trace::finish_plan` maps output violations to `code = <PipelineError code>`, `failure_kind = "invalid_output"`, and `stage = "plan"`.
- Error log messages MUST NOT interpolate IDs.

### 4.2 Concurrency

- All candidate data is Turn-scoped and borrowed from the loaded `Arc<StoryPack>`.
- No cache, queue, lock, task, or background work is introduced.
- The LLM call goes only through the injected `LlmGateway`.

### 4.3 Observability

- Candidate counts are attached to the explicitly passed Plan observation via `ObservationOutcome.metadata`.
- The warning uses structured numeric fields and the fixed message from §3.5.
- Selected counts are recorded after successful output validation.
- On failures before LLM output validation, selected counts are zero.
- Prompt and LLM child observations remain parented under the Plan span.

---

## 5. Acceptance Criteria

- [ ] `plan_candidates.rs` implements the contracts in §3.1.
- [ ] Candidate tests cover no activation, Fact exclusion, Rumor exclusion, empty Pack, stable ordering, per-kind truncation, and limit zero.
- [ ] Candidate truncation does not return `PipelineError`.
- [ ] `plan_output.rs` uses `#[serde(deny_unknown_fields)]` and all seven stable error codes.
- [ ] Output tests cover valid JSON, prose outside JSON, missing fields, unknown fields, empty goal, oversized goal, invalid/unknown/cross-kind IDs, duplicates, empty selections, and order preservation.
- [ ] RC prompt snapshots contain ID + Hint candidates and no unactivated body content.
- [ ] FTI prompt requires the exact JSON protocol.
- [ ] `PlanPipeline` performs one Pack load and one LLM call.
- [ ] `PlanOutput` contains `Vec<FactId>` and `Vec<RumorId>` only.
- [ ] Trace metadata tests compare all ten keys and values from §3.5.
- [ ] Warning logic is guarded by `fact_truncated || rumor_truncated`.
- [ ] `rg 'Return only the story goal as plain text' crates/aise-core/assets/prompts` returns zero matches.
- [ ] `cargo test -p aise-core pipeline::plan::` passes after all phases are applied.

---

## 6. Out of Scope / Future Work

- Selected ID resolution and Generate integration are specified in [Phase 2](./2026-10-14-plan-knowledge-retrieval-spec-phase-2-gpt.md).
- End-to-end verification and hard-refactor cleanup are specified in [Phase 3](./2026-10-14-plan-knowledge-retrieval-spec-phase-3-gpt.md).
- Relevance ranking or rotating truncation requires a separate design after observing real truncation frequency.

---

## 7. References

- Source design: [Plan 阶段知识召回选择](../../design/2026-10-14-plan-knowledge-retrieval-design-gpt.md)
- Phase 0: [Contracts and Config](./2026-10-14-plan-knowledge-retrieval-spec-phase-0-gpt.md)
- Prompt conventions: `doc/agents/guardrails/pipeline.md`
- Observability rules: `doc/agents/guardrails/observability.md`
