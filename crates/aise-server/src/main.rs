use aise_core::engine::HelloWorldEngine;
use aise_server::app::build_services;
use aise_server::observability::{ObservabilityConfig, ObservabilityRuntime, TelemetryDiagnostics};
use aise_server::session::SessionRegistry;
use aise_server::shutdown::wait_for_shutdown_signal;
use aise_server::tasks;
use aise_server::{AppState, ServerConfig, router};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::filter_fn;
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = ServerConfig::load()?;
    config.validate()?;

    std::fs::create_dir_all(&config.log_dir)?;
    let file_appender = tracing_appender::rolling::daily(&config.log_dir, "aise.log");
    let (file_writer, _file_guard) = tracing_appender::non_blocking(file_appender);
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let observability_load = ObservabilityConfig::load_from_env();
    let observation_config = observability_load.config.clone();
    let observation_issues = observability_load.issues.clone();
    let observability = ObservabilityRuntime::initialize(observability_load, TelemetryDiagnostics);
    let observation_runtime = observability.runtime;
    let observation_enabled = observation_runtime.is_enabled();
    let normal_logs = filter_fn(|metadata| metadata.target() != "aise::observation");

    tracing_subscriber::registry()
        .with(observability.layer)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stdout)
                .with_filter(normal_logs),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(file_writer)
                .with_ansi(false)
                .with_filter(filter_fn(|metadata| metadata.target() != "aise::observation")),
        )
        .with(filter)
        .init();
    for issue in observation_issues {
        TelemetryDiagnostics.configuration(
            issue.field,
            issue.error_kind,
            observation_config.endpoint_host().as_deref(),
        );
    }
    if observation_config.enabled && !observation_enabled {
        TelemetryDiagnostics.initialization("provider_unavailable", observation_config.endpoint_host().as_deref());
    }
    tracing::info!(
        target: "aise::telemetry",
        enabled = observation_enabled,
        environment = %observation_config.environment,
        release = %observation_config.release,
        sample_rate = observation_config.sample_rate,
        content_policy = ?observation_config.content_policy,
        queue_capacity = observation_config.max_queue_size,
        endpoint_host = observation_config.endpoint_host().as_deref().unwrap_or("unknown"),
        "OpenTelemetry configured"
    );

    let server_result = async {
        let services = build_services(&config).await?;
        let registry = SessionRegistry::new(config.max_sessions);
        let task_supervisor = tasks::TurnTaskSupervisor::new(config.turn_tasks())?;
        let state = Arc::new(
            AppState::new(
                services.engine,
                Arc::new(HelloWorldEngine),
                registry,
                task_supervisor.clone(),
                config.clone(),
            )
            .with_services(
                services.pack_service,
                services.character_card_service,
                services.instance_factory,
                services.story_history_reader,
                services.activation_preview,
            ),
        );
        let app = router(state, &config);

        let listener = tokio::net::TcpListener::bind(config.listen_addr).await?;
        tracing::info!(
            addr = %config.listen_addr,
            log_dir = %config.log_dir.display(),
            observation_enabled,
            "aise-server listening"
        );
        let server_shutdown = CancellationToken::new();
        let shutdown_signal = {
            let token = server_shutdown.clone();
            async move { token.cancelled().await }
        };
        let mut server = tokio::spawn(axum::serve(listener, app).with_graceful_shutdown(shutdown_signal).into_future());
        tokio::select! {
            result = &mut server => {
                match result {
                    Ok(Ok(())) => tracing::info!("http server stopped"),
                    Ok(Err(error)) => tracing::error!(error = %error, "http server failed"),
                    Err(error) => tracing::error!(error = %error, "http server task failed"),
                }
            }
            _ = wait_for_shutdown_signal() => {
                tracing::info!("shutdown signal received");
                if let Err(error) = task_supervisor.shutdown_with_grace().await {
                    tracing::warn!(error = %error, "turn task supervisor shutdown reported an error");
                }
                server_shutdown.cancel();
                match (&mut server).await {
                    Ok(Ok(())) => tracing::info!("http server stopped"),
                    Ok(Err(error)) => tracing::error!(error = %error, "http server failed"),
                    Err(error) => tracing::error!(error = %error, "http server task failed"),
                }
            }
        }
        if server.is_finished() {
            if let Err(error) = task_supervisor.shutdown_with_grace().await {
                tracing::warn!(error = %error, "turn task supervisor shutdown reported an error");
            }
        }
        Ok::<(), anyhow::Error>(())
    }
    .await;
    let shutdown_report = tokio::task::spawn_blocking(move || observation_runtime.shutdown_with_timeout()).await?;
    TelemetryDiagnostics.shutdown(&shutdown_report);
    server_result
}
