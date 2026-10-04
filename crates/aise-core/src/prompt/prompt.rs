use crate::prompt::{PromptError, PromptConfig, PromptSpec, PromptResult};
use crate::trace::Observation;

pub struct Prompt {
}

impl Prompt {
    pub fn new(config: PromptConfig) -> Result<Self, PromptError> {
        Ok(Self {})
    }

    pub fn render(&self, spec: PromptSpec, observation: &Observation) -> Result<PromptResult, PromptError> {
        Ok(PromptResult { messages: vec![] })
    }
}