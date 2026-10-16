use crate::core::NarrativeError;
use crate::llm::LlmError;
use crate::persistence::PersistenceError;
use crate::pipeline::common::PipelineStage;
use crate::prompt::PromptError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("pipeline stage {stage} llm call failed: {source}")]
    Llm {
        stage: PipelineStage,
        #[source]
        source: LlmError,
    },
    #[error("pipeline stage {stage} prompt failed: {source}")]
    Prompt {
        stage: PipelineStage,
        #[source]
        source: PromptError,
    },
    #[error("pipeline stage {stage} persistence failed: {source}")]
    Persistence {
        stage: PipelineStage,
        #[source]
        source: PersistenceError,
    },
    #[error("pipeline stage {stage} narrative failed: {source}")]
    Narrative {
        stage: PipelineStage,
        #[source]
        source: NarrativeError,
    },
    #[error("pipeline stage {stage} produced empty output")]
    EmptyOutput { stage: PipelineStage },
    #[error("pipeline stage {stage} produced invalid output ({code}): {message}")]
    InvalidOutput {
        stage: PipelineStage,
        code: &'static str,
        message: String,
    },
    #[error("pipeline stage {stage} observed inconsistent data ({code}): {message}")]
    InconsistentData {
        stage: PipelineStage,
        code: &'static str,
        message: String,
    },
    #[error("pipeline stage {stage} has invalid configuration ({code}): {message}")]
    InvalidConfig {
        stage: PipelineStage,
        code: &'static str,
        message: String,
    },
    #[error("validation failed at proposal version {proposal_version} ({code}): {message}")]
    ValidationFailed {
        code: &'static str,
        proposal_version: u32,
        message: String,
    },
}

impl PipelineError {
    pub fn llm(stage: PipelineStage) -> impl FnOnce(LlmError) -> Self {
        move |source| Self::Llm { stage, source }
    }

    pub fn prompt(stage: PipelineStage) -> impl FnOnce(PromptError) -> Self {
        move |source| Self::Prompt { stage, source }
    }

    pub fn persistence(stage: PipelineStage) -> impl FnOnce(PersistenceError) -> Self {
        move |source| Self::Persistence { stage, source }
    }

    pub fn narrative(stage: PipelineStage) -> impl FnOnce(NarrativeError) -> Self {
        move |source| Self::Narrative { stage, source }
    }

    pub fn invalid_output(stage: PipelineStage, code: &'static str, message: impl Into<String>) -> Self {
        Self::InvalidOutput {
            stage,
            code,
            message: message.into(),
        }
    }

    pub fn inconsistent_data(stage: PipelineStage, code: &'static str, message: impl Into<String>) -> Self {
        Self::InconsistentData {
            stage,
            code,
            message: message.into(),
        }
    }

    pub fn invalid_config(stage: PipelineStage, code: &'static str, message: impl Into<String>) -> Self {
        Self::InvalidConfig {
            stage,
            code,
            message: message.into(),
        }
    }
}
