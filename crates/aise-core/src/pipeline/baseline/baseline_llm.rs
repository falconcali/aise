use crate::core::{ChatMessage, PlayerContribution, TurnControl};
use crate::llm::{LlmCompletionSpec, LlmError, LlmGateway};
use crate::trace::Observation;

pub async fn process_player_input(
    gateway: &LlmGateway,
    input: &String,
    messages: Vec<ChatMessage>,
    turn_control: &TurnControl,
    observation: &Observation,
) -> Result<PlayerContribution, LlmError> {
    let llm_spec = LlmCompletionSpec {
        messages,
    };

    let llm_result = gateway.complete(llm_spec, turn_control, observation).await?;

    return Ok(PlayerContribution {
        raw: input.clone(),
        processed: llm_result.content,
    });
}
