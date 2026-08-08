use crate::domain::asset::ids::{EntityKey, SceneKey, TopicKey};
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::{CharacterId, EventId, FactId, StoryRevision, TurnId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudienceQuery {
    pub query: BoundedText,
    pub audiences: Vec<Audience>,
    #[serde(default)]
    pub tags: Vec<TopicKey>,
    pub max_items: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Audience {
    Player,
    Character,
    Narrator,
    All,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KnowledgeAudience {
    GlobalWriter,
    Character(CharacterId),
    Validator,
}

#[derive(Debug, Clone)]
pub struct KnowledgeQuery {
    pub audience: KnowledgeAudience,
    pub scene: SceneKey,
    pub entities: Vec<EntityKey>,
    pub topics: Vec<TopicKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeKind {
    Fact,
    Rumor,
    Memory,
    CurrentPerception,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeSource {
    Seed { pack_id: crate::domain::asset::ids::PackId },
    CommittedTurn { turn_id: TurnId, event_id: Option<EventId> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeSourceId {
    Fact(FactId),
    Rumor(crate::domain::asset::ids::RumorId),
    Memory(crate::domain::ids::MemoryId),
    Perception(EventId),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentPerception {
    pub character_id: CharacterId,
    pub source_event_id: EventId,
    pub content: BoundedText,
    pub story_revision: StoryRevision,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryResult {
    pub items: Vec<QueryResultItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryResultItem {
    pub content: BoundedText,
    pub score: f32,
}
