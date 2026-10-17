use crate::core::{ChatMessage, TurnControl};
use crate::llm::{LlmCompletionSpec, LlmError, LlmGateway, LlmReasoningEffort};
use crate::trace::Observation;

pub(super) async fn evaluate_story(
    gateway: &LlmGateway,
    messages: Vec<ChatMessage>,
    control: &TurnControl,
    observation: &Observation,
) -> Result<String, LlmError> {
    gateway
        .complete_with_reasoning_effort(LlmCompletionSpec { messages }, LlmReasoningEffort::High, control, observation)
        .await
        .map(|result| result.content)
}
