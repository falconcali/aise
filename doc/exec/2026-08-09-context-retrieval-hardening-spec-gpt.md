# Context Retrieval Hardening — Spec

> **Model**: GPT-5
> **Date**: 2026-08-09
> **Status**: Proposed
> **Source Design**: [Context Preparation and Retrieval Design](../design/2026-08-08-context-preparation-retrieval-design-gpt.md)
> **Phase**: Corrective completion

---

## 1. Goal

Complete the context-preparation and retrieval refactor so a materialized Story Pack produces revision-consistent, audience-safe Context and every validated Turn atomically advances the same authoritative state that the next Turn reads.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Materialize resolved Character assets, relationships, Pack constraints, World Book Facts and Rumors, and Role seed Memories when a Story Instance is created.
- Replace the parallel legacy character/world/memory models with the Story Instance and indexed knowledge models.
- Load one bounded `StoryReadSnapshot` without decoding Fact, Rumor, or Memory bodies.
- Make Summary boundaries, Narrative transitions, constraint expiry, knowledge IDs, and source revisions engine-owned.
- Make validated Character, relationship, perception, Narrative, Summary, scene, and knowledge changes visible to the next Turn through one atomic commit.
- Enforce exact per-request Memory ownership in Planning, SQL authorization, final Context validation, and prompt partitioning.
- Preserve every Entity/Topic match reason and every provider rank through candidate deduplication.
- Provide strict, deployable trusted prompts for the four business LLM profiles that are actually invoked.
- Replace the broken destructive migration behavior with an integrity-gated final schema.
- Return real persisted Turn metadata from the Story API instead of continuity placeholders.
- Restore positive-path, negative-path, migration, SQL-plan, and end-to-end regression coverage.

### 2.2 Non-Goals

- Does not add BM25, embedding, fuzzy matching, reranking, or provider score fusion.
- Does not add multi-player Story Instances.
- Does not add arbitrary free-text retrieval over the full knowledge table.
- Does not let an LLM create, delete, replace, relax, or expire Story constraints.
- Does not introduce an LLM-based validation stage; Validation remains deterministic.
- Does not make `PlayerActionOccurred` usable without a typed player-action protocol; Pack import rejects that condition in this phase.
- Does not preserve development databases that contain unrecoverable legacy runtime knowledge without Entity, Topic, salience, or source metadata; migration fails with a stable diagnostic instead of guessing.
- Does not change provider pricing, streaming, cancellation, or the shared LLM limiter.

### 2.3 Implementation Constraints

- This is a hard refactor under `R-REFACTOR-01/02`: no runtime fallback, dual read, dual write, compatibility DTO, or warning-and-skip path remains.
- `StoryReadSnapshot` is the generation read model; Story API history uses a separate bounded history query.
- Domain and turn contain no SQL or transport types. Persistence imports inward contracts and implements ports.
- The only production Candidate Retrievers are Entity and Topic, registered in that order.
- Business Pipelines construct only typed `ModelRequest<C>` values and call `LlmGateway::complete_typed`.
- No `#[allow(...)]`, dead-code anchor, inline test module, source comment, or function body in `mod.rs`/`lib.rs` is introduced or retained in touched code.
- No lock or transaction is held across an LLM call. No write lock is held across any `.await`.
- Every collection and serialized blob entering a Turn, Snapshot, prompt, migration, or API response is bounded before final allocation.

### 2.4 Required Implementation Order

1. Add the final Domain contracts and delete legacy Domain models.
2. Correct Pack dependency resolution, validation, and Story Instance materialization.
3. Add the integrity migration and normalized Snapshot projections.
4. Implement bounded Snapshot and Story-history reads.
5. Correct Planning, authorization, indexed lookup, provenance merge, ranking, and Context validation.
6. Replace the model-output and `ValidatedChangeSet` contracts, then implement the atomic commit.
7. Install strict packaged prompts and remove prompt fallback and the unused Narrative Validator profile.
8. Restore API, observability, integration tests, and toolchain gates.

No later step may be merged while an earlier step is represented by empty vectors, default fabricated state, placeholder assets, or a compatibility write.

---

## 3. Contracts

### 3.1 Final Module and Asset Layout

```text
crates/aise/src/domain/
├── asset/
│   └── text_matcher.rs
├── knowledge/
│   ├── entry.rs
│   ├── fact.rs
│   ├── memory.rs
│   ├── mod.rs
│   ├── query.rs
│   └── rumor.rs
├── story_instance/
│   ├── binding.rs
│   ├── constraint.rs
│   ├── info.rs
│   ├── mod.rs
│   ├── snapshot.rs
│   └── state.rs
└── text/
    ├── mod.rs
    └── token_estimator.rs

crates/aise/src/persistence/
├── knowledge_read_port.rs
├── sqlite_knowledge_reader.rs
├── sqlite_snapshot.rs
├── sqlite_story_history_reader.rs
├── story_history_read_port.rs
├── store.rs
└── tests/

crates/aise/src/prompt/
├── model_request.rs
├── profile.rs
├── runtime_context_encoder.rs
└── trusted_prompt_source.rs

crates/aise/assets/prompts/context-v2/
├── csi/
├── rc/
├── fti/
├── index.yaml
└── slots.yaml

crates/aise/assets/persistence/mig/
└── 0011_context_retrieval_integrity.sql
```

Delete these superseded production modules and symbols:

```text
crates/aise/src/domain/character.rs
crates/aise/src/domain/memory.rs
crates/aise/src/domain/world.rs
crates/aise/src/turn/token_estimator.rs
crates/aise/src/domain/asset/topic_matcher.rs
crates/aise/src/context/topic_matcher.rs
PromptProfile::NarrativeValidator
NarrativeValidatorContext
ModelRequest::narrative_validator
LlmCallPurpose::NarrativeValidation
builtin_catalog
_instance_factory_anchor
_entity_key_anchor
RoleControllerKindMirror
_condition_anchor
```

Historical SQL migration files may mention old tables and columns. Production Rust and the final schema may not read or write them.

### 3.2 Story Binding and Authoritative State

`domain/story_instance/binding.rs` owns immutable cast and controller identity:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "player_id", rename_all = "snake_case", deny_unknown_fields)]
pub enum RoleController {
    Player(PlayerId),
    Ai,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleBinding {
    pub role_key: StoryRoleKey,
    pub character_id: CharacterId,
    pub character_asset: FrozenCharacterAssetRef,
    pub controller: RoleController,
    pub bound_at_ms: i64,
}
```

`CharacterInstanceState` contains mutable Story state only. Character identity exists only in `RoleBinding`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterInstanceState {
    pub character_id: CharacterId,
    pub role_key: StoryRoleKey,
    pub location: LocationKey,
    pub goals: Vec<BoundedText>,
    pub attributes: BTreeMap<AttributeKey, ScalarValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RelationshipKey {
    pub source_character_id: CharacterId,
    pub target_character_id: CharacterId,
    pub kind: RelationshipKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipState {
    pub source_character_id: CharacterId,
    pub target_character_id: CharacterId,
    pub kind: RelationshipKind,
    pub trust: i16,
}
```

The sole persistent knowledge envelope is `domain::knowledge::entry::KnowledgeEntry`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case", deny_unknown_fields)]
pub enum KnowledgeEntry {
    Fact(WorldFact),
    Rumor(SharedRumor),
    Memory(MemoryEntry),
}

impl KnowledgeEntry {
    pub fn source_id(&self) -> KnowledgeSourceId;
    pub fn kind(&self) -> KnowledgeKind;
    pub fn content(&self) -> &BoundedText;
    pub fn entities(&self) -> &[KnowledgeEntity];
    pub fn topics(&self) -> &[TopicKey];
    pub fn salience(&self) -> u8;
    pub fn source(&self) -> &KnowledgeSource;
    pub fn source_revision(&self) -> StoryRevision;
    pub fn memory_owner(&self) -> Option<&CharacterId>;
}
```

`MaterializedStoryInstanceSpec` is complete and contains no parallel fact/rumor/memory arrays:

```rust
pub struct MaterializedStoryInstanceSpec {
    pub story_id: StoryId,
    pub pack: FrozenStoryPackRef,
    pub settings: InstanceSettings,
    pub bindings: BTreeMap<StoryRoleKey, RoleBinding>,
    pub characters: BTreeMap<CharacterId, CharacterInstanceState>,
    pub relationships: Vec<RelationshipState>,
    pub knowledge: Vec<KnowledgeEntry>,
    pub scene: CurrentScene,
    pub current_perceptions: Vec<CurrentPerception>,
    pub narrative_state: NarrativeRuntimeState,
    pub condition_state: NarrativeConditionStateView,
    pub active_constraints: Vec<ActiveStoryConstraint>,
    pub opening: BoundedText,
    pub created_at_ms: i64,
}
```

`StoryConstraintSource` contains only the Pack source in this phase:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryConstraintSource {
    pub pack_id: PackId,
    pub constraint_key: ConstraintKey,
}
```

There is no model-output contract for constraints.

### 3.3 Pack Resolution, Validation, and Materialization

An imported Pack remains self-contained:

```rust
pub struct ValidatedStoryPack {
    pub pack: StoryPack,
    pub canonical_manifest: Vec<u8>,
    pub digest: Sha256Digest,
    pub resolved_characters: BTreeMap<CharacterAssetKey, CharacterCard>,
    pub resolved_world_book: WorldBook,
}

pub struct FrozenStoryPack {
    pub pack_id: PackId,
    pub pack: StoryPack,
    pub digest: Sha256Digest,
    pub resolved_characters: BTreeMap<CharacterAssetKey, CharacterCard>,
    pub resolved_world_book: WorldBook,
}
```

`PackService::import` must either resolve each `CharacterAssetSource` and `WorldBookSource` to its pinned digest or return `AssetImportError::Invalid`. It must never create an empty placeholder World Book, discard embedded Character Cards, or map a validation failure to `AssetImportError::Io`.

Pack validation returns `AssetValidationCode` values and enforces all of the following before storage:

- Topic label and alias text is non-empty after trim, byte bounded, normalized once, and collision-free.
- Fact/Rumor/Memory Topic references exist in the resolved World Book.
- Per-entry Entity and Topic counts are within `AssetLimitsConfig`.
- Pack-authored entries contain no `KnowledgeEntity::Character`.
- Every Role seed relationship resolves a Role, has a unique `(source_role, target_role, kind)` tuple, and has bounded trust.
- Every Role seed Memory key is unique within that Role.
- Every constraint scope and lifecycle reference resolves.
- Every default cast resolves to a stored Character Card.
- `CharacterStateEquals` and `RelationshipReaches` are accepted and evaluated by `NarrativeDirector`; `PlayerActionOccurred` is rejected with `GraphConditionForbidden` in this phase.

`StoryInstanceFactory::create` performs one Store write after fully constructing the spec. IDs are deterministic:

```text
FactId   = "{story_id}:seed:fact:{fact_key}"
RumorId  = "{story_id}:seed:rumor:{rumor_key}"
MemoryId = "{story_id}:seed:memory:{role_key}:{memory_key}"
ConstraintId = "{story_id}:seed:constraint:{constraint_key}"
```

Materialization rules are exact:

1. `player_character`, when present, must resolve to one pinned Character in the Pack's resolved dependency set and replaces only the selected Role's default cast.
2. Every Role receives exactly one immutable `RoleBinding`; exactly one has `RoleController::Player`.
3. Every World Book Fact and Rumor becomes one `KnowledgeEntry` with `KnowledgeSource::Seed`, revision `0`, sorted/distinct Entity and Topic metadata, and the pinned Pack digest.
4. Every Role seed Memory becomes one owner-bound `KnowledgeEntry::Memory`. Its Entity index always includes both `Role(role_key)` and `Character(owner_id)` in addition to declared metadata.
5. Every relationship seed resolves source and target Role bindings and becomes one directed `RelationshipState`.
6. Every Pack constraint becomes one `ActiveStoryConstraint`.
7. Scene presence is derived from the Pack start state and contains only bound Character IDs.
8. Any limit, missing asset, missing binding, duplicate materialized ID, or invalid reference aborts before `Store::create_story_instance` is called.

### 3.4 Snapshot and Story-History Read Models

Replace the long Snapshot constructor with one parts value:

```rust
pub struct StoryReadSnapshotParts {
    pub story_id: StoryId,
    pub base_revision: StoryRevision,
    pub pack: FrozenStoryPackRef,
    pub story_profile: StoryProfile,
    pub instance_settings: InstanceSettings,
    pub role_definitions: BTreeMap<StoryRoleKey, StoryRole>,
    pub role_bindings: BTreeMap<StoryRoleKey, RoleBinding>,
    pub character_cards: BTreeMap<CharacterId, CharacterCard>,
    pub character_states: BTreeMap<CharacterId, CharacterInstanceState>,
    pub current_scene: CurrentScene,
    pub relationships: Vec<RelationshipState>,
    pub current_perceptions: Vec<CurrentPerception>,
    pub narrative_definition: NarrativeGraphDefinition,
    pub narrative_state: NarrativeRuntimeState,
    pub condition_state: NarrativeConditionStateView,
    pub story_continuity: StoryContinuity,
    pub active_constraints: Vec<ActiveStoryConstraint>,
    pub entity_catalog: Vec<KnowledgeEntity>,
    pub topic_dictionary: BTreeMap<TopicKey, TopicDefinition>,
    pub knowledge_snapshot: KnowledgeSnapshotRef,
}

impl StoryReadSnapshot {
    pub fn try_from_parts(parts: StoryReadSnapshotParts) -> Result<Self, StorySnapshotError>;
}
```

Construction rejects, with a stable `StorySnapshotError::Inconsistent { code }`, at least:

- Any Story ID, Pack digest, or revision mismatch.
- Anything other than exactly one player-controlled binding.
- Missing or extra Role bindings, Character states, or Character Cards.
- A Character state whose Role or ID disagrees with its binding.
- Scene, relationship, perception, constraint, condition, Entity, Topic, or Narrative references that do not resolve.
- Duplicate or unsorted metadata where canonical order is required.
- A `source_revision` greater than `base_revision` in any Snapshot metadata projection.

At Pack import, persist these bounded Snapshot projections separately from export blobs:

```text
story_packs.story_profile_json
story_packs.role_definitions_json
story_packs.narrative_definition_json
story_packs.topic_dictionary_json
story_packs.resolved_characters_json
```

`SqliteStore::load_story_snapshot` reads only those projections plus Story/Instance state and indexed Entity metadata. It never selects or decodes `pack_json`, `world_book_json`, `knowledge_entries.content`, or `knowledge_entries.payload_json`.

Before decoding each JSON/BLOB projection, SQL returns its byte length. A length over its configured maximum returns `StoreError::LimitExceeded` without fetching the blob. Collections use `LIMIT max + 1`; row `max + 1` is an error, never silent truncation. An unknown Entity kind, malformed Summary, malformed constraint, malformed source revision, or malformed projection is a serialization error, never `unwrap_or_default`.

Story API history uses a separate port:

```rust
#[derive(Debug, Clone, Copy)]
pub struct StoryHistoryQuery {
    pub after_sequence: Option<StorySequence>,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryTurnView {
    pub turn_id: TurnId,
    pub sequence: StorySequence,
    pub player_input: String,
    pub story_text: String,
    pub created_at: i64,
}

pub struct StoryHistoryPage {
    pub turns: Vec<StoryTurnView>,
    pub next_after_sequence: Option<StorySequence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryHistoryConfig {
    pub default_page_size: usize,
    pub max_page_size: usize,
    pub max_player_input_bytes: usize,
    pub max_story_text_bytes: usize,
}

#[async_trait]
pub trait StoryHistoryReadPort: Send + Sync {
    async fn load_story_history(
        &self,
        story_id: &StoryId,
        query: StoryHistoryQuery,
    ) -> Result<StoryHistoryPage, StoreError>;
}
```

`default_page_size`, `max_page_size`, `max_player_input_bytes`, and `max_story_text_bytes` are positive; default size is at most max size. `StoryHistoryReadPort` selects `sequence > after_sequence` in ascending sequence order and fetches `limit + 1`. If and only if row `limit + 1` exists, it removes that row and returns the last returned row's sequence as `next_after_sequence`; it never uses the removed row as the cursor. An empty page has no continuation.

`GET /stories/{story_id}` accepts optional `turn_after` and `turn_limit`, caps the limit with `StoryHistoryConfig.max_page_size`, and returns `turns: Vec<StoryTurnView>` plus `next_turn_after`. It maps persisted values exactly, never derives API history from `StoryContinuity`, and never returns fabricated empty input or timestamp zero.

### 3.5 Continuity, Summary, Constraints, and Token Estimation

The shared estimator moves inward so Domain may use it without a Domain-to-turn backedge:

```rust
pub fn estimate_text_tokens(text: &str) -> u64;
```

It lives at `domain/text/token_estimator.rs` and is the only text-token estimator used by Story Continuity, Baseline, Context, and LLM input estimation.

Its exact result is `max(1, ceil(text.chars().count() / 4))`, using checked or saturating integer conversion without a second estimator.

Snapshot continuity SQL is boundary-aware:

```sql
SELECT id, sequence, story_text
FROM story_turns
WHERE world_id = ?1 AND sequence > ?2
ORDER BY sequence ASC
LIMIT ?3;
```

`?2` is `summary.summarized_through` or `0`; `?3` is `max_recent_segments + 1`. The Store does not select the newest `N` and then remove summarized rows.

Model output contains `summary_text: Option<String>` only. `None` or trimmed-empty text produces `StateChange::Unchanged` and never erases an existing Summary. If deterministic Validation accepts non-empty Summary text, it assigns `summarized_through = snapshot.story_continuity().latest_sequence()`. Non-empty text with no pre-Turn sequence produces a repairable Schema issue and cannot seal a change set. Model JSON can never provide a boundary.

Constraint expiry is deterministic at change-set construction:

- `Persistent` remains active.
- `ThroughSequence { sequence }` remains active through that committed sequence and is removed before the next sequence.
- `UntilNarrativeNodeResolved { node_key }` is removed when the final committed node state is `Completed` or `Skipped`.
- Model output has no constraint field, including during repair.

### 3.6 Strict Model Outputs and Validated Changes

Every deserialized model-output type has `#[serde(deny_unknown_fields)]`. `BoundedText` deserialization alone is not a runtime bound; each parsed String and collection is converted through a config-aware validator before entering `TurnExecutionContext`.

Planner output remains limited to goal, gaps, and Character Think requests. Story generation and repair share this exact output shape:

```rust
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryProposalOutput {
    pub story_text: String,
    #[serde(default)]
    pub events: Vec<ProposedEvent>,
    #[serde(default)]
    pub character_changes: Vec<ProposedCharacterChange>,
    #[serde(default)]
    pub relationship_changes: Vec<ProposedRelationshipChange>,
    #[serde(default)]
    pub knowledge_changes: Vec<ProposedKnowledgeChange>,
    #[serde(default)]
    pub perceptions: Vec<ProposedPerception>,
    pub scene_change: Option<CurrentScene>,
    pub summary_text: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposedEvent {
    pub kind: EventKind,
    pub summary: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposedCharacterChange {
    pub character_id: CharacterId,
    pub location: Option<LocationKey>,
    pub goals: Option<Vec<String>>,
    #[serde(default)]
    pub attribute_updates: BTreeMap<AttributeKey, ScalarValue>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposedRelationshipChange {
    pub source_character_id: CharacterId,
    pub target_character_id: CharacterId,
    pub kind: RelationshipKind,
    pub trust_delta: i16,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProposedKnowledgeChange {
    Fact {
        content: String,
        proposition: Option<Proposition>,
        entities: Vec<KnowledgeEntity>,
        topics: Vec<TopicKey>,
        salience: u8,
        evidence: Vec<WorldFactEvidenceRef>,
    },
    Rumor {
        content: String,
        claim: Option<Claim>,
        entities: Vec<KnowledgeEntity>,
        topics: Vec<TopicKey>,
        salience: u8,
        source_character_id: Option<CharacterId>,
        truth_value: TruthValue,
        source_event_index: Option<u32>,
    },
    Memory {
        owner: CharacterId,
        memory_kind: MemoryKind,
        content: String,
        entities: Vec<KnowledgeEntity>,
        topics: Vec<TopicKey>,
        salience: u8,
        source_event_index: Option<u32>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposedPerception {
    pub character_id: CharacterId,
    pub source_event_index: u32,
    pub content: String,
}
```

Validation converts model output into non-deserializable final changes:

```rust
pub struct CharacterInstanceStateChange {
    pub character_id: CharacterId,
    pub new_state: CharacterInstanceState,
}

pub struct RelationshipStateChange {
    pub key: RelationshipKey,
    pub new_state: RelationshipState,
}

pub struct ValidatedNarrativeChange {
    pub node_key: NarrativeNodeKey,
    pub from: NarrativeNodeState,
    pub to: NarrativeNodeState,
    pub expected_graph_revision: u64,
}

pub struct ValidatedChangeSet {
    story_text: BoundedText,
    events: Vec<StoryEvent>,
    character_changes: Vec<CharacterInstanceStateChange>,
    relationship_changes: Vec<RelationshipStateChange>,
    knowledge_additions: Vec<KnowledgeEntry>,
    current_perceptions: Vec<CurrentPerception>,
    scene_change: StateChange<CurrentScene>,
    narrative_changes: Vec<ValidatedNarrativeChange>,
    condition_state: NarrativeConditionStateView,
    constraint_change: StateChange<Vec<ActiveStoryConstraint>>,
    summary_change: StateChange<StorySummary>,
}
```

Only deterministic Validation constructs `ValidatedChangeSet`. It applies Character patches to the corresponding Snapshot state instead of rebuilding a blank legacy state. It applies each relationship delta once. Unknown Character IDs, unresolved Topic/Entity references, invalid evidence, duplicate changes, and any bound violation produce Validation issues; none are dropped with `continue`, `filter`, warning, or default values.

Knowledge additions use deterministic IDs and the committed revision:

```text
FactId   = "{turn_id}:fact:{change_index}"
RumorId  = "{turn_id}:rumor:{change_index}"
MemoryId = "{turn_id}:memory:{owner_id}:{change_index}"
source_revision = base_revision + 1
source = CommittedTurn { turn_id, event_id }
```

Memory metadata always includes `Character(owner)`; owner mismatches are fatal validation issues. Narrative changes come from `WriterPlan.narrative_plan.proposed_transitions`, not model output. Global Narrative event intents become committed keyed Story events. Current perceptions refer only to an event in the same validated proposal and replace the prior bounded current-perception set at commit.

### 3.7 Atomic Store Commit

The Store accepts one complete commit value:

```rust
pub struct TurnCommitSpec {
    pub story_id: StoryId,
    pub base_revision: StoryRevision,
    pub expected_graph_revision: u64,
    pub turn: StoryTurn,
    pub changes: ValidatedChangeSet,
    pub idempotency_key: IdempotencyKey,
    pub request_digest: RequestDigest,
    pub outbox: Vec<OutboxRecord>,
    pub llm_calls: Vec<LlmCallUsage>,
}
```

`SqliteStore::commit_turn` performs, in one transaction and only after both optimistic checks pass:

1. Assign and insert the engine-owned next `StorySequence`.
2. Insert Story events and outbox rows.
3. Replace changed `story_instances.characters_json` and `relationships_json` from validated final maps.
4. Apply Narrative transitions, incrementing graph revision exactly once when the transition list is non-empty.
5. Replace current perceptions and condition state.
6. Apply scene, deterministic constraint expiry, and engine-owned Summary.
7. Insert each `KnowledgeEntry` plus all Entity and Topic mapping rows.
8. Store idempotency result and LLM ledger.
9. Increment `stories.revision` from `base_revision` to `base_revision + 1`.

Any SQL, serialization, uniqueness, Story revision, graph revision, or mapping failure rolls back all nine effects. The method never writes `worlds`, `characters`, `memory`, `facts_json`, `rumors_json`, or `memories_json`.

### 3.8 Final Persistence Schema and Migration

Add `0011_context_retrieval_integrity.sql`; do not edit the checksums of `0009` or `0010`. The migration uses a `_new` staging name and renames it after verification; the final production table remains `knowledge_entries`:

```sql
CREATE TABLE knowledge_entries_new (
    story_id TEXT NOT NULL REFERENCES stories(id) ON DELETE CASCADE,
    source_id TEXT NOT NULL,
    knowledge_kind TEXT NOT NULL CHECK (knowledge_kind IN ('fact', 'rumor', 'memory')),
    memory_owner_character_id TEXT,
    content TEXT NOT NULL,
    salience INTEGER NOT NULL CHECK (salience BETWEEN 0 AND 255),
    source_json TEXT NOT NULL CHECK (json_valid(source_json)),
    payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
    source_revision INTEGER NOT NULL CHECK (source_revision >= 0),
    PRIMARY KEY (story_id, knowledge_kind, source_id),
    CHECK (
        (knowledge_kind = 'memory' AND memory_owner_character_id IS NOT NULL)
        OR (knowledge_kind != 'memory' AND memory_owner_character_id IS NULL)
    )
);
```

The migration performs this exact sequence in one transaction:

1. Create `knowledge_entries_new`, `knowledge_entry_entities_new`, and `knowledge_entry_topics_new`; both new mapping tables have the composite cascading foreign key to `knowledge_entries_new`.
2. Copy or reconstruct rows into the three new tables and run all count, metadata, ownership, JSON, and foreign-key checks.
3. Rebuild `story_turns` and the Pack/Instance projections, then verify their constraints.
4. Drop the old mapping tables before the old `knowledge_entries` table, rename `knowledge_entries_new` to `knowledge_entries`, then rename both new mapping tables to their final names.
5. Create `ix_knowledge_entry_entities_lookup` and `ix_knowledge_entry_topics_lookup` against the final mapping tables and run `PRAGMA foreign_key_check`.
6. Drop legacy source tables/columns only after every prior check succeeds, then commit.

No `_new`, `_old`, or version-suffixed table or index remains after success. A failed check rolls back to the exact pre-migration schema and data.

The migration rebuilds `story_turns` so `sequence INTEGER NOT NULL` and `UNIQUE(world_id, sequence)` are table constraints, preserves event foreign keys, and removes `summary_delta`. It adds the Pack projection columns from §3.4 and these Instance columns:

```text
story_instances.settings_json             TEXT NOT NULL
story_instances.current_perceptions_json  TEXT NOT NULL
story_instances.condition_state_json      TEXT NOT NULL
```

It removes the duplicate `story_instances.revision` column and drops the final `worlds`, `characters`, and `memory` tables after integrity checks.

Migration behavior is mandatory:

- Reconstruct seed knowledge from the pinned `story_packs` projections and immutable bindings, even when `0010` already dropped the three legacy JSON columns.
- Preserve existing valid indexed rows only when their complete typed payload and index metadata can be proven.
- Reject a non-empty legacy Summary without a provable `summarized_through` value.
- Reject any legacy committed world fact, memory, or character mutation that lacks enough metadata to construct the final contract.
- Compare expected and inserted counts for Fact, Rumor, Memory, Entity mapping, and Topic mapping rows before dropping a source.
- Roll back the whole migration on any mismatch and expose stable code `context_retrieval_migration_unrecoverable`.

Migration tests start from schema versions `0008`, `0010`, and an empty fresh database. No test deletes or edits a fixture to make migration pass unless the fixture explicitly exercises the documented unrecoverable error.

### 3.9 Retrieval Plan and Matching

The final request contains engine-derived authorization:

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RetrievalRequest {
    pub audience: RetrievalAudience,
    pub knowledge_kinds: Vec<KnowledgeKind>,
    pub entities: Vec<KnowledgeEntity>,
    pub topics: Vec<TopicKey>,
    pub query_text: Option<BoundedText>,
    pub authorized_memory_owners: Vec<CharacterId>,
    pub reason: BoundedText,
    pub origin: RetrievalRequestOrigin,
    pub signal_priority: u8,
}
```

`authorized_memory_owners` is absent from `PlannerOutput`. `RetrievalPlanBuilder` derives it after resolving and validating all keys:

- For `GlobalWriter` with Memory, it is the sorted/distinct Character entities explicitly present in that request, and every owner must occur in final `character_think_requests`.
- For `Character { A }` with Memory, it is exactly `[A]`.
- For a request without Memory, it is empty.
- A Character request containing Fact is invalid.

Every final request has at least one knowledge kind and at least one Entity, Topic, or non-empty canonical query. Query expansion happens before the final per-request Entity/Topic limit checks. Unknown Role and Character keys are rejected exactly like every other Entity kind.

Narrative-derived requests include every referenced active node, global event key, global-event participant, global-event location, Character impulse Role, and Character impulse target. They do not contain only active node keys.

Entity and Topic matching share one `domain::asset::text_matcher` implementation. A term uses ASCII alphanumeric boundaries when every character is ASCII; punctuation and spaces do not switch it to substring mode. A term containing at least one non-ASCII character uses normalized literal substring matching. Planner query expansion and automatic signals call the same matcher.

Request deduplication uses a typed canonical key, not `Debug` strings:

```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct RetrievalRequestKey {
    audience: RetrievalAudience,
    knowledge_kinds: Vec<KnowledgeKind>,
    entities: Vec<KnowledgeEntity>,
    topics: Vec<TopicKey>,
    query_text: Option<String>,
    authorized_memory_owners: Vec<CharacterId>,
}
```

Final request ordering and winner precedence remain those in the source design. Exceeding a bound fails Planning; no request, key, or Character Think request is silently removed except exact duplicate requests or exact duplicate Character IDs.

### 3.10 Indexed Knowledge Read and Authorization

Index matches are Domain-owned and returned by the read port:

```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", content = "key", rename_all = "snake_case")]
pub enum KnowledgeIndexMatch {
    Entity(KnowledgeEntity),
    Topic(TopicKey),
}

pub struct KnowledgeRecord {
    pub source_id: KnowledgeSourceId,
    pub kind: KnowledgeKind,
    pub content: BoundedText,
    pub salience: u8,
    pub source: KnowledgeSource,
    pub source_revision: StoryRevision,
    pub memory_owner: Option<CharacterId>,
}

pub struct KnowledgeLookupHit {
    pub record: KnowledgeRecord,
    pub matches: Vec<KnowledgeIndexMatch>,
}
```

`KnowledgeFilter` receives the exact request owners, not every Character Think owner in the Turn:

```rust
pub struct KnowledgeFilter {
    pub audience: RetrievalAudience,
    pub knowledge_kinds: Vec<KnowledgeKind>,
    pub authorized_memory_owners: Vec<CharacterId>,
    pub max_item_bytes: usize,
}
```

Entity and Topic methods return `Vec<KnowledgeLookupHit>`. Their SQL applies all of these predicates before `ORDER BY` and `LIMIT`:

```text
story_id = snapshot.story_id
source_revision <= snapshot.base_revision
knowledge_kind IN non-empty requested kinds
requested Entity or Topic mapping match
Fact/Rumor/Memory audience authorization
Memory owner IN exact authorized owners
```

The read transaction first verifies the current Story revision and pinned Pack digest. Authorization failure in the Candidate Retriever returns before the Store call; the Store independently enforces the same predicate. Unauthorized rows cannot consume the SQL limit. Empty kinds, empty selectors, negative/corrupt revisions, out-of-range salience, oversized content, unknown kinds, or malformed source/payload are typed errors; they are never broadened, clamped, skipped, or defaulted.

Queries use the mapping indexes and return every requested match for each bounded selected source. They do not scan or deserialize all knowledge rows or the World Book.

### 3.11 Candidate Evidence, Ranking, and Final Context

Provider evidence has one owner and no duplicated parallel vectors:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct ProviderEvidence {
    pub provider_rank: u32,
    pub matches: Vec<KnowledgeIndexMatch>,
}

pub struct ContextCandidate {
    pub record: KnowledgeRecord,
    pub audience: RetrievalAudience,
    pub evidence: BTreeMap<CandidateRetrieverKind, ProviderEvidence>,
    pub signal_priority: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextProvenance {
    pub source_id: KnowledgeSourceId,
    pub knowledge_kind: KnowledgeKind,
    pub source: KnowledgeSource,
    pub source_revision: StoryRevision,
    pub audience: RetrievalAudience,
    pub memory_owner: Option<CharacterId>,
    pub evidence: BTreeMap<CandidateRetrieverKind, ProviderEvidence>,
}
```

Each retriever returns one map entry. Candidate deduplication key is `(RetrievalAudience, KnowledgeSourceId)`. Merge unions provider entries and matches, retains the minimum non-zero rank per provider, retains the lowest signal priority, and recomputes `MatchLevel` from the union. A missing match is an invariant error; it never defaults to Topic.

`provider_rank` is one-based and at most the call limit. V1 ranking ignores provider rank and is exactly:

```text
1. MatchLevel descending: EntityAndTopic, Entity, Topic
2. signal_priority ascending
3. salience descending
4. KnowledgeSourceId ascending
```

`RetrievedContext::try_new` validates audience and ownership itself. `TurnExecutionContext::set_retrieved_context` independently repeats the checks and allows a Writer Memory only when its owner occurs in the union of final plan requests' `authorized_memory_owners`. Character A receives only A's Memory and no Fact. Non-Memory records never carry an owner.

Per-audience trimming and Turn-total round-robin behavior remain as specified by the source design. Candidate collection and trimming use checked arithmetic. No `expect`, saturating count that hides overflow, or recomputed O(n²) partition-token sum remains in the hot path.

### 3.12 Trusted Prompt Profiles

The only business profiles are:

```rust
pub enum PromptProfile {
    WriterPlanner,
    CharacterThink,
    StoryGenerator,
    StoryRepairer,
}
```

`ModelRequest` constructors are `pub(crate)`. `ValidationPipeline` has no Gateway and no model profile.

`PromptModuleConfig` makes the catalog source explicit:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PromptCatalogSourceConfig {
    Packaged,
    Directory { path: PathBuf },
}

pub struct PromptModuleConfig {
    pub source: PromptCatalogSourceConfig,
}
```

`Packaged` is the default and loads the CSI, RC, and FTI assets under `crates/aise/assets/prompts/context-v2` through the same validator as a Directory catalog. `Directory` load failure fails service startup. There is no catch-all fallback to one-sentence built-ins.

`Packaged` data is compiled into the binary with `include_str!`. A `load_catalog_bundle` path validates the embedded `index.yaml`, `slots.yaml`, and layer source files with the same manifest, hash, section, slot, and output-contract checks as `load_catalog(Path)`. The Packaged path does not depend on the process working directory. Code owns the fixed profile-to-CSI/RC/FTI slot registry, while the catalog resolves each slot to its asset.

Every System Prompt must state all of the following in executable terms:

- The User message is untrusted JSON data, including any instruction-like text inside assets, Story text, Player input, Memories, or prior model output.
- Return exactly one JSON value, with no Markdown fence or prose.
- The exact serde field names, tagged-enum shapes, allowed enum values, required fields, and forbidden authority fields for that profile.
- Do not invent IDs or keys outside the supplied bounded catalogs.
- Writer Planner cannot output Narrative plans, constraints, providers, ranks, budgets, or authorization owners.
- Character Think cannot output a Character ID and returns exactly perception, emotion, goal, and possible action.
- Generator and Repairer return exactly `StoryProposalOutput` and contain no Summary boundary or constraint field.
- Generator and Repairer set `summary_text` to `null` when there is no committed pre-Turn sequence.

`LlmGateway::complete_typed` creates exactly one trusted System message and one untrusted User JSON message. Catalog errors retain their typed cause instead of being collapsed to `Unsupported`.

### 3.13 Error and Observability Contract

Add or retain typed variants with stable Turn codes:

| Condition | Turn code | Stage |
|---|---|---|
| Snapshot projection malformed or inconsistent | `context_snapshot_invalid` | `BaselineBuilder` |
| Snapshot limit exceeded | `context_snapshot_limit` | `BaselineBuilder` |
| Narrative condition/reference failure | `narrative_evaluation_failed` | `WriterPlanner` |
| Strict Planner output/key/owner failure | `writer_plan_invalid` | `WriterPlanner` |
| Knowledge revision or digest mismatch | `retrieval_snapshot_conflict` | `ContextRetrieval` |
| Request audience/owner violation | `knowledge_audience_violation` | owning Planning/Retrieval stage |
| Knowledge row malformed or oversized | `retrieval_record_invalid` | `ContextRetrieval` |
| Context count/token/byte overflow | `retrieval_context_limit` | `ContextRetrieval` |
| Strict model JSON or output bound failure | `model_output_invalid` | owning LLM stage |
| Commit graph/Story revision conflict | existing typed conflict code | `TurnCommitter` |
| Unrecoverable migration input | `context_retrieval_migration_unrecoverable` | startup |
| Prompt catalog/profile failure | `trusted_prompt_catalog_invalid` | startup |

No error, event, or metadata-only trace includes raw Player input, Story text, Prompt text, Memory content, Character thought, or raw model output.

Emit these structured spans:

```text
context.prepare
    story_id, turn_id, base_revision, character_count, constraint_count,
    entity_signal_count, topic_signal_count, status, error_code

narrative.evaluate
    story_id, turn_id, graph_revision, active_node_count,
    transition_count, intent_count, status, error_code

context.retrieve
    story_id, turn_id, base_revision, request_count,
    entity_candidate_count, topic_candidate_count, merged_count,
    writer_item_count, character_partition_count, total_tokens,
    status, error_code

story.commit
    story_id, turn_id, base_revision, committed_revision,
    knowledge_addition_count, transition_count, status, error_code
```

The existing trace sink may carry these fields. Do not create an unbounded metrics label from IDs, keys, reasons, or error messages.

### 3.14 Required Test Contract

The following named integration cases are required; tests must call production builders, readers, mergers, Pipelines, or HTTP handlers rather than manually constructing the expected result:

| Test | Required proof |
|---|---|
| `pack_import_preserves_resolved_character_and_world_assets` | Embedded dependencies survive import; unresolved frozen dependencies fail without placeholders |
| `story_instance_materializes_all_seed_state_once` | Positive Fact, Rumor, Memory, relationship, constraint, cast, and index counts match the Pack |
| `player_character_replaces_one_cast_only` | Custom pinned Character changes one binding and card only |
| `baseline_uses_one_snapshot_and_no_knowledge_bodies` | One Snapshot call and zero knowledge-body calls |
| `baseline_resolves_player_scene_and_off_scene_index_by_stable_id` | Binding-based player/scene/index resolution |
| `baseline_does_not_copy_player_input` | Serialized Baseline excludes input text |
| `snapshot_rejects_corrupt_json_instead_of_defaulting` | Summary/constraint/settings corruption is a typed error |
| `snapshot_limits_before_blob_decode` | Oversized projection is rejected before value fetch/decode |
| `snapshot_does_not_decode_pack_or_world_knowledge_bodies` | SQL trace contains no export-blob/body column |
| `continuity_query_is_summary_boundary_aware` | Unsummarized suffix starts at boundary + 1 and max + 1 fails |
| `summary_boundary_is_engine_owned` | Model boundary fields fail; accepted text gets pre-Turn latest sequence |
| `retrieval_signals_follow_fixed_priority_and_bounds` | Real builder output matches fixed priority and limits |
| `topic_matcher_handles_ascii_punctuation_boundaries_and_non_ascii_terms` | Multi-word ASCII does not become substring mode; Chinese matches |
| `narrative_requests_cover_intents_participants_locations_and_impulses` | Every typed Narrative reference becomes a request signal |
| `narrative_transitions_commit_and_advance_graph_revision` | Next Snapshot observes node state and graph revision |
| `planner_query_resolves_known_keys_before_retrieval` | Shared matcher expands known keys and rechecks bounds |
| `planner_rejects_unknown_role_and_character_keys` | No Role/Character exception exists |
| `writer_memory_requires_exact_request_owner` | Missing/unplanned/extra owner fails before lookup |
| `character_memory_is_owner_isolated` | Character A cannot receive B's Memory through either index |
| `sql_authorization_precedes_limit` | Unauthorized high-sort rows cannot hide authorized results |
| `knowledge_read_rejects_future_source_revision` | `source_revision > base_revision` returns conflict/invalid record |
| `entity_topic_duplicate_merges_all_provider_evidence` | One item retains both providers, ranks, and matches |
| `candidate_provider_rank_is_bounded_and_not_used_by_v1_ranking` | Rank contract and v1 order are independent |
| `conflicting_fact_rumor_and_memory_remain_distinct` | Equal content across typed source IDs yields distinct authorized items |
| `ranking_uses_exact_stable_order` | All four ranking keys are exercised |
| `retrieval_budget_round_robin_is_deterministic` | Real pipeline output follows Writer/Character rounds |
| `writer_memory_passes_final_context_validation_when_authorized` | Planning, SQL, merge, and final validator share one policy |
| `committed_knowledge_is_retrievable_on_next_turn` | Fact/Memory commit writes indexes and revision, then positive retrieval succeeds |
| `character_and_relationship_changes_are_visible_on_next_turn` | Instance JSON, not legacy tables, is authoritative |
| `commit_failure_rolls_back_all_state_and_indexes` | Every commit surface remains unchanged after injected failure |
| `migration_from_0008_preserves_seed_knowledge_and_sequence` | Exact counts, metadata, and sequences survive |
| `migration_rejects_ambiguous_summary_or_legacy_knowledge` | Stable unrecoverable code and no partial schema change |
| `packaged_prompt_catalog_has_four_strict_profiles` | All assets load and no Narrative Validator profile exists |
| `catalog_directory_error_never_falls_back` | Startup returns the typed catalog failure |
| `typed_context_emits_one_trusted_system_and_one_untrusted_user_message` | Exact message roles and profile for all four stages |
| `asset_and_player_content_never_enters_system_prompt` | Adversarial instructions remain only in User JSON |
| `story_api_returns_persisted_turn_metadata` | Real input, timestamp, ID, and sequence; no continuity placeholder |
| `context_retrieval_end_to_end_is_revision_consistent` | Pack import through next-Turn retrieval shares one revision chain |

---

## 4. Behavior Rules

1. **CRH-1 — Complete Instance**: Story Instance creation succeeds only after every required cast, seed state, constraint, and knowledge index row is materialized.
2. **CRH-2 — No Placeholders**: Missing resolved assets, state, metadata, or prompt files fail with typed errors; empty substitutes and default fabricated values are forbidden.
3. **CRH-3 — One Authority**: Character state lives in `story_instances.characters_json`; relationships live in `relationships_json`; Fact/Rumor/Memory live in `knowledge_entries`; no legacy state table is written.
4. **CRH-4 — One Snapshot**: Each non-replayed Turn loads exactly one `StoryReadSnapshot` before Planning and reuses its revision and Pack digest through commit.
5. **CRH-5 — Metadata-Only Snapshot**: Snapshot loading never selects or decodes knowledge content/payload or full Pack/World Book export blobs.
6. **CRH-6 — Fail Before Decode**: Store byte/count limits are checked by SQL metadata or `max + 1` before final blob decode or collection allocation.
7. **CRH-7 — No Silent Recovery**: Invalid persisted JSON, Entity kinds, revisions, bounds, or references never become empty/default state.
8. **CRH-8 — Engine-Owned Sequence**: Story sequence and Summary boundary cannot be supplied by a model or API caller.
9. **CRH-9 — Engine-Owned Narrative**: Narrative transitions and keyed global events derive only from deterministic Narrative evaluation.
10. **CRH-10 — Constraint Authority**: Model output cannot mutate constraints; lifecycle expiry is deterministic and atomic with the Turn.
11. **CRH-11 — Strict Outputs**: Unknown model-output fields and every field/collection overflow fail the owning stage.
12. **CRH-12 — Exact Request Authorization**: Writer Memory authorization is derived from Character entities in that request, never from a Turn-global owner list.
13. **CRH-13 — Defense in Depth**: Planner, Candidate Retriever, SQL reader, `RetrievedContext`, and `TurnExecutionContext` enforce the same audience matrix.
14. **CRH-14 — Authorization Before Limit**: Every audience and owner SQL predicate precedes ordering and limiting.
15. **CRH-15 — Revision Scope**: A lookup returns only entries whose Story, Pack digest, and source revision belong to the Snapshot.
16. **CRH-16 — Positive Match Required**: Every Candidate has at least one actual Entity or Topic match; no-match Candidates are invariant failures.
17. **CRH-17 — Lossless Evidence**: Deduplication preserves all provider match reasons and minimum ranks.
18. **CRH-18 — Stable V1 Rank**: Provider ranks and SQL row arrival order cannot alter the four-key v1 ranking order.
19. **CRH-19 — No Full-Scan Fallback**: A zero-result indexed lookup remains empty.
20. **CRH-20 — Shared Matcher**: Automatic signals and Planner expansion use the same normalization and boundary implementation.
21. **CRH-21 — Shared Estimator**: Continuity, Baseline, Context, and LLM accounting import the sole Domain text estimator.
22. **CRH-22 — Character Isolation**: Character Think receives only its Character view, impulses, perceptions, and Context partition; Generator receives no raw Character partition.
23. **CRH-23 — Atomic Visibility**: Every accepted state or knowledge change is either visible together at revision `N+1` or not visible at all.
24. **CRH-24 — No Filtered Errors**: Unknown or invalid model references fail Validation and are never removed with `filter`, `continue`, or warning.
25. **CRH-25 — Deployable Prompts**: The default packaged catalog is complete and strict; an explicitly selected Directory catalog never falls back.
26. **CRH-26 — Honest API**: API fields contain persisted values or are removed in an explicit API version change; placeholder values are forbidden.
27. **CRH-27 — Integrity Migration**: Destructive source removal happens only after count and metadata equality checks in the same transaction.
28. **CRH-28 — Bounded Work**: Retrieval stays sequential by request and provider; no per-request task fan-out or hidden queue is added.
29. **CRH-29 — No Lock Across Await**: Snapshot, retrieval, LLM, and commit I/O occur without a held application write guard.
30. **CRH-30 — Test Production Paths**: Acceptance tests invoke production code and include positive records; manually sorting or constructing the expected plan is not coverage.

### 4.1 Error Handling

- External/model/persisted input is never handled with `unwrap`, `expect`, lossy cast, clamp, or `unwrap_or_default`.
- `CharacterThinkPipeline` returns a Planning invariant for an unknown or player-controlled requested Character; it never logs and skips.
- Prompt source errors preserve profile, asset reference, and catalog cause in typed startup errors without prompt content.
- Migration error output includes schema version, Story ID where safe, and stable error code, but no Story or Memory body.
- Checked conversions reject negative SQLite integers before conversion to unsigned Domain values.

### 4.2 Concurrency

- Snapshot and knowledge reads use short read transactions and close them before any LLM call.
- Commit uses one SQL transaction and performs no event emission, channel send, or provider call before commit.
- Character Think calls remain sequential and use the shared Gateway limiter.
- Retrieval does not spawn by request, key, or provider; configured caps bound SQL parameter and result counts.

### 4.3 Observability

- Emit the four spans from §3.13 with structured fields and stable error codes.
- Record per-provider candidate counts before merge and final item/token counts after trim.
- Record migration source version, inserted counts, verified counts, and status without content fields.
- The existing `aise.pipeline` and LLM spans remain; the new spans describe subsystem work instead of replacing accounting spans.

---

## 5. Acceptance Criteria

### 5.1 Materialization and Snapshot

- [ ] All tests in §3.14 through `snapshot_does_not_decode_pack_or_world_knowledge_bodies` pass.
- [ ] A non-empty demo Pack produces positive Fact, Rumor, and owner Memory retrieval immediately after instance creation.
- [ ] `rg -n 'relationships: Vec::new\(\)|facts: Vec::new\(\)|rumors: Vec::new\(\)|memories: Vec::new\(\)|WorldBookKey::from\("placeholder"\)|resolved_characters: BTreeMap::new\(\)' crates/aise/src/story --glob '*.rs'` returns zero materialization matches.
- [ ] `rg -n 'InstanceSettings::default\(\)|current_perceptions: Vec::new\(\)|occurred_event_keys: BTreeSet::new\(\)' crates/aise/src/persistence/sqlite_snapshot.rs` returns zero matches.
- [ ] `rg -n 'unwrap_or_default\(\)' crates/aise/src/persistence/sqlite_snapshot.rs` returns zero matches.
- [ ] SQL-observation tests prove Snapshot loading does not select `pack_json`, `world_book_json`, `knowledge_entries.content`, or `payload_json`.
- [ ] `StoryReadSnapshot::try_from_parts` has no `#[allow(clippy::too_many_arguments)]`.

### 5.2 Planning and Retrieval

- [ ] All §3.14 tests from `retrieval_signals_follow_fixed_priority_and_bounds` through `writer_memory_passes_final_context_validation_when_authorized` pass.
- [ ] `KnowledgeLookupHit.matches` is non-empty for every positive Entity/Topic lookup fixture.
- [ ] Entity+Topic dedupe emits one item with two provider evidence entries and `EntityAndTopic` rank.
- [ ] Character Fact, cross-owner Memory, and unplanned Writer Memory tests assert Store call count `0`.
- [ ] SQL-plan tests name `ix_knowledge_entry_entities_lookup` and `ix_knowledge_entry_topics_lookup` and contain no full `knowledge_entries` scan.
- [ ] `rg -n 'allowed_writer_memory_owners|format!\("\{:\?\}"|match_level_from.*false, false' crates/aise/src/context crates/aise/src/planning crates/aise/src/persistence --glob '*.rs'` returns zero obsolete implementation matches.
- [ ] `rg -n 'keyword_score|collect_source|split_whitespace\(\).*to_lowercase' crates/aise/src/context --glob '*.rs'` returns zero matches.

### 5.3 Validation and Commit

- [ ] All §3.14 tests from `committed_knowledge_is_retrievable_on_next_turn` through `commit_failure_rolls_back_all_state_and_indexes` pass.
- [ ] A committed Fact and Memory have `source_revision = committed_revision` and are returned by the next Snapshot scope but not the prior scope.
- [ ] Character goal/location/attribute and relationship trust updates apply to prior state exactly once.
- [ ] Narrative node transitions and deterministic constraint expiry are visible in the next Baseline.
- [ ] Model Summary JSON containing `summarized_through` or any constraint field fails strict parsing.
- [ ] `rg -n 'domain::character|domain::memory|domain::world|\bWorldState\b|\bCharacterState\b' crates/aise/src crates/aise-server/src --glob '*.rs'` returns zero matches.
- [ ] `rg -n 'INSERT INTO (worlds|characters|memory)|UPDATE (worlds|characters|memory)' crates/aise/src --glob '*.rs'` returns zero matches.
- [ ] Invalid proposed Character, relationship, knowledge, perception, and event references each have a negative test proving they produce an issue instead of disappearing from the change set.

### 5.4 Migration, Prompt, and API

- [ ] Migration tests for `0008`, `0010`, fresh schema, ambiguous Summary, and unrecoverable legacy knowledge pass.
- [ ] Final schema reports `story_turns.sequence` as `NOT NULL`, composite mapping foreign keys with cascade, and no `worlds`, `characters`, or `memory` table.
- [ ] Backfill count assertions cover all three knowledge kinds and both mapping tables.
- [ ] All four packaged profile assets load with verified hashes and exact output contracts.
- [ ] `rg -n 'NarrativeValidator|narrative_validator|NarrativeValidation|builtin_catalog' crates/aise/src crates/aise-server/src --glob '*.rs'` returns zero matches.
- [ ] `CatalogPromptSource::from_config` has no error-catching fallback branch.
- [ ] Prompt adversarial tests prove asset/player/retrieved text occurs only in the User JSON message.
- [ ] Story API integration tests assert real `player_input`, `created_at`, `turn_id`, and `sequence`, plus bounded page continuation.
- [ ] `rg -n 'player_input: String::new\(\)|created_at: 0' crates/aise-server/src --glob '*.rs'` returns zero matches.

### 5.5 Static Architecture and Regression Gates

- [ ] `rg -n 'pub fn estimate_text_tokens\b' crates/aise/src --glob '*.rs'` returns exactly one match in `domain/text/token_estimator.rs`.
- [ ] `test ! -e crates/aise/src/turn/token_estimator.rs` succeeds.
- [ ] `test ! -e crates/aise/src/domain/character.rs && test ! -e crates/aise/src/domain/memory.rs && test ! -e crates/aise/src/domain/world.rs` succeeds.
- [ ] `rg -n '#\[allow\(' crates/aise/src/story/instance_factory.rs crates/aise/src/domain/story_instance/snapshot.rs crates/aise/src/domain/narrative_graph crates/aise/src/persistence --glob '*.rs'` returns zero matches in touched code.
- [ ] `rg -n 'TODO\(temp-debug\)|temporarily disabled|_anchor\b' crates/aise/src --glob '*.rs'` returns zero matches.
- [ ] `domain/turn/mod.rs`, every touched `mod.rs`, and `lib.rs` remain index-only.
- [ ] Every new unit-test module uses `#[path = "tests/<source>_tests.rs"]`; no inline test body is added.
- [ ] Existing Runtime, Story Pack, Narrative Graph, validation, persistence, prompt, SSE, and API regression suites remain present and pass.
- [ ] `context_retrieval_end_to_end_is_revision_consistent` uses non-empty Pack knowledge and passes.

### 5.6 Toolchain

- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo test --workspace --all-features` passes.
- [ ] `cargo +1.85 fmt --all -- --check` passes.
- [ ] `cargo +1.85 clippy --workspace --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo +1.85 test --workspace --all-features` passes.
- [ ] `git diff --check` passes.

---

## 6. Out of Scope / Future Work

- BM25 and embedding Candidate Retrievers require a separate retrieval-provider spec.
- Typed player-action input and `PlayerActionOccurred` activation require a separate API/domain spec.
- Cross-provider score fusion may use the retained provider evidence in a later ranking spec.
- Story-history cursor signing and long-term archival are separate API/storage concerns.

---

## 7. References

- Source design: [Context Preparation and Retrieval Design](../design/2026-08-08-context-preparation-retrieval-design-gpt.md)
- Superseded execution details where this spec conflicts: [Context Preparation and Retrieval Spec](2026-08-08-context-preparation-retrieval-spec-gpt.md)
- Required upstream contracts: [Story Pack v3 Spec](2026-08-07-story-pack-v3-spec-gpt.md)
- Reviewed implementation baseline: commit `470fa9f`
- Architecture: [AISE Architecture](../design/2026-08-04-Architecture-gpt.md)
- Guardrails: `doc/agents/guardrails/`
