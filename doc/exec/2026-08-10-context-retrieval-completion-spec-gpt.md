# Context Retrieval Completion — Spec

> **Model**: GPT-5.6
> **Date**: 2026-08-10
> **Status**: Proposed
> **Source Design**: [Context Preparation and Retrieval Design](../design/2026-08-08-context-preparation-retrieval-design-gpt.md)
> **Phase**: Corrective completion after `e549b5d`

---

## 1. Goal

Close the remaining migration, Narrative Graph, retrieval authorization, bounded-read, validation-authority, and layer-boundary gaps so the Context Preparation and Retrieval path is safe, deterministic, and mechanically covered end to end.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Stop an unreadable legacy Story Instance before any destructive migration and verify the final schema after migration.
- Make every Narrative transition persistent, emit activation/completion effects exactly once, and materialize every supported condition input.
- Make Story Pack validation typed, reference-complete, bounded, and identical to the pre-storage import decision.
- Close unknown Role/Character, Character audience, off-scene Character Think, and missing Narrative retrieval-signal paths.
- Check SQLite byte/count/revision limits before fetching or decoding large bodies and remove lossy integer conversions.
- Seal `ValidatedChangeSet`, strictly bound every nested model-output field, and preserve typed prompt/context failures.
- Restore the documented `config` leaf and index-only `mod.rs` boundaries.
- Implement the missing acceptance cases from the prior hardening spec plus the corrective regression cases in this spec.

### 2.2 Non-Goals

- Does not add a typed player-action event protocol; `PlayerActionOccurred` remains forbidden at Pack import.
- Does not add full-text, BM25, embedding, or zero-result full-table retrieval fallback.
- Does not synthesize missing player IDs, Character digests, Role bindings, or other unrecoverable legacy metadata.
- Does not add concurrent retrieval or Character Think fan-out.
- Does not redesign the HTTP Story API or Prompt authoring format beyond strictness required by this spec.
- Does not complete `.aisepack` binary-asset persistence or export semantics.

### 2.3 Implementation Constraints (for code generation)

- Execute against `origin/main` baseline `e549b5d`. Reconcile the current incomplete local edit in `story/instance_factory.rs` before implementation; seed knowledge materialization, Character freezing, and checked limit helpers must remain present in final form.
- This spec generates final-form code. Do **not** keep fallback paths, compatibility shims, dual reads, or dual writes.
- Do **not** edit checksums or contents of migrations `0001` through `0011`; add `0012_context_retrieval_completion.sql` and production pre/post-migration checks.
- Old helpers, public constructors, placeholder defaults, and bypass implementations superseded by this spec MUST be deleted.
- Every negative path returns a typed stable code; it never warns and continues, fabricates a value, filters the invalid item, or converts it with a lossy cast.
- Tests MUST invoke production validation, migration, planning, retrieval, Pipeline, Store, or HTTP paths. A manually assembled expected DTO is not acceptance coverage.

---

## 3. Contracts

### 3.1 Migration Integrity Contract

Add the following persistence-owned contract:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationIntegrityCode {
    LegacyProjectionOversized,
    LegacyInstanceUnreadable,
    LegacyBindingIncomplete,
    LegacyPlayerBindingInvalid,
    FinalForeignKeyViolation,
    FinalInstanceUnreadable,
    FinalRevisionInvalid,
}

pub enum StoreError {
    // existing variants
    MigrationIntegrity { code: MigrationIntegrityCode },
}

pub(crate) async fn preflight_pending_context_migration(
    pool: &sqlx::SqlitePool,
) -> Result<(), StoreError>;

pub(crate) async fn verify_final_context_integrity(
    pool: &sqlx::SqlitePool,
) -> Result<(), StoreError>;
```

`SqliteStore::connect` executes exactly this order:

```text
open pool with foreign_keys = ON
-> preflight_pending_context_migration
-> sqlx migrator through latest version
-> verify_final_context_integrity
-> expose SqliteStore
```

For a database whose latest applied version is lower than `11`, preflight validates each legacy Story Instance before `0010` or `0011` can run:

1. Projection byte sizes are read with `length(CAST(column AS BLOB))` and compared with migration-only hard caps before JSON bodies are fetched.
2. `bindings_json`, `characters_json`, Pack roles, resolved Character cards, `stories.player_character_id`, scene, summary, constraints, and Narrative state deserialize without defaults.
3. Every binding map key equals `RoleBinding.role_key`; every binding has a non-empty Character ID, pinned Character key/version/digest, controller, and `bound_at_ms`.
4. Binding, Role definition, Character state, and resolved Character-card key sets match exactly.
5. Exactly one binding is player-controlled and its Character ID equals `stories.player_character_id`.
6. The bound Character key and version equal the resolved Character card. No digest or player ID is invented when absent.
7. Any failure returns `StoreError::MigrationIntegrity` before the SQL migrator is invoked.

`0012_context_retrieval_completion.sql` is additive and uses a temporary `CHECK (value = 0)` guard table. It fails if any of the following is true:

- `pragma_foreign_key_check` returns a row.
- A final Story Instance projection is invalid JSON or lacks the required top-level shape.
- A binding has a missing required field or its JSON map key differs from `role_key`.
- A Story has anything other than one player-controlled binding.
- A persisted revision or sequence is negative; a `story_turns.sequence` is zero or null.
- A `knowledge_entries.source_revision` is greater than the owning Story revision.

The Rust post-migration verifier repeats semantic checks that SQLite cannot express, including typed deserialization, Character-card version/digest consistency, exact key-set equality, and a readable `StoryReadSnapshot` for every migrated Story under migration verification limits.

The positive `0008` fixture contains a complete runtime-valid Story Instance. The existing minimal fixture with only `{"character_id":"char-1"}` becomes a negative fixture and must fail before `0010` drops legacy columns.

### 3.2 Narrative Projection and Effect Contract

`NarrativeDirector::evaluate` derives a projected state without mutating the committed Snapshot:

```rust
pub struct NarrativePlan {
    pub active_nodes: Vec<NarrativeNodeKey>,
    pub active_goals: Vec<StoryGoal>,
    pub global_event_intents: Vec<GlobalEventIntent>,
    pub character_impulses: Vec<CharacterImpulse>,
    pub proposed_transitions: Vec<ProposedNarrativeTransition>,
    pub effect_dispositions: Vec<NarrativeEffectDisposition>,
}
```

The evaluation algorithm is exact:

1. All conditions read the committed Snapshot and committed `NarrativeRuntimeState`; a transition proposed in this call cannot trigger a second-hop condition in the same call.
2. An inactive node whose `activate_when` matches proposes `Inactive -> Active`.
3. A matching edge whose committed source is `Active` and committed target is `Inactive` proposes `Inactive -> Active` for the target. Merely appending the target to `active_nodes` is forbidden.
4. Multiple matching causes for the same target collapse to one deterministic transition.
5. An active node whose `complete_when` matches proposes `Active -> Completed`; otherwise a matching `skip_when` proposes `Active -> Skipped`.
6. Each node has at most one transition per evaluation. Completion has precedence over skip; a conflicting transition is an invariant error.
7. `active_nodes` and `active_goals` are rebuilt from projected state and include every node that remains or becomes `Active`; they exclude nodes projected to `Completed` or `Skipped`.
8. `on_activate` effects are emitted only for an `Inactive -> Active` transition. `on_complete` effects are emitted only for an `Active -> Completed` transition. Skip emits no effect.
9. Effects are ordered by node key, transition kind, and definition order. They never repeat while a node stays in the same state.
10. A missing Role, Character, Fact, node, event, or location reference is `NarrativeError::MissingReference`; it is never treated as AI-controlled and never skipped.
11. `max_effects_per_node`, total condition-node count, child count, and depth are checked defensively before emission/evaluation.
12. Commit applies all validated Narrative transitions once and increments `graph_revision` exactly once when the transition list is non-empty.

Materialize supported condition state at instance creation:

```rust
NarrativeConditionStateView {
    occurred_event_keys: BTreeSet::new(),
    player_action_event_keys: BTreeSet::new(),
    fact_values: resolved_world_book
        .facts
        .iter()
        .filter_map(|(key, seed)| seed.proposition.as_ref().map(|p| (key.clone(), p.value.clone())))
        .collect(),
}
```

Pack validation accepts `FactStateEquals` only when the referenced Fact exists and has a proposition. Committed unkeyed Facts do not mutate `fact_values`. `PlayerActionOccurred` is rejected with `AssetValidationCode::GraphConditionForbidden` until a typed player-action protocol exists.

### 3.3 Typed Story Pack Validation Contract

JSON validation and import use one shared typed path:

```rust
pub(crate) struct ValidatedPackCandidate {
    pub pack: StoryPack,
    pub canonical_manifest: Vec<u8>,
    pub resolved_characters: BTreeMap<CharacterAssetKey, CharacterCard>,
    pub resolved_world_book: WorldBook,
}

fn validate_manifest(
    bytes: &[u8],
    limits: &AssetLimitsConfig,
) -> Result<ValidatedPackCandidate, ValidationReport>;
```

`NativeAssetImporter::parse(AssetInput::Json)` returns `valid = true` if and only if `validate_manifest` succeeds. `PackService::import` consumes the returned candidate and does not run a weaker precheck followed by a stronger unrelated failure.

Validation performs typed `StoryPack` deserialization before semantic checks and enforces all of the following:

- Exactly one default cast per Role; no cast for an unknown Role; every cast resolves to a Character card.
- Every playable Role exists, and the Story Pack has exactly one non-empty Story Opening independent of player-role selection.
- Embedded Character map keys equal card keys; version and digest pinning is deterministic.
- Topic labels and aliases are non-empty after normalization, byte bounded, and collision-free.
- Fact, Rumor, and seed Memory Topic references resolve; entry Entity/Topic counts are bounded.
- Pack-authored knowledge contains no `KnowledgeEntity::Character`.
- Relationship target Roles resolve and `(source_role, target_role, kind)` tuples are unique.
- Memory keys are unique within each Role.
- Constraint Role, scene, and Narrative-node references resolve.
- Narrative entry nodes, edges, `NodeState`, `FactStateEquals`, `CharacterStateEquals`, `RelationshipReaches`, `RoleControllerIs`, Global Event participants/location, and Character Impulse target Roles resolve.
- Condition depth/count and effects-per-node limits are enforced over the typed recursive AST.
- `PlayerActionOccurred` is rejected in every nested condition position.
- Every text/key/list/map limit in `AssetLimitsConfig` is enforced after typed decoding.
- Validation issue count is capped at `max_validation_issues` in deterministic path order.

A Pack reported as valid must not later fail import because its final schema, default cast, World Book, Character dependency, Topic dictionary, or reference graph is invalid.

### 3.4 Retrieval Planning and Character Think Contract

Expose shared Domain resolution instead of copying pipeline-specific lookup rules:

```rust
impl StoryReadSnapshot {
    pub fn contains_entity(&self, entity: &KnowledgeEntity) -> bool;
}

impl CharacterView {
    pub fn try_from_snapshot(
        snapshot: &StoryReadSnapshot,
        character_id: &CharacterId,
    ) -> Result<Self, StorySnapshotError>;
}
```

`contains_entity` resolves Role through `role_bindings`, Character through `character_states`, and other variants through the bounded `entity_catalog`. Planner validation has no Role/Character exception.

Character Think rules are exact:

1. `validate_think_requests` validates against the Snapshot, not only the scene list.
2. A request is accepted only for a known AI-controlled binding.
3. Scene and off-scene Characters resolve through `CharacterView::try_from_snapshot` to the same full Role/binding/card/state shape.
4. `CharacterThinkPipeline` returns a typed invariant for any unresolved or player-controlled accepted request. It contains no warning-and-`continue` path.
5. A Character-audience retrieval request is allowed only when its `character_id` has an exact accepted `CharacterThinkRequest` in the same `WriterPlan`.
6. Character Memory authorization contains exactly that audience Character. Writer Memory authorization contains exactly the Character entities in that individual request, and every owner must be an accepted think request.
7. Unknown Role, Character, Topic, or other Entity keys fail before any `KnowledgeReadPort` call.

`narrative_requests` emits deterministic Global Writer requests for every typed reference in the `NarrativePlan`:

- active/new Narrative node keys;
- Global Event key, participants, and optional location;
- Character Impulse target Role and resolved target Character ID.

Expansion, deduplication, and final Entity/Topic/request bounds run after all Narrative and query-derived signals are added.

### 3.5 SQLite Read and Revision Contract

All SQLite text/blob bounds use UTF-8 byte length:

```sql
length(CAST(column_name AS BLOB))
```

Snapshot, knowledge, and Story-history readers use two phases inside one short read transaction:

```text
bounded metadata/max+1 query
-> validate count, byte length, signed integers, revision, and authorization
-> fetch only approved body rows
-> typed decode
-> commit read transaction
```

Required behavior:

- Snapshot projection checks use BLOB byte lengths for every JSON/text projection.
- Recent continuity metadata includes `id`, `sequence`, and Story-text byte length; oversized text fails before `story_text` is selected.
- Story history metadata includes page `max + 1`, IDs, sequences, timestamps, and both text byte lengths; bodies are fetched only after validation.
- Remove `StoryHistoryReadPort` implementations on raw `SqliteStore` and `Arc<SqliteStore>` that silently use defaults. Only configured `SqliteStoryHistoryReader` implements the port.
- Before the knowledge body query, reject any entry for the Story with `source_revision < 0` or `source_revision > snapshot.base_revision`; current Story revision and Pack digest must still equal the Snapshot.
- Before selecting `content`, `source_json`, or `payload_json`, run the fully authorized and selector-scoped metadata query and reject any matching row above its byte limit.
- Authorization and owner predicates remain before ordering and limiting.
- Every SQLite `i64 -> u64` and Domain `u64 -> i64` conversion uses `try_from`. No `as i64` or `as u64` remains in persistence boundary code.
- Commit rejects revision/sequence overflow before executing a write and leaves the transaction unchanged.

### 3.6 Strict Model Output and Sealed Commit Contract

`ValidatedChangeSet` stays publicly readable but is not publicly constructible:

```rust
pub struct ValidatedChangeSet {
    // private fields
}

pub(crate) struct ValidatedChangeSetParts {
    // crate-private fields
}

impl ValidatedChangeSet {
    pub(crate) fn from_validated_parts(
        parts: ValidatedChangeSetParts,
    ) -> Result<Self, TurnExecutionError>;
}
```

Production source contains exactly one call to `from_validated_parts`, in `validation/validation_pipeline.rs`. Persistence and integration tests obtain a change set by exercising `ValidationPipeline`; they do not construct `ValidatedChangeSetParts` directly.

Use an unambiguous strict evidence shape:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorldFactEvidenceRef {
    SnapshotFact { fact_id: FactId },
    ProposedEvent { event_index: u32 },
}
```

Before deserializing Generator or Repairer output, reject `completion.text.len() > max_proposal_bytes`. After deserialization, one shared recursive bounds validator checks:

- every top-level and nested collection, including evidence and attribute maps;
- every String/`BoundedText`/ID/key payload, including predicates and `ScalarValue::Decimal`/`Text`;
- Proposition/Claim subjects and every Entity/Topic reference;
- scene, goal, summary, event, perception, Fact, Rumor, and Memory payloads;
- total serialized proposal bytes and per-item bytes.

Unknown fields at any model-output level fail parsing. Generator and Repairer use the same validator and stable `model_output_invalid` code.

Prompt resolution and context encoding failures retain safe typed cause data:

```rust
pub enum LlmError {
    // existing variants
    PromptResolution {
        profile: PromptProfile,
        kind: PromptFailureKind,
    },
    ContextEncoding {
        profile: PromptProfile,
        kind: ContextEncodingFailureKind,
    },
}
```

The error never includes rendered prompt or untrusted context content. Mapping every prompt/context failure to `Protocol::Unsupported` is forbidden.

### 3.7 Layer and File Layout Contract

Final touched layout includes:

```text
crates/aise/src/
├── config/
│   ├── story_history.rs
│   └── prompt.rs
├── persistence/
│   ├── migration_integrity.rs
│   ├── sqlite_knowledge_reader.rs
│   ├── sqlite_snapshot.rs
│   └── sqlite_story_history_reader.rs
└── validation/
    └── validators/
        ├── deterministic_validator.rs
        └── mod.rs
```

Layer rules:

- Move `StoryHistoryConfig` from persistence to `config/story_history.rs`.
- `config::prompt` owns the four-value `PromptProfile` enum and stores asset IDs as validated Strings. Prompt converts them to `AssetRef` at its boundary and may re-export the config-owned profile.
- No file under `config/` imports any internal crate module.
- Add `config` to `dependency_direction_tests::forbidden_imports`; any `crate::` internal import from `config` fails the test.
- Rename stale `core_has_no_outer_transitive_dependency` terminology to `turn_and_domain_have_no_outer_transitive_dependency`.
- Move `DeterministicValidator` and its imports from `validation/validators/mod.rs` to `deterministic_validator.rs`; `mod.rs` remains declarations/re-exports only.
- Delete `RetrievalSignals::default()` and its fabricated `"unset"` keys. Tests construct valid signals with real keys.

### 3.8 Required Corrective Tests

Add these exact production-path cases:

| Test | Required proof |
| --- | --- |
| `migration_from_0008_produces_readable_snapshot` | Production connect migrates a complete v8 fixture and production snapshot loading succeeds |
| `migration_rejects_incomplete_binding_before_destructive_step` | Minimal binding fails and v8 legacy columns/data remain |
| `post_migration_integrity_rejects_foreign_key_or_future_revision` | `0012`/postflight fails with a stable code |
| `edge_activation_persists_and_effects_fire_once` | Edge target transitions to Active and activation effects do not repeat next Turn |
| `completion_effects_fire_once` | Completion effect is emitted only with Active-to-Completed transition |
| `unchanged_active_node_remains_in_active_goals` | Writer receives the objective on every Turn while node remains Active |
| `seed_fact_condition_state_is_materialized` | Valid `FactStateEquals` can evaluate true from Pack seed state |
| `player_action_condition_is_rejected_at_import` | Nested and root variants return `GraphConditionForbidden` |
| `pack_validation_and_import_share_typed_decision` | A missing Character/default cast/reference is invalid before storage |
| `off_scene_character_think_resolves_full_view` | Accepted off-scene AI request reaches the Gateway with full Character view |
| `unknown_character_audience_fails_before_lookup` | Unknown/unplanned audience causes zero Store calls |
| `history_and_snapshot_reject_oversize_before_body_fetch` | SQL observation proves no oversized body column is selected |
| `utf8_projection_limits_count_bytes_not_characters` | Multi-byte text cannot bypass configured byte limits |
| `negative_sqlite_revision_or_sequence_is_rejected` | No signed value wraps to a valid Domain value |
| `validated_change_set_is_not_publicly_constructible` | External compile-fail/static boundary and production Validation path pass |
| `model_output_rejects_nested_overflow_and_evidence_unknown_fields` | Evidence/count/scalar/predicate overflow and extra keys fail both stages |
| `config_is_a_leaf_module` | Prompt and Story-history config have zero internal imports |

The 35 currently absent named cases in [the prior hardening spec §3.14](./2026-08-09-context-retrieval-hardening-spec-gpt.md#314-required-test-contract) remain mandatory and keep their exact names. After this change, all 38 prior cases plus the 17 corrective cases above must exist and pass.

---

## 4. Behavior Rules

1. **CRC-1 — Stop Before Destruction**: Unrecoverable legacy state fails before `0010`/`0011`; no removed source column or table is the first indication of corruption.
2. **CRC-2 — Readable Migration**: A migration is successful only when production `StoryReadSnapshot` can read every migrated Story.
3. **CRC-3 — No Metadata Invention**: Migration never invents a player ID, Character digest, Role key, controller, revision, or sequence.
4. **CRC-4 — Transition Owns Effect**: Every Narrative effect is attached to exactly one validated state transition.
5. **CRC-5 — Projected Active Set**: Writer goals reflect projected state, including unchanged Active nodes.
6. **CRC-6 — Committed Inputs Only**: Narrative conditions read only committed Snapshot state and cannot cascade through same-evaluation proposals.
7. **CRC-7 — Supported Conditions Are Writable**: Every accepted condition has a deterministic materialization/update path; unsupported conditions fail import.
8. **CRC-8 — One Pack Decision**: Validation and pre-storage import share one typed semantic decision.
9. **CRC-9 — Exact Entity Resolution**: Role and Character keys use Snapshot authority and have no exception path.
10. **CRC-10 — Exact Character Audience**: Character Context exists only for an accepted AI think request for that same Character.
11. **CRC-11 — No Filtered Planning Errors**: Accepted requests are executed or the Turn fails; no warning-and-continue path exists.
12. **CRC-12 — Narrative Signal Completeness**: Every Narrative intent participant, location, event, Role, Character, and node becomes a bounded retrieval signal.
13. **CRC-13 — Bytes Before Bodies**: Count, byte, authorization, and revision metadata passes before any large body is fetched or decoded.
14. **CRC-14 — Checked Persistence Integers**: Signed/unsigned overflow is a typed Store error, never a wrap or clamp.
15. **CRC-15 — Validation Seals Authority**: Only deterministic Validation can construct the value accepted by Committer.
16. **CRC-16 — Recursive Output Bounds**: A limit applies to every nested field and collection, not only top-level text.
17. **CRC-17 — Config Is Leaf**: `config` depends only on std/external crates; automated checks enforce the rule.
18. **CRC-18 — No Placeholder Defaults**: Runtime DTO defaults cannot fabricate stable IDs such as `"unset"`.
19. **CRC-19 — Production-Path Coverage**: Named tests prove the real call chain and assert positive records, Store-call counts, and next-Turn visibility.

### 4.1 Error Handling

- External/model/persisted input uses no `unwrap`, `expect`, `unwrap_or_default`, lossy cast, warning-and-continue, or silent filter.
- Migration errors expose `MigrationIntegrityCode` plus safe schema version/Story ID fields; they contain no Story, prompt, Memory, or player-input body.
- Narrative missing references return `NarrativeError::MissingReference` with the stable key.
- Planner audience/key failures happen before retrieval and preserve zero Store-call assertions.
- Oversized Store projections return `StoreError::LimitExceeded`; malformed/negative persisted values return `StoreError::Serialization` or `MigrationIntegrity`.
- Prompt/context failures preserve profile and safe cause kind without content.

### 4.2 Concurrency

- Migration preflight/postflight runs before the Store is shared.
- Snapshot, history, and knowledge readers hold only a short read transaction and release it before any LLM call.
- Retrieval remains sequential by request and provider.
- Character Think remains sequential and uses the shared Gateway limiter.
- Commit remains one SQL transaction and performs no external call before commit.

### 4.3 Observability

- Emit `migration.context_preflight` and `migration.context_postflight` spans with source/final schema version, Story count, status, and stable error code only.
- `narrative.evaluate` records projected active count, transition count, activation-effect count, completion-effect count, and status.
- `context.retrieve` records rejected-before-lookup count and per-provider candidate counts without Character IDs as metric labels.
- `character.think` records requested/completed counts and a stable failure code; it never logs Character content.
- Existing `baseline.build`, `story.commit`, LLM accounting, and trace-content policies remain authoritative.

---

## 5. Acceptance Criteria

- [ ] All 17 cases in §3.8 exist with exact names and call production paths.
- [ ] All 38 cases in the prior hardening spec §3.14 exist; the missing-name check reports `missing=0`.
- [ ] `migration_rejects_incomplete_binding_before_destructive_step` asserts that `facts_json`, `rumors_json`, and `memories_json` and their seeded values still exist after failure.
- [ ] `migration_from_0008_produces_readable_snapshot` calls `SqliteStore::connect` and `Store::load_story_snapshot`, not a standalone `Migrator` only.
- [ ] Narrative activation/completion tests run two consecutive Turns and prove effect counts `1` then `0`.
- [ ] `pack_validation_and_import_share_typed_decision` changes the current false-positive fixture so a missing `protagonist_card` is invalid.
- [ ] Off-scene Character Think makes one fake Gateway call; unknown/player requests make zero calls.
- [ ] SQL-observation tests prove oversized Snapshot/history/knowledge bodies are not selected.
- [ ] Future and negative revision tests assert typed failure instead of an empty result.
- [ ] External code cannot name `ValidatedChangeSetParts` or call its constructor.
- [ ] `rg -n 'pub fn (new|from_validated_parts)\(.*ValidatedChangeSetParts|pub struct ValidatedChangeSetParts' crates/aise/src/turn` returns zero matches.
- [ ] `rg -n 'ValidatedChangeSet::(new|from_validated_parts)' crates/aise/src --glob '*.rs'` returns exactly one production match in `validation/validation_pipeline.rs`.
- [ ] `rg -n 'warn!|continue;' crates/aise/src/character/character_think_pipeline.rs` returns zero matches.
- [ ] `rg -n 'KnowledgeEntity::Character\(_\) \| KnowledgeEntity::Role\(_\)' crates/aise/src/planning` returns zero matches.
- [ ] `rg -n '\bas (i64|u64)\b' crates/aise/src/persistence --glob '*.rs'` returns zero matches.
- [ ] `rg -n 'length\((i|s|e)\.[a-z_]+\)' crates/aise/src/persistence --glob '*.rs'` returns zero uncast text/blob length matches.
- [ ] `rg -n 'crate::(turn|domain|runtime|context|planning|character|story|validation|llm|persistence|prompt|engine)' crates/aise/src/config --glob '*.rs'` returns zero matches.
- [ ] `validation/validators/mod.rs` contains declarations/re-exports only.
- [ ] `rg -n 'SceneKey::from\("unset"\)|LocationKey::from\("unset"\)' crates/aise/src crates/aise/tests --glob '*.rs'` returns zero matches.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo test --workspace --all-features` passes.
- [ ] `cargo +1.85 fmt --all -- --check` passes.
- [ ] `cargo +1.85 clippy --workspace --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo +1.85 test --workspace --all-features` passes.
- [ ] `git diff --check` passes.

---

## 6. Out of Scope / Future Work

- Typed player-action events may later make `PlayerActionOccurred` legal; until then the Pack condition remains forbidden.
- Complete `.aisepack` asset-byte persistence and deterministic archive export require a separate design/spec.
- BM25/embedding retrieval requires a versioned ranking contract and is not added to v1 retrieval.

---

## 7. References

- [Context Preparation and Retrieval Design](../design/2026-08-08-context-preparation-retrieval-design-gpt.md)
- [Context Retrieval Hardening Spec](./2026-08-09-context-retrieval-hardening-spec-gpt.md)
- [Story Pack v3 Spec](./2026-08-07-story-pack-v3-spec-gpt.md)
- [Architecture](../design/2026-08-04-Architecture-gpt.md)
- Guardrails: `doc/agents/guardrails/`
