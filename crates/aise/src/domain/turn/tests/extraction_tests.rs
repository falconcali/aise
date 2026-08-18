use crate::domain::turn::extraction::{StoryStateExtractionDto, StoryStateExtractionLimits};

fn limits() -> StoryStateExtractionLimits {
    StoryStateExtractionLimits {
        max_new_roles: 4,
        max_role_states: 8,
        max_relationship_states: 8,
        max_knowledge_items: 8,
        max_goals_per_role: 4,
        max_attributes_per_role: 8,
        max_item_bytes: 512,
        max_role_profile_bytes: 512,
        max_knowledge_change_bytes: 1024,
        max_cast_policy_violations: 4,
        max_condition_queries: 8,
        max_condition_evidence_bytes: 256,
        max_condition_reason_bytes: 256,
    }
}

fn empty_dto_json() -> serde_json::Value {
    serde_json::json!({
        "new_roles": [],
        "role_states": [],
        "relationship_states": [],
        "add_facts": [],
        "update_facts": [],
        "add_rumors": [],
        "update_rumors": [],
        "delete_rumor_ids": [],
        "add_memories": [],
        "update_memories": [],
        "delete_memory_ids": [],
        "narrative_condition_judgments": [],
        "cast_policy_violations": []
    })
}

#[test]
fn schema_declares_all_top_level_fields() {
    let schema = StoryStateExtractionDto::json_schema(limits());
    let required = schema
        .get("required")
        .and_then(|value| value.as_array())
        .expect("required list");
    for field in [
        "new_roles",
        "role_states",
        "relationship_states",
        "add_facts",
        "update_facts",
        "add_rumors",
        "update_rumors",
        "delete_rumor_ids",
        "add_memories",
        "update_memories",
        "delete_memory_ids",
        "narrative_condition_judgments",
        "cast_policy_violations",
    ] {
        assert!(required.iter().any(|value| value == field), "missing field {field}");
    }
    assert_eq!(required.len(), 13);
}

#[test]
fn schema_omits_schema_uri_and_disallows_extra_fields() {
    let schema = StoryStateExtractionDto::json_schema(limits());
    assert!(schema.get("$schema").is_none());
    assert_eq!(schema["additionalProperties"], false);
}

#[test]
fn schema_has_no_oneof_mutation_union() {
    let schema = StoryStateExtractionDto::json_schema(limits());
    assert!(!schema.to_string().contains("oneOf"));
}

#[test]
fn schema_fits_within_the_compact_contract_budget() {
    let schema = StoryStateExtractionDto::json_schema(limits());
    assert!(schema.to_string().len() <= 6144, "schema exceeds the compact contract budget");
    assert!(
        StoryStateExtractionDto::compact_prompt_shape().len() <= 1536,
        "compact prompt shape exceeds its budget"
    );
}

#[test]
fn dto_deserializes_the_empty_envelope() {
    let dto: StoryStateExtractionDto = serde_json::from_value(empty_dto_json()).expect("empty dto decodes");
    assert!(dto.new_roles.is_empty());
    assert!(dto.cast_policy_violations.is_empty());
}

#[test]
fn dto_rejects_unknown_top_level_fields() {
    let mut value = empty_dto_json();
    value
        .as_object_mut()
        .unwrap()
        .insert("current_scene".into(), serde_json::json!({}));
    let result: Result<StoryStateExtractionDto, _> = serde_json::from_value(value);
    assert!(result.is_err(), "extractor dto must reject a removed current_scene field");
}

#[test]
fn dto_rejects_unknown_fields_on_new_role() {
    let mut value = empty_dto_json();
    value["new_roles"] = serde_json::json!([{
        "role_id": "role_0001",
        "name": "n",
        "role_label": "",
        "narrative_function": "f",
        "background": "",
        "appearance": "",
        "personality": "",
        "speaking_style": "",
        "location": "loc",
        "goals": [],
        "attributes": {},
        "character_id": "npc-1"
    }]);
    let result: Result<StoryStateExtractionDto, _> = serde_json::from_value(value);
    assert!(result.is_err());
}

#[test]
fn dto_rejects_salience_or_topics_supplied_by_the_model() {
    let mut value = empty_dto_json();
    value["add_facts"] = serde_json::json!([{
        "content": "the bridge collapsed",
        "retrieval_hint": "bridge",
        "salience": 200
    }]);
    let result: Result<StoryStateExtractionDto, _> = serde_json::from_value(value);
    assert!(result.is_err(), "add_facts must not accept a model-supplied salience field");
}
