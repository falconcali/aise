use crate::config::ContextPreparationConfig;
use crate::domain::asset::validation::BoundedText;
use crate::domain::text::estimate_text_tokens;
use crate::domain::turn::StoryGeneratorOutput;
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
    pub fn new(gateway: Arc<LlmGateway>, context_config: ContextPreparationConfig) -> Self {
        Self {
            gateway,
            projector: Arc::new(DefaultStoryGeneratorPromptContextProjector::new(context_config)),
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
        let decision_role_count = projection.context.character_decisions.len();
        let writer_knowledge_count =
            projection.context.relevant_knowledge.facts.len() + projection.context.relevant_knowledge.rumors.len();
        let constraint_count = projection.context.active_story_constraints.len();
        let active_direction_count = projection.context.narrative_direction.active_directions.len();
        let world_event_intent_count = projection.context.narrative_direction.world_event_intents.len();
        let ai_role_count = projection.context.ai_roles.len();
        let dialogue_example_count = std::iter::once(&projection.context.player_role)
            .chain(projection.context.ai_roles.iter())
            .map(|role| role.dialogue_examples.len())
            .sum::<usize>();
        let dialogue_example_tokens = std::iter::once(&projection.context.player_role)
            .chain(projection.context.ai_roles.iter())
            .flat_map(|role| role.dialogue_examples.iter())
            .map(|example| {
                estimate_text_tokens(example.situation.as_str())
                    .saturating_add(estimate_text_tokens(example.response.as_str()))
            })
            .sum::<u64>();
        let source_dialogue_example_count = ctx
            .baseline()
            .map(|baseline| {
                std::iter::once(&baseline.player_role)
                    .chain(baseline.relevant_roles.iter())
                    .map(|role| role.profile.dialogue_examples.len())
                    .sum::<usize>()
            })
            .unwrap_or(0);
        let omitted_dialogue_example_count = source_dialogue_example_count.saturating_sub(dialogue_example_count);
        let prompt_section_bytes = projection
            .rc_vars
            .as_map()
            .values()
            .filter_map(serde_json::Value::as_str)
            .map(str::len)
            .sum::<usize>();
        let story_summary_bytes = projection.context.story_continuity.story_summary.as_str().len();
        let story_summary_tokens = estimate_text_tokens(projection.context.story_continuity.story_summary.as_str());
        let recent_story_count = projection.context.story_continuity.recent_story.len();
        let recent_story_bytes = projection
            .context
            .story_continuity
            .recent_story
            .iter()
            .map(|segment| segment.as_str().len())
            .sum::<usize>();
        let recent_story_tokens = projection
            .context
            .story_continuity
            .recent_story
            .iter()
            .map(|segment| estimate_text_tokens(segment.as_str()))
            .sum::<u64>();
        let writer_fact_count = projection.context.relevant_knowledge.facts.len();
        let writer_rumor_count = projection.context.relevant_knowledge.rumors.len();
        let cast_policy = match projection.context.instance_settings.cast_policy {
            crate::domain::story_instance::state::CastPolicy::Open => "open",
            crate::domain::story_instance::state::CastPolicy::IncidentalOnly => "incidental_only",
            crate::domain::story_instance::state::CastPolicy::Closed => "closed",
        };
        let request = PromptCompositionInput {
            profile: PromptProfile::StoryGenerator,
            rc_vars: projection.rc_vars,
            fti_vars: projection.fti_vars,
        };
        let max_output_tokens = ctx.budget().remaining_output_tokens().min(u64::from(u32::MAX)) as u32;
        tracing::info!(
            prompt_profile = "story_generator",
            decision_role_count,
            writer_knowledge_count,
            constraint_count,
            active_direction_count,
            world_event_intent_count,
            ai_role_count,
            dialogue_example_count,
            dialogue_example_tokens,
            omitted_dialogue_example_count,
            prompt_section_bytes,
            story_summary_bytes,
            story_summary_tokens,
            recent_story_count,
            recent_story_bytes,
            recent_story_tokens,
            writer_fact_count,
            writer_rumor_count,
            cast_policy,
            projection_ms,
            "story generator prompt projected"
        );
        let scope = ctx.llm_call_scope(TurnStage::StoryGenerator);
        let span = tracing::info_span!(
            "story_generator.generate",
            prompt_profile = "story_generator",
            decision_role_count,
            writer_knowledge_count,
            constraint_count,
        );
        let completion = self
            .gateway
            .complete_text_composed(scope, request, max_output_tokens, LlmCallPurpose::StoryGeneration)
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
        let trimmed = completion.text.trim();
        if trimmed.is_empty() {
            tracing::warn!(prompt_profile = "story_generator", "story generator output is trim-empty");
            return Err(TurnExecutionError::new(
                TurnFailureKind::Llm,
                "model_output_invalid",
                Some(TurnStage::StoryGenerator),
                "story generator output is empty".to_owned(),
            ));
        }
        let story_text = BoundedText::try_new(trimmed.to_owned(), "story_text", ctx.budget().max_story_text_bytes())
            .map_err(|error| {
                tracing::warn!(
                    prompt_profile = "story_generator",
                    error = %error,
                    "story generator output exceeds max_story_text_bytes"
                );
                TurnExecutionError::new(
                    TurnFailureKind::Llm,
                    "model_output_invalid",
                    Some(TurnStage::StoryGenerator),
                    format!("story generator output is invalid: {error}"),
                )
            })?;
        let story = StoryGeneratorOutput { story_text };
        tracing::info!(
            prompt_profile = "story_generator",
            output_bytes = completion.text.len(),
            story_text_bytes = story.story_text.as_str().len(),
            "story generator output decoded"
        );
        ctx.set_generated_story(story)
    }
}

fn map_projection_error(error: StoryGeneratorProjectionError) -> TurnExecutionError {
    let code = match error {
        StoryGeneratorProjectionError::MissingBaseline => "missing_baseline",
        StoryGeneratorProjectionError::MissingWriterPlan => "missing_writer_plan",
        StoryGeneratorProjectionError::InvalidPlayerInput => "invalid_player_input",
        StoryGeneratorProjectionError::UnknownDecisionRole { .. } => "unknown_decision_role",
        StoryGeneratorProjectionError::PlayerRoleDecision { .. } => "player_role_decision",
        StoryGeneratorProjectionError::DuplicateRoleDecision { .. } => "duplicate_role_decision",
        StoryGeneratorProjectionError::RequiredPromptDataExceedsBudget => "required_prompt_data_exceeds_budget",
        StoryGeneratorProjectionError::Invariant { code } => code,
    };
    TurnExecutionError::new(
        TurnFailureKind::InvariantViolation,
        code,
        Some(TurnStage::StoryGenerator),
        error.to_string(),
    )
}
