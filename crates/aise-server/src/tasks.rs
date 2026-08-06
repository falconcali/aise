use aise::AiseError;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

pub struct TurnTaskManager {
    admission: Arc<Semaphore>,
    tasks: tokio::sync::Mutex<JoinSet<()>>,
    shutdown: CancellationToken,
}

impl TurnTaskManager {
    pub fn new(max_concurrent_turns: usize) -> Result<Self, AiseError> {
        if max_concurrent_turns == 0 {
            return Err(AiseError::InvalidRequest("max_concurrent_turns must be positive".into()));
        }
        Ok(Self {
            admission: Arc::new(Semaphore::new(max_concurrent_turns)),
            tasks: tokio::sync::Mutex::new(JoinSet::new()),
            shutdown: CancellationToken::new(),
        })
    }

    pub fn shutdown(&self) {
        self.shutdown.cancel();
    }

    pub fn is_shutting_down(&self) -> bool {
        self.shutdown.is_cancelled()
    }

    pub async fn spawn<F>(&self, future: F) -> Result<(), AiseError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        if self.shutdown.is_cancelled() {
            return Err(AiseError::Backpressure("turn task manager is shutting down".into()));
        }
        let admission = self.admission.clone();
        let permit = tokio::select! {
            permit = admission.acquire_owned() => {
                permit.map_err(|_| AiseError::Internal("turn task admission semaphore closed".into()))?
            }
            _ = self.shutdown.cancelled() => {
                return Err(AiseError::Backpressure("turn task manager is shutting down".into()));
            }
        };
        let permit: tokio::sync::OwnedSemaphorePermit = permit;
        let mut tasks = self.tasks.lock().await;
        tasks.spawn(async move {
            let _permit = permit;
            future.await;
        });
        Ok(())
    }

    pub async fn active_turns(&self) -> usize {
        let mut tasks = self.tasks.lock().await;
        while tasks.try_join_next().is_some() {}
        tasks.len()
    }

    pub async fn shutdown_with_grace(&self, grace: Duration) {
        self.shutdown.cancel();
        let deadline = Instant::now() + grace;
        while self.active_turns().await > 0 && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let mut tasks = self.tasks.lock().await;
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
    }
}
