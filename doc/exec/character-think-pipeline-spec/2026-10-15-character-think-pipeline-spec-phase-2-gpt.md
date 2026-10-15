# Character Think Execution — Spec

> **Model**: GPT-5.6 Sol
> **Date**: 2026-10-15
> **Status**: Proposed
> **Source Design**: [Character Think Pipeline](../../design/2026-10-14-character-think-pipeline-design-gpt.md)
> **Phase**: Phase 2

---

## 1. Goal

Generate one bounded, role-bound Character Decision per validated non-player request with isolated prompts and bounded stable concurrency.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Add `ThinkConfig` and aggregate it into `PipelineConfig`.
- Replace the Think echo placeholder with role-aware prompt, LLM, parsing, and trace modules.
- Build one isolated prompt per Character Think request.
- Bind RoleId in engine code instead of trusting model output.
- Execute multiple requests with bounded concurrency and stable output order.
- Fail the entire Think stage if any request fails.
- Add a shared concurrency semaphore to `LlmGateway`.
- Add CSI/RC/FTI prompt assets for `think.character_decision`.
- Emit stage-level and per-role observations.

### 2.2 Non-Goals

- Does not expose another role's profile, backstory, or memories to a target call.
- Does not expose Plan `story_goal` to Character Think.
- Does not generate final story prose or committed world changes.
- Does not persist decisions or add runtime character memory.
- Does not generate chain-of-thought, long internal monologue, or guaranteed future outcomes.
- Does not retry, repair, or partially accept malformed Character Think output.
- Does not let the model return or choose RoleId.

### 2.3 Implementation Constraints

- Phases 0 and 1 are required.
- Phases 0–3 MUST land as one hard-refactor change.
- Think calls LLMs only through the injected shared `LlmGateway`.
- Think MUST NOT import Plan, Retrieval, Generate, or Runtime.
- Runtime remains responsible for optional-stage routing.
- Prompt text lives only in Prompt assets and follows CSI/RC/FTI ordering.
- No detached task, unbounded `join_all`, hidden queue, direct provider call, or partial-success path is allowed.
- No code comments are permitted.

---

## 3. Contracts

### 3.1 Think configuration

Create `crates/aise-core/src/pipeline/think/config.rs`:

```rust
use serde::Deserialize;
use std::num::NonZeroUsize;

const DEFAULT_MAX_CONCURRENCY: NonZeroUsize = NonZeroUsize::new(4).unwrap();
const DEFAULT_MAX_PROFILE_BYTES: usize = 4 * 1024;
const DEFAULT_MAX_BACKSTORY_BYTES: usize = 4 * 1024;
const DEFAULT_MAX_SEED_MEMORIES: usize = 8;
const DEFAULT_MAX_SEED_MEMORY_BYTES: usize = 8 * 1024;
const DEFAULT_MAX_DECISION_BYTES: usize = 512;
const DEFAULT_MAX_SUGGESTED_UTTERANCE_BYTES: usize = 512;
const DEFAULT_MAX_TOTAL_OUTPUT_BYTES: usize = 1024;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThinkConfig {
    pub max_concurrency: NonZeroUsize,
    pub max_profile_bytes: usize,
    pub max_backstory_bytes: usize,
    pub max_seed_memories: usize,
    pub max_seed_memory_bytes: usize,
    pub max_decision_bytes: usize,
    pub max_suggested_utterance_bytes: usize,
    pub max_total_output_bytes: usize,
}
```

`ThinkConfig::default()` returns `4`, `4096`, `4096`, `8`, `8192`, `512`, `512`, and `1024` respectively. The implementation may express the nonzero default without a const `unwrap`, but the public values MUST match.

Extend:

```rust
pub struct PipelineConfig {
    pub baseline: BaselineConfig,
    pub plan: PlanConfig,
    pub think: ThinkConfig,
    pub summary: SummaryConfig,
}
```

Re-export `ThinkConfig` from `pipeline/think/mod.rs` and `pipeline/mod.rs`.

### 3.2 Global LLM concurrency

Extend `LlmConfig`:

```rust
pub max_concurrent_calls: NonZeroUsize,
```

Its default is `16`. Extend `aise-service` configuration with:

```text
AISE_LLM_MAX_CONCURRENT_CALLS
```

The environment default is `16`; zero or an invalid integer fails service configuration.

`LlmGateway` MUST own:

```rust
pub struct LlmGateway {
    provider: Arc<dyn LlmProvider>,
    config: LlmConfig,
    limiter: Semaphore,
}
```

Every `complete` call MUST acquire one permit before provider dispatch and hold it until the provider future completes or is cancelled. Permit waiting MUST select against `TurnControl` cancellation and deadline.

Add:

```rust
#[error("llm concurrency limiter is closed")]
ConcurrencyLimiterClosed,
```

to `LlmError`, and map it to `kind() == "concurrency_limiter_closed"`.

### 3.3 Think stage contract

Replace the placeholder types:

```rust
#[derive(Serialize)]
pub struct ThinkInput<'a> {
    pub requests: &'a [CharacterThinkRequest],
    pub resolved_roles: &'a [ResolvedRole],
    pub player_contribution: &'a PlayerContribution,
    pub directions: &'a [NarrativeDirection],
}

#[derive(Serialize)]
pub struct ThinkOutput {
    pub decisions: Vec<CharacterDecision>,
}

pub struct ThinkPipeline {
    gateway: Arc<LlmGateway>,
    prompt: Arc<Prompt>,
    store: Arc<Store>,
    config: ThinkConfig,
}

impl ThinkPipeline {
    pub fn new(
        gateway: Arc<LlmGateway>,
        prompt: Arc<Prompt>,
        store: Arc<Store>,
        config: ThinkConfig,
    ) -> Self;
}
```

`Runtime::new` MUST construct `ThinkPipeline` with shared gateway, prompt, store, and `pipeline_config.think`.

### 3.4 Per-request execution

Create:

```rust
async fn decide(
    &self,
    request: &CharacterThinkRequest,
    role: &ResolvedRole,
    story_ctx: &StoryContext,
    input: &ThinkInput<'_>,
    control: &TurnControl,
    observation: &Observation,
) -> Result<CharacterDecision, PipelineError>;
```

The per-request flow is:

```text
find exactly one ResolvedRole by request RoleId
build bounded target-only prompt variables
render think.character_decision
call LlmGateway once
strictly parse and normalize output
bind request RoleId
return CharacterDecision
```

### 3.5 Seed-memory projection

Create `pipeline/think/think_context.rs` with:

```rust
pub(super) fn seed_memories(
    role: &ResolvedRole,
    config: &ThinkConfig,
) -> String;
```

The helper iterates `ResolvedRole.seed_memories` in `MemoryId` order and includes the longest stable prefix satisfying both:

```text
included item count <= max_seed_memories
rendered UTF-8 bytes <= max_seed_memory_bytes
```

Each item renders:

```text
- memory_id: "<JSON-escaped MemoryId>"
  content: "<JSON-escaped content>"
  retrieval_hint: "<JSON-escaped retrieval hint>"
```

An item that would exceed the remaining byte budget is not added, and iteration stops. No partial item is rendered.

### 3.6 Think prompt protocol

Register:

```toml
[[prompts]]
id = "think.character_decision"
csi = "csi/think-character-decision.md.j2"
rc = "rc/think-character-decision.md.j2"
fti = "fti/think-character-decision.md.j2"
```

The RC variables and section order are:

```text
Story Summary or Story Opening
Recent Story
Pending Player Contribution
Narrative Direction
Target Role ID
Target Name and Aliases
Target Profile
Target Backstory
Target Seed Memories
Thinking Focus
```

Aliases render as a JSON array. Profile renders as compact JSON. Backstory renders as its original UTF-8 text. Before Prompt rendering, reject a serialized profile larger than `max_profile_bytes` with `think_profile_too_long` and reject a backstory larger than `max_backstory_bytes` with `think_backstory_too_long`.

The prompt MUST enforce:

- The model acts only as the named target role.
- Continuity, player contribution, Narrative Direction, and Thinking Focus are context, not automatically character-known facts.
- The target may use profile, backstory, and seed memories as its own information.
- Hidden information about other roles is absent.
- Thinking Focus directs attention but is not an action command or required outcome.
- Output is an immediate decision and optional utterance, not reasoning or narration.
- The decision may diverge from the Planner's desired direction.
- The output contains no RoleId.

### 3.7 LLM output parsing

Create `pipeline/think/think_output.rs`:

```rust
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CharacterDecisionDto {
    decision: String,
    #[serde(default)]
    suggested_utterance: Option<String>,
}

pub(super) fn parse_character_decision(
    raw: &str,
    role_id: &RoleId,
    config: &ThinkConfig,
) -> Result<CharacterDecision, PipelineError>;
```

Exact model output:

```json
{
  "decision": "non-empty string",
  "suggested_utterance": "optional non-empty string or null"
}
```

Normalization:

1. trim both fields;
2. reject an empty decision;
3. map missing or null `suggested_utterance` to `None`;
4. reject a present empty `suggested_utterance`;
5. enforce each field's byte limit;
6. enforce the combined output byte limit;
7. construct `BoundedText`;
8. bind the provided engine RoleId.

Use:

```text
think_output_invalid_json
think_decision_empty
think_decision_too_long
think_suggested_utterance_empty
think_suggested_utterance_too_long
think_output_too_long
```

### 3.8 Input validation errors

Use:

```text
think_duplicate_request_role_id
think_player_role_forbidden
think_resolved_role_missing
think_profile_too_long
think_backstory_too_long
```

All Think errors map to `PipelineError::InvalidOutput` for malformed LLM data or `PipelineError::InconsistentData` for invalid engine handoff, with `stage = PipelineStage::Think`.

### 3.9 Bounded execution

Implement request execution with an ordered bounded stream equivalent to:

```rust
stream::iter(work)
    .map(|item| self.decide_item(item))
    .buffered(self.config.max_concurrency.get())
    .try_collect::<Vec<_>>()
    .await
```

The implementation MUST use `buffered`, not `buffer_unordered`, so output order equals request order. On the first observed error, it returns `Err`, drops remaining in-flight futures, and exposes no partial decisions.

### 3.10 Think observations

Create `pipeline/think/think_trace.rs` with stage lifecycle helpers and per-role helpers:

```rust
pub fn begin_think(
    parent: &Observation,
    request_count: usize,
) -> Observation;

pub fn finish_think(
    observation: Observation,
    result: &Result<ThinkOutput, PipelineError>,
) -> ();

pub fn begin_role(
    parent: &Observation,
    role_id: &RoleId,
) -> Observation;

pub fn finish_role(
    observation: Observation,
    result: &Result<CharacterDecision, PipelineError>,
) -> ();
```

Metadata:

```text
aise.think.requests
aise.think.decisions
aise.think.role_id
```

The per-role observation is the explicit parent passed to Prompt rendering and `LlmGateway::complete`.

### 3.11 File layout

```text
crates/aise-core/src/
  llm/
    config.rs
    error.rs
    gateway.rs
  pipeline/
    common/
      config.rs
    think/
      mod.rs
      config.rs
      think.rs
      think_context.rs
      think_llm.rs
      think_output.rs
      think_prompt.rs
      think_trace.rs
      tests/
        config_tests.rs
        think_context_tests.rs
        think_output_tests.rs
        think_prompt_tests.rs
        think_tests.rs
        think_trace_tests.rs
```

---

## 4. Behavior Rules

1. **R-CT-P2-1**: Zero Think requests cause Runtime to skip the Think stage.
2. **R-CT-P2-2**: Each non-empty request causes exactly one Prompt render and one `LlmGateway::complete` call.
3. **R-CT-P2-3**: Each call receives only its target `ResolvedRole`; no other role-private field is rendered.
4. **R-CT-P2-4**: Direct Think input validation defensively rejects duplicate and player RoleIds.
5. **R-CT-P2-5**: Decisions preserve request order regardless of completion order.
6. **R-CT-P2-6**: At most `ThinkConfig.max_concurrency` Character Think futures are polled concurrently.
7. **R-CT-P2-7**: All concurrent LLM calls also consume permits from the one shared `LlmGateway` limiter.
8. **R-CT-P2-8**: Any prompt, LLM, parse, validation, or handoff failure fails the whole Think stage.
9. **R-CT-P2-9**: RoleId is copied from the validated request and never deserialized from model output.
10. **R-CT-P2-10**: Character Decisions remain Turn-scoped and are never written by Think.

### 4.1 Error Handling

- Use the exact stable codes in §§3.7–3.8.
- Do not retry, repair JSON, strip prose, infer missing fields, or accept unknown fields.
- Permit wait cancellation returns `LlmError::Cancelled`; permit wait deadline exhaustion returns `LlmError::Timeout`.
- No `unwrap`, `expect`, or panic is allowed on request, role, prompt, LLM, or output data.

### 4.2 Concurrency

- Local Think concurrency and global Gateway concurrency are independent bounds.
- No mutex or write guard may be held across Prompt rendering or LLM `.await`.
- Cancelling or timing out the Turn drops in-flight Think futures and Gateway permits.
- The stage creates no detached tasks, channels, or retained futures.
- `max_concurrency` and `max_concurrent_calls` use `NonZeroUsize`; zero is rejected during configuration deserialization.

### 4.3 Observability

- The Think stage creates one `think` span under the PipelineRunner stage observation.
- Every request creates one role span under that Think span.
- Prompt and LLM observations are children of the corresponding role span.
- RoleId is a structured metadata field.
- Full profile, backstory, memory, decision, and utterance content MUST NOT be added to logs or metadata.
- Captured input/output content MUST pass through `Observation::capture_content`.

---

## 5. Acceptance Criteria

- [ ] `ThinkConfig` exists, is aggregated into `PipelineConfig`, and all eight defaults match §3.1.
- [ ] `LlmConfig.max_concurrent_calls` is nonzero, defaults to `16`, and is loaded by `aise-service`.
- [ ] A Gateway concurrency test proves active provider calls never exceed the configured permit count.
- [ ] Gateway permit waiting respects Turn cancellation and deadline.
- [ ] `ThinkInput`, `ThinkOutput`, and `ThinkPipeline` match §3.3.
- [ ] Prompt tests prove each call contains only the target role's private data.
- [ ] Prompt tests prove `story_goal` and other roles' backstory/memories are absent.
- [ ] Seed-memory tests cover stable order, count limit, byte limit, zero limits, and no partial item.
- [ ] Role-context tests reject oversized profile and backstory with the exact §3.8 codes.
- [ ] Output tests cover valid null/missing utterance and all six output codes in §3.7.
- [ ] Input tests cover all three §3.8 handoff codes.
- [ ] A controlled provider test proves stable decision order when calls complete out of order.
- [ ] A failure test proves no partial `ThinkOutput` reaches Runtime.
- [ ] Trace tests prove explicit stage → role → Prompt/LLM parentage.
- [ ] `rg "result: input.query|ThinkInput.*query" crates/aise-core/src/pipeline/think -g "*.rs"` returns zero matches.
- [ ] `rg "LlmProvider" crates/aise-core/src/pipeline -g "*.rs"` returns zero matches.

---

## 6. Out of Scope / Future Work

- Generate role projection and Character Decision consumption are specified in [Phase 3](./2026-10-15-character-think-pipeline-spec-phase-3-gpt.md).
- Runtime character-memory creation, update, eviction, and persistence require a separate design.

---

## 7. References

- Source design: [Character Think Pipeline](../../design/2026-10-14-character-think-pipeline-design-gpt.md)
- Pipeline guardrails: `doc/agents/guardrails/pipeline.md`
- Concurrency guardrails: `doc/agents/guardrails/concurrency.md`
- Observability guardrails: `doc/agents/guardrails/observability.md`
- Prompt conventions: `doc/agents/guardrails/pipeline.md`
