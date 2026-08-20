use crate::config::{PlannerConfig, RetrievalConfig};
use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::ids::TopicKey;
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::RoleId;
use crate::domain::knowledge::{KnowledgeKind, KnowledgeSourceId};
use crate::domain::narrative_graph::effect::CharacterImpulse;
use crate::domain::narrative_graph::projector::NarrativePlan;
use crate::domain::story_instance::role::RoleController;
use crate::domain::story_instance::snapshot::StoryReadSnapshot;
use crate::domain::turn::{
    BaselineContext, CharacterRetrievalRequest, CharacterThinkRequest, KnowledgeDelivery, KnowledgeRetrievalRequest,
    RetrievalPlan, RetrievalRequestOrigin, WriterPlan, WriterStoryGoal,
};
use crate::planning::error::PlanningError;
use crate::planning::planner_output::{
    CharacterThinkRequestDto, PlannerCharacterContextGapDto, PlannerWriterContextGapDto, WriterPlannerOutputDto,
};
use crate::planning::writer_planner_prompt::{IndexedRetrievalTarget, WriterPlannerPromptContext};
use std::collections::{BTreeMap, BTreeSet};

pub struct RetrievalPlanBuilder {
    retrieval: RetrievalConfig,
    planner: PlannerConfig,
}

impl RetrievalPlanBuilder {
    pub fn new(retrieval: RetrievalConfig, planner: PlannerConfig) -> Self {
        Self { retrieval, planner }
    }

    pub fn build(
        &self,
        baseline: &BaselineContext,
        narrative_plan: &NarrativePlan,
        planner_output: WriterPlannerOutputDto,
        snapshot: &StoryReadSnapshot,
        prompt_context: &WriterPlannerPromptContext,
    ) -> Result<WriterPlan, PlanningError> {
        let total_gaps = planner_output.writer_context_gaps.len() + planner_output.character_context_gaps.len();
        if total_gaps > self.planner.max_context_gaps {
            return Err(PlanningError::LimitExceeded {
                limit: "max_context_gaps",
            });
        }
        if planner_output.character_think_requests.len() > self.planner.max_character_think_requests {
            return Err(PlanningError::LimitExceeded {
                limit: "max_character_think_requests",
            });
        }
        if planner_output.story_goal.trim().is_empty() {
            return Err(PlanningError::InvalidOutput {
                code: "story_goal_empty",
            });
        }
        let story_goal = BoundedText::try_new(planner_output.story_goal, "story_goal", self.planner.max_goal_bytes)
            .map_err(|_| PlanningError::LimitExceeded {
                limit: "max_goal_bytes",
            })?;

        let think_request_domain = self.convert_think_requests(planner_output.character_think_requests)?;
        let validated_think_requests = self.validate_think_requests(think_request_domain, snapshot)?;
        let think_requests = merge_narrative_think_requests(
            validated_think_requests,
            &narrative_plan.character_impulses,
            baseline,
            &self.planner,
        )?;

        let mut base_cognition: BTreeMap<RoleId, (BoundedText, RetrievalRequestOrigin)> = BTreeMap::new();
        let mut knowledge_requests = Vec::new();
        knowledge_requests.extend(self.narrative_requests(narrative_plan)?);

        for gap in planner_output.writer_context_gaps {
            if let Some(outcome) = self.resolve_writer_gap(gap, prompt_context)? {
                match outcome {
                    GapOutcome::RoleCognition { role_id, reason } => {
                        base_cognition
                            .entry(role_id)
                            .or_insert((reason, RetrievalRequestOrigin::Planner));
                    }
                    GapOutcome::Knowledge(request) => knowledge_requests.push(request),
                }
            }
        }
        for gap in planner_output.character_context_gaps {
            if let Some(request) = self.resolve_character_gap(gap, &think_requests, prompt_context)? {
                knowledge_requests.push(request);
            }
        }
        for request in &think_requests {
            base_cognition
                .entry(request.role_id.clone())
                .or_insert_with(|| (request.reason.clone(), RetrievalRequestOrigin::Automatic));
        }

        let mut character_requests = Vec::new();
        for (role_id, (reason, origin)) in &base_cognition {
            character_requests.push(CharacterRetrievalRequest {
                role_id: role_id.clone(),
                reason: reason.clone(),
                origin: *origin,
            });
            knowledge_requests.push(self.make_request(RequestDraft {
                delivery: KnowledgeDelivery::Character {
                    role_id: role_id.clone(),
                },
                target_source_id: None,
                knowledge_kinds: vec![KnowledgeKind::Rumor, KnowledgeKind::Memory],
                entities: vec![KnowledgeEntity::Role(role_id.clone())],
                topics: Vec::new(),
                reason: reason.as_str(),
                origin: *origin,
                signal_priority: 0,
            })?);
        }
        character_requests.sort_by(|left, right| left.role_id.cmp(&right.role_id));

        let knowledge_requests = dedupe_and_sort(knowledge_requests);
        if knowledge_requests.len() > self.retrieval.max_requests {
            return Err(PlanningError::LimitExceeded { limit: "max_requests" });
        }
        Ok(WriterPlan {
            story_goal: WriterStoryGoal { summary: story_goal },
            retrieval_plan: RetrievalPlan {
                character_requests,
                knowledge_requests,
            },
            character_think_requests: think_requests,
        })
    }

    fn convert_think_requests(
        &self,
        requests: Vec<CharacterThinkRequestDto>,
    ) -> Result<Vec<CharacterThinkRequest>, PlanningError> {
        requests
            .into_iter()
            .map(|dto| {
                let role_id = RoleId::try_new(dto.role_id).map_err(|_| PlanningError::UnknownRole)?;
                if dto.reason.trim().is_empty() {
                    return Err(PlanningError::InvalidOutput {
                        code: "character_think_reason_empty",
                    });
                }
                let reason =
                    BoundedText::try_new(dto.reason, "reason", self.planner.max_reason_bytes).map_err(|_| {
                        PlanningError::LimitExceeded {
                            limit: "max_reason_bytes",
                        }
                    })?;
                Ok(CharacterThinkRequest { role_id, reason })
            })
            .collect()
    }

    fn validate_think_requests(
        &self,
        requests: Vec<CharacterThinkRequest>,
        snapshot: &StoryReadSnapshot,
    ) -> Result<Vec<CharacterThinkRequest>, PlanningError> {
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        for request in requests {
            if &request.role_id == snapshot.player_role_id() {
                return Err(PlanningError::PlayerRoleRequested);
            }
            let role = snapshot.role(&request.role_id).ok_or(PlanningError::UnknownRole)?;
            if !matches!(role.controller, RoleController::Ai) {
                return Err(PlanningError::UnknownRole);
            }
            if !seen.insert(request.role_id.clone()) {
                return Err(PlanningError::DuplicateRoleTarget);
            }
            out.push(request);
        }
        Ok(out)
    }

    fn narrative_requests(
        &self,
        narrative_plan: &NarrativePlan,
    ) -> Result<Vec<KnowledgeRetrievalRequest>, PlanningError> {
        let mut entities = BTreeSet::new();
        for node in &narrative_plan.active_nodes {
            entities.insert(KnowledgeEntity::NarrativeNode(node.clone()));
        }
        let mut requests = Vec::new();
        for entity in entities {
            requests.push(self.make_request(RequestDraft {
                delivery: KnowledgeDelivery::Writer,
                target_source_id: None,
                knowledge_kinds: vec![KnowledgeKind::Fact, KnowledgeKind::Rumor],
                entities: vec![entity],
                topics: Vec::new(),
                reason: "narrative reference",
                origin: RetrievalRequestOrigin::Narrative,
                signal_priority: 2,
            })?);
        }
        Ok(requests)
    }

    fn resolve_writer_gap(
        &self,
        gap: PlannerWriterContextGapDto,
        prompt_context: &WriterPlannerPromptContext,
    ) -> Result<Option<GapOutcome>, PlanningError> {
        if gap.reason.trim().is_empty() {
            return Err(PlanningError::InvalidOutput {
                code: "context_gap_reason_empty",
            });
        }
        let reason = BoundedText::try_new(gap.reason, "reason", self.planner.max_reason_bytes).map_err(|_| {
            PlanningError::LimitExceeded {
                limit: "max_reason_bytes",
            }
        })?;
        let target = match prompt_context.indexed_targets.get(gap.target_id.as_str()) {
            Some(target) => target,
            None => {
                warn_dropped_gap("writer", None, &gap.target_id, &PlanningError::UnknownRetrievalKey);
                return Ok(None);
            }
        };
        match target {
            IndexedRetrievalTarget::Role(role_id) => Ok(Some(GapOutcome::RoleCognition {
                role_id: role_id.clone(),
                reason,
            })),
            IndexedRetrievalTarget::Knowledge(source_id) => {
                let request = self.make_request(RequestDraft {
                    delivery: KnowledgeDelivery::Writer,
                    target_source_id: Some(source_id.clone()),
                    knowledge_kinds: vec![KnowledgeKind::Fact, KnowledgeKind::Rumor],
                    entities: Vec::new(),
                    topics: Vec::new(),
                    reason: reason.as_str(),
                    origin: RetrievalRequestOrigin::Planner,
                    signal_priority: 0,
                })?;
                Ok(Some(GapOutcome::Knowledge(request)))
            }
        }
    }

    fn resolve_character_gap(
        &self,
        gap: PlannerCharacterContextGapDto,
        think_requests: &[CharacterThinkRequest],
        prompt_context: &WriterPlannerPromptContext,
    ) -> Result<Option<KnowledgeRetrievalRequest>, PlanningError> {
        if gap.reason.trim().is_empty() {
            return Err(PlanningError::InvalidOutput {
                code: "context_gap_reason_empty",
            });
        }
        let reason = BoundedText::try_new(gap.reason, "reason", self.planner.max_reason_bytes).map_err(|_| {
            PlanningError::LimitExceeded {
                limit: "max_reason_bytes",
            }
        })?;
        let role_id = match RoleId::try_new(gap.role_id.clone()) {
            Ok(role_id) => role_id,
            Err(_) => {
                warn_dropped_gap("character", Some(&gap.role_id), &gap.target_id, &PlanningError::UnknownRole);
                return Ok(None);
            }
        };
        if !think_requests.iter().any(|request| request.role_id == role_id) {
            warn_dropped_gap(
                "character",
                Some(&gap.role_id),
                &gap.target_id,
                &PlanningError::KnowledgeAudienceViolation,
            );
            return Ok(None);
        }
        let target = match prompt_context.indexed_targets.get(gap.target_id.as_str()) {
            Some(target) => target,
            None => {
                warn_dropped_gap(
                    "character",
                    Some(&gap.role_id),
                    &gap.target_id,
                    &PlanningError::UnknownRetrievalKey,
                );
                return Ok(None);
            }
        };
        let source_id = match target {
            IndexedRetrievalTarget::Knowledge(source_id) => {
                if matches!(source_id, KnowledgeSourceId::Fact(_)) {
                    warn_dropped_gap(
                        "character",
                        Some(&gap.role_id),
                        &gap.target_id,
                        &PlanningError::KnowledgeAudienceViolation,
                    );
                    return Ok(None);
                }
                source_id.clone()
            }
            IndexedRetrievalTarget::Role(_) => {
                warn_dropped_gap(
                    "character",
                    Some(&gap.role_id),
                    &gap.target_id,
                    &PlanningError::KnowledgeAudienceViolation,
                );
                return Ok(None);
            }
        };
        self.make_request(RequestDraft {
            delivery: KnowledgeDelivery::Character { role_id },
            target_source_id: Some(source_id),
            knowledge_kinds: vec![KnowledgeKind::Rumor, KnowledgeKind::Memory],
            entities: Vec::new(),
            topics: Vec::new(),
            reason: reason.as_str(),
            origin: RetrievalRequestOrigin::Planner,
            signal_priority: 0,
        })
        .map(Some)
    }

    fn make_request(&self, draft: RequestDraft<'_>) -> Result<KnowledgeRetrievalRequest, PlanningError> {
        let mut knowledge_kinds = draft.knowledge_kinds;
        let mut entities = draft.entities;
        let mut topics = draft.topics;
        knowledge_kinds.sort();
        knowledge_kinds.dedup();
        entities.sort();
        entities.dedup();
        topics.sort();
        topics.dedup();
        let reason =
            BoundedText::try_new(draft.reason.to_owned(), "reason", self.planner.max_reason_bytes).map_err(|_| {
                PlanningError::LimitExceeded {
                    limit: "max_reason_bytes",
                }
            })?;
        Ok(KnowledgeRetrievalRequest {
            delivery: draft.delivery,
            target_source_id: draft.target_source_id,
            knowledge_kinds,
            entities,
            topics,
            reason,
            origin: draft.origin,
            signal_priority: draft.signal_priority,
        })
    }
}

fn warn_dropped_gap(gap_kind: &'static str, role_id: Option<&str>, target_id: &str, error: &PlanningError) {
    tracing::warn!(
        gap_kind,
        role_id = role_id.unwrap_or(""),
        target_id,
        error_kind = std::any::type_name_of_val(error),
        error = %error,
        "writer planner context gap dropped"
    );
}

enum GapOutcome {
    RoleCognition { role_id: RoleId, reason: BoundedText },
    Knowledge(KnowledgeRetrievalRequest),
}

struct RequestDraft<'a> {
    delivery: KnowledgeDelivery,
    target_source_id: Option<KnowledgeSourceId>,
    knowledge_kinds: Vec<KnowledgeKind>,
    entities: Vec<KnowledgeEntity>,
    topics: Vec<TopicKey>,
    reason: &'a str,
    origin: RetrievalRequestOrigin,
    signal_priority: u8,
}

pub fn merge_narrative_think_requests(
    planner_requests: Vec<CharacterThinkRequest>,
    impulses: &[CharacterImpulse],
    baseline: &BaselineContext,
    config: &PlannerConfig,
) -> Result<Vec<CharacterThinkRequest>, PlanningError> {
    let mut seen: BTreeSet<RoleId> = planner_requests.iter().map(|request| request.role_id.clone()).collect();
    let mut merged = planner_requests;
    let mut additions: BTreeMap<RoleId, BoundedText> = BTreeMap::new();
    for impulse in impulses {
        if seen.contains(&impulse.target_role_id) || additions.contains_key(&impulse.target_role_id) {
            continue;
        }
        if impulse.target_role_id == baseline.player_role.role_id {
            return Err(PlanningError::PlayerRoleRequested);
        }
        if !known_role(baseline, &impulse.target_role_id) {
            return Err(PlanningError::UnknownRole);
        }
        let reason_text = impulse
            .reason
            .as_ref()
            .map(BoundedText::as_str)
            .filter(|reason| !reason.trim().is_empty())
            .unwrap_or_else(|| impulse.goal.as_str());
        let reason = BoundedText::try_new(reason_text.to_owned(), "reason", config.max_reason_bytes).map_err(|_| {
            PlanningError::LimitExceeded {
                limit: "max_reason_bytes",
            }
        })?;
        additions.insert(impulse.target_role_id.clone(), reason);
    }
    for (role_id, reason) in additions {
        seen.insert(role_id.clone());
        merged.push(CharacterThinkRequest { role_id, reason });
    }
    if merged.len() > config.max_character_think_requests {
        return Err(PlanningError::LimitExceeded {
            limit: "max_character_think_requests",
        });
    }
    Ok(merged)
}

fn known_role(baseline: &BaselineContext, role_id: &RoleId) -> bool {
    baseline.player_role.role_id == *role_id
        || baseline.relevant_roles.iter().any(|role| &role.role_id == role_id)
        || baseline.role_index.iter().any(|entry| &entry.role_id == role_id)
}

fn dedupe_and_sort(requests: Vec<KnowledgeRetrievalRequest>) -> Vec<KnowledgeRetrievalRequest> {
    let mut by_key: BTreeMap<RetrievalRequestKey, KnowledgeRetrievalRequest> = BTreeMap::new();
    for request in requests {
        let key = canonical_key(&request);
        match by_key.get(&key) {
            Some(existing) => {
                let replace = request.signal_priority < existing.signal_priority
                    || (request.signal_priority == existing.signal_priority
                        && origin_rank(request.origin) < origin_rank(existing.origin));
                if replace {
                    by_key.insert(key, request);
                }
            }
            None => {
                by_key.insert(key, request);
            }
        }
    }
    let mut out: Vec<_> = by_key.into_values().collect();
    out.sort_by(|left, right| {
        left.signal_priority
            .cmp(&right.signal_priority)
            .then_with(|| origin_rank(left.origin).cmp(&origin_rank(right.origin)))
            .then_with(|| canonical_key(left).cmp(&canonical_key(right)))
    });
    out
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct RetrievalRequestKey {
    delivery: KnowledgeDelivery,
    target_source_id: Option<KnowledgeSourceId>,
    knowledge_kinds: Vec<KnowledgeKind>,
    entities: Vec<KnowledgeEntity>,
    topics: Vec<TopicKey>,
}

fn canonical_key(request: &KnowledgeRetrievalRequest) -> RetrievalRequestKey {
    RetrievalRequestKey {
        delivery: request.delivery.clone(),
        target_source_id: request.target_source_id.clone(),
        knowledge_kinds: request.knowledge_kinds.clone(),
        entities: request.entities.clone(),
        topics: request.topics.clone(),
    }
}

fn origin_rank(origin: RetrievalRequestOrigin) -> u8 {
    match origin {
        RetrievalRequestOrigin::Automatic => 0,
        RetrievalRequestOrigin::Narrative => 1,
        RetrievalRequestOrigin::Planner => 2,
    }
}

#[cfg(test)]
#[path = "tests/retrieval_plan_builder_tests.rs"]
mod tests;
