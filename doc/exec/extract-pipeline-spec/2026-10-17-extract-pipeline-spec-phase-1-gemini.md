# Extract Pipeline and Dynamic Knowledge Store — Spec (Phase 1: Narrative Extract Loop)

> **Model**: Gemini 4 Argon
> **Date**: 2026-10-17
> **Status**: Proposed
> **Source Design**: [Extract Pipeline 与动态知识存储 — Design](../../design/2026-10-16-extract-pipeline-design-gemini.md)
> **Phase**: Phase 1

---

## 1. Goal

Complete the narrative extraction flow in `ExtractPipeline` by deterministically collecting watched `SemanticQuery` items from `StoryPack.narrative` and `StoryContext.narrative`, evaluating non-empty queries through `LlmGateway` using the `extract.evaluate_narrative` CSI/RC/FTI prompt, strictly validating the returned `Truth` map, and resolving `NarrativeGraph` transitions.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Add `ExtractConfig` in `crates/aise-core/src/pipeline/extract/config.rs` and include `pub extract: ExtractConfig` in `PipelineConfig`.
- Upgrade `ExtractPipeline::new` to accept `(Arc<LlmGateway>, Arc<Prompt>, Arc<Store>, ExtractConfig)` and wire it in `Runtime::new`.
- Add `extract_narrative::collect_semantic_queries` to deterministically collect watched `SemanticQuery` items from `StoryPack.narrative` and `StoryContext.narrative`.
- Register prompt `extract.evaluate_narrative` in `crates/aise-core/assets/prompts/index.toml` with CSI, RC, and FTI templates.
- Implement `extract_prompt::evaluate_narrative`, `extract_llm::evaluate_narrative`, `extract_llm::parse_narrative_evaluations`, and `extract_trace::begin_evaluate_narrative` / `finish_evaluate_narrative`.
- Wire `ExtractPipeline::execute` to evaluate narrative queries via `LlmGateway` when `queries` is non-empty and skip the LLM call when `story_pack.narrative` is `None` or `queries.is_empty()`.
- Add unit tests for `config`, `extract_narrative`, `extract_prompt`, `extract_llm`, `extract_trace`, and `extract`.

### 2.2 Non-Goals

- Does not extract `Fact`, `Rumor`, or `Memory` changes via LLM (handled in Phase 2; `ExtractOutput.knowledge_change` returns `KnowledgeChange::default()` in Phase 1).
- Does not modify `StoryStore` or `CommitPipeline`.
- Does not modify frozen reference crates (`crates/aise` and `crates/aise-server`).

### 2.3 Implementation Constraints (for code generation)

- Requires [Phase 0](./2026-10-17-extract-pipeline-spec-phase-0-gemini.md).
- Every LLM call MUST route through the injected `LlmGateway` (`R-CONC-04`).
- Prompt text MUST live exclusively in `crates/aise-core/assets/prompts/{csi,rc,fti}/`; Rust code holds only prompt IDs, variable names, and data assembly (`R-AISE-08`).
- Keep type definitions and trait derives minimal and strictly necessary for production code: do **not** add test-only derives (such as `PartialEq`, `Eq`), speculative helper methods, or extra fields to `core` or `pipeline` types; tests MUST use `matches!`, field-level assertions, or `serde_json::to_value` comparisons instead of polluting production definitions.
- Rust source files MUST remain comment-free (`R-CODE-05`) and unit tests MUST live in `tests/<source>_tests.rs` (`R-CODE-02`).

---

## 3. Contracts

### 3.1 `ExtractConfig` and `ExtractPipeline`

Create `crates/aise-core/src/pipeline/extract/config.rs`:

```rust
use crate::pipeline::common::PipelineError;
use serde::Deserialize;
use std::num::NonZeroUsize;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractConfig {
    pub max_fact_changes: usize,
    pub max_rumor_changes: usize,
    pub max_roles_for_memory: usize,
    pub max_memory_changes_per_role: usize,
    pub max_memory_concurrency: NonZeroUsize,
}

impl ExtractConfig {
    pub fn validate(&self) -> Result<(), PipelineError>;
}

impl Default for ExtractConfig {
    fn default() -> Self;
}
```

> **Note (`TBD`)**: Exact default values and upper bounds for `ExtractConfig` capacity and byte budgets are marked `TBD` in the source design and MUST be finalized in the dedicated budget pass before acceptance.

Update `crates/aise-core/src/pipeline/common/config.rs` (`PipelineConfig`):

```rust
pub struct PipelineConfig {
    pub baseline: BaselineConfig,
    pub plan: PlanConfig,
    pub think: ThinkConfig,
    pub generate: GenerateConfig,
    pub validate: ValidateConfig,
    pub repair: RepairConfig,
    pub validation_loop: ValidationLoopConfig,
    pub extract: ExtractConfig,
    pub summary: SummaryConfig,
}
```

Update `crates/aise-core/src/pipeline/extract/extract.rs`:

```rust
pub struct ExtractPipeline {
    gateway: Arc<LlmGateway>,
    prompt: Arc<Prompt>,
    store: Arc<Store>,
    config: ExtractConfig,
}

impl ExtractPipeline {
    pub fn new(
        gateway: Arc<LlmGateway>,
        prompt: Arc<Prompt>,
        store: Arc<Store>,
        config: ExtractConfig,
    ) -> Self;
}
```

### 3.2 Narrative Query Collection and Resolution (`extract_narrative.rs`)

Update `crates/aise-core/src/pipeline/extract/extract_narrative.rs`:

```rust
use crate::core::{
    Change, NarrativeConditionId, NarrativeError, NarrativeEvidence, NarrativeState, SemanticQuery, StoryContext,
    StoryLifeCycle, StoryPack, Truth,
};
use std::collections::BTreeMap;

pub(super) struct NarrativeChange {
    pub narrative: Change<NarrativeState>,
    pub life_cycle: Change<StoryLifeCycle>,
}

pub(super) fn collect_semantic_queries(
    story_pack: &StoryPack,
    story_ctx: &StoryContext,
) -> Result<Vec<SemanticQuery>, NarrativeError> {
    match &story_pack.narrative {
        Some(graph) => Ok(graph.plan(&story_ctx.narrative)?.queries),
        None => Ok(Vec::new()),
    }
}

pub(super) fn resolve_narrative(
    story_pack: &StoryPack,
    story_ctx: &StoryContext,
    semantic_results: &BTreeMap<NarrativeConditionId, Truth>,
) -> NarrativeChange;
```

### 3.3 Prompt Contract (`extract_prompt.rs`)

Register in `crates/aise-core/assets/prompts/index.toml`:

```toml
[[prompts]]
id = "extract.evaluate_narrative"
csi = "csi/extract-evaluate-narrative.md.j2"
rc = "rc/extract-evaluate-narrative.md.j2"
fti = "fti/extract-evaluate-narrative.md.j2"
```

Create `crates/aise-core/src/pipeline/extract/extract_prompt.rs`:

```rust
pub(super) struct EvaluateNarrativePrompt<'a> {
    pub story_pack: &'a StoryPack,
    pub story_ctx: &'a StoryContext,
    pub proposal: &'a str,
    pub queries: &'a [SemanticQuery],
}

pub(super) fn evaluate_narrative(
    prompt: &Prompt,
    spec: EvaluateNarrativePrompt<'_>,
    observation: &Observation,
) -> Result<RenderedPrompt, PipelineError>;
```

Rendered prompt variables:
- `story_summary`: rendered via `pipeline_prompt::story_summary(story_ctx)` (empty string when `None`).
- `story_opening`: `story_pack.opening.as_str()` when `story_summary` is empty, otherwise empty string.
- `recent_story`: rendered via `pipeline_prompt::recent_story(story_ctx)`.
- `current_proposal`: `spec.proposal`.
- `semantic_queries`: deterministic JSON array of `{ "condition_id": "...", "criterion": "..." }` serialized from `spec.queries`.

### 3.4 LLM Output Protocol and Parser (`extract_llm.rs`)

The required JSON output shape from `extract.evaluate_narrative` is:

```json
{
  "evaluations": [
    {
      "condition_id": "cond_entered_temple",
      "truth": "satisfied"
    }
  ]
}
```

where `truth` is one of `"satisfied"`, `"unsatisfied"`, or `"unknown"` (`Truth`).

Create `crates/aise-core/src/pipeline/extract/extract_llm.rs`:

```rust
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EvaluateNarrativeOutputDto {
    evaluations: Vec<SemanticEvaluationDto>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SemanticEvaluationDto {
    condition_id: NarrativeConditionId,
    truth: Truth,
}

pub(super) async fn evaluate_narrative(
    gateway: &LlmGateway,
    messages: Vec<ChatMessage>,
    control: &TurnControl,
    observation: &Observation,
) -> Result<String, LlmError>;

pub(super) fn parse_narrative_evaluations(
    raw: &str,
    queries: &[SemanticQuery],
) -> Result<BTreeMap<NarrativeConditionId, Truth>, PipelineError>;
```

### 3.5 Trace Contract (`extract_trace.rs`)

Create `crates/aise-core/src/pipeline/extract/extract_trace.rs`:

```rust
pub(super) fn begin_evaluate_narrative(
    observation: &Observation,
    proposal: &str,
    queries: &[SemanticQuery],
) -> Observation;

pub(super) fn finish_evaluate_narrative(
    observation: Observation,
    queries: &[SemanticQuery],
    result: &Result<BTreeMap<NarrativeConditionId, Truth>, PipelineError>,
);
```

### 3.6 File / Directory Layout

```text
crates/aise-core/
├── assets/prompts/
│   ├── csi/extract-evaluate-narrative.md.j2
│   ├── rc/extract-evaluate-narrative.md.j2
│   ├── fti/extract-evaluate-narrative.md.j2
│   └── index.toml
└── src/pipeline/extract/
    ├── config.rs
    ├── extract.rs
    ├── extract_llm.rs
    ├── extract_narrative.rs
    ├── extract_prompt.rs
    ├── extract_trace.rs
    ├── mod.rs
    └── tests/
        ├── config_tests.rs
        ├── extract_llm_tests.rs
        ├── extract_narrative_tests.rs
        ├── extract_prompt_tests.rs
        └── extract_tests.rs
```

---

## 4. Behavior Rules

1. **R-P1-1 (Zero-LLM Fast Path)**: When `story_pack.narrative` is `None`, `ExtractPipeline` MUST return `NarrativeChange { narrative: Change::Unchanged, life_cycle: Change::Unchanged }` without calling `LlmGateway`. When `story_pack.narrative` is `Some(_)` and `queries.is_empty()`, `ExtractPipeline` MUST skip the `LlmGateway` call and invoke `extract_narrative::resolve_narrative` with an empty `BTreeMap` so deterministic conditions (`StoryStarted`, `TurnReaches`, `NodeIs`) still advance.
2. **R-P1-2 (Exact Query Set Matching)**: When `queries` is non-empty, `parse_narrative_evaluations` MUST verify:
   - No duplicate `condition_id` in `evaluations`.
   - Every returned `condition_id` exists in `queries`.
   - Every `condition_id` in `queries` is present in `evaluations` (when uncertain, the LLM MUST return `Truth::Unknown`; omitting any query is a contract failure).
3. **R-P1-3 (Authoritative Proposal Only)**: `extract.evaluate_narrative` MUST evaluate only the finalized `input.proposal` (plus prior story context from `StoryContext`); it MUST NOT receive `player_contribution`.

### 4.1 Error Handling

`parse_narrative_evaluations` returns `PipelineError::InvalidOutput { stage: PipelineStage::Extract, code, message }` with stable codes:
- `extract_narrative_invalid_json`: malformed JSON, unknown fields, invalid `ConditionId`, or invalid `Truth` variant.
- `extract_narrative_duplicate_condition`: a `condition_id` appears more than once.
- `extract_narrative_unknown_condition`: a returned `condition_id` is not in `queries`.
- `extract_narrative_missing_condition`: one or more `condition_id`s from `queries` are missing from `evaluations`.

### 4.2 Concurrency

- `evaluate_narrative` performs at most one `LlmGateway::complete` call per Turn through the shared injected `LlmGateway` (`R-CONC-04`).
- No lock or guard is held across `.await`.

### 4.3 Observability

- When `queries` is non-empty, `ExtractPipeline` creates a child `ObservationKind::Span` named `"extract_evaluate_narrative"` under the stage `&Observation` via `extract_trace::begin_evaluate_narrative` and passes `&narrative_observation` to `extract_prompt::evaluate_narrative` and `extract_llm::evaluate_narrative` (`R-OBS-02`).

---

## 5. Acceptance Criteria

- [ ] `ExtractConfig` is wired into `PipelineConfig` and validated in `PipelineConfig::validate()`.
- [ ] `extract.evaluate_narrative` is registered in `crates/aise-core/assets/prompts/index.toml` with CSI, RC, and FTI template files.
- [ ] `extract_narrative_tests` verifies deterministic transitions when `queries` is empty and semantic transitions when `semantic_results` contains `Satisfied`, `Unsatisfied`, and `Unknown`.
- [ ] `extract_llm_tests` covers valid evaluations and all four error codes in §4.1 (`extract_narrative_invalid_json`, `extract_narrative_duplicate_condition`, `extract_narrative_unknown_condition`, `extract_narrative_missing_condition`).
- [ ] `extract_tests` verifies: (a) 0 LLM calls when `queries` is empty, (b) 1 LLM call when `queries` is non-empty, and (c) node transition and `StoryLifeCycle::Completed` propagation when a terminal node completes.
- [ ] `cargo test -p aise-core` and `cargo clippy --workspace --all-targets -- -D warnings` pass.

---

## 6. Out of Scope / Future Work

- World knowledge (`Fact` / `Rumor`) extraction and per-role memory (`Memory`) extraction → handled in [Phase 2](./2026-10-17-extract-pipeline-spec-phase-2-gemini.md).
- Final numeric budget values in `ExtractConfig` remain `TBD` until the dedicated budget pass.

---

## 7. References

- Source design: [Extract Pipeline 与动态知识存储 — Design](../../design/2026-10-16-extract-pipeline-design-gemini.md)
- Phase 0: [StoryContext, KnowledgeChange, and Store Wiring](./2026-10-17-extract-pipeline-spec-phase-0-gemini.md)
- Pipeline guardrails: `doc/agents/guardrails/pipeline.md`
