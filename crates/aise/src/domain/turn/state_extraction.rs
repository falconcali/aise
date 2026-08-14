use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::ids::{AttributeKey, LocationKey, MemoryKind, TopicKey};
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::ids::{CharacterId, MemoryId, RumorId};
use crate::domain::knowledge::fact::Proposition;
use crate::domain::knowledge::query::KnowledgeSourceId;
use crate::domain::knowledge::rumor::{Claim, TruthValue};
use crate::domain::story_instance::state::{CurrentScene, RelationshipState};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy)]
pub struct StoryStateExtractionLimits {
    pub max_character_states: usize,
    pub max_relationship_states: usize,
    pub max_knowledge_changes: usize,
    pub max_goals_per_character: usize,
    pub max_attributes_per_character: usize,
    pub max_entities_per_knowledge: usize,
    pub max_topics_per_knowledge: usize,
    pub max_item_bytes: usize,
    pub max_knowledge_change_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryStateExtractorOutput {
    pub character_states: Vec<ExtractedCharacterState>,
    pub relationship_states: Vec<RelationshipState>,
    pub knowledge_changes: Vec<ProposedKnowledgeMutation>,
    pub current_scene: CurrentScene,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractedCharacterState {
    pub character_id: CharacterId,
    pub location: LocationKey,
    pub goals: Vec<BoundedText>,
    #[serde(default)]
    pub attributes: BTreeMap<AttributeKey, ScalarValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProposedKnowledgeMutation {
    Add {
        value: ProposedKnowledgeValue,
    },
    Update {
        target: KnowledgeSourceId,
        value: ProposedKnowledgeValue,
    },
    Delete {
        target: DeletableKnowledgeId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProposedKnowledgeValue {
    Fact {
        content: BoundedText,
        proposition: Option<Proposition>,
        entities: Vec<KnowledgeEntity>,
        topics: Vec<TopicKey>,
        salience: u8,
    },
    Rumor {
        content: BoundedText,
        claim: Option<Claim>,
        entities: Vec<KnowledgeEntity>,
        topics: Vec<TopicKey>,
        salience: u8,
        source_character_id: Option<CharacterId>,
        truth_value: TruthValue,
    },
    Memory {
        owner: CharacterId,
        memory_kind: MemoryKind,
        content: BoundedText,
        entities: Vec<KnowledgeEntity>,
        topics: Vec<TopicKey>,
        salience: u8,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum DeletableKnowledgeId {
    Rumor(RumorId),
    Memory(MemoryId),
}

impl StoryStateExtractorOutput {
    pub fn json_schema(limits: StoryStateExtractionLimits) -> Value {
        let bounded_string = || json!({"type": "string", "maxLength": limits.max_item_bytes});
        let knowledge_bounded_string = || json!({"type": "string", "maxLength": limits.max_knowledge_change_bytes});
        let semantic_key = || json!({"type": "string", "minLength": 1, "maxLength": limits.max_item_bytes});
        let entity = json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["kind", "key"],
            "properties": {
                "kind": {
                    "enum": ["world", "role", "character", "location", "scene", "narrative_node", "event"]
                },
                "key": {"type": "string", "minLength": 1}
            }
        });
        let scalar = json!({"type": ["boolean", "integer", "string"]});
        let proposition = json!({
            "type": ["object", "null"],
            "additionalProperties": false,
            "required": ["subject", "predicate", "value"],
            "properties": {
                "subject": entity,
                "predicate": bounded_string(),
                "value": scalar
            }
        });
        let entities_array = json!({
            "type": "array",
            "maxItems": limits.max_entities_per_knowledge,
            "items": entity
        });
        let topics_array = json!({
            "type": "array",
            "maxItems": limits.max_topics_per_knowledge,
            "items": semantic_key()
        });
        let salience = json!({"type": "integer", "minimum": 0, "maximum": 255});
        let fact_value = json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["kind", "content", "proposition", "entities", "topics", "salience"],
            "properties": {
                "kind": {"const": "fact"},
                "content": knowledge_bounded_string(),
                "proposition": proposition,
                "entities": entities_array,
                "topics": topics_array,
                "salience": salience
            }
        });
        let rumor_value = json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "kind", "content", "claim", "entities", "topics", "salience",
                "source_character_id", "truth_value"
            ],
            "properties": {
                "kind": {"const": "rumor"},
                "content": knowledge_bounded_string(),
                "claim": proposition,
                "entities": entities_array,
                "topics": topics_array,
                "salience": salience,
                "source_character_id": {"type": ["string", "null"]},
                "truth_value": {"enum": ["true", "false", "unverified"]}
            }
        });
        let memory_value = json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["kind", "owner", "memory_kind", "content", "entities", "topics", "salience"],
            "properties": {
                "kind": {"const": "memory"},
                "owner": {"type": "string", "minLength": 1},
                "memory_kind": semantic_key(),
                "content": knowledge_bounded_string(),
                "entities": entities_array,
                "topics": topics_array,
                "salience": salience
            }
        });
        let knowledge_value = json!({
            "oneOf": [fact_value, rumor_value, memory_value]
        });
        let deletable_target = json!({
            "oneOf": [
                {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["kind", "id"],
                    "properties": {"kind": {"const": "rumor"}, "id": {"type": "string", "minLength": 1}}
                },
                {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["kind", "id"],
                    "properties": {"kind": {"const": "memory"}, "id": {"type": "string", "minLength": 1}}
                }
            ]
        });
        let update_target = json!({
            "oneOf": [
                {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["kind", "id"],
                    "properties": {"kind": {"const": "fact"}, "id": {"type": "string", "minLength": 1}}
                },
                {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["kind", "id"],
                    "properties": {"kind": {"const": "rumor"}, "id": {"type": "string", "minLength": 1}}
                },
                {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["kind", "id"],
                    "properties": {"kind": {"const": "memory"}, "id": {"type": "string", "minLength": 1}}
                }
            ]
        });
        let knowledge_mutation = json!({
            "oneOf": [
                {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["operation", "value"],
                    "properties": {
                        "operation": {"const": "add"},
                        "value": knowledge_value
                    }
                },
                {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["operation", "target", "value"],
                    "properties": {
                        "operation": {"const": "update"},
                        "target": update_target,
                        "value": knowledge_value
                    }
                },
                {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["operation", "target"],
                    "properties": {
                        "operation": {"const": "delete"},
                        "target": deletable_target
                    }
                }
            ]
        });
        let character_state = json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["character_id", "location", "goals", "attributes"],
            "properties": {
                "character_id": {"type": "string", "minLength": 1},
                "location": {"type": "string", "minLength": 1},
                "goals": {
                    "type": "array",
                    "maxItems": limits.max_goals_per_character,
                    "items": bounded_string()
                },
                "attributes": {
                    "type": "object",
                    "maxProperties": limits.max_attributes_per_character,
                    "additionalProperties": scalar
                }
            }
        });
        let relationship_state = json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["source_character_id", "target_character_id", "kind", "trust"],
            "properties": {
                "source_character_id": {"type": "string", "minLength": 1},
                "target_character_id": {"type": "string", "minLength": 1},
                "kind": semantic_key(),
                "trust": {"type": "integer", "minimum": -32768, "maximum": 32767}
            }
        });
        let current_scene = json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["scene_key", "location_key", "time", "description", "present_character_ids"],
            "properties": {
                "scene_key": {"type": "string", "minLength": 1},
                "location_key": {"type": "string", "minLength": 1},
                "time": bounded_string(),
                "description": {"type": "string", "maxLength": limits.max_item_bytes},
                "present_character_ids": {
                    "type": "array",
                    "maxItems": limits.max_character_states,
                    "items": {"type": "string", "minLength": 1}
                }
            }
        });
        json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "additionalProperties": false,
            "required": ["character_states", "relationship_states", "knowledge_changes", "current_scene"],
            "properties": {
                "character_states": {
                    "type": "array",
                    "maxItems": limits.max_character_states,
                    "items": character_state
                },
                "relationship_states": {
                    "type": "array",
                    "maxItems": limits.max_relationship_states,
                    "items": relationship_state
                },
                "knowledge_changes": {
                    "type": "array",
                    "maxItems": limits.max_knowledge_changes,
                    "items": knowledge_mutation
                },
                "current_scene": current_scene
            }
        })
    }
}

#[cfg(test)]
#[path = "tests/state_extraction_tests.rs"]
mod tests;
