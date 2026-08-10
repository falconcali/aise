use aise::turn::turn_contract::TurnCancellation;
use aise::turn::turn_error::TurnExecutionError;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::{Semaphore, mpsc, oneshot};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct TurnTaskSupervisorConfig {
    pub max_active_turns: usize,
    pub admission_capacity: usize,
    pub admission_timeout_ms: u64,
    pub shutdown_grace_ms: u64,
}

impl TurnTaskSupervisorConfig {
    pub fn validate(&self) -> Result<(), TurnTaskError> {
        if self.max_active_turns == 0 {
            return Err(TurnTaskError::InvalidConfig("max_active_turns must be positive".into()));
        }
        if self.admission_capacity == 0 {
            return Err(TurnTaskError::InvalidConfig("admission_capacity must be positive".into()));
        }
        Ok(())
    }
}

impl Default for TurnTaskSupervisorConfig {
    fn default() -> Self {
        Self {
            max_active_turns: 8,
            admission_capacity: 64,
            admission_timeout_ms: 10_000,
            shutdown_grace_ms: 10_000,
        }
    }
}

#[derive(Debug, Error)]
pub enum TurnTaskError {
    #[error("turn task admission timed out after {0} ms")]
    AdmissionTimeout(u64),
    #[error("turn task supervisor is shutting down")]
    ShuttingDown,
    #[error("turn task supervisor is gone")]
    SupervisorGone,
    #[error("invalid turn task supervisor configuration: {0}")]
    InvalidConfig(String),
}

impl From<TurnTaskError> for TurnExecutionError {
    fn from(error: TurnTaskError) -> Self {
        match error {
            TurnTaskError::AdmissionTimeout(_) | TurnTaskError::ShuttingDown | TurnTaskError::SupervisorGone => {
                TurnExecutionError::backpressure(error.to_string())
            }
            TurnTaskError::InvalidConfig(message) => TurnExecutionError::invalid_request(message),
        }
    }
}

pub struct TurnTaskSpec {
    pub cancellation: TurnCancellation,
    pub future: Pin<Box<dyn Future<Output = ()> + Send>>,
}

enum TurnTaskCommand {
    Spawn {
        spec: TurnTaskSpec,
        reply: oneshot::Sender<Result<(), TurnTaskError>>,
    },
    Active(oneshot::Sender<usize>),
    Shutdown(oneshot::Sender<()>),
}

pub struct TurnTaskSupervisor {
    command_tx: mpsc::Sender<TurnTaskCommand>,
    service_cancellation: CancellationToken,
    admission_timeout: Duration,
    shutdown_started: AtomicBool,
    started: Arc<AtomicU64>,
    rejected: Arc<AtomicU64>,
}

impl TurnTaskSupervisor {
    pub fn new(config: TurnTaskSupervisorConfig) -> Result<Arc<Self>, TurnTaskError> {
        config.validate()?;
        let (command_tx, command_rx) = mpsc::channel(config.admission_capacity);
        let service_cancellation = CancellationToken::new();
        let started = Arc::new(AtomicU64::new(0));
        let rejected = Arc::new(AtomicU64::new(0));
        let handle = Arc::new(Self {
            command_tx,
            service_cancellation: service_cancellation.clone(),
            admission_timeout: Duration::from_millis(config.admission_timeout_ms),
            shutdown_started: AtomicBool::new(false),
            started: started.clone(),
            rejected: rejected.clone(),
        });
        let admission = Arc::new(Semaphore::new(config.max_active_turns));
        tokio::spawn(run_supervisor(
            command_rx,
            admission,
            Duration::from_millis(config.admission_timeout_ms),
            service_cancellation,
            Duration::from_millis(config.shutdown_grace_ms),
            started,
            rejected,
        ));
        Ok(handle)
    }

    pub async fn spawn(&self, spec: TurnTaskSpec) -> Result<(), TurnTaskError> {
        if self.service_cancellation.is_cancelled() {
            self.rejected.fetch_add(1, Ordering::Relaxed);
            return Err(TurnTaskError::ShuttingDown);
        }
        let (reply_tx, reply_rx) = oneshot::channel();
        let command = TurnTaskCommand::Spawn { spec, reply: reply_tx };
        match tokio::time::timeout(self.admission_timeout, self.command_tx.send(command)).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => return Err(TurnTaskError::SupervisorGone),
            Err(_) => {
                self.rejected.fetch_add(1, Ordering::Relaxed);
                return Err(TurnTaskError::AdmissionTimeout(self.admission_timeout.as_millis() as u64));
            }
        }
        match tokio::time::timeout(self.admission_timeout, reply_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(TurnTaskError::SupervisorGone),
            Err(_) => {
                self.rejected.fetch_add(1, Ordering::Relaxed);
                Err(TurnTaskError::AdmissionTimeout(self.admission_timeout.as_millis() as u64))
            }
        }
    }

    pub async fn active_turns(&self) -> usize {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self.command_tx.send(TurnTaskCommand::Active(reply_tx)).await.is_err() {
            return 0;
        }
        reply_rx.await.unwrap_or(0)
    }

    pub async fn shutdown_with_grace(&self) -> Result<(), TurnTaskError> {
        self.service_cancellation.cancel();
        if self.shutdown_started.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let (reply_tx, reply_rx) = oneshot::channel();
        match self.command_tx.send(TurnTaskCommand::Shutdown(reply_tx)).await {
            Ok(()) => reply_rx.await.map_err(|_| TurnTaskError::SupervisorGone),
            Err(_) => Err(TurnTaskError::SupervisorGone),
        }
    }

    pub fn shutdown(&self) {
        self.service_cancellation.cancel();
    }

    pub fn service_cancellation(&self) -> CancellationToken {
        self.service_cancellation.clone()
    }

    pub fn started(&self) -> u64 {
        self.started.load(Ordering::Relaxed)
    }

    pub fn rejected(&self) -> u64 {
        self.rejected.load(Ordering::Relaxed)
    }
}

async fn run_supervisor(
    mut command_rx: mpsc::Receiver<TurnTaskCommand>,
    admission: Arc<Semaphore>,
    admission_timeout: Duration,
    service_cancellation: CancellationToken,
    shutdown_grace: Duration,
    started: Arc<AtomicU64>,
    rejected: Arc<AtomicU64>,
) {
    let mut joinset = JoinSet::new();
    loop {
        tokio::select! {
            command = command_rx.recv() => {
                let Some(command) = command else { break };
                match command {
                    TurnTaskCommand::Spawn { spec, reply } => {
                        let permit = tokio::select! {
                            permit = admission.clone().acquire_owned() => match permit {
                                Ok(permit) => Some(permit),
                                Err(_) => {
                                    rejected.fetch_add(1, Ordering::Relaxed);
                                    let _ = reply.send(Err(TurnTaskError::SupervisorGone));
                                    continue;
                                }
                            },
                            _ = service_cancellation.cancelled() => {
                                rejected.fetch_add(1, Ordering::Relaxed);
                                let _ = reply.send(Err(TurnTaskError::ShuttingDown));
                                continue;
                            }
                            _ = tokio::time::sleep(admission_timeout) => {
                                rejected.fetch_add(1, Ordering::Relaxed);
                                let _ = reply.send(Err(TurnTaskError::AdmissionTimeout(admission_timeout.as_millis() as u64)));
                                continue;
                            }
                        };
                        let _ = reply.send(Ok(()));
                        started.fetch_add(1, Ordering::Relaxed);
                        let service = service_cancellation.clone();
                        joinset.spawn(async move {
                            let _permit = permit;
                            let mut future = spec.future;
                            let mut completed = false;
                            tokio::select! {
                                _ = &mut future => { completed = true; }
                                _ = service.cancelled() => { spec.cancellation.cancel(); }
                            }
                            if !completed {
                                let _ = (&mut future).await;
                            }
                        });
                    }
                    TurnTaskCommand::Active(reply) => {
                        while joinset.try_join_next().is_some() {}
                        let _ = reply.send(joinset.len());
                    }
                    TurnTaskCommand::Shutdown(reply) => {
                        let mut replies = shutdown_sequence(
                            &mut joinset,
                            &mut command_rx,
                            &admission,
                            shutdown_grace,
                            &rejected,
                        )
                        .await;
                        replies.push(reply);
                        for reply in replies {
                            let _ = reply.send(());
                        }
                        break;
                    }
                }
            }
            _ = service_cancellation.cancelled() => {
                let replies = shutdown_sequence(
                    &mut joinset,
                    &mut command_rx,
                    &admission,
                    shutdown_grace,
                    &rejected,
                )
                .await;
                for reply in replies {
                    let _ = reply.send(());
                }
                break;
            }
        }
    }
}

async fn shutdown_sequence(
    joinset: &mut JoinSet<()>,
    command_rx: &mut mpsc::Receiver<TurnTaskCommand>,
    admission: &Semaphore,
    shutdown_grace: Duration,
    rejected: &AtomicU64,
) -> Vec<oneshot::Sender<()>> {
    admission.close();
    let mut shutdown_replies = Vec::new();
    let deadline = Instant::now() + shutdown_grace;
    loop {
        while let Ok(command) = command_rx.try_recv() {
            match command {
                TurnTaskCommand::Spawn { reply, .. } => {
                    rejected.fetch_add(1, Ordering::Relaxed);
                    let _ = reply.send(Err(TurnTaskError::ShuttingDown));
                }
                TurnTaskCommand::Active(reply) => {
                    while joinset.try_join_next().is_some() {}
                    let _ = reply.send(joinset.len());
                }
                TurnTaskCommand::Shutdown(reply) => shutdown_replies.push(reply),
            }
        }
        while joinset.try_join_next().is_some() {}
        if joinset.is_empty() {
            break;
        }
        if Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    joinset.abort_all();
    while joinset.join_next().await.is_some() {}
    shutdown_replies
}
