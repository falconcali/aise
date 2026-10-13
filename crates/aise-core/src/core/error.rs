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

    #[error("activation key is empty.")]
    ActivationKeyEmpty,

    #[error("activation key too long: {actual} (maximum: {maximum})")]
    ActivationKeyTooLong { actual: u64, maximum: u64 },

    #[error("activation group key is invalid: {value}")]
    ActivationGroupKeyInvalid { value: String },

    #[error("activation probability is outside 0..=100: {value}")]
    ActivationProbabilityInvalid { value: u8 },

    #[error("{subject} digest failed: {message}")]
    DigestFailed { subject: &'static str, message: String },
}
