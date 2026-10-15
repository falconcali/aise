use crate::pipeline::baseline::BaselineConfig;
use crate::pipeline::generate::GenerateConfig;
use crate::pipeline::plan::PlanConfig;
use crate::pipeline::summary::SummaryConfig;
use crate::pipeline::think::ThinkConfig;
use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PipelineConfig {
    pub baseline: BaselineConfig,
    pub plan: PlanConfig,
    pub think: ThinkConfig,
    pub generate: GenerateConfig,
    pub summary: SummaryConfig,
}
