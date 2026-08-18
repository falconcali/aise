use super::*;
use crate::config::StructuredOutputMode;
use std::collections::BTreeSet;

fn capabilities(modes: &[StructuredOutputMode]) -> ProviderTransportCapabilities {
    ProviderTransportCapabilities {
        encodable_modes: modes.iter().copied().collect(),
    }
}

#[test]
fn resolution_prefers_native_json_schema_when_eligible() {
    let configured = vec![
        StructuredOutputMode::PromptFallback,
        StructuredOutputMode::NativeJsonSchema,
        StructuredOutputMode::JsonObject,
    ];
    let provider = capabilities(&[
        StructuredOutputMode::NativeJsonSchema,
        StructuredOutputMode::JsonObject,
        StructuredOutputMode::PromptFallback,
    ]);
    let mode = resolve_structured_output_mode(&configured, &provider).expect("mode resolves");
    assert_eq!(mode, StructuredOutputMode::NativeJsonSchema);
}

#[test]
fn resolution_falls_back_through_preference_order() {
    let configured = vec![StructuredOutputMode::JsonObject, StructuredOutputMode::PromptFallback];
    let provider = capabilities(&[StructuredOutputMode::PromptFallback]);
    let mode = resolve_structured_output_mode(&configured, &provider).expect("mode resolves");
    assert_eq!(mode, StructuredOutputMode::PromptFallback);
}

#[test]
fn resolution_fails_on_empty_intersection() {
    let configured = vec![StructuredOutputMode::NativeJsonSchema];
    let provider = capabilities(&[StructuredOutputMode::JsonObject]);
    assert_eq!(
        resolve_structured_output_mode(&configured, &provider),
        Err(StructuredOutputUnsupported)
    );
}

#[test]
fn canonical_schema_hash_is_stable_across_key_order() {
    let a = serde_json::json!({ "b": 1, "a": { "y": 2, "x": 1 } });
    let b = serde_json::json!({ "a": { "x": 1, "y": 2 }, "b": 1 });
    assert_eq!(canonical_schema_hash(&a), canonical_schema_hash(&b));
}

#[test]
fn canonical_schema_hash_differs_on_value_change() {
    let a = serde_json::json!({ "a": 1 });
    let b = serde_json::json!({ "a": 2 });
    assert_ne!(canonical_schema_hash(&a), canonical_schema_hash(&b));
}

#[test]
fn preference_order_covers_all_modes_without_duplicates() {
    let mut seen = BTreeSet::new();
    for mode in StructuredOutputMode::PREFERENCE_ORDER {
        assert!(seen.insert(mode));
    }
    assert_eq!(seen.len(), 4);
}
