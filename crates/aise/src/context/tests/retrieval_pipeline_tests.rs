use super::*;
use crate::config::{ActivationConfig, RetrievalConfig};
use crate::context::activation::KnowledgeActivationCoordinator;
use crate::domain::knowledge::activation::ActivationIndexMetadata;
use crate::domain::story_instance::snapshot::KnowledgeSnapshotRef;
use crate::persistence::activation_index_port::ActivationIndexPort;
use crate::persistence::activation_timed_state_port::{ActivationTimedStateQuery, ActivationTimedStateReadPort};
use crate::persistence::knowledge_read_port::{
    KnowledgeIndexQuery, KnowledgeIndexRecord, KnowledgeReadPort, KnowledgeRecord, OwnerMemoryQuery,
    SourceKnowledgeQuery,
};
use crate::persistence::store::StoreError;
use async_trait::async_trait;
use std::sync::Arc;

struct EmptyKnowledge;

#[async_trait]
impl KnowledgeReadPort for EmptyKnowledge {
    async fn find_by_source_ids(&self, _query: SourceKnowledgeQuery<'_>) -> Result<Vec<KnowledgeRecord>, StoreError> {
        Ok(Vec::new())
    }

    async fn find_memories_by_owner(&self, _query: OwnerMemoryQuery<'_>) -> Result<Vec<KnowledgeRecord>, StoreError> {
        Ok(Vec::new())
    }

    async fn list_index(&self, _query: KnowledgeIndexQuery<'_>) -> Result<Vec<KnowledgeIndexRecord>, StoreError> {
        Ok(Vec::new())
    }
}

struct EmptyIndex;

#[async_trait]
impl ActivationIndexPort for EmptyIndex {
    async fn load_snapshot(
        &self,
        knowledge: &KnowledgeSnapshotRef,
        _limits: crate::domain::knowledge::activation::ActivationIndexLimits,
    ) -> Result<Arc<ActivationIndexMetadata>, StoreError> {
        Ok(Arc::new(ActivationIndexMetadata {
            reference: crate::domain::knowledge::activation::ActivationIndexSnapshotRef::from_knowledge(
                knowledge, 0, 1,
            ),
            pack_entries: std::collections::BTreeMap::new(),
            entries: std::collections::BTreeMap::new(),
        }))
    }
}

struct EmptyTimed;

#[async_trait]
impl ActivationTimedStateReadPort for EmptyTimed {
    async fn load_timed_state(
        &self,
        _query: ActivationTimedStateQuery<'_>,
    ) -> Result<Vec<crate::domain::knowledge::activation::ActivationTimedState>, StoreError> {
        Ok(Vec::new())
    }
}

#[test]
fn retrieval_pipeline_constructs_with_activation_coordinator() {
    let knowledge: Arc<dyn KnowledgeReadPort> = Arc::new(EmptyKnowledge);
    let activation = ActivationConfig::default();
    let coordinator = Arc::new(KnowledgeActivationCoordinator::new(
        knowledge,
        Arc::new(EmptyIndex),
        Arc::new(EmptyTimed),
        activation.domain_index_limits(),
        activation.rule,
        activation.domain_runtime_limits(),
        activation.cache,
    ));
    let pipeline = ContextRetrievalPipeline::new(RetrievalConfig::default(), activation, coordinator);
    assert_eq!(pipeline.stage(), TurnStage::ContextRetrieval);
}
