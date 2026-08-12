use crate::prompt::profile::PromptProfile;
use crate::turn::turn_contract::LlmCallPurpose;

#[derive(Debug, Clone)]
pub struct ModelRequest<C> {
    profile: PromptProfile,
    context: C,
    max_output_tokens: u32,
    purpose: LlmCallPurpose,
}

impl<C> ModelRequest<C> {
    pub(crate) fn new(profile: PromptProfile, context: C, max_output_tokens: u32, purpose: LlmCallPurpose) -> Self {
        Self {
            profile,
            context,
            max_output_tokens,
            purpose,
        }
    }

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
