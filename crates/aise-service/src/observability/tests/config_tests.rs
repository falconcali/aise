use super::*;
use std::collections::HashMap;

fn load(values: &[(&str, &str)]) -> ObservabilityConfigLoad {
    let values: HashMap<&str, &str> = values.iter().copied().collect();
    ObservabilityConfig::load_with(|name| values.get(name).map(|value| (*value).to_owned()))
}

#[test]
fn enabled_configuration_uses_langfuse_endpoint_and_content_limits() {
    let load = load(&[
        ("LANGFUSE_TRACING_ENABLED", "true"),
        ("LANGFUSE_BASE_URL", "https://langfuse.example/base/"),
        ("LANGFUSE_PUBLIC_KEY", "pk-test"),
        ("LANGFUSE_SECRET_KEY", "sk-test"),
        ("AISE_TRACE_CONTENT_POLICY", "redacted_content"),
        ("AISE_TRACE_MAX_FIELD_BYTES", "1024"),
        ("AISE_TRACE_MAX_OBSERVATION_BYTES", "2048"),
    ]);

    assert!(load.config.enabled);
    assert_eq!(
        load.config.endpoint().as_deref(),
        Some("https://langfuse.example/base/api/public/otel/v1/traces")
    );
    assert_eq!(load.config.content_policy, ContentCapturePolicy::RedactedContent);
    assert_eq!(load.config.max_field_bytes, 1024);
    assert_eq!(load.config.max_observation_bytes, 2048);
    assert!(load.issues.is_empty());
}

#[test]
fn invalid_enabled_configuration_fails_open() {
    let load = load(&[("LANGFUSE_TRACING_ENABLED", "true")]);

    assert!(!load.config.enabled);
    assert!(load.issues.iter().any(|issue| issue.field == "LANGFUSE_PUBLIC_KEY"));
    assert!(load.issues.iter().any(|issue| issue.field == "LANGFUSE_SECRET_KEY"));
}
