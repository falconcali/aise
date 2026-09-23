use super::*;
use aise::turn::observability::{
    ObservationFields, ObservationFinish, ObservationSpan, ObservationStatus, ObservationStep,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tracing_subscriber::prelude::*;

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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exporter_posts_protobuf_to_the_langfuse_endpoint() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let address = listener.local_addr().unwrap();
    let (request_tx, request_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            let read = socket.read(&mut buffer).await.unwrap();
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
            if request_body_is_complete(&request) {
                break;
            }
        }
        let _ = request_tx.send(request);
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
    });

    let mut config = valid_enabled_config();
    config.base_url = format!("http://{address}");
    config.http_timeout_ms = 1_000;
    config.shutdown_timeout_ms = 2_000;
    let components = ObservabilityRuntime::initialize(
        ObservabilityConfigLoad {
            config,
            issues: Vec::new(),
        },
        TelemetryDiagnostics,
    );
    let layer = components.layer.unwrap();
    let runtime = components.runtime;
    let subscriber = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        let span = ObservationSpan::begin(ObservationStep::ValidateRequest, ObservationFields::default());
        span.finish(ObservationFinish {
            status: ObservationStatus::Ok,
            ..ObservationFinish::default()
        });
    });

    let report = tokio::task::spawn_blocking(move || runtime.shutdown_with_timeout())
        .await
        .unwrap();
    assert!(report.completed);
    let request = tokio::time::timeout(Duration::from_secs(5), request_rx).await.unwrap().unwrap();
    let request_text = String::from_utf8_lossy(&request);
    assert!(request_text.starts_with("POST /api/public/otel/v1/traces HTTP/1.1"));
    assert!(request_text.contains("authorization: Basic cGstdGVzdDpzay10ZXN0"));
    assert!(request_text.contains("x-langfuse-ingestion-version: 4"));
    let header_end = request.windows(4).position(|window| window == b"\r\n\r\n").unwrap() + 4;
    assert!(request.len() > header_end);
    server.await.unwrap();
}

fn request_body_is_complete(request: &[u8]) -> bool {
    let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let headers = String::from_utf8_lossy(&request[..header_end]);
    let Some(content_length) = headers.lines().find_map(|line| {
        line.strip_prefix("Content-Length:")
            .or_else(|| line.strip_prefix("content-length:"))
            .and_then(|value| value.trim().parse::<usize>().ok())
    }) else {
        return false;
    };
    request.len() >= header_end + 4 + content_length
}
