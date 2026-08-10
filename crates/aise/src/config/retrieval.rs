use super::error::ConfigError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalConfig {
    pub max_requests: usize,
    pub max_candidate_retrievers: usize,
    pub max_candidates_per_retriever: usize,
    pub max_candidates_total: usize,
    pub max_items_per_audience: usize,
    pub max_tokens_per_audience: u64,
    pub max_total_items: usize,
    pub max_total_tokens: u64,
    pub max_item_bytes: usize,
}

impl Default for RetrievalConfig {
    fn default() -> Self {
        Self {
            max_requests: 16,
            max_candidate_retrievers: 2,
            max_candidates_per_retriever: 32,
            max_candidates_total: 64,
            max_items_per_audience: 10,
            max_tokens_per_audience: 2048,
            max_total_items: 20,
            max_total_tokens: 4096,
            max_item_bytes: 4096,
        }
    }
}

impl RetrievalConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_requests == 0 {
            return Err(ConfigError::Invalid("retrieval.max_requests must be positive".into()));
        }
        if self.max_candidate_retrievers == 0 {
            return Err(ConfigError::Invalid(
                "retrieval.max_candidate_retrievers must be positive".into(),
            ));
        }
        if self.max_candidate_retrievers != 2 {
            return Err(ConfigError::Invalid("retrieval.max_candidate_retrievers must be 2".into()));
        }
        if self.max_candidates_per_retriever == 0 {
            return Err(ConfigError::Invalid(
                "retrieval.max_candidates_per_retriever must be positive".into(),
            ));
        }
        if self.max_candidates_total == 0 {
            return Err(ConfigError::Invalid("retrieval.max_candidates_total must be positive".into()));
        }
        if self.max_candidates_total < self.max_candidates_per_retriever {
            return Err(ConfigError::Invalid(
                "retrieval.max_candidates_total must be >= retrieval.max_candidates_per_retriever".into(),
            ));
        }
        if self.max_items_per_audience == 0 {
            return Err(ConfigError::Invalid("retrieval.max_items_per_audience must be positive".into()));
        }
        if self.max_candidates_total < self.max_items_per_audience {
            return Err(ConfigError::Invalid(
                "retrieval.max_candidates_total must be >= retrieval.max_items_per_audience".into(),
            ));
        }
        if self.max_tokens_per_audience == 0 {
            return Err(ConfigError::Invalid(
                "retrieval.max_tokens_per_audience must be positive".into(),
            ));
        }
        if self.max_total_items == 0 {
            return Err(ConfigError::Invalid("retrieval.max_total_items must be positive".into()));
        }
        if self.max_total_items < self.max_items_per_audience {
            return Err(ConfigError::Invalid(
                "retrieval.max_total_items must be >= retrieval.max_items_per_audience".into(),
            ));
        }
        if self.max_total_tokens == 0 {
            return Err(ConfigError::Invalid("retrieval.max_total_tokens must be positive".into()));
        }
        if self.max_total_tokens < self.max_tokens_per_audience {
            return Err(ConfigError::Invalid(
                "retrieval.max_total_tokens must be >= retrieval.max_tokens_per_audience".into(),
            ));
        }
        if self.max_item_bytes == 0 {
            return Err(ConfigError::Invalid("retrieval.max_item_bytes must be positive".into()));
        }
        Ok(())
    }
}
