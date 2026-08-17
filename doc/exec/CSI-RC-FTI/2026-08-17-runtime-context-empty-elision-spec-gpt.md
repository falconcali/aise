# Runtime Context Empty Elision — Spec

> **Model**: GPT-5
> **Date**: 2026-08-17
> **Status**: Proposed
> **Source Design**: [Runtime Context Empty Elision](../design/2026-08-17-runtime-context-empty-elision-design-gpt.md)
> **Phase**: N/A

---

## 1. Goal

Eliminate model-visible empty sentinels and empty sections from every Runtime Context while preserving required data, completeness semantics, meaningful scalar values, and all structured output contracts.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Apply one empty-elision contract to WriterPlanner, CharacterThink, StoryGenerator, StoryRepairer, and StoryStateExtractor RC projection.
- Render an empty optional section as an empty internal string and conditionally omit its heading and body from `.md.j2` output.
- Omit empty or inapplicable optional fields inside non-empty Story Profile, Role, Knowledge, Narrative, Decision, Impulse, and Validation Issue objects.
- Preserve index `scope` when index entries are empty and preserve meaningful scalar `0` and `false` values.
- Reject empty required stage data through typed projection errors rather than hiding the missing data.
- Delete prompt-only Optional wrappers and fields that current projectors always populate or never populate.
- Update RC templates and all prompt projection, composition, trace-contract, and section-order tests.

### 2.2 Non-Goals

- Does not change CSI, FTI, Planner output, Character Decision output, StoryGenerator output, StoryStateExtractor output, or any JSON Schema.
- Does not omit required empty arrays or nullable fields from model structured output; output serialization remains governed exclusively by its Schema.
- Does not change Domain, Snapshot, Story Pack, Persistence, HTTP/WS API, config, retrieval, or token-budget limits.
- Does not treat numeric `0`, boolean `false`, an empty string inside authored story prose, or an empty string inside a scalar attribute as automatically absent.
- Does not convert retrieval, index, persistence, or validation failures into empty RC sections.
- Does not reimplement Current Scene, Premise, or Story Continuity changes owned by the predecessor specs.
- Does not introduce a generic Prompt DSL, new dependency, new configuration flag, or compatibility sentinel.

### 2.3 Implementation Constraints (for code generation)

- Implement this spec after [Current Scene Removal](2026-08-17-current-scene-removal-spec-gpt.md) and [Story Context Simplification](2026-08-17-story-context-simplification-spec-gpt.md); use their final RC variable names and types.
- This spec generates final-form code. Do **not** retain `None.`, `null`, `[]`, `{}`, `N/A`, `not available`, or another replacement sentinel for absent optional RC data.
- Old renderer branches, Optional wrappers, prompt fields, template paths, and tests superseded by this spec MUST be deleted in the same change.
- All variables declared for an RC slot remain `required: true` strings in `slots.yaml`; every Projector MUST still supply every declared key.
- Empty optional RC values use exactly `Value::String(String::new())` internally. Do not omit the map key and do not use `Value::Null`.
- Existing prompt data isolation remains mandatory: asset, story, player, Knowledge, and validation text stays in RC and is never promoted into CSI or FTI.
- `R-ARCH-01/03/04/05`, `R-REFACTOR-01/02`, `R-CODE-01/02/05/07`, `R-LAYER-01`, and `R-AISE-01/02/03/06` remain mandatory.

---

## 3. Contracts

### 3.1 RC Value Protocol

Every declared RC variable MUST be present in `RuntimePromptVars` with a string value:

```rust
RuntimePromptVars::new(HashMap::from([
    ("required_section".into(), Value::String(required_rendered)),
    (
        "optional_section".into(),
        Value::String(optional_rendered.unwrap_or_default()),
    ),
]))
```

The semantic mapping is exact:

| Typed value | RC variable string | Model-visible result |
|---|---|---|
| Required value present | rendered non-empty string | section always rendered |
| Required value absent or trim-empty | no projection | typed error |
| Optional `None` | `""` | field/section omitted |
| Optional trim-empty text | `""` | field/section omitted |
| Optional empty list/map | `""` | field/section omitted |
| Optional non-empty value | rendered non-empty string | field/section rendered |
| Completeness/status with empty entries | status string only | section rendered with status |
| Scalar `0` or `false` | rendered scalar | retained |

No RC renderer may return an absence sentinel. A user-authored or model-authored prose value containing the literal text `None.` remains unchanged because its content is not an engine-generated sentinel.

### 3.2 Prompt-Only Type Hardening

After the predecessor specs, `StoryGeneratorPromptContext` MUST make Instance Settings non-optional:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct StoryGeneratorPromptContext {
    pub story_profile: StoryProfilePromptView,
    pub instance_settings: StoryGeneratorInstanceSettingsPromptView,
    pub story_continuity: StoryContinuityPromptView,
    pub player_role: StoryGeneratorRolePromptView,
    pub ai_roles: Vec<StoryGeneratorRolePromptView>,
    pub relevant_writer_knowledge: Vec<StoryGeneratorKnowledgePromptView>,
    pub story_goal: BoundedText,
    pub narrative_direction: StoryGeneratorNarrativeDirectionPromptView,
    pub active_story_constraints: Vec<ActiveStoryConstraintPromptView>,
    pub character_decisions: Vec<StoryGeneratorCharacterDecisionPromptView>,
    pub player_input: BoundedText,
}
```

`StoryGeneratorKnowledgePromptView.entry_id` is required and the always-empty `title` field is deleted:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct StoryGeneratorKnowledgePromptView {
    pub entry_id: KnowledgeSourceId,
    pub kind: KnowledgeKind,
    pub scope: KnowledgeScopePromptView,
    pub content: BoundedText,
}
```

Delete these final-form type shapes and every matching constructor/test path:

```rust
instance_settings: Option<StoryGeneratorInstanceSettingsPromptView>
entry_id: Option<KnowledgeSourceId>
title: Option<BoundedText>
```

StoryGenerator projection MUST construct Instance Settings directly from `baseline.instance_settings` and Knowledge `entry_id` directly from `item.provenance.source_id`.

### 3.3 Top-Level Section Policy

The final post-predecessor RC sections use these policies.

#### WriterPlanner

| Variable / section | Policy |
|---|---|
| `story_profile` / Story Profile | Required |
| `instance_settings` / Instance Settings | Required |
| `story_summary` / Story Summary | Optional |
| `recent_story` / Recent Story | Optional |
| `player_character` / Player Character | Required |
| `relevant_characters` / Relevant Characters | Optional |
| `relevant_knowledge` / Relevant Knowledge | Optional |
| `character_index` / Character Index | Completeness; always render `scope` |
| `knowledge_entry_index` / Knowledge Entry Index | Completeness; always render `scope` |
| `narrative_plan` / Narrative Plan | Optional |
| `active_story_constraints` / Active Story Constraints | Optional |
| `player_input` / Player Input | Required |

#### CharacterThink

| Variable / section | Policy |
|---|---|
| `target_character` / Target Character | Required |
| `current_character_state` / Current Character State | Required |
| `story_summary` / Story Summary | Optional |
| `recent_story` / Recent Story | Optional |
| `relevant_character_knowledge` / Relevant Character Knowledge / Memory | Optional |
| `narrative_character_impulses` / Narrative Character Impulses | Optional |
| `thinking_focus` / Thinking Focus | Required |
| `player_input` / Player Input | Required |

#### StoryGenerator

| Variable / section | Policy |
|---|---|
| `story_profile` / Story Profile | Required |
| `instance_settings` / Instance Settings | Required |
| `story_summary` / Story Summary | Optional |
| `recent_story` / Recent Story | Optional |
| `player_character` / Player Character | Required |
| `ai_characters` / AI Characters | Optional |
| `active_story_constraints` / Active Story Constraints | Optional |
| `story_goal` / Immediate Story Goal | Required |
| `narrative_direction` / Narrative Direction | Optional |
| `relevant_writer_knowledge` / Relevant Writer Knowledge | Optional |
| `character_decisions` / AI Character Decisions | Optional |
| `player_input` / Player Input | Required |

#### StoryRepairer

StoryRepairer inherits every StoryGenerator policy inside Original Story Generation Context. `previous_story_text` / Previous Story Text and `validation_issues` / Validation Issues are Required. `StoryRepairerProjectionError::MissingPreviousStory`, `EmptyValidationIssues`, and `PreviousStoryExceedsBounds` remain the enforcement path.

#### StoryStateExtractor

| Variable / section | Policy |
|---|---|
| `story_text` / Story Text | Required |
| `roles` / Pre-turn Roles | Required and non-empty |
| `relationships` / Pre-turn Relationships | Optional |
| `modifiable_knowledge` / Modifiable Knowledge | Optional |
| `condition_queries` / Narrative Condition Queries | Optional |
| `previous_extraction` / Previous Extraction | Optional |
| `validation_issues` / Validation Issues | Optional on initial extraction; Required and non-empty on re-extraction |

### 3.4 RC Template Protocol

Required sections remain unconditional:

```jinja
## Player Input

{{ player_input }}
```

Every Optional top-level section MUST wrap its heading and body in the same condition:

```jinja
{% if relevant_knowledge %}
## Relevant Knowledge

{{ relevant_knowledge }}
{% endif %}
```

Story Continuity retains the predecessor contract: the parent renders only when either child is non-empty, and each child renders independently.

```jinja
{% if story_summary or recent_story %}
## Story Continuity

{% if story_summary %}
### Story Summary

{{ story_summary }}
{% endif %}
{% if recent_story %}
### Recent Story

{{ recent_story }}
{% endif %}
{% endif %}
```

WriterPlanner Character Index and Knowledge Entry Index remain unconditional because their rendered values always contain `scope`.

StoryRepairer uses the same conditions with its existing nested heading levels. The outer `## Original Story Generation Context`, `## Previous Story Text`, and `## Validation Issues` headings remain unconditional.

StoryStateExtractor keeps Story Text and Pre-turn Roles unconditional. Relationships, Modifiable Knowledge, Condition Queries, Previous Extraction, and Validation Issues use independent conditions.

Templates MUST test string truthiness only. They MUST NOT compare against `None.`, `null`, `[]`, `{}`, or any other sentinel. Whitespace-control markers MAY remove template-generated blank lines but MUST NOT trim inserted RC values.

`crates/aise/assets/prompts/context-v2/slots.yaml` retains every final predecessor variable as `var_type: string, required: true`.

### 3.5 Collection Renderer Protocol

These logical collection renderers return `String::new()` for an empty input and retain their existing deterministic ordering for non-empty input:

| Stage | Renderer / value |
|---|---|
| WriterPlanner | Relevant Characters |
| WriterPlanner | Relevant Knowledge |
| WriterPlanner | Narrative Plan when all three collections are empty |
| WriterPlanner | Active Story Constraints |
| CharacterThink | Relevant Character Knowledge |
| CharacterThink | Narrative Character Impulses |
| StoryGenerator/Repairer | AI Characters |
| StoryGenerator/Repairer | Active Story Constraints |
| StoryGenerator/Repairer | Narrative Direction when goals and intents are empty |
| StoryGenerator/Repairer | Relevant Writer Knowledge |
| StoryGenerator/Repairer | AI Character Decisions |
| StoryStateExtractor | Pre-turn Relationships |
| StoryStateExtractor | Modifiable Knowledge |
| StoryStateExtractor | Narrative Condition Queries |
| StoryStateExtractor | Validation Issues on initial extraction |

Story Summary and Recent Story retain the exact prose-only empty-string and `\n\n` join contract from Story Context Simplification.

`previous_extraction: None` MUST project as `Value::String(String::new())`; a present extraction is rendered unchanged as the existing bounded pretty JSON string.

### 3.6 Index Renderer Protocol

Role and Knowledge indexes MUST render an empty entry set exactly as:

```text
scope: complete
```

or:

```text
scope: prefiltered
```

For non-empty entries they render:

```text
scope: <complete|prefiltered>
entries:
- ...
```

They MUST NOT render `entries: None.`, `entries: []`, or an empty `entries:` key. Target maps remain empty when no entries exist. A failed or unavailable index MUST return its existing typed Context/Projection error and MUST NOT use the empty-entry representation.

### 3.7 Object Field Protocol

#### Story Profile

`language`, `point_of_view`, and `tense` are Required fields. `genre`, `themes`, and `tone` lines render only when their vectors are non-empty. Story Profile itself remains Required.

#### Role and Role State

Every Role renders `role_id`, `name`, and `location`. `role` renders only when the label differs from the name. `appearance`, `personality`, `speaking_style`, and `background` render only for a present non-whitespace value. `dialogue_examples`, `goals`, and `attributes` render only when non-empty.

An empty goals vector MUST NOT produce `goals: None.` or `goals: []`. An empty attribute map/list MUST NOT produce `attributes: None.`, `attributes: {}`, or an `attributes:` heading.

Attribute values MUST use the existing scalar renderer. `ScalarValue::Integer(0)`, `ScalarValue::Bool(false)`, decimal zero strings, and empty `ScalarValue::Text` are retained because they are present state values.

#### Narrative Plan and Direction

WriterPlanner Narrative Plan appends only non-empty components:

```text
active_directions: [...]
character_impulses: [...]
world_event_intent_count: <positive integer>
```

Each line is independent. `world_event_intent_count` is omitted when zero and retained when positive. If all three components are empty, the whole section value is `""`.

StoryGenerator Narrative Direction independently appends non-empty `active_goals` and `event_intents`. If both are empty, the whole section value is `""`.

#### Knowledge

StoryGenerator/Repairer Writer Knowledge always renders `entry_id`, `kind`, `scope`, and `content`; `title` does not exist after §3.2.

StoryStateExtractor Knowledge renders `memory_owner` only for `KnowledgeKind::Memory`. The projection MUST enforce:

```text
Memory + owner       -> render memory_owner
Memory + no owner    -> Invariant { code: "modifiable_memory_owner_missing" }
Fact/Rumor + no owner -> omit memory_owner
Fact/Rumor + owner   -> Invariant { code: "modifiable_knowledge_owner_invalid" }
```

Change the helper signature so invalid ownership is rejected before RC rendering:

```rust
fn modifiable_knowledge_view(
    ctx: &TurnExecutionContext,
) -> Result<Vec<StoryStateExtractorKnowledgePromptView>, StoryStateExtractorProjectionError>;
```

#### Character Impulse and Decision

Character Impulse always renders `goal` and `urgency`; `emotion` and `reason` lines render only when present and non-whitespace.

Character Decision always renders `role_id`, `name`, and `decision`; `suggested_utterance` renders only when present and non-whitespace.

#### Validation Issue

Validation Issue always renders Code and Message. Location renders only when present. Item Index renders only when Location exists and `item_index` is present. A missing Location MUST produce no Location line, not `Location: None.`.

### 3.8 Required-Value Validation Protocol

Required values MUST be validated before `render_runtime_vars`:

| Condition | Required error |
|---|---|
| Empty Turn Player Input | existing `TurnRequestError::EmptyPlayerInput` |
| Empty CharacterThink request reason | existing `CharacterThinkProjectionError::InvalidPromptField` |
| Missing/invalid StoryGenerator baseline or plan | existing `MissingBaseline`, `MissingWriterPlan`, or `Invariant` |
| StoryStateExtractor has zero Roles | `StoryStateExtractorProjectionError::Invariant { code: "roles_empty" }` |
| Repairer has no Previous Story or no issues | existing `MissingPreviousStory` or `EmptyValidationIssues` |
| Re-extraction has no Validation Issues | existing `EmptyValidationIssues` |
| Modifiable Knowledge owner violates §3.7 | exact `Invariant` code from §3.7 |

Asset-import and Domain invariants continue to guarantee non-empty required Story Profile, Role identity, story text, and Narrative condition text. A Projector MUST NOT replace any required-value error with an empty variable.

### 3.9 File / Directory Layout

```text
crates/aise/
├── assets/prompts/context-v2/
│   ├── slots.yaml
│   └── rc/
│       ├── writer-planner.md.j2
│       ├── character-think.md.j2
│       ├── story-generator.md.j2
│       ├── story-repairer.md.j2
│       └── story-state-extractor.md.j2
├── src/planning/writer_planner_prompt.rs
├── src/character/character_think_prompt.rs
├── src/story/
│   ├── story_generator_prompt.rs
│   ├── story_repairer_prompt.rs
│   └── story_state_extractor_prompt.rs
├── src/prompt/tests/trusted_prompt_source_tests.rs
├── src/character/tests/character_think_prompt_tests.rs
└── src/story/tests/
    ├── story_generator_prompt_tests.rs
    ├── story_repairer_prompt_tests.rs
    └── story_state_extractor_prompt_tests.rs

crates/aise/tests/prompt_context_contract_tests.rs
```

Update tests only in existing dedicated test files. Do not add code to `mod.rs`/`lib.rs`, inline test modules, comments, dependencies, config, or migrations.

---

## 4. Behavior Rules

1. **RCE-1 — Optional absence**: An empty Optional RC section MUST project as `Value::String("")` and MUST contribute neither heading nor body to the composed RC.
2. **RCE-2 — Parent elision**: A parent section with no rendered child MUST also be omitted; Story Continuity is omitted only when both Summary and Recent Story are empty.
3. **RCE-3 — Field elision**: An empty or inapplicable Optional field inside a non-empty object MUST contribute no line, label, placeholder, or blank child block.
4. **RCE-4 — No sentinel**: Engine-generated RC structure MUST contain no `None.`, `null`, `N/A`, `not available`, empty list, empty map, or zero-count placeholder.
5. **RCE-5 — Required data**: Required sections are unconditional; missing or trim-empty required data MUST stop projection through the typed error in §3.8.
6. **RCE-6 — Completeness**: Character and Knowledge indexes always render `scope`; empty entries omit the `entries` key but not the section.
7. **RCE-7 — Failure distinction**: Retrieval, indexing, Snapshot, validation, and serialization failures MUST propagate as errors and MUST NOT produce the same representation as a genuine empty collection.
8. **RCE-8 — Scalar preservation**: Present integer `0`, boolean `false`, decimal zero, empty scalar text, and relationship trust `0` MUST render exactly as values.
9. **RCE-9 — Derived zero count**: `world_event_intent_count` renders only when positive; no other scalar field is omitted solely because it is zero or false.
10. **RCE-10 — Optional text**: Optional text is absent when its Option is `None` or its value is trim-empty; non-empty text retains its existing quoting and bytes.
11. **RCE-11 — Stable ordering**: Removing empty fields MUST NOT reorder remaining sections, collection items, object fields, indexes, Constraints, Decisions, or Validation Issues.
12. **RCE-12 — Repair context**: StoryRepairer always receives non-empty Previous Story Text and Validation Issues; only optional inherited generation sections and optional issue Location lines may be elided.
13. **RCE-13 — Re-extraction context**: Initial extraction omits retry-only sections; re-extraction renders non-empty Validation Issues and renders Previous Extraction only when available.
14. **RCE-14 — Output separation**: CSI, FTI, output schemas, required output arrays, nullable output fields, and model response validation remain byte-for-byte or semantically unchanged except for unrelated predecessor changes.
15. **RCE-15 — Runtime boundary**: MiniJinja conditions inspect only already-rendered RC strings; inserted user/story/asset text is data and is never recursively evaluated.
16. **RCE-16 — Budget**: Prompt budget calculation MUST use the final elided string values; no omitted heading, body, field, or sentinel contributes tokens.
17. **RCE-17 — Bounded work**: Empty elision adds no LLM call, unbounded scan, retry, queue, task, lock, database access, or dependency.

### 4.1 Error Handling

- Use only the typed errors and exact invariant codes in §3.8; do not add a generic catch-all empty-value fallback.
- `StoryStateExtractorPromptContextProjector::project` MUST apply `?` to `modifiable_knowledge_view` and MUST validate that projected Roles are non-empty before rendering.
- A required renderer receiving an impossible empty value MUST return from its Projector before template composition; it MUST NOT panic or call `unwrap()` on runtime data.
- MiniJinja render failure and slot validation continue through existing `PromptError` paths; template errors are never converted to empty text.

### 4.2 Concurrency

- No asynchronous call, ownership, transaction, limiter, or lock boundary changes.
- Existing LLM calls remain routed through `LlmGateway` and its shared concurrency limiter.
- All empty-elision work remains synchronous and bounded by the already bounded Prompt Context collections.

### 4.3 Observability

- Existing Prompt and LLM spans remain; no new metric is required.
- Existing count/byte/token fields continue to measure typed input collections and final Prompt values as currently defined; they MUST NOT log full prose or Knowledge bodies.
- An enabled Prompt trace MUST show omitted sections as absent, not as empty headings or sentinels.
- Required-value and ownership failures retain structured stage/error/invariant fields through existing tracing paths.

---

## 5. Acceptance Criteria

- [ ] RC renderer source contains no literal absence sentinel — `rg -n '"None\."|"N/A"|"not available"' crates/aise/src/planning/writer_planner_prompt.rs crates/aise/src/character/character_think_prompt.rs crates/aise/src/story/story_generator_prompt.rs crates/aise/src/story/story_repairer_prompt.rs crates/aise/src/story/story_state_extractor_prompt.rs` returns zero matches.
- [ ] RC templates contain no sentinel comparison or unconditional Optional section from §3.3 — verified by `runtime_context_templates_conditionally_elide_optional_sections`.
- [ ] Every final `slots.yaml` RC variable remains a required string and every Projector supplies every key, including empty Optional values — verified by `runtime_context_projectors_preserve_slot_key_sets`.
- [ ] A fully empty Optional fixture composes with no empty headings, `None.`, `null`, empty list/map field, or zero-count placeholder — verified for all five profiles by `runtime_context_empty_optional_fixture_has_no_sentinels`.
- [ ] A populated fixture renders every section in the final predecessor order — verified for all five profiles by `runtime_context_populated_fixture_preserves_section_order`.
- [ ] Story Summary and Recent Story retain raw prose rendering and independent conditional headings — existing Story Context Simplification tests pass.
- [ ] Empty Character and Knowledge indexes render exactly one `scope` line and no `entries` line — verified by `empty_indexes_preserve_scope_without_entries`.
- [ ] Prefiltered empty indexes retain `scope: prefiltered` — verified by `prefiltered_empty_indexes_remain_distinguishable`.
- [ ] Story Profile omits empty genre/themes/tone and retains required language/point-of-view/tense — verified by `story_profile_omits_empty_optional_lists`.
- [ ] Role renderers omit empty goals/attributes/examples and retain location plus present `0`, `false`, decimal zero, empty scalar text, and trust `0` — verified by `role_rendering_elides_empty_collections_but_preserves_scalars`.
- [ ] Narrative Plan and Narrative Direction omit empty component lines and omit the whole section when all components are empty — verified by `narrative_rendering_elides_empty_components`.
- [ ] `world_event_intent_count: 0` is absent and a positive count is present — verified by `narrative_event_count_renders_only_when_positive`.
- [ ] `StoryGeneratorPromptContext.instance_settings` and `StoryGeneratorKnowledgePromptView.entry_id` are non-Optional, and Knowledge `title` is deleted — `rg -n 'instance_settings: Option<StoryGeneratorInstanceSettingsPromptView>|entry_id: Option<KnowledgeSourceId>|title: Option<BoundedText>' crates/aise/src/story crates/aise/src/story/tests` returns zero matches.
- [ ] Empty AI Characters, Constraints, Writer Knowledge, Character Decisions, Character Knowledge, and Impulses omit their complete sections — verified by stage-specific prompt tests.
- [ ] Missing Suggested Utterance, Impulse emotion/reason, and Validation Location produce no corresponding line — verified by `optional_item_fields_are_omitted_without_sentinels`.
- [ ] Fact/Rumor Knowledge omits `memory_owner`; Memory includes it; invalid owner combinations return the exact §3.7 invariant codes — verified by `modifiable_knowledge_owner_contract`.
- [ ] Empty StoryStateExtractor Roles fail with `Invariant { code: "roles_empty" }` — verified by `story_state_extractor_rejects_empty_roles`.
- [ ] Repair and re-extraction still reject empty Validation Issues through existing typed errors — existing `story_repairer` and `story_state_extractor` projection tests pass.
- [ ] Structured output schemas retain their required arrays/nullability and are unaffected by RC elision — verified by existing exact-schema tests for all four LLM output profiles.
- [ ] Instruction-like empty/sentinel text inside Player Input or Story Text remains literal RC data and never enters CSI/FTI — verified by `authored_sentinel_text_is_not_structurally_elided_or_promoted`.
- [ ] Prompt budget tests use elided values and still reject required-data overflow — existing budget tests plus `empty_elision_reduces_runtime_context_tokens` pass.
- [ ] Prompt loading, slot validation, composition, engine flow, CharacterThink, generation, repair, extraction, and trace-contract tests pass — `cargo test --workspace` passes.
- [ ] Formatting and linting pass — `cargo fmt --all --check` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` pass.

---

## 6. Out of Scope / Future Work

- A future typed section builder may replace repeated MiniJinja conditions only if it preserves this exact semantic contract.
- Prompt token-efficiency measurement across production traces may be added separately; this spec requires deterministic structural tests, not a target percentage reduction.

---

## 7. References

- Source design: [Runtime Context Empty Elision](../design/2026-08-17-runtime-context-empty-elision-design-gpt.md)
- Required predecessor: [Current Scene Removal Spec](2026-08-17-current-scene-removal-spec-gpt.md)
- Required predecessor: [Story Context Simplification Spec](2026-08-17-story-context-simplification-spec-gpt.md)
- Prompt framework: [CSI-RC-FTI Prompt Framework](CSI-RC-FTI/2026-08-11-csi-rc-fti-prompt-spec-gpt.md)
- Guardrails: [`doc/agents/guardrails/`](../agents/guardrails/)
