use crate::core::{ChatMessage, TurnControl};
use crate::llm::{LlmCompletionSpec, LlmError, LlmGateway};
use crate::trace::Observation;

pub(super) async fn repair_story(
    gateway: &LlmGateway,
    messages: Vec<ChatMessage>,
    control: &TurnControl,
    observation: &Observation,
) -> Result<String, LlmError> {
    gateway
        .complete(LlmCompletionSpec { messages }, control, observation)
        .await
        .map(|result| result.content)
}
