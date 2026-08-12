use crate::character::character_think_prompt::{
    CharacterThinkPromptContextProjector, DefaultCharacterThinkPromptContextProjector,
};
use crate::config::CharacterThinkConfig;
use crate::domain::asset::validation::BoundedText;
use crate::domain::turn::CharacterThought;
use crate::domain::turn::character::CharacterThoughtOutput;
use crate::llm::gateway::LlmGateway;
use crate::prompt::{PromptCompositionInput, PromptProfile};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};
use crate::turn::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;

pub struct CharacterThinkPipeline {
    gateway: Arc<LlmGateway>,
    projector: DefaultCharacterThinkPromptContextProjector,
    config: CharacterThinkConfig,
}

impl CharacterThinkPipeline {
    pub fn new(gateway: Arc<LlmGateway>, config: CharacterThinkConfig) -> Self {
        Self {
            gateway,
            projector: DefaultCharacterThinkPromptContextProjector::new(config.clone()),
            config,
        }
    }
}

#[async_trait]
impl TurnExecutionPipeline for CharacterThinkPipeline {
    fn stage(&self) -> TurnStage {
        TurnStage::CharacterThink
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), TurnExecutionError> {
        let plan = ctx
            .plan()
            .ok_or_else(|| invariant("writer plan not set before character think"))?
            .clone();
        let mut thoughts = Vec::with_capacity(plan.character_think_requests.len());
        for request in &plan.character_think_requests {
            let projection_started = Instant::now();
            let projection = self
                .projector
                .project(ctx, request)
                .map_err(|error| invariant(error.to_string()))?;
            tracing::info!(
                story_id = %ctx.story_id(),
                turn_id = %ctx.turn_id(),
                target_character_id = %request.character_id,
                recent_story_segments = projection.context.story_continuity.recent_story.len(),
                character_knowledge_count = projection.context.relevant_character_knowledge.len(),
                character_impulse_count = projection.context.narrative_character_impulses.len(),
                thinking_focus_bytes = projection.context.thinking_focus.as_str().len(),
                projection_duration_ms = projection_started.elapsed().as_millis(),
                "character think prompt projected"
            );
            let model_request = PromptCompositionInput {
                profile: PromptProfile::CharacterThink,
                rc_vars: projection.rc_vars,
                fti_vars: projection.fti_vars,
            };
            let max_output_tokens = ctx
                .budget()
                .remaining_output_tokens()
                .min(u64::from(self.config.max_output_tokens)) as u32;
            let scope = ctx.llm_call_scope(TurnStage::CharacterThink);
            let completion = self
                .gateway
                .complete_composed(
                    scope,
                    model_request,
                    max_output_tokens,
                    crate::turn::turn_contract::LlmCallPurpose::CharacterThink,
                )
                .await
                .map_err(|error| {
                    TurnExecutionError::new(
                        TurnFailureKind::Llm,
                        "llm_error",
                        Some(TurnStage::CharacterThink),
                        error.to_string(),
                    )
                })?;
            let output: CharacterThoughtOutput = serde_json::from_str(&completion.text)
                .map_err(|_| invariant("character thought output is not valid JSON"))?;
            let perception = normalize_output(output.perception, "perception", self.config.max_field_bytes)?;
            let emotion = normalize_output(output.emotion, "emotion", self.config.max_field_bytes)?;
            let goal = normalize_output(output.goal, "goal", self.config.max_field_bytes)?;
            let possible_action =
                normalize_output(output.possible_action, "possible_action", self.config.max_field_bytes)?;
            let total_bytes = perception
                .as_str()
                .len()
                .saturating_add(emotion.as_str().len())
                .saturating_add(goal.as_str().len())
                .saturating_add(possible_action.as_str().len());
            if total_bytes > self.config.max_total_output_bytes {
                return Err(invariant("character thought exceeds total output byte budget"));
            }
            let thought = CharacterThought {
                character_id: request.character_id.clone(),
                perception,
                emotion,
                goal,
                possible_action,
            };
            thoughts.push(thought);
        }
        ctx.set_character_thoughts(thoughts)
    }
}

fn invariant(message: impl Into<String>) -> TurnExecutionError {
    TurnExecutionError::invariant(message)
}

fn normalize_output(
    value: BoundedText,
    field: &'static str,
    maximum_bytes: usize,
) -> Result<BoundedText, TurnExecutionError> {
    let normalized = value.as_str().trim();
    if normalized.is_empty() {
        return Err(invariant("character thought contains an empty required field"));
    }
    BoundedText::try_new(normalized.to_owned(), field, maximum_bytes)
        .map_err(|_| invariant("character thought field exceeds byte budget"))
}

#[cfg(test)]
#[path = "tests/character_think_pipeline_tests.rs"]
mod tests;
