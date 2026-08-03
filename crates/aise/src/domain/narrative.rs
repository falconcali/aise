use super::ids::{EventId, TurnId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryTurn {
    pub id: TurnId,
    pub player_input: String,
    pub story_text: String,
    pub summary_delta: Option<String>,

    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryEvent {
    pub id: EventId,
    pub turn_id: TurnId,
    pub seq: u32,
    pub kind: EventKind,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    Dialogue,
    Action,
    WorldChange,
    Chapter,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorySummary {
    pub text: String,
}
