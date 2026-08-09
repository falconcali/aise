use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StorySequence(u64);

impl StorySequence {
    pub fn try_new(value: u64) -> Result<Self, StoryContinuityError> {
        if value == 0 {
            return Err(StoryContinuityError::ZeroSequence);
        }
        Ok(Self(value))
    }

    pub fn get(self) -> u64 {
        self.0
    }

    pub fn next(self) -> Result<Self, StoryContinuityError> {
        let next = self.0.checked_add(1).ok_or(StoryContinuityError::SequenceOverflow)?;
        Self::try_new(next)
    }
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum StoryContinuityError {
    #[error("story sequence must be positive")]
    ZeroSequence,
    #[error("story sequence overflow")]
    SequenceOverflow,
    #[error("story summary text and summarized_through must either both be present or both be absent")]
    InvalidSummaryBoundary,
    #[error("recent story segments are not strictly ordered")]
    OutOfOrder,
    #[error("story summary and recent story overlap")]
    Overlap,
    #[error("story continuity contains a sequence gap")]
    Gap,
    #[error("story continuity limit exceeded: {limit}")]
    LimitExceeded { limit: &'static str },
}
