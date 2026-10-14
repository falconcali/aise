# Plan Knowledge Retrieval Contracts and Config — Spec

> **Model**: GPT-5.6 Sol
> **Date**: 2026-10-14
> **Status**: Proposed
> **Source Design**: [Plan 阶段知识召回选择](../../design/2026-10-14-plan-knowledge-retrieval-design-gpt.md)
> **Phase**: Phase 0

---

## 1. Goal

Establish the final ID-only Plan/Retrieval contracts and Plan-owned limits before implementing candidate selection and retrieval execution.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Add `PlanConfig` with authoritative defaults for candidate count and story-goal size.
- Aggregate `PlanConfig` into `PipelineConfig` and inject it into `PlanPipeline`.
- Finalize `PlanOutput` as `BoundedText + Vec<FactId> + Vec<RumorId>`.
- Define the final ID-only `RetrievalInput` and entry-bearing `RetrievalOutput`.
- Add typed `PipelineError` variants required by Plan output validation and Retrieval consistency checks.
- Delete obsolete Hint-bearing cross-stage retrieval types and their re-exports.
- Make `RetrievalHint` cloneable because `FactBody` and `RumorBody` are cloneable.

### 2.2 Non-Goals

- Does not collect or truncate Plan candidates.
- Does not change Plan prompt templates or parse JSON LLM output.
- Does not load selected knowledge in `RetrievalPipeline`.
- Does not wire retrieved entries into `GeneratePipeline`.
- Does not add BM25, embedding, free-text queries, Character Memory, or audience-specific retrieval.

### 2.3 Implementation Constraints

- Phases 0–3 form one hard-refactor change and MUST be landed together; do not ship an intermediate phase.
- This spec generates final-form contracts. Do **not** add compatibility aliases, deprecated wrappers, fallback fields, or dual paths.
- Old types superseded by this phase MUST be deleted.
- `mod.rs` and `lib.rs` remain index-only.
- No code comments are permitted.
- Maintained crates MUST NOT depend on `crates/aise` or `crates/aise-server`.

---

## 3. Contracts

### 3.1 Plan configuration

Create `crates/aise-core/src/pipeline/plan/config.rs` with this contract:

```rust
use serde::Deserialize;

const DEFAULT_MAX_RETRIEVAL_CANDIDATES_PER_KIND: usize = 32;
const DEFAULT_MAX_STORY_GOAL_BYTES: usize = 512;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanConfig {
    pub max_retrieval_candidates_per_kind: usize,
    pub max_story_goal_bytes: usize,
}

impl Default for PlanConfig {
    fn default() -> Self {
        Self {
            max_retrieval_candidates_per_kind: DEFAULT_MAX_RETRIEVAL_CANDIDATES_PER_KIND,
            max_story_goal_bytes: DEFAULT_MAX_STORY_GOAL_BYTES,
        }
    }
}
```

`PipelineConfig` MUST have this final shape:

```rust
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PipelineConfig {
    pub baseline: BaselineConfig,
    pub plan: PlanConfig,
    pub summary: SummaryConfig,
}
```

`pipeline/plan/mod.rs` MUST re-export `PlanConfig` to `pipeline`, and `pipeline/mod.rs` MUST publicly re-export it:

```rust
pub use plan::PlanConfig;
```

### 3.2 Plan stage contract

`PlanOutput` MUST be:

```rust
#[derive(Serialize)]
pub struct PlanOutput {
    pub story_goal: BoundedText,
    pub facts_to_retrieve: Vec<FactId>,
    pub rumors_to_retrieve: Vec<RumorId>,
}

impl PlanOutput {
    pub fn requires_retrieval(&self) -> bool {
        !self.facts_to_retrieve.is_empty() || !self.rumors_to_retrieve.is_empty()
    }

    pub fn requires_character_thinking(&self) -> bool {
        false
    }
}
```

`PlanPipeline` MUST own its configuration:

```rust
pub struct PlanPipeline {
    gateway: Arc<LlmGateway>,
    prompt: Arc<Prompt>,
    store: Arc<Store>,
    config: PlanConfig,
}

impl PlanPipeline {
    pub fn new(
        gateway: Arc<LlmGateway>,
        prompt: Arc<Prompt>,
        store: Arc<Store>,
        config: PlanConfig,
    ) -> Self;
}
```

`Runtime::new` MUST move `pipeline_config.plan` into `PlanPipeline::new`.

### 3.3 Retrieval stage contract

Replace the string placeholder contract with:

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

The stage module owns `RetrievalInput` and `RetrievalOutput`. Only `pipeline::runtime` may import them.

### 3.4 Pipeline errors

Extend `PipelineError` with:

```rust
#[error("pipeline stage {stage} produced invalid output ({code}): {message}")]
InvalidOutput {
    stage: PipelineStage,
    code: &'static str,
    message: String,
},

#[error("pipeline stage {stage} observed inconsistent data ({code}): {message}")]
InconsistentData {
    stage: PipelineStage,
    code: &'static str,
    message: String,
},
```

Add constructors:

```rust
pub fn invalid_output(
    stage: PipelineStage,
    code: &'static str,
    message: impl Into<String>,
) -> Self;

pub fn inconsistent_data(
    stage: PipelineStage,
    code: &'static str,
    message: impl Into<String>,
) -> Self;
```

### 3.5 Deleted contracts

Delete `crates/aise-core/src/core/knowledge/retrieval.rs` and remove every re-export or use of:

```text
RetrievalFactEntry
RetrievalRumorEntry
RetrievalFacts
RetrievalRumors
```

No replacement core wrapper is introduced. Cross-stage data uses existing ID and entry types.

### 3.6 File layout

```text
crates/aise-core/src/
  core/
    knowledge/
      mod.rs
      hint.rs
  pipeline/
    mod.rs
    common/
      config.rs
      error.rs
    plan/
      mod.rs
      config.rs
      plan.rs
    retrieval/
      retrieval.rs
```

---

## 4. Behavior Rules

1. **R-P0-1**: `PlanOutput::requires_retrieval()` returns `true` exactly when either ID vector is non-empty.
2. **R-P0-2**: `PlanOutput::requires_character_thinking()` remains `false`; Character Think is outside this change.
3. **R-P0-3**: `PlanConfig::default()` returns `32` candidate entries per kind and `512` bytes for `story_goal`.
4. **R-P0-4**: A configured candidate limit of `0` is valid and means no Fact or Rumor candidate is offered.
5. **R-P0-5**: `RetrievalInput` contains IDs only; it MUST NOT contain retrieval hints, bodies, queries, or Plan stage types.
6. **R-P0-6**: `RetrievalOutput` contains resolved `FactEntry` and `RumorEntry` values in request order.
7. **R-P0-7**: The four deleted retrieval wrapper names MUST have zero references under `crates/aise-core/src`.
8. **R-P0-8**: No stage imports another stage; Runtime remains the only stage-contract bridge.

### 4.1 Error Handling

- Plan JSON/schema/ID-selection violations use `PipelineError::InvalidOutput` with `stage = PipelineStage::Plan`.
- A selected ID that cannot be resolved from the same PackRef uses `PipelineError::InconsistentData` with `stage = PipelineStage::Retrieval`.
- Error messages MUST be diagnostic but MUST NOT be used as machine-readable codes.
- No `unwrap`, `expect`, or panic is allowed for LLM output or Pack lookup.

### 4.2 Concurrency

- This phase adds no mutable shared state.
- `PlanConfig` is moved into the long-lived `PlanPipeline` at Runtime construction.
- No lock or guard may be introduced.

### 4.3 Observability

- This phase defines no new log or trace event.
- The new `PipelineError` variants MUST remain serializable through existing stage observation error mapping.

---

## 5. Acceptance Criteria

- [ ] `PlanConfig` exists at `crates/aise-core/src/pipeline/plan/config.rs` and defaults to `32` / `512`.
- [ ] `PipelineConfig` contains `pub plan: PlanConfig`.
- [ ] `PlanPipeline::new` receives and stores `PlanConfig`.
- [ ] `PlanOutput` matches §3.2 exactly.
- [ ] Unit tests cover both empty and non-empty `requires_retrieval` cases.
- [ ] `RetrievalInput` and `RetrievalOutput` match §3.3 exactly.
- [ ] `PipelineError` contains `InvalidOutput` and `InconsistentData`.
- [ ] `RetrievalHint` derives `Clone`.
- [ ] `crates/aise-core/src/core/knowledge/retrieval.rs` is deleted.
- [ ] `rg 'RetrievalFactEntry|RetrievalRumorEntry|RetrievalFacts|RetrievalRumors' crates/aise-core/src` returns zero matches.
- [ ] `rg 'crate::pipeline::(plan|retrieval)' crates/aise-core/src/pipeline/{plan,retrieval} -g '*.rs'` shows no cross-stage import.

---

## 6. Out of Scope / Future Work

- Candidate collection, truncation, prompt projection, and output validation are specified in [Phase 1](./2026-10-14-plan-knowledge-retrieval-spec-phase-1-gpt.md).
- Retrieval execution and Generate integration are specified in [Phase 2](./2026-10-14-plan-knowledge-retrieval-spec-phase-2-gpt.md).
- Repository-wide verification is specified in [Phase 3](./2026-10-14-plan-knowledge-retrieval-spec-phase-3-gpt.md).

---

## 7. References

- Source design: [Plan 阶段知识召回选择](../../design/2026-10-14-plan-knowledge-retrieval-design-gpt.md)
- Pipeline guardrails: `doc/agents/guardrails/pipeline.md`
- Layer guardrails: `doc/agents/guardrails/layer-dependencies.md`
- Code organization: `doc/agents/guardrails/code-organization.md`
