use super::*;

#[test]
fn metadata_only_skips_serialization() {
    let encoder = BoundedContentEncoder::new(
        ContentCapturePolicy::MetadataOnly,
        ContentCaptureLimits {
            max_field_bytes: 16,
            max_observation_bytes: 16,
            detector_overlap_bytes: 4,
        },
    );

    assert_eq!(encoder.encode(&"secret", 16), None);
}

#[test]
fn encoder_keeps_a_bounded_utf8_prefix() {
    let encoder = BoundedContentEncoder::new(
        ContentCapturePolicy::RedactedContent,
        ContentCaptureLimits {
            max_field_bytes: 5,
            max_observation_bytes: 5,
            detector_overlap_bytes: 0,
        },
    );

    let content = encoder.encode(&"世界", 5).unwrap();

    assert!(content.json.is_char_boundary(content.json.len()));
    assert!(content.captured_bytes <= 5);
    assert!(content.truncated);
    assert_eq!(content.captured_bytes, content.json.len());
}

#[test]
fn encoder_hashes_the_complete_serialized_stream() {
    let limits = ContentCaptureLimits {
        max_field_bytes: 4,
        max_observation_bytes: 4,
        detector_overlap_bytes: 0,
    };
    let bounded = BoundedContentEncoder::new(ContentCapturePolicy::FullContent, limits.clone());
    let complete = BoundedContentEncoder::new(
        ContentCapturePolicy::FullContent,
        ContentCaptureLimits {
            max_field_bytes: 1024,
            max_observation_bytes: 1024,
            detector_overlap_bytes: 0,
        },
    );

    let bounded_content = bounded.encode(&vec!["alpha", "beta"], 4).unwrap();
    let complete_content = complete.encode(&vec!["alpha", "beta"], 1024).unwrap();

    assert_eq!(bounded_content.sha256, complete_content.sha256);
    assert_eq!(bounded_content.original_bytes, complete_content.original_bytes);
    assert!(bounded_content.truncated);
    assert!(!complete_content.truncated);
}

#[test]
fn encoder_enforces_the_observation_limit() {
    let encoder = BoundedContentEncoder::new(
        ContentCapturePolicy::FullContent,
        ContentCaptureLimits {
            max_field_bytes: 100,
            max_observation_bytes: 3,
            detector_overlap_bytes: 2,
        },
    );

    let content = encoder.encode(&"abcdef", usize::MAX).unwrap();

    assert!(content.captured_bytes <= 5);
}

#[test]
fn generation_usage_requires_mutually_exclusive_total() {
    let valid = GenerationUsage {
        input: 3,
        input_cached_tokens: 2,
        output: 4,
        output_reasoning_tokens: 1,
        total: 10,
    };
    let invalid = GenerationUsage {
        total: 11,
        ..valid.clone()
    };

    assert!(valid.is_valid());
    assert!(!invalid.is_valid());
}

#[test]
fn generation_cost_requires_usd_and_non_negative_finite_values() {
    let valid = GenerationCost {
        currency: "USD",
        input: Some(0.1),
        input_cached_tokens: None,
        output: Some(0.2),
        output_reasoning_tokens: None,
        total: 0.3,
    };
    let wrong_currency = GenerationCost {
        currency: "CNY",
        ..valid.clone()
    };
    let invalid_total = GenerationCost {
        total: f64::NAN,
        ..valid.clone()
    };

    assert!(valid.is_exportable());
    assert!(!wrong_currency.is_exportable());
    assert!(!invalid_total.is_exportable());
}
