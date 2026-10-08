use crate::pipeline::summary::SummaryConfig;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PipelineConfig {
    pub summary: SummaryConfig,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            summary: SummaryConfig::default(),
        }
    }
}
