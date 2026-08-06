use crate::config::CoordinatorConfig;
use crate::core::turn_contract::TurnCancellation;
use crate::core::turn_error::{TurnExecutionError, TurnFailureKind};
use crate::domain::ids::StoryId;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
pub struct StoryTurnCoordinator {
    stories: Mutex<HashMap<StoryId, StoryEntry>>,
    max_waiters_per_story: usize,
    max_total_waiters: usize,
    idle_timeout: Duration,
    shutdown: CancellationToken,
    total_waiters: AtomicUsize,
    active_permits: AtomicUsize,
}

#[derive(Debug)]
struct StoryEntry {
    semaphore: Arc<Semaphore>,
    waiters: usize,
    idle_since: Option<Instant>,
}

impl StoryEntry {
    fn new() -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(1)),
            waiters: 0,
            idle_since: None,
        }
    }
}

#[derive(Debug)]
pub struct StoryPermit {
    inner: Option<OwnedSemaphorePermit>,
    story_id: StoryId,
    coordinator: Arc<StoryTurnCoordinator>,
}

impl Drop for StoryPermit {
    fn drop(&mut self) {
        self.inner.take();
        self.coordinator.release(&self.story_id);
    }
}

impl StoryTurnCoordinator {
    pub fn new(config: &CoordinatorConfig) -> Arc<Self> {
        Arc::new(Self {
            stories: Mutex::new(HashMap::new()),
            max_waiters_per_story: config.max_waiters_per_story,
            max_total_waiters: config.max_total_waiters,
            idle_timeout: Duration::from_secs(config.idle_timeout_secs),
            shutdown: CancellationToken::new(),
            total_waiters: AtomicUsize::new(0),
            active_permits: AtomicUsize::new(0),
        })
    }

    pub fn shutdown(&self) {
        self.shutdown.cancel();
    }

    pub fn active_permits(&self) -> usize {
        self.active_permits.load(Ordering::Relaxed)
    }

    pub fn entry_count(&self) -> usize {
        self.stories.lock().unwrap().len()
    }

    pub fn total_waiters(&self) -> usize {
        self.total_waiters.load(Ordering::Relaxed)
    }

    pub fn reclaim_idle(&self) {
        let mut map = self.stories.lock().unwrap();
        self.reclaim(&mut map, Instant::now());
    }

    pub async fn shutdown_with_grace(&self, grace: Duration) {
        self.shutdown.cancel();
        let deadline = Instant::now() + grace;
        while self.active_permits.load(Ordering::Relaxed) > 0 && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    pub async fn acquire(
        self: &Arc<Self>,
        story_id: &StoryId,
        deadline: Instant,
        cancellation: &TurnCancellation,
    ) -> Result<StoryPermit, TurnExecutionError> {
        if self.shutdown.is_cancelled() {
            return Err(backpressure("story coordinator is shutting down"));
        }
        let semaphore = {
            let mut map = self.stories.lock().unwrap();
            self.reclaim(&mut map, Instant::now());
            if self.total_waiters.load(Ordering::Relaxed) >= self.max_total_waiters {
                return Err(backpressure("story coordinator waiter capacity exceeded"));
            }
            let entry = map.entry(story_id.clone()).or_insert_with(StoryEntry::new);
            if entry.waiters >= self.max_waiters_per_story {
                return Err(backpressure("story waiter capacity exceeded"));
            }
            entry.waiters += 1;
            self.total_waiters.fetch_add(1, Ordering::Relaxed);
            entry.semaphore.clone()
        };

        let permit = self.wait_for_permit(semaphore, deadline, cancellation).await;

        {
            let mut map = self.stories.lock().unwrap();
            if let Some(entry) = map.get_mut(story_id) {
                entry.waiters = entry.waiters.saturating_sub(1);
                entry.idle_since = None;
            }
            self.total_waiters.fetch_sub(1, Ordering::Relaxed);
        }

        match permit {
            Ok(permit) => {
                self.active_permits.fetch_add(1, Ordering::Relaxed);
                Ok(StoryPermit {
                    inner: Some(permit),
                    story_id: story_id.clone(),
                    coordinator: self.clone(),
                })
            }
            Err(error) => Err(error),
        }
    }

    async fn wait_for_permit(
        &self,
        semaphore: Arc<Semaphore>,
        deadline: Instant,
        cancellation: &TurnCancellation,
    ) -> Result<OwnedSemaphorePermit, TurnExecutionError> {
        if cancellation.is_cancelled() {
            return Err(TurnExecutionError::cancelled(None));
        }
        if Instant::now() >= deadline {
            return Err(TurnExecutionError::deadline_exceeded(None));
        }
        tokio::select! {
            permit = semaphore.acquire_owned() => {
                permit.map_err(|_| invariant("story permit semaphore closed"))
            }
            _ = cancellation.token().cancelled() => Err(TurnExecutionError::cancelled(None)),
            _ = self.shutdown.cancelled() => Err(backpressure("story coordinator is shutting down")),
            _ = tokio::time::sleep_until(deadline.into()) => Err(TurnExecutionError::deadline_exceeded(None)),
        }
    }

    fn release(&self, story_id: &StoryId) {
        let now = Instant::now();
        self.active_permits.fetch_sub(1, Ordering::Relaxed);
        let mut map = self.stories.lock().unwrap();
        if let Some(entry) = map.get_mut(story_id) {
            if entry.waiters == 0 {
                entry.idle_since = Some(now);
            }
        }
        self.reclaim(&mut map, now);
    }

    fn reclaim(&self, map: &mut HashMap<StoryId, StoryEntry>, now: Instant) {
        map.retain(|_, entry| match entry.idle_since {
            Some(since) => now.saturating_duration_since(since) < self.idle_timeout,
            None => true,
        });
    }
}

fn backpressure(message: &'static str) -> TurnExecutionError {
    TurnExecutionError::new(TurnFailureKind::Backpressure, "backpressure", None, message)
}

fn invariant(message: &'static str) -> TurnExecutionError {
    TurnExecutionError::new(TurnFailureKind::InvariantViolation, "coordinator_invariant", None, message)
}
