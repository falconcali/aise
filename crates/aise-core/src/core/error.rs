use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("invalid ID: {field}-{value}")]
    InvalidId { field: &'static str, value: String },
}
