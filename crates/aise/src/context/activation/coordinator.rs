use crate::config::{ActivationIndexLimits, ActivationRuleLimitsConfig};
use crate::domain::ids::{StoryId, TurnNumber};
use crate::domain::knowledge::activation::GenerationTrigger;
use crate::domain::knowledge::activation::{
    ActivationContinuation, ActivationEntryInput, ActivationExecutionInput, ActivationIndexSnapshot,
    ActivationMacroValues, ActivationRequest, ActivationResult, ActivationRunMode, ActivationRuntimeLimits,
    ActivationScanBuffer, ExternalActivationSeed, KnowledgeActivationEngine,
};
use crate::domain::knowledge::{KnowledgeKind, KnowledgeSourceId};
use crate::domain::story_instance::snapshot::KnowledgeSnapshotRef;
use crate::domain::text::estimate_text_tokens;
use crate::domain::turn::KnowledgeDelivery;
use crate::persistence::knowledge_read_port::{KnowledgeFilter, SourceKnowledgeQuery};
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
    rule_limits: ActivationRuleLimitsConfig,
    runtime_limits: ActivationRuntimeLimits,
}

impl KnowledgeActivationCoordinator {
    pub fn new(
        knowledge: Arc<dyn KnowledgeReadPort>,
        index: Arc<dyn ActivationIndexPort>,
        timed_state: Arc<dyn ActivationTimedStateReadPort>,
        index_limits: ActivationIndexLimits,
        rule_limits: ActivationRuleLimitsConfig,
        runtime_limits: ActivationRuntimeLimits,
    ) -> Self {
        Self {
            knowledge,
            index,
            timed_state,
            engine: KnowledgeActivationEngine,
            index_limits,
            rule_limits,
            runtime_limits,
        }
    }

    pub async fn run(
        &self,
        snapshot: &KnowledgeSnapshotRef,
        scan_buffer: &ActivationScanBuffer,
        macros: ActivationMacroValues,
        story_id: &StoryId,
        turn_number: TurnNumber,
        generation_trigger: GenerationTrigger,
        mode: ActivationRunMode,
        external_seeds: &[ExternalActivationSeed],
        continuation: Option<ActivationContinuation>,
    ) -> Result<ActivationResult, StoreError> {
        let index = self.prepare_index(snapshot, macros).await?;
        let timed_state = self
            .timed_state
            .load_timed_state(ActivationTimedStateQuery {
                snapshot,
                limit: self.runtime_limits.max_activated_entries,
            })
            .await?;
        let request = ActivationRequest {
            story_id,
            turn_number,
            generation_trigger,
            mode,
            knowledge_snapshot: snapshot,
            index_snapshot: &index,
            scan_buffer,
            timed_state: &timed_state,
            external_seeds,
            continuation,
            limits: self.runtime_limits,
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

    pub async fn prepare_index(
        &self,
        snapshot: &KnowledgeSnapshotRef,
        macros: ActivationMacroValues,
    ) -> Result<ActivationIndexSnapshot, StoreError> {
        let index = self.index.load_snapshot(snapshot, self.index_limits).await?;
        let source_ids = index.metadata.keys().cloned().collect::<Vec<_>>();
        let filter = KnowledgeFilter {
            delivery: KnowledgeDelivery::Writer,
            knowledge_kinds: vec![KnowledgeKind::Fact, KnowledgeKind::Rumor],
            max_item_bytes: self.runtime_limits.max_single_entry_bytes,
        };
        let records = if source_ids.is_empty() {
            Vec::new()
        } else {
            self.knowledge
                .find_by_source_ids(SourceKnowledgeQuery {
                    snapshot,
                    filter: &filter,
                    source_ids: &source_ids,
                    limit: source_ids.len(),
                })
                .await?
        };
        if records.len() != source_ids.len() {
            return Err(StoreError::ConstraintViolation {
                constraint: "activation_execution_body_incomplete".into(),
            });
        }
        let entries = records
            .into_iter()
            .map(|record| ActivationEntryInput {
                source_id: record.source_id,
                deliveries: vec![KnowledgeDelivery::Writer],
                token_cost: estimate_text_tokens(record.content.as_str()),
                body: record.content,
            })
            .collect::<Vec<_>>();
        let max_total_body_bytes = self
            .index_limits
            .max_entries
            .saturating_mul(self.runtime_limits.max_single_entry_bytes);
        let execution_input = ActivationExecutionInput::try_new(
            entries,
            macros,
            self.index_limits.max_entries,
            max_total_body_bytes.max(1),
            self.rule_limits.max_macro_value_bytes,
            self.rule_limits.max_macro_expansion_bytes,
        )
        .map_err(|error| StoreError::ConstraintViolation {
            constraint: error.to_string(),
        })?;
        Ok((*index).clone().with_execution_input(execution_input))
    }

    pub fn runtime_limits(&self) -> ActivationRuntimeLimits {
        self.runtime_limits
    }

    pub fn knowledge(&self) -> &Arc<dyn KnowledgeReadPort> {
        &self.knowledge
    }

    pub fn authorize_seed(
        &self,
        index: &ActivationIndexSnapshot,
        source_id: &KnowledgeSourceId,
        delivery: &KnowledgeDelivery,
    ) -> bool {
        let Some(metadata) = index.metadata.get(source_id) else {
            return false;
        };
        match delivery {
            KnowledgeDelivery::Writer => matches!(metadata.kind, KnowledgeKind::Fact | KnowledgeKind::Rumor),
            KnowledgeDelivery::Character { .. } => metadata.kind == KnowledgeKind::Rumor,
        }
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
