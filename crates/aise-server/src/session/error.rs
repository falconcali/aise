use thiserror::Error;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("session quota exceeded ({0})")]
    QuotaExceeded(usize),
}
