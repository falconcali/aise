use std::fmt;
use async_trait::async_trait;
use crate::core::story::StoryContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineStage {
    Baseline,
    Plan,
    Retrieval,
    Think,
    Generate,
    Validate,
    Repair,
    Extract,
    Commit,
}

impl PipelineStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            PipelineStage::Baseline => "baseline",
            PipelineStage::Plan => "plan",
            PipelineStage::Retrieval => "retrieval",
            PipelineStage::Think => "think",
            PipelineStage::Generate => "generate",
            PipelineStage::Validate => "validate",
            PipelineStage::Repair => "repair",
            PipelineStage::Extract => "extract",
            PipelineStage::Commit => "commit",
        }
    }
}

impl fmt::Display for PipelineStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

pub trait Pipeline: Send + Sync {
    fn stage(&self) -> PipelineStage;
}

pub struct BaselineContext {
    pub story_ctx: StoryContext
}

#[async_trait]
pub trait BaselinePipeline: Pipeline {
    async fn execute(&self, story_ctx: &StoryContext) -> Result<BaselineContext, PipelineError>;
}

pub struct WriterPlan {
    pub baseline_ctx: BaselineContext
}

#[async_trait]
pub trait PlanPipeline: Pipeline {
    async fn execute(&self, baseline_ctx: &BaselineContext) -> Result<WriterPlan, PipelineError>;
}