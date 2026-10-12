use crate::core::{BoundedText, CoreError};
use serde::{Deserialize, Serialize};
use std::fmt::Display;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RetrievalHint(BoundedText);

impl RetrievalHint {
    pub const MAX_BYTES: usize = 256;
    pub const LIMIT_NAME: &str = "retrieval_hint";

    pub fn try_new(value: impl Into<String>) -> Result<Self, CoreError> {
        let value = value.into();
        let trim_value = value.trim();
        if trim_value.is_empty() {
            return Err(CoreError::RetrievalHintEmpty);
        }

        let bounded_text =
            BoundedText::try_new(trim_value, Self::LIMIT_NAME, Self::MAX_BYTES).map_err(|e| match e {
                CoreError::BoundedTextLimitExceeded {
                    limit: Self::LIMIT_NAME,
                    actual,
                    maximum,
                } => CoreError::RetrievalHintTooLong { actual, maximum },
                _ => e,
            })?;

        Ok(Self(bounded_text))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn into_string(self) -> String {
        self.0.into_string()
    }
}

impl TryFrom<String> for RetrievalHint {
    type Error = CoreError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<RetrievalHint> for String {
    fn from(hint: RetrievalHint) -> Self {
        hint.into_string()
    }
}

impl Display for RetrievalHint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.as_str())
    }
}
