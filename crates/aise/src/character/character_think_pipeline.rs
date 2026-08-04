use crate::core::turn_context::TurnExecutionContext;
use crate::core::turn_data::CharacterThought;
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::error::AiseError;
use async_trait::async_trait;

#[derive(Default)]
pub struct CharacterThinkPipeline;

#[async_trait]
impl TurnExecutionPipeline for CharacterThinkPipeline {
    fn stage(&self) -> TurnStage {
        TurnStage::CharacterThink
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), AiseError> {
        let plan = ctx
            .plan()
            .ok_or_else(|| AiseError::InvariantViolation("writer plan not set before character think".into()))?
            .clone();
        let player_input = ctx.player_input().to_string();
        let thoughts: Vec<CharacterThought> = plan
            .character_requests
            .into_iter()
            .map(|character_id| CharacterThought {
                character_id,
                perception: player_input.clone(),
                emotion: String::new(),
                goal: String::new(),
                possible_action: String::new(),
            })
            .collect();
        ctx.set_character_thoughts(thoughts)
    }
}
