use crate::config::RetrievalConfig;
use crate::context::error::ContextError;
use crate::context::retrieval_pipeline::ContextRetrievalPipeline;
use crate::context::{EntityCandidateRetriever, TopicCandidateRetriever};
use crate::persistence::knowledge_read_port::{
    EntityKnowledgeQuery, KnowledgeReadPort, KnowledgeRecord, TopicKnowledgeQuery,
};
use crate::persistence::store::StoreError;
use async_trait::async_trait;
use std::sync::Arc;

struct EmptyKnowledge;

#[async_trait]
impl KnowledgeReadPort for EmptyKnowledge {
    async fn find_by_entities(&self, _query: EntityKnowledgeQuery<'_>) -> Result<Vec<KnowledgeRecord>, StoreError> {
        Ok(Vec::new())
    }

    async fn find_by_topics(&self, _query: TopicKnowledgeQuery<'_>) -> Result<Vec<KnowledgeRecord>, StoreError> {
        Ok(Vec::new())
    }
}

#[test]
fn retrieval_pipeline_requires_entity_and_topic_retrievers() {
    let knowledge: Arc<dyn KnowledgeReadPort> = Arc::new(EmptyKnowledge);
    let err = ContextRetrievalPipeline::new(RetrievalConfig::default(), Vec::new());
    assert!(matches!(err, Err(ContextError::InvalidRetrieverSet { .. })));
    let ok = ContextRetrievalPipeline::new(
        RetrievalConfig::default(),
        vec![
            Arc::new(EntityCandidateRetriever::new(knowledge.clone())),
            Arc::new(TopicCandidateRetriever::new(knowledge)),
        ],
    );
    assert!(ok.is_ok());
}
