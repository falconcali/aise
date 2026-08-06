use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub async fn wait_for_shutdown_signal() -> Option<()> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm = signal(SignalKind::terminate()).ok()?;
        let mut sigint = signal(SignalKind::interrupt()).ok()?;
        tokio::select! {
            _ = sigterm.recv() => Some(()),
            _ = sigint.recv() => Some(()),
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await.ok()?;
        Some(())
    }
}

pub async fn shutdown_all(server_shutdown: CancellationToken, task_shutdown: CancellationToken, grace: Duration) {
    server_shutdown.cancel();
    task_shutdown.cancel();
    tokio::time::sleep(grace).await;
}
