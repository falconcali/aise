use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ConfigError {
    #[error("invalid configuration: {0}")]
    Invalid(String),
    #[error("failed to read configuration: {0}")]
    Io(String),
    #[error("failed to parse configuration: {0}")]
    Parse(String),
}
