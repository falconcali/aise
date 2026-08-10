# Story Pack v3 — Spec

> **Model**: GPT-5
> **Date**: 2026-08-07
> **Status**: Proposed
> **Source Design**: [AISE Story Pack Design v3.0](../design/2026-08-06-StoryPackDesign-gpt.md)
> **Phase**: N-A

---

## 1. Goal

Implement AISE-native Story Pack v3 assets, immutable Story templates, role-to-character instantiation, isolated knowledge, bounded Narrative Graph execution, and trusted Prompt integration as one final-form Turn Runtime path.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Define strict `aise_char_v3`, `aise_world_v3`, and `aise_story_v3` DTOs and validation.
- Import and export `story.aise.json` and bounded `story.aise-pack` containers.
- Persist immutable, version-pinned Story Packs and create isolated Story Instances from them.
- Separate `StoryRole`, `CharacterCard`, and immutable `RoleBinding` ownership.
- Replace `story_instructions` and `StoryConfig` with untrusted `StoryProfile` data and trusted engine configuration.
- Model authoritative `Fact`, public `Rumor`, character-owned `Memory`, and transient `CurrentPerception`.
- Add immutable `NarrativeGraphDefinition`, mutable `NarrativeRuntimeState`, typed conditions, and the two permitted effect kinds.
- Integrate Narrative planning, audience-filtered retrieval, AI-only Character Think, Proposal validation, Repair, and atomic commit through the existing eight-stage `TurnRuntime`.
- Route every Pipeline LLM call through a typed `ModelRequest<C>`; only the trusted project `prompt` module may create System messages.
- Replace the ad hoc Story creation HTTP contract with Pack validation/import/export, Story Instance creation, Story read, and save export contracts.
- Add deterministic validation, persistence, API, security, isolation, and end-to-end tests.

### 2.2 Non-Goals

- Does not import, map, convert, or fall back to SillyTavern, Tavern Card, Lorebook, v1, v2, or any other non-v3 asset format.
- Does not preserve legacy `story_instructions`, `StoryConfig`, direct Story creation, old JSON shapes, dual schemas, or dual-write behavior.
- Does not let Story assets select models, Prompt profiles, Prompt assets, tools, skills, retrieval algorithms, budgets, timeouts, validation policy, or concurrency.
- Does not add arbitrary scripts, SQL, regular expressions, templates, macros, plugins, executable expressions, or arbitrary-string Narrative effects.
- Does not let Narrative Graph generate final prose, force character actions, force player actions, call tools, or patch authoritative state.
- Does not make embeddings, indexes, summaries, Character Thoughts, Planner hypotheses, or Proposals authoritative.
- Does not add unbounded Narrative cycles; v3 graphs are DAGs.
- Does not add a separate CLI binary. The service and HTTP contracts in this spec implement the semantics of the commands in the source design.
- Does not add Story save import; v3 in this change exports saves only.
- Does not redesign the fixed `TurnRuntime` stage order, terminal delivery, LLM accounting, or bounded Validation/Repair loop.

### 2.3 Implementation Constraints

- Implement the final form in one change. Do not retain fallback paths, compatibility shims, deprecated constructors, adapters between old and new Story models, dual schemas, or dead flags (`R-REFACTOR-01`, `R-REFACTOR-02`).
- Delete superseded code, configuration, persistence columns, API fields, tests, fixtures, and documentation in the same change.
- Keep ownership inside `domain`, `story`, `context`, `planning`, `character`, `validation`, `persistence`, and `runtime`; inner modules must not import `aise-server` or concrete transport types (`R-AISE-04`, `R-LAYER-01`).
- Keep `TurnRuntime` as the only Pipeline orchestrator. Every Pipeline implements `TurnExecutionPipeline` and exchanges per-Turn data only through `&mut TurnExecutionContext` (`R-AISE-01`, `R-AISE-02`, `R-AISE-03`).
- Keep every collection, text value, archive, Snapshot, Context, graph, query, Proposal, validation issue list, and LLM request bounded by validated engine configuration (`R-ARCH-03`, `R-ARCH-04`).
- Never hold a lock guard across `.await`, event emission, channel send, or I/O (`R-CONC-01`, `R-CONC-03`).
- Route completion, streaming, embedding, Repair, and narrative-validation calls through the single application-owned `LlmGateway` limiter (`R-CONC-04`).
- Keep `mod.rs` and `lib.rs` index-only; use directory modules; place unit tests in `tests/<source>_tests.rs`; add no ordinary code comments; keep imports contiguous (`R-CODE-01`, `R-CODE-02`, `R-CODE-05`, `R-CODE-07`).
- Use typed `thiserror` errors in turn and Domain. Parse, reference, limit, Store, LLM, and I/O failures must be diagnosable and use structured tracing fields (`R-OBS-01`, `R-OBS-04`, `R-OBS-05`).
- Any archive dependency is permitted only for `.aise-pack` processing, must be declared at workspace level, must support Rust 1.85, and must be justified by bounded streaming archive inspection and symlink metadata access (`R-DEP-01`).

### 2.4 Required Implementation Order

1. Native asset DTOs, key types, limits, strict parsing, validation reports, and container safety.
2. Pack repository, immutable Pack import/export, Story Instance factory, role binding, seed materialization, and Story API replacement.
3. Fact/Rumor/Memory separation, one-revision Snapshot loading, audience-filtered retrieval, and bounded Context items.
4. Narrative Graph validation, pure `NarrativeDirector`, Writer Plan integration, impulse dispatch, Proposal changes, validators, and atomic commit.
5. Trusted Prompt boundary and typed `ModelRequest<C>`; delete Pipeline-created System messages and all legacy Story instruction paths.
6. Full static checks, unit tests, integration tests, API tests, security tests, formatting, and linting.

No later item may introduce a compatibility path around an earlier boundary.

---

## 3. Contracts

### 3.1 File and Module Layout

~~~text
crates/aise/src/
├── domain/
│   ├── mod.rs
│   ├── asset/
│   │   ├── mod.rs
│   │   ├── character_card.rs
│   │   ├── frozen_ref.rs
│   │   ├── ids.rs
│   │   ├── story_pack.rs
│   │   ├── validation.rs
│   │   └── world_book.rs
│   ├── knowledge/
│   │   ├── mod.rs
│   │   ├── fact.rs
│   │   ├── memory.rs
│   │   ├── query.rs
│   │   └── rumor.rs
│   ├── narrative_graph/
│   │   ├── mod.rs
│   │   ├── condition.rs
│   │   ├── definition.rs
│   │   ├── director.rs
│   │   ├── effect.rs
│   │   └── state.rs
│   └── story_instance/
│       ├── mod.rs
│       ├── binding.rs
│       ├── snapshot.rs
│       └── state.rs
├── story/
│   ├── mod.rs
│   ├── instance_factory.rs
│   ├── pack_service.rs
│   ├── story_generator.rs
│   └── story_repairer.rs
├── context/
│   ├── mod.rs
│   ├── baseline_ctx_builder.rs
│   ├── context_item.rs
│   └── retrieval_pipeline.rs
├── planning/
│   ├── mod.rs
│   └── writer_planner.rs
├── character/
│   ├── mod.rs
│   └── character_think_pipeline.rs
├── validation/
│   ├── mod.rs
│   ├── validation_pipeline.rs
│   └── validators/
│       ├── mod.rs
│       ├── asset_reference.rs
│       ├── knowledge_boundary.rs
│       ├── narrative_authority.rs
│       └── player_control.rs
├── persistence/
│   ├── mod.rs
│   ├── asset_store.rs
│   ├── store.rs
│   ├── sqlite_asset_store.rs
│   └── sqlite_store.rs
└── prompt/
    ├── mod.rs
    ├── model_request.rs
    ├── profile.rs
    └── runtime_context_encoder.rs

crates/aise-server/src/api/
├── mod.rs
├── pack.rs
├── routes.rs
└── story.rs

crates/aise/tests/
├── asset_import_tests.rs
├── narrative_graph_tests.rs
├── story_instance_tests.rs
├── story_pack_runtime_tests.rs
└── trust_boundary_tests.rs
~~~

All listed `mod.rs` files contain declarations and re-exports only. Existing source files that become directory entry modules must be migrated to this layout when touched.
The tree lists new, relocated, and contract-owning paths, not every preserved file. Existing trusted Prompt catalog, asset, loader, policy, renderer, resolver, slot, and validator modules remain inside `prompt`; they must not become content-facing APIs.

### 3.2 Stable Keys, Versions, Digests, and Bounded Values

~~~rust
pub struct CharacterAssetKey(Arc<str>);
pub struct WorldBookKey(Arc<str>);
pub struct StoryPackKey(Arc<str>);
pub struct StoryRoleKey(Arc<str>);
pub struct SceneKey(Arc<str>);
pub struct LocationKey(Arc<str>);
pub struct EntityKey(Arc<str>);
pub struct TopicKey(Arc<str>);
pub struct FactKey(Arc<str>);
pub struct RumorKey(Arc<str>);
pub struct MemoryKey(Arc<str>);
pub struct NarrativeNodeKey(Arc<str>);
pub struct NarrativeEdgeKey(Arc<str>);
pub struct CanonicalEventKey(Arc<str>);
pub struct AssetId(Arc<str>);
pub struct PackId(Arc<str>);
pub struct PlayerId(Arc<str>);
pub struct RumorId(Arc<str>);
pub struct SemanticVersion(Arc<str>);
pub struct Sha256Digest([u8; 32]);
pub struct AttributeKey(Arc<str>);
pub struct RelationshipKind(Arc<str>);
pub struct MemoryKind(Arc<str>);

impl StoryRoleKey {
    pub fn try_new(value: impl Into<String>) -> Result<Self, AssetValidationError>;
    pub fn as_str(&self) -> &str;
}

pub enum ScalarValue {
    Bool(bool),
    Integer(i64),
    Decimal(String),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "key", rename_all = "snake_case")]
pub enum EntityRef {
    World(EntityKey),
    Role(StoryRoleKey),
}

pub struct BoundedText(String);

impl BoundedText {
    pub fn try_new(
        value: impl Into<String>,
        field: &'static str,
        maximum_bytes: usize,
    ) -> Result<Self, AssetValidationError>;
    pub fn as_str(&self) -> &str;
}

#[derive(Debug, thiserror::Error)]
pub enum AssetValidationError {
    #[error("invalid asset field {path}: {code}")]
    Invalid {
        code: AssetValidationCode,
        path: String,
    },
    #[error("asset limit {limit} exceeded: actual {actual}, maximum {maximum}")]
    LimitExceeded {
        limit: &'static str,
        actual: u64,
        maximum: u64,
    },
}
~~~

Every key type has the same fallible constructor and accessor contract as `StoryRoleKey`. Keys are non-empty ASCII identifiers, at most the configured key byte limit, and match `[a-z0-9]+(?:[._-][a-z0-9]+)*`. `SemanticVersion` accepts SemVer 2.0.0 only. Digests serialize as lowercase `sha256:<64 hex>`. Different key domains are never interchangeable.

### 3.3 Native Character Card Contract

~~~rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterCard {
    pub spec: CharacterSpec,
    pub spec_version: AssetSpecVersion,
    pub character_key: CharacterAssetKey,
    pub meta: CharacterMeta,
    pub profile: CharacterProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CharacterSpec {
    #[serde(rename = "aise_char_v3")]
    V3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetSpecVersion {
    #[serde(rename = "3.0")]
    V3_0,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterMeta {
    pub name: BoundedText,
    pub creator: Option<BoundedText>,
    pub version: SemanticVersion,
    #[serde(default)]
    pub tags: Vec<BoundedText>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterProfile {
    pub description: BoundedText,
    pub personality: Vec<BoundedText>,
    pub values: Vec<BoundedText>,
    #[serde(default)]
    pub fears: Vec<BoundedText>,
    pub speaking_style: SpeakingStyle,
    #[serde(default)]
    pub dialogue_examples: Vec<DialogueExample>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeakingStyle {
    pub register: BoundedText,
    pub verbosity: BoundedText,
    #[serde(default)]
    pub traits: Vec<BoundedText>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DialogueExample {
    pub situation: BoundedText,
    pub response: BoundedText,
}
~~~

`CharacterCard` has no scene, goal, health, location, relationship, faction state, Memory, World Book, opening, Prompt, message-role, tool, model, or runtime-configuration field.

### 3.4 Native World Book and Knowledge Seed Contract

~~~rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldBook {
    pub spec: WorldSpec,
    pub spec_version: AssetSpecVersion,
    pub world_book_key: WorldBookKey,
    pub meta: WorldBookMeta,
    #[serde(default)]
    pub facts: BTreeMap<FactKey, FactSeed>,
    #[serde(default)]
    pub rumors: BTreeMap<RumorKey, RumorSeed>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorldSpec {
    #[serde(rename = "aise_world_v3")]
    V3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldBookMeta {
    pub name: BoundedText,
    pub version: SemanticVersion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactSeed {
    pub proposition: Option<Proposition>,
    pub content: BoundedText,
    #[serde(default)]
    pub entities: Vec<EntityKey>,
    #[serde(default)]
    pub tags: Vec<TopicKey>,
    pub salience: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RumorSeed {
    pub claim: Option<Proposition>,
    pub content: BoundedText,
    #[serde(default)]
    pub entities: Vec<EntityKey>,
    #[serde(default)]
    pub tags: Vec<TopicKey>,
    pub salience: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposition {
    pub subject: EntityRef,
    pub predicate: BoundedText,
    pub value: ScalarValue,
}
~~~

`salience` is inclusive `0..=100`. A Fact asserts authoritative world truth; a Rumor records public belief and never asserts that its `claim` is true.

### 3.5 Native Story Pack and Story Role Contract

~~~rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryPack {
    pub spec: StorySpec,
    pub spec_version: AssetSpecVersion,
    pub meta: StoryPackMeta,
    pub story: StoryProfile,
    pub character_assets: BTreeMap<CharacterAssetKey, CharacterAssetSource>,
    pub roles: BTreeMap<StoryRoleKey, StoryRole>,
    pub default_cast: BTreeMap<StoryRoleKey, DefaultCast>,
    pub play: PlayDefinition,
    pub world_book: WorldBookSource,
    pub start: StoryStart,
    pub narrative: NarrativeGraphDefinition,
    #[serde(default)]
    pub assets: BTreeMap<AssetId, StaticAssetDescriptor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StorySpec {
    #[serde(rename = "aise_story_v3")]
    V3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryPackMeta {
    pub pack_key: StoryPackKey,
    pub title: BoundedText,
    pub author: BoundedText,
    pub version: SemanticVersion,
    pub description: BoundedText,
    #[serde(default)]
    pub tags: Vec<BoundedText>,
    pub cover_asset: Option<AssetId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryProfile {
    pub premise: BoundedText,
    pub language: BoundedText,
    pub genre: Vec<BoundedText>,
    pub themes: Vec<BoundedText>,
    pub style: StoryStyle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryStyle {
    pub tone: Vec<BoundedText>,
    pub point_of_view: BoundedText,
    pub tense: BoundedText,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryRole {
    pub role_label: BoundedText,
    pub narrative_function: BoundedText,
    pub initial_state: InitialRoleState,
    #[serde(default)]
    pub initial_relationships: Vec<RelationshipSeed>,
    #[serde(default)]
    pub seed_memories: Vec<MemorySeed>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InitialRoleState {
    pub location: LocationKey,
    pub goals: Vec<BoundedText>,
    #[serde(default)]
    pub attributes: BTreeMap<AttributeKey, ScalarValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipSeed {
    pub target_role_key: StoryRoleKey,
    pub kind: RelationshipKind,
    pub trust: i16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemorySeed {
    pub memory_key: MemoryKey,
    pub kind: MemoryKind,
    pub content: BoundedText,
    #[serde(default)]
    pub tags: Vec<TopicKey>,
    pub salience: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayDefinition {
    pub player_count: u16,
    pub playable_role_keys: Vec<StoryRoleKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryStart {
    pub scene_key: SceneKey,
    pub location_key: LocationKey,
    pub time: BoundedText,
    pub description: BoundedText,
    pub role_openings: BTreeMap<StoryRoleKey, BoundedText>,
}
~~~

`StoryRole` must reject `name`, `description` as identity, `appearance`, `personality`, `values`, `fears`, `speaking_style`, and `dialogue_examples`. Each Role key appears exactly once in `roles` and `default_cast`; every playable Role has exactly one opening.

### 3.6 Embedded and Frozen Asset References

~~~rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CharacterAssetSource {
    Embedded(CharacterCard),
    Frozen(FrozenCharacterAssetRef),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WorldBookSource {
    Embedded(WorldBook),
    Frozen(FrozenWorldBookRef),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenCharacterAssetRef {
    pub character_key: CharacterAssetKey,
    pub version: SemanticVersion,
    pub digest: Sha256Digest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenWorldBookRef {
    pub world_book_key: WorldBookKey,
    pub version: SemanticVersion,
    pub digest: Sha256Digest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultCast {
    pub character_ref: CharacterAssetKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticAssetDescriptor {
    pub path: String,
    pub mime_type: StaticMimeType,
    pub digest: Sha256Digest,
}

pub enum StaticMimeType {
    Png,
    Jpeg,
    Webp,
    Gif,
    OggAudio,
    MpegAudio,
}
~~~

An imported Pack is self-contained after dependency resolution. `FrozenCharacterAssetRef` and `FrozenWorldBookRef` must resolve during import; runtime loading must not fetch or follow newer asset versions.

### 3.7 Narrative Graph Definition and State

~~~rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NarrativeGraphDefinition {
    pub entry_nodes: Vec<NarrativeNodeKey>,
    pub nodes: BTreeMap<NarrativeNodeKey, NarrativeNodeDefinition>,
    pub edges: Vec<NarrativeEdgeDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NarrativeNodeDefinition {
    pub title: BoundedText,
    pub objective: BoundedText,
    pub activate_when: NarrativeCondition,
    pub complete_when: NarrativeCondition,
    pub skip_when: Option<NarrativeCondition>,
    pub effects: NarrativeNodeEffects,
    pub terminal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NarrativeNodeEffects {
    #[serde(default)]
    pub on_activate: Vec<NarrativeEffectDefinition>,
    #[serde(default)]
    pub on_complete: Vec<NarrativeEffectDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NarrativeEdgeDefinition {
    pub edge_key: NarrativeEdgeKey,
    pub from: NarrativeNodeKey,
    pub to: NarrativeNodeKey,
    pub when: NarrativeCondition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum NarrativeCondition {
    All { conditions: Vec<NarrativeCondition> },
    Any { conditions: Vec<NarrativeCondition> },
    Not { condition: Box<NarrativeCondition> },
    StoryStarted,
    NodeState { node_key: NarrativeNodeKey, state: NarrativeNodeState },
    EventOccurred { event_key: CanonicalEventKey },
    FactStateEquals { fact_key: FactKey, value: ScalarValue },
    CharacterStateEquals {
        role_key: StoryRoleKey,
        attribute: BoundedText,
        value: ScalarValue,
    },
    RelationshipReaches {
        source_role_key: StoryRoleKey,
        target_role_key: StoryRoleKey,
        minimum_trust: i16,
    },
    TurnReaches { turn: u64 },
    PlayerActionOccurred { event_key: CanonicalEventKey },
    RoleControllerIs {
        role_key: StoryRoleKey,
        controller: RoleControllerKind,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum NarrativeEffectDefinition {
    GlobalEvent(GlobalEventIntentDefinition),
    CharacterImpulse(CharacterImpulseDefinition),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlobalEventIntentDefinition {
    pub event_key: CanonicalEventKey,
    pub category: BoundedText,
    #[serde(default)]
    pub participants: Vec<EntityRef>,
    pub location: Option<LocationKey>,
    pub description: BoundedText,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterImpulseDefinition {
    pub target_role_key: StoryRoleKey,
    pub goal: BoundedText,
    pub reason: Option<BoundedText>,
    pub emotion: Option<BoundedText>,
    pub urgency: ImpulseUrgency,
    pub valid_for_turns: Option<NonZeroU32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImpulseUrgency {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NarrativeNodeState {
    Inactive,
    Active,
    Completed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarrativeRuntimeState {
    pub graph_revision: u64,
    pub node_states: BTreeMap<NarrativeNodeKey, NarrativeNodeState>,
    pub activation_turns: BTreeMap<NarrativeNodeKey, TurnId>,
}

pub struct GlobalEventIntent {
    pub source_node: NarrativeNodeKey,
    pub event_key: CanonicalEventKey,
    pub category: BoundedText,
    pub participants: Vec<EntityRef>,
    pub location: Option<LocationKey>,
    pub description: BoundedText,
}

pub struct CharacterImpulse {
    pub source_node: NarrativeNodeKey,
    pub target_role_key: StoryRoleKey,
    pub target_character_id: CharacterId,
    pub goal: BoundedText,
    pub reason: Option<BoundedText>,
    pub emotion: Option<BoundedText>,
    pub urgency: ImpulseUrgency,
    pub expires_after_turn: Option<u64>,
}
~~~

The only accepted serialized Narrative effect tags are `global_event` and `character_impulse`. There is no catch-all or unknown effect variant.

### 3.8 Narrative Evaluation Contract

~~~rust
pub struct NarrativeEvaluation<'a> {
    pub definition: &'a NarrativeGraphDefinition,
    pub state: &'a NarrativeRuntimeState,
    pub snapshot: &'a StoryReadSnapshot,
}

pub struct NarrativeDirector {
    limits: NarrativeLimits,
}

pub struct NarrativeLimits {
    pub max_nodes: usize,
    pub max_edges: usize,
    pub max_condition_depth: usize,
    pub max_conditions_per_node: usize,
    pub max_effects_per_node: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum NarrativeError {
    #[error("narrative reference is missing: {key}")]
    MissingReference { key: String },
    #[error("narrative condition limit exceeded")]
    ConditionLimitExceeded,
    #[error("narrative invariant violated: {code}")]
    Invariant { code: &'static str },
}

impl NarrativeDirector {
    pub fn evaluate(
        &self,
        input: NarrativeEvaluation<'_>,
    ) -> Result<NarrativePlan, NarrativeError>;
}

pub struct NarrativePlan {
    pub active_nodes: Vec<NarrativeNodeKey>,
    pub active_goals: Vec<StoryGoal>,
    pub global_event_intents: Vec<GlobalEventIntent>,
    pub character_impulses: Vec<CharacterImpulse>,
    pub proposed_transitions: Vec<ProposedNarrativeTransition>,
    pub effect_dispositions: Vec<NarrativeEffectDisposition>,
}

pub struct ProposedNarrativeTransition {
    pub node_key: NarrativeNodeKey,
    pub from: NarrativeNodeState,
    pub to: NarrativeNodeState,
    pub expected_graph_revision: u64,
}

pub enum NarrativeEffectDisposition {
    Pending,
    NotApplicable(NotApplicableReason),
}

pub enum NotApplicableReason {
    PlayerControlled,
}
~~~

`NarrativeDirector::evaluate` is synchronous, deterministic, side-effect free, and LLM-free. It reads only committed Snapshot state. It does not mutate `NarrativeRuntimeState`. Role references resolve through the current `RoleBinding` map.

### 3.9 Story Instance and Role Binding Contract

~~~rust
pub enum RoleController {
    Player(PlayerId),
    Ai,
}

pub enum RoleControllerKind {
    Player,
    Ai,
}

pub struct RoleBinding {
    pub role_key: StoryRoleKey,
    pub character_id: CharacterId,
    pub character_asset: FrozenCharacterAssetRef,
    pub controller: RoleController,
}

pub struct CreateStoryInstanceSpec {
    pub pack_id: PackId,
    pub player_id: PlayerId,
    pub player_role_key: StoryRoleKey,
    pub player_character: Option<FrozenCharacterAssetRef>,
    pub created_at_ms: i64,
}

pub struct StoryInstanceFactory {
    asset_store: Arc<dyn AssetStore>,
    store: Arc<dyn Store>,
    limits: StoryInstantiationLimits,
}

pub struct StoryInstantiationLimits {
    pub max_roles: usize,
    pub max_facts: usize,
    pub max_rumors: usize,
    pub max_memories: usize,
    pub max_relationships: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum StoryInstantiationError {
    #[error("story pack was not found")]
    PackNotFound,
    #[error("story role was not found")]
    RoleNotFound,
    #[error("story role is not playable")]
    RoleNotPlayable,
    #[error("character asset was not found")]
    CharacterNotFound,
    #[error("story instantiation limit exceeded: {limit}")]
    LimitExceeded { limit: &'static str },
    #[error("story store operation failed")]
    Store(StoreError),
}

impl StoryInstanceFactory {
    pub async fn create(
        &self,
        spec: CreateStoryInstanceSpec,
    ) -> Result<StoryInfo, StoryInstantiationError>;
}

pub struct CharacterInstanceState {
    pub character_id: CharacterId,
    pub role_key: StoryRoleKey,
    pub identity: FrozenCharacterAssetRef,
    pub location: LocationKey,
    pub goals: Vec<BoundedText>,
    pub attributes: BTreeMap<AttributeKey, ScalarValue>,
}
~~~

The factory resolves the selected player Role, replaces only that Role's default cast when `player_character` is present, binds every other Role to `default_cast`, creates exactly one stable `CharacterId` per Role, and writes all initial state in one Store transaction.

### 3.10 Authoritative Knowledge Contract

~~~rust
pub enum FactSource {
    Seed { pack_id: PackId, fact_key: FactKey },
    CommittedTurn { turn_id: TurnId, event_id: EventId },
}

pub struct WorldFact {
    pub id: FactId,
    pub proposition: Option<Proposition>,
    pub content: BoundedText,
    pub source: FactSource,
    pub story_revision: StoryRevision,
}

pub struct SharedRumor {
    pub id: RumorId,
    pub claim: Option<Proposition>,
    pub content: BoundedText,
    pub source: KnowledgeSource,
    pub story_revision: StoryRevision,
}

pub struct MemoryEntry {
    pub id: MemoryId,
    pub owner: CharacterId,
    pub kind: MemoryKind,
    pub content: BoundedText,
    pub source: KnowledgeSource,
    pub story_revision: StoryRevision,
    pub created_at_ms: i64,
}

pub enum KnowledgeAudience {
    GlobalWriter,
    Character(CharacterId),
    Validator,
}

pub struct KnowledgeQuery {
    pub audience: KnowledgeAudience,
    pub scene: SceneKey,
    pub entities: Vec<EntityKey>,
    pub topics: Vec<TopicKey>,
}

pub struct RetrievedContextItem {
    pub source_id: KnowledgeSourceId,
    pub knowledge_kind: KnowledgeKind,
    pub story_revision: StoryRevision,
    pub role_scope: Option<StoryRoleKey>,
    pub character_scope: Option<CharacterId>,
    pub content: BoundedText,
    pub relevance_score: OrderedScore,
    pub token_cost: u64,
}

pub enum KnowledgeKind {
    Fact,
    Rumor,
    Memory,
    CurrentPerception,
}

pub enum KnowledgeSource {
    Seed { pack_id: PackId },
    CommittedTurn { turn_id: TurnId, event_id: Option<EventId> },
}

pub enum KnowledgeSourceId {
    Fact(FactId),
    Rumor(RumorId),
    Memory(MemoryId),
    Perception(EventId),
}

pub struct CurrentPerception {
    pub character_id: CharacterId,
    pub source_event_id: EventId,
    pub content: BoundedText,
    pub story_revision: StoryRevision,
}

pub struct OrderedScore(u32);
~~~

There is no `known_by` field on `WorldFact`. Character retrieval returns relevant Rumors, only the target character's Memories, and that character's Current Perception; it returns no Fact merely because the Fact was available to Planner or Generator.

### 3.11 Story Snapshot Contract

~~~rust
pub struct StoryReadSnapshot {
    story_id: StoryId,
    base_revision: StoryRevision,
    pack: FrozenStoryPackRef,
    story_profile: StoryProfile,
    role_definitions: BTreeMap<StoryRoleKey, StoryRole>,
    role_bindings: BTreeMap<StoryRoleKey, RoleBinding>,
    character_cards: BTreeMap<CharacterId, CharacterCard>,
    character_states: BTreeMap<CharacterId, CharacterInstanceState>,
    world_facts: Vec<WorldFact>,
    shared_rumors: Vec<SharedRumor>,
    memories: Vec<MemoryEntry>,
    current_perceptions: Vec<CurrentPerception>,
    current_scene: CurrentScene,
    relationships: Vec<RelationshipState>,
    narrative_definition: NarrativeGraphDefinition,
    narrative_state: NarrativeRuntimeState,
    canonical_events: Vec<StoryEvent>,
    recent_turns: Vec<StoryTurn>,
    story_summary: StorySummary,
    active_constraints: Vec<StoryConstraint>,
}

pub struct FrozenStoryPackRef {
    pub pack_id: PackId,
    pub pack_key: StoryPackKey,
    pub version: SemanticVersion,
    pub digest: Sha256Digest,
}

pub struct RelationshipState {
    pub source_character_id: CharacterId,
    pub target_character_id: CharacterId,
    pub kind: RelationshipKind,
    pub trust: i16,
}

impl StoryReadSnapshot {
    pub fn story_id(&self) -> &StoryId;
    pub fn base_revision(&self) -> StoryRevision;
    pub fn story_profile(&self) -> &StoryProfile;
    pub fn role_binding(&self, key: &StoryRoleKey) -> Option<&RoleBinding>;
    pub fn character_memory(&self, id: &CharacterId) -> impl Iterator<Item = &MemoryEntry>;
    pub fn narrative_definition(&self) -> &NarrativeGraphDefinition;
    pub fn narrative_state(&self) -> &NarrativeRuntimeState;
}
~~~

`Store::load_story_snapshot` loads all fields from one database read transaction at one `base_revision`. No Pipeline may reload Pack, Graph, Role, Fact, Rumor, Memory, or Character state independently during the Turn.

### 3.12 Typed Runtime Context and Prompt Boundary

~~~rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptProfile {
    WriterPlanner,
    CharacterThink,
    StoryGenerator,
    StoryRepairer,
    NarrativeValidator,
}

pub struct ModelRequest<C> {
    profile: PromptProfile,
    context: C,
    max_output_tokens: u32,
    purpose: LlmCallPurpose,
}

impl ModelRequest<WriterPlannerContext> {
    pub(crate) fn writer_planner(
        context: WriterPlannerContext,
        max_output_tokens: u32,
    ) -> Self;
}

impl ModelRequest<CharacterThinkContext> {
    pub(crate) fn character_think(
        context: CharacterThinkContext,
        max_output_tokens: u32,
    ) -> Self;
}

impl ModelRequest<StoryGeneratorContext> {
    pub(crate) fn story_generator(
        context: StoryGeneratorContext,
        max_output_tokens: u32,
    ) -> Self;
}

pub trait TrustedPromptSource: Send + Sync {
    fn resolve(
        &self,
        profile: PromptProfile,
    ) -> Result<TrustedSystemPrompt, PromptError>;
}

pub struct TrustedSystemPrompt(String);

pub struct UntrustedContextMessage {
    content: String,
}

pub struct RuntimeContextEncoder;

impl RuntimeContextEncoder {
    pub fn encode<C: Serialize>(
        &self,
        context: &C,
    ) -> Result<UntrustedContextMessage, PromptError>;
}

impl LlmGateway {
    pub fn new(
        provider: Arc<dyn LlmProvider>,
        prompt_source: Arc<dyn TrustedPromptSource>,
        config: LlmConfig,
    ) -> Result<Self, TurnExecutionError>;

    pub async fn complete<C: Serialize>(
        &self,
        scope: TurnLlmCallScope<'_>,
        request: ModelRequest<C>,
        reservation: LlmBudgetReservation,
    ) -> Result<LlmCompletion, LlmError>;
}

pub struct BaselineContext {
    pub story_profile: StoryProfile,
    pub roles: Vec<RoleContext>,
    pub current_scene: CurrentScene,
    pub active_narrative: ActiveNarrative,
    pub recent_story: Vec<StoryTurn>,
    pub story_summary: StorySummary,
    pub player_input: BoundedText,
}

pub struct RoleContext {
    pub role_key: StoryRoleKey,
    pub role: StoryRole,
    pub binding: RoleBinding,
    pub identity: CharacterProfile,
    pub character_state: CharacterInstanceState,
}

pub struct ActiveNarrative {
    pub graph_revision: u64,
    pub active_nodes: Vec<NarrativeNodeKey>,
    pub goals: Vec<StoryGoal>,
}

pub struct WriterPlannerContext {
    pub baseline: BaselineContext,
    pub canonical_facts: Vec<RetrievedContextItem>,
    pub shared_rumors: Vec<RetrievedContextItem>,
}

pub struct CharacterThinkContext {
    pub role: RoleContext,
    pub identity: CharacterProfile,
    pub memories: Vec<RetrievedContextItem>,
    pub shared_rumors: Vec<RetrievedContextItem>,
    pub current_perception: Vec<CurrentPerception>,
    pub impulses: Vec<CharacterImpulse>,
    pub scene: CurrentScene,
    pub player_input: BoundedText,
}

pub struct StoryGeneratorContext {
    pub baseline: BaselineContext,
    pub canonical_facts: Vec<RetrievedContextItem>,
    pub shared_rumors: Vec<RetrievedContextItem>,
    pub character_thoughts: Vec<CharacterThought>,
    pub narrative_plan: NarrativePlan,
}
~~~

`TrustedPromptSource::resolve` accepts only `PromptProfile`. It accepts no Story, Character, World, player, save, or LLM-output value. `RuntimeContextEncoder` emits one canonical JSON User message and cannot emit System, Assistant, Developer, Tool, or arbitrary-role messages. Pipelines cannot construct `ChatMessage`, `Role::System`, or raw provider message vectors.

### 3.13 Writer Plan, Character Thought, and Proposal Contract

~~~rust
pub struct WriterPlan {
    pub retrieval_requests: Vec<KnowledgeQuery>,
    pub character_requests: Vec<CharacterId>,
    pub story_goal: StoryGoal,
    pub narrative: NarrativePlan,
}

pub struct CharacterThought {
    pub character_id: CharacterId,
    pub perception: BoundedText,
    pub emotion: BoundedText,
    pub goal: BoundedText,
    pub possible_action: BoundedText,
}

pub struct StoryProposal {
    pub story_text: BoundedText,
    pub events: Vec<ProposedEvent>,
    pub character_changes: Vec<ProposedCharacterChange>,
    pub world_change: ProposedWorldChange,
    pub rumor_changes: Vec<ProposedRumorChange>,
    pub memory_changes: Vec<ProposedMemoryChange>,
    pub scene_change: Option<CurrentScene>,
    pub narrative_changes: Vec<ProposedNarrativeChange>,
    pub constraint_changes: Vec<ProposedConstraintChange>,
    pub summary_change: Option<StorySummary>,
}

pub struct ProposedNarrativeChange {
    pub node_key: NarrativeNodeKey,
    pub from: NarrativeNodeState,
    pub to: NarrativeNodeState,
    pub expected_graph_revision: u64,
}

pub struct ProposedRumorChange {
    pub rumor_id: Option<RumorId>,
    pub operation: KnowledgeChangeOperation,
    pub claim: Option<Proposition>,
    pub content: Option<BoundedText>,
    pub evidence: Vec<WorldFactEvidenceRef>,
}

pub enum KnowledgeChangeOperation {
    Add,
    Replace,
    Remove,
}

pub struct ProposedConstraintChange {
    pub constraint_id: ConstraintId,
    pub operation: KnowledgeChangeOperation,
    pub text: Option<BoundedText>,
}
~~~

`CharacterThought`, `NarrativePlan`, `GlobalEventIntent`, `CharacterImpulse`, and `StoryProposal` are untrusted, Turn-scoped values. None implements a Store write operation.

### 3.14 Validation and Atomic Commit Contract

~~~rust
pub enum ValidationIssueCode {
    SchemaInvalid,
    ReferenceMissing,
    ModificationForbidden,
    DomainInvariantViolated,
    KnowledgeBoundaryViolated,
    WorldFactEvidenceMissing,
    WorldFactEvidenceInvalid,
    NarrativeAuthorityViolated,
    NarrativeRevisionConflict,
    NarrativeInconsistent,
    CharacterInconsistent,
}

pub struct ValidatedRumorChange {
    rumor_id: Option<RumorId>,
    operation: KnowledgeChangeOperation,
    claim: Option<Proposition>,
    content: Option<BoundedText>,
    source: KnowledgeSource,
}

pub struct ValidatedNarrativeChange {
    node_key: NarrativeNodeKey,
    from: NarrativeNodeState,
    to: NarrativeNodeState,
    expected_graph_revision: u64,
}

pub struct ValidatedChangeSet {
    story_text: BoundedText,
    events: Vec<StoryEvent>,
    character_changes: Vec<CharacterStateChange>,
    world_change: StateChange<WorldState>,
    rumor_changes: Vec<ValidatedRumorChange>,
    memory_changes: Vec<MemoryStateChange>,
    scene_change: StateChange<CurrentScene>,
    narrative_changes: Vec<ValidatedNarrativeChange>,
    constraint_change: StateChange<Vec<StoryConstraint>>,
    summary_change: StateChange<StorySummary>,
}

pub struct TurnCommitSpec {
    pub story_id: StoryId,
    pub base_revision: StoryRevision,
    pub expected_graph_revision: u64,
    pub change_set: ValidatedChangeSet,
    pub turn: StoryTurn,
    pub idempotency_key: IdempotencyKey,
    pub request_digest: RequestDigest,
    pub outbox: Vec<OutboxRecord>,
    pub llm_calls: Vec<LlmCallUsage>,
}
~~~

`ValidatedChangeSet`, `ValidatedRumorChange`, and `ValidatedNarrativeChange` have no public constructors and do not implement `Deserialize`. Only the final deterministic Validation conversion creates them.

### 3.15 Store and Pack Service Ports

~~~rust
#[async_trait]
pub trait AssetStore: Send + Sync {
    async fn find_pack_by_digest(
        &self,
        digest: &Sha256Digest,
    ) -> Result<Option<FrozenStoryPack>, StoreError>;

    async fn import_pack(
        &self,
        pack: ValidatedStoryPack,
    ) -> Result<FrozenStoryPack, StoreError>;

    async fn load_pack(
        &self,
        pack_id: &PackId,
    ) -> Result<FrozenStoryPack, StoreError>;

    async fn export_pack(
        &self,
        pack_id: &PackId,
    ) -> Result<FrozenStoryPack, StoreError>;
}

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
}

pub struct PackInfo {
    pub pack_id: PackId,
    pub pack_key: StoryPackKey,
    pub version: SemanticVersion,
    pub digest: Sha256Digest,
}

pub struct MaterializedStoryInstanceSpec {
    pub story_id: StoryId,
    pub pack: FrozenStoryPackRef,
    pub bindings: BTreeMap<StoryRoleKey, RoleBinding>,
    pub characters: BTreeMap<CharacterId, CharacterInstanceState>,
    pub relationships: Vec<RelationshipState>,
    pub facts: Vec<WorldFact>,
    pub rumors: Vec<SharedRumor>,
    pub memories: Vec<MemoryEntry>,
    pub scene: CurrentScene,
    pub opening: BoundedText,
    pub narrative_state: NarrativeRuntimeState,
    pub created_at_ms: i64,
}

pub struct StorySave {
    pub spec: StorySaveSpec,
    pub spec_version: AssetSpecVersion,
    pub story_id: StoryId,
    pub pack: FrozenStoryPackRef,
    pub base_revision: StoryRevision,
    pub bindings: BTreeMap<StoryRoleKey, RoleBinding>,
    pub state: StorySaveState,
    pub history: Vec<StoryTurn>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorySaveSpec {
    #[serde(rename = "aise_save_v3")]
    V3,
}

pub struct StorySaveState {
    pub characters: BTreeMap<CharacterId, CharacterInstanceState>,
    pub relationships: Vec<RelationshipState>,
    pub facts: Vec<WorldFact>,
    pub rumors: Vec<SharedRumor>,
    pub memories: Vec<MemoryEntry>,
    pub scene: CurrentScene,
    pub narrative_state: NarrativeRuntimeState,
    pub summary: StorySummary,
    pub constraints: Vec<StoryConstraint>,
}

#[async_trait]
pub trait Store: Send + Sync {
    async fn create_story_instance(
        &self,
        spec: &MaterializedStoryInstanceSpec,
    ) -> Result<StoryInfo, StoreError>;

    async fn load_story_snapshot(
        &self,
        story_id: &StoryId,
        limits: SnapshotLimits,
    ) -> Result<StoryReadSnapshot, StoreError>;

    async fn export_story_save(
        &self,
        story_id: &StoryId,
    ) -> Result<StorySave, StoreError>;

    async fn commit_turn(
        &self,
        spec: &TurnCommitSpec,
    ) -> Result<CommittedTurnResult, StoreError>;
}

pub struct PackService {
    importer: NativeAssetImporter,
    asset_store: Arc<dyn AssetStore>,
}

impl PackService {
    pub fn validate(
        &self,
        input: AssetInput<'_>,
    ) -> ValidationReport;

    pub async fn import(
        &self,
        input: AssetInput<'_>,
    ) -> Result<PackInfo, AssetImportError>;

    pub async fn export(
        &self,
        pack_id: &PackId,
        format: PackExportFormat,
    ) -> Result<PackExport, AssetExportError>;
}
~~~

`AssetStore::import_pack` is idempotent by canonical manifest digest plus Pack version. A duplicate digest returns the existing `PackId`; the same `StoryPackKey + version` with a different digest returns `StoreError::ConstraintViolation`.

### 3.16 Native Import, Container, and Error Contract

~~~rust
pub enum AssetInput<'a> {
    Json(&'a [u8]),
    Pack(&'a [u8]),
}

pub enum PackExportFormat {
    Json,
    AisePack,
}

pub enum PackExport {
    Json(Vec<u8>),
    AisePack(Vec<u8>),
}

#[derive(Debug, thiserror::Error)]
pub enum AssetExportError {
    #[error("story pack was not found")]
    NotFound,
    #[error("asset store operation failed")]
    Store(StoreError),
    #[error("asset export I/O failed: {code}")]
    Io { code: &'static str },
}

pub struct ValidationReport {
    pub valid: bool,
    pub issues: Vec<AssetValidationIssue>,
}

pub struct AssetValidationIssue {
    pub code: AssetValidationCode,
    pub path: String,
    pub message: String,
}

pub enum AssetValidationCode {
    UnsupportedSpec,
    UnsupportedSpecVersion,
    InvalidKey,
    InvalidVersion,
    UnknownField,
    ForbiddenField,
    MissingReference,
    DuplicateKey,
    MissingDefaultCast,
    MissingPlayableOpening,
    CharacterIdentityFieldInRole,
    InvalidSalience,
    LimitExceeded,
    GraphCycle,
    GraphUnreachable,
    GraphReferenceInvalid,
    GraphEffectForbidden,
    GraphConditionForbidden,
    AssetReferenceUnpinned,
    AssetDigestMismatch,
    ArchivePathUnsafe,
    ArchiveDuplicatePath,
    ArchiveSymlinkForbidden,
    ArchiveMimeForbidden,
    ArchiveSizeExceeded,
    ArchiveRatioExceeded,
}

#[derive(Debug, thiserror::Error)]
pub enum AssetImportError {
    #[error("asset validation failed")]
    Invalid(ValidationReport),
    #[error("asset store operation failed")]
    Store(StoreError),
    #[error("asset I/O failed: {code}")]
    Io { code: &'static str },
}
~~~

Every validation issue path is a JSON Pointer or archive-relative path. Error messages are bounded and never include full asset text.

### 3.17 Configuration Contract

~~~rust
pub struct AssetLimitsConfig {
    pub max_key_bytes: usize,
    pub max_text_bytes: usize,
    pub max_tags_per_item: usize,
    pub max_roles: usize,
    pub max_character_assets: usize,
    pub max_world_facts: usize,
    pub max_world_rumors: usize,
    pub max_seed_memories_per_role: usize,
    pub max_relationships_per_role: usize,
    pub max_graph_nodes: usize,
    pub max_graph_edges: usize,
    pub max_condition_depth: usize,
    pub max_conditions_per_node: usize,
    pub max_effects_per_node: usize,
    pub max_manifest_bytes: usize,
    pub max_compressed_pack_bytes: u64,
    pub max_uncompressed_pack_bytes: u64,
    pub max_compression_ratio: u32,
    pub max_asset_files: usize,
    pub max_single_asset_bytes: u64,
    pub max_validation_issues: usize,
}

pub struct PromptModuleConfig {
    pub catalog_path: PathBuf,
    pub profile_assets: BTreeMap<PromptProfile, AssetRef>,
}

pub struct AiseConfig {
    pub llm: LlmConfig,
    pub storage: StorageConfig,
    pub turn: TurnConfig,
    pub coordinator: CoordinatorConfig,
    pub content: TurnContentLimitsConfig,
    pub assets: AssetLimitsConfig,
    pub prompt: PromptModuleConfig,
}
~~~

Every numeric limit is strictly positive. Aggregate limits must be greater than or equal to their per-item limits. `PromptModuleConfig`, `LlmConfig`, `TurnConfig`, and `AssetLimitsConfig` are loaded only from trusted deployment configuration; none is deserialized from an imported Pack or save.

### 3.18 HTTP Protocol

~~~http
POST /api/packs/validate
Content-Type: application/json | application/vnd.aise.pack+zip

POST /api/packs
Content-Type: application/json | application/vnd.aise.pack+zip

GET /api/packs/{pack_id}/export?format=json|aise-pack

POST /api/stories
Content-Type: application/json

{
  "pack_id": "string",
  "player_id": "string",
  "player_role_key": "role.guardian",
  "character_ref": {
    "character_key": "character.kai",
    "version": "1.0.0",
    "digest": "sha256:..."
  }
}

GET /api/stories/{story_id}
GET /api/stories/{story_id}/export
~~~

Response mapping:

| Operation | Condition | Status |
| --- | --- | --- |
| validate Pack | parsed and validated | `200` with `ValidationReport` |
| import Pack | new immutable Pack | `201` with `PackInfo` |
| import Pack | identical digest already exists | `200` with existing `PackInfo` |
| any Pack operation | invalid native asset or unsafe container | `422` with bounded issue codes and paths |
| any Pack operation | unsupported media type | `415` |
| create Story | valid Pack, Role, and Character binding | `201` with `StoryInfo` |
| create Story | missing Pack, Role, or Character | `404` |
| create Story | Role is not playable or Character ref is invalid | `422` |
| export Pack or Story | target exists | `200` |
| Store unavailable | any operation | `503` |

The old `CreateStoryRequest { story_instructions, style, point_of_view, tense }` payload is removed and returns `422`; it is not translated to v3.

### 3.19 Persistence Transaction Boundary

Story Instance creation writes the following in one transaction:

~~~text
Story row pinned to Pack ID, Pack version, and Pack digest
Role bindings for every StoryRole
Character instance state for every binding
Relationship seeds resolved from RoleKey to CharacterId
Fact seeds with FactSource::Seed
Shared Rumor seeds
Memory seeds owned by resolved CharacterId
Initial Scene and selected role opening
NarrativeRuntimeState at graph_revision = 0
Initial Story revision
~~~

Turn commit writes the following in one transaction:

~~~text
Story text and canonical events
Character state and relationship changes
World Fact changes
Shared Rumor changes
Character Memory changes
Scene and summary changes
Narrative node-state transitions and graph_revision
Story revision compare-and-swap
Graph revision compare-and-swap
Idempotent Turn result
Outbox records
LLM usage and charge ledger
~~~

Failure of either revision compare-and-swap rolls back the entire transaction and returns `StoreError::RevisionConflict`. No seed is reapplied after Story Instance creation.

---

## 4. Behavior Rules

1. **SP-1 — Native formats only**: The importer accepts exactly `aise_char_v3`, `aise_world_v3`, and `aise_story_v3` with `spec_version = "3.0"`; every other discriminator fails with `UnsupportedSpec` or `UnsupportedSpecVersion`.
2. **SP-2 — Strict fields**: Every executable asset DTO rejects unknown fields before Domain construction. Raw unknown JSON, `metadata`, or `extensions` is never retained.
3. **SP-3 — Forbidden semantics**: Any field named `system_prompt`, `developer_prompt`, `prompt`, `post_history_instructions`, `jailbreak`, `message_role`, `template`, `position`, `depth`, `injection_order`, `stop`, `model`, `tools`, `skills`, `temperature`, or `max_tokens` at any asset depth fails with `ForbiddenField`.
4. **SP-4 — Content has no authority**: Story Pack, Character Card, World Book, player input, save data, and LLM output are encoded only as untrusted Runtime Context and cannot select or interpolate a System Prompt.
5. **SP-5 — Immutable Pack**: Import assigns `PackId`, freezes dependency versions and digests, and never mutates the imported Pack. A Pack change requires a new version and digest.
6. **SP-6 — Template/instance separation**: Pack import creates no `StoryId`. Story Instance creation creates all mutable state and never writes it back to Pack or Character assets.
7. **SP-7 — Orthogonal ownership**: Character identity comes only from `CharacterCard`; story goals, location, attributes, relationships, and seed Memories come only from `StoryRole`. No merge rule permits one owner to overwrite the other.
8. **SP-8 — Complete cast**: Before Story Instance commit, every StoryRole has exactly one valid RoleBinding and CharacterId. RoleBinding is immutable for the Story Instance lifetime.
9. **SP-9 — Single-player contract**: v3 requires `play.player_count = 1` and exactly one player-controlled RoleBinding. Every other RoleBinding is AI-controlled.
10. **SP-10 — Player substitution**: A supplied player Character replaces only the selected playable Role's default cast. All other Roles retain their default cast.
11. **SP-11 — Role references**: Graph, relationship, knowledge, opening, and story-specific event definitions reference `StoryRoleKey`. Runtime resolves the key through current RoleBindings and never through default Character asset keys or names.
12. **SP-12 — Apply seeds once**: Start scene, selected role opening, initial Role state, relationships, Fact seeds, Rumor seeds, Memory seeds, and Narrative initial state are written only by Story Instance creation.
13. **SP-13 — Knowledge separation**: Fact, Rumor, and Memory remain separate authoritative collections. Conflicting entries are retained; none automatically corrects, upgrades, or overwrites another.
14. **SP-14 — Fact visibility**: Fact retrieval for Planner or Generator does not expose the Fact to Character Think. Character knowledge arises only from relevant Rumor, own Memory, or Current Perception.
15. **SP-15 — Memory ownership**: Every persistent Memory has exactly one CharacterId owner. Character Think for character A receives no Memory owned by character B.
16. **SP-16 — Transient cognition**: Current Perception, Character Thought, and Planner hypothesis live only in the current `TurnExecutionContext`. Persistence requires a validated proposed change.
17. **SP-17 — Audience-first retrieval**: Retrieval filters by `KnowledgeAudience` before ranking. It never retrieves the full corpus or relies on Prompt text to hide unauthorized items.
18. **SP-18 — Bounded retrieval**: Candidate collection, result count, item bytes, aggregate tokens, sorting work, and duplicate tracking stop at configured limits. Ties use stable IDs for deterministic order.
19. **SP-19 — DAG validation**: Import rejects cycles, missing entry nodes, unreachable nodes, unreachable terminal nodes, missing edge endpoints, duplicate keys, and condition/effect references that do not resolve.
20. **SP-20 — Typed conditions**: Narrative conditions use only §3.7 variants, read only committed Snapshot data, and have no side effects. Depth and child counts are checked before evaluation.
21. **SP-21 — Two effects only**: Narrative effects deserialize only as `GlobalEvent` or `CharacterImpulse`; direct text, forced action, player impulse, state patch, Prompt fragment, and Tool call shapes fail import.
22. **SP-22 — Intent is not state**: `GlobalEventIntent` becomes authoritative only after Generator conversion to a Proposed Event, deterministic Validation, and commit.
23. **SP-23 — Character autonomy**: `CharacterImpulse` supplies private Character Think context and cannot directly create dialogue, action, Memory, or Character state.
24. **SP-24 — Player control**: An impulse targeting a player-controlled Role produces exactly `NotApplicable(PlayerControlled)`, is not dispatched to Character Think, and creates no substitute player goal, dialogue, action, or choice.
25. **SP-25 — Effect once-only**: `on_activate` and `on_complete` effects are emitted only with the corresponding validated state transition and never repeat while a node remains in the same state.
26. **SP-26 — Planner integration**: Writer Planner calls the pure `NarrativeDirector`, stores the returned `NarrativePlan` in WriterPlan, and does not persist node transitions.
27. **SP-27 — Pipeline isolation**: Baseline Builder, Planner, Retrieval, Character Think, Generator, Validation/Repair, and Committer communicate only through `&mut TurnExecutionContext`; none directly invokes another Pipeline.
28. **SP-28 — AI-only Character Think**: Character Think iterates bounded AI-controlled RoleBindings requested by Writer Plan. It skips the player-controlled Role even if the LLM requests it.
29. **SP-29 — Proposal authority**: Generator and Repairer output only `StoryProposal`. Repairer may replace the Proposal but cannot modify Pack, Graph Definition, Snapshot, or committed state.
30. **SP-30 — Validation order**: Deterministic validation executes Schema, Reference, Modification Permission, Domain Invariant, Knowledge Boundary, Player Control, Fact Evidence, Narrative Authority, then narrative/character validation before sealed `ValidatedChangeSet` conversion.
31. **SP-31 — Atomic commit**: Only a sealed `ValidatedChangeSet` enters `TurnCommitter`. Story, World, Character, Rumor, Memory, Scene, Narrative, Outbox, idempotency, and LLM ledger changes commit atomically.
32. **SP-32 — Revision consistency**: The commit compares both Story `base_revision` and Narrative `graph_revision`. Any mismatch rolls back all writes and reports a structured conflict.
33. **SP-33 — One Snapshot**: Baseline Builder loads one bounded `StoryReadSnapshot`; all remaining stages read that Snapshot and Turn-local derivatives.
34. **SP-34 — Prompt profile fixed by code**: Writer Planner always uses `WriterPlanner`, Character Think uses `CharacterThink`, Generator uses `StoryGenerator`, Repairer uses `StoryRepairer`, and narrative validation uses `NarrativeValidator`.
35. **SP-35 — Prompt assembly**: `LlmGateway` resolves the trusted System Prompt from `TrustedPromptSource`, encodes typed Context as canonical JSON User data, and passes the resulting provider request through the shared limiter and existing accounting transaction.
36. **SP-36 — No content-built messages**: Content-facing modules accept no `system: String`, raw `Vec<ChatMessage>`, message role, Prompt template, or Prompt asset identifier.
37. **SP-37 — Context provenance**: Every retrieved Context item retains source ID, knowledge kind, Story revision, Role scope, Character scope, relevance score, and token cost through Generator and Validator.
38. **SP-38 — Container paths**: `.aise-pack` rejects absolute paths, drive-qualified paths, `..`, empty segments, backslash aliases, normalized duplicate paths, symlinks, hard links, and asset references outside `assets/`.
39. **SP-39 — Container budgets**: The importer checks compressed bytes before extraction, streams entries, checks per-file and aggregate uncompressed bytes while reading, enforces compression ratio and file count, and stops immediately on the first hard limit violation.
40. **SP-40 — Static assets only**: The container accepts only §3.6 MIME variants, verifies magic bytes and SHA-256 digest, never executes content, and never opens remote URLs or host filesystem paths.
41. **SP-41 — JSON export rule**: Pack JSON export succeeds only when `assets` is empty; a Pack with static assets must use `aise-pack` export and JSON export returns `422 assets_require_pack_container`.
42. **SP-42 — Diagnosable failures**: Import, instance creation, Snapshot load, graph evaluation, retrieval, Prompt resolution, LLM, Validation, and commit errors carry a stable code and structured identifiers; logs do not interpolate IDs into messages.
43. **SP-43 — Observability**: Pack validate/import/export and Story instantiate operations run inside tracing spans with `pack_id`, `pack_key`, `story_id`, `format`, `status`, and bounded issue counts. LLM calls retain the existing `llm.call` span.
44. **SP-44 — No lock across async work**: Asset cache and Story coordination locks are released before Store, archive, event, or LLM work. Import and instantiation add no hidden queues or unbounded fan-out.
45. **SP-45 — Export separation**: Pack export contains only the immutable Pack and packaged static assets. Story save export contains the pinned Pack ref plus mutable instance state and history; neither export can alter engine or Prompt configuration.
46. **SP-46 — Hard replacement**: `story_instructions`, `StoryConfig`, `ContextSource::LoreBook`, `FactSource::UserEdit`, Pipeline-created System messages, and the legacy Story-create payload have zero production references after this change.
47. **SP-47 — Player agency**: A player action originates only from validated player input and becomes graph-readable only after commit as a canonical player-action event. Graph conditions may wait for and respond to that event but cannot synthesize it.
48. **SP-48 — Engine-owned policy**: Retrieval algorithm, ranking implementation, Turn budget, Context budget, Validation policy, LLM model, concurrency, timeout, and Prompt asset selection come only from trusted `AiseConfig`; asset hints can affect ranking inputs but cannot select or relax policy.

### 4.1 Error Handling

- JSON syntax failure returns `AssetValidationCode::SchemaInvalid` at path `/`.
- Unknown or forbidden fields report their exact JSON Pointer and never include the field value.
- Missing Pack, Character asset, World Book, Role, node, edge, Fact, Rumor, Memory, Scene, location, or event references return typed missing-reference errors with the missing stable key.
- Limit failures report `actual`, `maximum`, and a stable limit name.
- Archive failures report only the bounded normalized entry path, not raw archive bytes.
- turn/Domain APIs return typed errors and never expose `anyhow::Error`, `sqlx::Error`, archive-library errors, or `serde_json::Error`.
- External input paths contain no `unwrap` or `expect`. Broken internal invariants may use the repository's existing invariant error path.

### 4.2 Concurrency

- Asset validation and graph evaluation are CPU-bounded synchronous work and spawn no tasks.
- Pack import performs at most one Store transaction and reads one archive entry at a time.
- Story instantiation performs at most one dependency-resolution batch and one Store transaction; it does not spawn one task per Role.
- Retrieval collects at most `max_retrieval_candidates` authorized candidates and returns at most `max_retrieved_items`.
- Character Think fan-out remains bounded by both `TurnConfig.max_character_thoughts` and the shared `LlmGateway` limiter.
- No lock guard crosses `.await`; no event, trace write, channel send, filesystem operation, Store operation, or LLM call occurs while a write lock is held.

### 4.3 Observability

Required spans:

~~~text
asset.validate { format, bytes, status, issue_count }
asset.import { pack_key, pack_version, digest, status }
asset.export { pack_id, format, status }
story.instantiate { pack_id, player_role_key, story_id, role_count, status }
narrative.evaluate { story_id, turn_id, graph_revision, active_nodes, transition_count }
context.retrieve { story_id, turn_id, audience, candidate_count, result_count, token_cost }
llm.call { story_id, turn_id, stage, purpose, profile, provider, model }
story.commit { story_id, turn_id, base_revision, graph_revision, status }
~~~

No span field records full Story text, Character profile, Memory, Prompt, player input, archive content, or LLM response unless the existing development-only redacted-content policy explicitly permits a bounded representation.

---

## 5. Acceptance Criteria

- [ ] `doc/exec/2026-08-07-story-pack-v3-spec-gpt.md` remains linked to the source design and all implementation PRs cite this spec.
- [ ] All v3 native DTOs use strict unknown-field rejection; `cargo test -p aise --test asset_import_tests strict_unknown_fields_are_rejected` passes.
- [ ] Each forbidden Prompt/runtime field is rejected at every nesting depth; `cargo test -p aise --test trust_boundary_tests forbidden_asset_fields_are_rejected_recursively` passes.
- [ ] Non-v3 and legacy formats have no fallback; `cargo test -p aise --test asset_import_tests only_native_v3_specs_are_accepted` passes.
- [ ] Character identity fields in StoryRole fail import; `cargo test -p aise --test asset_import_tests story_role_cannot_define_character_identity` passes.
- [ ] Every Role requires a default cast and every playable Role requires an opening; `cargo test -p aise --test asset_import_tests cast_and_opening_coverage_is_total` passes.
- [ ] Embedded and frozen Character/World references validate key, version, and digest; `cargo test -p aise --test asset_import_tests frozen_references_are_verified` passes.
- [ ] Archive traversal, duplicate normalized paths, symlinks, MIME mismatch, digest mismatch, zip bombs, excessive ratio, file count, and size limits are rejected; `cargo test -p aise --test asset_import_tests aise_pack_security_limits_are_enforced` passes.
- [ ] Graph cycles, unreachable nodes/terminals, invalid references, excess condition depth, and forbidden effects fail import; `cargo test -p aise --test narrative_graph_tests invalid_graphs_are_rejected` passes.
- [ ] Branch, merge, parallel activation, and multiple terminal nodes evaluate deterministically; `cargo test -p aise --test narrative_graph_tests dag_shapes_evaluate_deterministically` passes.
- [ ] Node effects fire once per validated activation/completion transition; `cargo test -p aise --test narrative_graph_tests transition_effects_fire_once` passes.
- [ ] Pack import is immutable and digest-idempotent and creates no Story; `cargo test -p aise --test story_instance_tests pack_import_does_not_create_instance` passes.
- [ ] Story Instance creation binds every Role exactly once before the first Turn; `cargo test -p aise --test story_instance_tests every_role_has_one_binding` passes.
- [ ] `play.player_count` values other than `1` fail validation and each valid instance has exactly one player-controlled binding; `cargo test -p aise --test story_instance_tests v3_enforces_one_player_binding` passes.
- [ ] A custom player Character replaces only the selected Role identity while Role state and other casts remain unchanged; `cargo test -p aise --test story_instance_tests player_character_replaces_one_cast_only` passes.
- [ ] RoleBinding is immutable and a different cast requires a different StoryId; `cargo test -p aise --test story_instance_tests bindings_are_instance_lifetime_immutable` passes.
- [ ] Start, openings, Fact/Rumor/Memory seeds, relationships, and graph state apply once; `cargo test -p aise --test story_instance_tests seeds_are_materialized_once` passes.
- [ ] Two instances from the same Pack remain isolated; `cargo test -p aise --test story_instance_tests instances_from_one_pack_are_isolated` passes.
- [ ] Snapshot loading returns Pack, Roles, bindings, identities, state, all knowledge classes, Narrative definition/state, history, and constraints from one revision; `cargo test -p aise persistence_tests::snapshot_is_revision_consistent` passes.
- [ ] Fact, Rumor, and Memory conflicts remain separate and no retrieval mutates them; `cargo test -p aise --test story_pack_runtime_tests knowledge_layers_do_not_overwrite_each_other` passes.
- [ ] Character A receives no hidden Fact or Character B Memory; `cargo test -p aise --test story_pack_runtime_tests character_retrieval_enforces_audience` passes.
- [ ] Current Perception and Character Thought are absent from persistence unless represented by validated changes; `cargo test -p aise --test story_pack_runtime_tests transient_context_is_not_persisted` passes.
- [ ] A player-targeted Character Impulse yields `NotApplicable(PlayerControlled)` and causes zero Character Think calls; `cargo test -p aise --test story_pack_runtime_tests player_impulse_is_not_dispatched` passes.
- [ ] Graph evaluation cannot synthesize player actions and sees a player action only after its canonical event is committed; `cargo test -p aise --test story_pack_runtime_tests graph_waits_for_committed_player_action` passes.
- [ ] An AI-targeted Character Impulse reaches only the bound Character's typed Context; `cargo test -p aise --test story_pack_runtime_tests ai_impulse_is_private_to_target` passes.
- [ ] Global Event Intent cannot become a canonical event before Proposal validation and commit; `cargo test -p aise --test story_pack_runtime_tests event_intent_requires_validation_and_commit` passes.
- [ ] Narrative changes, Story changes, knowledge changes, Outbox, idempotency result, and LLM ledger roll back together on either revision conflict; `cargo test -p aise validation_commit_tests::narrative_and_story_commit_are_atomic` passes.
- [ ] Every Pipeline still implements `TurnExecutionPipeline` and no Pipeline directly calls another; `cargo test -p aise --test dependency_direction_tests` passes.
- [ ] All Pipeline LLM requests use their fixed `PromptProfile` and the shared Gateway limiter; `cargo test -p aise --test llm_gateway_tests pipeline_profiles_are_fixed_and_limited` passes.
- [ ] Imported content cannot select, interpolate, or modify System Prompt content; `cargo test -p aise --test trust_boundary_tests asset_content_never_enters_system_prompt` passes.
- [ ] Imported content cannot override model, retrieval, budget, validation, concurrency, timeout, tool, skill, or Prompt profile configuration; `cargo test -p aise --test trust_boundary_tests asset_content_cannot_override_engine_policy` passes.
- [ ] Typed Runtime Context is canonical JSON User data and preserves knowledge provenance; `cargo test -p aise --test trust_boundary_tests runtime_context_is_untrusted_canonical_data` passes.
- [ ] Pack validate/import/export and Story create/read/export HTTP mappings match §3.18; `cargo test -p aise-server --test story_api_tests` passes.
- [ ] Exported Pack contains no runtime state and exported Story save contains its pinned Pack ref and mutable state; `cargo test -p aise --test story_instance_tests pack_and_save_exports_are_separate` passes.
- [ ] All new limit fields reject zero and every bounded collection has overflow tests; `cargo test -p aise config_tests::asset_and_story_pack_limits_validate` passes.
- [ ] `rg -n "story_instructions|StoryConfig|ContextSource::LoreBook|FactSource::UserEdit" crates` returns zero matches.
- [ ] `rg -n "Role::System|system_message\\(" crates/aise/src/planning crates/aise/src/character crates/aise/src/story crates/aise/src/context` returns zero matches.
- [ ] `rg -n "Vec<ChatMessage>|system: String" crates/aise/src/planning crates/aise/src/character crates/aise/src/story crates/aise/src/context crates/aise/src/domain` returns zero matches.
- [ ] `rg -n "DirectStoryText|ForceCharacterAction|ForcePlayerAction|PlayerImpulse|StatePatch|PromptFragment|ToolCall" crates/aise/src/domain` returns zero executable Narrative variants.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo test --workspace --all-features` passes.

---

## 6. Out of Scope / Future Work

- Explicit finite-repeat Narrative structures require a new source design and spec; v3 remains acyclic.
- Multiplayer control assignment beyond the single-player selection contract requires a separate design.
- Remote asset registries, network dependency resolution, asset marketplace behavior, and third-party format conversion require separate designs.
- Retrieval algorithm selection and ranking quality remain engine configuration concerns; this spec defines authorization, provenance, determinism, and bounds only.

---

## 7. References

- Source design: [AISE Story Pack Design v3.0](../design/2026-08-06-StoryPackDesign-gpt.md)
- Turn Runtime architecture: [AISE Technical Architecture v3.1](../design/2026-08-04-Architecture-gpt.md)
- Current legacy Story state to replace: `crates/aise/src/domain/story_state.rs:10`
- Current legacy Story API to replace: `crates/aise-server/src/api/story.rs:10`
- Current content-built System messages to remove: `crates/aise/src/prompt/context_merger.rs:41`
- Current audience-agnostic retrieval to replace: `crates/aise/src/context/retrieval_pipeline.rs:130`
- Guardrails: [Architecture and refactor](../agents/guardrails/architecture-refactor.md), [layer dependencies](../agents/guardrails/layer-dependencies.md), [concurrency](../agents/guardrails/concurrency.md), [code organization](../agents/guardrails/code-organization.md), and [observability](../agents/guardrails/observability.md)
