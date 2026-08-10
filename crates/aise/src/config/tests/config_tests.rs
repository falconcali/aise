use super::*;

#[test]
fn content_recording_permitted_only_in_development() {
    assert!(content_recording_allowed(Some("development")));
    assert!(!content_recording_allowed(Some("production")));
    assert!(!content_recording_allowed(Some("staging")));
    assert!(!content_recording_allowed(None));
}

#[test]
fn redacted_content_policy_requires_development_environment() {
    let config = LlmConfig {
        base_url: "http://localhost:8000".into(),
        model: "test-model".into(),
        trace_content: TraceContentPolicy::RedactedContent,
        ..LlmConfig::default()
    };
    let env = std::env::var("AISE_ENV").ok();
    if env.as_deref() == Some("development") {
        config.validate().expect("development permits redacted content");
    } else {
        assert!(
            config.validate().is_err(),
            "redacted content must fail validate outside development"
        );
    }
}

#[test]
fn full_content_policy_requires_development_environment() {
    let config = LlmConfig {
        base_url: "http://localhost:8000".into(),
        model: "test-model".into(),
        trace_content: TraceContentPolicy::FullContent,
        ..LlmConfig::default()
    };
    let env = std::env::var("AISE_ENV").ok();
    if env.as_deref() == Some("development") {
        config.validate().expect("development permits full content");
    } else {
        assert!(
            config.validate().is_err(),
            "full content must fail validate outside development"
        );
    }
}

#[test]
fn metadata_only_policy_validates_in_any_environment() {
    let config = LlmConfig {
        base_url: "http://localhost:8000".into(),
        model: "test-model".into(),
        trace_content: TraceContentPolicy::MetadataOnly,
        ..LlmConfig::default()
    };
    config.validate().expect("metadata only is always permitted");
}
