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

#[test]
fn structured_output_config_defaults_to_prompt_fallback() {
    let config = StructuredOutputConfig::default();
    assert_eq!(config.default_modes, vec![StructuredOutputMode::PromptFallback]);
    config.validate().expect("default structured output config is valid");
}

#[test]
fn structured_output_config_rejects_empty_default_modes() {
    let config = StructuredOutputConfig {
        default_modes: Vec::new(),
        model_capabilities: Vec::new(),
    };
    assert!(config.validate().is_err());
}

#[test]
fn structured_output_config_rejects_duplicate_default_modes() {
    let config = StructuredOutputConfig {
        default_modes: vec![StructuredOutputMode::JsonObject, StructuredOutputMode::JsonObject],
        model_capabilities: Vec::new(),
    };
    assert!(config.validate().is_err());
}

#[test]
fn structured_output_config_rejects_duplicate_model_overrides() {
    let entry = ModelStructuredOutputCapabilities {
        provider: "openai_compat".into(),
        model: "gpt".into(),
        supported_modes: vec![StructuredOutputMode::JsonObject],
    };
    let config = StructuredOutputConfig {
        default_modes: vec![StructuredOutputMode::PromptFallback],
        model_capabilities: vec![entry.clone(), entry],
    };
    assert!(config.validate().is_err());
}

#[test]
fn structured_output_config_rejects_empty_override_provider_or_model() {
    let config = StructuredOutputConfig {
        default_modes: vec![StructuredOutputMode::PromptFallback],
        model_capabilities: vec![ModelStructuredOutputCapabilities {
            provider: String::new(),
            model: "gpt".into(),
            supported_modes: vec![StructuredOutputMode::JsonObject],
        }],
    };
    assert!(config.validate().is_err());
}

#[test]
fn structured_output_config_exact_override_replaces_default_modes() {
    let config = StructuredOutputConfig {
        default_modes: vec![StructuredOutputMode::PromptFallback],
        model_capabilities: vec![ModelStructuredOutputCapabilities {
            provider: "openai_compat".into(),
            model: "gpt-5".into(),
            supported_modes: vec![StructuredOutputMode::NativeJsonSchema],
        }],
    };
    assert_eq!(
        config.configured_modes("openai_compat", "gpt-5"),
        &[StructuredOutputMode::NativeJsonSchema]
    );
    assert_eq!(
        config.configured_modes("openai_compat", "other-model"),
        &[StructuredOutputMode::PromptFallback]
    );
}
