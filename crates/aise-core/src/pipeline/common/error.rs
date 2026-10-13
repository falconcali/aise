use crate::core::NarrativeError;
use crate::llm::LlmError;
use crate::persistence::PersistenceError;
use crate::pipeline::common::PipelineStage;
use crate::prompt::PromptError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("activation knowledge error.")]
    ActivaionKnowledge,

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
}
