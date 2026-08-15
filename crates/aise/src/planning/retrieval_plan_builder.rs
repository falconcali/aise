use crate::config::{PlannerConfig, RetrievalConfig};
use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::ids::TopicKey;
use crate::domain::asset::text_matcher::{TextMatcher, normalize_match_text, term_matches};
use crate::domain::asset::validation::BoundedText;
use crate::domain::ids::RoleId;
use crate::domain::knowledge::KnowledgeKind;
use crate::domain::narrative_graph::projector::NarrativePlan;
use crate::domain::story_instance::snapshot::StoryReadSnapshot;
use crate::domain::turn::{
    BaselineContext, CharacterThinkRequest, EntitySignal, RetrievalAudience, RetrievalPlan, RetrievalRequest,
    RetrievalRequestOrigin, WriterPlan, WriterStoryGoal,
};
use crate::planning::error::PlanningError;
use crate::planning::planner_output::PlannerOutput;
use crate::planning::writer_planner_prompt::{WriterPlannerPromptContext, is_ai_role};
use std::collections::BTreeSet;

pub struct RetrievalPlanBuilder {
    retrieval: RetrievalConfig,
    planner: PlannerConfig,
    topic_matcher: TextMatcher,
}

impl RetrievalPlanBuilder {
    pub fn new(retrieval: RetrievalConfig, planner: PlannerConfig) -> Self {
        Self {
            retrieval,
            planner,
            topic_matcher: TextMatcher,
        }
    }

    pub fn build(
        &self,
        baseline: &BaselineContext,
        narrative_plan: &NarrativePlan,
        planner_output: PlannerOutput,
        snapshot: &StoryReadSnapshot,
        prompt_context: &WriterPlannerPromptContext,
    ) -> Result<WriterPlan, PlanningError> {
        if planner_output.context_gaps.len() > self.planner.max_context_gaps {
            return Err(PlanningError::LimitExceeded {
                limit: "max_context_gaps",
            });
        }
        if planner_output.character_think_requests.len() > self.planner.max_character_think_requests {
            return Err(PlanningError::LimitExceeded {
                limit: "max_character_think_requests",
            });
        }
        if planner_output.story_goal.as_str().trim().is_empty() {
            return Err(PlanningError::InvalidOutput {
                code: "story_goal_empty",
            });
        }
        if planner_output.story_goal.as_str().len() > self.planner.max_goal_bytes {
            return Err(PlanningError::LimitExceeded {
                limit: "max_goal_bytes",
            });
        }
        let think_requests = self.validate_think_requests(planner_output.character_think_requests, baseline)?;
        let mut requests = Vec::new();
        requests.extend(self.narrative_requests(narrative_plan)?);
        for gap in planner_output.context_gaps {
            requests.push(self.planner_gap_request(gap, baseline, snapshot, &think_requests, prompt_context)?);
        }
        let requests = dedupe_and_sort(requests);
        if requests.len() > self.retrieval.max_requests {
            return Err(PlanningError::LimitExceeded { limit: "max_requests" });
        }
        Ok(WriterPlan {
            story_goal: WriterStoryGoal {
                summary: planner_output.story_goal,
            },
            retrieval_plan: RetrievalPlan { requests },
            character_think_requests: think_requests,
        })
    }

    fn validate_think_requests(
        &self,
        requests: Vec<CharacterThinkRequest>,
        baseline: &BaselineContext,
    ) -> Result<Vec<CharacterThinkRequest>, PlanningError> {
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        for request in requests {
            if request.reason.as_str().len() > self.planner.max_reason_bytes {
                return Err(PlanningError::LimitExceeded {
                    limit: "max_reason_bytes",
                });
            }
            if request.reason.as_str().trim().is_empty() {
                return Err(PlanningError::InvalidOutput {
                    code: "character_think_reason_empty",
                });
            }
            if request.role_id == baseline.player_role.role_id {
                return Err(PlanningError::PlayerCharacterRequested);
            }
            if !is_ai_role(baseline, &request.role_id) {
                return Err(PlanningError::UnknownCharacter);
            }
            if !seen.insert(request.role_id.clone()) {
                continue;
            }
            out.push(request);
        }
        Ok(out)
    }

    fn narrative_requests(&self, narrative_plan: &NarrativePlan) -> Result<Vec<RetrievalRequest>, PlanningError> {
        let mut entities = BTreeSet::new();
        for node in &narrative_plan.active_nodes {
            entities.insert(KnowledgeEntity::NarrativeNode(node.clone()));
        }
        let mut requests = Vec::new();
        for entity in entities {
            requests.push(self.make_request(RequestDraft {
                audience: RetrievalAudience::GlobalWriter,
                target_source_id: None,
                knowledge_kinds: vec![KnowledgeKind::Fact, KnowledgeKind::Rumor],
                entities: vec![entity],
                topics: Vec::new(),
                query_text: None,
                reason: "narrative reference",
                origin: RetrievalRequestOrigin::Narrative,
                signal_priority: 2,
            })?);
        }
        Ok(requests)
    }

    fn planner_gap_request(
        &self,
        mut gap: crate::planning::planner_output::PlannerContextGap,
        baseline: &BaselineContext,
        snapshot: &StoryReadSnapshot,
        think_requests: &[CharacterThinkRequest],
        prompt_context: &WriterPlannerPromptContext,
    ) -> Result<RetrievalRequest, PlanningError> {
        if gap.reason.as_str().len() > self.planner.max_reason_bytes {
            return Err(PlanningError::LimitExceeded {
                limit: "max_reason_bytes",
            });
        }
        if gap.reason.as_str().trim().is_empty() {
            return Err(PlanningError::InvalidOutput {
                code: "context_gap_reason_empty",
            });
        }
        if gap.target_id.is_some() == gap.query_text.is_some() {
            return Err(PlanningError::InvalidOutput {
                code: "retrieval_selector_exclusivity",
            });
        }
        let mut entities = Vec::new();
        let mut topics = Vec::new();
        let mut target_source_id = None;
        if let Some(target_id) = &gap.target_id {
            if let Some(role_id) = prompt_context.role_targets.get(target_id) {
                if !matches!(&gap.audience, RetrievalAudience::GlobalWriter) {
                    return Err(PlanningError::KnowledgeAudienceViolation);
                }
                entities.push(KnowledgeEntity::Role(role_id.clone()));
            } else if let Some(source_id) = prompt_context.knowledge_targets.get(target_id) {
                if matches!(&gap.audience, RetrievalAudience::Character { .. })
                    && matches!(source_id, crate::domain::knowledge::KnowledgeSourceId::Fact(_))
                {
                    return Err(PlanningError::KnowledgeAudienceViolation);
                }
                target_source_id = Some(source_id.clone());
            } else {
                return Err(PlanningError::UnknownRetrievalKey);
            }
        }
        if let Some(query) = &gap.query_text {
            if query.as_str().len() > self.planner.max_query_bytes {
                return Err(PlanningError::LimitExceeded {
                    limit: "max_query_bytes",
                });
            }
            let matched_topics = self.topic_matcher.match_topics(query.as_str(), snapshot.topic_dictionary());
            for topic in matched_topics {
                if !topics.contains(&topic) {
                    topics.push(topic);
                }
            }
            let haystack = normalize_match_text(query.as_str());
            for entity in snapshot.entity_catalog() {
                let key = match entity {
                    KnowledgeEntity::World(key) => key.as_str(),
                    KnowledgeEntity::Role(id) => id.as_str(),
                    KnowledgeEntity::Location(key) => key.as_str(),
                    KnowledgeEntity::Scene(key) => key.as_str(),
                    KnowledgeEntity::NarrativeNode(key) => key.as_str(),
                    KnowledgeEntity::Event(key) => key.as_str(),
                };
                if term_matches(&haystack, &normalize_match_text(key)) && !entities.contains(entity) {
                    entities.push(entity.clone());
                }
            }
            gap.query_text = Some(
                BoundedText::try_new(normalize_match_text(query.as_str()), "query_text", self.planner.max_query_bytes)
                    .map_err(|_| PlanningError::InvalidOutput {
                        code: "query_text_invalid",
                    })?,
            );
        }
        if entities.len() > self.planner.max_entities_per_request {
            return Err(PlanningError::LimitExceeded {
                limit: "max_entities_per_request",
            });
        }
        if topics.len() > self.planner.max_topics_per_request {
            return Err(PlanningError::LimitExceeded {
                limit: "max_topics_per_request",
            });
        }
        authorize_gap(&gap, think_requests)?;
        for entity in &entities {
            if !entity_is_known(entity, snapshot.entity_catalog(), &baseline.retrieval_signals.entities) {
                return Err(PlanningError::UnknownRetrievalKey);
            }
        }
        for topic in &topics {
            if !snapshot.topic_dictionary().contains_key(topic) {
                return Err(PlanningError::UnknownRetrievalKey);
            }
        }
        let knowledge_kinds = match &gap.audience {
            RetrievalAudience::GlobalWriter => vec![KnowledgeKind::Fact, KnowledgeKind::Rumor],
            RetrievalAudience::Character { .. } => vec![KnowledgeKind::Rumor, KnowledgeKind::Memory],
        };
        self.make_request(RequestDraft {
            audience: gap.audience,
            target_source_id,
            knowledge_kinds,
            entities,
            topics,
            query_text: gap.query_text,
            reason: gap.reason.as_str(),
            origin: RetrievalRequestOrigin::Planner,
            signal_priority: 0,
        })
    }

    fn make_request(&self, draft: RequestDraft<'_>) -> Result<RetrievalRequest, PlanningError> {
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
        let authorized_memory_owners = authorized_memory_owners(&draft.audience, &knowledge_kinds, &entities)?;
        Ok(RetrievalRequest {
            audience: draft.audience,
            target_source_id: draft.target_source_id,
            knowledge_kinds,
            entities,
            topics,
            query_text: draft.query_text,
            authorized_memory_owners,
            reason,
            origin: draft.origin,
            signal_priority: draft.signal_priority,
        })
    }
}

fn authorized_memory_owners(
    audience: &RetrievalAudience,
    knowledge_kinds: &[KnowledgeKind],
    entities: &[KnowledgeEntity],
) -> Result<Vec<RoleId>, PlanningError> {
    if !knowledge_kinds.contains(&KnowledgeKind::Memory) {
        return Ok(Vec::new());
    }
    match audience {
        RetrievalAudience::GlobalWriter => {
            let mut owners = entities
                .iter()
                .filter_map(|entity| match entity {
                    KnowledgeEntity::Role(id) => Some(id.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            owners.sort();
            owners.dedup();
            if owners.is_empty() {
                return Err(PlanningError::KnowledgeAudienceViolation);
            }
            Ok(owners)
        }
        RetrievalAudience::Character { role_id } => Ok(vec![role_id.clone()]),
    }
}

struct RequestDraft<'a> {
    audience: RetrievalAudience,
    target_source_id: Option<crate::domain::knowledge::KnowledgeSourceId>,
    knowledge_kinds: Vec<KnowledgeKind>,
    entities: Vec<KnowledgeEntity>,
    topics: Vec<TopicKey>,
    query_text: Option<BoundedText>,
    reason: &'a str,
    origin: RetrievalRequestOrigin,
    signal_priority: u8,
}

fn authorize_gap(
    gap: &crate::planning::planner_output::PlannerContextGap,
    think_requests: &[CharacterThinkRequest],
) -> Result<(), PlanningError> {
    match &gap.audience {
        RetrievalAudience::Character { role_id } => {
            if !think_requests.iter().any(|request| &request.role_id == role_id) {
                return Err(PlanningError::KnowledgeAudienceViolation);
            }
        }
        RetrievalAudience::GlobalWriter => {}
    }
    Ok(())
}

fn dedupe_and_sort(requests: Vec<RetrievalRequest>) -> Vec<RetrievalRequest> {
    let mut by_key: std::collections::BTreeMap<RetrievalRequestKey, RetrievalRequest> =
        std::collections::BTreeMap::new();
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
    audience: RetrievalAudience,
    target_source_id: Option<crate::domain::knowledge::KnowledgeSourceId>,
    knowledge_kinds: Vec<KnowledgeKind>,
    entities: Vec<KnowledgeEntity>,
    topics: Vec<TopicKey>,
    query_text: Option<String>,
    authorized_memory_owners: Vec<RoleId>,
}

fn canonical_key(request: &RetrievalRequest) -> RetrievalRequestKey {
    RetrievalRequestKey {
        audience: request.audience.clone(),
        target_source_id: request.target_source_id.clone(),
        knowledge_kinds: request.knowledge_kinds.clone(),
        entities: request.entities.clone(),
        topics: request.topics.clone(),
        query_text: request.query_text.as_ref().map(ToString::to_string),
        authorized_memory_owners: request.authorized_memory_owners.clone(),
    }
}

fn origin_rank(origin: RetrievalRequestOrigin) -> u8 {
    match origin {
        RetrievalRequestOrigin::Automatic => 0,
        RetrievalRequestOrigin::Narrative => 1,
        RetrievalRequestOrigin::Planner => 2,
    }
}

fn entity_is_known(entity: &KnowledgeEntity, catalog: &[KnowledgeEntity], signals: &[EntitySignal]) -> bool {
    catalog.contains(entity)
        || signals.iter().any(|signal| &signal.entity == entity)
        || matches!(entity, KnowledgeEntity::Role(_))
}

#[cfg(test)]
#[path = "tests/retrieval_plan_builder_tests.rs"]
mod tests;
