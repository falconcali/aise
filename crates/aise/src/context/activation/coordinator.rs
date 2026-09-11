use crate::config::ActivationIndexLimits;
use crate::domain::knowledge::activation::GenerationTrigger;
use crate::domain::knowledge::activation::{
    ActivationRequest, ActivationResult, ActivationRuntimeLimits, ActivationScanBuffer, KnowledgeActivationEngine,
};
use crate::domain::story_instance::snapshot::KnowledgeSnapshotRef;
use crate::persistence::{
    ActivationIndexPort, ActivationTimedStateQuery, ActivationTimedStateReadPort, KnowledgeReadPort, StoreError,
};
use async_trait::async_trait;
use std::sync::Arc;

pub struct KnowledgeActivationCoordinator {
    knowledge: Arc<dyn KnowledgeReadPort>,
    index: Arc<dyn ActivationIndexPort>,
    timed_state: Arc<dyn ActivationTimedStateReadPort>,
    engine: KnowledgeActivationEngine,
    index_limits: ActivationIndexLimits,
    runtime_limits: ActivationRuntimeLimits,
}

impl KnowledgeActivationCoordinator {
    pub fn new(
        knowledge: Arc<dyn KnowledgeReadPort>,
        index: Arc<dyn ActivationIndexPort>,
        timed_state: Arc<dyn ActivationTimedStateReadPort>,
        index_limits: ActivationIndexLimits,
        runtime_limits: ActivationRuntimeLimits,
    ) -> Self {
        Self {
            knowledge,
            index,
            timed_state,
            engine: KnowledgeActivationEngine,
            index_limits,
            runtime_limits,
        }
    }

    pub async fn run(
        &self,
        snapshot: &KnowledgeSnapshotRef,
        scan_buffer: &ActivationScanBuffer,
        generation_trigger: GenerationTrigger,
        request: ActivationRequest<'_>,
    ) -> Result<ActivationResult, StoreError> {
        let index = self.index.load_snapshot(snapshot, self.index_limits).await?;
        let timed_state = self
            .timed_state
            .load_timed_state(ActivationTimedStateQuery {
                snapshot,
                limit: self.runtime_limits.max_activated_entries,
            })
            .await?;
        let request = ActivationRequest {
            story_id: request.story_id,
            turn_number: request.turn_number,
            generation_trigger,
            mode: request.mode,
            knowledge_snapshot: snapshot,
            index_snapshot: &index,
            scan_buffer,
            timed_state: &timed_state,
            external_seeds: request.external_seeds,
            continuation: request.continuation,
            limits: request.limits,
        };
        self.run_prepared(request).map_err(|error| StoreError::ConstraintViolation {
            constraint: error.to_string(),
        })
    }

    pub fn run_prepared(
        &self,
        request: ActivationRequest<'_>,
    ) -> Result<ActivationResult, crate::domain::knowledge::activation::ActivationError> {
        self.engine.run(request)
    }

    pub fn runtime_limits(&self) -> ActivationRuntimeLimits {
        self.runtime_limits
    }

    pub fn knowledge(&self) -> &Arc<dyn KnowledgeReadPort> {
        &self.knowledge
    }
}

#[async_trait]
pub trait ActivationBodyLoader: Send + Sync {
    async fn load_activated(
        &self,
        snapshot: &KnowledgeSnapshotRef,
        result: &ActivationResult,
    ) -> Result<(), StoreError>;
}
