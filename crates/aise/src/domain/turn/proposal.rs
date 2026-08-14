use crate::domain::asset::ids::NarrativeConditionKey;
use crate::domain::narrative_graph::condition::NarrativeTruthValue;
use crate::domain::narrative_graph::resolver::ProposedNarrativeTransition;
use crate::domain::narrative_graph::state::PendingNarrativeEffect;
use crate::domain::turn::extraction::StoryCandidateVersion;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct ValidatedNarrativeResolution {
    pub candidate_version: StoryCandidateVersion,
    pub transitions: Vec<ProposedNarrativeTransition>,
    pub condition_results: BTreeMap<NarrativeConditionKey, NarrativeTruthValue>,
    pub pending_effects: Vec<PendingNarrativeEffect>,
    pub next_graph_revision: u64,
}
