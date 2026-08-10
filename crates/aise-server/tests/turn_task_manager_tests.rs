use aise::turn::turn_contract::TurnCancellation;
use aise_server::tasks::{TurnTaskError, TurnTaskSpec, TurnTaskSupervisor, TurnTaskSupervisorConfig};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

fn config(max_active_turns: usize, admission_timeout_ms: u64, shutdown_grace_ms: u64) -> TurnTaskSupervisorConfig {
    TurnTaskSupervisorConfig {
        max_active_turns,
        admission_capacity: 16,
        admission_timeout_ms,
        shutdown_grace_ms,
    }
}

fn empty_task() -> TurnTaskSpec {
    TurnTaskSpec {
        cancellation: TurnCancellation::new(),
        future: Box::pin(std::future::pending::<()>()),
    }
}

#[tokio::test]
async fn shutdown_cancels_waiters_and_waits_for_owned_turns() {
    let tasks = TurnTaskSupervisor::new(config(8, 5_000, 5_000)).unwrap();
    let completed = Arc::new(AtomicUsize::new(0));
    let (tx, rx) = oneshot::channel();
    let c = completed.clone();
    tasks
        .spawn(TurnTaskSpec {
            cancellation: TurnCancellation::new(),
            future: Box::pin(async move {
                let _ = rx.await;
                c.fetch_add(1, Ordering::SeqCst);
            }),
        })
        .await
        .unwrap();
    assert_eq!(tasks.active_turns().await, 1);

    let manager = tasks.clone();
    let shutdown = tokio::spawn(async move { manager.shutdown_with_grace().await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(tasks.active_turns().await, 1, "task keeps running during grace");

    tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), shutdown)
        .await
        .expect("shutdown completes after active task finishes")
        .expect("shutdown task completed without panic")
        .expect("supervisor shutdown succeeded");
    assert_eq!(completed.load(Ordering::SeqCst), 1, "owned task ran to completion");
    assert_eq!(tasks.active_turns().await, 0);
}

#[tokio::test]
async fn shutdown_aborts_only_after_grace_and_joins_all_tasks() {
    let tasks = TurnTaskSupervisor::new(config(8, 1_000, 200)).unwrap();
    let completed = Arc::new(AtomicUsize::new(0));
    let c = completed.clone();
    tasks
        .spawn(TurnTaskSpec {
            cancellation: TurnCancellation::new(),
            future: Box::pin(async move {
                std::future::pending::<()>().await;
                c.fetch_add(1, Ordering::SeqCst);
            }),
        })
        .await
        .unwrap();

    let start = Instant::now();
    tasks.shutdown_with_grace().await.unwrap();
    assert!(start.elapsed() < Duration::from_secs(2), "grace is bounded");
    assert_eq!(completed.load(Ordering::SeqCst), 0, "stuck task aborted before completion");
    assert_eq!(tasks.active_turns().await, 0);
}

#[tokio::test]
async fn task_admission_rejects_over_capacity_without_unbounded_wait() {
    let tasks = TurnTaskSupervisor::new(config(1, 200, 1_000)).unwrap();
    let (tx, rx) = oneshot::channel();
    tasks
        .spawn(TurnTaskSpec {
            cancellation: TurnCancellation::new(),
            future: Box::pin(async move {
                let _ = rx.await;
            }),
        })
        .await
        .unwrap();

    let start = Instant::now();
    let error = tasks
        .spawn(empty_task())
        .await
        .expect_err("second turn rejected by bounded admission timeout");
    assert!(start.elapsed() < Duration::from_secs(2), "admission wait is bounded");
    assert!(
        matches!(error, TurnTaskError::AdmissionTimeout(_)),
        "second turn rejected with admission timeout"
    );

    tx.send(()).unwrap();
}

#[tokio::test]
async fn shutdown_rejects_new_tasks_and_cancels_waiting() {
    let tasks = TurnTaskSupervisor::new(config(1, 5_000, 1_000)).unwrap();
    let (tx, rx) = oneshot::channel();
    tasks
        .spawn(TurnTaskSpec {
            cancellation: TurnCancellation::new(),
            future: Box::pin(async move {
                let _ = rx.await;
            }),
        })
        .await
        .unwrap();

    let manager = tasks.clone();
    let waiting = tokio::spawn(async move { manager.spawn(empty_task()).await });
    tokio::time::sleep(Duration::from_millis(100)).await;

    tasks.shutdown();
    let waiting_error = waiting.await.unwrap().expect_err("waiting spawn cancelled by shutdown");
    assert!(matches!(waiting_error, TurnTaskError::ShuttingDown));
    let new_error = tasks.spawn(empty_task()).await.expect_err("new spawn rejected after shutdown");
    assert!(matches!(new_error, TurnTaskError::ShuttingDown));

    tx.send(()).unwrap();
}

#[tokio::test]
async fn service_shutdown_reaches_running_turn_cancellation() {
    let tasks = TurnTaskSupervisor::new(config(8, 5_000, 5_000)).unwrap();
    let cancellation = TurnCancellation::new();
    let (entered_tx, entered_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    tasks
        .spawn(TurnTaskSpec {
            cancellation: cancellation.clone(),
            future: Box::pin(async move {
                let _ = entered_tx.send(());
                let _ = release_rx.await;
            }),
        })
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), entered_rx)
        .await
        .expect("task started")
        .expect("task entered");
    assert!(!cancellation.is_cancelled());

    tasks.shutdown();

    let token = cancellation.clone();
    tokio::time::timeout(Duration::from_secs(2), async {
        while !token.is_cancelled() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("service shutdown reaches the running turn cancellation");
    assert!(cancellation.is_cancelled());

    let _ = release_tx.send(());
}

#[test]
fn zero_admission_limit_is_rejected() {
    assert!(TurnTaskSupervisor::new(config(0, 1_000, 1_000)).is_err());
}
