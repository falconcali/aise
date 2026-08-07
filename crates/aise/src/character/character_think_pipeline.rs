use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_contract::LlmCallPurpose;
use crate::core::turn_data::CharacterThought;
use crate::core::turn_error::TurnExecutionError;
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::core::turn_trace::truncate;
use crate::domain::ids::CharacterId;
use crate::llm::gateway::LlmGateway;
use crate::llm::message::CompletionSpec;
use crate::prompt::ContextMerger;
use async_trait::async_trait;
use serde::Deserialize;
use std::sync::Arc;

const MAX_THOUGHT_FIELD_CHARS: usize = 300;
const MAX_PARSE_ERROR_PREVIEW_CHARS: usize = 200;

pub struct CharacterThinkPipeline {
    gateway: Arc<LlmGateway>,
    merger: ContextMerger,
}

impl CharacterThinkPipeline {
    pub fn new(gateway: Arc<LlmGateway>) -> Self {
        Self {
            gateway,
            merger: ContextMerger,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct ThoughtOutput {
    #[serde(default)]
    perception: String,
    #[serde(default)]
    emotion: String,
    #[serde(default)]
    goal: String,
    #[serde(default)]
    possible_action: String,
}

#[async_trait]
impl TurnExecutionPipeline for CharacterThinkPipeline {
    fn stage(&self) -> TurnStage {
        TurnStage::CharacterThink
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), TurnExecutionError> {
        let baseline = ctx
            .baseline()
            .ok_or_else(|| invariant("baseline context not set before character think"))?
            .clone();
        let plan = ctx
            .plan()
            .ok_or_else(|| invariant("writer plan not set before character think"))?
            .clone();
        let player_input = ctx.player_input().to_string();
        let mut thoughts = Vec::with_capacity(plan.character_requests.len());
        for character_id in &plan.character_requests {
            let Some(character) = baseline
                .relevant_characters
                .iter()
                .find(|candidate| &candidate.id == character_id)
                .cloned()
            else {
                tracing::warn!(
                    turn_id = ctx.turn_id().as_str(),
                    story_id = ctx.story_id().as_str(),
                    character_id = character_id.as_str(),
                    "aise.character_think.skip_unknown_character"
                );
                continue;
            };
            let messages = self
                .merger
                .thought_messages(&character, &player_input, baseline.current_scene.as_deref());
            let max_output = ctx.budget().remaining_output_tokens().min(u64::from(u32::MAX)) as u32;
            let spec = CompletionSpec {
                messages,
                max_output_tokens: max_output,
                purpose: LlmCallPurpose::CharacterThink,
            };
            let mut scope = ctx.llm_call_scope(TurnStage::CharacterThink);
            let estimated_input = crate::llm::accounting::TokenAccountant::estimate_input_tokens(&spec.messages);
            let reservation = scope.reserve_llm(estimated_input, u64::from(max_output))?;
            let completion = self.gateway.complete(scope, spec, reservation).await?;
            thoughts.push(parse_thought(&completion.text, character_id.clone())?);
        }
        ctx.set_character_thoughts(thoughts)
    }
}

fn parse_thought(text: &str, character_id: CharacterId) -> Result<CharacterThought, TurnExecutionError> {
    let output: ThoughtOutput = serde_json::from_str(text).map_err(|error| {
        TurnExecutionError::invariant(format!(
            "character thought output is not valid JSON: {error}; raw_output={}",
            truncate(text, MAX_PARSE_ERROR_PREVIEW_CHARS)
        ))
    })?;
    Ok(CharacterThought {
        character_id,
        perception: bound_field(&output.perception),
        emotion: bound_field(&output.emotion),
        goal: bound_field(&output.goal),
        possible_action: bound_field(&output.possible_action),
    })
}

fn bound_field(value: &str) -> String {
    value.chars().take(MAX_THOUGHT_FIELD_CHARS).collect()
}

fn invariant(message: impl Into<String>) -> TurnExecutionError {
    TurnExecutionError::invariant(message)
}

#[cfg(test)]
#[path = "tests/character_think_pipeline_tests.rs"]
mod tests;
