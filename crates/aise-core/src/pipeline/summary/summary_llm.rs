use crate::core::{ChatMessage, TurnControl};
use crate::llm::{LlmCompletionSpec, LlmError, LlmGateway};
use crate::trace::Observation;

pub async fn summarize_story(
    gateway: &LlmGateway,
    messages: Vec<ChatMessage>,
    turn_control: &TurnControl,
    observation: &Observation,
) -> Result<String, LlmError> {
    gateway
        .complete(LlmCompletionSpec { messages }, turn_control, observation)
        .await
        .map(|result| result.content)
}
