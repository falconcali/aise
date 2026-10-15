use crate::core::{ChatMessage, TurnControl};
use crate::llm::{LlmCompletionSpec, LlmError, LlmGateway};
use crate::trace::Observation;

pub async fn character_decision(
    gateway: &LlmGateway,
    messages: Vec<ChatMessage>,
    control: &TurnControl,
    observation: &Observation,
) -> Result<String, LlmError> {
    let result = gateway.complete(LlmCompletionSpec { messages }, control, observation).await?;
    Ok(result.content)
}
