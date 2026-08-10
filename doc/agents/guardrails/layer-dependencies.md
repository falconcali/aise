# Layer Dependencies

General principle: dependencies flow one way, from outer transport/entry layers
toward inner domain/turn layers. Inner layers MUST NOT know about outer layers.

```text
transport / api / adapters
        |
        v
application / services
        |
        v
  turn contracts
        |
        +--+--+
        |     |
        v     v
     domain  config (leaf foundation)
```

`config` is a leaf foundation layer: it MUST NOT import any internal module,
and `turn` plus every upper layer MAY depend on it. `domain` is pure and
self-contained: it MUST NOT depend on `turn`, `config`, or any outer layer.
Turn data objects (`domain::turn`) live inside `domain`; the `turn` module only
defines Turn execution contracts.

## R-LAYER-01 - turn does not depend on the entry layer

**Level: MUST**

- turn/domain modules MUST NOT import transport, API, or adapter modules.
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

- `config` MUST NOT import any internal module (`turn`, `domain`, `runtime`,
  pipelines, `llm`, `persistence`, `engine`); it MAY depend only on external
  crates and std.
- `turn` and every upper layer MAY depend on `config` for typed limits, content
  policies, and settings.
- Configuration defaults MUST have one authoritative source in `config`;
  `turn`/pipelines MUST NOT keep a second copy of the same limits.

## R-LAYER-04 - domain is pure and self-contained

**Level: MUST**

- `domain` MUST NOT import `turn`, `config`, `runtime`, `llm`, `persistence`,
  `engine`, or any pipeline module; it MAY depend only on its own submodules.
- `domain` MUST NOT reference Turn-stage concepts (`TurnStage`, `TurnBudget`,
  `TurnExecutionContext`, `LlmGateway`, `Store`, ...).
- Cross-submodule reads inside `domain` (e.g. `narrative_graph::director` reading
  `story_instance::snapshot`) are read-only and MUST NOT carry I/O or write
  state.

## R-LAYER-05 - turn depends one-way on domain and config

**Level: MUST**

- `turn` MAY depend on `domain` (including `domain::turn` data objects),
  `config`, and turn-internal modules.
- `turn` MUST NOT depend on `runtime`, any specific pipeline, `llm`,
  `persistence`, or `engine`.
- `turn` is the single definition layer for Turn execution contracts; Turn data
  objects are owned by `domain::turn`.

## R-LAYER-06 - pipelines depend only on ports, not adapters

**Level: MUST**

- Pipelines MAY depend on `turn`, `domain`, `config`, `llm::gateway` (the only
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
| `domain` | its own submodules (incl. `turn` DTOs) | `turn`, `config`, `runtime`, pipelines, `llm`, `persistence`, `engine` |
| `config` | external crates / std | any internal module |
| `turn` | `domain`, `config`, turn-internal | `runtime`, pipelines, `llm`, `persistence`, `engine` |
| `runtime` | `turn`, `domain`, `config`, injected traits | concrete pipeline types, persistence adapters, `llm` providers |
| `llm` | restricted `turn` contracts, `config`, `prompt` | `runtime`, concrete pipelines, full `TurnExecutionContext`, `persistence` |
| pipelines | `turn`, `domain`, `config`, `llm::gateway`, persistence ports, `prompt` | other pipelines, persistence adapters, `runtime`, `engine`, `llm` internals |
| `prompt` | `turn`, `domain`, `config` | `runtime`, concrete pipelines, `persistence`, `llm` internals |
| persistence ports | `turn`, `domain`, `config`, persistence-internal | adapters, `runtime`, concrete pipelines |
| persistence adapters | persistence ports, `domain`, `turn`, `config` | reverse dependency from `turn`/`domain` |
| `engine` | `config`, `turn`, `runtime`, persistence ports | concrete pipelines, persistence adapters |
| aise-server (composition root) | everything | nothing (wires `TurnPipelineSetBuilder` once) |

Forbidden reverse dependencies: `turn -> runtime`,
`turn -> planning/story/validation/character/context`, `llm -> runtime`,
`pipeline A -> pipeline B`, `domain -> turn/config/runtime/adapter`.

`TurnCommitter` lives in `persistence/` but is a commit coordinator; database
connections, SQL, and transaction implementations belong to the Store adapter.
