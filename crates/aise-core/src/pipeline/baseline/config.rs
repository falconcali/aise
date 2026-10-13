use crate::core::ActivationConfig;
use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineConfig {
    pub activation: ActivationConfig,
}
