use crate::domain::ids::StoryId;
use crate::domain::knowledge::activation::{
    ActivationIndexSnapshot, ActivationResult, ActivationSeedKind, ActivationStopReason, ActivationWorkUsage,
    GenerationTrigger,
};
use crate::domain::knowledge::{KnowledgeKind, KnowledgeSourceId};
use crate::domain::story_instance::snapshot::KnowledgeSnapshotRef;
use crate::domain::turn::KnowledgeDelivery;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct ActivationPreviewSpec {
    pub story_id: StoryId,
    pub player_contribution: String,
    pub generation_trigger: GenerationTrigger,
    pub external_targets: Vec<ActivationPreviewTarget>,
}

#[derive(Debug, Clone)]
pub struct ActivationPreviewTarget {
    pub source_id: KnowledgeSourceId,
    pub delivery: KnowledgeDelivery,
    pub mandatory: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationPreviewLimits {
    pub max_player_contribution_bytes: usize,
    pub max_external_targets: usize,
    pub max_response_entries: usize,
    pub max_evidence_per_entry: usize,
    pub max_response_evidence: usize,
    pub max_response_bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivationPreviewResult {
    pub story_id: StoryId,
    pub base_revision: u64,
    pub evaluated_turn_number: u64,
    pub pack_digest: String,
    pub overlay_version: u64,
    pub generation_trigger: GenerationTrigger,
    pub activated: Vec<ActivationPreviewEntry>,
    pub rejection_counts: BTreeMap<String, u32>,
    pub stop_reason: ActivationStopReason,
    pub usage: ActivationWorkUsage,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivationPreviewEntry {
    pub source_id: KnowledgeSourceId,
    pub knowledge_kind: KnowledgeKind,
    pub deliveries: Vec<KnowledgeDelivery>,
    pub activation_class: ActivationSeedKind,
    pub round: u16,
    pub recursion_level: u16,
    pub rank: u32,
    pub token_cost: u64,
    pub evidence: Vec<ActivationPreviewEvidence>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivationPreviewEvidence {
    pub pattern_kind: crate::domain::knowledge::activation::ActivationPatternKind,
    pub pattern_ordinal: u16,
    pub fragment_kind: crate::domain::knowledge::activation::ScanFragmentKind,
    pub recency_depth: u16,
    pub match_count: u16,
    pub group_score_contribution: u16,
}

pub fn project_preview(
    snapshot: &KnowledgeSnapshotRef,
    evaluated_turn_number: u64,
    overlay_version: u64,
    trigger: GenerationTrigger,
    result: &ActivationResult,
    index: &ActivationIndexSnapshot,
    limits: ActivationPreviewLimits,
) -> Result<ActivationPreviewResult, ActivationPreviewError> {
    validate_limits(limits)?;
    let mut truncated = result.activated.len() > limits.max_response_entries;
    let mut total_evidence = 0usize;
    let mut activated = Vec::new();
    for item in result.activated.iter().take(limits.max_response_entries) {
        let Some(metadata) = index.metadata.get(&item.source_id) else {
            return Err(ActivationPreviewError::MissingMetadata);
        };
        let mut evidence = Vec::new();
        for detail in item.evidence.iter().take(limits.max_evidence_per_entry) {
            if total_evidence >= limits.max_response_evidence {
                truncated = true;
                break;
            }
            total_evidence += 1;
            evidence.push(ActivationPreviewEvidence {
                pattern_kind: detail.pattern_kind,
                pattern_ordinal: detail.pattern_ordinal,
                fragment_kind: detail.fragment_kind,
                recency_depth: detail.recency_depth,
                match_count: detail.match_count,
                group_score_contribution: detail.group_score_contribution,
            });
        }
        truncated |= item.evidence.len() > evidence.len();
        activated.push(ActivationPreviewEntry {
            source_id: item.source_id.clone(),
            knowledge_kind: metadata.kind,
            deliveries: item.deliveries.clone(),
            activation_class: item.activation_class,
            round: item.evidence.first().map_or(0, |value| value.round),
            recursion_level: item.evidence.first().map_or(0, |value| value.recursion_level),
            rank: item.rank,
            token_cost: item.token_cost,
            evidence,
        });
    }
    let rejection_counts = result
        .rejection_summary
        .iter()
        .map(|(reason, count)| (serde_json::to_string(reason).unwrap_or_else(|_| "unknown".into()), *count))
        .collect();
    let mut projected = ActivationPreviewResult {
        story_id: snapshot.story_id.clone(),
        base_revision: snapshot.base_revision.get(),
        evaluated_turn_number,
        pack_digest: snapshot.pack_digest.to_string(),
        overlay_version,
        generation_trigger: trigger,
        activated,
        rejection_counts,
        stop_reason: result.stop_reason,
        usage: result.continuation.consumed,
        truncated,
    };
    while serde_json::to_vec(&projected)
        .map_err(|_| ActivationPreviewError::Serialization)?
        .len()
        > limits.max_response_bytes
    {
        let Some(entry) = projected.activated.last_mut() else {
            return Err(ActivationPreviewError::ResponseLimitExceeded);
        };
        if entry.evidence.pop().is_none() {
            return Err(ActivationPreviewError::ResponseLimitExceeded);
        }
        projected.truncated = true;
    }
    Ok(projected)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ActivationPreviewError {
    #[error("activation preview limits must be positive")]
    InvalidLimits,
    #[error("activation preview response limit exceeded")]
    ResponseLimitExceeded,
    #[error("activation preview metadata is missing")]
    MissingMetadata,
    #[error("activation preview response serialization failed")]
    Serialization,
}

fn validate_limits(limits: ActivationPreviewLimits) -> Result<(), ActivationPreviewError> {
    if [
        limits.max_player_contribution_bytes,
        limits.max_external_targets,
        limits.max_response_entries,
        limits.max_evidence_per_entry,
        limits.max_response_evidence,
        limits.max_response_bytes,
    ]
    .into_iter()
    .any(|value| value == 0)
    {
        return Err(ActivationPreviewError::InvalidLimits);
    }
    Ok(())
}
