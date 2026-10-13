use crate::core::{CoreError, Sha256Digest};
use serde::Serialize;
use sha2::{Digest, Sha256};

impl Sha256Digest {
    pub fn canonical<T: Serialize>(subject: &'static str, value: &T) -> Result<Self, CoreError> {
        let canonical = serde_json::to_vec(value).map_err(|error| CoreError::DigestFailed {
            subject,
            message: error.to_string(),
        })?;
        Self::try_new(format!("{:x}", Sha256::digest(&canonical))).map_err(|error| CoreError::DigestFailed {
            subject,
            message: error.to_string(),
        })
    }
}
