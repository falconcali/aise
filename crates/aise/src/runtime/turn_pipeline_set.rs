use crate::core::turn_error::{TurnExecutionError, TurnFailureKind};
use crate::core::turn_pipeline::{TurnExecutionPipeline, TurnStage};

pub struct TurnPipelineSet {
    initializer: Box<dyn TurnExecutionPipeline>,
    baseline_builder: Box<dyn TurnExecutionPipeline>,
    writer_planner: Box<dyn TurnExecutionPipeline>,
    retrieval: Box<dyn TurnExecutionPipeline>,
    character_think: Box<dyn TurnExecutionPipeline>,
    story_generator: Box<dyn TurnExecutionPipeline>,
    validation: Box<dyn TurnExecutionPipeline>,
    story_repairer: Box<dyn TurnExecutionPipeline>,
    committer: Box<dyn TurnExecutionPipeline>,
}

impl TurnPipelineSet {
    pub fn builder() -> TurnPipelineSetBuilder {
        TurnPipelineSetBuilder::default()
    }

    pub fn initializer(&self) -> &dyn TurnExecutionPipeline {
        self.initializer.as_ref()
    }

    pub fn baseline_builder(&self) -> &dyn TurnExecutionPipeline {
        self.baseline_builder.as_ref()
    }

    pub fn writer_planner(&self) -> &dyn TurnExecutionPipeline {
        self.writer_planner.as_ref()
    }

    pub fn retrieval(&self) -> &dyn TurnExecutionPipeline {
        self.retrieval.as_ref()
    }

    pub fn character_think(&self) -> &dyn TurnExecutionPipeline {
        self.character_think.as_ref()
    }

    pub fn story_generator(&self) -> &dyn TurnExecutionPipeline {
        self.story_generator.as_ref()
    }

    pub fn validation(&self) -> &dyn TurnExecutionPipeline {
        self.validation.as_ref()
    }

    pub fn story_repairer(&self) -> &dyn TurnExecutionPipeline {
        self.story_repairer.as_ref()
    }

    pub fn committer(&self) -> &dyn TurnExecutionPipeline {
        self.committer.as_ref()
    }
}

#[derive(Default)]
pub struct TurnPipelineSetBuilder {
    initializer: Option<Box<dyn TurnExecutionPipeline>>,
    baseline_builder: Option<Box<dyn TurnExecutionPipeline>>,
    writer_planner: Option<Box<dyn TurnExecutionPipeline>>,
    retrieval: Option<Box<dyn TurnExecutionPipeline>>,
    character_think: Option<Box<dyn TurnExecutionPipeline>>,
    story_generator: Option<Box<dyn TurnExecutionPipeline>>,
    validation: Option<Box<dyn TurnExecutionPipeline>>,
    story_repairer: Option<Box<dyn TurnExecutionPipeline>>,
    committer: Option<Box<dyn TurnExecutionPipeline>>,
}

impl TurnPipelineSetBuilder {
    pub fn initializer(mut self, pipeline: Box<dyn TurnExecutionPipeline>) -> Self {
        self.initializer = Some(pipeline);
        self
    }

    pub fn baseline_builder(mut self, pipeline: Box<dyn TurnExecutionPipeline>) -> Self {
        self.baseline_builder = Some(pipeline);
        self
    }

    pub fn writer_planner(mut self, pipeline: Box<dyn TurnExecutionPipeline>) -> Self {
        self.writer_planner = Some(pipeline);
        self
    }

    pub fn retrieval(mut self, pipeline: Box<dyn TurnExecutionPipeline>) -> Self {
        self.retrieval = Some(pipeline);
        self
    }

    pub fn character_think(mut self, pipeline: Box<dyn TurnExecutionPipeline>) -> Self {
        self.character_think = Some(pipeline);
        self
    }

    pub fn story_generator(mut self, pipeline: Box<dyn TurnExecutionPipeline>) -> Self {
        self.story_generator = Some(pipeline);
        self
    }

    pub fn validation(mut self, pipeline: Box<dyn TurnExecutionPipeline>) -> Self {
        self.validation = Some(pipeline);
        self
    }

    pub fn story_repairer(mut self, pipeline: Box<dyn TurnExecutionPipeline>) -> Self {
        self.story_repairer = Some(pipeline);
        self
    }

    pub fn committer(mut self, pipeline: Box<dyn TurnExecutionPipeline>) -> Self {
        self.committer = Some(pipeline);
        self
    }

    pub fn build(self) -> Result<TurnPipelineSet, TurnExecutionError> {
        let initializer = bind("initializer", self.initializer, TurnStage::TurnInitializer)?;
        let baseline_builder = bind("baseline_builder", self.baseline_builder, TurnStage::BaselineBuilder)?;
        let writer_planner = bind("writer_planner", self.writer_planner, TurnStage::WriterPlanner)?;
        let retrieval = bind("retrieval", self.retrieval, TurnStage::ContextRetrieval)?;
        let character_think = bind("character_think", self.character_think, TurnStage::CharacterThink)?;
        let story_generator = bind("story_generator", self.story_generator, TurnStage::StoryGenerator)?;
        let validation = bind("validation", self.validation, TurnStage::Validation)?;
        let story_repairer = bind("story_repairer", self.story_repairer, TurnStage::StoryRepairer)?;
        let committer = bind("committer", self.committer, TurnStage::TurnCommitter)?;
        Ok(TurnPipelineSet {
            initializer,
            baseline_builder,
            writer_planner,
            retrieval,
            character_think,
            story_generator,
            validation,
            story_repairer,
            committer,
        })
    }
}

fn bind(
    field: &'static str,
    pipeline: Option<Box<dyn TurnExecutionPipeline>>,
    expected: TurnStage,
) -> Result<Box<dyn TurnExecutionPipeline>, TurnExecutionError> {
    let pipeline = pipeline.ok_or_else(|| invariant(format!("pipeline set is missing {field}")))?;
    let actual = pipeline.stage();
    if actual != expected {
        return Err(invariant(format!(
            "pipeline field {field} bound to stage {}, expected {expected}",
            actual.as_str()
        )));
    }
    Ok(pipeline)
}

fn invariant(message: String) -> TurnExecutionError {
    TurnExecutionError::new(TurnFailureKind::InvariantViolation, "invalid_pipeline_set", None, message)
}
