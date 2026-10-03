use crate::core::TurnControl;
use crate::llm::{LlmCompletionSpec, LlmError, LlmGateway, Message, MessageRole};
use crate::trace::Observation;

pub async fn process_player_input(
    gateway: &LlmGateway,
    input: &str,
    turn_control: &TurnControl,
    observation: &Observation,
) -> Result<String, LlmError> {
    let spec = LlmCompletionSpec {
        messages: vec![
            Message {
                role: MessageRole::System,
                content: "把用户的输入处理的更加文艺丰满一些, 不要过于生硬.".into(),
            },
            Message {
                role: MessageRole::User,
                content: input.to_string(),
            },
        ],
    };

    let response = gateway.complete(spec, turn_control, observation).await?;
    Ok(response.content)
}
