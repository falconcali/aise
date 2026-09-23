use super::*;

fn valid_enabled_config() -> ObservabilityConfig {
    ObservabilityConfig {
        enabled: true,
        public_key: Some("pk-test".into()),
        secret_key: Some("sk-test".into()),
        max_queue_size: 8,
        max_export_batch_size: 4,
        schedule_delay_ms: 60_000,
        http_timeout_ms: 10,
        shutdown_timeout_ms: 100,
        ..ObservabilityConfig::default()
    }
}

#[test]
fn disabled_configuration_creates_no_layer_or_provider() {
    let components = ObservabilityRuntime::initialize(
        ObservabilityConfigLoad {
            config: ObservabilityConfig::default(),
            issues: Vec::new(),
        },
        TelemetryDiagnostics,
    );

    assert!(components.layer.is_none());
    assert!(!components.runtime.is_enabled());
    assert!(components.runtime.shutdown_with_timeout().completed);
}

#[test]
fn invalid_enabled_configuration_fails_open() {
    let mut config = valid_enabled_config();
    config.enabled = false;
    let components = ObservabilityRuntime::initialize(
        ObservabilityConfigLoad {
            config,
            issues: vec![ObservabilityConfigIssue {
                field: "LANGFUSE_SECRET_KEY",
                error_kind: "missing_credential",
            }],
        },
        TelemetryDiagnostics,
    );

    assert!(components.layer.is_none());
    assert!(!components.runtime.is_enabled());
}

#[test]
fn enabled_runtime_builds_one_declared_export_chain() {
    assert_eq!(
        EXPORT_CHAIN,
        "SdkTracerProvider->TraceAttributePropagationProcessor->BatchSpanProcessor->LangfuseExportAdapter->OTLP HTTP/protobuf"
    );
    let components = ObservabilityRuntime::initialize(
        ObservabilityConfigLoad {
            config: valid_enabled_config(),
            issues: Vec::new(),
        },
        TelemetryDiagnostics,
    );

    assert!(components.layer.is_some());
    assert!(components.runtime.is_enabled());
    assert!(components.runtime.shutdown_with_timeout().completed);
}

#[test]
fn langfuse_headers_are_exact() {
    let headers = langfuse_headers("pk-test", "sk-test");

    assert_eq!(headers.len(), 2);
    assert_eq!(
        headers.get("Authorization").map(String::as_str),
        Some("Basic cGstdGVzdDpzay10ZXN0")
    );
    assert_eq!(headers.get("x-langfuse-ingestion-version").map(String::as_str), Some("4"));
}
