# Turn Pipeline Constraints

Project-specific hard constraints for Turn execution in `crates/aise-core`.
Architecture baseline:
[pipeline framework design](../../design/2026-10-13-pipeline-framework-design-opus.md).

## R-AISE-01 - Runtime is the single orchestrator

**Level: MUST**

- Turn execution MUST be orchestrated only by
  `pipeline::runtime::Runtime::run_turn` with a fixed stage topology.
- Pipelines MUST NOT call each other (see `R-LAYER-03`).
- Optional stages are decided by `Runtime` from stage outputs, never by a
  pipeline.

## R-AISE-02 - Typed stage contract

**Level: MUST**

- Every stage MUST implement `pipeline::common::Pipeline`:

```rust
pub trait Pipeline: Send + Sync {
    type Input: Send + Serialize;
    type Output: Send + Serialize;

    fn stage(&self) -> PipelineStage;

    async fn execute(
        &self,
        input: Self::Input,
        story_ctx: &StoryContext,
        control: &TurnControl,
        sink: &dyn TurnEventSink,
        observation: &Observation,
    ) -> Result<Self::Output, PipelineError>;
}
```

- Stages MUST run only through `PipelineRunner::run`.
- Data passes between stages only via `Input` / `Output` moved by `Runtime`.
- NEVER exchange business state through `Observation`, `StoryContext`, or
  hidden shared state.

## R-AISE-03 - Turn data lifetime

**Level: MUST**

- Turn-scoped data MUST live only in the `run_turn` stack frame.
- Turn-scoped data MUST NOT be persisted directly or shared across Turns.
- `StoryContext` is a read-only snapshot for every stage; it changes only by
  applying a `StoryCommit` through the commit stage.

## R-AISE-04 - Module and stage layout

**Level: MUST**

- Top-level aise-core modules: `core` / `engine` / `llm` / `persistence` /
  `pipeline` / `prompt` / `trace`.
- `pipeline/` holds `common/`, `runtime/`, and one directory per stage.
- Each stage splits by abstraction level:

```text
pipeline/{stage}/
  mod.rs                  pub(super) re-exports only
  {stage}.rs              Input, Output, {Stage}Pipeline, impl Pipeline
  {stage}_prompt.rs       prompt id, *_VAR constants, PromptVars assembly
  {stage}_llm.rs          thin LlmGateway::complete wrapper and result mapping
  {stage}_trace.rs        begin_* / finish_* observation helpers
  {stage}_{domain}.rs     pure computation, no I/O or LLM
  config.rs               {Stage}Config (optional)
  tests/{source}_tests.rs
```

- `execute` MUST follow: `{stage}_trace::begin_*` -> private orchestration
  method -> `{stage}_trace::finish_*` -> map to `Output`.

## R-AISE-05 - Commit owns writes

**Level: MUST**

- Only the commit stage MAY write Turn results to storage.
- The commit stage MUST apply the whole `StoryCommit` atomically (all or
  nothing) and MUST NOT call an LLM.
- Other stages MUST NOT call storage write methods.

## R-AISE-06 - Bounded Validate/Repair loop

**Level: MUST**

- The Validate/Repair loop budget MUST be owned by `Runtime` and consumed
  before each repair.
- On exhaustion, the Turn MUST end with a diagnosable outcome, never loop
  forever.

## R-AISE-07 - Character thoughts are not world facts

**Level: MUST**

- Character thoughts are a character's viewpoint, not world facts.
- They MUST NOT be committed as world state by the generator.

## R-AISE-08 - Prompt library conventions

**Level: MUST**

- Prompt id is `{stage}.{action}`, registered in
  `crates/aise-core/assets/prompts/index.toml`.
- Each prompt has three layers rendered in `PromptLayer::ORDERED` order:
  - CSI (System): role and rules.
  - RC (User): runtime data, treated as untrusted.
  - FTI (System): final task and output format.
- Template files live at `{csi|rc|fti}/{stage}-{action}.md.j2`.
- Shared prior-story and constraint rendering MUST go through
  `pipeline/common/pipeline_prompt.rs`.
- NEVER hardcode prompt text in Rust.
