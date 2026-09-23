use super::*;
use std::collections::HashMap;

fn load(values: &[(&str, &str)]) -> ObservabilityConfigLoad {
    let values: HashMap<&str, &str> = values.iter().copied().collect();
    ObservabilityConfig::load_with(|name| values.get(name).map(|value| (*value).to_owned()))
}

fn enabled_values() -> Vec<(&'static str, &'static str)> {
    vec![
        ("LANGFUSE_TRACING_ENABLED", "true"),
        ("LANGFUSE_PUBLIC_KEY", "pk-test"),
        ("LANGFUSE_SECRET_KEY", "sk-test"),
    ]
}

#[test]
fn disabled_defaults_are_safe_and_environment_only() {
    let load = load(&[]);

    assert!(!load.config.enabled);
    assert_eq!(load.config.base_url, "https://cloud.langfuse.com");
    assert_eq!(load.config.environment, "development");
    assert_eq!(load.config.sample_rate, 1.0);
    assert_eq!(load.config.content_policy, ContentCapturePolicy::MetadataOnly);
    assert!(load.issues.is_empty());
    assert_eq!(load.config.endpoint(), None);
}

#[test]
fn enabled_configuration_derives_exact_endpoint() {
    let mut values = enabled_values();
    values.push(("LANGFUSE_BASE_URL", "https://langfuse.example/base///"));

    let load = load(&values);

    assert!(load.config.enabled);
    assert_eq!(
        load.config.endpoint().as_deref(),
        Some("https://langfuse.example/base/api/public/otel/v1/traces")
    );
    assert_eq!(load.config.endpoint_host().as_deref(), Some("langfuse.example"));
}

#[test]
fn missing_credentials_disable_tracing_without_error_result() {
    let load = load(&[("LANGFUSE_TRACING_ENABLED", "true")]);

    assert!(!load.config.enabled);
    assert!(load.issues.iter().any(|issue| issue.field == "LANGFUSE_PUBLIC_KEY"));
    assert!(load.issues.iter().any(|issue| issue.field == "LANGFUSE_SECRET_KEY"));
}

#[test]
fn invalid_url_disables_enabled_configuration() {
    let mut values = enabled_values();
    values.push(("LANGFUSE_BASE_URL", "file:///secret"));

    let load = load(&values);

    assert!(!load.config.enabled);
    assert!(load.issues.contains(&ObservabilityConfigIssue {
        field: "LANGFUSE_BASE_URL",
        error_kind: "invalid_url",
    }));
}

#[test]
fn invalid_sample_rate_disables_enabled_configuration() {
    let mut values = enabled_values();
    values.push(("LANGFUSE_SAMPLE_RATE", "1.1"));

    let load = load(&values);

    assert!(!load.config.enabled);
    assert!(load.issues.iter().any(|issue| issue.field == "LANGFUSE_SAMPLE_RATE"));
}

#[test]
fn batch_size_larger_than_queue_disables_enabled_configuration() {
    let mut values = enabled_values();
    values.extend([
        ("OTEL_BSP_MAX_QUEUE_SIZE", "8"),
        ("OTEL_BSP_MAX_EXPORT_BATCH_SIZE", "9"),
    ]);

    let load = load(&values);

    assert!(!load.config.enabled);
    assert!(load.issues.iter().any(|issue| issue.field == "OTEL_BSP_MAX_EXPORT_BATCH_SIZE"));
}

#[test]
fn http_timeout_must_be_less_than_shutdown_timeout() {
    let mut values = enabled_values();
    values.extend([
        ("AISE_TRACE_HTTP_TIMEOUT_MS", "5000"),
        ("AISE_TRACE_SHUTDOWN_TIMEOUT_MS", "5000"),
    ]);

    let load = load(&values);

    assert!(!load.config.enabled);
    assert!(
        load.issues
            .iter()
            .any(|issue| issue.error_kind == "insufficient_shutdown_budget")
    );
}

#[test]
fn production_full_content_downgrades_to_metadata_only() {
    let mut values = enabled_values();
    values.extend([
        ("LANGFUSE_TRACING_ENVIRONMENT", "production"),
        ("AISE_TRACE_CONTENT_POLICY", "full_content"),
        ("AISE_TRACE_FULL_CONTENT_ALLOWED", "true"),
    ]);

    let load = load(&values);

    assert!(!load.config.enabled);
    assert_eq!(load.config.content_policy, ContentCapturePolicy::MetadataOnly);
    assert!(load.issues.iter().any(|issue| issue.error_kind == "full_content_not_allowed"));
}

#[test]
fn development_full_content_requires_explicit_allowance() {
    let mut values = enabled_values();
    values.extend([
        ("AISE_TRACE_CONTENT_POLICY", "full_content"),
        ("AISE_TRACE_FULL_CONTENT_ALLOWED", "true"),
    ]);

    let load = load(&values);

    assert!(load.config.enabled);
    assert_eq!(load.config.content_policy, ContentCapturePolicy::FullContent);
    assert!(load.issues.is_empty());
}
