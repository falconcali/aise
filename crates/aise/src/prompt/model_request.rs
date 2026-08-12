use crate::domain::asset::validation::BoundedText;
use crate::domain::turn::proposal::StoryProposal;
use crate::domain::turn::{BaselineContext, CharacterThought, ContextItem, WriterPlan};
use crate::prompt::profile::PromptProfile;
use crate::turn::turn_contract::LlmCallPurpose;
use crate::turn::turn_validation::ValidationIssue;
use serde::Serialize;

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
