use crate::domain::asset::validation::{AssetValidationCode, AssetValidationError, BoundedText};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RetrievalHintError {
    #[error("retrieval_hint must not be empty")]
    Empty,
    #[error("retrieval_hint exceeds {maximum} bytes: actual {actual}")]
    TooLong { actual: u64, maximum: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RetrievalHint(BoundedText);

impl RetrievalHint {
    pub const MAX_BYTES: usize = 256;

    pub fn try_new(value: impl Into<String>) -> Result<Self, RetrievalHintError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(RetrievalHintError::Empty);
        }
        let bounded = BoundedText::try_new(value, "retrieval_hint", Self::MAX_BYTES).map_err(|error| match error {
            AssetValidationError::LimitExceeded { actual, maximum, .. } => {
                RetrievalHintError::TooLong { actual, maximum }
            }
            AssetValidationError::Invalid { .. } => RetrievalHintError::Empty,
        })?;
        Ok(Self(bounded))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl TryFrom<String> for RetrievalHint {
    type Error = RetrievalHintError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<RetrievalHint> for String {
    fn from(value: RetrievalHint) -> Self {
        value.0.as_str().to_owned()
    }
}

impl std::fmt::Display for RetrievalHint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.as_str())
    }
}

pub fn normalize_static_retrieval_hint(
    content: &BoundedText,
    configured: Option<RetrievalHint>,
) -> Result<RetrievalHint, AssetValidationError> {
    if let Some(hint) = configured {
        return Ok(hint);
    }
    if content.as_str().len() <= RetrievalHint::MAX_BYTES {
        return RetrievalHint::try_new(content.as_str()).map_err(|_| AssetValidationError::Invalid {
            code: AssetValidationCode::RetrievalHintRequired,
            path: "retrieval_hint".to_owned(),
        });
    }
    Err(AssetValidationError::Invalid {
        code: AssetValidationCode::RetrievalHintRequired,
        path: "retrieval_hint".to_owned(),
    })
}

#[cfg(test)]
#[path = "tests/hint_tests.rs"]
mod tests;
