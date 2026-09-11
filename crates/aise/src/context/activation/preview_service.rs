use super::coordinator::KnowledgeActivationCoordinator;
use super::preview::{
    ActivationPreviewError, ActivationPreviewLimits, ActivationPreviewResult, ActivationPreviewSpec, project_preview,
};
use crate::config::{
    ActivationConfig, AssetLimitsConfig, ContextPreparationConfig, NarrativeConfig, TurnContentLimitsConfig,
};
use crate::domain::asset::validation::BoundedText;
use crate::domain::knowledge::activation::{ActivationRequest, ActivationRunMode, ExternalActivationSeed};
use crate::domain::turn::KnowledgeDelivery;
use crate::persistence::{
    ActivationIndexPort, ActivationTimedStateQuery, ActivationTimedStateReadPort, KnowledgeReadPort, Store, StoreError,
};
use std::sync::Arc;

pub struct KnowledgeActivationPreviewService {
    store: Arc<dyn Store>,
    index: Arc<dyn ActivationIndexPort>,
    timed_state: Arc<dyn ActivationTimedStateReadPort>,
    coordinator: Arc<KnowledgeActivationCoordinator>,
    content_limits: TurnContentLimitsConfig,
    context_config: ContextPreparationConfig,
    asset_limits: AssetLimitsConfig,
    narrative_config: NarrativeConfig,
    activation_config: ActivationConfig,
    preview_limits: ActivationPreviewLimits,
}

#[derive(Debug, Clone)]
pub struct ActivationPreviewServiceConfig {
    pub content_limits: TurnContentLimitsConfig,
    pub context_config: ContextPreparationConfig,
    pub asset_limits: AssetLimitsConfig,
    pub narrative_config: NarrativeConfig,
    pub activation_config: ActivationConfig,
    pub preview_limits: ActivationPreviewLimits,
}

impl KnowledgeActivationPreviewService {
    pub fn new(
        store: Arc<dyn Store>,
        knowledge: Arc<dyn KnowledgeReadPort>,
        index: Arc<dyn ActivationIndexPort>,
        timed_state: Arc<dyn ActivationTimedStateReadPort>,
        config: ActivationPreviewServiceConfig,
    ) -> Self {
        let coordinator = KnowledgeActivationCoordinator::new(
            knowledge,
            index.clone(),
            timed_state.clone(),
            config.activation_config.index,
            runtime_limits(&config.activation_config),
        );
        Self {
            store,
            index,
            timed_state,
            coordinator: Arc::new(coordinator),
            content_limits: config.content_limits,
            context_config: config.context_config,
            asset_limits: config.asset_limits,
            narrative_config: config.narrative_config,
            activation_config: config.activation_config,
            preview_limits: config.preview_limits,
        }
    }

    pub async fn preview(
        &self,
        spec: ActivationPreviewSpec,
    ) -> Result<ActivationPreviewResult, ActivationPreviewError> {
        if spec.player_contribution.len() > self.preview_limits.max_player_contribution_bytes
            || spec.external_targets.len() > self.preview_limits.max_external_targets
            || matches!(
                spec.generation_trigger,
                crate::domain::knowledge::activation::GenerationTrigger::Repair
            )
        {
            return Err(ActivationPreviewError::InvalidLimits);
        }
        let snapshot_limits = crate::domain::turn::SnapshotLimits::from_config(
            &self.content_limits,
            &self.context_config,
            &self.asset_limits,
            &self.narrative_config,
        );
        let snapshot = self
            .store
            .load_story_snapshot(&spec.story_id, snapshot_limits)
            .await
            .map_err(ActivationPreviewError::Store)?;
        let info = self
            .store
            .get_story(&spec.story_id)
            .await
            .map_err(ActivationPreviewError::Store)?
            .ok_or(ActivationPreviewError::Store(StoreError::NotFound))?;
        let evaluated_turn_number = info
            .last_committed_turn_number
            .checked_add(1)
            .ok_or(ActivationPreviewError::InvalidLimits)?;
        let contribution = BoundedText::try_new(
            spec.player_contribution,
            "player_contribution",
            self.preview_limits.max_player_contribution_bytes,
        )
        .map_err(|_| ActivationPreviewError::InvalidLimits)?;
        let scan_buffer = crate::domain::knowledge::activation::ActivationScanBuffer::try_new(
            vec![crate::domain::knowledge::activation::ScanFragment::new(
                crate::domain::knowledge::activation::ScanFragmentKind::PlayerContribution,
                0,
                0,
                contribution,
            )],
            self.activation_config.runtime.max_scan_fragments,
            self.activation_config.runtime.max_scan_bytes,
        )
        .map_err(|_| ActivationPreviewError::InvalidLimits)?;
        let index = self
            .index
            .load_snapshot(snapshot.knowledge_snapshot(), self.activation_config.index)
            .await
            .map_err(ActivationPreviewError::Store)?;
        let timed_state = self
            .timed_state
            .load_timed_state(ActivationTimedStateQuery {
                snapshot: snapshot.knowledge_snapshot(),
                limit: self.activation_config.runtime.max_activated_entries,
            })
            .await
            .map_err(ActivationPreviewError::Store)?;
        let mut seeds = Vec::with_capacity(spec.external_targets.len());
        for target in spec.external_targets {
            let Some(metadata) = index.metadata.get(&target.source_id) else {
                return Err(ActivationPreviewError::UnauthorizedTarget);
            };
            if !authorized_delivery(metadata.kind, &target.delivery) {
                return Err(ActivationPreviewError::UnauthorizedTarget);
            }
            seeds.push(ExternalActivationSeed {
                source_id: target.source_id,
                delivery: target.delivery,
                kind: crate::domain::knowledge::activation::ActivationSeedKind::PreviewOverride,
                provider_rank: None,
                mandatory: target.mandatory,
            });
        }
        let result = self
            .coordinator
            .run_prepared(ActivationRequest {
                story_id: &spec.story_id,
                turn_number: crate::domain::ids::TurnNumber::try_new(evaluated_turn_number)
                    .map_err(|_| ActivationPreviewError::InvalidLimits)?,
                generation_trigger: spec.generation_trigger,
                mode: ActivationRunMode::Preview,
                knowledge_snapshot: snapshot.knowledge_snapshot(),
                index_snapshot: &index,
                scan_buffer: &scan_buffer,
                timed_state: &timed_state,
                external_seeds: &seeds,
                continuation: None,
                limits: runtime_limits(&self.activation_config),
            })
            .map_err(ActivationPreviewError::Activation)?;
        project_preview(
            snapshot.knowledge_snapshot(),
            evaluated_turn_number,
            index.reference.overlay_version,
            spec.generation_trigger,
            &result,
            &index,
            self.preview_limits,
        )
    }
}

fn authorized_delivery(kind: crate::domain::knowledge::KnowledgeKind, delivery: &KnowledgeDelivery) -> bool {
    match delivery {
        KnowledgeDelivery::Writer => matches!(
            kind,
            crate::domain::knowledge::KnowledgeKind::Fact | crate::domain::knowledge::KnowledgeKind::Rumor
        ),
        KnowledgeDelivery::Character { .. } => kind == crate::domain::knowledge::KnowledgeKind::Rumor,
    }
}

fn runtime_limits(config: &ActivationConfig) -> crate::domain::knowledge::activation::ActivationRuntimeLimits {
    let source = config.runtime;
    crate::domain::knowledge::activation::ActivationRuntimeLimits {
        minimum_activations: source.minimum_activations,
        initial_scan_depth: source.initial_scan_depth,
        max_scan_depth: source.max_scan_depth,
        include_summary_at_max_depth: source.include_summary_at_max_depth,
        max_scan_fragments: source.max_scan_fragments,
        max_scan_bytes: source.max_scan_bytes,
        max_scan_tokens: source.max_scan_tokens,
        max_literal_patterns: source.max_literal_patterns,
        max_regex_patterns: source.max_regex_patterns,
        max_pattern_matches: source.max_pattern_matches,
        max_candidates_per_round: source.max_candidates_per_round,
        max_recursion_steps: source.max_recursion_steps,
        max_recursion_fragments: source.max_recursion_fragments,
        max_recursion_bytes: source.max_recursion_bytes,
        max_recursion_tokens: source.max_recursion_tokens,
        max_activated_entries: source.max_activated_entries,
        max_depth_expansions: source.max_depth_expansions,
        max_external_candidates: source.max_external_candidates,
        max_evidence_per_entry: source.max_evidence_per_entry,
        max_evidence_bytes: source.max_evidence_bytes,
        max_items_per_audience: source.max_items_per_audience,
        max_tokens_per_audience: source.max_tokens_per_audience,
        max_total_items: source.max_total_items,
        max_total_tokens: source.max_total_tokens,
        max_single_entry_bytes: source.max_single_entry_bytes,
        reserved_tokens: source.reserved_tokens,
        mandatory_tokens: source.mandatory_tokens,
    }
}

impl From<StoreError> for ActivationPreviewError {
    fn from(value: StoreError) -> Self {
        Self::Store(value)
    }
}
