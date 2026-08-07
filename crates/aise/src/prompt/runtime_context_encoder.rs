use crate::prompt::error::PromptError;
use crate::prompt::profile::UntrustedContextMessage;
use serde::Serialize;

pub struct RuntimeContextEncoder;

impl RuntimeContextEncoder {
    pub fn encode<C: Serialize>(&self, context: &C) -> Result<UntrustedContextMessage, PromptError> {
        let json = serde_json::to_string(context)
            .map_err(|error| PromptError::RenderError(format!("context encode failed: {error}")))?;
        Ok(UntrustedContextMessage::new(json))
    }
}
