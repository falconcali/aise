# Character Role Profile Assets — Phase 0 Spec

> **Model**: GPT-5
> **Date**: 2026-08-14
> **Status**: Proposed
> **Source Design**: [Character Card 与 Story Role Profile](../../design/CSI-RC-FTI/2026-08-14-character-role-profile-design-gpt.md)
> **Phase**: Phase 0 of 3 — asset and identity contracts

---

## 1. Goal

Replace the Character Card and Story Pack role asset contracts with one reusable `CharacterProfile`, a global UUID `CharacterId`, a story-local `RoleId`, and a required Role default profile without retaining the v3 Character/Role/Binding asset path.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Define the final `CharacterId`, `RoleId`, `CharacterProfile`, `CharacterCard`, `StoryRoleDefinition`, and related Story Pack v4 contracts.
- Move display name into `CharacterProfile`; replace `description` with `appearance`; merge personality, values, and fears into one optional `personality` text; merge `SpeakingStyle` into one optional `speaking_style` text.
- Require every Story Role Definition to contain a valid `default_profile`; add Role-owned optional `background`.
- Remove Story Pack `character_assets` and `default_cast`; a Story Pack is independently instantiable from Role default profiles.
- Add immutable Character Card import, exact-version lookup, listing, validation, and persistence keyed by global `CharacterId`.
- Bump Character Card and Story Pack wire contracts to v4 and reject v3 shapes without aliases or conversion parsers.
- Add field-specific profile, background, example, and collection limits and one shared profile validator.
- Rebuild asset persistence through migration `0015_character_role_profile_assets.sql` after the StoryStateExtractor migration `0014`.
- Update Story Pack fixtures, import/export tests, Character Card tests, configuration, and active documentation required by these contracts.

### 2.2 Non-Goals

- Does not implement the Story Instance `StoryRole` aggregate, Snapshot convergence, RoleId runtime migration, or runtime API changes; Phase 1 owns those changes.
- Does not change Runtime Context rendering or any CSI/RC/FTI wording; Phase 2 owns Prompt projection.
- Does not implement field-level profile merge, default-profile fallback for a selected Card, partial Character Cards, or per-field provenance.
- Does not let Character Cards carry Story background, goals, location, relationships, memories, knowledge, Controller, Narrative state, Prompt instructions, model settings, tools, or runtime configuration.
- Does not dynamically generate a missing profile during Story Instance creation or a Turn.
- Does not make an existing Story Instance follow later Character Card edits.
- Does not preserve imported v3 Pack or Story Instance rows. The migration fails explicitly when legacy Pack/Instance data exists; operators must export content before the change and re-import authored v4 assets into a new database.
- Does not add Character Card deletion, mutable in-place updates, or UI authoring workflows.

### 2.3 Implementation Constraints

- This three-file suite is one atomic hard refactor. Phase numbers define implementation order, not mergeable compatibility stages.
- Do **not** retain `CharacterAssetKey`, `StoryRoleKey`, `SpeakingStyle`, `DefaultCast`, v3 serde aliases, fallback parsers, adapters, dual schemas, or dual writes (`R-REFACTOR-01`, `R-REFACTOR-02`).
- Do **not** merge or release the repository until Phase 0, Phase 1, and Phase 2 acceptance criteria all pass.
- `domain` remains self-contained and must not import `config`, persistence, runtime, API, or Pipeline modules (`R-LAYER-04`).
- Configuration owns adjustable limits; Domain value objects enforce only intrinsic syntax and hard identity bounds (`R-CODE-06`).
- All imported JSON is untrusted data. It must not select Prompt assets, System messages, models, tools, budgets, or instruction authority.
- Keep `mod.rs` and `lib.rs` index-only, put unit tests in dedicated `tests/<source>_tests.rs` files, add no ordinary code comments, and keep imports contiguous (`R-CODE-01`, `R-CODE-02`, `R-CODE-05`, `R-CODE-07`).
- Migration `0014_story_state_extractor_split.sql` from the related extractor spec must exist and run before migration `0015`.

### 2.4 Required Suite Order

1. Implement the Character Decision and StoryStateExtractor split specs, including migration `0014`.
2. Implement this Phase 0 asset and identity contract.
3. Implement Phase 1 runtime convergence and migration `0016`.
4. Implement Phase 2 Prompt projection convergence.
5. Run suite-wide zero-match, migration, format, lint, and test checks before merge.

---

## 3. Contracts

### 3.1 Identity Types

Place both types in `crates/aise/src/domain/ids.rs` and remove `CharacterAssetKey` and `StoryRoleKey` from `domain/asset/ids.rs`.

```rust
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CharacterId(Arc<str>);

impl CharacterId {
    pub fn new_uuid() -> Self;
    pub fn try_new(value: impl Into<String>) -> Result<Self, DomainInputError>;
    pub fn as_str(&self) -> &str;
}

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RoleId(Arc<str>);

impl RoleId {
    pub const MAX_BYTES: usize = 128;

    pub fn try_new(value: impl Into<String>) -> Result<Self, DomainInputError>;
    pub fn as_str(&self) -> &str;
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum DomainInputError {
    #[error("character_id must be a canonical UUID")]
    InvalidCharacterId,
    #[error("role_id must match [a-z0-9]+(?:[._-][a-z0-9]+)* and contain at most 128 bytes")]
    InvalidRoleId,
}
```

All existing non-character `DomainInputError` variants remain unchanged.

Identity rules:

- `CharacterId::try_new` accepts a UUID string only, normalizes it to lowercase hyphenated form, and rejects nil UUIDs.
- `CharacterId::new_uuid` uses UUID v4. `CharacterId` serializes as its canonical string.
- `RoleId::try_new` accepts non-empty lowercase ASCII semantic keys matching `[a-z0-9]+(?:[._-][a-z0-9]+)*` and at most `RoleId::MAX_BYTES` UTF-8 bytes.
- Neither type implements unchecked `From<&str>` or `From<String>`.
- Name is never accepted by an ID constructor and no name-based lookup API is added.
- `(StoryId, RoleId)` is the only cross-Story-Instance address for a runtime Role. Phase 0 defines `RoleId`; Phase 1 removes the instance-level use of `CharacterId`.

### 3.2 Version Discriminators

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CharacterSpec {
    #[serde(rename = "aise_char_v4")]
    V4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StorySpec {
    #[serde(rename = "aise_story_v4")]
    V4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetSpecVersion {
    #[serde(rename = "3.0")]
    V3_0,
    #[serde(rename = "4.0")]
    V4_0,
}
```

- Character Card accepts only `(aise_char_v4, 4.0)`.
- Story Pack accepts only `(aise_story_v4, 4.0)`.
- World Book remains `(aise_world_v3, 3.0)` in this change.
- Crossed pairs such as `(aise_story_v4, 3.0)` fail with `unsupported_spec_version` before typed import.

### 3.3 Shared Character Profile

Replace the contents of `crates/aise/src/domain/asset/character_card.rs` with the final contracts below.

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterProfile {
    pub name: BoundedText,
    pub appearance: Option<BoundedText>,
    pub personality: Option<BoundedText>,
    pub speaking_style: Option<BoundedText>,
    #[serde(default)]
    pub dialogue_examples: Vec<DialogueExample>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DialogueExample {
    pub situation: BoundedText,
    pub response: BoundedText,
}
```

Canonical JSON shape:

```json
{
  "name": "The Traveler",
  "appearance": "A mud-stained dark travel coat and a dried blood mark on the left sleeve.",
  "personality": "Cautious and curious; values truth and personal safety.",
  "speaking_style": "Concise, probing, and reluctant to reveal conclusions.",
  "dialogue_examples": [
    {
      "situation": "Asked whether the forest is safe",
      "response": "Safe compared with what?"
    }
  ]
}
```

Profile rules:

- `name` is required and non-empty after trimming.
- `appearance`, `personality`, and `speaking_style` are optional. If present, each is non-empty after trimming. Empty strings are rejected instead of normalized to `None`.
- `dialogue_examples` may be absent and then deserializes to an empty list. Every present example has non-empty `situation` and `response`.
- `CharacterProfile` contains no `background`, story history, state, knowledge, Controller, or Prompt field.
- `description`, list-valued `personality`, `values`, `fears`, object-valued `speaking_style`, `register`, `verbosity`, and `traits` are unknown fields and fail typed deserialization.
- A Role default profile and a Card profile use this exact Rust type and the exact same validation function. No second Role-profile DTO or validator exists.

### 3.4 Character Card v4

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterCard {
    pub spec: CharacterSpec,
    pub spec_version: AssetSpecVersion,
    pub character_id: CharacterId,
    pub meta: CharacterMeta,
    pub profile: CharacterProfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterMeta {
    pub creator: Option<BoundedText>,
    pub version: SemanticVersion,
    #[serde(default)]
    pub tags: Vec<BoundedText>,
}
```

- The display name exists only at `profile.name`; `meta.name` is deleted.
- One logical reusable character keeps the same `CharacterId` across Card versions.
- A new version is immutable. Importing the same `(character_id, version)` with a different digest returns `StoreError::ConstraintViolation { constraint: "character_version_digest_conflict" }`.
- `CharacterId`, Card version, and digest are catalog/source metadata and are not part of the Profile value.

### 3.5 Story Pack v4 and Role Definition

Replace the Role-related contracts in `crates/aise/src/domain/asset/story_pack.rs` with:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryPack {
    pub spec: StorySpec,
    pub spec_version: AssetSpecVersion,
    pub meta: StoryPackMeta,
    pub story: StoryProfile,
    pub roles: BTreeMap<RoleId, StoryRoleDefinition>,
    pub play: PlayDefinition,
    pub world_book: WorldBookSource,
    pub start: StoryStart,
    pub narrative: NarrativeGraphDefinition,
    #[serde(default)]
    pub constraints: BTreeMap<ConstraintKey, StoryConstraintDefinition>,
    #[serde(default)]
    pub assets: BTreeMap<AssetId, StaticAssetDescriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryRoleDefinition {
    pub role_label: BoundedText,
    pub narrative_function: BoundedText,
    pub default_profile: CharacterProfile,
    pub background: Option<BoundedText>,
    pub initial_state: InitialRoleState,
    #[serde(default)]
    pub initial_relationships: Vec<RelationshipSeed>,
    #[serde(default)]
    pub seed_memories: Vec<MemorySeed>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipSeed {
    pub target_role_id: RoleId,
    pub kind: RelationshipKind,
    pub trust: i16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayDefinition {
    pub player_count: u16,
    pub playable_role_ids: Vec<RoleId>,
}
```

Contract rules:

- The `roles` map key is the authoritative `RoleId`; `StoryRoleDefinition` does not duplicate it as a field.
- `default_profile` is required for every Role and must pass the shared Profile validator even if a Card may later replace it.
- `background` is Story-owned, optional, and bounded. It never appears in `CharacterProfile` or `CharacterCard`.
- `character_assets`, `default_cast`, `playable_role_keys`, `target_role_key`, and the old `StoryRole` type are deleted.
- A Story Pack with zero Character Cards remains valid and fully instantiable.
- `RoleId` replaces `StoryRoleKey` in Narrative definition DTOs during this suite. Phase 1 replaces all runtime evaluation and lookup paths.

### 3.6 Frozen Character Card Reference

Replace `FrozenCharacterAssetRef` with:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenCharacterCardRef {
    pub character_id: CharacterId,
    pub version: SemanticVersion,
    pub digest: Sha256Digest,
}
```

- Delete `CharacterAssetSource`, `FrozenCharacterAssetRef`, and `DefaultCast`.
- `FrozenCharacterCardRef` identifies one immutable stored Card revision.
- Loading requires all three values to match one row; no latest-version fallback is permitted.
- Runtime Prompt projections must never serialize this type; Phase 2 enforces that boundary.

### 3.7 Asset Limits

Replace `max_character_assets` and add the following fields to `AssetLimitsConfig` and `config/aise_config.toml`:

```rust
pub struct AssetLimitsConfig {
    pub max_key_bytes: usize,
    pub max_text_bytes: usize,
    pub max_tags_per_item: usize,
    pub max_profile_name_bytes: usize,
    pub max_profile_appearance_bytes: usize,
    pub max_profile_personality_bytes: usize,
    pub max_profile_speaking_style_bytes: usize,
    pub max_profile_total_bytes: usize,
    pub max_dialogue_examples_per_profile: usize,
    pub max_dialogue_situation_bytes: usize,
    pub max_dialogue_response_bytes: usize,
    pub max_role_background_bytes: usize,
}
```

All unrelated asset-limit fields remain unchanged.

Required defaults:

| Key | Default |
|---|---:|
| `max_profile_name_bytes` | 256 |
| `max_profile_appearance_bytes` | 2,048 |
| `max_profile_personality_bytes` | 4,096 |
| `max_profile_speaking_style_bytes` | 2,048 |
| `max_profile_total_bytes` | 65,536 |
| `max_dialogue_examples_per_profile` | 16 |
| `max_dialogue_situation_bytes` | 1,024 |
| `max_dialogue_response_bytes` | 2,048 |
| `max_role_background_bytes` | 16,384 |

Every field must be positive. `max_key_bytes` must be at most `RoleId::MAX_BYTES`. The compact JSON serialization of each validated `CharacterProfile` must be at most `max_profile_total_bytes`. No serde aliases for `max_character_assets` or any old profile limit are permitted.

### 3.8 Validation API

Add one shared validator in `story/pack_service.rs` or a dedicated sibling file owned by the Story asset service:

```rust
fn validate_character_profile(
    profile: &CharacterProfile,
    path: &str,
    limits: &AssetLimitsConfig,
    report: &mut ValidationReport,
);
```

It must be called for:

- `CharacterCard.profile` at `/profile`;
- every `StoryRoleDefinition.default_profile` at `/roles/{role_id}/default_profile`.

Add or retain these validation codes:

```rust
pub enum AssetValidationCode {
    SchemaInvalid,
    UnsupportedSpec,
    UnsupportedSpecVersion,
    InvalidKey,
    InvalidVersion,
    UnknownField,
    ForbiddenField,
    MissingReference,
    DuplicateKey,
    MissingStoryOpening,
    InvalidSalience,
    LimitExceeded,
    EmptyText,
}
```

All unrelated validation codes remain unchanged.

Remove `MissingDefaultCast` and `CharacterIdentityFieldInRole` when no active caller remains.

Validation rules:

- `validate_pack_value` accepts only Story Pack v4 and does not look for `character_assets` or `default_cast`.
- It validates every Role key, Role default profile, optional background, playable Role reference, relationship target, Narrative Role reference, and Role count before typed import.
- Character Card validation accepts only Character Card v4 and validates ID, metadata, tags, and Profile bounds.
- After field/count checks, the shared Profile validator serializes the typed Profile with `serde_json::to_vec` and applies `max_profile_total_bytes`; serialization failure is `schema_invalid`, and overflow is `limit_exceeded` at the Profile path.
- The recursive forbidden-field check remains and continues to reject Prompt/model/tool/runtime authority fields in both Card and Pack data.
- Validation accumulates at most `max_validation_issues`; after the limit, no additional issue is appended.

### 3.9 Character Card Persistence Port

Extend the existing asset persistence boundary without exposing SQLite types:

```rust
#[derive(Debug, Clone)]
pub struct ValidatedCharacterCard {
    pub card: CharacterCard,
    pub canonical_json: Vec<u8>,
    pub digest: Sha256Digest,
}

#[derive(Debug, Clone)]
pub struct FrozenCharacterCard {
    pub card: CharacterCard,
    pub digest: Sha256Digest,
}

impl FrozenCharacterCard {
    pub fn frozen_ref(&self) -> FrozenCharacterCardRef;
}

#[derive(Debug, Clone)]
pub struct CharacterCardInfo {
    pub character_id: CharacterId,
    pub name: BoundedText,
    pub creator: Option<BoundedText>,
    pub version: SemanticVersion,
    pub digest: Sha256Digest,
}

#[async_trait]
pub trait AssetStore: Send + Sync {
    async fn find_character_by_digest(
        &self,
        digest: &Sha256Digest,
    ) -> Result<Option<FrozenCharacterCard>, StoreError>;

    async fn import_character(
        &self,
        value: ValidatedCharacterCard,
    ) -> Result<FrozenCharacterCard, StoreError>;

    async fn load_character(
        &self,
        reference: &FrozenCharacterCardRef,
    ) -> Result<FrozenCharacterCard, StoreError>;

    async fn list_characters(&self) -> Result<Vec<CharacterCardInfo>, StoreError>;
}
```

The existing Story Pack methods remain on `AssetStore` with the v4 Pack types.

`ValidatedStoryPack` and `FrozenStoryPack` must remove `resolved_characters`. Story Pack persistence stores no Character Card JSON.

### 3.10 Character Card Service and HTTP Contract

Add `crates/aise/src/story/character_card_service.rs` and `crates/aise-server/src/api/character_card.rs`.

```rust
pub struct CharacterCardService {
    asset_store: Arc<dyn AssetStore>,
    limits: AssetLimitsConfig,
}

impl CharacterCardService {
    pub fn validate(&self, bytes: &[u8]) -> ValidationReport;
    pub async fn import(&self, bytes: &[u8]) -> Result<CharacterCardInfo, CharacterCardImportError>;
    pub async fn list(&self) -> Result<Vec<CharacterCardInfo>, CharacterCardImportError>;
}
```

Import canonicalization is exact: parse and validate the JSON value, deserialize `CharacterCard`, serialize that typed value with `serde_json::to_vec`, set `canonical_json` to those bytes, and set `digest` to SHA-256 of those bytes. Raw request whitespace and object-key order therefore do not create another Card digest.

Routes:

| Method | Path | Request | Success |
|---|---|---|---|
| `POST` | `/api/character-cards/validate` | Character Card JSON body | `200` `ValidationResponse` |
| `POST` | `/api/character-cards` | Character Card JSON body | `201` `CharacterCardInfoView` |
| `GET` | `/api/character-cards` | none | `200` ordered list of `CharacterCardInfoView` |

```rust
#[derive(Debug, Serialize)]
pub struct CharacterCardInfoView {
    pub character_id: String,
    pub name: String,
    pub creator: Option<String>,
    pub version: String,
    pub digest: String,
}
```

Ordering is `profile.name`, then `character_id`, then Semantic Version string. API and trace output may expose Card identity metadata but never full Profile text unless a separate read endpoint is designed later.

### 3.11 SQLite Migration 0015

Add `crates/aise/assets/persistence/mig/0015_character_role_profile_assets.sql`.

The migration must:

1. Assert that migration `0014` has already run by requiring the final `story_instances` schema from the StoryStateExtractor split.
2. Abort through a named SQLite check constraint `character_role_profile_legacy_data_present` when `story_packs` or `story_instances` contains any row.
3. Rebuild `story_packs` without `characters_json` and `resolved_characters_json`.
4. Preserve the remaining Pack columns with `json_valid` checks and the unique Pack digest/key-version constraints.
5. Create `character_cards` with this schema:

```sql
CREATE TABLE character_cards (
    character_id  TEXT NOT NULL,
    version       TEXT NOT NULL,
    digest        TEXT NOT NULL UNIQUE,
    card_json     TEXT NOT NULL CHECK (json_valid(card_json)),
    canonical_json BLOB NOT NULL,
    created_at    INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (character_id, version)
);
```

6. Create `ix_character_cards_name` only if the implementation stores a separate normalized display-name column; otherwise listing must deserialize bounded rows and sort in Rust.
7. Run `PRAGMA foreign_key_check` before completing.
8. Drop the temporary migration guard table.

The migration must not rewrite v3 JSON, invent UUIDs, retain unparseable rows, or silently delete user data. A populated legacy database receives the named diagnostic failure and must be explicitly replaced after content export.

### 3.12 File and Directory Layout

```text
crates/aise/src/
├── domain/
│   ├── ids.rs
│   └── asset/
│       ├── character_card.rs
│       ├── frozen_ref.rs
│       ├── ids.rs
│       ├── story_pack.rs
│       └── validation.rs
├── config/
│   └── assets.rs
├── persistence/
│   ├── asset_store.rs
│   └── sqlite_asset_store.rs
└── story/
    ├── character_card_service.rs
    ├── pack_service.rs
    └── tests/
        └── character_card_service_tests.rs

crates/aise-server/src/api/
├── character_card.rs
├── mod.rs
└── routes.rs

crates/aise/assets/persistence/mig/
└── 0015_character_role_profile_assets.sql
```

---

## 4. Behavior Rules

1. **CRP0-ID-01**: `CharacterId` identifies only a reusable Character Card and is always a non-nil canonical UUID.
2. **CRP0-ID-02**: `RoleId` identifies only a Role inside one Story definition/instance and is never interchangeable with `CharacterId`.
3. **CRP0-ID-03**: Name is display data; duplicate names are valid and never participate in lookup, equality, persistence keys, Narrative references, or model target binding.
4. **CRP0-PROFILE-01**: Card Profile and Role default profile use the exact same `CharacterProfile` type and validator.
5. **CRP0-PROFILE-02**: A Profile selected from a Card is an indivisible value. No function may merge it with a Role default profile.
6. **CRP0-PROFILE-03**: `background`, current state, goals, relationships, memories, and knowledge cannot appear in `CharacterProfile`.
7. **CRP0-PROFILE-04**: Present optional Profile text is non-empty; absence is represented only by `None`.
8. **CRP0-PACK-01**: Every Story Role Definition has one valid `default_profile`, independent of any Character Card.
9. **CRP0-PACK-02**: Story Pack import rejects `character_assets`, `default_cast`, `StoryRoleKey` field names, and all v3 discriminators.
10. **CRP0-CARD-01**: Character Card import is content-addressed and immutable by `(character_id, version, digest)`.
11. **CRP0-CARD-02**: Exact Card lookup never substitutes another version or digest.
12. **CRP0-TRUST-01**: Card/Pack strings remain Runtime data and can never enter trusted Prompt source, CSI, FTI, slot metadata, or output schemas.
13. **CRP0-BOUND-01**: Every Profile field, example, tag list, Role list, and background is rejected before persistence when its configured limit is exceeded.
14. **CRP0-MIG-01**: Migration `0015` is fresh-data-only and fails with `character_role_profile_legacy_data_present` instead of dropping or partially converting legacy data.
15. **CRP0-MIG-02**: No v3 parser, serde alias, migration loader, or dual table remains after the migration.

### 4.1 Error Handling

- Invalid UUID input returns `DomainInputError::InvalidCharacterId` with the exact message in §3.1.
- Invalid Role syntax or length returns `DomainInputError::InvalidRoleId` with the exact message in §3.1.
- Character Card JSON/schema/profile failures return `CharacterCardImportError::Invalid(ValidationReport)`; they never become `StoreError::Serialization`.
- Missing exact Card revision returns `StoreError::NotFound`.
- Same `(character_id, version)` with a different digest returns `StoreError::ConstraintViolation { constraint: "character_version_digest_conflict" }`.
- JSON, digest, and Store errors must not include raw Profile or background content.
- No import path may use `.unwrap()` or `.expect()` on request data.

### 4.2 Concurrency

- Character Card and Pack Store calls remain async behind `AssetStore`; no background task, queue, cache, or fan-out is added.
- No lock guard may be held across Store I/O.
- Import idempotency is enforced by database uniqueness and an exact digest lookup, not by a process-local mutex.

### 4.3 Observability

- Wrap Character Card Store operations in structured spans named `character_card.validate`, `character_card.import`, and `character_card.list`.
- Record only `character_id`, `version`, `digest`, `status`, `error_code`, byte counts, issue count, and latency.
- Never record Profile text, dialogue examples, Role background, raw JSON, or validation message content in production telemetry.
- Story Pack import telemetry renames `character_asset_count` to no replacement because Packs no longer own Character Cards.

---

## 5. Acceptance Criteria

### Identity and Domain

- [ ] `CharacterId` and `RoleId` match §3.1 and have no unchecked `From<&str>`/`From<String>` implementation.
- [ ] UUID canonicalization, nil rejection, Role syntax, Role byte bound, serialization, deserialization, ordering, and cross-type compile rejection have dedicated tests.
- [ ] `CharacterAssetKey` and `StoryRoleKey` are absent from active Rust, SQL, JSON fixtures, API DTOs, and Prompt assets after all three phases.
- [ ] Duplicate Profile names import successfully under different `CharacterId` or `RoleId` values.

### Profile and Asset Schema

- [ ] `CharacterProfile`, `DialogueExample`, `CharacterCard`, `CharacterMeta`, and `StoryRoleDefinition` match §§3.3–3.5.
- [ ] Role default Profile and Card Profile call the same validator function.
- [ ] Missing Role `default_profile` fails; optional Profile fields may be absent; present empty optional fields fail.
- [ ] v3 discriminators and every removed Profile field fail validation with no compatibility path.
- [ ] Story Pack with zero Character Cards and no default cast imports successfully.
- [ ] Story Pack exports only the v4 shape and round-trips byte-equivalent typed data.

### Character Card Store and API

- [ ] Importing identical Card bytes twice returns the same stored Card identity without a duplicate row.
- [ ] Importing a different digest for the same `(character_id, version)` returns `character_version_digest_conflict`.
- [ ] Exact reference lookup validates `character_id`, `version`, and `digest`; changing any one returns `NotFound` or the typed mismatch result selected by the Store contract.
- [ ] Character Card list ordering is deterministic and API responses contain metadata only.
- [ ] Card/Pack Prompt-like strings remain untrusted data in trust-boundary tests.

### Limits and Migration

- [ ] Every new `AssetLimitsConfig` field has the exact default and positive-value validation in §3.7.
- [ ] Old `max_character_assets` is rejected rather than aliased.
- [ ] Migration `0015` applies after `0014` to a fresh database and `PRAGMA foreign_key_check` is empty.
- [ ] Migration `0015` fails with named constraint `character_role_profile_legacy_data_present` on a populated v14 database and leaves rows unchanged.
- [ ] `story_packs` has no `characters_json` or `resolved_characters_json`; `character_cards` matches §3.11.

### Suite-Wide Hard-Refactor Checks

- [ ] `rg -n 'CharacterAssetKey|StoryRoleKey|RoleBinding|DefaultCast|FrozenCharacterAssetRef|CharacterAssetSource' crates config` returns zero matches after Phase 2.
- [ ] `rg -n 'profile\.description|pub values:|pub fears:|struct SpeakingStyle|speaking_style\.register|speaking_style\.verbosity|speaking_style\.traits|speaking_register|speaking_verbosity|speaking_traits' crates/aise/src crates/aise/assets/prompts` returns zero matches after Phase 2.
- [ ] `cargo fmt --all --check` passes.
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo test --workspace` passes.

---

## 6. Required Tests

### 6.1 Identity Tests

Add dedicated tests for:

1. canonical lowercase hyphenated UUID acceptance;
2. uppercase UUID normalization;
3. nil UUID rejection;
4. malformed UUID rejection;
5. valid Role IDs using dots, underscores, and hyphens;
6. empty, uppercase, whitespace, leading separator, trailing separator, repeated separator, control character, and over-128-byte Role rejection;
7. JSON round-trip for both newtypes;
8. duplicate names with distinct IDs.

### 6.2 Profile and Pack Validation Tests

Add one case for each required/optional field, each exact byte limit, one-byte overflow, dialogue example count overflow, unknown field, old v3 field, missing default profile, unknown relationship Role, unknown playable Role, and unknown Narrative Role.

The same valid and invalid Profile fixtures must be exercised once as a Card Profile and once as a Role default Profile to prove validator reuse.

### 6.3 Character Card Store/API Tests

Cover fresh import, idempotent import, version conflict, exact lookup, wrong digest, multiple versions under one ID, deterministic list order, invalid JSON, invalid Profile, injection-like Profile data, and response redaction.

### 6.4 Migration Tests

Test both:

- an empty schema at version 14 migrating to version 15;
- a populated version-14 database where migration fails and Pack/Instance row counts and digests remain unchanged.

---

## 7. Out of Scope / Future Work

- Character Card deletion and retention policy require a separate lifecycle design because existing Story Instances may retain source metadata.
- Character Card search, tags filtering, thumbnails, editor UI, and remote catalogs are separate product features.
- Automatic v3 content conversion is intentionally absent; a standalone offline converter may be designed later without entering runtime code.

---

## 8. References

- Source design: `doc/design/CSI-RC-FTI/2026-08-14-character-role-profile-design-gpt.md`.
- Phase 1: `doc/exec/character-role-profile-spec/2026-08-14-character-role-profile-spec-phase-1-gpt.md`.
- Phase 2: `doc/exec/character-role-profile-spec/2026-08-14-character-role-profile-spec-phase-2-gpt.md`.
- Prior Story Pack contract: `doc/exec/2026-08-07-story-pack-v3-spec-gpt.md`.
- StoryStateExtractor prerequisite: `doc/exec/CSI-RC-FTI/2026-08-14-story-state-extractor-split-spec-gpt.md`.
- Current Card schema: `crates/aise/src/domain/asset/character_card.rs:7`.
- Current Story Role schema: `crates/aise/src/domain/asset/story_pack.rs:70`.
- Current asset Store: `crates/aise/src/persistence/asset_store.rs:10`.
- Project guardrails: `AGENTS.md` and `doc/agents/guardrails/`.
