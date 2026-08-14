use super::*;
use crate::domain::asset::ids::SceneKey;

fn limits() -> StoryStateExtractionLimits {
    StoryStateExtractionLimits {
        max_character_states: 8,
        max_relationship_states: 16,
        max_knowledge_changes: 16,
        max_goals_per_character: 4,
        max_attributes_per_character: 8,
        max_entities_per_knowledge: 8,
        max_topics_per_knowledge: 4,
        max_item_bytes: 512,
        max_knowledge_change_bytes: 1024,
    }
}

fn current_scene() -> CurrentScene {
    CurrentScene {
        scene_key: SceneKey::from("scene_1"),
        location_key: LocationKey::from("village"),
        time: BoundedText::try_new("morning", "time", 128).unwrap(),
        description: BoundedText::try_new("scene", "description", 512).unwrap(),
        present_character_ids: Vec::new(),
    }
}

#[test]
fn json_schema_reports_configured_array_bounds() {
    let schema = StoryStateExtractorOutput::json_schema(limits());
    assert_eq!(schema["properties"]["character_states"]["maxItems"].as_u64().unwrap(), 8);
    assert_eq!(schema["properties"]["relationship_states"]["maxItems"].as_u64().unwrap(), 16);
    assert_eq!(schema["properties"]["knowledge_changes"]["maxItems"].as_u64().unwrap(), 16);
}

#[test]
fn output_rejects_unknown_fields() {
    let raw = serde_json::json!({
        "character_states": [],
        "relationship_states": [],
        "knowledge_changes": [],
        "current_scene": {
            "scene_key": "scene_1",
            "location_key": "village",
            "time": "morning",
            "description": "scene",
            "present_character_ids": []
        },
        "extra": true
    });
    let result: Result<StoryStateExtractorOutput, _> = serde_json::from_value(raw);
    assert!(result.is_err());
}

#[test]
fn output_round_trips_through_json() {
    let output = StoryStateExtractorOutput {
        character_states: Vec::new(),
        relationship_states: Vec::new(),
        knowledge_changes: Vec::new(),
        current_scene: current_scene(),
    };
    let raw = serde_json::to_value(&output).unwrap();
    let decoded: StoryStateExtractorOutput = serde_json::from_value(raw).unwrap();
    assert_eq!(decoded, output);
}

#[test]
fn deletable_knowledge_id_distinguishes_rumor_and_memory() {
    let rumor = DeletableKnowledgeId::Rumor(RumorId::from("r1"));
    let memory = DeletableKnowledgeId::Memory(MemoryId::from("r1"));
    assert_ne!(rumor, memory);
}
