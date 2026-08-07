use aise::core::turn_trace::TraceSpanSink;
use aise_server::app::build_services;
use aise_server::session::SessionRegistry;
use aise_server::shutdown::wait_for_shutdown_signal;
use aise_server::tasks;
use aise_server::{AppState, ServerConfig, new_trace_writer, router};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = ServerConfig::load()?;
    config.validate()?;

    std::fs::create_dir_all(&config.trace_dir)?;
    let file_appender = tracing_appender::rolling::daily(&config.trace_dir, "aise.log");
    let (file_writer, _file_guard) = tracing_appender::non_blocking(file_appender);
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stdout))
        .with(tracing_subscriber::fmt::layer().with_writer(file_writer).with_ansi(false))
        .with(filter)
        .init();

    let trace_writer = new_trace_writer(&config)?;
    let trace_sink: Arc<dyn TraceSpanSink> = trace_writer.clone();
    let services = build_services(&config, trace_sink).await?;
    let registry = SessionRegistry::new(config.max_sessions);
    let task_supervisor = tasks::TurnTaskSupervisor::new(config.turn_tasks())?;
    let state = Arc::new(
        AppState::new(services.engine, registry, task_supervisor.clone(), config.clone())
            .with_services(services.pack_service, services.instance_factory),
    );
    let app = router(state, &config);

    let listener = tokio::net::TcpListener::bind(config.listen_addr).await?;
    tracing::info!(
        addr = %config.listen_addr,
        trace_dir = %config.trace_dir.display(),
        "aise-server listening"
    );
    let server_shutdown = CancellationToken::new();
    let shutdown_signal = {
        let token = server_shutdown.clone();
        async move { token.cancelled().await }
    };
    let server = tokio::spawn(axum::serve(listener, app).with_graceful_shutdown(shutdown_signal).into_future());
    tokio::select! {
        result = server => {
            match result {
                Ok(Ok(())) => tracing::info!("http server stopped"),
                Ok(Err(error)) => tracing::error!(error = %error, "http server failed"),
                Err(error) => tracing::error!(error = %error, "http server task failed"),
            }
        }
        _ = wait_for_shutdown_signal() => {
            tracing::info!("shutdown signal received; draining turn tasks");
            if let Err(error) = task_supervisor.shutdown_with_grace().await {
                tracing::warn!(error = %error, "turn task supervisor shutdown reported an error");
            }
            trace_writer.shutdown_with_grace().await;
            server_shutdown.cancel();
            let _ = server_shutdown.cancelled().await;
        }
    }
    Ok(())
}
