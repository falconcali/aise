use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct TurnRequest {
    pub player_input: String,
    #[serde(default)]
    pub include_trace: bool,
}
