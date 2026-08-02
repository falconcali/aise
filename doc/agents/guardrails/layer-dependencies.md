# Layer Dependencies

General principle: dependencies flow one way, from outer transport/entry layers
toward inner domain/core layers. Inner layers MUST NOT know about outer layers.

```text
transport / api / adapters -> application / services -> core / domain
```

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
