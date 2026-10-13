# Concurrency and Locks

## R-CONC-01 - No write lock across await

**Level: MUST**

- NEVER hold a write guard (`RwLockWriteGuard`, `MutexGuard`) across `.await`.
- NEVER return a write guard from a function.
- MUST scope the lock to the smallest synchronous critical section, then drop it
  before any async work.

```rust
// BAD
let mut stories = self.stories.write().await;
let entry = stories.get_mut(&story_id).ok_or(PersistenceError::NotFound)?;
self.persist(entry).await?;

// GOOD
let snapshot = {
    let mut stories = self.stories.write().await;
    let entry = stories.get_mut(&story_id).ok_or(PersistenceError::NotFound)?;
    entry.apply(commit);
    entry.clone()
};
self.persist(&snapshot).await?;
```

---

## R-CONC-02 - Keep read locks short

**Level: SHOULD**

- SHOULD keep read locks short.
- SHOULD clone the needed data and release the lock before I/O, network calls,
  or `.await`.

```rust
// GOOD
let pack = {
    let packs = self.packs.read().await;
    packs.get(pack_ref).cloned()
};
if let Some(pack) = pack {
    render(&pack).await?;
}
```

---

## R-CONC-03 - No side effects under a write lock

**Level: MUST**

- NEVER emit `TurnEvent`s, send on channels, or perform I/O while holding a
  write lock.
- MUST apply the state mutation, drop the lock, then perform side effects.

```rust
// BAD
let mut stories = self.stories.write().await;
stories.apply(commit);
sink.emit(TurnEvent::Committed)?;

// GOOD
{
    let mut stories = self.stories.write().await;
    stories.apply(commit);
}
sink.emit(TurnEvent::Committed)?;
```

---

## R-CONC-04 - Route LLM calls through `LlmGateway` and its limiter

**Level: MUST**

- Every LLM call (completion, streaming, embedding) MUST go through the
  injected `llm::LlmGateway`.
- NEVER call an `LlmProvider` directly outside `LlmGateway`.
- `LlmGateway` MUST own a shared concurrency limiter (e.g. a
  `tokio::sync::Semaphore`) and acquire a permit before provider dispatch, so
  backpressure and provider rate limits stay centrally enforced.
- `LlmGateway` MUST be created once by the composition root
  (`engine::AiseFactory`) and shared as `Arc<LlmGateway>`; NEVER create a
  second gateway per pipeline or per Turn.
