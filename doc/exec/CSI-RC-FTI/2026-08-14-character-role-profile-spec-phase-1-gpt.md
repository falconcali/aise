# Character Role Runtime — Phase 1 Spec

> **Model**: GPT-5
> **Date**: 2026-08-14
> **Status**: Proposed
> **Source Design**: [Character Card 与 Story Role Profile](../../design/CSI-RC-FTI/2026-08-14-character-role-profile-design-gpt.md)
> **Phase**: Phase 1 of 3 — runtime aggregate, RoleId references, and persistence

---

## 1. Goal

Make `StoryRole` the single Story Instance character aggregate and migrate every story-local reference, Snapshot, Knowledge owner, Narrative target, state-extraction target, Store record, and runtime API from instance `CharacterId`/`StoryRoleKey`/`RoleBinding` joins to `RoleId`.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Add the Story Instance `StoryRole` aggregate with Controller, frozen Effective Profile, optional frozen Card source, Role-owned story fields, and mutable Role state.
- Delete `RoleBinding`, `StoryInstanceBinding`, and `CharacterInstanceState`.
- Resolve each Role Profile exactly once during Story Instance creation: selected Card Profile or Role default Profile, never a merge.
- Replace the four-map Snapshot join with `RoleId -> StoryRoleView` and enforce one player-controlled Role.
- Migrate Current Scene, Relationship, Knowledge, Narrative Graph, Retrieval, CharacterThink request/decision, state extraction, validation, commit, API, and Store contracts to `RoleId`.
- Remove `KnowledgeEntity::Character`; a Story character is represented only by `KnowledgeEntity::Role(RoleId)`.
- Supersede the `CharacterId` fields in the Character Decision and StoryStateExtractor specs with the exact RoleId contracts in this document.
- Rebuild runtime persistence through `0016_character_role_runtime.sql` after Phase 0 migration `0015`.
- Rename active limits, error codes, trace fields, tests, and fixtures that refer to runtime characters as separately keyed Character instances.

### 2.2 Non-Goals

- Does not change Character Card or Story Pack asset shapes beyond Phase 0.
- Does not redesign stage-specific Runtime Context text, Prompt profile selection, or CSI/RC/FTI prose; Phase 2 owns those changes.
- Does not add dynamic Role creation during a Turn. A future validated Proposal may add it under a separate lifecycle design.
- Does not persist Character Decisions, CharacterThink output, Planner hypotheses, or Prompt projections.
- Does not place Memory inside `StoryRoleState`; Memory remains authoritative Knowledge owned by a `RoleId`.
- Does not make Role background automatically visible to CharacterThink.
- Does not preserve a separate player Role/Character column in `stories`; Controller inside the persisted Role aggregate is authoritative.
- Does not add multiplayer orchestration. The current runtime still requires exactly one player-controlled Role.
- Does not preserve runtime data written between migrations `0015` and `0016`; the suite is deployed atomically and `0016` rejects such a mid-state.

### 2.3 Implementation Constraints

- Phase 0 contracts and migration `0015` are prerequisites. This phase and Phase 2 remain part of the same atomic hard refactor.
- The StoryStateExtractor split spec, including migration `0014`, is a prerequisite. Where it names `CharacterId`, `character_states`, `present_character_ids`, or `source_character_id`, this spec provides the final superseding contract.
- The Character Decision spec is a prerequisite. Keep its decision semantics and lifecycle; change only story-local identity fields from `CharacterId` to `RoleId` as specified here.
- Delete old code and columns in the same change. No aliases, dual maps, fallback lookup by name, adapter DTOs, or conversion branches may remain (`R-REFACTOR-01`, `R-REFACTOR-02`).
- `StoryReadSnapshot` owns a bounded immutable view for one revision. It must not retain a Character Card Store handle or read the Character library during a Turn (`R-ARCH-02`, `R-ARCH-04`).
- `TurnRuntime` remains the only Pipeline orchestrator; Pipelines communicate only through `TurnExecutionContext` (`R-AISE-01`, `R-AISE-02`, `R-AISE-03`).
- Persistence ports remain adapter-independent; Domain and Turn modules must not import SQLite or API types (`R-LAYER-01`, `R-LAYER-04`, `R-LAYER-06`).
- Every collection and serialized Role aggregate is bounded before Store I/O. No operation scans an unbounded Character Card library or all Stories.

---

## 3. Contracts

### 3.1 Story Role Aggregate

Delete `crates/aise/src/domain/story_instance/binding.rs`. Add `crates/aise/src/domain/story_instance/role.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "player_id", rename_all = "snake_case", deny_unknown_fields)]
pub enum RoleController {
    Player(PlayerId),
    Ai,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryRoleState {
    pub location: LocationKey,
    pub goals: Vec<BoundedText>,
    #[serde(default)]
    pub attributes: BTreeMap<AttributeKey, ScalarValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryRole {
    pub role_id: RoleId,
    pub controller: RoleController,
    pub role_label: BoundedText,
    pub narrative_function: BoundedText,
    pub background: Option<BoundedText>,
    pub effective_profile: CharacterProfile,
    pub source_character: Option<FrozenCharacterCardRef>,
    pub state: StoryRoleState,
}

impl StoryRole {
    pub fn is_player_controlled(&self) -> bool;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoryRoleView {
    pub role_id: RoleId,
    pub controller: RoleController,
    pub role_label: BoundedText,
    pub narrative_function: BoundedText,
    pub background: Option<BoundedText>,
    pub effective_profile: CharacterProfile,
    pub source_character_id: Option<CharacterId>,
    pub state: StoryRoleState,
}

impl From<&StoryRole> for StoryRoleView;
```

Ownership rules:

- `StoryRole` is the persisted Story Instance aggregate. `StoryRoleView` is the immutable Snapshot projection used by a Turn.
- `source_character` stores the exact Character Card ID, version, and digest used at instantiation. It is metadata only and never owns Story state.
- `StoryRoleView` exposes only optional `source_character_id`; Card version/digest remain persistence provenance and are not required by Turn Pipelines.
- `role_label`, `narrative_function`, `background`, and Effective Profile are copied into the aggregate during instantiation so the instance is replayable without reading mutable libraries.
- `StoryRoleState` contains no Role ID, Profile, Memory, Relationship, Controller, or Background.
- Delete `CharacterInstanceState`, `RoleBinding`, `StoryInstanceBinding`, `character_id_for_role`, and all binding lookup helpers.

### 3.2 Instance Creation and Profile Selection

```rust
#[derive(Debug, Clone)]
pub struct CreateStoryInstanceSpec {
    pub pack_id: PackId,
    pub player_id: PlayerId,
    pub player_role_id: RoleId,
    pub role_profile_selections: BTreeMap<RoleId, FrozenCharacterCardRef>,
    pub created_at_ms: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum StoryInstantiationError {
    #[error("story pack was not found")]
    PackNotFound,
    #[error("story role was not found: {role_id}")]
    RoleNotFound { role_id: RoleId },
    #[error("story role is not playable: {role_id}")]
    RoleNotPlayable { role_id: RoleId },
    #[error("character card was not found")]
    CharacterCardNotFound,
    #[error("character card reference does not match stored content")]
    CharacterCardReferenceMismatch,
    #[error("story role profile selection is duplicated: {role_id}")]
    DuplicateRoleProfileSelection { role_id: RoleId },
    #[error("story materialization reference is invalid: {code}")]
    InvalidReference { code: &'static str },
    #[error("story instantiation limit exceeded: {limit}")]
    LimitExceeded { limit: &'static str },
    #[error("story store operation failed")]
    Store(StoreError),
}
```

`StoryInstanceFactory::create` must execute this exact resolution:

1. Load the immutable v4 Pack.
2. Validate `player_role_id` exists and is included in `play.playable_role_ids`.
3. Validate every `role_profile_selections` key identifies an existing Role and the map count is at most Pack Role count and `max_roles`.
4. Load selected Character Cards sequentially through exact `FrozenCharacterCardRef`; do not issue unbounded concurrent Store calls.
5. For each Role in ascending `RoleId` order:
   - use the selected Card Profile when a selection exists;
   - otherwise use the Role Definition `default_profile`;
   - copy the selected Profile as a whole;
   - never copy any missing field from the other Profile;
   - set `source_character` to the exact Card reference only for the selected-Card case;
   - copy Role Definition story fields and initial state;
   - assign `Player(player_id)` only to `player_role_id`, otherwise `Ai`.
6. Materialize relationships and seed Memories directly by `RoleId`.
7. Build `CurrentScene.present_role_ids` from Roles whose initial location equals `start.location_key`.
8. Persist all Roles, relationships, scene, knowledge, Narrative state, constraints, and opening atomically.

After step 5, creation and all future Turns must not read the Character Card Store for those Roles.

### 3.3 Scene and Relationship Contracts

Replace the Role-related shapes in `domain/story_instance/state.rs` with:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentScene {
    pub scene_key: SceneKey,
    pub location_key: LocationKey,
    pub time: BoundedText,
    pub description: BoundedText,
    pub present_role_ids: Vec<RoleId>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RelationshipKey {
    pub source_role_id: RoleId,
    pub target_role_id: RoleId,
    pub kind: RelationshipKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipState {
    pub source_role_id: RoleId,
    pub target_role_id: RoleId,
    pub kind: RelationshipKind,
    pub trust: i16,
}
```

- `present_role_ids` is sorted ascending and duplicate-free at every committed boundary.
- Relationship identity remains directed and keyed by `(source_role_id, target_role_id, kind)`.
- Self-relationships retain the existing project policy; this change does not silently alter it.

### 3.4 Story Snapshot

```rust
#[derive(Debug, Clone)]
pub struct StoryReadSnapshot {
    story_id: StoryId,
    base_revision: StoryRevision,
    pack: FrozenStoryPackRef,
    story_profile: StoryProfile,
    instance_settings: InstanceSettings,
    roles: BTreeMap<RoleId, StoryRoleView>,
    player_role_id: RoleId,
    current_scene: CurrentScene,
    relationships: Vec<RelationshipState>,
    narrative_definition: NarrativeGraphDefinition,
    narrative_state: NarrativeRuntimeState,
    condition_state: NarrativeConditionStateView,
    story_continuity: StoryContinuity,
    active_constraints: Vec<ActiveStoryConstraint>,
    entity_catalog: Vec<KnowledgeEntity>,
    topic_dictionary: BTreeMap<TopicKey, TopicDefinition>,
    knowledge_snapshot: KnowledgeSnapshotRef,
}

impl StoryReadSnapshot {
    pub fn roles(&self) -> &BTreeMap<RoleId, StoryRoleView>;
    pub fn role(&self, role_id: &RoleId) -> Option<&StoryRoleView>;
    pub fn player_role_id(&self) -> &RoleId;
    pub fn player_role(&self) -> &StoryRoleView;
}
```

All existing non-character Snapshot accessors remain unchanged.

Delete these fields and accessors:

```text
role_definitions
role_bindings
character_cards
character_states
current_perceptions
role_binding(...)
role_bindings()
character_cards()
character_states()
current_perceptions()
```

`try_from_parts` validates, in order:

1. Knowledge Snapshot Story ID, Pack digest, and base revision.
2. Role count and aggregate byte limits.
3. Every Role map key equals `StoryRoleView.role_id`.
4. Exactly one Role is player-controlled; store its key as `player_role_id` and use that validated key for both player accessors.
5. Every `present_role_ids` value exists and the list is sorted/unique.
6. Every Relationship endpoint exists and every Relationship key is unique.
7. Every Role entity in `entity_catalog` resolves to a Role; no Character entity variant exists.
8. Narrative and continuity invariants already owned by the Snapshot.

Use exact inconsistency codes:

```text
role_map_key_mismatch
player_role_count
scene_role_order
scene_role_missing
relationship_role_missing
relationship_duplicate
entity_role_missing
```

### 3.5 Knowledge Identity and Ownership

Replace story-local Character identity throughout Knowledge:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "key", rename_all = "snake_case", deny_unknown_fields)]
pub enum KnowledgeEntity {
    World(EntityKey),
    Role(RoleId),
    Location(LocationKey),
    Scene(SceneKey),
    NarrativeNode(NarrativeNodeKey),
    Event(CanonicalEventKey),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryEntry {
    pub id: MemoryId,
    pub owner: RoleId,
    pub kind: MemoryKind,
    pub content: BoundedText,
    pub entities: Vec<KnowledgeEntity>,
    pub topics: Vec<TopicKey>,
    pub salience: u8,
    pub source: KnowledgeSource,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedRumor {
    pub id: RumorId,
    pub key: Option<RumorKey>,
    pub content: BoundedText,
    pub claim: Option<Claim>,
    pub entities: Vec<KnowledgeEntity>,
    pub topics: Vec<TopicKey>,
    pub salience: u8,
    pub source_role_id: Option<RoleId>,
    pub truth_value: TruthValue,
    pub source: KnowledgeSource,
}
```

This shape assumes the StoryStateExtractor split already removed Knowledge `story_revision` and `event_id` provenance.

Rules:

- Delete `KnowledgeEntity::Character`, `SharedRumor.source_role_key`, and `SharedRumor.source_character_id`.
- `KnowledgeEntry::memory_owner()` returns `Option<&RoleId>`.
- Seed Memory owner is its containing Role map key.
- A Rumor source, if known, is one `RoleId`; it is not duplicated as Role key plus Character ID.
- Canonical Memory entity lists include `KnowledgeEntity::Role(owner)` exactly once.
- Character Card `CharacterId` never appears in Knowledge, retrieval entities, propositions, claims, or memory ownership.

### 3.6 Retrieval and Planning Contracts

```rust
impl RetrievalTargetId {
    pub fn for_role(role_id: &RoleId) -> Self;
}

pub enum RetrievalAudience {
    GlobalWriter,
    Character { role_id: RoleId },
}

pub struct RetrievalRequest {
    pub audience: RetrievalAudience,
    pub target_source_id: Option<KnowledgeSourceId>,
    pub knowledge_kinds: Vec<KnowledgeKind>,
    pub entities: Vec<KnowledgeEntity>,
    pub topics: Vec<TopicKey>,
    pub query_text: Option<BoundedText>,
    pub authorized_memory_owners: Vec<RoleId>,
    pub reason: BoundedText,
    pub origin: RetrievalRequestOrigin,
    pub signal_priority: u8,
}

pub struct CharacterThinkRequest {
    pub role_id: RoleId,
    pub reason: BoundedText,
}

pub struct RetrievalSignals {
    pub scene_key: SceneKey,
    pub location_key: LocationKey,
    pub present_role_ids: Vec<RoleId>,
    pub entities: Vec<EntitySignal>,
    pub topics: Vec<TopicSignal>,
}

pub struct RetrievedContext {
    writer: Vec<ContextItem>,
    roles: BTreeMap<RoleId, Vec<ContextItem>>,
}

impl RetrievedContext {
    pub fn for_role(&self, role_id: &RoleId) -> &[ContextItem];
    pub fn roles(&self) -> &BTreeMap<RoleId, Vec<ContextItem>>;
}
```

- `RetrievalTargetId::for_role` renders `role:{role_id}`. Delete `for_character` and the `character:` prefix.
- JSON audience shape remains semantically named `character` but uses `role_id`:

```json
{ "kind": "character", "role_id": "protagonist" }
```

- `RetrievedContextLimits.max_character_audiences` becomes `max_role_audiences`.
- Character-scoped Fact rejection and owner-specific Memory checks compare `RoleId`.
- `authorized_memory_owners` contains Role IDs only.
- Names are never accepted as a retrieval target or audience.

### 3.7 Character Decision and CharacterThink Binding Override

The final types from the Character Decision spec become:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CharacterDecision {
    pub role_id: RoleId,
    pub decision: BoundedText,
    pub suggested_utterance: Option<BoundedText>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CharacterDecisionOutput {
    pub decision: BoundedText,
    pub suggested_utterance: Option<BoundedText>,
}
```

- The model still returns no ID. The engine binds `CharacterDecision.role_id` from the validated `CharacterThinkRequest.role_id`.
- All count/order/duplicate/player-exclusion rules from the Character Decision spec compare `RoleId`.
- Character Decisions remain Turn-local and never persist.
- Type names retain “Character” because CharacterThink is a behavior stage; only the identity field changes.
- `CharacterDecision` drops `Deserialize` because the engine only ever constructs it from a validated `role_id` plus normalized model output; it is never deserialized directly. `PartialEq, Eq` are added to support the ordering/duplicate assertions required by §3.4 Snapshot and Store tests.
- The directory module from the Character Decision spec (`crates/aise/src/domain/turn/character/mod.rs` and `character/decision.rs`) is retained unchanged; this phase only edits the `character_id` field inside `decision.rs` to `role_id`. Do not flatten the module back into a sibling `character_decision.rs` file (`R-CODE-01`).

### 3.8 StoryStateExtractor Override

The final extractor model boundary becomes:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryStateExtractorOutput {
    pub role_states: Vec<ExtractedRoleState>,
    pub relationship_states: Vec<RelationshipState>,
    pub knowledge_changes: Vec<ProposedKnowledgeMutation>,
    pub current_scene: CurrentScene,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractedRoleState {
    pub role_id: RoleId,
    pub location: LocationKey,
    pub goals: Vec<BoundedText>,
    #[serde(default)]
    pub attributes: BTreeMap<AttributeKey, ScalarValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProposedKnowledgeValue {
    Fact {
        content: BoundedText,
        proposition: Option<Proposition>,
        entities: Vec<KnowledgeEntity>,
        topics: Vec<TopicKey>,
        salience: u8,
    },
    Rumor {
        content: BoundedText,
        claim: Option<Claim>,
        entities: Vec<KnowledgeEntity>,
        topics: Vec<TopicKey>,
        salience: u8,
        source_role_id: Option<RoleId>,
        truth_value: TruthValue,
    },
    Memory {
        owner: RoleId,
        memory_kind: MemoryKind,
        content: BoundedText,
        entities: Vec<KnowledgeEntity>,
        topics: Vec<TopicKey>,
        salience: u8,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExtractorKnowledgeEntry {
    pub source_id: KnowledgeSourceId,
    pub kind: KnowledgeKind,
    pub content: BoundedText,
    pub memory_owner: Option<RoleId>,
    pub salience: u8,
}
```

- Rename `character_states` to `role_states` and `ExtractedCharacterState` to `ExtractedRoleState`.
- Every extractor `character_id`, Memory owner Character ID, Rumor source Character ID, Relationship Character ID, and Scene present Character ID is replaced by the exact Role fields above.
- The final-state, changed-only, Knowledge mutation, provenance, validation/repair, and no-perception semantics from the extractor spec remain unchanged.
- `ValidatedChangeSet.character_state_changes` becomes `role_state_changes`; each change key is `RoleId` and immutable Role/Profile fields are copied from Snapshot, never model output.

### 3.9 Narrative Graph Role References

Replace Role fields in Narrative DTOs and runtime values:

```rust
pub enum NarrativeCondition {
    RoleStateEquals {
        role_id: RoleId,
        attribute: BoundedText,
        value: ScalarValue,
    },
    RelationshipReaches {
        source_role_id: RoleId,
        target_role_id: RoleId,
        minimum_trust: i16,
    },
    RoleControllerIs {
        role_id: RoleId,
        controller: RoleControllerKind,
    },
}

pub struct CharacterImpulseDefinition {
    pub target_role_id: RoleId,
    pub goal: BoundedText,
    pub reason: Option<BoundedText>,
    pub emotion: Option<BoundedText>,
    pub urgency: ImpulseUrgency,
    pub valid_for_turns: Option<NonZeroU32>,
}

pub struct CharacterImpulse {
    pub source_node: NarrativeNodeKey,
    pub target_role_id: RoleId,
    pub goal: BoundedText,
    pub reason: Option<BoundedText>,
    pub emotion: Option<BoundedText>,
    pub urgency: ImpulseUrgency,
    pub expires_after_turn: Option<u64>,
}
```

All existing non-role Narrative condition variants remain unchanged.

- Delete `CharacterStateEquals`, `target_role_key`, and `target_character_id`.
- `NarrativeDirector` calls `snapshot.role(role_id)` directly. It performs no Binding or Character State join.
- Player-controlled disposition uses `StoryRoleView.controller`.
- Relationship conditions compare RoleId endpoints directly.

### 3.10 Prepared Context Domain Types

Phase 1 prepares the data model that Phase 2 renders:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct RoleContextView {
    pub role_id: RoleId,
    pub role_label: BoundedText,
    pub narrative_function: BoundedText,
    pub background: Option<BoundedText>,
    pub profile: CharacterProfile,
    pub state: StoryRoleState,
    pub controller: RoleController,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoleIndexEntry {
    pub role_id: RoleId,
    pub name: BoundedText,
    pub role_label: BoundedText,
    pub narrative_function: BoundedText,
    pub location_key: LocationKey,
    pub player_controlled: bool,
}

pub struct BaselineContext {
    pub story_profile: StoryProfile,
    pub instance_settings: InstanceSettings,
    pub player_role: RoleContextView,
    pub current_scene: CurrentScene,
    pub scene_roles: Vec<RoleContextView>,
    pub referenced_roles: Vec<RoleContextView>,
    pub relevant_knowledge: Vec<RelevantKnowledge>,
    pub role_index_scope: RetrievalIndexScope,
    pub knowledge_entry_index_scope: RetrievalIndexScope,
    pub knowledge_entry_index: Vec<KnowledgeEntryIndexEntry>,
    pub role_index: Vec<RoleIndexEntry>,
    pub story_continuity: StoryContinuity,
    pub active_story_constraints: Vec<ActiveStoryConstraint>,
    pub narrative_state_view: NarrativeStateView,
    pub retrieval_signals: RetrievalSignals,
}
```

`BaselineContextBuilder` obtains all Role data from `snapshot.roles()` with no secondary lookup. Player, scene, referenced, and index partitions are duplicate-free and ordered by `RoleId`.

### 3.11 Store Contracts

```rust
#[derive(Debug, Clone)]
pub struct MaterializedStoryInstanceSpec {
    pub story_id: StoryId,
    pub pack: FrozenStoryPackRef,
    pub settings: InstanceSettings,
    pub roles: BTreeMap<RoleId, StoryRole>,
    pub relationships: Vec<RelationshipState>,
    pub knowledge: Vec<KnowledgeEntry>,
    pub scene: CurrentScene,
    pub opening: BoundedText,
    pub narrative_state: NarrativeRuntimeState,
    pub condition_state: NarrativeConditionStateView,
    pub active_constraints: Vec<ActiveStoryConstraint>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone)]
pub struct StoryInstanceMeta {
    pub pack_id: PackId,
    pub roles: BTreeMap<RoleId, StoryRole>,
}
```

- `bindings` and `characters` fields are deleted from both types.
- `Store::create_story_instance` writes one `roles_json` value.
- `Store::load_story_instance_meta` returns one Role map.
- `commit_turn` updates only `StoryRole.state`; Profile, Controller, source Card, background, label, and Narrative function are immutable.
- A Role-state change whose key differs from `new_state.role_id` is impossible because `StoryRoleState` contains no ID; Store looks up the aggregate by the change `RoleId` and replaces only its `state`.
- Relationship and Knowledge writes use Role IDs.

### 3.12 HTTP Story Runtime Contract

Replace the request/response fields in `crates/aise-server/src/api/story.rs`:

```rust
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateStoryInstanceRequest {
    pub pack_id: String,
    pub player_id: String,
    pub player_role_id: String,
    #[serde(default)]
    pub role_profiles: Vec<RoleProfileSelectionRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleProfileSelectionRequest {
    pub role_id: String,
    pub character_id: String,
    pub version: String,
    pub digest: String,
}

#[derive(Debug, Serialize)]
pub struct StoryInstanceView {
    pub story_id: String,
    pub base_revision: u64,
    pub pack_id: String,
    pub player_role_id: String,
    pub current_scene: String,
    pub opening: StoryOpeningView,
}

#[derive(Debug, Serialize)]
pub struct StoryView {
    pub story_id: String,
    pub base_revision: u64,
    pub premise: String,
    pub current_scene: String,
    pub player_role_id: String,
    pub opening: Option<StoryOpeningView>,
    pub turns: Vec<StoryTurnView>,
    pub next_turn_after: Option<u64>,
    pub roles: Vec<RoleStateView>,
}

#[derive(Debug, Serialize)]
pub struct RoleStateView {
    pub role_id: String,
    pub name: String,
    pub source_character_id: Option<String>,
    pub location: String,
    pub goals: Vec<String>,
    pub attributes: Vec<AttributeView>,
}
```

- Duplicate `role_profiles.role_id` values return `400 Bad Request` before Store access.
- Invalid UUID, Role ID, SemVer, or digest returns `400 Bad Request`.
- Missing exact Card returns `422 Unprocessable Entity`; missing Pack returns `404`.
- `player_character_id`, `player_role_key`, `character_id` as a runtime Role ID, `role_key`, and `characters` are removed from Story endpoints.
- `source_character_id` is optional provenance and never used to target a Story Role.

### 3.13 Configuration Renames

Apply these hard renames with no aliases:

| Old | New |
|---|---|
| `content.max_characters` | `content.max_roles` |
| `content.max_character_bytes` | `content.max_role_bytes` |
| `context.max_scene_characters` | `context.max_scene_roles` |
| `context.max_character_index` | `context.max_role_index` |
| `RetrievedContextLimits.max_character_audiences` | `max_role_audiences` |
| `SnapshotLimits.max_characters` | `max_roles` |
| `SnapshotLimits.max_character_bytes` | `max_role_bytes` |
| `SnapshotLimits.max_scene_characters` | `max_scene_roles` |

Defaults remain numerically unchanged except `content.max_role_bytes`, which becomes `131072`. Cross-config validation requires:

```text
content.max_role_bytes
>= assets.max_profile_total_bytes
 + assets.max_role_background_bytes
 + assets.max_text_bytes
```

Failure uses the exact startup error `content.max_role_bytes is smaller than the configured role aggregate bounds`. Every actual compact serialized `StoryRole` is also checked against `max_role_bytes` during creation, Snapshot loading, and commit.

### 3.14 SQLite Migration 0016

Add `crates/aise/assets/persistence/mig/0016_character_role_runtime.sql`.

The migration must:

1. Assert the Phase 0 `character_cards` table and v4 `story_packs` schema exist.
2. Abort through named constraint `character_role_runtime_midstate_data_present` if `story_instances`, `knowledge_entries`, `story_turns`, or `story_segments` contains rows. This detects an invalid deployment that served traffic between phases.
3. Rebuild `stories` without `player_character_id`; preserve revision, timestamps, current scene, Summary, and constraints columns.
4. Rebuild `story_instances` with:

```sql
CREATE TABLE story_instances (
    story_id             TEXT PRIMARY KEY REFERENCES stories(id) ON DELETE CASCADE,
    pack_id              TEXT NOT NULL REFERENCES story_packs(pack_id),
    settings_json        TEXT NOT NULL CHECK (json_valid(settings_json)),
    roles_json           TEXT NOT NULL CHECK (json_valid(roles_json)),
    relationships_json   TEXT NOT NULL CHECK (json_valid(relationships_json)),
    narrative_state_json TEXT NOT NULL CHECK (json_valid(narrative_state_json)),
    condition_state_json TEXT NOT NULL CHECK (json_valid(condition_state_json)),
    created_at_ms        INTEGER NOT NULL
);
```

5. Rebuild `knowledge_entries`, renaming `memory_owner_character_id` to `memory_owner_role_id` and retaining the Memory/non-Memory check.
6. Rebuild `knowledge_entry_entities` so the only Story-character entity kind is `role`; `character` is not accepted.
7. Preserve the final active/inactive Knowledge columns introduced by migration `0014`.
8. Remove `bindings_json`, `characters_json`, and every Character-instance column/index.
9. Run `PRAGMA foreign_key_check` and drop the migration guard.

No SQL column stores both RoleId and instance CharacterId for the same subject.

### 3.15 File and Directory Layout

```text
crates/aise/src/
├── domain/
│   ├── ids.rs
│   ├── asset/
│   │   ├── entity.rs
│   │   └── story_pack.rs
│   ├── knowledge/
│   │   ├── entry.rs
│   │   ├── memory.rs
│   │   └── rumor.rs
│   ├── narrative_graph/
│   │   ├── definition.rs
│   │   ├── director.rs
│   │   └── effect.rs
│   ├── story_instance/
│   │   ├── role.rs
│   │   ├── snapshot.rs
│   │   └── state.rs
│   └── turn/
│       ├── baseline.rs
│       ├── character/
│       │   ├── mod.rs
│       │   └── decision.rs
│       ├── planning.rs
│       ├── retrieval.rs
│       └── state_extraction.rs
├── context/
│   ├── baseline_ctx_builder.rs
│   ├── retrieval_pipeline.rs
│   └── retrieval_signal_builder.rs
├── story/
│   └── instance_factory.rs
└── persistence/
    ├── knowledge_read_port.rs
    ├── sqlite_knowledge_reader.rs
    ├── sqlite_snapshot.rs
    ├── sqlite_store.rs
    └── store.rs

crates/aise/assets/persistence/mig/
└── 0016_character_role_runtime.sql
```

`domain/story_instance/binding.rs` is deleted and removed from the index.

---

## 4. Behavior Rules

1. **CRP1-ROLE-01**: One Story Instance Role has one `RoleId`, one Controller, one frozen Effective Profile, one Role state, and at most one frozen Character Card source.
2. **CRP1-ROLE-02**: Every Story-local reference targets `RoleId`; global `CharacterId` appears only in Character Card storage and optional Role source provenance.
3. **CRP1-ROLE-03**: Name matching, Role-label matching, positional matching, and fallback targeting are prohibited.
4. **CRP1-PROFILE-01**: Selected Card Profile replaces the complete default Profile; no field is copied from the default Profile in that case.
5. **CRP1-PROFILE-02**: No selection uses a frozen copy of the complete default Profile and stores no source Character ID.
6. **CRP1-PROFILE-03**: Editing or importing a later Character Card version cannot change an existing StoryRole.
7. **CRP1-SNAPSHOT-01**: Snapshot Role data comes from one `roles` map; no caller joins Role definitions, bindings, Cards, and state maps.
8. **CRP1-SNAPSHOT-02**: Exactly one Role is player-controlled; zero or multiple player Roles fail Snapshot creation.
9. **CRP1-KNOW-01**: Memory ownership and Rumor source use RoleId; Character Card identity never grants knowledge.
10. **CRP1-KNOW-02**: Role background does not become Memory, Fact, Rumor, or CharacterThink-visible data merely because it exists.
11. **CRP1-NARR-01**: Narrative conditions and impulses resolve RoleId directly and never resolve through Binding.
12. **CRP1-EXTRACT-01**: StoryStateExtractor output uses RoleId for every Story character target and contains no Character Card ID.
13. **CRP1-CTX-01**: Character Decision and retrieval collections preserve validated request order while binding targets by RoleId.
14. **CRP1-STORE-01**: Commit may replace only mutable Role state; Profile, source Card, Controller, Role label, Narrative function, and background are immutable.
15. **CRP1-STORE-02**: Story creation and Turn commit remain atomic and revision-checked.
16. **CRP1-MIG-01**: No traffic may be served between migrations `0015` and `0016`; mid-state data causes the exact named migration failure.

### 4.1 Error Handling

- Unknown Role references fail with a typed error containing structured `role_id`; error text must not include Profile, background, Memory, or player input.
- Missing selected Card and exact-reference mismatch fail Story creation; the factory never falls back to the Role default Profile after a Card was explicitly selected.
- Snapshot inconsistency returns `StorySnapshotError::Inconsistent { code }` using §3.4 codes.
- Model-supplied invalid/unknown `role_id` fails validation before any state mutation or commit.
- Store serialization errors use `InvalidRoleState` in place of `InvalidCharacterState`; no `anyhow::Error` leaks from Domain/Store ports.

### 4.2 Concurrency

- Selected Card loads are sequential and bounded by `max_roles`; no unbounded `join_all`, spawned tasks, channels, or hidden queue is introduced.
- Every LLM call still uses the shared `LlmGateway`; this identity refactor adds no direct provider call.
- No lock guard is held across Card/Pack/Story Store I/O, LLM calls, events, or channel sends.
- Snapshot Role maps are immutable for the Turn; state changes become visible only after atomic commit and the next Snapshot load.

### 4.3 Observability

- Replace runtime structured fields named `character_id` or `role_key` with `role_id` in instance creation, Snapshot, retrieval, CharacterThink, Story generation, extraction, validation, and commit spans.
- Retain `character_id` only for Character Card import/lookup and optional source provenance events.
- Instance creation records Role count, selected Card count, player Role ID, status, error code, and latency; it never records Profile/background text.
- Snapshot and retrieval traces use `role_count`, `scene_role_count`, `role_audience_count`, and bounded counts/tokens.
- Production logs must not serialize `StoryRole`, `CharacterProfile`, `CharacterDecision`, Memory content, or player input.

---

## 5. Acceptance Criteria

### Aggregate and Instantiation

- [ ] `StoryRole`, `StoryRoleState`, `StoryRoleView`, and `RoleController` match §3.1.
- [ ] `domain/story_instance/binding.rs`, `RoleBinding`, `StoryInstanceBinding`, and `CharacterInstanceState` are deleted.
- [ ] Default, selected-player, selected-AI, multiple-selected, unknown Role, wrong Card digest, and duplicate-selection cases have tests.
- [ ] Selected Card Profile is copied as a whole; optional fields absent in the Card remain absent even when present in the default Profile.
- [ ] Editing/importing a later Card version leaves a previously created Story Snapshot byte-equivalent for Role Profile data.

### Snapshot and Runtime References

- [ ] `StoryReadSnapshot` stores exactly one `BTreeMap<RoleId, StoryRoleView>` and no parallel Role/Card/Binding/state maps.
- [ ] Snapshot map-key, player count, scene order/existence, Relationship, entity, limit, and Knowledge invariants have dedicated tests.
- [ ] Scene, Relationship, Memory, Rumor, Knowledge entity, Narrative condition/impulse, retrieval audience, CharacterThink request/decision, extractor, validation, and commit fields use `RoleId`.
- [ ] Duplicate display names do not affect any resolution or validation path.
- [ ] Character Card `CharacterId` never appears in model-owned StoryStateExtractor schema.

### Store, Migration, and API

- [ ] `MaterializedStoryInstanceSpec` and `StoryInstanceMeta` match §3.11.
- [ ] SQLite writes and reads one `roles_json`; no Binding or Character-instance JSON remains.
- [ ] `memory_owner_role_id` is the only persisted Memory-owner column.
- [ ] Migration `0016` applies after `0015` on a fresh database and `PRAGMA foreign_key_check` is empty.
- [ ] Migration `0016` fails with `character_role_runtime_midstate_data_present` if traffic-created rows exist after `0015`.
- [ ] Story API request/response JSON matches §3.12 and rejects every removed field through `deny_unknown_fields` where applicable.
- [ ] API returns Role list ordered by `RoleId` and identifies the player by `player_role_id` only.

### Prerequisite-Spec Overrides

- [ ] Character Decision keeps the earlier spec semantics but binds `role_id` rather than `character_id`.
- [ ] StoryStateExtractor top-level field is `role_states`; all final state, Relationship, Knowledge, and Scene targets are Role IDs.
- [ ] StoryStateExtractor remains perception-free and proposal-event-free as required by its source spec.
- [ ] No active source or test reintroduces the superseded CharacterId contracts from those documents.

### Hard-Refactor Verification

- [ ] `rg -n 'RoleBinding|StoryInstanceBinding|CharacterInstanceState|role_bindings|character_states' crates/aise/src crates/aise-server/src` returns zero matches.
- [ ] `rg -n 'StoryRoleKey|role_key|present_character_ids|target_character_id|memory_owner_character_id' crates/aise/src crates/aise-server/src crates/aise/assets/persistence/mig config` returns zero legacy runtime matches.
- [ ] `rg -n 'KnowledgeEntity::Character|"character".*entity|entity_kind.*character' crates/aise/src crates/aise/tests crates/aise/assets/persistence/mig` returns zero matches.
- [ ] `cargo fmt --all --check` passes.
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo test --workspace` passes.

---

## 6. Required Tests

### 6.1 Story Role Resolution Matrix

| Case | Effective Profile | Source |
|---|---|---|
| no selection | exact Role `default_profile` | `None` |
| player Card selected | exact stored Card Profile | exact frozen Card ref |
| AI Card selected | exact stored Card Profile | exact frozen Card ref |
| selected Card omits appearance | `appearance = None` | exact frozen Card ref; no fallback |
| selected Card missing/wrong digest | Story creation fails | no Story rows |
| two Roles use same Card | two frozen Profile copies | same source Character ID allowed |
| duplicate names | independent Roles | resolved only by RoleId |

### 6.2 Snapshot and Identity Tests

Test map-key mismatch, zero/two player Roles, duplicate/missing scene Role, duplicate/missing Relationship endpoint, missing Role entity, Role count/byte limits, deterministic ordering, and Snapshot independence from later Character Card updates.

### 6.3 Knowledge and Retrieval Tests

Test owner-authorized Memory, other-Role Memory rejection, writer Memory access under existing policy, Rumor source Role, no Character entity variant, `role:` target IDs, `{kind: character, role_id}` round-trip, and duplicate-name retrieval.

### 6.4 Narrative Tests

Test Role state equality, Relationship threshold, Controller check, AI impulse production, player impulse not-applicable disposition, unknown Role failure, and the absence of Binding lookup.

### 6.5 Extractor/Decision/Commit Tests

Test exact Role binding, unknown/duplicate/player CharacterThink request rejection, Role-state changed-only semantics, Relationship final state, Memory/Rumor Role targets, Scene final-state ordering, immutable Role fields across commit, and repair/re-extraction reuse of the same Role IDs.

### 6.6 API and Migration Tests

Test default-only creation, per-Role Card selections, malformed UUID/Role/digest, duplicate selection, Story read output, removed-field rejection, fresh 0015→0016 migration, mid-state guard failure, and foreign-key integrity.

---

## 7. Out of Scope / Future Work

- Dynamic incidental Role creation requires a validated Role-definition/profile/state creation contract and bounded cleanup policy.
- Multiplayer requires multiple player Controllers and a revised input/turn protocol.
- Exposing Card version/digest provenance in admin APIs is separate from the player-facing Story API.
- Persisted Role profile editing would break replayability and is intentionally not planned.

---

## 8. References

- Source design: `doc/design/CSI-RC-FTI/2026-08-14-character-role-profile-design-gpt.md`.
- Phase 0: `doc/exec/CSI-RC-FTI/2026-08-14-character-role-profile-spec-phase-0-gpt.md`.
- Phase 2: `doc/exec/CSI-RC-FTI/2026-08-14-character-role-profile-spec-phase-2-gpt.md`.
- Character Decision contract overridden here only for identity: `doc/exec/CSI-RC-FTI/2026-08-14-character-think-decision-spec-gpt.md`.
- StoryStateExtractor contract overridden here only for identity and Role naming: `doc/exec/CSI-RC-FTI/2026-08-14-story-state-extractor-split-spec-gpt.md`.
- Current Role Binding: `crates/aise/src/domain/story_instance/binding.rs:13`.
- Current Snapshot joins: `crates/aise/src/domain/story_instance/snapshot.rs:39`.
- Current factory identity generation: `crates/aise/src/story/instance_factory.rs:124`.
- Current Store aggregate: `crates/aise/src/persistence/store.rs:60`.
- Current Story API: `crates/aise-server/src/api/story.rs:16`.
- Project guardrails: `AGENTS.md` and `doc/agents/guardrails/`.
