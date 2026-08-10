# Layer Dependencies

General principle: dependencies flow one way, from outer transport/entry layers
toward inner domain/core layers. Inner layers MUST NOT know about outer layers.

```text
transport / api / adapters
        |
        v
application / services
        |
        v
   core contracts
        |
        +--+--+
        |     |
        v     v
     domain  config (leaf foundation)
```

`config` is a leaf foundation layer: it MUST NOT import any internal module,
and `core` plus every upper layer MAY depend on it. `domain` is pure and
self-contained: it MUST NOT depend on `core`, `config`, or any outer layer.

## R-LAYER-01 - Core does not depend on the entry layer

**Level: MUST**

- Core/domain modules MUST NOT import transport, API, or adapter modules.
- Cross-layer notifications MUST use injected traits, not concrete outer types.
- NEVER add a backedge from an inner layer to an outer layer.

## R-LAYER-02 - Reverse dependencies use traits and injection

**Level: SHOULD**

- Lower layers SHOULD notify upper layers only through injected trait objects or
  generic bounds.
- Lower layers MUST NOT import upper-layer concrete types for callbacks.
- Wiring of concrete implementations SHOULD happen once, at the composition
  root.

## R-LAYER-03 - config is a leaf foundation layer

**Level: MUST**

- `config` MUST NOT import any internal module (`core`, `domain`, `runtime`,
  pipelines, `llm`, `persistence`, `engine`); it MAY depend only on external
  crates and std.
- `core` and every upper layer MAY depend on `config` for typed limits, content
  policies, and settings.
- Configuration defaults MUST have one authoritative source in `config`;
  `core`/pipelines MUST NOT keep a second copy of the same limits.

## R-LAYER-04 - domain is pure and self-contained

**Level: MUST**

- `domain` MUST NOT import `core`, `config`, `runtime`, `llm`, `persistence`,
  `engine`, or any pipeline module; it MAY depend only on its own submodules.
- `domain` MUST NOT reference Turn-stage concepts (`TurnStage`, `TurnBudget`,
  `TurnExecutionContext`, `LlmGateway`, `Store`, ...).
- Cross-submodule reads inside `domain` (e.g. `narrative_graph::director` reading
  `story_instance::snapshot`) are read-only and MUST NOT carry I/O or write
  state.

## R-LAYER-05 - core depends one-way on domain and config

**Level: MUST**

- `core` MAY depend on `domain`, `config`, and core-internal modules.
- `core` MUST NOT depend on `runtime`, any specific pipeline, `llm`,
  `persistence`, or `engine`.
- `core` is the single definition layer for Turn contracts; domain types enter
  the Turn world only through `core`.

## R-LAYER-06 - pipelines depend only on ports, not adapters

**Level: MUST**

- Pipelines MAY depend on `core`, `domain`, `config`, `llm::gateway` (the only
  cross-cutting dependency), persistence ports (`store`, `asset_store`,
  `knowledge_read_port`) including their error types, and `prompt`.
- Pipelines MUST NOT import persistence adapters (`sqlite_*`) or other pipeline
  modules.
- `llm` MAY depend only on the restricted Turn LLM scope (`TurnLlmCallScope`,
  `turn_contract`, `turn_error`, `turn_trace`), `config`, and `prompt`; it MUST
  NOT import the full `TurnExecutionContext` or any concrete pipeline.

## Dependency matrix

| module | may depend on | must not depend on |
| --- | --- | --- |
| `domain` | its own submodules | `core`, `config`, `runtime`, pipelines, `llm`, `persistence`, `engine` |
| `config` | external crates / std | any internal module |
| `core` | `domain`, `config`, core-internal | `runtime`, pipelines, `llm`, `persistence`, `engine` |
| `runtime` | `core`, `domain`, `config`, injected traits | concrete pipeline types, persistence adapters, `llm` providers |
| `llm` | restricted `core` contracts, `config`, `prompt` | `runtime`, concrete pipelines, full `TurnExecutionContext`, `persistence` |
| pipelines | `core`, `domain`, `config`, `llm::gateway`, persistence ports, `prompt` | other pipelines, persistence adapters, `runtime`, `engine`, `llm` internals |
| `prompt` | `core`, `domain`, `config` | `runtime`, concrete pipelines, `persistence`, `llm` internals |
| persistence ports | `core`, `domain`, `config`, persistence-internal | adapters, `runtime`, concrete pipelines |
| persistence adapters | persistence ports, `domain`, `core`, `config` | reverse dependency from `core`/`domain` |
| `engine` | `config`, `core`, `runtime`, persistence ports | concrete pipelines, persistence adapters |
| aise-server (composition root) | everything | nothing (wires `TurnPipelineSetBuilder` once) |

Forbidden reverse dependencies: `core -> runtime`,
`core -> planning/story/validation/character/context`, `llm -> runtime`,
`pipeline A -> pipeline B`, `domain -> core/config/runtime/adapter`.

`TurnCommitter` lives in `persistence/` but is a commit coordinator; database
connections, SQL, and transaction implementations belong to the Store adapter.
