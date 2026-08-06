use super::ids::{EventId, TurnId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryTurn {
    pub id: TurnId,
    pub player_input: String,
    pub story_text: String,

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
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Dialogue,
    Action,
    WorldChange,
    Chapter,
}

impl EventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventKind::Dialogue => "dialogue",
            EventKind::Action => "action",
            EventKind::WorldChange => "world_change",
            EventKind::Chapter => "chapter",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorySummary {
    pub text: String,
}
