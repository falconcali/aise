use crate::core::story_proposal::StoryProposal;
use crate::core::turn_contract::LlmCallPurpose;
use crate::core::turn_data::{BaselineContext, CharacterThought, CharacterView, ContextItem, WriterPlan};
use crate::core::turn_validation::ValidationIssue;
use crate::domain::asset::validation::BoundedText;
use crate::domain::knowledge::query::CurrentPerception;
use crate::domain::narrative_graph::director::NarrativePlan;
use crate::domain::narrative_graph::effect::CharacterImpulse;
use crate::domain::story_instance::state::CurrentScene;
use crate::prompt::profile::PromptProfile;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct WriterPlannerContext {
    pub baseline: BaselineContext,
    pub narrative_plan: NarrativePlan,
    pub player_input: BoundedText,
}

#[derive(Debug, Clone, Serialize)]
pub struct CharacterThinkContext {
    pub character: CharacterView,
    pub current_scene: CurrentScene,
    pub retrieved_context: Vec<ContextItem>,
    pub current_perception: Vec<CurrentPerception>,
    pub impulses: Vec<CharacterImpulse>,
    pub player_input: BoundedText,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryGeneratorContext {
    pub baseline: BaselineContext,
    pub writer_plan: WriterPlan,
    pub writer_context: Vec<ContextItem>,
    pub character_thoughts: Vec<CharacterThought>,
    pub player_input: BoundedText,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryRepairerContext {
    pub generation: StoryGeneratorContext,
    pub previous_proposal: StoryProposal,
    pub issues: Vec<ValidationIssue>,
}

#[derive(Debug, Clone)]
pub struct ModelRequest<C> {
    profile: PromptProfile,
    context: C,
    max_output_tokens: u32,
    purpose: LlmCallPurpose,
}

impl<C> ModelRequest<C> {
    pub fn profile(&self) -> PromptProfile {
        self.profile
    }

    pub fn context(&self) -> &C {
        &self.context
    }

    pub fn into_context(self) -> C {
        self.context
    }

    pub fn max_output_tokens(&self) -> u32 {
        self.max_output_tokens
    }

    pub fn purpose(&self) -> LlmCallPurpose {
        self.purpose
    }
}

impl ModelRequest<WriterPlannerContext> {
    pub fn writer_planner(context: WriterPlannerContext, max_output_tokens: u32) -> Self {
        Self {
            profile: PromptProfile::WriterPlanner,
            context,
            max_output_tokens,
            purpose: LlmCallPurpose::WriterPlan,
        }
    }
}

impl ModelRequest<CharacterThinkContext> {
    pub fn character_think(context: CharacterThinkContext, max_output_tokens: u32) -> Self {
        Self {
            profile: PromptProfile::CharacterThink,
            context,
            max_output_tokens,
            purpose: LlmCallPurpose::CharacterThink,
        }
    }
}

impl ModelRequest<StoryGeneratorContext> {
    pub fn story_generator(context: StoryGeneratorContext, max_output_tokens: u32) -> Self {
        Self {
            profile: PromptProfile::StoryGenerator,
            context,
            max_output_tokens,
            purpose: LlmCallPurpose::StoryGeneration,
        }
    }
}

impl ModelRequest<StoryRepairerContext> {
    pub fn story_repairer(context: StoryRepairerContext, max_output_tokens: u32) -> Self {
        Self {
            profile: PromptProfile::StoryRepairer,
            context,
            max_output_tokens,
            purpose: LlmCallPurpose::StoryRepair,
        }
    }
}
