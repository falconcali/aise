use crate::core::turn_contract::LlmCallPurpose;
use crate::core::turn_data::{BaselineContext, CharacterThought};
use crate::domain::asset::validation::BoundedText;
use crate::domain::character::CharacterState;
use crate::domain::knowledge::query::CurrentPerception;
use crate::domain::story_state::CurrentScene;
use crate::prompt::profile::PromptProfile;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct WriterPlannerContext {
    pub baseline: BaselineContext,
}

#[derive(Debug, Clone, Serialize)]
pub struct CharacterThinkContext {
    pub baseline: BaselineContext,
    pub character: CharacterState,
    pub player_input: BoundedText,
    pub current_perception: Vec<CurrentPerception>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryGeneratorContext {
    pub baseline: BaselineContext,
    pub thoughts: Vec<CharacterThought>,
    pub current_scene: Option<CurrentScene>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryRepairerContext {
    pub generator: StoryGeneratorContext,
    pub issues: Vec<crate::core::turn_validation::ValidationIssue>,
    pub previous_proposal: crate::core::story_proposal::StoryProposal,
}

#[derive(Debug, Clone, Serialize)]
pub struct NarrativeValidatorContext {
    pub baseline: BaselineContext,
    pub proposal: crate::core::story_proposal::StoryProposal,
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

impl ModelRequest<NarrativeValidatorContext> {
    pub fn narrative_validator(context: NarrativeValidatorContext) -> Self {
        Self {
            profile: PromptProfile::NarrativeValidator,
            context,
            max_output_tokens: 256,
            purpose: LlmCallPurpose::NarrativeValidation,
        }
    }
}
