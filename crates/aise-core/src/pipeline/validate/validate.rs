use crate::pipeline::common::{Pipeline, PipelineStage, PipelineError, ValidateScoreResult};
use crate::core::{TurnControl, TurnEventSink};
use crate::trace::Observation;

pub struct ValidateInput {
    pub query: String
}

#[derive(Clone)]
pub struct ValidateScoreConfig {
    pub name: String,
    pub description: String,

    pub target_score: f32,
}

#[derive(Clone)]
pub struct ValidateOutput {
    pub result: String,
    pub is_valid: bool,
    pub scores: Vec<ValidateScoreResult>
}

pub struct ValidatePipeline {
    pub score_configs: Vec<ValidateScoreConfig>
}

impl Pipeline for ValidatePipeline {
    type Input = ValidateInput;
    type Output = ValidateOutput;

    fn stage(&self) -> PipelineStage {
        PipelineStage::Validate
    }

    async fn execute(
        &self,
        input: Self::Input,
        control: &TurnControl,
        sink: &dyn TurnEventSink,
        observation: &Observation
    ) -> Result<Self::Output, PipelineError> {
        Ok(ValidateOutput { 
            result: input.query,
            is_valid: true, 
            scores: self.score_configs.iter().map(|config| ValidateScoreResult {
                name: config.name.clone(),
                description: config.description.clone(),
                score: 10.0,
                target_score: config.target_score,
                advice: "".to_string()
            }).collect() })
    }
}