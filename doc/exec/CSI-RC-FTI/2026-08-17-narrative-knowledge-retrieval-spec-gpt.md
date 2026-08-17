# Narrative, Knowledge, and Retrieval Context Reconciliation — Spec

> **Model**: GPT-5
> **Date**: 2026-08-17
> **Status**: Proposed
> **Source Design**: [Narrative、Knowledge 与 Retrieval Context 收敛](../design/2026-08-17-narrative-knowledge-retrieval-design-gpt.md)
> **Phase**: N/A

---

## 1. Goal

Replace divergent Narrative prompt projections, metadata-heavy Knowledge rendering, flat Memory retrieval, redundant retrieval target IDs, and UUID-derived Knowledge IDs with one stage-correct Narrative Direction contract, typed World/Character context partitions, minimal indexes, and canonical short Story-scoped Knowledge IDs.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Give WriterPlanner, StoryGenerator, and StoryRepairer one shared model-facing Narrative Direction projection containing Active Directions and full semantic World Event Intents.
- Route Character Impulses only to their target CharacterThink call and deterministically merge impulse targets into `character_think_requests`.
- Render loaded Fact/Rumor content under type headings without IDs, titles, per-item kind/scope labels, provenance, or retrieval metadata.
- Remove Memory from global Knowledge views and store every retrieved Memory under its owner `RoleId`.
- Replace the flat `RetrievedContext` with typed World Knowledge and Character Context partitions.
- Make an indexed Character target retrieve a Role view plus bounded Role-scoped Rumor/Memory context rather than treating the Role as an unpartitioned Knowledge-only entity query.
- Automatically create the same bounded Role-scoped Rumor/Memory retrieval for every CharacterThink request and deduplicate demand by Role.
- Reduce Character and Knowledge indexes to one canonical `target_id` plus one `retrieval_hint`, grouped by target kind.
- Add a required bounded `retrieval_hint` to Fact/Rumor seed and runtime records.
- Replace Story/Turn UUID-derived Fact/Rumor/Memory IDs with deterministic short Story-scoped IDs.
- Keep IDs only where the model must reference an indexed or modifiable object.
- Update prompt assets, Prompt DTOs, Turn DTOs, Domain Knowledge types, retrieval ports, SQLite schema, validators, tests, traces, and superseded documentation references.

### 2.2 Non-Goals

- Does not change `CharacterId`, `StoryId`, `TurnId`, `TraceId`, `LlmCallId`, idempotency keys, or other global/infrastructure identifier policies; `RoleId` keeps its existing syntax/lifetime policy subject to the cross-domain reservation in §3.4.
- Does not expose `CharacterId`, Story/Turn/Trace IDs, Knowledge provenance, provider evidence, scores, token costs, revisions, or storage keys to model-visible RC.
- Does not add BM25, embeddings, a reranker, a vector store, or a new retrieval provider.
- Does not add an LLM call for Narrative reconciliation, retrieval hints, Memory summarization, or ID allocation.
- Does not turn Memory into objective world truth or grant one Role access to another Role's Memory.
- Does not make Fact directly visible to CharacterThink; character access remains grounded in Story Continuity, observable story, Memory, or an authorized Rumor.
- Does not change WriterPlanner output fields beyond existing `story_goal`, `context_gaps`, and `character_think_requests`.
- Does not change Character Decision or StoryGenerator output schemas.
- Does not remove IDs from Character/Knowledge indexes or StoryStateExtractor modifiable targets; those are executable references, not read-only Relevant Knowledge metadata.
- Does not reimplement Current Scene removal, Premise removal, Story Continuity prose rendering, or empty-value elision.

### 2.3 Implementation Constraints (for code generation)

- Implement after [Current Scene Removal](2026-08-17-current-scene-removal-spec-gpt.md), [Story Context Simplification](2026-08-17-story-context-simplification-spec-gpt.md), and [Runtime Context Empty Elision](2026-08-17-runtime-context-empty-elision-spec-gpt.md).
- Current Scene Removal owns migration `0018`, Story Context Simplification owns `0019`, and this spec owns `0020`.
- This spec supersedes the conflicting Narrative/Knowledge contracts in the older WriterPlanner and StoryGenerator Prompt specs and §3.2/§3.7 of Runtime Context Empty Elision.
- Preserve the later [NarrativePlan Projection and Semantic Resolution](CSI-RC-FTI/2026-08-13-narrative-plan-resolution-spec-gpt.md) Domain ownership, condition-query, Effect lifecycle, Resolver, and atomic-commit contracts.
- This spec generates final-form code. Do **not** keep old Narrative renderer names, UUID-derived Knowledge IDs, Prompt aliases, `RetrievalTargetId`, flat Context partitions, compatibility parsing, fallback IDs, or dual prompt paths.
- Every old type, field, constructor, SQL column shape, fixture, test, and doc assertion superseded here MUST be deleted in the same change.
- All prompt data remains RC data. No StoryPack, WorldBook, Knowledge body, retrieval hint, Memory, or Narrative text may enter CSI or FTI as instruction authority.
- Do not add code comments, inline test bodies, dependencies, unbounded collections, unbounded queries, background tasks, or new LLM calls.
- `R-ARCH-01/03/04/05`, `R-REFACTOR-01/02`, `R-LAYER-01/04/06`, `R-CODE-01/02/04/05/06/07`, `R-CONC-01/03/04`, and `R-AISE-01/02/03/06/07` remain mandatory.

---

## 3. Contracts

### 3.1 Final Vocabulary and Precedence

| Term | Final meaning | Model-visible stages |
|---|---|---|
| `NarrativePlan` | Internal Turn-scoped Domain projection containing active nodes, directions, Effects, impulses, and dispositions | None directly |
| Narrative Direction | Shared Prompt view of Active Directions and semantic World Event Intents | WriterPlanner, StoryGenerator, StoryRepairer |
| Character Impulse | Role-scoped internal motivation guidance | Matching CharacterThink only |
| Immediate Story Goal | WriterPlanner's one-turn narrative transition | StoryGenerator, StoryRepairer |
| Relevant Knowledge | Already loaded World Fact/Rumor content | WriterPlanner, StoryGenerator, StoryRepairer |
| Character Context | Role view plus Role-scoped Known Rumors and Memories | CharacterThink and authorized author stages |
| Character Index | Unloaded Role discovery metadata | WriterPlanner only |
| Knowledge Index | Unloaded Fact/Rumor discovery metadata | WriterPlanner only |
| Modifiable Knowledge | Existing target set that StoryStateExtractor may update/delete | StoryStateExtractor only |

The Prompt-facing name is `Narrative Direction`. Delete WriterPlanner's model-visible `Narrative Plan` heading and never serialize raw `NarrativePlan` fields to RC.

### 3.2 Shared Narrative Direction Prompt Contract

Add one shared Prompt-layer type and renderer:

```rust
#[derive(Debug, Clone)]
pub struct NarrativeDirectionPromptView {
    pub active_directions: Vec<BoundedText>,
    pub world_event_intents: Vec<WorldEventIntentPromptView>,
}

#[derive(Debug, Clone)]
pub struct WorldEventIntentPromptView {
    pub category: BoundedText,
    pub participants: Vec<KnowledgeEntity>,
    pub location: Option<LocationKey>,
    pub description: BoundedText,
}

pub fn project_narrative_direction(plan: &NarrativePlan) -> NarrativeDirectionPromptView;

pub fn render_narrative_direction(view: &NarrativeDirectionPromptView) -> String;
```

Place these in `crates/aise/src/prompt/narrative_direction.rs` and re-export them from the prompt index. The Prompt module may read Domain types but MUST NOT import a pipeline module.

Projection is exact:

| `NarrativePlan` field | Prompt result |
|---|---|
| `active_directions[].dramatic_focus` | `active_directions` in original stable order |
| `world_event_intents[].category` | required category |
| `world_event_intents[].participants` | semantic participants in original stable order |
| `world_event_intents[].location` | optional location |
| `world_event_intents[].description` | required description |
| `active_nodes` | omitted |
| `character_impulses` | omitted |
| `effect_dispositions` | omitted |
| Effect ID, source node, hidden event key | omitted |

Render with this exact semantic shape; omit an empty child subsection and return `String::new()` when both are empty:

```markdown
### Active Directions

- "<dramatic_focus>"

### World Event Intents

- category: "<category>"
  participants: ["<kind>:<key>"]
  location: "<location>"
  description: "<description>"
```

`participants` and `location` lines are omitted when empty/absent. `category` and `description` are required; trim-empty values are projection invariants. Serialize each participant as its existing stable `<entity-kind>:<entity-key>` semantic reference, never a CharacterCard UUID.

WriterPlanner, StoryGenerator, and StoryRepairer MUST call this same projector and renderer. Delete:

```text
render_narrative_plan
StoryGeneratorNarrativeDirectionPromptView
active_goals
event_intents
world_event_intent_count
model-visible character_impulses in WriterPlanner
```

### 3.3 Character Impulse Routing Contract

Keep the existing CharacterThink impulse item shape, but route it only by exact `target_role_id`. Merge Narrative impulse targets into the validated request list before building automatic Character Knowledge requests:

```rust
fn merge_narrative_think_requests(
    planner_requests: Vec<CharacterThinkRequest>,
    impulses: &[CharacterImpulse],
    baseline: &BaselineContext,
    config: &PlannerConfig,
) -> Result<Vec<CharacterThinkRequest>, PlanningError>;
```

The merge algorithm is exact:

1. Validate Planner requests with the existing Role/controller/reason rules.
2. Preserve valid Planner request order.
3. For every impulse target not already requested, append one request; sort impulse-only additions by `RoleId`.
4. Use the non-whitespace impulse `reason` when present, otherwise its required `goal`, as the internal request reason.
5. Multiple impulses for one Role produce one CharacterThink request; CharacterThink still receives every unexpired impulse for that Role.
6. Re-check `max_character_think_requests` after merging.
7. An unknown, player-controlled, or otherwise CharacterThink-ineligible impulse target returns the existing Narrative/Planning typed invariant error; it is never ignored.

StoryGenerator/Repairer receive the resulting Character Decisions and MUST NOT receive Character Impulses. WriterPlanner RC MUST NOT render them.

### 3.4 Canonical Story-Scoped Knowledge ID Contract

`FactId`, `RumorId`, and `MemoryId` remain distinct newtypes but stop accepting arbitrary strings. Their canonical string grammar is:

```text
FactId   = "fact_"   + canonical_story_local_sequence
RumorId  = "rumor_"  + canonical_story_local_sequence
MemoryId = "memory_" + canonical_story_local_sequence
```

Examples:

```text
fact_0001
rumor_0002
memory_0003
```

All three kinds share one monotonically increasing sequence inside one StoryInstance. Provide validated sequence/high-water types, one constructor, and one pure allocator:

```rust
pub struct KnowledgeSequence(NonZeroU64);

pub struct KnowledgeIdHighWater(u64);

pub struct KnowledgeIdAllocation {
    pub assigned: Vec<KnowledgeSourceId>,
    pub new_high_water: KnowledgeIdHighWater,
}

pub fn new_knowledge_source_id(
    kind: KnowledgeKind,
    sequence: KnowledgeSequence,
) -> Result<KnowledgeSourceId, KnowledgeIdError>;

pub fn allocate_knowledge_ids(
    base: KnowledgeIdHighWater,
    addition_kinds: &[KnowledgeKind],
) -> Result<KnowledgeIdAllocation, KnowledgeIdError>;
```

Rules:

- `canonical_story_local_sequence` is non-zero; values `1..9999` are left-zero-padded to exactly four digits, while values `10000+` use ordinary decimal with no leading zero. Parsing MUST round-trip this formatter, so `00001`, `+001`, and other alternate spellings are invalid.
- Story creation allocates Seed Knowledge in the fixed kind order Fact, Rumor, Memory. Within those groups, order by `FactKey`, `RumorKey`, and `(RoleId, MemoryKey)` respectively.
- `story_instances.knowledge_id_high_water` equals the largest sequence ever allocated for that StoryInstance; it is zero only before any Knowledge exists.
- `StoryReadSnapshot` and `KnowledgeSnapshotRef` carry the same validated `KnowledgeIdHighWater` read in the snapshot transaction.
- Validation assigns IDs only after the final extraction/change set passes structural validation. Add operations consume successive sequence values in their existing stable operation order across all three kinds.
- The Commit transaction verifies the existing base Story revision, verifies the stored high-water equals the Snapshot base, writes the accepted Knowledge mutations, and advances the stored high-water to `KnowledgeIdAllocation.new_high_water` atomically.
- A revision/high-water mismatch returns the existing optimistic-concurrency conflict and writes nothing. Reusing the same base Snapshot and accepted change set produces the same candidate IDs.
- Update retains the target ID. Delete never decrements high-water and never permits ID reuse.
- StoryId, TurnId, Story revision, owner RoleId, source key, content, UUID, hash, timestamp, and random bytes MUST NOT appear in the Knowledge ID.
- `KnowledgeSource::Seed` and `KnowledgeSource::CommittedTurn { turn_id }` remain internal provenance.
- Serialization and deserialization validate the canonical grammar without adding a regex dependency.
- A prefix/newtype/stored-kind mismatch, a value above SQLite's signed-integer maximum, and an allocation overflow return typed `KnowledgeIdError` values.
- StoryInstance validation rejects a `RoleId` whose complete string matches any canonical Fact/Rumor/Memory ID. This reserves only the three exact Knowledge ID shapes; all other existing RoleId syntax remains valid.
- `RoleId` remains the canonical Story-scoped Character target and is not wrapped in another visible target ID.

Delete arbitrary `From<String>`/`From<&str>` construction paths for these three ID types from production code. Tests may use the validated constructors or canonical parsing.

### 3.5 Fact/Rumor Retrieval Hint Contract

Add one persisted Domain value and use it on every Fact/Rumor definition and runtime value, but not Memory:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RetrievalHint(BoundedText);

impl RetrievalHint {
    pub const MAX_BYTES: usize = 256;

    pub fn try_new(value: impl Into<String>) -> Result<Self, RetrievalHintError>;
}

pub struct FactSeed {
    pub retrieval_hint: RetrievalHint,
    // existing fields
}

pub struct RumorSeed {
    pub retrieval_hint: RetrievalHint,
    // existing fields
}

pub struct WorldFact {
    pub id: FactId,
    pub retrieval_hint: RetrievalHint,
    // existing fields
}

pub struct SharedRumor {
    pub id: RumorId,
    pub retrieval_hint: RetrievalHint,
    // existing fields
}
```

Add the same required field to `ProposedKnowledgeValue::Fact` and `ProposedKnowledgeValue::Rumor`. Do not add it to `ProposedKnowledgeValue::Memory`.

`RetrievalHint::try_new` requires trim-non-empty text whose UTF-8 byte length is at most `MAX_BYTES`; it stores the original bounded text without trimming or paraphrasing. The fixed Domain limit is authoritative for assets, extraction, persistence hydration, and indexes so changing runtime config cannot invalidate stored Knowledge. The hint describes what retrieving the entry would provide. It is stored and indexed but is never rendered with already loaded content.

The StoryStateExtractor output JSON Schema MUST require `retrieval_hint` for Fact/Rumor add/update values, set `maxLength` to `RetrievalHint::MAX_BYTES`, and reject it for Memory. Post-deserialization typed validation remains the authoritative UTF-8 byte check. `WorldSpec` accepts only `V4` serialized as `aise_world_v4`, paired with `AssetSpecVersion::V4_0` serialized as `4.0`:

```text
spec         = aise_world_v4
spec_version = 4.0
```

Delete `aise_world_v3` parsing and fixtures. StoryPack remains at the predecessor contract `aise_story_v5`; CharacterCard versions remain unchanged.

### 3.6 Baseline and Index Types

Replace the flat baseline Knowledge list and redundant target fields:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct RelevantWorldKnowledge {
    pub facts: Vec<RelevantWorldKnowledgeItem>,
    pub rumors: Vec<RelevantWorldKnowledgeItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RelevantWorldKnowledgeItem {
    pub source_id: KnowledgeSourceId,
    pub content: BoundedText,
    pub source_priority: u8,
    pub salience: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoleIndexEntry {
    pub role_id: RoleId,
    pub retrieval_hint: BoundedText,
}

#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeIndexEntry {
    pub source_id: KnowledgeSourceId,
    pub retrieval_hint: RetrievalHint,
}
```

`BaselineContext` uses:

```rust
pub relevant_world_knowledge: RelevantWorldKnowledge,
pub knowledge_index_scope: RetrievalIndexScope,
pub knowledge_index: Vec<KnowledgeIndexEntry>,
```

Delete:

```text
RelevantKnowledge
KnowledgeEntryIndexEntry
BaselineContext.relevant_knowledge
BaselineContext.knowledge_entry_index_scope
BaselineContext.knowledge_entry_index
RoleIndexEntry.target_id
RoleIndexEntry.name
RoleIndexEntry.role_label
KnowledgeEntryIndexEntry.target_id
KnowledgeEntryIndexEntry.entry_id
KnowledgeEntryIndexEntry.kind
RetrievalTargetId
```

`RoleIndexEntry.retrieval_hint` is the Story Role's bounded `narrative_function`. `KnowledgeIndexEntry.retrieval_hint` comes from the stored Fact/Rumor. A Memory record in `knowledge_index` is `ContextError::InvalidRecord { code: "memory_in_knowledge_index" }`.

The Knowledge Read Port index record becomes:

```rust
pub struct KnowledgeIndexRecord {
    pub source_id: KnowledgeSourceId,
    pub retrieval_hint: RetrievalHint,
}
```

The kind is available from `KnowledgeSourceId`; do not duplicate it.

### 3.7 Indexed Retrieval Target Contract

Use a tagged lookup value, not a third ID domain:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexedRetrievalTarget {
    Role(RoleId),
    Knowledge(KnowledgeSourceId),
}

pub struct WriterPlannerPromptContext {
    pub indexed_targets: BTreeMap<String, IndexedRetrievalTarget>,
    pub provided_role_ids: Vec<RoleId>,
    pub provided_knowledge_ids: Vec<KnowledgeSourceId>,
}
```

The lookup key is exactly `RoleId::as_str()` or `KnowledgeSourceId::as_str()` and exactly matches rendered `target_id`. Duplicate text across target domains returns `WriterPlannerProjectionError::RetrievalTargetCollision` before the LLM call.

Planner resolution rules:

| Indexed target | Allowed audience | Internal request |
|---|---|---|
| Role | `global_writer` only | `CharacterRetrievalRequest` plus ensured Role cognition request |
| Fact | `global_writer` only | exact Writer Knowledge request |
| Rumor | `global_writer` or matching `character` | exact World/Character Rumor request |
| Memory | never indexed | reject as unknown target |

Keep the Planner JSON field name `target_id`. It contains the exact single ID copied from an index. Do not introduce `entry_id`, `retrieval_id`, or a Prompt alias.

### 3.8 Retrieval Plan and Retrieved Context Contract

Replace the knowledge-only role target behavior with separate request types:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalPlan {
    pub character_requests: Vec<CharacterRetrievalRequest>,
    pub knowledge_requests: Vec<KnowledgeRetrievalRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterRetrievalRequest {
    pub role_id: RoleId,
    pub reason: BoundedText,
    pub origin: RetrievalRequestOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum KnowledgeDelivery {
    Writer,
    Character { role_id: RoleId },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeRetrievalRequest {
    pub delivery: KnowledgeDelivery,
    pub target_source_id: Option<KnowledgeSourceId>,
    pub knowledge_kinds: Vec<KnowledgeKind>,
    pub entities: Vec<KnowledgeEntity>,
    pub topics: Vec<TopicKey>,
    pub query_text: Option<BoundedText>,
    pub reason: BoundedText,
    pub origin: RetrievalRequestOrigin,
    pub signal_priority: u8,
}
```

`authorized_memory_owners` is deleted. Character delivery and the target `role_id` are the authorization boundary.

For the union of every exact Character Index target and every final `CharacterThinkRequest`, `RetrievalPlanBuilder` ensures one base automatic `KnowledgeRetrievalRequest` per `RoleId` with:

```text
delivery        = Character { role_id }
knowledge_kinds = [Rumor, Memory]
entities        = [Role(role_id)]
query_text      = absent
origin          = Planner when an exact Character target contributes, otherwise Automatic
signal_priority = 0
```

This automatic request exists even when no character-scoped `context_gap` was emitted. Build base cognition demand in a `BTreeMap<RoleId, ...>` before generic request deduplication. Exact Character target and CharacterThink demand for the same Role collapse to one base request; the earliest Planner target reason wins, otherwise use the unique Think request reason. A character-scoped gap still requires a matching Think request and adds only its additional exact target/query semantics as a separate bounded request.

Replace `ContextItem`, `ContextProvenance`, and flat `RetrievedContext` with:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct RetrievedKnowledgeItem {
    pub source_id: KnowledgeSourceId,
    pub content: BoundedText,
    pub source: KnowledgeSource,
    pub relevance: RelevanceRank,
    pub provider_evidence: BTreeMap<CandidateRetrieverKind, ProviderEvidence>,
    pub token_cost: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RetrievedWorldKnowledge {
    pub facts: Vec<RetrievedKnowledgeItem>,
    pub rumors: Vec<RetrievedKnowledgeItem>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RetrievedCharacterContext {
    pub role: Option<RoleContextView>,
    pub known_rumors: Vec<RetrievedKnowledgeItem>,
    pub memories: Vec<RetrievedKnowledgeItem>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RetrievedContext {
    world: RetrievedWorldKnowledge,
    characters: BTreeMap<RoleId, RetrievedCharacterContext>,
}
```

Exact partition invariants:

- `world.facts` contains only Fact IDs.
- `world.rumors` contains only Rumor IDs.
- `characters[role].known_rumors` contains only Rumor IDs delivered to that exact Role.
- `characters[role].memories` contains only Memory IDs whose stored owner is that exact Role.
- `characters[role].role`, when present, has that exact `role_id` and came from an indexed Character request.
- Fact in Character Context, Memory in World, owner mismatch, unknown Role, or duplicated conflicting content returns a typed `RetrievedContextError`; it is never re-bucketed or dropped.
- Existing per-audience, total item, total token, item byte, and Role partition bounds remain mandatory.

An exact Character Index target MUST load the bounded Role view from the existing immutable Snapshot and ensure the same Role's bounded Rumor/Memory request. It MUST NOT be implemented only as `KnowledgeEntity::Role` lookup. The store query may remain a dedicated Knowledge request, but its results are deposited only in that Role's `RetrievedCharacterContext`.

### 3.9 Prompt-Facing Knowledge Types

Use content-only Prompt views:

```rust
#[derive(Debug, Clone, Default)]
pub struct WorldKnowledgePromptView {
    pub facts: Vec<BoundedText>,
    pub rumors: Vec<BoundedText>,
}

#[derive(Debug, Clone, Default)]
pub struct RoleKnowledgePromptView {
    pub known_rumors: Vec<BoundedText>,
    pub memories: Vec<BoundedText>,
}
```

Add one deterministic merge:

```rust
pub fn merge_world_knowledge(
    baseline: &RelevantWorldKnowledge,
    retrieved: &RetrievedWorldKnowledge,
) -> Result<WorldKnowledgePromptView, PromptProjectionError>;
```

Merge baseline items first in their stable order, then retrieved items in ranked stable order. Deduplicate by internal `KnowledgeSourceId`. The same ID with different kind/content is an invariant error. Do not deduplicate distinct IDs solely because their text matches; distinct Fact and Rumor claims may conflict.

Delete:

```text
StoryGeneratorKnowledgePromptView
KnowledgeScopePromptView
CharacterThinkKnowledgePromptView
CharacterThinkKnowledgeKind
entry_id/title/kind/scope fields in read-only Prompt views
```

StoryGenerator/Repairer Role views add `knowledge: RoleKnowledgePromptView`. CharacterThink target Role adds the same field. IDs remain internal during these projections.

### 3.10 Runtime Context Rendering Contract

#### WriterPlanner

Rename slots:

```text
narrative_plan         -> narrative_direction
knowledge_entry_index  -> knowledge_index
```

Render:

```markdown
## Relevant Knowledge

### Facts

- "<content>"

### Rumors

- "<content>"

## Character Index

scope: <complete|prefiltered>

### Retrievable Characters

- target_id: "<RoleId>"
  retrieval_hint: "<hint>"

## Knowledge Index

scope: <complete|prefiltered>

### Retrievable Facts

- target_id: "<FactId>"
  retrieval_hint: "<hint>"

### Retrievable Rumors

- target_id: "<RumorId>"
  retrieval_hint: "<hint>"

## Narrative Direction

{{ narrative_direction }}
```

Each empty Fact/Rumor/Character child group is omitted. Relevant Knowledge is omitted when both groups are empty. Both Index sections always retain `scope`; an empty Index has no child heading or `entries` key.

#### CharacterThink

Delete the `relevant_character_knowledge` slot and heading. Render Known Rumors and Memories inside Target Character after identity/profile fields and before its current state content. Only the exact target Role partition is read.

#### StoryGenerator and StoryRepairer

Rename:

```text
relevant_writer_knowledge -> relevant_knowledge
```

Use the same Relevant Knowledge and Narrative Direction renderers as WriterPlanner. StoryRepairer uses the same semantic content at its nested heading level.

AI/Player Character items render optional nested `Known Rumors` and `Memories` groups from their exact Role partition. An indexed retrieved Role is added once to the appropriate Character collection; baseline and retrieved Role IDs are deduplicated with baseline precedence and conflicting Role content is an invariant error.

#### Common content rules

- One bullet equals one Knowledge body encoded with the existing safe string encoder.
- Read-only Relevant Knowledge/Role Knowledge contains no ID, title, per-item kind, scope, source, owner, hint, rank, score, revision, provider, or token field.
- Group headings carry kind semantics.
- Existing empty-elision conditions omit empty headings and values.
- Rumor remains a claim; Memory remains one Role's possibly incomplete subjective recollection.

### 3.11 StoryStateExtractor Target Contract

StoryStateExtractor is the explicit ID-retaining exception. Replace its flat modifiable view with:

```rust
#[derive(Debug, Clone)]
pub struct ModifiableKnowledgePromptItem {
    pub id: KnowledgeSourceId,
    pub content: BoundedText,
}

#[derive(Debug, Clone, Default)]
pub struct ModifiableWorldKnowledgePromptView {
    pub facts: Vec<ModifiableKnowledgePromptItem>,
    pub rumors: Vec<ModifiableKnowledgePromptItem>,
}

#[derive(Debug, Clone)]
pub struct ModifiableMemoryPromptView {
    pub id: MemoryId,
    pub content: BoundedText,
}
```

Every Pre-turn Role view contains its own `memories: Vec<ModifiableMemoryPromptView>`. `Modifiable Knowledge` contains only grouped Facts/Rumors. Exact rendering:

```markdown
### Facts

- id: "fact_0001"
  content: "<content>"

### Rumors

- id: "rumor_0002"
  content: "<content>"
```

Memory renders under its owner Role as `id + content`. Do not render `source_id`, title, kind, scope, source, retrieval hint, or `memory_owner`.

The modifiable set is the union of baseline Relevant World Knowledge, retrieved World Knowledge, and Role-scoped retrieved Memories/Rumors, deduplicated by canonical ID. A Memory without a known owner Role or in the wrong Role bucket returns the exact typed partition error before the LLM call.

New Fact/Rumor output values require `retrieval_hint`; new Memory output does not. Add operations contain no ID. Validation assigns IDs through §3.4 before constructing `ValidatedKnowledgeOperation::Add`.

Operation legality is unchanged: Fact supports add/update but not delete; Rumor and Memory support add/update/delete. Grouping and short IDs do not widen mutation authority.

### 3.12 Persistence and Asset Migration Contract

Add `crates/aise/assets/persistence/mig/0020_narrative_knowledge_context.sql`.

The migration MUST start with this fail-fast guard before rebuilding either table:

```sql
CREATE TEMP TABLE narrative_knowledge_context_migration_guard (
    value INTEGER CONSTRAINT narrative_knowledge_context_legacy_data_present CHECK (value = 0)
);

INSERT INTO narrative_knowledge_context_migration_guard
SELECT 1 WHERE EXISTS (SELECT 1 FROM story_packs)
UNION ALL
SELECT 1 WHERE EXISTS (SELECT 1 FROM story_instances)
UNION ALL
SELECT 1 WHERE EXISTS (SELECT 1 FROM knowledge_entries);

DROP TABLE narrative_knowledge_context_migration_guard;
```

The guard covers persisted WorldBook v3 assets as well as long Knowledge IDs. The migration MUST NOT delete, partially rewrite, or silently preserve those rows. Fresh-store schema adds the Story-local allocator state and Fact/Rumor hint:

```sql
story_instances.knowledge_id_high_water INTEGER NOT NULL DEFAULT 0
knowledge_entries.retrieval_hint TEXT
```

with checks equivalent to:

```text
High-water -> integer >= 0
Fact/Rumor -> retrieval_hint is non-null and trim-non-empty
Memory     -> retrieval_hint is null
Fact ID    -> canonical fact_<sequence>
Rumor ID   -> canonical rumor_<sequence>
Memory ID  -> canonical memory_<sequence>
```

`knowledge_entries`, `knowledge_entry_entities`, and `knowledge_entry_topics` retain the same Story-scoped foreign-key relationship. Story creation writes the Seed count as high-water. Insert/update code writes the hint column for Fact/Rumor and `NULL` for Memory. Turn Commit advances high-water and applies Knowledge mutations in the same transaction after the existing Story revision check. `KnowledgeReadPort::list_index` selects the hint without loading the Knowledge body.

Update WorldBook import/export/schema fixtures to `aise_world_v4` only. Do not retain a v3 parser, converter, fallback-generated hint, or old-ID compatibility path. Operators must re-import StoryPack v5 assets containing WorldBook v4 after recreating/emptying a guarded development store.

### 3.13 Prompt Asset and Slot Contract

Update `crates/aise/assets/prompts/context-v2/slots.yaml` so every final RC key remains `var_type: string, required: true`:

| Profile | Removed key | Added/retained key |
|---|---|---|
| WriterPlanner | `narrative_plan` | `narrative_direction` |
| WriterPlanner | `knowledge_entry_index` | `knowledge_index` |
| CharacterThink | `relevant_character_knowledge` | none; nested in `target_character` |
| StoryGenerator | `relevant_writer_knowledge` | `relevant_knowledge` |
| StoryRepairer | `relevant_writer_knowledge` | `relevant_knowledge` |

Every Projector still provides every declared key. Empty optional rendered values use `Value::String(String::new())` as required by Runtime Context Empty Elision.

Update CSI/FTI terminology:

- WriterPlanner uses Relevant Knowledge, Character Index, Knowledge Index, and Narrative Direction.
- It copies the exact indexed `target_id` and never invents an ID.
- It treats index hints as discovery metadata, not story facts.
- Delete CSI/FTI wording that asks a model to inspect a Knowledge entry's declared `kind`, `scope`, `title`, or owner; group headings and Role containment now carry those semantics.
- It does not need a duplicate context gap solely to load a requested CharacterThink Role's Memories.
- StoryGenerator distinguishes Fact, Rumor, Memory, Narrative Direction, Story Goal, and Character Decision without exposing engine mechanics.
- CharacterThink treats nested Known Rumors/Memories as its authorized character context.

### 3.14 File / Directory Layout

```text
crates/aise/
├── assets/
│   ├── persistence/mig/0020_narrative_knowledge_context.sql
│   └── prompts/context-v2/
│       ├── slots.yaml
│       ├── csi/{writer-planner,character-think,story-generator}.md.j2
│       ├── rc/{writer-planner,character-think,story-generator,story-repairer,story-state-extractor}.md.j2
│       └── fti/{writer-planner,character-think,story-generator,story-state-extractor}.md.j2
├── src/
│   ├── prompt/
│   │   ├── narrative_direction.rs
│   │   └── tests/narrative_direction_tests.rs
│   ├── domain/
│   │   ├── ids.rs
│   │   ├── knowledge/{fact,rumor,memory,query,entry,hint}.rs
│   │   ├── asset/{world_book,story_pack}.rs
│   │   └── turn/{baseline,planning,retrieval,extraction}.rs
│   ├── context/{baseline_ctx_builder,retrieval_pipeline}.rs
│   ├── planning/{writer_planner_prompt,retrieval_plan_builder}.rs
│   ├── character/character_think_prompt.rs
│   ├── story/{instance_factory,story_generator_prompt,story_repairer_prompt,story_state_extractor_prompt}.rs
│   ├── validation/validation_pipeline.rs
│   └── persistence/{knowledge_read_port,sqlite_knowledge_reader,sqlite_store,sqlite_snapshot}.rs
└── tests/prompt_context_contract_tests.rs
```

Update tests only in existing dedicated `tests/<source>_tests.rs` files or the listed new Prompt test file. Keep every `mod.rs`/`lib.rs` index-only.

---

## 4. Behavior Rules

1. **NKR-1 — Shared direction**: WriterPlanner, StoryGenerator, and StoryRepairer MUST derive Narrative Direction through the same shared projector/renderer from the same immutable `NarrativePlan`.
2. **NKR-2 — Full world intent**: Every delivered World Event Intent MUST expose all semantic fields in §3.2, never only a count, effect ID, source node, or hidden event key.
3. **NKR-3 — Impulse isolation**: Character Impulse MUST be visible only to its target CharacterThink; WriterPlanner and StoryGenerator RC MUST contain none.
4. **NKR-4 — Impulse execution**: Every valid AI Character Impulse target MUST produce exactly one CharacterThink request after merge, even when Planner omitted it.
5. **NKR-5 — Loaded content**: Read-only Relevant Knowledge MUST render only grouped Fact/Rumor bodies and MUST contain no engine-generated per-item metadata.
6. **NKR-6 — Baseline preservation**: StoryGenerator/Repairer MUST receive both Baseline and supplemental World Knowledge; supplemental retrieval MUST NOT replace Baseline items.
7. **NKR-7 — Memory ownership**: Memory MUST exist only under its exact owner Role context; Global/World Memory and cross-Role Memory are typed errors.
8. **NKR-8 — Character authorization**: CharacterThink reads only its exact Character Context; no fallback to Writer or another Role partition is permitted.
9. **NKR-9 — Character retrieval**: An indexed Role target loads the Role view and ensures bounded Role-scoped Rumor/Memory retrieval; it MUST NOT be satisfied solely by unpartitioned Knowledge entity hits.
10. **NKR-10 — Automatic cognition retrieval**: Every indexed Role target or final CharacterThink request ensures one base Role-scoped Rumor/Memory retrieval without requiring a duplicate context gap; duplicate demand for one Role collapses.
11. **NKR-11 — Index minimality**: Each rendered index entry contains exactly `target_id` and `retrieval_hint`; kind is expressed by the group and identity by the one canonical ID.
12. **NKR-12 — Index completeness**: Character and Knowledge Index always render scope; empty groups and empty `entries` placeholders do not render.
13. **NKR-13 — Index membership**: Knowledge Index contains only unloaded Fact/Rumor targets; Character Index contains only unloaded Role targets; Memory is never indexed.
14. **NKR-14 — Exact target**: A Planner exact target resolves only when its string exactly matches a rendered index target and obeys the audience matrix in §3.7.
15. **NKR-15 — Hint boundary**: Retrieval hint is discovery metadata only; after an entry is loaded, its hint is not rendered or treated as story evidence.
16. **NKR-16 — Short canonical ID**: Every Fact/Rumor/Memory ID follows §3.4 and contains no global ID or random component.
17. **NKR-17 — No Prompt alias**: The engine MUST NOT allocate, store, or trace a separate short Prompt alias for a canonical Role/Knowledge target.
18. **NKR-18 — ID visibility**: Knowledge ID appears only in WriterPlanner indexes, StoryStateExtractor modifiable targets, structured update/delete targets, internal state, and diagnostics; it is absent from read-only planning/generation content.
19. **NKR-19 — Extractor exception**: StoryStateExtractor retains short IDs exactly where update/delete requires them and nests each Memory target under its owner Role.
20. **NKR-20 — Stable ordering**: Grouping and omission MUST preserve the existing deterministic rank/order within each semantic bucket.
21. **NKR-21 — Conflict preservation**: Distinct canonical IDs with identical or conflicting text MUST remain distinct internally; Fact and Rumor MUST never merge solely by text.
22. **NKR-22 — Empty elision**: Empty optional groups and parent sections follow Runtime Context Empty Elision; no `None.`, empty collection sentinel, or empty heading is reintroduced.
23. **NKR-23 — Output boundary**: Character Decision and StoryGenerator schemas are unchanged; StoryStateExtractor changes only by adding required Fact/Rumor `retrieval_hint` and accepting the new canonical ID grammar.
24. **NKR-24 — No added call**: This refactor adds no LLM call, retry, background task, unbounded scan, hidden queue, or cross-pipeline invocation.

### 4.1 Error Handling

- Invalid Knowledge ID input returns a typed `KnowledgeIdError`; production code never constructs an unchecked ID or uses `unwrap()`.
- Target text collision returns `WriterPlannerProjectionError::RetrievalTargetCollision` before Prompt composition.
- Invalid retrieval partition returns `RetrievedContextError::{InvalidKind, InvalidMemoryOwner, InvalidRole, ConflictingDuplicate}` as applicable.
- Missing Fact/Rumor hint fails the relevant asset/output schema. Present trim-empty or oversized hints map to existing asset `EmptyText`/`LimitExceeded`, Turn `ExtractionSchemaInvalid`, or persistence `InvalidRecord` errors with the exact field path; no invalid hint is defaulted or truncated.
- A Memory returned for Writer delivery, a Fact returned for Character delivery, or a Role mismatch MUST fail the Turn with code `retrieval_partition_invalid`; it MUST NOT be dropped or moved.
- A guarded database encountering migration `0020` MUST abort before mutation with constraint `narrative_knowledge_context_legacy_data_present`.
- Existing retrieval/store/Prompt errors propagate through current `TurnExecutionError` mappings; no absence or compatibility fallback is allowed.

### 4.2 Concurrency

- Narrative projection and Prompt projection remain synchronous and read-only.
- Role retrieval reads the existing immutable Snapshot and adds no cross-pipeline call.
- Knowledge ID generation uses the Snapshot's Story-local high-water plus bounded Add count and requires no global allocator, random generator, or extra database round trip.
- Existing Story-level Turn serialization, optimistic revision check, and transaction boundary serialize the high-water advance; the LLM limiter and retrieval bounds remain unchanged.
- No write guard may cross `.await`; no I/O or channel send may occur while a write guard is held.

### 4.3 Observability

- Narrative trace records direction count, world intent count, impulse count, and auto-added Think request count without logging Narrative bodies.
- Retrieval trace records Fact/Rumor/Memory counts by destination and retrieved Character count; replace `writer_item_count`/generic role item counts where they no longer match the typed structure.
- Validation/commit traces record Knowledge mutation counts by kind and operation; canonical IDs use structured fields when logged.
- Default logs MUST NOT contain Knowledge bodies, retrieval hints, Memory bodies, Player Input, or Role private context.
- Enabled Prompt trace naturally records the final RC; no parallel legacy RC or alias map is emitted.

---

## 5. Acceptance Criteria

### 5.1 Narrative Reconciliation

- [ ] Shared `NarrativeDirectionPromptView` and `WorldEventIntentPromptView` exist only in `prompt/narrative_direction.rs` and are used by WriterPlanner, StoryGenerator, and StoryRepairer — verified by `shared_narrative_direction_projection_is_stage_consistent`.
- [ ] WriterPlanner and StoryGenerator render byte-identical Narrative Direction bodies for the same plan — verified by `writer_planner_and_generator_share_narrative_direction_body`.
- [ ] World Event Intent renders category, non-empty participants, optional location, and description but no effect/source/node/event keys — verified by `world_event_intent_renders_full_semantics_without_bookkeeping`.
- [ ] Active Direction renders dramatic focus but no source node — verified by `active_direction_hides_source_node`.
- [ ] WriterPlanner and StoryGenerator prompt code contains no `active_goals`, generic `event_intents`, or `world_event_intent_count` — `rg -n 'active_goals|world_event_intent_count|event_intents' crates/aise/src/planning/writer_planner_prompt.rs crates/aise/src/story/story_generator_prompt.rs crates/aise/src/story/story_repairer_prompt.rs` returns zero matches.
- [ ] Character Impulse is absent from WriterPlanner/Generator/Repairer RC and present only for its exact CharacterThink target — verified by `character_impulse_is_target_scoped`.
- [ ] An impulse-only AI Role receives one automatic Think request; duplicate Planner/impulse targets still receive one request and all impulses — verified by `narrative_impulse_merges_character_think_request_once`.

### 5.2 Relevant Knowledge and Character Context

- [ ] WriterPlanner, StoryGenerator, and StoryRepairer Relevant Knowledge group bodies under Facts/Rumors with no read-only IDs or per-item metadata — verified by `relevant_knowledge_renders_grouped_content_only`.
- [ ] StoryGenerator receives Baseline Knowledge when Planner requests no supplemental retrieval — verified by `generator_preserves_baseline_relevant_knowledge`.
- [ ] Baseline and supplemental duplicates collapse by canonical ID without collapsing distinct Fact/Rumor IDs — verified by `world_knowledge_merge_uses_id_not_text`.
- [ ] CharacterThink has no `Relevant Character Knowledge / Memory` section and renders only target Role Known Rumors/Memories inside Target Character — verified by `character_think_nests_authorized_knowledge_under_target_role`.
- [ ] StoryGenerator attaches Role-scoped content only to the exact matching Character block — verified by `generator_attaches_character_context_by_role_id`.
- [ ] Memory in World, Fact in Character Context, or cross-Role owner mismatch returns `retrieval_partition_invalid` — typed partition tests pass.
- [ ] Prompt source contains no `StoryGeneratorKnowledgePromptView`, `KnowledgeScopePromptView`, `CharacterThinkKnowledgePromptView`, or `CharacterThinkKnowledgeKind` — `rg` returns zero matches under `crates/aise/src`.

### 5.3 Retrieval and Indexes

- [ ] `RetrievalTargetId` is deleted — `rg -n 'RetrievalTargetId' crates/aise/src crates/aise/tests` returns zero matches.
- [ ] Character Index entries render exactly one canonical Role target and hint; Knowledge Index entries render exactly one Fact/Rumor target and hint under typed groups — exact golden tests pass.
- [ ] Empty complete/prefiltered indexes retain only their exact scope and no entries key/group — `empty_grouped_indexes_preserve_scope` passes.
- [ ] Memory never appears in Knowledge Index — `knowledge_index_rejects_memory` passes.
- [ ] Provided Fact/Rumor bodies are excluded from Knowledge Index by internal canonical ID — `provided_world_knowledge_is_not_indexed` passes.
- [ ] Exact Character target loads a `RoleContextView` and deposits bounded Rumor/Memory results under the same Role, never a global bucket — `indexed_character_target_loads_role_context_bundle` passes.
- [ ] Every CharacterThink request creates one bounded Character Rumor/Memory request without a context gap — `character_think_automatically_retrieves_role_cognition` passes.
- [ ] Exact Character target plus CharacterThink demand for the same Role produces one base cognition request — `role_cognition_request_deduplicates_by_role` passes.
- [ ] Character-scoped extra gap without matching Think request remains rejected — existing audience validation tests pass.
- [ ] Fact exact target with Character audience and Role exact target with Character audience are rejected; Rumor follows §3.7 — `indexed_target_audience_matrix_is_enforced` passes.

### 5.4 IDs, Hints, and Persistence

- [ ] A Seed fixture with one Fact, one Rumor, and one Memory yields `fact_0001`, `rumor_0002`, and `memory_0003`; multi-entry fixtures follow the exact stable order from §3.4 — `seed_knowledge_ids_are_short_and_stable` passes.
- [ ] Turn additions continue from persisted `knowledge_id_high_water` in stable accepted Add order across kinds — `runtime_knowledge_ids_use_story_local_sequence` passes.
- [ ] Reusing the same base Snapshot and accepted change set yields the same candidate IDs; commit advances high-water atomically — `knowledge_id_allocation_is_retry_stable` passes.
- [ ] Delete never decrements high-water and a later addition never reuses a deleted ID — `knowledge_id_sequence_never_reuses_deleted_value` passes.
- [ ] Knowledge ID source contains no StoryId/TurnId formatting — `rg -n 'story_id.*seed:(fact|rumor|memory)|turn_id.*(fact|rumor|memory)' crates/aise/src/story/instance_factory.rs crates/aise/src/validation/validation_pipeline.rs` returns zero matches.
- [ ] Invalid prefix, too-short/zero/non-decimal sequence, redundant leading zero, whitespace, suffix, random UUID, SQLite-range overflow, and allocation overflow fail typed parsing/allocation — dedicated ID tests pass.
- [ ] `CharacterId` UUID and general `RoleId` syntax/lifetime policies remain unchanged; StoryInstance validation rejects only Role IDs matching a canonical Knowledge ID shape — existing `ids_tests` plus `role_id_cannot_collide_with_knowledge_id` pass.
- [ ] Fact/Rumor seed and runtime values require bounded `retrieval_hint`; Memory rejects it — asset and extraction schema tests pass.
- [ ] `RetrievalHint::MAX_BYTES` is exactly `256`; trim-empty and oversized UTF-8 values fail identically in asset, extraction, and persistence hydration paths — `retrieval_hint_domain_bound_is_uniform` passes.
- [ ] `KnowledgeReadPort::list_index` returns hints without selecting body content — SQLite query contract test passes.
- [ ] Migration `0020` succeeds after `0018`/`0019` on a fresh store with a zero high-water and rejects any stored StoryPack/StoryInstance/Knowledge row with named constraint `narrative_knowledge_context_legacy_data_present` and no mutation — migration integration tests pass.
- [ ] Story creation stores the exact Seed Knowledge count as high-water; a failed optimistic commit changes neither mutations nor high-water — allocator/store integration tests pass.
- [ ] Only `aise_world_v4` paired with `4.0` imports; v3 and crossed discriminator/version pairs are rejected with no compatibility parser — WorldBook import tests pass.

### 5.5 StoryStateExtractor and Prompt Contracts

- [ ] Modifiable Fact/Rumor targets render grouped short `id + content`; Memory renders only under its owner Pre-turn Role — `extractor_groups_modifiable_targets_and_nests_memories` passes.
- [ ] Read-only Relevant Knowledge has no IDs while modifiable target fixtures retain IDs — `knowledge_id_visibility_is_purpose_bound` passes.
- [ ] Fact/Rumor add/update output requires `retrieval_hint`; Memory does not — exact output-schema test passes.
- [ ] New add output cannot provide an ID; update/delete still require a canonical existing target — existing mutation reference tests plus canonical-ID tests pass.
- [ ] `slots.yaml` has the exact key changes in §3.13 and every key remains a required string — `runtime_context_projectors_preserve_slot_key_sets` passes.
- [ ] WriterPlanner/Generator/Repairer use `Narrative Direction`, Generator/Repairer use `Relevant Knowledge`, and WriterPlanner uses `Knowledge Index` — trusted prompt source tests pass.
- [ ] Empty groups/sections produce no heading, `None.`, empty list/map, or placeholder — Runtime Context Empty Elision tests pass.
- [ ] Player/Story/Knowledge text remains RC data and never enters CSI/FTI — prompt trust-boundary tests pass.

### 5.6 Quality Gates

- [ ] Old long-ID builders, flat Context DTOs, old Prompt DTOs, old slots, old WorldBook v3 fixtures, and superseded tests are deleted in the same change — targeted `rg` checks above return zero matches.
- [ ] New unit tests live in dedicated `tests/<source>_tests.rs`; source files contain no inline test bodies or comments.
- [ ] `cargo fmt --all --check` passes.
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo test --workspace` passes.

---

## 6. Out of Scope / Future Work

- Retrieval-hint quality metrics and authoring UI lint may be added after production trace evaluation; this spec requires only deterministic bounds and fixtures.
- BM25/Embedding may consume the same World/Character partitions later, but may not change their authority or Prompt contracts.
- Knowledge compaction or Memory reflection requires a separate design and may not reintroduce global Memory.

---

## 7. References

- Source design: [Narrative、Knowledge 与 Retrieval Context 收敛](../design/2026-08-17-narrative-knowledge-retrieval-design-gpt.md)
- Narrative authority: [NarrativePlan Projection and Semantic Resolution](CSI-RC-FTI/2026-08-13-narrative-plan-resolution-spec-gpt.md)
- Retrieval baseline: [Context Preparation and Retrieval](../design/2026-08-08-context-preparation-retrieval-design-gpt.md)
- Character identity baseline: [Character Card 与 Story Role Profile](../design/CSI-RC-FTI/2026-08-14-character-role-profile-design-gpt.md)
- Prompt framework: [CSI-RC-FTI Prompt Framework](CSI-RC-FTI/2026-08-11-csi-rc-fti-prompt-spec-gpt.md)
- Required predecessors: [Current Scene Removal](2026-08-17-current-scene-removal-spec-gpt.md), [Story Context Simplification](2026-08-17-story-context-simplification-spec-gpt.md), [Runtime Context Empty Elision](2026-08-17-runtime-context-empty-elision-spec-gpt.md)
- Guardrails: [`doc/agents/guardrails/`](../agents/guardrails/)
