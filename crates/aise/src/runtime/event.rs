use crate::domain::ids::TurnId;

#[derive(Debug, Clone)]
pub enum TurnEvent {
    StageStarted(&'static str),

    Token(String),

    Validation { pass: bool },

    Finished { turn_id: TurnId },
}

pub trait TurnEventSink: Send + Sync {
    fn emit(&self, event: TurnEvent);
}

#[derive(Debug, Clone)]
pub struct TurnResult {
    pub turn_id: TurnId,
    pub story_text: String,
}
