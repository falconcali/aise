use serde::{Deserialize, Serialize};

pub const MAX_LLM_CONTENT_CHARS: usize = 2000;
pub const MAX_LLM_RESPONSE_CHARS: usize = 4000;

pub fn truncate(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_owned();
    }
    let prefix: String = text.chars().take(max_chars).collect();
    format!("{prefix}…[+{} chars]", count - max_chars)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnTrace {
    pub trace_id: String,
    pub turn_id: String,
    pub story_id: String,
    pub started_at_ms: u64,
    pub ended_at_ms: u64,
    pub duration_ms: u64,
    pub dropped_span_count: u32,
    pub spans: Vec<TraceSpan>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSpan {
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub kind: String,
    pub name: String,
    pub started_at_ms: u64,
    pub ended_at_ms: u64,
    pub duration_ms: u64,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SpanPayload {
    Turn(TurnData),
    Pipeline(PipelineData),
    LlmCall(LlmCallData),
    ToolCall(ToolCallData),
    Validation(ValidationData),
    Persist(PersistData),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnData {
    pub story_id: String,
    pub turn_id: String,
    pub player_input: String,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineData {
    pub stage: String,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmCallData {
    pub model: String,
    pub messages: Vec<MessageData>,
    pub max_tokens: u32,
    pub temperature: f32,
    pub stream: bool,
    pub status: String,
    pub response: Option<String>,
    pub error: Option<String>,
    pub latency_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageData {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallData {
    pub tool: String,
    pub args: serde_json::Value,
    pub result: serde_json::Value,
    pub ok: bool,
    pub latency_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationData {
    pub pass: bool,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistData {
    pub turn_id: String,
    pub status: String,
    pub error: Option<String>,
    pub latency_ms: u64,
}
