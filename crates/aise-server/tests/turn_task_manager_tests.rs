use aise::AiseError;
use aise_server::tasks::TurnTaskManager;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

#[tokio::test]
async fn shutdown_waits_for_owned_turn_tasks() {
    let tasks = Arc::new(TurnTaskManager::new(8).unwrap());
    let completed = Arc::new(AtomicUsize::new(0));
    let (tx, rx) = oneshot::channel();
    let c = completed.clone();
    tasks
        .spawn(async move {
            let _ = rx.await;
            c.fetch_add(1, Ordering::SeqCst);
        })
        .await
        .unwrap();
    assert_eq!(tasks.active_turns().await, 1);

    let manager = tasks.clone();
    let shutdown = tokio::spawn(async move { manager.shutdown_with_grace(Duration::from_secs(5)).await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(tasks.active_turns().await, 1, "task keeps running during grace");

    tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), shutdown)
        .await
        .expect("shutdown completes after active task finishes")
        .unwrap();
    assert_eq!(completed.load(Ordering::SeqCst), 1, "owned task ran to completion");
    assert_eq!(tasks.active_turns().await, 0);
}

#[tokio::test]
async fn shutdown_aborts_tasks_exceeding_grace() {
    let tasks = Arc::new(TurnTaskManager::new(8).unwrap());
    let completed = Arc::new(AtomicUsize::new(0));
    let c = completed.clone();
    tasks
        .spawn(async move {
            std::future::pending::<()>().await;
            c.fetch_add(1, Ordering::SeqCst);
        })
        .await
        .unwrap();

    let start = Instant::now();
    tasks.shutdown_with_grace(Duration::from_millis(200)).await;
    assert!(start.elapsed() < Duration::from_secs(2), "grace is bounded");
    assert_eq!(completed.load(Ordering::SeqCst), 0, "stuck task aborted before completion");
    assert_eq!(tasks.active_turns().await, 0);
}

#[tokio::test]
async fn admission_limit_blocks_extra_turns() {
    let tasks = Arc::new(TurnTaskManager::new(1).unwrap());
    let (tx, rx) = oneshot::channel();
    tasks
        .spawn(async move {
            let _ = rx.await;
        })
        .await
        .unwrap();

    let manager = tasks.clone();
    let spawned = tokio::spawn(async move { manager.spawn(async move { std::future::pending::<()>().await }).await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(tasks.active_turns().await, 1, "second turn waits on admission");

    tx.send(()).unwrap();
    spawned.await.unwrap().expect("second turn admitted after first finishes");
}

#[tokio::test]
async fn shutdown_rejects_new_tasks_and_cancels_waiting() {
    let tasks = Arc::new(TurnTaskManager::new(1).unwrap());
    let (tx, rx) = oneshot::channel();
    tasks
        .spawn(async move {
            let _ = rx.await;
        })
        .await
        .unwrap();

    let manager = tasks.clone();
    let waiting = tokio::spawn(async move { manager.spawn(async move { std::future::pending::<()>().await }).await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(tasks.active_turns().await, 1);

    tasks.shutdown();
    let waiting_error = waiting.await.unwrap().expect_err("waiting spawn cancelled by shutdown");
    assert!(matches!(waiting_error, AiseError::Backpressure(_)));
    let new_error = tasks.spawn(async move {}).await.expect_err("new spawn rejected after shutdown");
    assert!(matches!(new_error, AiseError::Backpressure(_)));

    tx.send(()).unwrap();
}

#[test]
fn zero_admission_limit_is_rejected() {
    assert!(TurnTaskManager::new(0).is_err());
}
