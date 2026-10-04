use crate::llm::LlmError;
use crate::pipeline::common::PipelineStage;
use crate::prompt::PromptError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("pipeline stage {stage} failed: {message}")]
    StageFailed { stage: PipelineStage, message: String },
}

impl PipelineError {
    pub fn new(stage: PipelineStage, message: impl Into<String>) -> Self {
        Self::StageFailed {
            stage,
            message: message.into(),
        }
    }
}

impl From<LlmError> for PipelineError {
    fn from(error: LlmError) -> Self {
        Self::StageFailed {
            stage: PipelineStage::Baseline,
            message: error.to_string(),
        }
    }
}

impl From<PromptError> for PipelineError {
    fn from(error: PromptError) -> Self {
        Self::StageFailed {
            stage: PipelineStage::Baseline,
            message: error.to_string(),
        }
    }
}
