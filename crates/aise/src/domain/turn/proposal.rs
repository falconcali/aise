use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::ids::{AttributeKey, LocationKey, MemoryKind, RelationshipKind, TopicKey};
use crate::domain::asset::validation::ScalarValue;
use crate::domain::ids::{CharacterId, FactId};
use crate::domain::knowledge::fact::Proposition;
use crate::domain::knowledge::rumor::{Claim, TruthValue};
use crate::domain::narrative::EventKind;
use crate::domain::story_instance::state::CurrentScene;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryProposalOutput {
    pub story_text: String,
    #[serde(default)]
    pub events: Vec<ProposedEvent>,
    #[serde(default)]
    pub character_changes: Vec<ProposedCharacterChange>,
    #[serde(default)]
    pub relationship_changes: Vec<ProposedRelationshipChange>,
    #[serde(default)]
    pub knowledge_changes: Vec<ProposedKnowledgeChange>,
    #[serde(default)]
    pub perceptions: Vec<ProposedPerception>,
    pub scene_change: Option<CurrentScene>,
    pub summary_text: Option<String>,
}

pub type StoryProposal = StoryProposalOutput;

impl StoryProposalOutput {
    pub fn json_schema(max_items: usize, max_item_bytes: usize, max_total_bytes: usize) -> Value {
        let bounded_string = || json!({"type": "string", "maxLength": max_item_bytes});
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
        let evidence = json!({
            "oneOf": [
                {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["snapshot_fact"],
                    "properties": {"snapshot_fact": {"type": "string", "minLength": 1}}
                },
                {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["proposed_event"],
                    "properties": {
                        "proposed_event": {
                            "type": "object",
                            "additionalProperties": false,
                            "required": ["event_index"],
                            "properties": {"event_index": {"type": "integer", "minimum": 0}}
                        }
                    }
                }
            ]
        });
        let knowledge_common = json!({
            "content": bounded_string(),
            "entities": {"type": "array", "maxItems": max_items, "items": entity},
            "topics": {"type": "array", "maxItems": max_items, "items": {"type": "string", "minLength": 1}},
            "salience": {"type": "integer", "minimum": 0, "maximum": 255}
        });
        let scene = json!({
            "type": ["object", "null"],
            "additionalProperties": false,
            "required": ["scene_key", "location_key", "time", "description", "present_character_ids"],
            "properties": {
                "scene_key": {"type": "string", "minLength": 1},
                "location_key": {"type": "string", "minLength": 1},
                "time": bounded_string(),
                "description": {"type": "string", "maxLength": max_total_bytes},
                "present_character_ids": {
                    "type": "array",
                    "maxItems": max_items,
                    "items": {"type": "string", "minLength": 1}
                }
            }
        });
        json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "additionalProperties": false,
            "required": [
                "story_text",
                "events",
                "character_changes",
                "relationship_changes",
                "knowledge_changes",
                "perceptions",
                "scene_change",
                "summary_text"
            ],
            "properties": {
                "story_text": {"type": "string", "minLength": 1, "maxLength": max_total_bytes},
                "events": {
                    "type": "array",
                    "maxItems": max_items,
                    "items": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["kind", "summary"],
                        "properties": {
                            "kind": {"enum": ["dialogue", "action", "world_change", "chapter"]},
                            "summary": bounded_string()
                        }
                    }
                },
                "character_changes": {
                    "type": "array",
                    "maxItems": max_items,
                    "items": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["character_id", "location", "goals", "attribute_updates"],
                        "properties": {
                            "character_id": {"type": "string", "minLength": 1},
                            "location": {"type": ["string", "null"]},
                            "goals": {
                                "oneOf": [
                                    {"type": "null"},
                                    {
                                        "type": "array",
                                        "maxItems": max_items,
                                        "items": bounded_string()
                                    }
                                ]
                            },
                            "attribute_updates": {
                                "type": "object",
                                "maxProperties": max_items,
                                "additionalProperties": scalar
                            }
                        }
                    }
                },
                "relationship_changes": {
                    "type": "array",
                    "maxItems": max_items,
                    "items": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["source_character_id", "target_character_id", "kind", "trust_delta"],
                        "properties": {
                            "source_character_id": {"type": "string", "minLength": 1},
                            "target_character_id": {"type": "string", "minLength": 1},
                            "kind": {"type": "string", "minLength": 1},
                            "trust_delta": {"type": "integer", "minimum": -32768, "maximum": 32767}
                        }
                    }
                },
                "knowledge_changes": {
                    "type": "array",
                    "maxItems": max_items,
                    "items": {
                        "oneOf": [
                            {
                                "type": "object",
                                "additionalProperties": false,
                                "required": ["kind", "content", "proposition", "entities", "topics", "salience", "evidence"],
                                "properties": {
                                    "kind": {"const": "fact"},
                                    "content": knowledge_common["content"],
                                    "proposition": proposition,
                                    "entities": knowledge_common["entities"],
                                    "topics": knowledge_common["topics"],
                                    "salience": knowledge_common["salience"],
                                    "evidence": {"type": "array", "maxItems": max_items, "items": evidence}
                                }
                            },
                            {
                                "type": "object",
                                "additionalProperties": false,
                                "required": [
                                    "kind", "content", "claim", "entities", "topics", "salience",
                                    "source_character_id", "truth_value", "source_event_index"
                                ],
                                "properties": {
                                    "kind": {"const": "rumor"},
                                    "content": knowledge_common["content"],
                                    "claim": proposition,
                                    "entities": knowledge_common["entities"],
                                    "topics": knowledge_common["topics"],
                                    "salience": knowledge_common["salience"],
                                    "source_character_id": {"type": ["string", "null"]},
                                    "truth_value": {"enum": ["true", "false", "unverified"]},
                                    "source_event_index": {"type": ["integer", "null"], "minimum": 0}
                                }
                            },
                            {
                                "type": "object",
                                "additionalProperties": false,
                                "required": [
                                    "kind", "owner", "memory_kind", "content", "entities",
                                    "topics", "salience", "source_event_index"
                                ],
                                "properties": {
                                    "kind": {"const": "memory"},
                                    "owner": {"type": "string", "minLength": 1},
                                    "memory_kind": {"type": "string", "minLength": 1},
                                    "content": knowledge_common["content"],
                                    "entities": knowledge_common["entities"],
                                    "topics": knowledge_common["topics"],
                                    "salience": knowledge_common["salience"],
                                    "source_event_index": {"type": ["integer", "null"], "minimum": 0}
                                }
                            }
                        ]
                    }
                },
                "perceptions": {
                    "type": "array",
                    "maxItems": max_items,
                    "items": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["character_id", "source_event_index", "content"],
                        "properties": {
                            "character_id": {"type": "string", "minLength": 1},
                            "source_event_index": {"type": "integer", "minimum": 0},
                            "content": bounded_string()
                        }
                    }
                },
                "scene_change": scene,
                "summary_text": {"type": ["string", "null"], "maxLength": max_total_bytes}
            }
        })
    }

    pub fn is_within_bounds(&self, max_items: usize, max_item_bytes: usize, max_total_bytes: usize) -> bool {
        if self.story_text.trim().is_empty()
            || self.story_text.len() > max_total_bytes
            || self.events.len() > max_items
            || self.character_changes.len() > max_items
            || self.relationship_changes.len() > max_items
            || self.knowledge_changes.len() > max_items
            || self.perceptions.len() > max_items
            || self.events.iter().any(|event| event.summary.len() > max_item_bytes)
            || self
                .perceptions
                .iter()
                .any(|perception| perception.content.len() > max_item_bytes)
            || self
                .summary_text
                .as_ref()
                .is_some_and(|summary| summary.len() > max_total_bytes)
        {
            return false;
        }
        for change in &self.character_changes {
            if change.attribute_updates.len() > max_items
                || change.goals.as_ref().is_some_and(|goals| {
                    goals.len() > max_items || goals.iter().any(|goal| goal.len() > max_item_bytes)
                })
            {
                return false;
            }
        }
        for change in &self.knowledge_changes {
            let (content, entities, topics) = match change {
                ProposedKnowledgeChange::Fact {
                    content,
                    entities,
                    topics,
                    ..
                }
                | ProposedKnowledgeChange::Rumor {
                    content,
                    entities,
                    topics,
                    ..
                }
                | ProposedKnowledgeChange::Memory {
                    content,
                    entities,
                    topics,
                    ..
                } => (content, entities, topics),
            };
            if content.len() > max_item_bytes || entities.len() > max_items || topics.len() > max_items {
                return false;
            }
        }
        self.scene_change.as_ref().is_none_or(|scene| {
            scene.time.as_str().len() <= max_item_bytes
                && scene.description.as_str().len() <= max_total_bytes
                && scene.present_character_ids.len() <= max_items
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposedEvent {
    pub kind: EventKind,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposedCharacterChange {
    pub character_id: CharacterId,
    pub location: Option<LocationKey>,
    pub goals: Option<Vec<String>>,
    #[serde(default)]
    pub attribute_updates: BTreeMap<AttributeKey, ScalarValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposedRelationshipChange {
    pub source_character_id: CharacterId,
    pub target_character_id: CharacterId,
    pub kind: RelationshipKind,
    pub trust_delta: i16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProposedKnowledgeChange {
    Fact {
        content: String,
        proposition: Option<Proposition>,
        entities: Vec<KnowledgeEntity>,
        topics: Vec<TopicKey>,
        salience: u8,
        evidence: Vec<WorldFactEvidenceRef>,
    },
    Rumor {
        content: String,
        claim: Option<Claim>,
        entities: Vec<KnowledgeEntity>,
        topics: Vec<TopicKey>,
        salience: u8,
        source_character_id: Option<CharacterId>,
        truth_value: TruthValue,
        source_event_index: Option<u32>,
    },
    Memory {
        owner: CharacterId,
        memory_kind: MemoryKind,
        content: String,
        entities: Vec<KnowledgeEntity>,
        topics: Vec<TopicKey>,
        salience: u8,
        source_event_index: Option<u32>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposedPerception {
    pub character_id: CharacterId,
    pub source_event_index: u32,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldFactEvidenceRef {
    SnapshotFact(FactId),
    ProposedEvent { event_index: u32 },
}

impl WorldFactEvidenceRef {
    pub fn as_str(&self) -> String {
        match self {
            Self::SnapshotFact(fact_id) => format!("snapshot_fact:{}", fact_id.as_str()),
            Self::ProposedEvent { event_index } => format!("proposed_event:{event_index}"),
        }
    }
}
