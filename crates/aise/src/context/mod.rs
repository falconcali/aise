pub mod baseline_ctx_builder;
pub mod candidate_retriever;
pub mod entity_candidate_retriever;
pub mod error;
pub mod retrieval_pipeline;
pub mod retrieval_signal_builder;
pub mod topic_candidate_retriever;

pub use crate::domain::asset::text_matcher::TextMatcher;
pub use baseline_ctx_builder::BaselineContextBuilder;
pub use candidate_retriever::{CandidateRetrievalRequest, CandidateRetriever, ContextCandidate};
pub use entity_candidate_retriever::EntityCandidateRetriever;
pub use error::ContextError;
pub use retrieval_pipeline::ContextRetrievalPipeline;
pub use retrieval_signal_builder::RetrievalSignalBuilder;
pub use topic_candidate_retriever::TopicCandidateRetriever;

#[cfg(test)]
#[path = "tests/retrieval_pipeline_tests.rs"]
mod tests;
