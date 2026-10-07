use crate::core::{ChatMessage, TurnControl};
use crate::llm::{LlmCompletionSpec, LlmError, LlmGateway};
use crate::trace::Observation;

pub async fn process_plan(
    gateway: &LlmGateway,
    messages: Vec<ChatMessage>,
    turn_control: &TurnControl,
    observation: &Observation,
) -> Result<String, LlmError> {
    let result = gateway
        .complete(LlmCompletionSpec { messages }, turn_control, observation)
        .await?;
    Ok(result.content)
}
