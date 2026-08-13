use crate::domain::knowledge::KnowledgeKind;
use crate::domain::text::estimate_text_tokens;
use crate::domain::turn::StoryProposal;
use crate::llm::gateway::LlmGateway;
use crate::prompt::{PromptCompositionInput, PromptProfile};
use crate::story::story_generator_prompt::{
    DefaultStoryGeneratorPromptContextProjector, StoryGeneratorProjectionError, StoryGeneratorPromptContextProjector,
};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_contract::LlmCallPurpose;
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};
use crate::turn::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;
use tracing::Instrument;

pub struct StoryGenerator {
    gateway: Arc<LlmGateway>,
    projector: Arc<dyn StoryGeneratorPromptContextProjector>,
}

impl StoryGenerator {
    pub fn new(gateway: Arc<LlmGateway>) -> Self {
        Self {
            gateway,
            projector: Arc::new(DefaultStoryGeneratorPromptContextProjector),
        }
    }

    pub fn with_projector(gateway: Arc<LlmGateway>, projector: Arc<dyn StoryGeneratorPromptContextProjector>) -> Self {
        Self { gateway, projector }
    }
}

#[async_trait]
impl TurnExecutionPipeline for StoryGenerator {
    fn stage(&self) -> TurnStage {
        TurnStage::StoryGenerator
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), TurnExecutionError> {
        let projection_started = Instant::now();
        let projection = self.projector.project(ctx).map_err(map_projection_error)?;
        let projection_ms = projection_started.elapsed().as_millis() as u64;
        let thought_count = projection.context.character_thoughts.len();
        let writer_knowledge_count = projection.context.relevant_writer_knowledge.len();
        let constraint_count = projection.context.active_story_constraints.len();
        let active_goal_count = projection.context.narrative_direction.active_goals.len();
        let event_intent_count = projection.context.narrative_direction.event_intents.len();
        let ai_character_count = projection.context.ai_characters.len();
        let story_summary_bytes = projection.context.story_continuity.story_summary.as_str().len();
        let story_summary_tokens = estimate_text_tokens(projection.context.story_continuity.story_summary.as_str());
        let recent_story_count = projection.context.story_continuity.recent_story.len();
        let recent_story_bytes = projection
            .context
            .story_continuity
            .recent_story
            .iter()
            .map(|segment| segment.text.as_str().len())
            .sum::<usize>();
        let recent_story_tokens = projection
            .context
            .story_continuity
            .recent_story
            .iter()
            .map(|segment| estimate_text_tokens(segment.text.as_str()))
            .sum::<u64>();
        let writer_fact_count = projection
            .context
            .relevant_writer_knowledge
            .iter()
            .filter(|entry| entry.kind == KnowledgeKind::Fact)
            .count();
        let writer_rumor_count = projection
            .context
            .relevant_writer_knowledge
            .iter()
            .filter(|entry| entry.kind == KnowledgeKind::Rumor)
            .count();
        let writer_memory_count = projection
            .context
            .relevant_writer_knowledge
            .iter()
            .filter(|entry| entry.kind == KnowledgeKind::Memory)
            .count();
        let cast_policy = projection
            .context
            .instance_settings
            .as_ref()
            .map(|settings| match settings.cast_policy {
                crate::domain::story_instance::state::CastPolicy::Open => "open",
                crate::domain::story_instance::state::CastPolicy::IncidentalOnly => "incidental_only",
                crate::domain::story_instance::state::CastPolicy::Closed => "closed",
            })
            .unwrap_or("none");
        let request = PromptCompositionInput {
            profile: PromptProfile::StoryGenerator,
            rc_vars: projection.rc_vars,
            fti_vars: projection.fti_vars,
        };
        let max_output_tokens = ctx.budget().remaining_output_tokens().min(u64::from(u32::MAX)) as u32;
        tracing::info!(
            prompt_profile = "story_generator",
            thought_count,
            writer_knowledge_count,
            constraint_count,
            active_goal_count,
            event_intent_count,
            ai_character_count,
            story_summary_bytes,
            story_summary_tokens,
            recent_story_count,
            recent_story_bytes,
            recent_story_tokens,
            writer_fact_count,
            writer_rumor_count,
            writer_memory_count,
            cast_policy,
            projection_ms,
            "story generator prompt projected"
        );
        let scope = ctx.llm_call_scope(TurnStage::StoryGenerator);
        let span = tracing::info_span!(
            "story_generator.generate",
            prompt_profile = "story_generator",
            thought_count,
            writer_knowledge_count,
            constraint_count,
        );
        let completion = self
            .gateway
            .complete_composed(scope, request, max_output_tokens, LlmCallPurpose::StoryGeneration)
            .instrument(span)
            .await
            .map_err(|error| {
                TurnExecutionError::new(
                    TurnFailureKind::Llm,
                    "llm_error",
                    Some(TurnStage::StoryGenerator),
                    error.to_string(),
                )
            })?;
        let proposal: StoryProposal = serde_json::from_str(&completion.text).map_err(|error| {
            tracing::warn!(
                prompt_profile = "story_generator",
                error = %error,
                "story generator proposal decode failed"
            );
            TurnExecutionError::new(
                TurnFailureKind::Llm,
                "model_output_invalid",
                Some(TurnStage::StoryGenerator),
                format!("story proposal output is invalid: {error}"),
            )
        })?;
        if !proposal.is_within_bounds(
            ctx.budget().max_total_items(),
            ctx.budget().max_item_bytes(),
            ctx.budget().max_proposal_bytes(),
        ) {
            tracing::warn!(
                prompt_profile = "story_generator",
                output_bytes = completion.text.len(),
                event_count = proposal.events.len(),
                character_change_count = proposal.character_changes.len(),
                relationship_change_count = proposal.relationship_changes.len(),
                knowledge_change_count = proposal.knowledge_changes.len(),
                perception_count = proposal.perceptions.len(),
                proposal_bounds = "invalid",
                "story generator proposal rejected"
            );
            return Err(TurnExecutionError::new(
                TurnFailureKind::Llm,
                "model_output_invalid",
                Some(TurnStage::StoryGenerator),
                "story proposal output exceeds a field or collection bound",
            ));
        }
        tracing::info!(
            prompt_profile = "story_generator",
            output_bytes = completion.text.len(),
            event_count = proposal.events.len(),
            character_change_count = proposal.character_changes.len(),
            relationship_change_count = proposal.relationship_changes.len(),
            knowledge_change_count = proposal.knowledge_changes.len(),
            perception_count = proposal.perceptions.len(),
            proposal_bounds = "valid",
            "story generator proposal decoded"
        );
        ctx.set_story_proposal(proposal)
    }
}

fn map_projection_error(error: StoryGeneratorProjectionError) -> TurnExecutionError {
    let code = match error {
        StoryGeneratorProjectionError::MissingBaseline => "missing_baseline",
        StoryGeneratorProjectionError::MissingWriterPlan => "missing_writer_plan",
        StoryGeneratorProjectionError::InvalidPlayerInput => "invalid_player_input",
        StoryGeneratorProjectionError::UnknownThoughtCharacter { .. } => "unknown_thought_character",
        StoryGeneratorProjectionError::PlayerCharacterThought { .. } => "player_character_thought",
        StoryGeneratorProjectionError::DuplicateCharacterThought { .. } => "duplicate_character_thought",
        StoryGeneratorProjectionError::RequiredPromptDataExceedsBudget { .. } => {
            "story_generator_prompt_budget_exceeded"
        }
        StoryGeneratorProjectionError::Invariant { .. } => "story_generator_prompt_invariant",
    };
    TurnExecutionError::new(
        TurnFailureKind::InvariantViolation,
        code,
        Some(TurnStage::StoryGenerator),
        error.to_string(),
    )
}
