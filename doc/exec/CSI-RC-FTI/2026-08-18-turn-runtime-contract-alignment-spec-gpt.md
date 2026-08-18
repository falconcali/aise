# Turn Runtime Contract Alignment — Spec

> **Model**: GPT-5
> **Date**: 2026-08-18
> **Status**: Proposed
> **Source Design**: [Narrative、Knowledge 与 Retrieval Context 收敛](../design/2026-08-17-narrative-knowledge-retrieval-design-gpt.md)
> **Phase**: N/A

---

## 1. Goal

Replace UUID-based Turn identity, ambiguous retrieval-index metadata, free-text Planner retrieval, Prompt-embedded rich Domain schemas, and divergent cast-policy behavior with one compact, provider-neutral, story-scoped Turn and LLM contract that preserves atomic state creation.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Delete `TurnId` and identify every committed Turn by the composite `(StoryId, TurnNumber)` key.
- Keep `TurnNumber` distinct from Story revision, Story segment sequence, retry count, repair count, and trace identity.
- Remove `RetrievalIndexScope`, every rendered `scope: complete|prefiltered` line, and every Planner/internal `query_text` retrieval field.
- Make WriterPlanner request only exact `target_id` values rendered in the complete Character or Knowledge Index; no suitable target means no context gap.
- Preserve `retrieval_hint` as Planner-only discovery metadata, normalize omitted short static hints from content, and require explicit dynamic Fact/Rumor hints.
- Project the canonical `StoryPack.meta.title` as `title` inside WriterPlanner and StoryGenerator/Repairer Story Profile RC without duplicating the title in the asset Story Profile schema.
- Introduce a provider-neutral structured-output contract and deterministic transport negotiation across native JSON Schema, forced strict tool, JSON-object, and Prompt-fallback providers.
- Replace rich model-facing Domain/DB schemas with dedicated slim LLM DTOs and deterministic application-side enrichment.
- Return StoryGenerator and StoryRepairer prose as bounded plain text instead of a one-field JSON wrapper.
- Align StoryGenerator, StoryStateExtractor, validation, and commit behavior with `CastPolicy::{Open, IncidentalOnly, Closed}`.
- Permit `CastPolicy::Open` to create and atomically persist material new AI Roles, their relationships, and Knowledge through exact engine-allocated Role IDs.
- Update Runtime Context, CSI/FTI assets, Domain and Turn contracts, LLM gateway/provider adapters, SQLite schema, validators, events, traces, tests, and conflicting documentation assertions.

### 2.2 Non-Goals

- Does not change `StoryId`, `TraceId`, `LlmCallId`, span IDs, idempotency keys, request digests, or provider request IDs.
- Does not assume `TurnNumber == StoryRevision` or `TurnNumber == StorySequence`; those counters retain separate semantics.
- Does not add BM25, embeddings, a reranker, a vector database, fuzzy Planner target matching, or an LLM retrieval-query rewrite.
- Does not expose Knowledge content, retrieval hints, provider schemas, Turn keys, or storage metadata to a stage that does not need them.
- Does not add a second LLM call to generate, summarize, validate, or repair a retrieval hint.
- Does not claim to deterministically judge whether a hint is a perfect semantic summary; the contract enforces provenance and bounds, while quality remains a Prompt/evaluation concern.
- Does not persist anonymous crowds, passers-by, or other non-material incidental characters as Roles.
- Does not create Character Cards for runtime Roles; a dynamic Role is StoryInstance-local and has `source_character = None`.
- Does not automatically retry a rejected structured-output transport with a weaker transport mode.
- Does not migrate historical trace files; new trace serialization uses `turn_number`, while old diagnostic files remain immutable artifacts.
- Does not add a compatibility reader for old Turn UUID database rows; migration behavior is the explicit fail-fast contract in §3.3.
- Does not change Narrative authority, condition semantics, correction-round limits, or the single atomic Turn commit boundary.

### 2.3 Implementation Constraints (for code generation)

- Implement after [Current Scene Removal](2026-08-17-current-scene-removal-spec-gpt.md), [Story Context Simplification](2026-08-17-story-context-simplification-spec-gpt.md), [Runtime Context Empty Elision](2026-08-17-runtime-context-empty-elision-spec-gpt.md), and [Narrative, Knowledge, and Retrieval Context Reconciliation](2026-08-17-narrative-knowledge-retrieval-spec-gpt.md).
- The predecessor specs own migrations `0018`, `0019`, and `0020`; this spec exclusively owns `0021_turn_runtime_contract_alignment.sql`.
- This spec supersedes these conflicting clauses in `2026-08-17-narrative-knowledge-retrieval-spec-gpt.md`: §2.2 TurnId, WriterPlanner-field, and output-schema non-goals; §3.5 mandatory static hints without normalization; §3.6 and §3.10 index scope; §3.8 Planner/internal `query_text`; §4 NKR-12 and NKR-23; §4.1 rejection of missing short static hints; and the corresponding acceptance criteria.
- This spec supersedes the “new Role creation is excluded” contract in [StoryGenerator and StoryStateExtractor Split](CSI-RC-FTI/2026-08-14-story-state-extractor-split-spec-gpt.md) whenever `cast_policy = open`.
- This spec generates final-form code. Do **not** keep fallback fields, serde aliases, compatibility shims, dual writes, legacy DB readers, parallel Prompt assets, or a second output-schema path.
- `StructuredOutputMode::PromptFallback` is the one explicit exception to the word “fallback”: it is a configured final provider transport, not a legacy compatibility path, automatic retry, or runtime downgrade.
- Delete every superseded type, field, generator, SQL column shape, Prompt variable, renderer branch, fixture, and test in the same change.
- Historical migrations remain immutable. Only migration `0021` may define the new storage shape.
- All LLM calls continue through the shared `LlmGateway` and `LlmLimiter`; no pipeline may call an `LlmProvider` directly.
- All mutable Story state, new Roles, relationships, Knowledge, allocator high-water values, Narrative state, Turn row, events, segment, and outbox rows commit in one existing Store transaction.
- Do not add code comments, inline test bodies, unbounded collections, unbounded queries, background tasks, or hidden retries.
- `R-ARCH-01/03/04/05`, `R-REFACTOR-01/02`, `R-LAYER-01/04/06`, `R-CODE-01/02/04/05/06/07`, `R-CONC-01/03/04`, `R-OBS-01/02/03`, and `R-AISE-01/02/03/06/07` remain mandatory.

---

## 3. Contracts

### 3.1 Final Vocabulary and Supersession

| Term | Final meaning | Explicitly not equivalent to |
|---|---|---|
| `TurnNumber` | Non-zero monotonically increasing committed-Turn number inside one Story | UUID, revision, segment sequence, attempt |
| `TurnKey` | Persisted composite key `(story_id, turn_number)` | A concatenated string ID |
| `TraceId` | Unique diagnostic identity for one execution attempt | Committed Turn identity |
| Character Index | Complete bounded global list of unloaded Role targets and hints | A prefiltered result set |
| Knowledge Index | Complete bounded global list of unloaded Fact/Rumor targets and hints | Loaded Knowledge or a query interface |
| `retrieval_hint` | Planner-only description of what retrieval will provide | Story evidence or Generator content |
| LLM DTO | Minimal semantic model-output shape | Domain entity, DB row, or commit change set |
| LLM output contract | Provider-neutral schema, compact fallback shape, decoder, and local validator | Prompt slot `OutputContract` |
| Material new Role | A new person who is identified, speaks, independently acts, gains a relationship, or is likely to recur | Anonymous incidental background presence |

The existing Prompt-slot rendered-text validator named `prompt::slot::OutputContract` is unrelated. The new transport abstraction MUST be named `LlmOutputContract`; do not overload or re-export the two under one name.

The motivating trace establishes this measurable baseline:

| Observation | Baseline |
|---|---|
| Turn identity | UUID repeated through Turn, context, retrieval, commit, and trace payloads |
| Empty indexes | `scope: complete` rendered without useful entries |
| Extractor schema | 9,120 bytes embedded in FTI; 10,557-byte FTI message |
| Extractor token ratio | 3,705 input tokens for 41 output tokens |
| Cast result | `open` Generator created a speaking stranger; Extractor returned no Role or Knowledge change |

The acceptance limits in §5 are regressions against these facts, not optional optimization targets.

### 3.2 Turn Identity Contract

Replace `TurnId` with validated numeric and composite types:

```rust
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum TurnNumberError {
    #[error("turn number must be non-zero")]
    Zero,
    #[error("turn number exceeds SQLite signed integer range")]
    ExceedsSqliteRange,
    #[error("turn number overflow")]
    Overflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u64", into = "u64")]
pub struct TurnNumber(NonZeroU64);

impl TurnNumber {
    pub fn try_new(value: u64) -> Result<Self, TurnNumberError> {
        let value = NonZeroU64::new(value).ok_or(TurnNumberError::Zero)?;
        if value.get() > i64::MAX as u64 {
            return Err(TurnNumberError::ExceedsSqliteRange);
        }
        Ok(Self(value))
    }

    pub fn get(self) -> u64 {
        self.0.get()
    }

    pub fn checked_next(self) -> Result<Self, TurnNumberError> {
        let next = self.get().checked_add(1).ok_or(TurnNumberError::Overflow)?;
        Self::try_new(next).map_err(|_| TurnNumberError::Overflow)
    }
}

impl TryFrom<u64> for TurnNumber {
    type Error = TurnNumberError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<TurnNumber> for u64 {
    fn from(value: TurnNumber) -> Self {
        value.get()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnKey {
    pub story_id: StoryId,
    pub turn_number: TurnNumber,
}

#[derive(Debug, Clone)]
pub struct TurnIdentity {
    key: TurnKey,
    idempotency_key: IdempotencyKey,
    started_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommittedTurnResult {
    pub turn_number: TurnNumber,
    pub story_revision: StoryRevision,
    pub story_text: String,
    pub llm_usage: LlmUsageAggregate,
    pub llm_calls: Vec<LlmCallUsage>,
}
```

`StoryInfo` exposes the persisted high-water:

```rust
pub struct StoryInfo {
    pub story_id: StoryId,
    pub created_at_ms: i64,
    pub base_revision: StoryRevision,
    pub last_committed_turn_number: u64,
}
```

Zero is valid only for `StoryInfo.last_committed_turn_number` before the first committed Turn. A `TurnNumber` itself is always non-zero and must fit SQLite's signed 64-bit positive range.

`TurnIdentity` exposes `key() -> &TurnKey`, `story_id() -> &StoryId`, `turn_number() -> TurnNumber`, `idempotency_key() -> &IdempotencyKey`, and `started_at_ms() -> i64`. `TurnExecutionContext` delegates `turn_key()` and `turn_number()`; no caller reconstructs a `TurnKey` from formatted text.

The execution sequence is exact:

1. Validate the external request and acquire the existing per-Story coordinator permit.
2. Load `StoryInfo` and perform the existing idempotency lookup.
3. If an idempotent committed result exists with the same digest, return its stored `turn_number` without allocating or executing a candidate.
4. Otherwise compute the candidate as `last_committed_turn_number + 1` with checked arithmetic and construct `TurnIdentity`.
5. Every repair and state re-extraction inside that execution keeps the same candidate number.
6. Failed, cancelled, timed-out, or conflicting executions do not update the high-water; a later attempt may reuse the same candidate number and is distinguished by `TraceId`.
7. Commit verifies Story revision, Narrative revision, allocator high-waters, and `stories.last_turn_number + 1 == commit.turn.number` inside the transaction.
8. Only a successful atomic commit updates `stories.last_turn_number` to the candidate.

Replace Turn-bearing Domain contracts:

```rust
pub struct StoryTurn {
    pub number: TurnNumber,
    pub sequence: StorySequence,
    pub player_input: String,
    pub story_text: String,
    pub created_at: i64,
}

pub enum StorySegmentOrigin {
    Opening,
    Turn { turn_number: TurnNumber },
}

pub struct StoryEvent {
    pub id: EventId,
    pub turn_number: TurnNumber,
    pub seq: u32,
    pub kind: EventKind,
    pub payload: Value,
}

pub struct StoryTurnView {
    pub turn_number: TurnNumber,
    pub sequence: StorySequence,
    pub player_input: String,
    pub story_text: String,
    pub created_at: i64,
}

pub struct OutboxRecord {
    pub id: String,
    pub story_id: StoryId,
    pub turn_number: TurnNumber,
    pub event_type: String,
    pub payload: Value,
    pub created_at: i64,
}

pub enum KnowledgeSource {
    Seed { pack_id: PackId, pack_digest: Sha256Digest },
    CommittedTurn { turn_number: TurnNumber },
}

pub struct PendingNarrativeEffect {
    pub created_by_turn: Option<TurnNumber>,
}

pub struct NarrativeRuntimeState {
    pub activation_turns: BTreeMap<NarrativeNodeKey, TurnNumber>,
}
```

All unlisted semantic fields in these two Narrative structs remain unchanged.

`KnowledgeSource::CommittedTurn` and Narrative runtime values are already owned by one StoryInstance, so they store only `TurnNumber`. Cross-Story APIs and persistence always carry `TurnKey` or separate `story_id` and `turn_number` columns.

Deterministic infrastructure IDs use the composite key but are not model-visible:

```text
EventId     = "<story_id>:turn:<turn_number>:event:<event_index>"
Outbox ID   = "<story_id>:turn:<turn_number>:outbox:<event_index>"
Segment ID  = "<story_id>:turn:<turn_number>"
```

Delete all production definitions and call sites for:

```text
TurnId
IdGenerator::new_turn_id
UuidIdGenerator
TurnIdentity::turn_id
TurnExecutionContext::turn_id
CommittedTurnResult.turn_id
StoryTurn.id
StoryEvent.turn_id
OutboxRecord.turn_id
```

UUID generation remains permitted only for `TraceId`, `LlmCallId`, span IDs, provider IDs, and other explicitly non-Turn infrastructure identities.

### 3.3 Turn Persistence and Event Protocol

Add `crates/aise/assets/persistence/mig/0021_turn_runtime_contract_alignment.sql`. The migration MUST first reject any committed legacy Turn-bearing data before rebuilding tables:

```sql
CREATE TEMP TABLE turn_runtime_alignment_guard (
    value INTEGER CONSTRAINT turn_runtime_alignment_legacy_turn_data CHECK (value = 0)
);

INSERT INTO turn_runtime_alignment_guard
SELECT 1 WHERE EXISTS (SELECT 1 FROM story_turns)
UNION ALL
SELECT 1 WHERE EXISTS (SELECT 1 FROM story_events)
UNION ALL
SELECT 1 WHERE EXISTS (SELECT 1 FROM story_segments WHERE origin = 'turn')
UNION ALL
SELECT 1 WHERE EXISTS (SELECT 1 FROM outbox);
```

The guard continues with the exact legacy JSON checks before any table mutation:

```sql
INSERT INTO turn_runtime_alignment_guard
SELECT 1
FROM knowledge_entries
WHERE json_type(source_json, '$.committed_turn.turn_id') = 'text'
   OR json_type(payload_json, '$.value.source.committed_turn.turn_id') = 'text';

INSERT INTO turn_runtime_alignment_guard
SELECT 1
FROM story_instances instance
WHERE EXISTS (
    SELECT 1
    FROM json_each(instance.narrative_state_json, '$.activation_turns') activation
    WHERE activation.type = 'text'
)
OR EXISTS (
    SELECT 1
    FROM json_each(instance.narrative_state_json, '$.pending_effects') effect
    WHERE json_type(effect.value, '$.created_by_turn') = 'text'
);
```

All three JSON columns retain their predecessor `json_valid` checks, so `json_each` never runs against invalid stored JSON. There is no UUID-to-number converter in this change.

The final relevant storage shape is:

```sql
ALTER TABLE stories ADD COLUMN last_turn_number INTEGER NOT NULL DEFAULT 0
    CHECK (last_turn_number >= 0);

ALTER TABLE story_instances ADD COLUMN role_id_high_water INTEGER NOT NULL DEFAULT 0
    CHECK (role_id_high_water >= 0);

CREATE TABLE story_turns_new (
    story_id TEXT NOT NULL REFERENCES stories(id) ON DELETE CASCADE,
    turn_number INTEGER NOT NULL CHECK (turn_number > 0),
    player_input TEXT NOT NULL,
    story_text TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status = 'ok'),
    created_at INTEGER NOT NULL,
    idempotency_key TEXT NOT NULL,
    request_digest TEXT NOT NULL,
    base_revision INTEGER NOT NULL CHECK (base_revision >= 0),
    committed_revision INTEGER NOT NULL CHECK (committed_revision > base_revision),
    result_json TEXT NOT NULL CHECK (json_valid(result_json)),
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    PRIMARY KEY (story_id, turn_number),
    UNIQUE (story_id, idempotency_key),
    UNIQUE (story_id, sequence)
);

CREATE TABLE story_events_new (
    id TEXT PRIMARY KEY,
    story_id TEXT NOT NULL,
    turn_number INTEGER NOT NULL,
    seq INTEGER NOT NULL CHECK (seq >= 0),
    kind TEXT NOT NULL,
    payload TEXT NOT NULL CHECK (json_valid(payload)),
    UNIQUE (story_id, turn_number, seq),
    FOREIGN KEY (story_id, turn_number)
        REFERENCES story_turns_new(story_id, turn_number)
        ON DELETE CASCADE
);
```

Rebuild `story_segments` so a Turn-origin row has `turn_number INTEGER`, a composite foreign key `(story_id, turn_number)`, and `UNIQUE(story_id, turn_number)`. Opening rows have `turn_number IS NULL`; Turn rows have a positive value. Rebuild `outbox` with `turn_number INTEGER NOT NULL` and no `turn_id` column. Rename legacy `world_id` in `story_turns` to `story_id`; no final query or type may retain the legacy name.

Because the guard requires all committed Turn/event/outbox rows to be empty, migration copy statements copy only existing opening `story_segments` rows and preserve their `id`, `story_id`, `sequence`, `story_text`, and `created_at` exactly. Every existing Story receives `last_turn_number = 0`.

Before adding `role_id_high_water`, the same migration guard rejects any existing `story_instances.roles_json` key that matches the canonical dynamic Role grammar from §3.9:

```sql
INSERT INTO turn_runtime_alignment_guard
SELECT 1
FROM story_instances instance, json_each(instance.roles_json) role
WHERE role.key GLOB 'role_[0-9]*'
  AND substr(role.key, 6) NOT GLOB '*[^0-9]*'
  AND (
      (length(substr(role.key, 6)) = 4
       AND CAST(substr(role.key, 6) AS INTEGER) BETWEEN 1 AND 9999)
      OR
      (length(substr(role.key, 6)) > 4
       AND substr(role.key, 6, 1) BETWEEN '1' AND '9')
  );
```

This prevents an authored persisted Role from becoming indistinguishable from an allocator-owned Role. Existing semantic Role keys remain untouched.

`result_json` serializes `turn_number`, never `turn_id`. Idempotency lookup remains keyed by `(story_id, idempotency_key)` and returns the complete stored result.

Turn event payloads use these shapes:

```json
{
  "type": "stage_started",
  "payload": { "turn_number": 1, "stage": "writer_planner" }
}
```

```json
{
  "type": "committed",
  "payload": { "turn_number": 1, "story_revision": 1, "replayed": false }
}
```

`validation_completed`, `failed`, `cancelled`, and `conflict` use `turn_number` when a candidate identity exists. A request rejected before Story lookup/candidate construction omits `turn_number`; it must not fabricate zero or a random identifier. `trace_completed` carries the final `TurnTrace` object.

`TurnEvent::turn_number()` returns `Option<TurnNumber>` by value. Story history continues paging by `StorySequence`, but each `StoryTurnView` exposes its independent `turn_number`; history code never treats sequence as the Turn key.

The final trace headers are:

```rust
pub struct TurnTrace {
    pub trace_id: TraceId,
    pub story_id: String,
    pub turn_number: Option<TurnNumber>,
    pub started_at_ms: u64,
    pub ended_at_ms: u64,
    pub duration_ms: u64,
    pub dropped_span_count: u32,
    pub spans: Vec<TraceSpan>,
}

pub struct TurnData {
    pub story_id: String,
    pub turn_number: Option<TurnNumber>,
    pub player_input: String,
    pub status: String,
    pub error: Option<String>,
}

pub struct PersistData {
    pub story_id: String,
    pub turn_number: TurnNumber,
    pub status: String,
    pub error: Option<String>,
    pub latency_ms: u64,
}
```

### 3.4 Complete Index and Exact Retrieval Contract

Delete `RetrievalIndexScope` and both scope fields from the final baseline:

```rust
pub struct BaselineContext {
    pub story_title: BoundedText,
    pub story_profile: StoryProfile,
    pub role_index: Vec<RoleIndexEntry>,
    pub knowledge_index: Vec<KnowledgeIndexEntry>,
}
```

All other `BaselineContext` fields retain the predecessor contract.

The final model-output and internal request shapes contain no query text:

```rust
pub struct PlannerWriterContextGapDto {
    pub target_id: String,
    pub reason: String,
}

pub struct PlannerCharacterContextGapDto {
    pub role_id: String,
    pub target_id: String,
    pub reason: String,
}

pub struct CharacterThinkRequestDto {
    pub role_id: String,
    pub reason: String,
}

pub struct KnowledgeRetrievalRequest {
    pub delivery: KnowledgeDelivery,
    pub target_source_id: Option<KnowledgeSourceId>,
    pub knowledge_kinds: Vec<KnowledgeKind>,
    pub entities: Vec<KnowledgeEntity>,
    pub topics: Vec<TopicKey>,
    pub reason: BoundedText,
    pub origin: RetrievalRequestOrigin,
    pub signal_priority: u8,
}
```

WriterPlanner's dedicated LLM DTO separates Writer and Character gaps to avoid a tagged/nullable audience union:

```rust
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriterPlannerOutputDto {
    pub story_goal: String,
    pub writer_context_gaps: Vec<PlannerWriterContextGapDto>,
    pub character_context_gaps: Vec<PlannerCharacterContextGapDto>,
    pub character_think_requests: Vec<CharacterThinkRequestDto>,
}
```

Projection converts both gap arrays into the existing typed Domain retrieval plan. Rules are exact:

- `target_id` must byte-for-byte match one target rendered in the current Turn's Character or Knowledge Index map.
- A Writer gap may target a Role, Fact, or Rumor according to the predecessor target matrix.
- A Character gap must name the matching CharacterThink `role_id` and may target only Knowledge authorized for that Role.
- A Memory is never globally indexed and cannot be a Planner target.
- If no suitable indexed target exists, WriterPlanner emits no gap. It must not paraphrase a query, invent a target, or use content text as an ID.
- Automatic Role cognition retrieval and Narrative entity/topic retrieval remain deterministic internal requests; neither path adds a free-text query field.

Render only non-empty complete indexes:

```markdown
## Character Index

### Retrievable Characters

- target_id: "keeper"
  retrieval_hint: "The lodge keeper who knows the surrounding forest."

## Knowledge Index

### Retrievable Facts

- target_id: "fact_0001"
  retrieval_hint: "The lodge's remote location and access."
```

There is no `scope`, `entries`, `None.`, empty child heading, or empty parent Index section. The CSI/FTI definition is sufficient: an Index is a complete list of currently retrievable unloaded targets; a hint describes the information behind a target; only an exact `target_id` is executable.

Complete remains bounded: Story/Knowledge materialization MUST ensure the total Role and Fact/Rumor counts can fit the configured global index limits. Exceeding a limit returns `ContextError::IndexLimitExceeded { index, actual, maximum }` before the WriterPlanner call. It must not silently prefilter, truncate, sample, or relabel an incomplete index as complete.

Delete:

```text
RetrievalIndexScope
BaselineContext.role_index_scope
BaselineContext.knowledge_index_scope
PlannerContextGap.query_text
KnowledgeRetrievalRequest.query_text
RequestDraft.query_text
PlannerConfig.max_query_bytes
retrieval_selector_exclusivity
query_text_invalid
```

Player Input sizing must use the existing Turn/content limit, not the deleted Planner query limit.

### 3.5 Retrieval Hint Contract

The predecessor `RetrievalHint` newtype remains authoritative:

```rust
pub struct RetrievalHint(BoundedText);

impl RetrievalHint {
    pub const MAX_BYTES: usize = 256;
    pub fn try_new(value: impl Into<String>) -> Result<Self, RetrievalHintError>;
}

pub fn normalize_static_retrieval_hint(
    content: &BoundedText,
    configured: Option<RetrievalHint>,
) -> Result<RetrievalHint, AssetValidationError>;

pub struct FactSeed {
    pub content: BoundedText,
    #[serde(default)]
    pub retrieval_hint: Option<RetrievalHint>,
}

pub struct RumorSeed {
    pub content: BoundedText,
    #[serde(default)]
    pub retrieval_hint: Option<RetrievalHint>,
}

pub struct WorldFact {
    pub retrieval_hint: RetrievalHint,
}

pub struct SharedRumor {
    pub retrieval_hint: RetrievalHint,
}
```

These snippets show changed fields; all predecessor semantic fields remain.

Static normalization is exact:

1. A configured trim-non-empty hint of at most 256 UTF-8 bytes is retained exactly.
2. If the hint is absent and `content` is at most 256 UTF-8 bytes, the complete content becomes the hint.
3. If the hint is absent and content exceeds 256 UTF-8 bytes, import fails with `AssetValidationCode::RetrievalHintRequired` at the exact Fact/Rumor hint path.
4. Normalization happens during StoryPack/WorldBook build/import before canonical serialization, digest calculation, freezing, or persistence.
5. A `ValidatedStoryPack` and every stored Fact/Rumor always contain a non-optional `RetrievalHint`; runtime reads have no missing-hint fallback.

Dynamic State Extractor Fact/Rumor additions and updates always carry both `content` and `retrieval_hint`. The hint may equal content exactly when the content is concise. When content is longer than 256 bytes, the hint remains a concise description sufficient for Planner relevance judgment and must not introduce information absent from content. Delete operations contain only the exact target ID.

The application enforces trim-non-empty and byte bounds locally. CSI/FTI enforces semantic faithfulness. A dynamic output missing a Fact/Rumor hint is an invalid structured output and enters state re-extraction; it is not defaulted after the LLM call. Every Fact/Rumor update replaces the stored hint together with content, preventing a stale hint.

Memory remains Role-scoped and does not enter the global Knowledge Index. It has no global `retrieval_hint`. If a future Role-local Memory index is introduced, it requires a separate contract and must preserve Role ownership.

### 3.6 Story Title Projection Contract

`StoryPack.meta.title` is the single canonical title. Do not add a duplicate `title` field to the asset `StoryProfile` object.

Add the canonical value to Snapshot/Baseline projection:

```rust
pub struct StoryReadSnapshotParts {
    pub story_title: BoundedText,
}

pub struct StoryReadSnapshot {
    story_title: BoundedText,
}

pub struct BaselineContext {
    pub story_title: BoundedText,
    pub story_profile: StoryProfile,
}

pub struct StoryProfilePromptView {
    pub title: BoundedText,
    pub language: BoundedText,
    pub genre: Vec<BoundedText>,
    pub themes: Vec<BoundedText>,
    pub tone: Vec<BoundedText>,
    pub point_of_view: BoundedText,
    pub tense: BoundedText,
}
```

SQLite Snapshot loading selects `json_extract(story_packs.pack_json, '$.meta.title')` in the bounded pack projection query; it must not load the full Pack merely to obtain the title. Empty/invalid/oversized stored title is `StoreError::Serialization::InvalidStoryState` or the existing content-limit error.

WriterPlanner and StoryGenerator/Repairer render `title` as the first Story Profile key. CharacterThink and StoryStateExtractor do not receive the title. This prevents a potentially revealing title from becoming character knowledge and avoids irrelevant extraction attention.

Visibility is fixed:

| Stage | Story title |
|---|---|
| WriterPlanner | visible |
| CharacterThink | absent |
| StoryGenerator | visible |
| StoryRepairer | visible through Generator context |
| StoryStateExtractor | absent |

### 3.7 Provider-Neutral LLM Output Contract

Add the transport-independent contract in `crates/aise/src/llm/output_contract.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructuredOutputMode {
    NativeJsonSchema,
    ForcedStrictTool,
    JsonObject,
    PromptFallback,
}

#[derive(Debug, Clone)]
pub struct ProviderTransportCapabilities {
    pub encodable_modes: BTreeSet<StructuredOutputMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelStructuredOutputCapabilities {
    pub provider: String,
    pub model: String,
    pub supported_modes: Vec<StructuredOutputMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructuredOutputConfig {
    pub default_modes: Vec<StructuredOutputMode>,
    pub model_capabilities: Vec<ModelStructuredOutputCapabilities>,
}

pub struct LlmOutputContract<T> {
    pub name: &'static str,
    pub schema: Arc<Value>,
    pub compact_prompt_shape: Arc<str>,
    pub validate: Arc<dyn Fn(&T) -> Result<(), LlmOutputViolation> + Send + Sync>,
}

pub enum CompletionOutputRequest {
    Text,
    Structured(ResolvedStructuredOutputRequest),
}

pub struct ResolvedStructuredOutputRequest {
    pub contract_name: &'static str,
    pub schema: Arc<Value>,
    pub schema_hash: Sha256Digest,
    pub mode: StructuredOutputMode,
}
```

`LlmConfig` adds `pub structured_output: StructuredOutputConfig`. `StructuredOutputConfig::default()` is `default_modes = [PromptFallback]` and an empty override list. Validation rejects an empty default, duplicate modes, duplicate `(provider, model)` entries, empty provider/model strings, and an empty override mode list. Exact `(provider_name, model)` override wins as a complete replacement for `default_modes`; there is no provider-only or fuzzy model match. Native JSON Schema, strict tool, and JSON-object support must therefore be explicitly declared for the configured deployment/model. The concrete provider adapter separately reports which wire modes it can encode. A mode is eligible only when it exists in both sets.

Selection uses this fixed preference order:

```text
native_json_schema
forced_strict_tool
json_object
prompt_fallback
```

Selection runs once before a provider request. If the intersection is empty, return `LlmProtocolErrorKind::StructuredOutputUnsupported`. A provider rejection, malformed tool response, schema rejection, or decode failure does not trigger a second request in a weaker mode.

Extend `LlmProvider` without provider-name branching in a pipeline:

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn provider_name(&self) -> &'static str;
    fn transport_capabilities(&self) -> ProviderTransportCapabilities;
    async fn complete(&self, req: &CompletionRequest) -> Result<LlmCompletion, LlmProviderError>;
    async fn complete_stream(
        &self,
        req: &CompletionRequest,
        on_delta: DeltaSink,
    ) -> Result<LlmCompletion, LlmProviderError>;
    async fn embed(&self, req: &EmbeddingRequest) -> Result<EmbeddingOutput, LlmProviderError>;
}
```

The request/spec boundary is explicit:

```rust
pub struct CompletionSpec {
    pub messages: Vec<ChatMessage>,
    pub max_output_tokens: u32,
    pub purpose: LlmCallPurpose,
    pub output: CompletionOutputSpec,
}

pub enum CompletionOutputSpec {
    Text,
    Structured,
}

pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: u32,
    pub temperature: f32,
    pub purpose: LlmCallPurpose,
    pub output: CompletionOutputRequest,
}
```

Expose two explicit Gateway paths and delete the ambiguous all-purpose `complete_composed` entry point:

```rust
pub async fn complete_text_composed(
    &self,
    scope: TurnLlmCallScope<'_>,
    input: PromptCompositionInput,
    max_output_tokens: u32,
    purpose: LlmCallPurpose,
) -> Result<LlmCompletion, LlmError>;

pub async fn complete_structured_composed<T>(
    &self,
    scope: TurnLlmCallScope<'_>,
    input: PromptCompositionInput,
    max_output_tokens: u32,
    purpose: LlmCallPurpose,
    contract: LlmOutputContract<T>,
) -> Result<StructuredLlmCompletion<T>, LlmError>
where
    T: DeserializeOwned + Send + 'static;
```

`complete_structured_composed` always performs this one path:

1. Compose CSI/RC/FTI without an embedded full schema.
2. Resolve one mode for the configured provider/model.
3. For `JsonObject` and `PromptFallback`, append one trusted System message generated from `compact_prompt_shape`; for native JSON Schema and strict tool, append no output-shape Prompt message.
4. Reserve budget and acquire the existing shared limiter exactly once.
5. Send the resolved output request through the provider adapter.
6. Normalize provider content or strict-tool arguments to one UTF-8 JSON object string.
7. Deserialize with `deny_unknown_fields` into `T`.
8. Run `contract.validate` even when the provider claims strict conformance.
9. Return the typed value plus usage/charge/finish metadata.

`OpenAiCompatProvider` maps modes as follows:

| Mode | Request mapping | Response source |
|---|---|---|
| `NativeJsonSchema` | `response_format.type = json_schema`, exact contract name, `strict = true`, schema | `message.content` |
| `ForcedStrictTool` | one strict function tool using the schema and forced exact `tool_choice` | exactly one matching `tool_calls[0].function.arguments` |
| `JsonObject` | `response_format.type = json_object` | `message.content` |
| `PromptFallback` | no provider-specific structured field | `message.content` |

A strict-tool response with zero calls, multiple calls, a different function name, non-empty competing prose, or invalid arguments is `LlmProtocolErrorKind::InvalidStructuredOutput`. Provider adapters normalize transport differences; pipelines never inspect tool calls or provider response envelopes.

The canonical schema hash is SHA-256 over recursively key-sorted compact JSON. Contract names are stable versioned identifiers:

```text
writer_plan.v2
character_decision.v2
story_state_extraction.v2
```

Each DTO module exposes one constructor; no pipeline assembles schema fragments:

```rust
pub fn writer_plan_output_contract(limits: PlannerOutputLimits) -> LlmOutputContract<WriterPlannerOutputDto>;

pub fn character_decision_output_contract(
    limits: CharacterDecisionLimits,
) -> LlmOutputContract<CharacterDecisionDto>;

pub fn story_state_extraction_output_contract(
    limits: StoryStateExtractionLimits,
) -> LlmOutputContract<StoryStateExtractionDto>;
```

Contract tests compare required/property field sets against the DTO fixtures and snapshot both provider schema and compact Prompt shape. A field change without updating both representations fails tests; pipelines never carry a copied schema string.

### 3.8 Slim LLM DTO and Application Enrichment Contract

Use LLM-only DTOs that contain semantic choices and exact executable references, not storage representation. Every DTO uses `serde(deny_unknown_fields)`. Required arrays are always present and use `[]` when empty. Optional model strings use an empty string sentinel and are normalized to `None` after decoding; the LLM schemas contain no nullable union for those fields.

CharacterThink uses:

```rust
pub struct CharacterDecisionDto {
    pub decision: String,
    pub suggested_utterance: String,
}
```

`suggested_utterance = ""` means absent. `decision` must be trim-non-empty.

StoryStateExtractor uses one flat semantic envelope with no `oneOf` mutation union:

```rust
pub struct StoryStateExtractionDto {
    pub new_roles: Vec<NewRoleDto>,
    pub role_states: Vec<RoleStateDto>,
    pub relationship_states: Vec<RelationshipStateDto>,
    pub add_facts: Vec<FactDraftDto>,
    pub update_facts: Vec<FactUpdateDto>,
    pub add_rumors: Vec<RumorDraftDto>,
    pub update_rumors: Vec<RumorUpdateDto>,
    pub delete_rumor_ids: Vec<String>,
    pub add_memories: Vec<MemoryDraftDto>,
    pub update_memories: Vec<MemoryUpdateDto>,
    pub delete_memory_ids: Vec<String>,
    pub narrative_condition_judgments: Vec<NarrativeConditionJudgmentDto>,
    pub cast_policy_violations: Vec<String>,
}

pub struct NewRoleDto {
    pub role_id: String,
    pub name: String,
    pub role_label: String,
    pub narrative_function: String,
    pub background: String,
    pub appearance: String,
    pub personality: String,
    pub speaking_style: String,
    pub location: String,
    pub goals: Vec<String>,
    pub attributes: BTreeMap<String, ScalarValue>,
}

pub struct RoleStateDto {
    pub role_id: String,
    pub location: String,
    pub goals: Vec<String>,
    pub attributes: BTreeMap<String, ScalarValue>,
}

pub struct RelationshipStateDto {
    pub source_role_id: String,
    pub target_role_id: String,
    pub kind: String,
    pub trust: i64,
}

pub struct FactDraftDto {
    pub content: String,
    pub retrieval_hint: String,
}

pub struct FactUpdateDto {
    pub id: String,
    pub content: String,
    pub retrieval_hint: String,
}

pub struct RumorDraftDto {
    pub content: String,
    pub retrieval_hint: String,
    pub source_role_id: String,
    pub truth_value: TruthValue,
}

pub struct RumorUpdateDto {
    pub id: String,
    pub content: String,
    pub retrieval_hint: String,
    pub source_role_id: String,
    pub truth_value: TruthValue,
}

pub struct MemoryDraftDto {
    pub owner_role_id: String,
    pub memory_kind: String,
    pub content: String,
}

pub struct MemoryUpdateDto {
    pub id: String,
    pub memory_kind: String,
    pub content: String,
}

pub struct NarrativeConditionJudgmentDto {
    pub condition_key: String,
    pub status: NarrativeConditionStatus,
    pub evidence: String,
    pub reason: String,
}
```

`source_role_id = ""` means no known source. Empty optional `NewRoleDto` profile/background strings map to `None`; the extractor must prefer empty strings over invented traits. `controller`, `source_character`, and dialogue examples never appear in the LLM DTO. A committed runtime Role is constructed with:

```text
controller        = RoleController::Ai
source_character  = None
dialogue_examples = []
```

Remove these fields from model-facing Fact/Rumor/Memory values:

```text
proposition
claim
entities
topics
salience
owner on Memory update
storage integer bounds
revision/source/provenance
```

Application enrichment is deterministic and adds no LLM call:

```rust
pub const DEFAULT_RUNTIME_KNOWLEDGE_SALIENCE: u8 = 128;

pub fn enrich_extracted_knowledge(
    dto: &StoryStateExtractionDto,
    snapshot: &StoryReadSnapshot,
    accepted_new_roles: &[StoryRole],
) -> Result<Vec<ValidatedKnowledgeMutation>, ExtractionEnrichmentError>;
```

Exact enrichment rules:

- New Fact/Rumor/Memory uses salience `128`.
- Update preserves the target's existing salience.
- Fact `proposition` and Rumor `claim` are `None` for additions and become `None` when content is updated.
- Topics are recomputed by the existing deterministic topic matcher over `content + "\n" + retrieval_hint` for Fact/Rumor and over Memory content for Memory.
- Fact entities are empty.
- Rumor entities contain the resolved `source_role_id` when non-empty, otherwise empty.
- Memory entities contain exactly its immutable owner Role.
- Update replaces computed topics/entities according to these rules; it does not preserve stale model-invisible semantic indexes.
- Collections are canonicalized and remain subject to the existing bounded Domain validators.

`TruthValue` and `memory_kind` remain model-facing because they are semantic state, not storage metadata. Relationship `trust` remains semantic, but its model schema says only `integer`; local conversion rejects values outside the Domain's accepted range without exposing Rust/SQLite integer limits in the Prompt contract.

The generated provider JSON Schemas MUST:

- omit the `$schema` URI;
- use `additionalProperties: false` on objects;
- contain no repeated Fact/Rumor/Memory `oneOf` operation tree;
- contain no `maxLength: 131072`, `-32768`, `32767`, `255`, DB column, Rust integer type, or revision bound;
- retain only semantic item/text limits needed to protect the Turn budget;
- be derived from the dedicated DTO contract, never from Domain/DB serialization.

`story_state_extraction.v2` canonical schema must serialize to at most 6,144 UTF-8 bytes. Its compact Prompt-fallback shape must be at most 1,536 UTF-8 bytes. These are test-enforced limits, not approximate targets.

StoryGenerator and StoryRepairer use `complete_text_composed`. Normalize the provider text by rejecting trim-empty output and enforcing `max_story_text_bytes`, then construct the existing internal bounded story value. Delete `StoryGeneratorOutput::json_schema`; the internal `StoryGeneratorOutput` wrapper may remain only as a non-LLM Turn value if `TurnExecutionContext` still benefits from it.

### 3.9 Cast Policy and Dynamic Role Contract

Add Story-local dynamic Role allocation:

```rust
pub struct RoleIdHighWater(u64);

pub struct DynamicRoleCandidatePool {
    pub candidates: Vec<RoleId>,
    pub base_high_water: RoleIdHighWater,
}

pub fn allocate_dynamic_role_candidates(
    base: RoleIdHighWater,
    maximum: usize,
) -> Result<DynamicRoleCandidatePool, RoleIdAllocationError>;
```

Dynamic Role IDs use the canonical grammar `role_<sequence>` with the same non-zero, four-digit-minimum formatting rule as the predecessor Knowledge IDs:

```text
role_0001
role_0002
role_10000
```

StoryPack validation rejects authored Role IDs matching the exact dynamic Role grammar. `story_instances.role_id_high_water` starts at zero, never decreases, and advances only through accepted dynamic Roles in a successful commit.

Snapshot contracts carry the same validated base value:

```rust
pub struct StoryReadSnapshotParts {
    pub role_id_high_water: RoleIdHighWater,
}

pub struct StoryReadSnapshot {
    role_id_high_water: RoleIdHighWater,
}
```

These are field additions to the same Snapshot structs from §3.6; the final structs contain both `story_title` and `role_id_high_water`, not alternate definitions.

Add `state_extractor.max_new_roles_per_turn` with default `4`, positive validation, and a hard upper bound of `16`. When `cast_policy = open`, the extractor projector renders exactly that many sequential candidates from the Snapshot high-water under `Available New Role IDs`. The extractor must use a prefix of the candidates in rendered order; skipping `role_0001` and returning `role_0002` is a re-extraction issue. When the policy is not open, the section is omitted and the candidate pool is empty.

Render `Available Locations` from the complete bounded union of known `KnowledgeEntity::Location` keys and locations held by existing Roles. New Role and Role-state output must copy an exact rendered location key. The extractor cannot create a Location key.

StoryStateExtractor RC adds:

```markdown
## Instance Settings

cast_policy: open

## Available New Role IDs

- "role_0001"
- "role_0002"

## Available Locations

- "lodge_hall"
```

Apply this exact policy matrix end to end:

| `CastPolicy` | Generator may introduce | Extractor output | Validator/commit |
|---|---|---|---|
| `Open` | Incidental characters and material new Roles | Persist every material new Role in `new_roles`; incidental background is omitted | Accept valid new Roles and atomically commit them |
| `IncidentalOnly` | Anonymous/non-material incidental characters only | `new_roles` must be empty; material introduction goes to `cast_policy_violations` | Any violation requires Story Repair |
| `Closed` | No new character, including an incidental character | `new_roles` must be empty; any introduction goes to `cast_policy_violations` | Any violation requires Story Repair |

A new person is material if any condition is true:

1. The prose names or identifies the person beyond an anonymous background function.
2. The person speaks.
3. The person independently acts on the scene.
4. The prose establishes a relationship involving the person.
5. The person provides consequential Knowledge or is likely to persist into a later Turn.

The stranger in the motivating trace speaks, independently acts, and provides a consequential claim; under `open` it is therefore a new Role. The extractor must assign the next available Role ID and may add the claim as an unverified Rumor whose `source_role_id` is that new Role.

`cast_policy_violations` contains concise evidence/descriptions, not a new ID. Any non-empty value creates `ValidationIssueCode::CastPolicyViolation`, class `Story`, remedy `RepairStory`. Under `open`, it is used only when the prose establishes more material Roles than the candidate pool can represent. Under `IncidentalOnly` or `Closed`, it reports the prohibited introduction. A repaired story is extracted again under the same policy and candidate pool.

New Role validation is exact:

- `role_id` must be the next unused prefix candidate and unique.
- `name` and `narrative_function` must be trim-non-empty and grounded in Story Text.
- `role_label = ""` normalizes to `name`; non-empty labels remain bounded.
- Optional profile/background fields may be empty and must not be fabricated to fill the schema.
- Location must resolve through `Available Locations`.
- New Role IDs join the known-Role set before validating relationships, Rumor sources, Memory owners, and Narrative candidate state.
- The same Role ID cannot appear in both `new_roles` and `role_states`.
- A new relationship may be created only when Story Text establishes it and at least one endpoint is a new Role; existing-existing relationship updates retain predecessor rules.
- New Role additions, Role-state updates, relationships, Knowledge, Narrative resolution, Role/Knowledge high-waters, and Turn commit are one transaction.
- On later Turns, a dynamic Role participates in Baseline/Character Index projection exactly like an authored Story Role; its `narrative_function` is the Character Index `retrieval_hint` when the Role is unloaded.

Extend the final validated change set:

```rust
pub enum ValidatedRelationshipOperation {
    Add(RelationshipState),
    Update(RelationshipStateChange),
}

pub struct ValidatedChangeSetParts {
    pub new_roles: Vec<StoryRole>,
    pub role_changes: Vec<RoleStateChange>,
    pub relationship_operations: Vec<ValidatedRelationshipOperation>,
    pub knowledge_mutations: Vec<ValidatedKnowledgeMutation>,
    pub next_role_id_high_water: RoleIdHighWater,
}
```

All unlisted Snapshot and validated-change-set fields retain the predecessor contracts.

`CandidateNarrativeStateView` must include accepted new Roles and relationships before Narrative condition resolution. Store commit verifies the base Role high-water and inserts new Roles into `roles_json`; a collision or high-water mismatch is a revision conflict and writes nothing.

### 3.10 Prompt Asset and Slot Contract

Update `crates/aise/assets/prompts/context-v2/slots.yaml`:

| Profile | Remove | Add/retain |
|---|---|---|
| WriterPlanner RC | none beyond predecessor | `story_profile`, index variables without scope content |
| WriterPlanner FTI | `output_schema` | no output variable |
| CharacterThink FTI | `output_schema` | no output variable |
| StoryGenerator FTI | `output_schema` | no output variable |
| StoryRepairer FTI | `output_schema` | no output variable |
| StoryStateExtractor RC | none | `instance_settings`, `available_new_role_ids`, `available_locations` |
| StoryStateExtractor FTI | `output_schema` | no output variable |

Structured-output CSI/FTI says “return the required structured output” and names semantic fields/rules, but contains no full JSON Schema, provider mode, tool name, schema hash, `$schema` URI, or transport-specific instruction. The Gateway alone appends the compact trusted shape for `JsonObject`/`PromptFallback`.

WriterPlanner CSI/FTI states:

```text
The Character and Knowledge Indexes are complete discovery metadata.
retrieval_hint describes what retrieving target_id will provide; it is not story evidence.
Copy an exact target_id only when that target is materially needed.
If no suitable target exists, emit no context gap; never invent an ID or query.
```

StoryGenerator and StoryRepairer CSI/FTI define all three cast policies exactly as §3.9. StoryStateExtractor CSI/FTI repeats the same matrix, materiality test, candidate-ID rule, concise-hint rule, and evidence-only extraction boundary. There must be no instruction that simultaneously allows Generator creation and forbids Extractor Role creation under `open`.

Generator identity wording is exact: bind every already-known actor by the supplied `role_id`; under `open`, a genuinely new actor has no Role ID during prose generation, so Generator must not invent or print one. StateExtractor assigns the next available ID after the prose establishes materiality. WriterPlanner and CharacterThink may request private thinking only for existing AI Roles, never for a not-yet-created actor.

### 3.11 File / Directory Layout

```text
crates/aise/
├── assets/
│   ├── persistence/mig/0021_turn_runtime_contract_alignment.sql
│   └── prompts/context-v2/
│       ├── slots.yaml
│       ├── csi/{writer-planner,story-generator,story-state-extractor}.md.j2
│       └── fti/{writer-planner,character-think,story-generator,story-repairer,story-state-extractor}.md.j2
├── src/
│   ├── config/{llm,state_extractor}.rs
│   ├── domain/
│   │   ├── ids.rs
│   │   ├── narrative.rs
│   │   ├── narrative_graph/state.rs
│   │   ├── knowledge/query.rs
│   │   ├── story_instance/{info,role,snapshot,state}.rs
│   │   └── turn/{baseline,planning,extraction,story_generation}.rs
│   ├── llm/{message,provider,gateway,openai_compat,output_contract}.rs
│   ├── planning/{planner_output,retrieval_plan_builder,writer_planner,writer_planner_prompt}.rs
│   ├── character/{character_think_pipeline,character_think_prompt}.rs
│   ├── story/{story_generator,story_generator_prompt,story_repairer,story_state_extractor,story_state_extractor_prompt}.rs
│   ├── validation/{validation_pipeline,validators}.rs
│   ├── persistence/{store,sqlite_store,sqlite_snapshot,sqlite_story_history_reader,turn_committer}.rs
│   ├── runtime/turn_runtime.rs
│   ├── turn/{turn_context,turn_contract,turn_event,turn_trace}.rs
│   └── engine.rs
└── tests/
    ├── llm_structured_output_tests.rs
    ├── turn_number_migration_tests.rs
    └── prompt_context_contract_tests.rs
```

New unit tests live in dedicated `tests/<source>_tests.rs` modules. Keep `mod.rs` and `lib.rs` index-only.

---

## 4. Behavior Rules

1. **TRA-1 — Composite Turn identity**: A committed Turn is addressable only by `(story_id, turn_number)`; no UUID, concatenated surrogate Turn ID, or DB-global Turn primary key may coexist.
2. **TRA-2 — Commit-only numbering**: Only successful commit advances `last_turn_number`; failed/cancelled/conflicting attempts consume no number.
3. **TRA-3 — Retry stability**: Repair and re-extraction keep one candidate number; idempotent replay returns the originally committed number.
4. **TRA-4 — Counter separation**: Code must not derive Turn number from revision or segment sequence, even when sample values happen to match.
5. **TRA-5 — Trace separation**: Multiple attempts may share a candidate Turn number; `TraceId` is the unique attempt identity.
6. **TRA-6 — Complete indexes**: Every rendered Character/Knowledge Index is complete within the configured global bound and contains no scope marker.
7. **TRA-7 — Exact target only**: Planner gaps resolve only by exact current Index target; unknown target is invalid output and absence of a suitable target means no gap.
8. **TRA-8 — No query retrieval**: No model DTO, Domain retrieval request, config field, trace field, or provider query contains `query_text`.
9. **TRA-9 — Hint boundary**: `retrieval_hint` is visible to WriterPlanner only while content is unloaded; loaded Generator/Character content never displays its hint.
10. **TRA-10 — Static hint normalization**: Missing short static hint becomes exact content before canonicalization; missing long static hint fails import.
11. **TRA-11 — Dynamic hint requirement**: Dynamic Fact/Rumor add/update always supplies a bounded hint; update changes content and hint atomically.
12. **TRA-12 — Title source**: Runtime Story Profile title comes only from `StoryPack.meta.title`; no duplicate asset-authority field is introduced.
13. **TRA-13 — Title visibility**: WriterPlanner and Generator/Repairer see title; CharacterThink and StateExtractor never do.
14. **TRA-14 — Provider neutrality**: Pipelines select text versus structured contracts only and contain no provider/model switch.
15. **TRA-15 — One negotiated mode**: Gateway resolves exactly one structured-output mode before one provider call and never silently downgrades after failure.
16. **TRA-16 — Local authority**: Typed deserialization and local semantic validation run for every structured response, including strict native responses.
17. **TRA-17 — Schema out of Prompt**: Native JSON Schema and strict-tool modes inject zero schema/shape bytes into CSI/RC/FTI messages.
18. **TRA-18 — Compact fallback**: JSON-object and Prompt-fallback modes append only the bounded compact contract message, not the full JSON Schema.
19. **TRA-19 — DTO separation**: LLM DTOs contain only semantic outputs and exact references; Domain/DB entities are constructed after validation.
20. **TRA-20 — Prose is text**: StoryGenerator and StoryRepairer return bounded prose directly and do not pay for or parse a one-field JSON wrapper.
21. **TRA-21 — Cast consistency**: The same `CastPolicy` semantics apply in Planner guidance, Generator, Extractor, validation, repair, and commit.
22. **TRA-22 — Open creation**: Under `open`, every material new person is emitted as a new Role using an exact available candidate ID.
23. **TRA-23 — Incidental boundary**: Under `incidental_only`, anonymous background characters may remain prose-only; any material new person requires Story Repair.
24. **TRA-24 — Closed boundary**: Under `closed`, any newly introduced character requires Story Repair and cannot be silently ignored by extraction.
25. **TRA-25 — Evidence only**: Extractor must not invent new Role traits, background, relationships, or Knowledge absent from Story Text; empty optional fields are valid.
26. **TRA-26 — New Role reference set**: Accepted new Role IDs become valid relationship, Rumor-source, Memory-owner, and Narrative references within the same candidate.
27. **TRA-27 — Atomic creation**: No new Role, relationship, Knowledge entry, high-water, Narrative transition, Turn row, or outbox record is visible unless the entire Turn commits.
28. **TRA-28 — Boundedness**: Indexes, candidate Role IDs, DTO arrays, Prompt fallback shapes, schemas, and enrichment outputs retain explicit hard limits.
29. **TRA-29 — No added call**: Hint normalization, output validation, metadata enrichment, Role allocation, and provider normalization add no LLM call.
30. **TRA-30 — Hard deletion**: Legacy `TurnId`, scope, query, full-schema Prompt slots, rich extractor unions, and contradictory cast instructions are removed in the same implementation.

### 4.1 Error Handling

- `TurnNumber::try_new(0)`, values above SQLite's signed range, and increment overflow return typed `TurnNumberError`; production code never uses `unwrap()` for external/stored values.
- A commit whose candidate is not stored high-water plus one returns `StoreError::RevisionConflict` and writes nothing.
- The migration guard aborts with named constraint `turn_runtime_alignment_legacy_turn_data` before any schema mutation.
- An oversized complete index returns `context_index_limit_exceeded`; it is never truncated or silently prefiltered.
- Unknown Planner target returns `PlanningError::UnknownRetrievalKey`; a missing gap is valid.
- Static long content without a configured hint returns `AssetValidationCode::RetrievalHintRequired` at the exact JSON pointer.
- Malformed structured output, wrong strict tool, invalid JSON, unknown field, local limit failure, or semantic contract failure returns an `LlmError` carrying contract name and non-content reason.
- WriterPlanner/CharacterThink structured-output errors retain their existing Turn failure class. StateExtractor structural/semantic errors produce `ExtractionSchemaInvalid` and `ReextractState` within the existing correction budget.
- A cast-policy violation produces `ValidationIssueCode::CastPolicyViolation` with `RepairStory`, not `ReextractState`.
- An invalid candidate Role ID, unknown location, duplicate Role, or bad new-Role reference produces a typed extraction issue with `ReextractState`.
- Provider capability misconfiguration fails at Gateway construction or with `StructuredOutputUnsupported`; no provider call is made.
- Plain-text Story output that is empty or oversized returns `model_output_invalid` and enters the existing Story failure/repair behavior; it is not truncated.

### 4.2 Concurrency

- Candidate Turn and Role numbers are derived only after acquiring the existing per-Story coordinator permit.
- Cross-process correctness relies on the Store transaction's Story revision and high-water predicates, not on the in-memory permit alone.
- Failed optimistic commit advances neither `last_turn_number`, `role_id_high_water`, nor `knowledge_id_high_water`.
- Structured-output mode resolution and contract hashing are immutable/read-only and require no hot-path write lock.
- Provider calls continue through the existing shared limiter and reservation accounting. Adding multiple provider transports must not create one limiter per mode.
- No write guard crosses `.await`; no I/O or channel send occurs while a write guard is held.
- There is no speculative Role-ID reservation transaction. Candidate IDs are Snapshot-derived and become consumed only at atomic commit.

### 4.3 Observability

- Every Turn/pipeline/LLM/persist span records `story_id`, candidate `turn_number` when available, and `trace_id`; no span records a Turn UUID.
- Every structured LLM span records `output_contract`, `schema_hash`, `structured_output_mode`, `schema_bytes`, `prompt_contract_bytes`, `decode_status`, and `validation_status`.
- `prompt_contract_bytes = 0` for native JSON Schema and forced strict tool.
- Provider/model/mode are structured fields. Do not derive mode by parsing provider name, base URL, or model name in log code.
- Log one warning on structured-output failure with bounded reason and contract metadata; default logs do not include schema, DTO body, story text, Knowledge content, retrieval hint, or tool arguments.
- Extractor projection logs `cast_policy`, available candidate count, available location count, and accepted new-Role count without Role profile content.
- Commit trace records Turn number, new Role count, Knowledge mutations by operation/kind, base/new Role and Knowledge high-waters, and success/error code.
- Existing full-content development trace policy may record final Prompt/response content, but it emits only the selected final path and never a duplicate legacy schema Prompt.

---

## 5. Acceptance Criteria

### 5.1 Turn Identity and Persistence

- [ ] `TurnNumber` and `TurnKey` match §3.2 and reject zero/SQLite overflow — `turn_number_validates_canonical_range` passes.
- [ ] First successful Turn in a Story is number `1` even when Story opening has segment sequence `1` — `turn_number_is_independent_from_story_sequence` passes.
- [ ] Failed, cancelled, and revision-conflicting attempts do not advance the number; the next success reuses the candidate — `failed_turn_does_not_consume_number` passes.
- [ ] Same idempotency key/digest returns the stored number and performs no LLM/commit call; a digest mismatch preserves the existing conflict — `idempotent_replay_preserves_turn_number` passes.
- [ ] Story revision and Turn number are separately asserted in tests and never converted into one another — `turn_number_is_not_story_revision` passes.
- [ ] Turn rows use composite PK `(story_id, turn_number)` and every dependent table uses matching columns/FKs — migration schema assertions pass.
- [ ] Migration `0021` succeeds on the predecessor fresh schema and fails before mutation when any legacy Turn-bearing row exists — `turn_number_migration_tests` pass.
- [ ] Production source contains no `TurnId`, `turn_id`, `new_turn_id`, or `UuidIdGenerator` — `rg -n '\bTurnId\b|\bturn_id\b|new_turn_id|UuidIdGenerator' crates/aise/src crates/aise/tests` returns zero matches.
- [ ] Migration `0021` final table definitions and all runtime SQL queries contain no `world_id` or `turn_id`; the only `turn_id` text in `0021` is inside the named legacy JSON guard paths — migration contract test plus targeted runtime-source `rg` pass.
- [ ] `TraceId`, `LlmCallId`, and span IDs remain unique and UUID-capable — existing trace/LLM tests pass.

### 5.2 Retrieval, Hint, and Title

- [ ] `RetrievalIndexScope` and both Baseline scope fields are deleted — targeted `rg` returns zero matches.
- [ ] WriterPlanner RC contains no `scope:`, `entries:`, or empty Index section — `complete_indexes_render_without_scope_or_empty_shells` passes.
- [ ] Index limit overflow fails before LLM invocation and does not prefilter/truncate — `complete_index_rejects_capacity_overflow` passes.
- [ ] Planner DTO uses separate Writer/Character exact-target arrays and contains no nullable selector union — exact contract snapshot passes.
- [ ] No production/test/config source contains `query_text`, `max_query_bytes`, or `retrieval_selector_exclusivity` — targeted `rg` returns zero matches.
- [ ] Unknown or invented target fails; no suitable target with no gap succeeds — `planner_uses_exact_index_target_or_no_gap` passes.
- [ ] Missing static hint copies exact content when content is at most 256 UTF-8 bytes before digest/persistence — `short_static_content_defaults_retrieval_hint` passes.
- [ ] Missing static hint for 257+ bytes fails at exact asset path; an explicit bounded hint succeeds — `long_static_content_requires_retrieval_hint` passes.
- [ ] Dynamic Fact/Rumor add/update requires content plus hint; Memory and delete outputs contain no hint — structured DTO and validator tests pass.
- [ ] Loaded Relevant Knowledge and Role Knowledge never render hints — `retrieval_hint_is_planner_index_only` passes.
- [ ] Story Profile renders canonical title first for WriterPlanner and Generator/Repairer, with no title visible to CharacterThink or Extractor — `story_title_visibility_is_stage_bounded` passes.
- [ ] Asset `StoryProfile` has no duplicate title; Snapshot derives it from `StoryPack.meta.title` — type and SQLite projection tests pass.

### 5.3 Provider-Neutral Structured Output

- [ ] `LlmOutputContract`, four `StructuredOutputMode` variants, exact capability intersection, and fixed preference order match §3.7 — unit tests pass.
- [ ] Pipelines call only `complete_text_composed` or `complete_structured_composed` and contain no provider/model branching — targeted `rg` plus review passes.
- [ ] Native JSON Schema request body, forced strict-tool body/choice, JSON-object body, and Prompt-fallback body match §3.7 — `llm_structured_output_tests` golden cases pass.
- [ ] Strict-tool response normalization rejects zero/multiple/wrong tool calls and competing prose — protocol tests pass.
- [ ] Every provider mode produces the same typed DTO and local validator invocation — `all_structured_modes_share_decode_and_validation` passes.
- [ ] Provider rejection results in exactly one provider call and no weaker-mode retry — `structured_output_does_not_runtime_downgrade` passes.
- [ ] Native/strict modes inject zero contract bytes into messages; JSON-object/Prompt fallback inject at most the compact bound — `structured_output_prompt_injection_is_mode_bounded` passes.
- [ ] LLM trace records contract name/hash/mode/byte counts and does not log schema by default — trace contract tests pass.
- [ ] Every LLM call still passes through one shared limiter and token reservation path — existing limiter tests plus `structured_modes_share_limiter` pass.
- [ ] `output_schema` is absent from Prompt slots, projectors, and prompt assets — `rg -n 'output_schema' crates/aise/src crates/aise/assets/prompts crates/aise/tests` returns zero matches.

### 5.4 Slim DTOs and Token Attention

- [ ] WriterPlanner, CharacterThink, and StateExtractor use dedicated DTOs rather than Domain/DB serialization — type dependency tests pass.
- [ ] Extractor DTO matches §3.8 and has no knowledge-operation `oneOf`, proposition, claim, entities, topics, salience, or storage bounds — exact schema snapshot passes.
- [ ] `story_state_extraction.v2` canonical schema is `<= 6144` bytes and compact fallback shape is `<= 1536` bytes — `state_extractor_contract_stays_compact` passes.
- [ ] Native structured-output Extractor FTI contains no schema body; the motivating 9,120-byte schema pattern cannot recur — prompt composition test passes.
- [ ] Application enrichment produces exact default/preserved salience, recomputed topics, minimal entities, and absent proposition/claim from §3.8 — `extracted_knowledge_enrichment_is_deterministic` passes.
- [ ] Empty optional New Role fields and suggested utterance normalize to `None`; invented placeholders are not required — DTO normalization tests pass.
- [ ] StoryGenerator/Repairer return bounded plain text, have no JSON schema, and do not parse `{"story_text": ...}` — `story_prose_uses_text_completion` passes.
- [ ] Full workspace source contains no LLM schema `maxLength: 131072` or repeated rich extractor mutation schema — targeted `rg` and schema test pass.

### 5.5 Cast Policy and Atomic Role Creation

- [ ] Generator and Extractor prompt assets encode byte-consistent `open`, `incidental_only`, and `closed` semantics — `cast_policy_prompt_semantics_are_aligned` passes.
- [ ] Under `open`, the motivating speaking stranger becomes `role_0001`, is persisted as AI/no Character Card, and may source an unverified Rumor — `open_cast_persists_material_stranger_and_claim` passes.
- [ ] Under `incidental_only`, an anonymous crowd is not persisted, while a speaking stranger yields `CastPolicyViolation` and Story Repair — both policy tests pass.
- [ ] Under `closed`, any new character evidence yields Story Repair rather than silent extraction omission — `closed_cast_repairs_new_character` passes.
- [ ] Available dynamic Role candidates are exact, bounded, prefix-consumed, and omitted outside `open` — allocator/projector tests pass.
- [ ] Authored `role_0001`-shape IDs fail Pack validation; semantic authored Role IDs remain valid — `dynamic_role_id_namespace_is_reserved` passes.
- [ ] New Role may be referenced by same-output relationship, Rumor, Memory, and Narrative candidate state; unknown IDs still fail re-extraction — reference tests pass.
- [ ] New Role optional profile fields remain empty when prose does not establish them — `new_role_extraction_does_not_invent_profile` passes.
- [ ] Failed validation/commit changes neither roles nor Role/Knowledge/Turn high-waters — `dynamic_role_creation_is_atomic` passes.
- [ ] Successful commit writes Role, relationship, Knowledge, Narrative state, high-waters, Turn, segment, events, and outbox atomically — Store integration test passes.

### 5.6 Quality Gates

- [ ] All superseded fields/types/functions/assets/tests are deleted in the same change; no compatibility aliases or dual paths remain — targeted `rg` checks above pass.
- [ ] Every new test lives in a dedicated `tests/<source>_tests.rs`; source files contain no inline test bodies or comments.
- [ ] `cargo fmt --all --check` passes.
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo test --workspace` passes.

---

## 6. Out of Scope / Future Work

- A separate offline migration utility may map historical Turn UUID databases to numbers; it must not become a runtime compatibility reader or dual schema.
- Retrieval-hint quality metrics and authoring lint may be added after trace evaluation; they must not add a Turn-time LLM call.
- Additional native provider adapters may implement the same capability/normalization contracts; pipelines and DTOs must remain unchanged.
- A future Role-local Memory index may define its own hint contract while preserving owner isolation; Memory remains absent from the global Knowledge Index here.

No implementation decision in this spec is TBD.

---

## 7. References

- Source design: [Narrative、Knowledge 与 Retrieval Context 收敛](../design/2026-08-17-narrative-knowledge-retrieval-design-gpt.md)
- Required predecessor: [Narrative, Knowledge, and Retrieval Context Reconciliation](2026-08-17-narrative-knowledge-retrieval-spec-gpt.md)
- Turn architecture: [AISE Architecture](../design/2026-08-04-Architecture-gpt.md)
- Retrieval baseline: [Context Preparation and Retrieval](../design/2026-08-08-context-preparation-retrieval-design-gpt.md)
- Extractor split baseline: [StoryGenerator and StoryStateExtractor Split](CSI-RC-FTI/2026-08-14-story-state-extractor-split-spec-gpt.md)
- Character/Role identity baseline: [Character Card and Story Role Profile](../design/CSI-RC-FTI/2026-08-14-character-role-profile-design-gpt.md)
- Prompt framework: [CSI-RC-FTI Prompt Framework](CSI-RC-FTI/2026-08-11-csi-rc-fti-prompt-spec-gpt.md)
- Context predecessors: [Current Scene Removal](2026-08-17-current-scene-removal-spec-gpt.md), [Story Context Simplification](2026-08-17-story-context-simplification-spec-gpt.md), [Runtime Context Empty Elision](2026-08-17-runtime-context-empty-elision-spec-gpt.md)
- Guardrails: [`doc/agents/guardrails/`](../agents/guardrails/)
