# Layer Dependencies

Scope: `crates/aise-core` and `crates/aise-service`. `crates/aise` and
`crates/aise-server` are frozen reference crates (see `AGENTS.md` Scope).

General principle: dependencies flow one way, from the composition root toward
the leaf modules. Inner modules MUST NOT know about outer modules.

```text
aise-service (binary)
        |
        v
engine (in-crate composition root: AiseEngine, AiseFactory, EngineConfig)
        |
        v
pipeline::runtime (Runtime, the single Turn orchestrator)
        |
        v
pipeline stages (baseline, plan, retrieval, think, generate,
                 validate, repair, extract, summary, commit)
        |
        v
pipeline::common (Pipeline, PipelineRunner, PipelineStage, PipelineError,
                  PipelineConfig, pipeline_prompt, pipeline_trace)
        |
        v
llm        prompt        persistence
  |  \       |   \           |
  |   \      |    \          |
  v    v     v     v         v
 core  trace (leaf modules)
```

`core` is the pure domain model (story, role, narrative, knowledge, turn, IDs,
errors). `trace` is the cross-cutting observability foundation
(`Trace`, `Observation`, `ObservationSession`). Both are leaves.

## R-LAYER-01 - Inner modules do not depend on outer modules

**Level: MUST**

- `core` and `trace` MUST NOT import any other internal module.
- `llm`, `prompt`, and `persistence` MUST NOT import `pipeline` or `engine`.
- `pipeline` MUST NOT import `engine`.
- No aise-core module may import `aise-service`, `crates/aise`, or
  `crates/aise-server`.
- Notifications toward callers MUST use injected traits defined in the inner
  module (e.g. `core::TurnEventSink`), not concrete outer types.

## R-LAYER-02 - Reverse dependencies use traits and injection

**Level: MUST**

- Inner modules MUST notify outer modules only through injected trait objects
  or generic bounds.
- Concrete implementations (LLM providers, store backends, event sinks) MUST be
  wired once, at the composition root (`engine::AiseFactory` / `AiseEngine`,
  or `aise-service` for sinks and observability exporters).
- Long-lived services (`LlmGateway`, `Prompt`, `Store`) MUST be created by the
  composition root and injected as `Arc<_>`; pipelines MUST NOT construct them.

## R-LAYER-03 - Pipeline stages are isolated

**Level: MUST**

- A pipeline stage module MUST NOT import any other stage module, neither its
  `Pipeline` type nor its data types.
- Types shared by more than one stage MUST live in `core` (domain data) or in
  `pipeline::common` (pipeline mechanics).
- Only `pipeline::runtime` MAY import stage modules.
- `pipeline::common` MUST NOT import stage modules, with one exception:
  `pipeline/common/config.rs` MAY import stage `*Config` types to aggregate
  them into `PipelineConfig`.

## R-LAYER-04 - Pipelines use services, not backends

**Level: MUST**

- Pipelines MUST call LLMs only through `llm::LlmGateway`; they MUST NOT import
  `LlmProvider` implementations (e.g. `OpenAiCompatProvider`).
- Pipelines MUST access storage only through `persistence::Store` and the
  `PackStore` / `StoryStore` traits; they MUST NOT import store backends
  (`*_mem`, `*_sqlite`).
- Pipelines MUST render prompts only through `prompt::Prompt`.

## R-LAYER-05 - Config follows its owning module

**Level: MUST**

- Each module owns its own typed `*Config` in its own `config.rs`
  (`LlmConfig`, `PromptConfig`, `PersistanceConfig`, `PipelineConfig`, stage
  configs such as `SummaryConfig`).
- `EngineConfig` is the single root configuration; every module config MUST be
  reachable from it.
- Stage configs MUST be aggregated under `PipelineConfig`.
- A limit or setting MUST have exactly one authoritative definition; NEVER
  duplicate the same default in two modules.

## Dependency matrix

Anything not listed under "may depend on" is forbidden.

| module | may depend on | must not depend on |
| --- | --- | --- |
| `core` | its own submodules | every other internal module |
| `trace` | external crates / std | every internal module |
| `llm` | `core`, `trace` | `prompt`, `persistence`, `pipeline`, `engine` |
| `prompt` | `core`, `trace` | `llm`, `persistence`, `pipeline`, `engine` |
| `persistence` | `core` | `llm`, `prompt`, `trace`, `pipeline`, `engine` |
| `pipeline::common` | `core`, `llm`, `persistence`, `prompt`, `trace`; stage `*Config` (only in `config.rs`) | other stage items, `runtime`, `engine` |
| pipeline stages | `core`, `trace`, `llm::LlmGateway` and LLM data types, `persistence::Store` and store traits, `prompt::Prompt` and prompt data types, `pipeline::common` | other stages, `runtime`, `engine`, LLM providers, store backends |
| `pipeline::runtime` | `core`, `llm`, `persistence`, `prompt`, `trace`, `pipeline::common`, all stages | `engine` |
| `engine` | every aise-core module | (nothing inside aise-core may import `engine`) |
| `aise-service` | public API of `aise-core` | `crates/aise`, `crates/aise-server` |

Forbidden reverse dependencies: `core -> *`, `trace -> *`,
`llm/prompt/persistence -> pipeline/engine`, `stage A -> stage B`,
`stage -> runtime`, `pipeline -> engine`.
