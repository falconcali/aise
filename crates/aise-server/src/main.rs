use std::sync::Arc;

use aise_server::session::SessionRegistry;
use aise_server::{AppState, ServerConfig, build_engine, router};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = ServerConfig::load();

    // Logs go to stdout (debug) and a rolling file under `trace/` (git-ignored).
    std::fs::create_dir_all(&config.trace_dir)?;
    let file_appender = tracing_appender::rolling::daily(&config.trace_dir, "aise.log");
    let (file_writer, _file_guard) = tracing_appender::non_blocking(file_appender);
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stdout))
        .with(tracing_subscriber::fmt::layer().with_writer(file_writer).with_ansi(false))
        .with(filter)
        .init();

    let engine = build_engine(&config).await?;
    let registry = SessionRegistry::new(config.max_sessions);
    let state = Arc::new(AppState::new(engine, registry, config.clone()));
    let app = router(state, &config);

    let listener = tokio::net::TcpListener::bind(config.listen_addr).await?;
    tracing::info!(
        addr = %config.listen_addr,
        trace_dir = %config.trace_dir.display(),
        "aise-server listening"
    );
    axum::serve(listener, app).await?;
    Ok(())
}
