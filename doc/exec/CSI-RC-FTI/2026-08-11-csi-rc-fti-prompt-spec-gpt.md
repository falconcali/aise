# CSI-RC-FTI Prompt Architecture — Spec 2.0 Final

> **Model**: GPT-5.6 Sol  
> **Date**: 2026-08-12  
> **Status**: Final  
> **Source Design**: [Context Preparation and Retrieval — Design](../../design/2026-08-08-context-preparation-retrieval-design-gpt.md)  
> **Related Architecture**: [AISE Architecture](../../design/2026-08-04-Architecture-gpt.md)  
> **Supersedes**: `2026-08-11-csi-rc-fti-prompt-spec-gpt.md`  
> **Child Specs**: [WriterPlanner CSI-RC-FTI](./2026-08-11-writer-planner-csi-rc-fti-prompt-spec-gpt.md), [CharacterThink CSI-RC-FTI](./2026-08-12-character-think-csi-rc-fti-prompt-spec-gpt.md)  
> **Phase**: N/A

---

## 1. Goal

Implement only the shared CSI-RC-FTI prompt architecture so every Turn LLM stage composes trusted CSI, stage-specific untrusted RC, and trusted FTI through one reusable framework, while all profile-specific prompt semantics remain exclusively owned by that profile's child spec.

---

## 2. Scope & Non-Goals

### 2.1 In Scope

- Define the shared logical three-layer contract:
  - `CSI` — Core System Instruction.
  - `RC` — Runtime Context.
  - `FTI` — Final Task Instruction.
- Define layer trust, ordering, ownership, and composition invariants.
- Define the shared `PromptProfile`, `PromptComposition`, profile-to-layer asset binding, composition API, and metadata contracts.
- Integrate CSI-RC-FTI composition with the existing `PromptCatalog`, `PromptResolver`, `PromptRenderer`, slot validation, and prompt metadata pipeline.
- Replace the current generic whole-context JSON encoder in Turn LLM prompt generation.
- Define the extension seam through which each child spec supplies:
  - its typed prompt projection;
  - RC render variables;
  - trusted FTI variables when required;
  - its three prompt assets and slot registrations.
- Define the shared trust-boundary validation, error handling, concurrency, observability, and architecture-level tests.
- Preserve the four existing logical profiles:
  - `WriterPlanner`;
  - `CharacterThink`;
  - `StoryGenerator`;
  - `StoryRepairer`.

### 2.2 Non-Goals

This parent spec MUST NOT define or generate profile-specific prompt behavior.

Specifically, it does **not** define:

- exact CSI wording, `MUST` / `SHOULD` / `NEVER` rules, or FTI wording for any profile;
- exact RC sections, RC ordering, field lists, semantic fragments, visibility rules, or token budgets for any profile;
- `WriterPlanner` planning semantics, retrieval semantics, output fields, validation, or `.md.j2` content;
- `CharacterThink` target selection, epistemic rules, story-continuity handling, Thinking Focus semantics, thought fields, validation, or `.md.j2` content;
- `StoryGenerator` or `StoryRepairer` profile semantics before their dedicated child specs exist;
- stage-specific output models or structured-output validation;
- Turn pipeline order, retrieval execution, Character Think execution, story generation, validation, repair, or commit semantics;
- domain-model changes made only for prompt-rendering convenience;
- provider-specific LLM transport semantics beyond the shared logical prompt handoff boundary.

### 2.3 Implementation Constraints

- This spec generates **shared prompt-architecture code only**.
- This spec MUST NOT generate or modify profile-specific `.md.j2` bodies.
- This spec MUST NOT generate profile-specific `*PromptContext`, projector, output, validator, token-budget, or stage-execution code.
- Profile-specific implementation MUST be generated only from that profile's child spec.
- If this parent spec and a child spec differ on profile semantics, the child spec is authoritative for that profile.
- Child specs MUST consume the shared contracts in this spec and MUST NOT reimplement an alternative CSI-RC-FTI composition framework.
- This is a hard replacement of the current Turn prompt-composition path. Do **not** keep fallback paths, compatibility shims, or dual prompt-generation paths.
- Old shared types/functions/modules superseded by this spec MUST be deleted, not deprecated.
- The existing `PromptCatalog`, `PromptResolver`, `PromptRenderer`, slot registry, and prompt metadata system MUST be reused rather than duplicated.
- The current generic `Serialize -> serde_json::to_string(context)` Turn RC path MUST be removed.
- Runtime story data MUST NOT select or modify CSI assets, FTI assets, slot IDs, prompt packs, output schemas, or provider-role authority.
- Prompt-facing data remains a read-only projection of authoritative Turn state; the prompt layer MUST NOT become a second mutable source of truth.

---

## 3. Contracts

### 3.1 Ownership Contract

| Concern | Parent architecture spec | Profile child spec | Turn / domain layer |
|---|---|---|---|
| CSI-RC-FTI logical order and trust | Owns | Uses | N/A |
| `PromptProfile` and shared composition types | Owns | Uses | N/A |
| Profile-to-CSI/RC/FTI slot binding mechanism | Owns | Registers entries | N/A |
| `PromptCatalog` / renderer integration | Owns | Uses | N/A |
| Typed stage prompt context | Does not define | Owns | Supplies source state |
| RC sections, ordering, labels, omission rules | Does not define | Owns | N/A |
| CSI / FTI exact instructions | Does not define | Owns | N/A |
| `.md.j2` bodies | Does not define | Owns | N/A |
| Output type / schema / semantic validation | Does not define | Owns | Owns authoritative domain types where applicable |
| Retrieval / stage execution behavior | Does not define | References only when required | Owns |
| Provider transport encoding | Defines handoff seam only | Does not define | Provider layer owns |

Code generation from this file MUST stop at the parent-owned column.

### 3.2 Logical Prompt Composition

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptProfile {
    WriterPlanner,
    CharacterThink,
    StoryGenerator,
    StoryRepairer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PromptLayer {
    Csi,
    Rc,
    Fti,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreSystemInstruction(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeContextMessage(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalTaskInstruction(String);

#[derive(Debug, Clone)]
pub struct PromptComposition {
    pub profile: PromptProfile,
    pub csi: CoreSystemInstruction,
    pub rc: RuntimeContextMessage,
    pub fti: FinalTaskInstruction,
    pub metadata: PromptCompositionMetadata,
}

#[derive(Debug, Clone)]
pub struct PromptCompositionMetadata {
    pub csi: PromptMetadata,
    pub rc: PromptMetadata,
    pub fti: PromptMetadata,
}
```

The semantic contract is fixed:

| Layer | Authority | Shared architectural purpose |
|---|---|---|
| `CSI` | trusted engine instruction | durable identity, responsibility, rules, and runtime-data boundary |
| `RC` | untrusted runtime data | only stage-specific data selected by the child implementation |
| `FTI` | trusted engine instruction | immediate task reminder and output contract |

The logical model-visible order is always:

```text
CSI
↓
RC
↓
FTI
```

There is no fourth logical prompt layer.

### 3.3 Profile Asset Binding

The shared architecture binds every implemented profile to exactly three trusted prompt slots.

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptProfileAssets {
    pub csi_slot: SlotId,
    pub rc_slot: SlotId,
    pub fti_slot: SlotId,
}

#[derive(Debug, Default)]
pub struct PromptProfileRegistry {
    entries: HashMap<PromptProfile, PromptProfileAssets>,
}

impl PromptProfileRegistry {
    pub fn register(
        &mut self,
        profile: PromptProfile,
        assets: PromptProfileAssets,
    ) -> Result<(), PromptError>;

    pub fn assets_for(
        &self,
        profile: PromptProfile,
    ) -> Result<&PromptProfileAssets, PromptError>;
}
```

Registration is architecture-level; the concrete slot IDs for a profile are supplied with that profile's child implementation.

A profile registration is valid only when:

```text
csi_slot != rc_slot
csi_slot != fti_slot
rc_slot  != fti_slot
```

All three slots MUST resolve through the existing `PromptCatalog`.

### 3.4 Composition Input Boundary

The shared composer accepts already-prepared render variables. It MUST NOT know how a profile projects `TurnExecutionContext`.

```rust
#[derive(Debug, Clone, Default)]
pub struct RuntimePromptVars(HashMap<String, serde_json::Value>);

#[derive(Debug, Clone, Default)]
pub struct TrustedPromptVars(HashMap<String, serde_json::Value>);

#[derive(Debug, Clone)]
pub struct PromptCompositionInput {
    pub profile: PromptProfile,
    pub rc_vars: RuntimePromptVars,
    pub fti_vars: TrustedPromptVars,
}
```

Required construction boundary:

```text
TurnExecutionContext / stage request
        │
        │  child-spec-owned typed projection
        ▼
Profile-specific PromptContext
        │
        │  child-spec-owned semantic rendering preparation
        ▼
RuntimePromptVars + TrustedPromptVars
        │
        │  shared architecture
        ▼
PromptComposer
```

Rules for variable ownership:

- `CSI` receives no runtime variables from `PromptCompositionInput`.
- `RuntimePromptVars` may contain untrusted story/runtime data only.
- `TrustedPromptVars` may contain engine-authored FTI data only, such as a trusted schema fragment generated from code.
- Child code MUST NOT copy player text, story assets, memories, retrieved data, previous model output, or validation text into `TrustedPromptVars`.
- The parent architecture does not define the keys inside either variable map; each child spec owns its keys.

### 3.5 Prompt Composer

```rust
pub struct PromptComposer<'a> {
    catalog: &'a PromptCatalog,
    profiles: &'a PromptProfileRegistry,
}

impl<'a> PromptComposer<'a> {
    pub fn new(
        catalog: &'a PromptCatalog,
        profiles: &'a PromptProfileRegistry,
    ) -> Self;

    pub fn compose(
        &self,
        input: &PromptCompositionInput,
        options: &PromptRenderOptions,
    ) -> Result<PromptComposition, PromptError>;
}
```

`PromptComposer::compose` performs only shared architecture work:

```text
1. Resolve PromptProfileAssets from PromptProfileRegistry.
2. Render CSI with empty variables through PromptCatalog.
3. Render RC with RuntimePromptVars through PromptCatalog.
4. Render FTI with TrustedPromptVars through PromptCatalog.
5. Require all three results to be text/fragment output, not message bundles.
6. Wrap each rendered layer in its trust-specific newtype.
7. Return PromptComposition with per-layer PromptMetadata.
```

The composer MUST NOT:

- inspect `TurnExecutionContext`;
- branch on WriterPlanner/CharacterThink/StoryGenerator/StoryRepairer semantics;
- know RC section names;
- know output field names;
- validate stage output semantics;
- perform retrieval;
- call the LLM.

### 3.6 PromptCatalog Extension

The existing catalog remains the only asset resolution and Jinja rendering path.

Add a metadata-preserving text helper instead of duplicating catalog logic:

```rust
impl PromptCatalog {
    pub fn render_text_with_metadata(
        &self,
        slot_id: &str,
        vars: &HashMap<String, serde_json::Value>,
        options: &PromptRenderOptions,
    ) -> Result<(String, PromptMetadata), PromptError>;
}
```

Required behavior:

```text
render_text_with_metadata
    -> render_slot
    -> require RenderedPrompt::Text
    -> normalize rendered text using the existing normalization behavior
    -> return (text, metadata)
```

`PromptComposer` MUST call this API for all three layers.

### 3.7 Provider Handoff Contract

The prompt subsystem hands the provider layer a logical `PromptComposition`, not a serialized whole-context JSON string.

```rust
pub trait ProviderPromptEncoder: Send + Sync {
    type Encoded;

    fn encode(
        &self,
        composition: &PromptComposition,
    ) -> Result<Self::Encoded, PromptError>;
}
```

Provider implementations MAY differ in physical message-role encoding, but every implementation MUST preserve these logical invariants:

```text
CSI remains trusted.
RC remains untrusted data.
FTI remains trusted.
Model-visible semantic order remains CSI -> RC -> FTI.
Runtime content cannot acquire trusted instruction authority through encoding.
```

This parent spec defines the seam and invariants only. Provider-specific role/message transport is outside this spec.

### 3.8 Shared Errors

Reuse `PromptError` and add only shared architecture variants required by the new framework:

```rust
pub enum PromptError {
    // existing variants remain

    #[error("prompt profile already registered: {0}")]
    DuplicateProfileRegistration(String),

    #[error("prompt profile is not registered: {0}")]
    ProfileNotRegistered(String),

    #[error("prompt profile `{profile}` reuses slot `{slot}` across CSI/RC/FTI")]
    DuplicateLayerSlot {
        profile: String,
        slot: String,
    },

    #[error("prompt layer `{layer}` for profile `{profile}` must render as text")]
    LayerMustRenderAsText {
        profile: String,
        layer: String,
    },

    #[error("prompt trust boundary violated: {0}")]
    TrustBoundaryViolation(String),
}
```

Do not add profile-specific error variants in this parent spec.

### 3.9 Shared File / Directory Layout

The architecture implementation MAY add or reshape only shared prompt-framework files:

```text
crates/aise/src/prompt/
├── composition.rs             # PromptLayer, wrappers, PromptComposition, vars, PromptComposer
├── profile.rs                 # PromptProfile + PromptProfileAssets + PromptProfileRegistry
├── catalog.rs                 # reuse; add render_text_with_metadata
├── error.rs                   # shared architecture errors only
├── model_request.rs           # shared request envelope only; profile contexts move to child-owned code
├── renderer.rs                # reuse
├── resolver.rs                # reuse
├── metadata.rs                # reuse
├── model.rs                   # reuse shared SlotId / PromptMetadata dependencies as appropriate
└── tests/
    ├── composition_tests.rs
    └── profile_registry_tests.rs
```

The obsolete generic encoder is removed from the Turn prompt path:

```text
crates/aise/src/prompt/runtime_context_encoder.rs   # DELETE if no non-Turn caller remains
```

The shared physical asset root is:

```text
crates/aise/assets/prompts/context-v2/
├── csi/
├── rc/
└── fti/
```

This parent spec MUST NOT create or edit the profile-specific `.md.j2` files inside those directories. Those files are generated by the corresponding child specs.

### 3.10 Parent / Child Codegen Protocol

The required implementation sequence is:

```text
Parent architecture spec
    -> shared composition framework compiles independently

WriterPlanner child spec
    -> WriterPlanner projection + vars + slots + assets + output validation

CharacterThink child spec
    -> CharacterThink projection + vars + slots + assets + output validation

Future StoryGenerator child spec
    -> StoryGenerator-specific implementation

Future StoryRepairer child spec
    -> StoryRepairer-specific implementation
```

A child spec may modify shared code only to register or consume its profile through the contracts above. It MUST NOT fork or replace the shared composer/catalog path.

---

## 4. Behavior Rules

### 4.1 Ownership and Codegen Boundary

1. **P-OWN-01**: Code generated from this parent spec MUST contain no profile-specific RC field list, RC section order, CSI rule text, FTI rule text, output field list, stage validator, or profile `.md.j2` body.
2. **P-OWN-02**: `WriterPlanner` semantics are authoritative only in `2026-08-11-writer-planner-csi-rc-fti-prompt-spec-gpt.md`.
3. **P-OWN-03**: `CharacterThink` semantics are authoritative only in `2026-08-12-character-think-csi-rc-fti-prompt-spec-gpt.md`.
4. **P-OWN-04**: Until dedicated child specs exist, this parent spec MUST NOT invent StoryGenerator or StoryRepairer CSI/RC/FTI details.
5. **P-OWN-05**: A child spec MUST use `PromptComposition`, `PromptProfileRegistry`, `PromptComposer`, and `PromptCatalog`; it MUST NOT introduce a second prompt-composition framework.
6. **P-OWN-06**: Profile-specific prompt context types MUST NOT live in shared `prompt/model_request.rs` solely because all stages call an LLM; they belong with the owning stage/profile implementation.

### 4.2 Composition and Trust

7. **P-COMP-01**: Every migrated Turn LLM profile MUST produce exactly three logical layers: one CSI, one RC, and one FTI.
8. **P-COMP-02**: The logical order MUST always be CSI -> RC -> FTI.
9. **P-COMP-03**: CSI MUST render from a trusted project slot selected by the registered `PromptProfile`; runtime data MUST NOT provide CSI variables.
10. **P-COMP-04**: RC MUST render only from child-produced `RuntimePromptVars`.
11. **P-COMP-05**: FTI MUST render from a trusted project slot and may consume only `TrustedPromptVars` produced by engine code.
12. **P-COMP-06**: Structured output instructions remain part of FTI; the framework MUST NOT introduce a fourth logical output layer.
13. **P-COMP-07**: Runtime strings that look like instructions remain RC data and MUST NOT change slot selection, prompt pack selection, trusted variables, or provider authority.
14. **P-COMP-08**: The parent framework MUST NOT serialize `TurnExecutionContext`, a stage prompt context, or another whole domain object directly into RC JSON.
15. **P-COMP-09**: RC semantic rendering is child-owned; the shared composer treats RC variables as opaque render inputs.

### 4.3 Asset and Rendering Integration

16. **P-ASSET-01**: `PromptCatalog` remains the single source of truth for slot resolution, asset selection, Jinja rendering, input-variable validation, policies, and prompt metadata.
17. **P-ASSET-02**: `PromptProfileRegistry` stores trusted slot bindings only; no story/runtime value may mutate the registry during a Turn.
18. **P-ASSET-03**: Registering the same profile twice MUST return `PromptError::DuplicateProfileRegistration`.
19. **P-ASSET-04**: Registering one slot in more than one layer of the same profile MUST return `PromptError::DuplicateLayerSlot`.
20. **P-ASSET-05**: Composing an unregistered profile MUST return `PromptError::ProfileNotRegistered` before rendering begins.
21. **P-ASSET-06**: CSI, RC, and FTI slots MUST render to text/fragment-compatible output. Message-bundle output MUST fail composition.
22. **P-ASSET-07**: `PromptComposer` MUST preserve `PromptMetadata` separately for CSI, RC, and FTI.

### 4.4 Migration

23. **P-MIG-01**: Remove `RuntimeContextEncoder::encode<C: Serialize>` from the Turn LLM path.
24. **P-MIG-02**: Remove generic `serde_json::to_string(context)` whole-context RC encoding from the Turn LLM path.
25. **P-MIG-03**: Do not keep the old JSON context encoder as a runtime fallback after CSI-RC-FTI composition is wired.
26. **P-MIG-04**: Existing shared catalog/resolver/renderer code MUST be extended in place rather than replaced by `context-v2`-specific duplicates.
27. **P-MIG-05**: Existing old profile-specific context structs in shared prompt code MUST be removed or moved only when their owning child spec replaces them; this parent spec MUST NOT invent their replacement fields.

### 4.5 Error Handling

- All composition, slot-resolution, render, and trust-boundary failures MUST return `PromptError`; no `unwrap`, `expect`, silent fallback, or default prompt is allowed in the Turn LLM path.
- A failure rendering any one of CSI, RC, or FTI MUST fail the whole composition.
- Profile registration errors MUST be detected during trusted startup/assembly when possible, before Turn execution.
- Profile-specific projection or output-validation errors remain child-owned and MUST NOT be collapsed into generic architecture semantics.

### 4.6 Concurrency

- `PromptProfileRegistry` MUST be fully built before concurrent Turn execution and treated as immutable afterward.
- `PromptComposer::compose` MUST require no mutable global state and MUST be safe to call concurrently when its referenced catalog/registry are shared read-only values.
- Prompt composition MUST NOT hold a lock across an LLM `.await`.
- Existing shared LLM concurrency/rate-limit contracts remain unchanged; this spec MUST NOT introduce a bypass path.

### 4.7 Observability

For every composed request, tracing metadata SHOULD include:

```text
prompt.profile
prompt.csi.slot
prompt.rc.slot
prompt.fti.slot
prompt.csi.pack
prompt.rc.pack
prompt.fti.pack
prompt.csi.asset_hash
prompt.rc.asset_hash
prompt.fti.asset_hash
prompt.csi.render_ms
prompt.rc.render_ms
prompt.fti.render_ms
prompt.csi.bytes
prompt.rc.bytes
prompt.fti.bytes
```

Observability MUST NOT log full RC content by default.

The architecture layer MUST NOT emit profile-semantic metrics such as Planner gap counts or CharacterThink knowledge counts; those belong to child implementations.

---

## 5. Acceptance Criteria

- [ ] `PromptProfile` still contains exactly `WriterPlanner`, `CharacterThink`, `StoryGenerator`, and `StoryRepairer`.
- [ ] `PromptLayer`, `CoreSystemInstruction`, `RuntimeContextMessage`, `FinalTaskInstruction`, `PromptComposition`, and `PromptCompositionMetadata` exist as shared prompt-framework types.
- [ ] `PromptProfileRegistry` binds an implemented profile to exactly three distinct `SlotId`s and rejects duplicate profile registration.
- [ ] `PromptCompositionInput` contains only `profile`, `rc_vars`, and `fti_vars`; it does not contain `TurnExecutionContext` or any stage-specific context type.
- [ ] `PromptComposer::compose` renders CSI, RC, and FTI exclusively through `PromptCatalog` and returns per-layer `PromptMetadata`.
- [ ] `PromptCatalog::render_text_with_metadata` reuses `render_slot` and existing text normalization.
- [ ] The shared composer contains no branch that inspects WriterPlanner, CharacterThink, StoryGenerator, or StoryRepairer semantic fields.
- [ ] Code generated from this spec creates or edits no profile-specific `.md.j2` body.
- [ ] Code generated from this spec defines no WriterPlanner RC sections, WriterPlanner output fields, CharacterThink RC sections, CharacterThought fields, or profile-specific output validation.
- [ ] `rg 'serde_json::to_string\(context\)' crates/aise/src` returns zero matches in the Turn prompt-generation path.
- [ ] `RuntimeContextEncoder::encode` has no Turn LLM caller; `runtime_context_encoder.rs` is deleted when no other valid caller remains.
- [ ] Runtime story/player/retrieval/model-output data cannot choose CSI/FTI slots or populate `TrustedPromptVars` — verified by architecture trust-boundary tests.
- [ ] Registering an unregistered profile path fails with `PromptError::ProfileNotRegistered` and does not fall back to another profile.
- [ ] A layer that resolves to `RenderedPrompt::Messages` fails composition rather than silently flattening messages.
- [ ] `cargo test prompt::` passes.
- [ ] `cargo fmt --check` passes.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes.
- [ ] The WriterPlanner child implementation can be generated without changing shared composition semantics.
- [ ] The CharacterThink child implementation can be generated without changing shared composition semantics.

---

## 6. Out of Scope / Future Work

- Exact `WriterPlanner` CSI-RC-FTI, projection, prompt assets, structured output, and validation are owned by [`2026-08-11-writer-planner-csi-rc-fti-prompt-spec-gpt.md`](./2026-08-11-writer-planner-csi-rc-fti-prompt-spec-gpt.md).
- Exact `CharacterThink` CSI-RC-FTI, projection, prompt assets, structured output, and validation are owned by [`2026-08-12-character-think-csi-rc-fti-prompt-spec-gpt.md`](./2026-08-12-character-think-csi-rc-fti-prompt-spec-gpt.md).
- `StoryGenerator` requires its own CSI-RC-FTI child spec before profile-specific prompt code is generated.
- `StoryRepairer` requires its own CSI-RC-FTI child spec before profile-specific prompt code is generated.
- Provider-specific physical message-role encoding remains owned by each LLM provider adapter; this architecture supplies only the logical composition and handoff contract.

---

## 7. References

- Source design: [`../../design/2026-08-08-context-preparation-retrieval-design-gpt.md`](../../design/2026-08-08-context-preparation-retrieval-design-gpt.md)
- Related architecture: [`../../design/2026-08-04-Architecture-gpt.md`](../../design/2026-08-04-Architecture-gpt.md)
- Previous parent spec: [`2026-08-11-csi-rc-fti-prompt-spec-gpt.md`](./2026-08-11-csi-rc-fti-prompt-spec-gpt.md)
- WriterPlanner child spec: [`2026-08-11-writer-planner-csi-rc-fti-prompt-spec-gpt.md`](./2026-08-11-writer-planner-csi-rc-fti-prompt-spec-gpt.md)
- CharacterThink child spec: [`2026-08-12-character-think-csi-rc-fti-prompt-spec-gpt.md`](./2026-08-12-character-think-csi-rc-fti-prompt-spec-gpt.md)
- Existing prompt profile: [`../../../crates/aise/src/prompt/profile.rs`](../../../crates/aise/src/prompt/profile.rs)
- Existing prompt catalog: [`../../../crates/aise/src/prompt/catalog.rs`](../../../crates/aise/src/prompt/catalog.rs)
- Existing prompt renderer: [`../../../crates/aise/src/prompt/renderer.rs`](../../../crates/aise/src/prompt/renderer.rs)
- Existing generic runtime encoder to replace: [`../../../crates/aise/src/prompt/runtime_context_encoder.rs`](../../../crates/aise/src/prompt/runtime_context_encoder.rs)
