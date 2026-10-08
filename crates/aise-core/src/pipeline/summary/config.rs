use serde::Deserialize;

const DEFAULT_SUMMARY_TURN_COUNT: usize = 6;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SummaryConfig {
    pub summary_turn_count: usize,
}

impl Default for SummaryConfig {
    fn default() -> Self {
        Self {
            summary_turn_count: DEFAULT_SUMMARY_TURN_COUNT,
        }
    }
}
