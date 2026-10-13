# Errors and Observability

## R-OBS-01 - Failure paths are diagnosable

**Level: MUST**

- Parse, LLM, tool, and I/O failures MUST emit a structured error event or
  return a diagnosable error before unwinding.
- Code MUST NOT silently drop errors.

```rust
// BAD
match analyze_intent(&ctx).await {
    Ok(intent) => intent,
    Err(_) => return,
}

// GOOD
match analyze_intent(&ctx).await {
    Ok(intent) => intent,
    Err(e) => {
        tracing::error!(stage = "intent", error = %e, "intent analysis failed");
        return Err(e.into());
    }
}
```

---

## R-OBS-02 - Explicit-parent observations

**Level: MUST**

- Turn work MUST be observed through `trace::Observation` with an explicitly
  passed parent; NEVER rely on ambient span context to find the parent.
- The observation tree is fixed by structure:
  - `PipelineRunner::run` creates one `Chain` observation per stage and
    captures the stage `Input` / `Output` (`pipeline_trace.rs`).
  - Each stage creates its sub-task `Span` observations through the
    `begin_*` / `finish_*` helpers in its own `{stage}_trace.rs`.
  - `LlmGateway::complete` creates the LLM generation observation
    (`llm_trace.rs`); `Prompt::render` creates the prompt-render observation.
- Every LLM call MUST receive the caller's `&Observation`; new LLM entry points
  and tools MUST NOT bypass observations.
- Pipelines MUST NOT create ad-hoc `tracing` spans; `tracing` spans are created
  only inside `trace::Observation`.
- Failed observations MUST carry an `ObservationError` with `code`,
  `failure_kind`, `stage`, and `message`.
- Observation content MUST go through `Observation::capture_content` so the
  configured capture policy applies.

```rust
// GOOD - stage span via {stage}_trace.rs, LLM call receives the parent
let plan_observation = plan_trace::begin_plan(observation, &input.player_contribution.processed);
let result = self.process_story_plan(story_ctx, &input, control, &plan_observation).await;
plan_trace::finish_plan(plan_observation, &result);
```

---

## R-OBS-03 - Non-fatal errors warn and continue

**Level: SHOULD**

- Non-fatal errors SHOULD use `warn!` and continue.
- Non-fatal errors SHOULD NOT abort the full turn or session.

```rust
// GOOD
if let Err(error) = store.pack_store().release(story_id).await {
    tracing::warn!(target: "aise::engine", story_id = %story_id, error = %error, "story asset release failed");
}
```

---

## R-OBS-04 - Structured log fields

**Level: MUST**

- Logs MUST use structured fields for identifiers and error data.
- Logs MUST NOT interpolate identifiers into message strings.

```rust
// BAD
tracing::info!("turn {} handled event for session {}", turn_id, session_id);

// GOOD
tracing::info!(
    session_id = %session_id,
    turn_id = %turn_id,
    "turn handled event"
);
```

---

## R-OBS-05 - Panic only for invariants

**Level: SHOULD**

- `panic!`, `unwrap`, and `expect` SHOULD be used only for broken invariants.
- Business errors MUST use `Result`.
- Every aise-core module MUST define typed errors with `thiserror`
  (`CoreError`, `NarrativeError`, `LlmError`, `PromptError`,
  `PersistenceError`, `PipelineError`, `EngineError`, ...) so callers can
  discriminate; NEVER leak `anyhow::Error` out of `aise-core`.
- `aise-service` MAY use `anyhow` to aggregate and report errors.

```rust
// BAD
let pack = packs.get(pack_ref).unwrap();

// GOOD
let pack = packs.get(pack_ref).cloned().ok_or(PersistenceError::NotFound)?;
```
