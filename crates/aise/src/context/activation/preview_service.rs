use super::coordinator::KnowledgeActivationCoordinator;
use super::preview::{
    ActivationPreviewError, ActivationPreviewLimits, ActivationPreviewResult, ActivationPreviewSpec, project_preview,
};
use crate::config::{
    ActivationConfig, AssetLimitsConfig, ContextPreparationConfig, NarrativeConfig, TurnContentLimitsConfig,
};
use crate::domain::asset::validation::BoundedText;
use crate::domain::knowledge::activation::{
    ActivationMacroValues, ActivationRequest, ActivationRunMode, ActivationScanBuffer, ExternalActivationSeed,
    ScanFragment, ScanFragmentKind,
};
use crate::persistence::{
    ActivationIndexPort, ActivationTimedStateQuery, ActivationTimedStateReadPort, KnowledgeReadPort, Store, StoreError,
};
use std::sync::Arc;

pub struct KnowledgeActivationPreviewService {
    store: Arc<dyn Store>,
    coordinator: Arc<KnowledgeActivationCoordinator>,
    timed_state: Arc<dyn ActivationTimedStateReadPort>,
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
            index,
            timed_state.clone(),
            config.activation_config.domain_index_limits(),
            config.activation_config.rule,
            config.activation_config.domain_runtime_limits(),
            config.activation_config.cache,
        );
        Self {
            store,
            coordinator: Arc::new(coordinator),
            timed_state,
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
        let turn_number = crate::domain::ids::TurnNumber::try_new(evaluated_turn_number)
            .map_err(|_| ActivationPreviewError::InvalidLimits)?;
        let contribution = BoundedText::try_new(
            spec.player_contribution,
            "player_contribution",
            self.preview_limits.max_player_contribution_bytes,
        )
        .map_err(|_| ActivationPreviewError::InvalidLimits)?;
        let scan_buffer = ActivationScanBuffer::try_new(
            vec![ScanFragment::new(
                ScanFragmentKind::PlayerContribution,
                0,
                0,
                contribution,
            )],
            self.activation_config.runtime.max_scan_fragments,
            self.activation_config.runtime.max_scan_bytes,
        )
        .map_err(|_| ActivationPreviewError::InvalidLimits)?;
        let player = snapshot
            .role(snapshot.player_role_id())
            .ok_or(ActivationPreviewError::InvalidLimits)?;
        let macros = ActivationMacroValues {
            player_name: player.effective_profile.name.as_str().to_owned(),
            player_role_label: player.role_label.as_str().to_owned(),
        };
        let index = self
            .coordinator
            .prepare_index_for_run(
                snapshot.knowledge_snapshot(),
                &macros,
                turn_number,
                spec.generation_trigger,
                ActivationRunMode::Preview,
            )
            .await
            .map_err(ActivationPreviewError::Activation)?;
        let macro_digest = crate::domain::knowledge::activation::macro_digest(&macros);
        let fragment_matches = self
            .coordinator
            .match_fragments(&spec.story_id, &index, &macro_digest, &scan_buffer);
        let timed_state = self
            .timed_state
            .load_timed_state(ActivationTimedStateQuery {
                snapshot: snapshot.knowledge_snapshot(),
                limit: self.activation_config.index.max_entries,
            })
            .await
            .map_err(ActivationPreviewError::Store)?;
        let mut seeds = Vec::with_capacity(spec.external_targets.len());
        for target in spec.external_targets {
            if !self.coordinator.authorize_seed(&index, &target.source_id, &target.delivery) {
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
        let turn_number = crate::domain::ids::TurnNumber::try_new(evaluated_turn_number)
            .map_err(|_| ActivationPreviewError::InvalidLimits)?;
        let result = self
            .coordinator
            .drive(
                ActivationRequest {
                    story_id: &spec.story_id,
                    turn_number,
                    generation_trigger: spec.generation_trigger,
                    mode: ActivationRunMode::Preview,
                    knowledge_snapshot: snapshot.knowledge_snapshot(),
                    index_snapshot: &index,
                    scan_buffer: &scan_buffer,
                    fragment_matches: &fragment_matches,
                    timed_state: &timed_state,
                    external_seeds: &seeds,
                    continuation: None,
                    limits: self.activation_config.domain_runtime_limits(),
                },
                snapshot.knowledge_snapshot(),
            )
            .await
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

impl From<StoreError> for ActivationPreviewError {
    fn from(value: StoreError) -> Self {
        Self::Store(value)
    }
}
