use crate::domain::turn::extraction::{
    StoryStateExtractionEnvelopeOutput, StoryStateExtractionLimits, StoryStateExtractorOutput,
};

fn limits() -> StoryStateExtractionLimits {
    StoryStateExtractionLimits {
        max_role_states: 8,
        max_relationship_states: 8,
        max_knowledge_changes: 8,
        max_goals_per_role: 4,
        max_attributes_per_role: 8,
        max_entities_per_knowledge: 4,
        max_topics_per_knowledge: 4,
        max_item_bytes: 512,
        max_knowledge_change_bytes: 1024,
        max_condition_queries: 8,
        max_condition_evidence_bytes: 256,
        max_condition_reason_bytes: 256,
    }
}

#[test]
fn envelope_schema_declares_state_and_condition_judgments() {
    let schema = StoryStateExtractionEnvelopeOutput::json_schema(limits());
    let object = schema.as_object().expect("schema must be an object");
    let required = object
        .get("required")
        .and_then(|value| value.as_array())
        .expect("required list");
    assert!(required.iter().any(|value| value == "state"));
    assert!(required.iter().any(|value| value == "narrative_condition_judgments"));
}

#[test]
fn state_schema_declares_all_top_level_fields() {
    let schema = StoryStateExtractorOutput::json_schema(limits());
    let required = schema
        .get("required")
        .and_then(|value| value.as_array())
        .expect("required list");
    for field in ["role_states", "relationship_states", "knowledge_changes"] {
        assert!(required.iter().any(|value| value == field), "missing field {field}");
    }
    assert_eq!(required.len(), 3);
    let schema_text = schema.to_string();
    assert!(!schema_text.contains("character_states"));
    assert!(!schema_text.contains("character_id"));
    assert!(!schema_text.contains("role_key"));
}

#[test]
fn extractor_schema_has_no_current_scene() {
    let schema = StoryStateExtractorOutput::json_schema(limits());
    assert!(!schema.to_string().contains("current_scene"));
    let required = schema
        .get("required")
        .and_then(|value| value.as_array())
        .expect("required list");
    assert!(!required.iter().any(|value| value == "current_scene"));
}

#[test]
fn extractor_rejects_removed_current_scene_field() {
    let value = serde_json::json!({
        "role_states": [],
        "relationship_states": [],
        "knowledge_changes": [],
        "current_scene": {
            "scene_key": "scene_1",
            "location_key": "village",
            "time": "morning",
            "description": "The village wakes.",
            "present_role_ids": []
        }
    });
    let result: Result<StoryStateExtractorOutput, _> = serde_json::from_value(value);
    assert!(result.is_err(), "extractor output must reject a removed current_scene field");
}
