use super::{ObservabilityConfig, ObservabilityRuntime, TelemetryDiagnostics};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::filter_fn;
use tracing_subscriber::prelude::*;

pub struct ObservabilityBootstrap {
    config: ObservabilityConfig,
    runtime: ObservabilityRuntime,
}

impl ObservabilityBootstrap {
    pub fn initialize() -> Self {
        let load = ObservabilityConfig::load_from_env();
        let config = load.config.clone();
        let issues = load.issues.clone();
        let components = ObservabilityRuntime::initialize(load, TelemetryDiagnostics);
        let enabled = components.runtime.is_enabled();
        let normal_logs = filter_fn(|metadata| metadata.target() != "aise::observation");
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

        tracing_subscriber::registry()
            .with(components.layer)
            .with(tracing_subscriber::fmt::layer().with_filter(normal_logs))
            .with(filter)
            .init();

        for issue in issues {
            TelemetryDiagnostics.configuration(issue.field, issue.error_kind, config.endpoint_host().as_deref());
        }
        if config.enabled && !enabled {
            TelemetryDiagnostics.initialization("provider_unavailable", config.endpoint_host().as_deref());
        }
        tracing::info!(
            target: "aise::telemetry",
            enabled,
            environment = %config.environment,
            release = %config.release,
            sample_rate = config.sample_rate,
            content_policy = ?config.content_policy,
            queue_capacity = config.max_queue_size,
            endpoint_host = config.endpoint_host().as_deref().unwrap_or("unknown"),
            "OpenTelemetry configured"
        );

        Self {
            config,
            runtime: components.runtime,
        }
    }

    pub fn config(&self) -> &ObservabilityConfig {
        &self.config
    }

    pub async fn shutdown(self) -> Result<(), tokio::task::JoinError> {
        let report = tokio::task::spawn_blocking(move || self.runtime.shutdown_with_timeout()).await?;
        TelemetryDiagnostics.shutdown(&report);
        Ok(())
    }
}
