# Player Contribution Interpretation — Spec

> **Model**: GPT-5.6 Sol
> **Date**: 2026-08-19
> **Status**: Proposed
> **Source Design**: [Player Contribution Interpretation — Design](../design/2026-08-19-player-contribution-interpretation-design-gpt.md)
> **Phase**: N-A

---

## 1. Goal

Upgrade Writer Planner to emit one bounded, ordered, structured interpretation of the raw player contribution and make every downstream story stage use that interpretation while restoring `story_goal` to direction-only guidance.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Replace `writer_planner_output.v1` with `writer_planner_output.v2`, adding required `interpreted_player_contribution.units` to the structured output.
- Add domain types for `speech`, `action`, `private_state`, and `requested_outcome` units and store the validated interpretation on `WriterPlan`.
- Make Writer Planner perform context-aware, ordered, multi-unit semantic interpretation in its existing LLM call.
- Render raw `Pending Player Contribution` only in Writer Planner RC and render it as an explicitly delimited literal data block rather than a JSON-quoted prose string.
- Make `story_goal` describe only the immediate story direction; it MUST NOT quote, paraphrase, enumerate, or classify the player contribution.
- Replace raw contribution prompt data in Character Think, Story Generator, and Story Repairer with the `interpreted_player_contribution` projected from `WriterPlan`.
- Make `unit.kind` the only authority for how supplied player content is realized while allowing Story Generator to add compatible Player Character behavior for narrative quality.
- Update CSI, RC, and FTI assets plus prompt slots, contract references, config limits, renderers, re-exports, and tests.

### 2.2 Non-Goals

- Does not add a `PlayerContributionInterpreter` pipeline, another LLM call, a second model, or provider routing.
- Does not add confidence scores, alternative classifications, clarification UI, fallback classification, or user-facing interpretation controls.
- Does not add semantic validation that compares raw text with units, compares units with `story_goal`, or inspects generated prose for realization correctness.
- Does not add a Validation Pipeline rule, automatic repair trigger, or retry policy for classification errors.
- Does not make interpreted units an exhaustive allowlist of Player Character behavior in generated prose.
- Does not delete raw `player_contribution` from `TurnRequest`, `TurnExecutionContext`, persistence, history, trace, or the web API.
- Does not change database schema, HTTP JSON, story-history JSON, idempotency, retrieval-signal extraction, or committed Turn metadata.
- Does not guarantee action success or requested outcomes.

### 2.3 Implementation Constraints (for code generation)

- This spec generates final-form code. Do **not** keep fallback paths, compatibility shims, dual output contracts, or dual downstream prompt slots.
- Delete `writer_planner_output.v1` and replace it with `writer_planner_output.v2` in the same change.
- Character Think, Story Generator, and Story Repairer MUST NOT retain a raw `player_contribution` prompt variable after this change.
- Do not add comments to Rust source. Preserve `R-CODE-01`, `R-CODE-02`, and `R-CODE-05` from `AGENTS.md`.
- Route the existing Writer Planner LLM request through the current `LlmGateway`; do not add an LLM call site or concurrency path.

---

## 3. Contracts

### 3.1 Domain types

Add the following final-form types to `crates/aise/src/domain/turn/planning.rs` and re-export them from `crates/aise/src/domain/turn/mod.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerContributionKind {
    Speech,
    Action,
    PrivateState,
    RequestedOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerContributionUnit {
    pub kind: PlayerContributionKind,
    pub content: BoundedText,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterpretedPlayerContribution {
    pub units: Vec<PlayerContributionUnit>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WriterPlan {
    pub interpreted_player_contribution: InterpretedPlayerContribution,
    pub story_goal: WriterStoryGoal,
    pub retrieval_plan: RetrievalPlan,
    pub character_think_requests: Vec<CharacterThinkRequest>,
}
```

`PlayerContributionUnit.content` is normalized semantic content. It is not a source span and MUST NOT contain a copy of the whole raw contribution unless the contribution is semantically one unit.

### 3.2 Writer Planner DTO and output contract

Replace the current contract constant and extend the DTOs in `crates/aise/src/planning/planner_output.rs`:

```rust
pub const WRITER_PLANNER_CONTRACT_NAME: &str = "writer_planner_output.v2";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriterPlannerOutputDto {
    pub interpreted_player_contribution: InterpretedPlayerContributionDto,
    pub story_goal: String,
    pub writer_context_gaps: Vec<PlannerWriterContextGapDto>,
    pub character_context_gaps: Vec<PlannerCharacterContextGapDto>,
    pub character_think_requests: Vec<CharacterThinkRequestDto>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterpretedPlayerContributionDto {
    pub units: Vec<PlayerContributionUnitDto>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerContributionUnitDto {
    pub kind: PlayerContributionKindDto,
    pub content: String,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerContributionKindDto {
    Speech,
    Action,
    PrivateState,
    RequestedOutcome,
}
```

The JSON Schema MUST require this exact top-level shape:

```json
{
  "interpreted_player_contribution": {
    "units": [
      {
        "kind": "speech | action | private_state | requested_outcome",
        "content": "non-empty normalized semantic content"
      }
    ]
  },
  "story_goal": "non-empty direction-only string",
  "writer_context_gaps": [],
  "character_context_gaps": [],
  "character_think_requests": []
}
```

Required schema properties:

```json
{
  "type": "object",
  "additionalProperties": false,
  "required": [
    "interpreted_player_contribution",
    "story_goal",
    "writer_context_gaps",
    "character_context_gaps",
    "character_think_requests"
  ]
}
```

`interpreted_player_contribution`, every unit, and all existing nested DTOs MUST set `additionalProperties: false`.

The interpretation portion of the generated schema MUST contain these constraints:

```json
{
  "type": "object",
  "additionalProperties": false,
  "required": ["units"],
  "properties": {
    "units": {
      "type": "array",
      "minItems": 1,
      "maxItems": 32,
      "items": {
        "type": "object",
        "additionalProperties": false,
        "required": ["kind", "content"],
        "properties": {
          "kind": {
            "type": "string",
            "enum": ["speech", "action", "private_state", "requested_outcome"]
          },
          "content": {
            "type": "string",
            "minLength": 1,
            "maxLength": 16384
          }
        }
      }
    }
  }
}
```

The implementation MUST derive `maxItems` and `maxLength` from `PlannerConfig`; the numbers above are the required default-schema values. `writer_planner_compact_prompt_shape()` MUST list the required interpretation object, its four kind values, the configured maximum unit count, and the configured aggregate byte limit.

### 3.3 Planner configuration

Extend `PlannerConfig` in `crates/aise/src/config/planner.rs`:

```rust
pub struct PlannerConfig {
    pub max_context_gaps: usize,
    pub max_character_think_requests: usize,
    pub max_goal_bytes: usize,
    pub max_reason_bytes: usize,
    pub max_entities_per_request: usize,
    pub max_topics_per_request: usize,
    pub max_kinds_per_request: usize,
    pub max_player_contribution_units: usize,
    pub max_interpreted_player_contribution_bytes: usize,
}
```

Required defaults:

```rust
max_player_contribution_units: 32,
max_interpreted_player_contribution_bytes: 16 * 1024,
```

Update the checked-in `[aise.planner]` table in `config/aise_config.toml` with the equivalent explicit values:

```toml
max_player_contribution_units = 32
max_interpreted_player_contribution_bytes = 16384
```

The checked-in config and `PlannerConfig::default()` MUST remain numerically identical for both fields.

`PlannerConfig::validate()` MUST reject zero for either new field with these exact messages:

```text
planner.max_player_contribution_units must be positive
planner.max_interpreted_player_contribution_bytes must be positive
```

### 3.4 Structural output validation

`writer_planner_contract()` MUST structurally validate:

```text
1 <= interpreted_player_contribution.units.len()
interpreted_player_contribution.units.len() <= max_player_contribution_units
unit.content.trim() is non-empty for every unit
sum(unit.content.len()) <= max_interpreted_player_contribution_bytes
```

Convert DTO units in `RetrievalPlanBuilder::build()` before constructing `WriterPlan`:

```rust
fn convert_player_contribution(
    &self,
    value: InterpretedPlayerContributionDto,
) -> Result<InterpretedPlayerContribution, PlanningError>;
```

Each content value MUST be converted to `BoundedText`. Conversion MUST preserve unit order and map DTO kinds one-to-one to domain kinds.

No validator may compare the unit classification with raw `player_contribution`, inspect whether `story_goal` references it, or judge story realization in this phase.

### 3.5 Prompt context contracts

Writer Planner remains the only prompt context with raw contribution:

```rust
pub struct WriterPlannerPromptProjection {
    pub context: WriterPlannerPromptContext,
    pub rc_vars: RuntimePromptVars,
    pub fti_vars: TrustedPromptVars,
}
```

Replace `render_data()` for the Writer Planner contribution with:

```rust
fn render_pending_player_contribution(value: &str) -> String;
```

It MUST render the value as a literal block with this shape:

```yaml
text: |-
  我有点害怕
```

Every original line MUST be indented beneath `text: |-`. The renderer's YAML/container syntax is not part of the player's text and MUST be described as non-semantic framing in Writer Planner CSI.

Replace downstream prompt fields:

```rust
pub struct CharacterThinkPromptContext {
    pub target_role: CharacterThinkRolePromptView,
    pub current_role_state: CharacterThinkStatePromptView,
    pub story_continuity: CharacterThinkStoryContinuityPromptView,
    pub narrative_character_impulses: Vec<CharacterThinkImpulsePromptView>,
    pub thinking_focus: BoundedText,
    pub interpreted_player_contribution: InterpretedPlayerContribution,
}

pub struct StoryGeneratorPromptContext {
    pub story_profile: StoryProfilePromptView,
    pub instance_settings: StoryGeneratorInstanceSettingsPromptView,
    pub story_continuity: StoryContinuityPromptView,
    pub player_role: StoryGeneratorRolePromptView,
    pub ai_roles: Vec<StoryGeneratorRolePromptView>,
    pub relevant_knowledge: WorldKnowledgePromptView,
    pub story_goal: BoundedText,
    pub narrative_direction: NarrativeDirectionPromptView,
    pub active_story_constraints: Vec<ActiveStoryConstraintPromptView>,
    pub character_decisions: Vec<StoryGeneratorCharacterDecisionPromptView>,
    pub interpreted_player_contribution: InterpretedPlayerContribution,
}
```

Both projectors MUST clone `ctx.plan()?.interpreted_player_contribution`. They MUST NOT call `ctx.player_contribution()`.

Add one shared renderer under the existing prompt module rather than duplicating stage-local renderers:

```rust
pub fn render_interpreted_player_contribution(
    value: &InterpretedPlayerContribution,
) -> String;
```

Required output:

```yaml
- kind: private_state
  content: "玩家角色感到些许害怕"
```

The shared renderer MUST preserve unit order. Story Repairer MUST continue to reuse `StoryGeneratorPromptContext`, so it receives the identical rendered interpretation without a separate raw field.

### 3.6 Prompt slots and contract references

Update `crates/aise/assets/prompts/context-v2/slots.yaml`:

```yaml
context.writer_planner.rc:
  vars:
    - { name: player_contribution, var_type: string, required: true }

context.character_think.rc:
  vars:
    - { name: interpreted_player_contribution, var_type: string, required: true }

context.story_generator.rc:
  vars:
    - { name: interpreted_player_contribution, var_type: string, required: true }

context.story_repairer.rc:
  vars:
    - { name: interpreted_player_contribution, var_type: string, required: true }
```

Remove `player_contribution` from the latter three profiles. Update `crates/aise/assets/prompts/context-v2/index.yaml`:

```yaml
output_contract_ref: writer_planner_output.v2
```

### 3.7 Runtime Context headings

Required headings:

```markdown
Writer Planner:  ## Pending Player Contribution
Character Think: ## Interpreted Player Contribution
Story Generator: ## Interpreted Player Contribution
Story Repairer:  ### Interpreted Player Contribution
```

No downstream RC template may contain `{{ player_contribution }}` or the heading `Pending Player Contribution`.

### 3.8 File / directory impact

```text
crates/aise/src/
├── config/planner.rs
├── domain/turn/
│   ├── mod.rs
│   └── planning.rs
├── planning/
│   ├── mod.rs
│   ├── planner_output.rs
│   ├── retrieval_plan_builder.rs
│   └── writer_planner_prompt.rs
├── prompt/
│   ├── mod.rs
│   └── player_contribution.rs
├── character/character_think_prompt.rs
└── story/
    ├── story_generator_prompt.rs
    └── story_repairer_prompt.rs

crates/aise/assets/prompts/context-v2/
├── csi/{writer-planner,character-think,story-generator,story-repairer}.md.j2
├── rc/{writer-planner,character-think,story-generator,story-repairer}.md.j2
├── fti/{writer-planner,character-think,story-generator,story-repairer}.md.j2
├── index.yaml
└── slots.yaml

config/
└── aise_config.toml
```

Unit tests remain in the existing `tests/<source>_tests.rs` files. Add `crates/aise/src/prompt/tests/player_contribution_tests.rs` and register it through the existing prompt test-module pattern; do not add inline test modules.

---

## 4. Behavior Rules

1. **R-1 — Single interpretation**: Writer Planner MUST emit exactly one non-empty ordered `interpreted_player_contribution.units` array for every accepted raw contribution.
2. **R-2 — Context-aware classification**: Writer Planner MUST use the full supplied Runtime Context and linguistic semantics to select kinds; it MUST NOT implement classification as quote-, keyword-, or regex-only matching.
3. **R-3 — Exhaustive ordered decomposition**: Every material speech, action, private-state, or requested-outcome component MUST appear exactly once, in source order; a mixed clause MUST be split when its components require different kinds.
4. **R-4 — Kind semantics**: `speech` means intended spoken content; `action` means Player Character behavior or attempt; `private_state` means internal thought, emotion, sensation, belief, suspicion, intention, or hope; `requested_outcome` means a desired world/NPC result not owned as Player Character behavior.
5. **R-5 — Intelligent inference**: Explicit quotation and speech/thought verbs are evidence, not requirements. `我有点害怕` MUST be demonstrated as `private_state`; `我说：“我有点害怕。”` MUST be demonstrated as `speech`.
6. **R-6 — Container neutrality**: Quotes, YAML markers, headings, or other formatting added by RC rendering MUST NOT count as evidence for any unit kind.
7. **R-7 — Semantic preservation**: Unit `content` MAY normalize person, tense, ellipsis, and wording, but MUST preserve the essential supplied meaning and MUST NOT add a new contribution component.
8. **R-8 — Direction-only goal**: `story_goal` MUST describe the immediate desired story direction and MUST NOT quote, paraphrase, enumerate, or classify raw input or interpreted units.
9. **R-9 — Downstream authority**: Character Think, Story Generator, and Story Repairer MUST use `interpreted_player_contribution` as their only representation of supplied player material and MUST NOT access raw `ctx.player_contribution()` during prompt projection.
10. **R-10 — Supplied-content modality**: Story Generator MUST realize supplied unit content according to `unit.kind`; neither `story_goal` nor prose convenience may turn `private_state` into supplied speech, speech into thought, or `requested_outcome` into supplied Player Character behavior.
11. **R-11 — Creative expansion**: Unit kinds constrain only the supplied content. Story Generator MAY add plausible Player Character speech, action, reactions, decisions, or private states—including from a private-state-only input—when the addition improves the story and remains consistent with continuity, character identity, hard constraints, causal state, and the units' essential meaning.
12. **R-12 — Action causality**: An `action` unit establishes behavior or an attempt, not guaranteed success or consequences; Story Generator resolves results through story causality.
13. **R-13 — Private-state scope**: A `private_state` unit establishes only the Player Character's subjective state. Character Think MUST NOT use it as Target Character knowledge without an independent observable or authorized basis.
14. **R-14 — Requested outcomes**: A `requested_outcome` is non-authoritative. Story Generator MAY accept, adapt, defer, complicate, or reject it for causal and narrative quality, but MUST NOT treat it as guaranteed world state or automatically convert it to Player Character speech/action.
15. **R-15 — Story quality**: After preserving unit modality and essential meaning, Story Generator SHOULD prefer coherent, engaging, causally progressing prose over minimal literal transcription.
16. **R-16 — Prompt quotas**: Each modified CSI MUST retain the project's `MUST 10 / SHOULD 3 / NEVER 5` rule budget; consolidate wording rather than exceeding the quota.
17. **R-17 — No hidden compatibility**: Active code and prompt assets MUST reference only `writer_planner_output.v2`; no v1 parser, fallback schema, or duplicated raw downstream slot remains.

### 4.1 Required Writer Planner examples

Writer Planner CSI or FTI MUST contain these contrastive mappings in structured form:

```json
{"input":"我有点害怕","units":[{"kind":"private_state","content":"玩家角色感到些许害怕"}]}
{"input":"我说：“我有点害怕。”","units":[{"kind":"speech","content":"我有点害怕"}]}
{"input":"我后退一步，问“你是谁”，心想他可能认识我","units":[{"kind":"action","content":"后退一步"},{"kind":"speech","content":"你是谁"},{"kind":"private_state","content":"对方可能认识玩家角色"}]}
{"input":"让门外的人立刻投降","units":[{"kind":"requested_outcome","content":"门外的人立刻投降"}]}
```

These are classification examples, not deterministic string special cases.

### 4.2 Error handling

- Keep the existing structured-output decode and schema failure path through `LlmGateway` and `TurnFailureKind::Llm`.
- Map empty units, excessive unit count, empty content, or aggregate content overflow to `LlmOutputViolation` under contract name `writer_planner_output.v2`.
- Map DTO-to-domain limit failures through existing `PlanningError::InvalidOutput` or `PlanningError::LimitExceeded`; add stable codes/limit names only where the existing variants require them.
- `StoryGeneratorProjectionError::InvalidPlayerContribution` MUST be deleted because raw contribution is no longer parsed by that projector.
- Character Think and Story Generator MUST return their existing `MissingStageState` / `MissingWriterPlan` errors when no `WriterPlan` is available; do not reconstruct from raw input.

### 4.3 Concurrency

- Do not add an LLM call, task, channel, lock, cache, queue, or background worker.
- Writer Planner continues to make exactly one `complete_structured_composed()` call through the injected `LlmGateway` and existing shared limiter.
- Interpretation is Turn-local through `WriterPlan`; it MUST NOT be persisted as mutable runtime state or shared across Turns.

### 4.4 Observability

- Keep the existing `writer_plan` LLM span and structured-output metadata.
- Contract observability MUST report `writer_planner_output.v2` through the existing output-contract name/hash fields.
- Do not log raw contribution, normalized unit content, or a new classification payload outside the existing trace-content policy.
- Do not add metrics or tracing spans in this phase.

---

## 5. Acceptance Criteria

- [ ] `PlayerContributionKind`, `PlayerContributionUnit`, `InterpretedPlayerContribution`, and the new `WriterPlan` field match §3.1 — verified by `cargo test -p aise planning::tests::writer_planner_tests::interpreted_player_contribution_roundtrips`
- [ ] `WRITER_PLANNER_CONTRACT_NAME` is exactly `writer_planner_output.v2`, and the schema required list matches §3.2 — verified by `cargo test -p aise planning::tests::planner_output_tests`
- [ ] Schema/DTO tests accept all four enum kinds and reject unknown kinds, empty units, null units, unknown fields, empty content, excessive unit count, and aggregate byte overflow — verified by `cargo test -p aise planning::tests::planner_output_tests`
- [ ] `PlannerConfig` contains and validates both limits and defaults from §3.3, and `config/aise_config.toml` declares the same values — verified by `cargo test -p aise --test config_tests planner_interpretation_limits`
- [ ] `RetrievalPlanBuilder` preserves unit order and kind/content mappings in `WriterPlan` — verified by `cargo test -p aise planning::tests::retrieval_plan_builder_tests`
- [ ] Writer Planner RC renders multiline raw contribution under `text: |-` without JSON-quoting the whole value — verified by `cargo test -p aise planning::tests::writer_planner_prompt_tests`
- [ ] Writer Planner prompt contains all four required contrastive mappings and direction-only `story_goal` wording — verified by `cargo test -p aise planning::tests::writer_planner_prompt_tests`
- [ ] Character Think RC receives typed units, labels private/requested units correctly, and receives no raw marker — verified by `cargo test -p aise character::tests::character_think_prompt_tests`
- [ ] Story Generator RC renders typed units in order and never contains the original raw contribution marker — verified by `cargo test -p aise story::tests::story_generator_prompt_tests`
- [ ] Story Repairer reuses the exact rendered interpreted contribution from generation context and receives no raw contribution — verified by `cargo test -p aise story::tests::story_repairer_prompt_tests`
- [ ] Story Generator CSI/FTI explicitly allow compatible Player Character expansion from private-state-only input and contain no thought-only speech/action prohibition — verified by `cargo test -p aise story::tests::story_generator_prompt_tests`
- [ ] Writer Planner, Character Think, Story Generator, and Story Repairer CSI retain exactly 10 MUST, 3 SHOULD, and 5 NEVER bullets — verified by `cargo test -p aise --test prompt_context_contract_tests csi_rule_budgets`
- [ ] `slots.yaml` contains raw `player_contribution` only for Writer Planner and `interpreted_player_contribution` for all three downstream profiles — verified by `cargo test -p aise --test prompt_context_contract_tests interpreted_player_contribution_slot_ownership`
- [ ] `index.yaml` references `writer_planner_output.v2`, and `rg 'writer_planner_output\.v1' crates/aise/src crates/aise/assets crates/aise/tests` returns zero matches
- [ ] `rg 'ctx\.player_contribution\(\)' crates/aise/src/character crates/aise/src/story` returns zero matches
- [ ] `rg '\{\{ player_contribution \}\}|Pending Player Contribution' crates/aise/assets/prompts/context-v2/{csi,rc,fti}/{character-think,story-generator,story-repairer}.md.j2` returns zero matches
- [ ] No semantic validator, classification retry, or new LLM call was added — verified by code review and unchanged Writer Planner call count
- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes
- [ ] `cargo test --workspace --all-targets --all-features` passes

---

## 6. Out of Scope / Future Work

- Measure per-kind and mixed-unit classification accuracy on real player traces.
- Add confidence, alternatives, or clarification only if product data shows that ambiguity handling improves the experience.
- Split interpretation into a dedicated pipeline or specialized model only if measured accuracy gains justify another LLM call.
- Add semantic cross-field or prose-realization validation in a later Validation Pipeline spec.
- Add interpretation inspection/correction UI if regeneration alone proves insufficient.

---

## 7. References

- Source design: [Player Contribution Interpretation — Design](../design/2026-08-19-player-contribution-interpretation-design-gpt.md)
- Prior realization design: [Player Contribution Realization — Design](../design/CSI-RC-FTI/2026-08-19-player-contribution-realization-design-gpt.md)
- Prior realization spec: [Player Contribution Realization — Spec](CSI-RC-FTI/2026-08-19-player-contribution-realization-spec-gpt.md)
- Writer Planner prompt spec: [Writer Planner CSI-RC-FTI Prompt Spec](CSI-RC-FTI/2026-08-11-writer-planner-csi-rc-fti-prompt-spec-gpt.md)
- Story Generator prompt spec: [Story Generator CSI-RC-FTI Prompt Spec](CSI-RC-FTI/2026-08-12-story-generator-csi-rc-fti-prompt-spec-gpt.md)
- External prior art: [FIREBALL](https://aclanthology.org/2023.acl-long.229.pdf), [context-aware SLU](https://aclanthology.org/W17-5514.pdf), [multi-intent span extraction](https://aclanthology.org/2024.findings-emnlp.919.pdf)
- Guardrails: `AGENTS.md` and `doc/agents/guardrails/`
