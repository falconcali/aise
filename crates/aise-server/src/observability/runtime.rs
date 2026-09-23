use crate::observability::baggage_processor::TraceAttributePropagationProcessor;
use crate::observability::config::{ObservabilityConfig, ObservabilityConfigLoad};
use crate::observability::diagnostics::TelemetryDiagnostics;
use crate::observability::langfuse_exporter::LangfuseExportAdapter;
use crate::observability::propagation::StreamingMasker;
use aise::turn::observability::ContentCapturePolicy;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use opentelemetry::KeyValue;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::{Protocol, WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::{BatchConfigBuilder, BatchSpanProcessor, Sampler, SdkTracerProvider};
use std::collections::HashMap;
use std::time::Duration;
use tracing_subscriber::Layer as _;

pub const EXPORT_CHAIN: &str = "SdkTracerProvider->TraceAttributePropagationProcessor->BatchSpanProcessor->LangfuseExportAdapter->OTLP HTTP/protobuf";

pub type ObservationLayer = Box<dyn tracing_subscriber::Layer<tracing_subscriber::Registry> + Send + Sync>;

pub struct ObservabilityComponents {
    pub layer: Option<ObservationLayer>,
    pub runtime: ObservabilityRuntime,
}

pub struct ObservabilityRuntime {
    provider: Option<SdkTracerProvider>,
    shutdown_timeout: Duration,
}

impl ObservabilityRuntime {
    pub fn initialize(load: ObservabilityConfigLoad, diagnostics: TelemetryDiagnostics) -> ObservabilityComponents {
        let endpoint_host = load.config.endpoint_host();
        for issue in &load.issues {
            diagnostics.configuration(issue.field, issue.error_kind, endpoint_host.as_deref());
        }
        if !load.config.enabled {
            return disabled(load.config.shutdown_timeout_ms);
        }

        let config = load.config;
        let provider = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            build_provider(&config, diagnostics.clone())
        })) {
            Ok(Ok(provider)) => provider,
            Ok(Err(error_kind)) => {
                diagnostics.initialization(error_kind, endpoint_host.as_deref());
                return disabled(config.shutdown_timeout_ms);
            }
            Err(_) => {
                diagnostics.initialization("provider_build_panicked", endpoint_host.as_deref());
                return disabled(config.shutdown_timeout_ms);
            }
        };
        let tracer = provider.tracer("aise-server");
        let layer: ObservationLayer = tracing_opentelemetry::layer()
            .with_tracer(tracer)
            .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
                metadata.target() == "aise::observation"
            }))
            .boxed();

        tracing::info!(
            target: "aise::telemetry",
            enabled = true,
            environment = config.environment,
            release = config.release,
            sample_rate = config.sample_rate,
            content_policy = ?config.content_policy,
            queue_capacity = config.max_queue_size,
            endpoint_host = endpoint_host.as_deref().unwrap_or("unknown"),
            "OpenTelemetry initialized"
        );

        ObservabilityComponents {
            layer: Some(layer),
            runtime: Self {
                provider: Some(provider),
                shutdown_timeout: Duration::from_millis(config.shutdown_timeout_ms),
            },
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.provider.is_some()
    }

    pub fn shutdown_with_timeout(mut self) -> ShutdownReport {
        let report = match self.provider.take() {
            Some(provider) => match provider.shutdown_with_timeout(self.shutdown_timeout) {
                Ok(()) => ShutdownReport {
                    completed: true,
                    dropped_span_count: 0,
                    error_kind: None,
                },
                Err(_) => ShutdownReport {
                    completed: false,
                    dropped_span_count: 0,
                    error_kind: Some("shutdown_failed"),
                },
            },
            None => ShutdownReport {
                completed: true,
                dropped_span_count: 0,
                error_kind: None,
            },
        };
        report
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShutdownReport {
    pub completed: bool,
    pub dropped_span_count: u64,
    pub error_kind: Option<&'static str>,
}

fn disabled(shutdown_timeout_ms: u64) -> ObservabilityComponents {
    ObservabilityComponents {
        layer: None,
        runtime: ObservabilityRuntime {
            provider: None,
            shutdown_timeout: Duration::from_millis(shutdown_timeout_ms),
        },
    }
}

fn build_provider(
    config: &ObservabilityConfig,
    diagnostics: TelemetryDiagnostics,
) -> Result<SdkTracerProvider, &'static str> {
    let endpoint = config.endpoint().ok_or("missing_endpoint")?;
    let public_key = config.public_key.as_deref().ok_or("missing_public_key")?;
    let secret_key = config.secret_key.as_deref().ok_or("missing_secret_key")?;
    let endpoint_host = config.endpoint_host().unwrap_or_else(|| "unknown".into());
    let headers = langfuse_headers(public_key, secret_key);
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_endpoint(endpoint)
        .with_protocol(Protocol::HttpBinary)
        .with_timeout(Duration::from_millis(config.http_timeout_ms))
        .with_headers(headers)
        .build()
        .map_err(|_| "exporter_build_failed")?;
    let masker = StreamingMasker::new(
        config.max_field_bytes,
        matches!(config.content_policy, ContentCapturePolicy::RedactedContent),
    );
    let adapter = LangfuseExportAdapter::new(exporter, masker, diagnostics).with_endpoint_host(endpoint_host);
    let batch_config = BatchConfigBuilder::default()
        .with_max_queue_size(config.max_queue_size)
        .with_max_export_batch_size(config.max_export_batch_size)
        .with_scheduled_delay(Duration::from_millis(config.schedule_delay_ms))
        .build();
    let batch = BatchSpanProcessor::builder(adapter).with_batch_config(batch_config).build();
    let processor = TraceAttributePropagationProcessor::new(batch);
    let resource = Resource::builder_empty()
        .with_service_name("aise-server")
        .with_attributes([
            KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
            KeyValue::new("deployment.environment.name", config.environment.clone()),
            KeyValue::new("langfuse.release", config.release.clone()),
            KeyValue::new("langfuse.version", "1"),
        ])
        .build();
    Ok(SdkTracerProvider::builder()
        .with_sampler(Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(config.sample_rate))))
        .with_resource(resource)
        .with_span_processor(processor)
        .build())
}

fn langfuse_headers(public_key: &str, secret_key: &str) -> HashMap<String, String> {
    let authorization = STANDARD.encode(format!("{public_key}:{secret_key}"));
    HashMap::from([
        ("Authorization".into(), format!("Basic {authorization}")),
        ("x-langfuse-ingestion-version".into(), "4".into()),
    ])
}

#[cfg(test)]
#[path = "tests/runtime_tests.rs"]
mod tests;
