# Knowledge Retrieval and Generation Handoff — Spec

> **Model**: GPT-5.6 Sol
> **Date**: 2026-10-14
> **Status**: Proposed
> **Source Design**: [Plan 阶段知识召回选择](../../design/2026-10-14-plan-knowledge-retrieval-design-gpt.md)
> **Phase**: Phase 2

---

## 1. Goal

Resolve the Fact/Rumor IDs selected by Plan from the same Story Pack and supply those bodies to Story Generation through Runtime-owned typed data flow.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Give `RetrievalPipeline` an injected `Store`.
- Resolve selected Fact/Rumor IDs from `StoryContext.pack_ref`.
- Preserve selected ID order in `RetrievalOutput`.
- Return diagnosable consistency errors for missing selected IDs.
- Add a Retrieval child observation with requested/resolved counts.
- Make Runtime execute Retrieval only when Plan selected at least one ID.
- Extend `GenerateInput` with retrieved Fact/Rumor entries.
- Render activated and retrieved knowledge together in Generate prompt data.
- Add Retrieval, Runtime handoff, and Generate prompt tests.

### 2.2 Non-Goals

- Does not re-rank, filter, truncate, or reinterpret Plan-selected IDs.
- Does not call an LLM in Retrieval.
- Does not persist retrieved entries or cache them across Turns.
- Does not modify Story Pack or StoryContext.
- Does not add Character Think, Memory, audience partitioning, BM25, embedding, or free-text queries.
- Does not expose retrieval hints to Generate.

### 2.3 Implementation Constraints

- Phases 0 and 1 are required.
- Phases 0–3 MUST land as one hard-refactor change.
- Runtime remains the only module that imports both Plan and Retrieval stage contracts.
- Retrieval accesses storage only through injected `Store`.
- Retrieval does not import Plan or Generate.
- Generate does not import Retrieval.
- No code comments, fallback lookup, skipped IDs, or partial success.

---

## 3. Contracts

### 3.1 Retrieval pipeline

`crates/aise-core/src/pipeline/retrieval/retrieval.rs` MUST have:

```rust
pub struct RetrievalPipeline {
    store: Arc<Store>,
}

impl RetrievalPipeline {
    pub fn new(store: Arc<Store>) -> Self;

    async fn load_story_pack(
        &self,
        story_ctx: &StoryContext,
    ) -> Result<Arc<StoryPack>, PipelineError>;

    fn resolve(
        &self,
        story_pack: &StoryPack,
        input: &RetrievalInput<'_>,
    ) -> Result<RetrievalOutput, PipelineError>;
}
```

The final stage contract is:

```rust
#[derive(Serialize)]
pub struct RetrievalInput<'a> {
    pub fact_ids: &'a [FactId],
    pub rumor_ids: &'a [RumorId],
}

#[derive(Default, Serialize)]
pub struct RetrievalOutput {
    pub facts: Vec<FactEntry>,
    pub rumors: Vec<RumorEntry>,
}
```

Each resolved entry MUST be constructed with its requested ID, `EntryOrigin::Seed`, and a clone of the corresponding Story Pack body.

### 3.2 Retrieval trace

Create `crates/aise-core/src/pipeline/retrieval/retrieval_trace.rs`:

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetrievalTraceMetrics {
    pub facts_requested: usize,
    pub facts_resolved: usize,
    pub rumors_requested: usize,
    pub rumors_resolved: usize,
}

pub fn begin_retrieval(
    observation: &Observation,
    input: &RetrievalInput<'_>,
) -> Observation;

pub fn finish_retrieval(
    observation: Observation,
    result: &Result<RetrievalOutput, PipelineError>,
    metrics: &RetrievalTraceMetrics,
);
```

The child observation MUST use:

```text
name = "retrieve_knowledge"
kind = ObservationKind::Retriever
```

Metadata keys:

```text
aise.retrieval.facts_requested
aise.retrieval.facts_resolved
aise.retrieval.rumors_requested
aise.retrieval.rumors_resolved
```

### 3.3 Retrieval consistency errors

Use:

```text
code = "retrieval_fact_missing"
code = "retrieval_rumor_missing"
```

Both map to:

```rust
PipelineError::InconsistentData {
    stage: PipelineStage::Retrieval,
    code,
    message,
}
```

The message identifies the missing domain without interpolating the ID into a log message. The ID may remain in the returned typed error message for caller diagnostics.

### 3.4 Runtime handoff

`Runtime::new` MUST construct:

```rust
retrieval: RetrievalPipeline::new(Arc::clone(&store))
```

`Runtime::run_turn` MUST construct:

```rust
RetrievalInput {
    fact_ids: &plan_output.facts_to_retrieve,
    rumor_ids: &plan_output.rumors_to_retrieve,
}
```

The Runtime data flow is:

```text
PlanOutput
  -> requires_retrieval()
  -> optional PipelineRunner::run(RetrievalPipeline)
  -> borrowed RetrievalOutput entries
  -> GenerateInput
```

When Retrieval is skipped, Generate receives empty retrieved slices. Runtime MUST NOT construct fake entries or reuse Activated Knowledge as Retrieval output.

### 3.5 Generate input

Extend `GenerateInput`:

```rust
#[derive(Serialize)]
pub struct GenerateInput<'a> {
    pub story_goal: &'a str,
    pub player_contribution: &'a PlayerContribution,
    pub directions: &'a [NarrativeDirection],
    pub constraints: &'a [BoundedText],
    pub knowledge: &'a ActivatedKnowledge,
    pub retrieved_facts: &'a [FactEntry],
    pub retrieved_rumors: &'a [RumorEntry],
}
```

Add common prompt helpers:

```rust
pub(in crate::pipeline) fn known_facts_with_retrieved(
    activated: &[FactEntry],
    retrieved: &[FactEntry],
) -> String;

pub(in crate::pipeline) fn rumors_with_retrieved(
    activated: &[RumorEntry],
    retrieved: &[RumorEntry],
) -> String;
```

Each helper renders activated entries first and retrieved entries second, preserving each slice order and using the existing bullet format.

`generate_prompt.rs` MUST use these helpers for `known_facts` and `rumors`.

### 3.6 File layout

```text
crates/aise-core/src/pipeline/
  common/
    pipeline_prompt.rs
    tests/
      story_context_prompt_tests.rs
  retrieval/
    mod.rs
    retrieval.rs
    retrieval_trace.rs
    tests/
      retrieval_tests.rs
      retrieval_trace_tests.rs
  generate/
    generate.rs
    generate_prompt.rs
    tests/
      generate_prompt_tests.rs
  runtime/
    runtime.rs
    tests/
      runtime_tests.rs
```

---

## 4. Behavior Rules

1. **R-P2-1**: Retrieval loads exactly the Story Pack addressed by `story_ctx.pack_ref`.
2. **R-P2-2**: Retrieval resolves every Fact ID against `story_pack.seed_facts` and every Rumor ID against `story_pack.seed_rumors`.
3. **R-P2-3**: Retrieval preserves the request order independently for Facts and Rumors.
4. **R-P2-4**: Every resolved entry has `EntryOrigin::Seed`.
5. **R-P2-5**: A missing ID fails the entire Retrieval stage; no partial `RetrievalOutput` is returned.
6. **R-P2-6**: Retrieval performs no LLM call, write, ranking, filtering, truncation, or deduplication.
7. **R-P2-7**: Runtime skips Retrieval exactly when both Plan ID lists are empty.
8. **R-P2-8**: Runtime executes Retrieval exactly once when either Plan ID list is non-empty.
9. **R-P2-9**: Retrieved entries remain Turn-scoped in `Runtime::run_turn`.
10. **R-P2-10**: Generate Known Facts are activated Facts followed by retrieved Facts.
11. **R-P2-11**: Generate Rumors are activated Rumors followed by retrieved Rumors.
12. **R-P2-12**: Retrieved Fact and Rumor content remains story data in RC; it never enters CSI or FTI.
13. **R-P2-13**: Generate prompt output for an empty Retrieval result is byte-for-byte equivalent to the pre-retrieval activated-only rendering.

### 4.1 Error Handling

- Pack load failures map to `PipelineError::Persistence { stage: Retrieval, .. }`.
- Missing Fact IDs use `retrieval_fact_missing`.
- Missing Rumor IDs use `retrieval_rumor_missing`.
- Retrieval trace failures use `failure_kind = "inconsistent_data"` for missing IDs and preserve existing mappings for persistence failures.
- Never ignore a missing ID and never continue with a partial list.

### 4.2 Concurrency

- Retrieval performs one asynchronous Pack read before synchronous ID resolution.
- No lock is held across `.await`.
- No event, I/O, or channel send occurs while holding a lock.
- No background tasks, caches, queues, or concurrent fan-out are introduced.

### 4.3 Observability

- `PipelineRunner` retains the Retrieval Chain observation.
- `RetrievalPipeline::execute` creates one `Retriever` child observation through `retrieval_trace`.
- Requested counts are set before resolution.
- Resolved counts reflect only a successful complete result; they remain zero on failure.
- No retrieved body or ID is written to warning/error log messages.

---

## 5. Acceptance Criteria

- [ ] `RetrievalPipeline` owns `Arc<Store>` and is constructed by Runtime.
- [ ] Retrieval tests resolve Fact-only, Rumor-only, mixed, and empty inputs.
- [ ] Retrieval tests verify independent request-order preservation.
- [ ] Retrieval tests verify `EntryOrigin::Seed`.
- [ ] Missing Fact and Rumor tests assert the exact error codes.
- [ ] A mixed request with one missing ID returns no output.
- [ ] `rg 'LlmGateway' crates/aise-core/src/pipeline/retrieval` returns zero matches.
- [ ] Runtime tests cover skip, Fact-only, Rumor-only, and mixed routing.
- [ ] `GenerateInput` matches §3.5.
- [ ] Prompt tests verify activated-first/retrieved-second ordering.
- [ ] Prompt tests verify retrieved Fact content appears only in Known Facts.
- [ ] Prompt tests verify retrieved Rumor content appears only in Rumors.
- [ ] Retrieval trace tests verify all four metadata keys.
- [ ] `cargo test -p aise-core pipeline::retrieval::` passes after all phases are applied.
- [ ] `cargo test -p aise-core pipeline::generate::` passes after all phases are applied.

---

## 6. Out of Scope / Future Work

- End-to-end validation and old-path removal checks are specified in [Phase 3](./2026-10-14-plan-knowledge-retrieval-spec-phase-3-gpt.md).
- Retrieval token budgets, audience isolation, dynamic knowledge, and semantic providers require separate designs.

---

## 7. References

- Source design: [Plan 阶段知识召回选择](../../design/2026-10-14-plan-knowledge-retrieval-design-gpt.md)
- Phase 0: [Contracts and Config](./2026-10-14-plan-knowledge-retrieval-spec-phase-0-gpt.md)
- Phase 1: [Plan Candidate Selection](./2026-10-14-plan-knowledge-retrieval-spec-phase-1-gpt.md)
- Pipeline framework: `doc/design/2026-10-13-pipeline-framework-design-opus.md`
- Concurrency guardrails: `doc/agents/guardrails/concurrency.md`
