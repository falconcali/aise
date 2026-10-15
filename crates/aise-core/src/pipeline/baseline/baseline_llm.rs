use crate::core::{ChatMessage, PlayerContribution, TurnControl};
use crate::llm::{LlmCompletionSpec, LlmError, LlmGateway, LlmReasoningEffort};
use crate::trace::Observation;

pub async fn process_player_input(
    gateway: &LlmGateway,
    input: &str,
    messages: Vec<ChatMessage>,
    turn_control: &TurnControl,
    observation: &Observation,
) -> Result<PlayerContribution, LlmError> {
    let llm_result = gateway
        .complete_with_reasoning_effort(
            LlmCompletionSpec { messages },
            LlmReasoningEffort::None,
            turn_control,
            observation,
        )
        .await?;
    Ok(PlayerContribution {
        raw: input.to_owned(),
        processed: llm_result.content,
    })
}

#[cfg(test)]
#[path = "tests/baseline_llm_tests.rs"]
mod tests;
