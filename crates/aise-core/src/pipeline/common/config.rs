use crate::pipeline::baseline::BaselineConfig;
use crate::pipeline::summary::SummaryConfig;
use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PipelineConfig {
    pub baseline: BaselineConfig,
    pub summary: SummaryConfig,
}
