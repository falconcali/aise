# Current Scene Removal — Spec

> **Model**: GPT-5
> **Date**: 2026-08-17
> **Status**: Proposed
> **Source Design**: [Current Scene Removal](../design/2026-08-17-current-scene-removal-design-gpt.md)
> **Phase**: N/A

---

## 1. Goal

Remove runtime `CurrentScene` end to end and make `StoryContinuity`, `StoryRoleState`, and bounded relevance selection the only sources for narrative continuity, role state, and prompt context.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Delete `CurrentScene` from the runtime domain, snapshots, baseline context, extraction output, validation, commit, persistence, API, UI, prompt contracts, configuration, and tests.
- Remove `scene_roles`, `referenced_roles`, `CharacterPresence`, and all authoritative scene/off-scene labels; replace them with one bounded `relevant_roles` projection.
- Remove `current_scene` from WriterPlanner, CharacterThink, StoryGenerator, StoryRepairer, and StoryStateExtractor CSI-RC-FTI contracts.
- Remove `current_scene` from the StoryStateExtractor JSON schema and from `ValidatedChangeSet`.
- Replace scene-derived retrieval signals with Player Input, player Role/Location state, and bounded Recent Story signals.
- Add the SQLite migration and breaking Story API/UI changes required by the removal.

### 2.2 Non-Goals

- Does not change the `StoryStart` Story Pack asset contract; its static `scene_key`, `location_key`, `time`, `description`, and `opening` fields remain unchanged.
- Does not add a replacement scene summarizer, perception object, scene cache, compatibility field, or derived `current_scene` API value.
- Does not change Story Summary generation, Recent Story retention, Story Role state semantics, Knowledge semantics, or Narrative Graph evaluation.
- Does not infer or persist exact camera presence for a Role.
- Does not preserve backward compatibility for config keys, Rust APIs, JSON extraction output, or HTTP response fields removed by this spec.

### 2.3 Implementation Constraints (for code generation)

- This spec generates final-form code. Do **not** keep fallback paths, compatibility shims, deprecated aliases, optional legacy fields, or dual-write logic.
- Old types, functions, modules, prompt variables, config keys, API fields, tests, and UI elements superseded by this spec MUST be deleted, not deprecated.
- No mid-state in which `CurrentScene` and continuity-derived context coexist is allowed.
- Historical migrations remain immutable. Migration `0005_authoritative_story_state.sql` MUST remain unchanged; a new migration removes the column.
- `R-ARCH-01`, `R-ARCH-03/04`, `R-REFACTOR-01/02`, `R-CODE-01/02/05/07`, `R-LAYER-01`, and `R-AISE-01/02/03` remain mandatory.

---

## 3. Contracts

### 3.1 Runtime Types

Delete `CurrentScene` and its public re-exports from:

```rust
crate::domain::story_instance::state::CurrentScene
crate::domain::story_instance::CurrentScene
crate::domain::CurrentScene
```

`StoryReadSnapshot` and its construction contract MUST have no scene field or accessor:

```rust
pub struct StoryReadSnapshotParts {
    pub story_id: StoryId,
    pub base_revision: StoryRevision,
    pub pack: FrozenStoryPackRef,
    pub story_profile: StoryProfile,
    pub instance_settings: InstanceSettings,
    pub roles: BTreeMap<RoleId, StoryRoleView>,
    pub relationships: Vec<RelationshipState>,
    pub narrative_definition: NarrativeGraphDefinition,
    pub narrative_state: NarrativeRuntimeState,
    pub fact_values: BTreeMap<FactKey, ScalarValue>,
    pub story_continuity: StoryContinuity,
    pub active_constraints: Vec<ActiveStoryConstraint>,
    pub entity_catalog: Vec<KnowledgeEntity>,
    pub topic_dictionary: BTreeMap<TopicKey, TopicDefinition>,
    pub knowledge_snapshot: KnowledgeSnapshotRef,
}
```

`BaselineContext` MUST use a single relevant-role collection:

```rust
pub struct BaselineContext {
    pub story_profile: StoryProfile,
    pub instance_settings: InstanceSettings,
    pub player_role: RoleContextView,
    pub relevant_roles: Vec<RoleContextView>,
    pub relevant_knowledge: Vec<RelevantKnowledge>,
    pub role_index_scope: RetrievalIndexScope,
    pub knowledge_entry_index_scope: RetrievalIndexScope,
    pub knowledge_entry_index: Vec<KnowledgeEntryIndexEntry>,
    pub role_index: Vec<RoleIndexEntry>,
    pub story_continuity: StoryContinuity,
    pub active_story_constraints: Vec<ActiveStoryConstraint>,
    pub narrative_graph_state_index: NarrativeGraphStateIndex,
    pub retrieval_signals: RetrievalSignals,
}
```

`RetrievalSignals` MUST contain no cached scene identity or presence list:

```rust
#[derive(Debug, Clone, Serialize, Default)]
pub struct RetrievalSignals {
    pub entities: Vec<EntitySignal>,
    pub topics: Vec<TopicSignal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalSignalOrigin {
    PlayerInput,
    RoleState,
    Narrative,
    RecentStory,
    Summary,
}
```

`StoryStateExtractorOutput` MUST contain only independently owned mutable state:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryStateExtractorOutput {
    pub role_states: Vec<ExtractedRoleState>,
    pub relationship_states: Vec<RelationshipState>,
    pub knowledge_changes: Vec<ProposedKnowledgeMutation>,
}
```

`ValidatedChangeSet` and `ValidatedChangeSetParts` MUST contain:

```rust
pub struct ValidatedChangeSetParts {
    pub story_text: BoundedText,
    pub role_changes: Vec<RoleStateChange>,
    pub relationship_changes: Vec<RelationshipStateChange>,
    pub knowledge_mutations: Vec<ValidatedKnowledgeMutation>,
    pub narrative_events: Vec<StoryEvent>,
    pub narrative_resolution: ValidatedNarrativeResolution,
    pub constraint_change: StateChange<Vec<ActiveStoryConstraint>>,
}
```

`MaterializedStoryInstanceSpec` MUST delete `scene: CurrentScene`; all other existing fields remain unchanged.

Delete these prompt-only types and variants:

```rust
StoryGeneratorScenePromptView
CharacterThinkScenePromptView
CharacterPresence
CharacterThinkProjectionError::RoleNotPresent
```

`StoryGeneratorRolePromptView` MUST have no `presence` field.

### 3.2 Selection and Validation Functions

Relevant Role selection MUST use this contract:

```rust
fn select_relevant_roles(
    snapshot: &StoryReadSnapshot,
    signals: &RetrievalSignals,
    max_relevant_roles: usize,
) -> Vec<RoleContextView>;
```

The function MUST:

1. Ignore the Player Role, because it is projected separately.
2. Select only Role IDs present in `signals.entities` as `KnowledgeEntity::Role`.
3. Rank each Role by its minimum matching `EntitySignal.priority`, then by `role_id` ascending.
4. Deduplicate by exact `role_id`.
5. Return at most `max_relevant_roles`; unselected Roles remain eligible for `role_index`.

`RetrievalSignalBuilder::build` keeps its public signature:

```rust
pub fn build(
    &self,
    snapshot: &StoryReadSnapshot,
    player_input: &str,
) -> Result<RetrievalSignals, ContextError>;
```

It MUST emit signals in this precedence order:

```text
priority 0: Player Input text matches
priority 1: Player Role entity and Player Role location entity
priority 2 + recency_rank: Recent Story text matches, newest segment first and recency_rank starting at 0
```

The builder scans at most `recent_segments_for_signals` segments. It MUST convert `2 + recency_rank` to `u8` with checked conversion and return `ContextError::SignalLimitExceeded { limit: "recent_segments_for_signals" }` if the configured value cannot be represented. It MUST NOT emit `RetrievalSignalOrigin::Scene`, a Scene entity from cached state, or a present-Role list.

Character Think request validation MUST receive the snapshot and accept any existing AI-controlled Role:

```rust
fn validate_think_requests(
    &self,
    requests: Vec<CharacterThinkRequest>,
    snapshot: &StoryReadSnapshot,
) -> Result<Vec<CharacterThinkRequest>, PlanningError>;
```

`DefaultCharacterThinkPromptContextProjector::project` MUST resolve the target with `ctx.snapshot().role(&request.role_id)`. It MUST reject an unknown Role and a Player-controlled Role, but MUST NOT require the Role to be in `relevant_roles` or to satisfy any presence predicate.

### 3.3 Prompt Slot Protocol

`crates/aise/assets/prompts/context-v2/slots.yaml` MUST expose these RC variables:

| Slot | Required variables after this change |
|---|---|
| `context.writer_planner.rc` | `story_profile`, `instance_settings`, `story_summary`, `recent_story`, `player_character`, `relevant_characters`, `relevant_knowledge`, `character_index`, `knowledge_entry_index`, `narrative_plan`, `active_story_constraints`, `player_input` |
| `context.character_think.rc` | `target_character`, `current_character_state`, `story_summary`, `recent_story`, `relevant_character_knowledge`, `narrative_character_impulses`, `thinking_focus`, `player_input` |
| `context.story_generator.rc` | `story_profile`, `instance_settings`, `story_summary`, `recent_story`, `player_character`, `ai_characters`, `active_story_constraints`, `story_goal`, `narrative_direction`, `relevant_writer_knowledge`, `character_decisions`, `player_input` |
| `context.story_repairer.rc` | StoryGenerator variables plus `previous_story_text`, `validation_issues` |
| `context.story_state_extractor.rc` | `story_text`, `roles`, `relationships`, `modifiable_knowledge`, `condition_queries`, `previous_extraction`, `validation_issues` |

No CSI, RC, FTI, prompt variable, rendered heading, or output instruction may contain `Current Scene`, `current_scene`, `Pre-turn Current Scene`, `scene_characters`, or `referenced_characters`.

WriterPlanner RC MUST replace the two character blocks with:

```markdown
## Relevant Characters

{{ relevant_characters }}
```

Relevant Character and AI Character rendering MUST omit any `presence` field. The model infers presence from Story Continuity.

### 3.4 Story State Extractor Protocol

The exact top-level structured output remains an envelope, but `state` has only three fields:

```json
{
  "state": {
    "role_states": [],
    "relationship_states": [],
    "knowledge_changes": []
  },
  "narrative_condition_judgments": []
}
```

The JSON Schema MUST set `additionalProperties: false` and require exactly:

```json
{
  "required": ["role_states", "relationship_states", "knowledge_changes"]
}
```

An extractor response containing `current_scene` MUST fail deserialization or schema validation and enter the existing bounded re-extraction path.

### 3.5 Persistence Protocol

Add exactly one migration:

```text
crates/aise/assets/persistence/mig/0018_drop_current_scene.sql
```

with:

```sql
ALTER TABLE stories DROP COLUMN current_scene;
```

Fresh and upgraded databases MUST end with no `stories.current_scene` column. `SqliteStore` MUST:

- omit `current_scene` from `INSERT INTO stories`;
- omit scene serialization during story creation;
- omit scene updates during Turn commit;
- omit scene selection, deserialization, byte limits, and Role-count limits during snapshot loading.

Historical migration `0005_authoritative_story_state.sql` MUST NOT be edited.

### 3.6 Config Protocol

Delete:

```rust
TurnContentLimitsConfig::max_scene_bytes
SnapshotLimits::max_scene_bytes
ContextPreparationConfig::max_scene_roles
SnapshotLimits::max_scene_roles
```

Add the hard replacement:

```rust
pub struct ContextPreparationConfig {
    pub max_relevant_roles: usize,
}
```

`context.max_relevant_roles` MUST be positive and MUST be less than or equal to `content.max_roles`. Delete `content.max_scene_bytes` and replace `context.max_scene_roles` with `context.max_relevant_roles` in `config/aise_config.toml`.

### 3.7 HTTP and UI Protocol

The final server response types are:

```rust
#[derive(Debug, Serialize)]
pub struct StoryInstanceView {
    pub story_id: String,
    pub base_revision: u64,
    pub pack_id: String,
    pub player_role_id: String,
    pub opening: StoryOpeningView,
}

#[derive(Debug, Serialize)]
pub struct StoryView {
    pub story_id: String,
    pub base_revision: u64,
    pub premise: String,
    pub player_role_id: String,
    pub opening: Option<StoryOpeningView>,
    pub turns: Vec<StoryTurnView>,
    pub next_turn_after: Option<u64>,
    pub roles: Vec<RoleStateView>,
}
```

The built-in web app MUST delete the Current Scene container, label, and `story.current_scene` rendering branch. No replacement summary is added.

### 3.8 File / Directory Layout

```text
crates/aise/
├── assets/persistence/mig/0018_drop_current_scene.sql
├── assets/prompts/context-v2/{slots.yaml,csi,rc,fti}
├── src/config/{aise.rs,content.rs,context.rs}
├── src/context/{baseline_ctx_builder.rs,retrieval_signal_builder.rs}
├── src/domain/story_instance/{mod.rs,snapshot.rs,state.rs}
├── src/domain/turn/{baseline.rs,extraction.rs,retrieval.rs}
├── src/planning/{retrieval_plan_builder.rs,writer_planner_prompt.rs}
├── src/character/character_think_prompt.rs
├── src/story/{instance_factory.rs,story_generator_prompt.rs,story_repairer_prompt.rs,story_state_extractor_prompt.rs}
├── src/turn/turn_validation.rs
├── src/validation/{validation_pipeline.rs,validators/}
└── src/persistence/{store.rs,sqlite_snapshot.rs,sqlite_store.rs}

crates/aise-server/
├── src/api/story.rs
├── assets/app.js
└── tests/story_api_tests.rs

config/aise_config.toml
```

Tests colocated under existing `tests/` directories and crate integration-test directories MUST be updated in the same change.

---

## 4. Behavior Rules

1. **CSR-1 — Narrative authority**: WriterPlanner, CharacterThink, StoryGenerator, and StoryRepairer MUST derive the current narrative situation from `Story Summary + Recent Story`; newer Recent Story text wins over older Summary text.
2. **CSR-2 — Runtime deletion**: No runtime object, snapshot, baseline, extraction, validation result, change set, persistence record, API response, or UI model may carry `CurrentScene` or `current_scene`.
3. **CSR-3 — No replacement cache**: The implementation MUST NOT introduce `SceneState`, `SceneSnapshot`, `CurrentSituation`, `PresentRoles`, or an equivalent aggregate that recreates the deleted contract.
4. **CSR-4 — Role ownership**: A Role's exact `location`, `goals`, and `attributes` remain owned exclusively by `StoryRoleState` and update only through validated `role_states` extraction.
5. **CSR-5 — Relevant Roles**: Baseline preparation MUST select `relevant_roles` only from bounded retrieval signals and MUST NOT label them as present, absent, scene, or off-scene.
6. **CSR-6 — Role index**: Every non-player Role not selected into `relevant_roles` remains eligible for `role_index`, subject to the existing index bound and scope semantics.
7. **CSR-7 — Character Think**: A Character Think request is valid for any exact existing AI-controlled `role_id`; scene presence is neither required nor represented.
8. **CSR-8 — Generator characters**: StoryGenerator AI Characters MUST be the deduplicated, Role-ID-sorted, bounded union of Baseline Relevant Roles, Character Think targets, and Narrative Character Impulse targets resolved from the Snapshot.
9. **CSR-9 — Retrieval**: Player Input has priority `0`; Player Role and its Location have priority `1`; Recent Story matches follow in newest-first order. Signal collection MUST remain bounded by existing entity/topic limits.
10. **CSR-10 — Extractor**: StoryStateExtractor MUST emit only changed Role states, changed Relationship states, Knowledge mutations, and required Narrative Condition judgments.
11. **CSR-11 — Validation**: Deterministic validators MUST delete all scene-specific schema, invariant, and reference checks. Role locations resolve against known Location entities or locations already held by existing Roles; `KnowledgeEntity::Scene` resolves only through `entity_catalog`.
12. **CSR-12 — Commit**: A successful Turn commit writes Story Text, validated Role/Relationship/Knowledge/Narrative/Constraint changes, but never writes a derived scene value.
13. **CSR-13 — Prompts**: CSI and FTI wording MUST refer to committed Story Continuity and hard constraints, never to an authoritative Current Scene.
14. **CSR-14 — Repair**: StoryRepairer inherits the scene-free StoryGenerator context and uses Previous Story Text plus Validation Issues; it MUST NOT receive a scene value through a repair-only path.
15. **CSR-15 — Migration**: Database upgrades drop the obsolete column once; no code reads it before or after commit, and no backfill is performed.
16. **CSR-16 — API break**: HTTP responses omit `current_scene`; clients must use `opening`, `turns`, and Role states. No null, empty, computed, or deprecated compatibility field is returned.
17. **CSR-17 — Static StoryStart**: `StoryStart` remains unchanged and MUST NOT be copied into mutable runtime scene state.
18. **CSR-18 — Bounded work**: The change adds no LLM call, unbounded Role scan, unbounded history scan, queue, task, lock, or dependency.

### 4.1 Error Handling

- `DefaultCharacterThinkPromptContextProjector` returns `CharacterThinkProjectionError::UnknownRole` for an unknown Role and `PlayerControlledRole` for the Player Role; `RoleNotPresent` is deleted.
- `RetrievalPlanBuilder` returns `PlanningError::UnknownRole` when a Think target does not resolve to an existing AI-controlled Snapshot Role.
- Config validation returns the existing typed `ConfigError::Invalid` with exact messages `context.max_relevant_roles must be positive` and `context.max_relevant_roles must be <= content.max_roles`.
- Migration, serialization, and Store failures continue through existing typed errors; no scene-removal path may ignore an error or call `unwrap()` on external data.

### 4.2 Concurrency

- No new asynchronous work or LLM call is introduced.
- Existing LLM calls remain routed through `LlmGateway` and its shared limiter.
- Snapshot reads and commits retain their existing transaction and lock boundaries.

### 4.3 Observability

- Existing `context.prepare` tracing MUST report `relevant_role_count`; delete scene-role accounting.
- Prompt trace payloads MUST contain no `current_scene` variable or rendered Current Scene heading.
- Existing LLM, validation, migration, and commit spans remain unchanged apart from removed scene fields; no new metric is required.

---

## 5. Acceptance Criteria

- [ ] `CurrentScene` and all re-exports are deleted — `rg -n 'CurrentScene|current_scene|Current Scene|Pre-turn Current Scene' crates/aise/src crates/aise/tests crates/aise/assets/prompts crates/aise-server/src crates/aise-server/tests crates/aise-server/assets config examples` returns zero matches.
- [ ] Scene-presence abstractions are deleted — `rg -n 'scene_roles|scene_characters|referenced_roles|referenced_characters|CharacterPresence|RoleNotPresent' crates/aise/src crates/aise/tests crates/aise/assets/prompts` returns zero matches.
- [ ] Obsolete limits are deleted — `rg -n 'max_scene_bytes|max_scene_roles' crates/aise/src crates/aise/tests crates/aise-server config` returns zero matches.
- [ ] WriterPlanner renders exactly one `## Relevant Characters` block and no scene/off-scene presence field — verified by `writer_planner_prompt_uses_relevant_characters_without_presence`.
- [ ] CharacterThink accepts an existing AI Role not selected into Baseline Relevant Roles — verified by `character_think_allows_existing_ai_role_without_presence_state`.
- [ ] CharacterThink still rejects unknown and Player-controlled Roles — verified by `character_think_rejects_unknown_role` and `character_think_rejects_player_role`.
- [ ] StoryGenerator AI Characters include relevant, Think-targeted, and impulse-targeted Roles once each in Role-ID order — verified by `story_generator_unions_relevant_and_requested_ai_roles`.
- [ ] `RetrievalSignals` contains only `entities` and `topics`, and player Location is emitted with `RoleState` origin and priority `1` — verified by `retrieval_signals_use_role_state_without_scene_cache`.
- [ ] `StoryStateExtractorOutput::json_schema` requires only `role_states`, `relationship_states`, and `knowledge_changes` — verified by `extractor_schema_has_no_current_scene`.
- [ ] Extractor JSON containing `current_scene` is rejected — verified by `extractor_rejects_removed_current_scene_field`.
- [ ] `ValidatedChangeSet` carries no scene field and Turn commit updates no scene column — verified by `validated_change_set_has_no_scene_contract` and `commit_turn_does_not_write_current_scene`.
- [ ] Migration `0018_drop_current_scene.sql` exists and upgraded/fresh schemas have no `stories.current_scene` column — verified by `persistence_migration_drops_current_scene` using `PRAGMA table_info(stories)`.
- [ ] `StoryInstanceView` and `StoryView` serialize without `current_scene` — verified by `story_api_omits_current_scene`.
- [ ] The built-in web app contains no Current Scene panel or `story.current_scene` access — `rg -n 'current_scene|当前场景|scene-label' crates/aise-server/assets` returns zero matches.
- [ ] `config/aise_config.toml` contains `max_relevant_roles` and no obsolete scene limits — verified by the zero-match checks above and config validation tests.
- [ ] All prompt templates render with the updated slot manifest — `cargo test -p aise prompt` passes.
- [ ] Fresh database, upgrade migration, engine flow, Story Pack runtime, persistence, and API integration tests pass — `cargo test --workspace` passes.
- [ ] Formatting and linting pass — `cargo fmt --all --check` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` pass.

---

## 6. Out of Scope / Future Work

- Removing static scene metadata from `StoryStart` requires a separate Story Pack asset-version design and migration.
- Improving Story Summary generation or dynamically resizing the Recent Story window requires a separate continuity spec.
- Adding UI-generated scene summaries is explicitly deferred and must not reuse a persisted runtime `CurrentScene` contract.

---

## 7. References

- Source design: [Current Scene Removal](../design/2026-08-17-current-scene-removal-design-gpt.md)
- Prior context design: [Context Preparation and Retrieval](../design/2026-08-08-context-preparation-retrieval-design-gpt.md)
- Prior extractor design: [Story State Extractor Split](../design/CSI-RC-FTI/2026-08-14-story-state-extractor-split-design-gpt.md)
- Prior Character Think design: [Character Think Decision](../design/CSI-RC-FTI/2026-08-14-character-think-decision-design-gpt.md)
- Guardrails: [`doc/agents/guardrails/`](../agents/guardrails/)
