use crate::prompt::{PromptConfig, PromptError};
use crate::core::{ChatMessage};

#[derive(Clone, Debug)]
pub struct PromptSpec {}

#[derive(Clone, Debug)]
pub struct RenderedPrompt {
    pub messages: Vec<ChatMessage>,
}

impl RenderedPrompt {
    pub fn into_messages(self) -> Vec<ChatMessage> {
        self.messages
    }
}

pub struct Prompt {}

impl Prompt {
    pub fn new(config: PromptConfig) -> Result<Self, PromptError> {
        Ok(Self {})
    }

    pub fn render(&self, spec: PromptSpec) -> Result<RenderedPrompt, PromptError> {
        Ok(RenderedPrompt { messages: vec![ChatMessage::system("Hello, world!")],
        })
    }
}