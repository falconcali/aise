use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("invalid ID: {field}-{value}")]
    InvalidId { field: &'static str, value: String },

    #[error("bounded text limit exceeded: {limit} (actual: {actual}, maximum: {maximum})")]
    BoundedTextLimitExceeded {
        limit: &'static str,
        actual: u64,
        maximum: u64,
    },

    #[error("retrieval hint too long: {actual} (maximum: {maximum})")]
    RetrievalHintTooLong { actual: u64, maximum: u64 },

    #[error("retrieval hint is empty.")]
    RetrievalHintEmpty,

    #[error("{subject} digest failed: {message}")]
    DigestFailed { subject: &'static str, message: String },
}
