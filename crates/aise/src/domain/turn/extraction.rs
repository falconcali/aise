use crate::domain::asset::entity::KnowledgeEntity;
use crate::domain::asset::ids::{MemoryKind, NarrativeConditionKey, Sha256Digest, TopicKey};
use crate::domain::asset::text_matcher::TextMatcher;
use crate::domain::asset::validation::{BoundedText, ScalarValue};
use crate::domain::ids::{FactId, MemoryId, RoleId, RumorId, TurnNumber};
use crate::domain::knowledge::hint::{RetrievalHint, normalize_static_retrieval_hint};
use crate::domain::knowledge::query::{KnowledgeSourceId, allocate_knowledge_ids};
use crate::domain::knowledge::rumor::TruthValue;
use crate::domain::knowledge::{KnowledgeEntry, KnowledgeKind, KnowledgeSource};
use crate::domain::story_instance::role::StoryRole;
use crate::domain::story_instance::snapshot::StoryReadSnapshot;
use crate::domain::turn::retrieval::RetrievedContext;
use crate::turn::turn_validation::{ValidatedKnowledgeMutation, ValidatedKnowledgeOperation};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use thiserror::Error;

pub const DEFAULT_RUNTIME_KNOWLEDGE_SALIENCE: u8 = 128;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum DeletableKnowledgeId {
    Rumor(RumorId),
    Memory(MemoryId),
}

#[derive(Debug, Clone, Copy)]
pub struct StoryStateExtractionLimits {
    pub max_new_roles: usize,
    pub max_role_states: usize,
    pub max_relationship_states: usize,
    pub max_knowledge_items: usize,
    pub max_goals_per_role: usize,
    pub max_attributes_per_role: usize,
    pub max_item_bytes: usize,
    pub max_role_profile_bytes: usize,
    pub max_knowledge_change_bytes: usize,
    pub max_cast_policy_violations: usize,
    pub max_condition_queries: usize,
    pub max_condition_evidence_bytes: usize,
    pub max_condition_reason_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewRoleDto {
    pub role_id: String,
    pub name: String,
    pub role_label: String,
    pub narrative_function: String,
    pub background: String,
    pub appearance: String,
    pub personality: String,
    pub speaking_style: String,
    pub location: String,
    pub goals: Vec<String>,
    #[serde(default)]
    pub attributes: BTreeMap<String, ScalarValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleStateDto {
    pub role_id: String,
    pub location: String,
    pub goals: Vec<String>,
    #[serde(default)]
    pub attributes: BTreeMap<String, ScalarValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipStateDto {
    pub source_role_id: String,
    pub target_role_id: String,
    pub kind: String,
    pub trust: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactDraftDto {
    pub content: String,
    pub retrieval_hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactUpdateDto {
    pub id: String,
    pub content: String,
    pub retrieval_hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RumorDraftDto {
    pub content: String,
    pub retrieval_hint: String,
    pub source_role_id: String,
    pub truth_value: TruthValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RumorUpdateDto {
    pub id: String,
    pub content: String,
    pub retrieval_hint: String,
    pub source_role_id: String,
    pub truth_value: TruthValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryDraftDto {
    pub owner_role_id: String,
    pub memory_kind: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryUpdateDto {
    pub id: String,
    pub memory_kind: String,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NarrativeConditionStatus {
    Satisfied,
    Unsatisfied,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NarrativeConditionJudgmentDto {
    pub condition_key: String,
    pub status: NarrativeConditionStatus,
    pub evidence: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryStateExtractionDto {
    pub new_roles: Vec<NewRoleDto>,
    pub role_states: Vec<RoleStateDto>,
    pub relationship_states: Vec<RelationshipStateDto>,
    pub add_facts: Vec<FactDraftDto>,
    pub update_facts: Vec<FactUpdateDto>,
    pub add_rumors: Vec<RumorDraftDto>,
    pub update_rumors: Vec<RumorUpdateDto>,
    pub delete_rumor_ids: Vec<String>,
    pub add_memories: Vec<MemoryDraftDto>,
    pub update_memories: Vec<MemoryUpdateDto>,
    pub delete_memory_ids: Vec<String>,
    pub narrative_condition_judgments: Vec<NarrativeConditionJudgmentDto>,
    pub cast_policy_violations: Vec<String>,
}

impl StoryStateExtractionDto {
    pub fn json_schema(limits: StoryStateExtractionLimits) -> Value {
        let text = |max: usize| json!({"type": "string", "maxLength": max});
        let key = || json!({"type": "string", "minLength": 1, "maxLength": limits.max_item_bytes});
        let scalar = json!({"type": ["boolean", "integer", "string"]});
        let attributes = json!({
            "type": "object",
            "maxProperties": limits.max_attributes_per_role,
            "additionalProperties": scalar
        });
        let goals = json!({
            "type": "array",
            "maxItems": limits.max_goals_per_role,
            "items": text(limits.max_item_bytes)
        });
        let new_role = json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "role_id", "name", "role_label", "narrative_function", "background", "appearance",
                "personality", "speaking_style", "location", "goals", "attributes"
            ],
            "properties": {
                "role_id": key(),
                "name": text(limits.max_role_profile_bytes),
                "role_label": text(limits.max_role_profile_bytes),
                "narrative_function": text(limits.max_role_profile_bytes),
                "background": text(limits.max_role_profile_bytes),
                "appearance": text(limits.max_role_profile_bytes),
                "personality": text(limits.max_role_profile_bytes),
                "speaking_style": text(limits.max_role_profile_bytes),
                "location": key(),
                "goals": goals,
                "attributes": attributes
            }
        });
        let role_state = json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["role_id", "location", "goals", "attributes"],
            "properties": {
                "role_id": key(),
                "location": key(),
                "goals": goals,
                "attributes": attributes
            }
        });
        let relationship_state = json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["source_role_id", "target_role_id", "kind", "trust"],
            "properties": {
                "source_role_id": key(),
                "target_role_id": key(),
                "kind": key(),
                "trust": {"type": "integer"}
            }
        });
        let fact_draft = json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["content", "retrieval_hint"],
            "properties": {
                "content": text(limits.max_knowledge_change_bytes),
                "retrieval_hint": text(RetrievalHint::MAX_BYTES)
            }
        });
        let fact_update = json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["id", "content", "retrieval_hint"],
            "properties": {
                "id": key(),
                "content": text(limits.max_knowledge_change_bytes),
                "retrieval_hint": text(RetrievalHint::MAX_BYTES)
            }
        });
        let truth_value = json!({"enum": ["true", "false", "unverified"]});
        let rumor_draft = json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["content", "retrieval_hint", "source_role_id", "truth_value"],
            "properties": {
                "content": text(limits.max_knowledge_change_bytes),
                "retrieval_hint": text(RetrievalHint::MAX_BYTES),
                "source_role_id": text(limits.max_item_bytes),
                "truth_value": truth_value
            }
        });
        let rumor_update = json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["id", "content", "retrieval_hint", "source_role_id", "truth_value"],
            "properties": {
                "id": key(),
                "content": text(limits.max_knowledge_change_bytes),
                "retrieval_hint": text(RetrievalHint::MAX_BYTES),
                "source_role_id": text(limits.max_item_bytes),
                "truth_value": truth_value
            }
        });
        let memory_draft = json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["owner_role_id", "memory_kind", "content"],
            "properties": {
                "owner_role_id": key(),
                "memory_kind": key(),
                "content": text(limits.max_knowledge_change_bytes)
            }
        });
        let memory_update = json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["id", "memory_kind", "content"],
            "properties": {
                "id": key(),
                "memory_kind": key(),
                "content": text(limits.max_knowledge_change_bytes)
            }
        });
        let condition_status = json!({"enum": ["satisfied", "unsatisfied", "unknown"]});
        let judgment = json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["condition_key", "status", "evidence", "reason"],
            "properties": {
                "condition_key": key(),
                "status": condition_status,
                "evidence": text(limits.max_condition_evidence_bytes),
                "reason": text(limits.max_condition_reason_bytes)
            }
        });
        let ids_array = |max: usize| json!({"type": "array", "maxItems": max, "items": key()});
        let strings_array =
            |max: usize| json!({"type": "array", "maxItems": max, "items": text(limits.max_item_bytes)});
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "new_roles", "role_states", "relationship_states", "add_facts", "update_facts", "add_rumors",
                "update_rumors", "delete_rumor_ids", "add_memories", "update_memories", "delete_memory_ids",
                "narrative_condition_judgments", "cast_policy_violations"
            ],
            "properties": {
                "new_roles": {"type": "array", "maxItems": limits.max_new_roles, "items": new_role},
                "role_states": {"type": "array", "maxItems": limits.max_role_states, "items": role_state},
                "relationship_states": {
                    "type": "array", "maxItems": limits.max_relationship_states, "items": relationship_state
                },
                "add_facts": {"type": "array", "maxItems": limits.max_knowledge_items, "items": fact_draft},
                "update_facts": {"type": "array", "maxItems": limits.max_knowledge_items, "items": fact_update},
                "add_rumors": {"type": "array", "maxItems": limits.max_knowledge_items, "items": rumor_draft},
                "update_rumors": {"type": "array", "maxItems": limits.max_knowledge_items, "items": rumor_update},
                "delete_rumor_ids": ids_array(limits.max_knowledge_items),
                "add_memories": {"type": "array", "maxItems": limits.max_knowledge_items, "items": memory_draft},
                "update_memories": {"type": "array", "maxItems": limits.max_knowledge_items, "items": memory_update},
                "delete_memory_ids": ids_array(limits.max_knowledge_items),
                "narrative_condition_judgments": {
                    "type": "array", "maxItems": limits.max_condition_queries, "items": judgment
                },
                "cast_policy_violations": strings_array(limits.max_cast_policy_violations)
            }
        })
    }

    pub fn compact_prompt_shape() -> String {
        "Return exactly one JSON object with required arrays (use [] when empty): new_roles, role_states, \
relationship_states, add_facts, update_facts, add_rumors, update_rumors, delete_rumor_ids, add_memories, \
update_memories, delete_memory_ids, narrative_condition_judgments, cast_policy_violations. Each item uses the \
semantic fields named in the CSI/RC instructions. No additional fields, no prose outside the object."
            .to_owned()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NarrativeConditionResult {
    pub condition_key: NarrativeConditionKey,
    pub status: NarrativeConditionStatus,
    pub evidence: BoundedText,
    pub reason: BoundedText,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryCandidateVersion {
    pub content_digest: Sha256Digest,
    pub repair_attempt: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryStateExtractionEnvelope {
    pub candidate_version: StoryCandidateVersion,
    pub expected_graph_revision: u64,
    pub state: StoryStateExtractionDto,
    pub narrative_condition_results: Vec<NarrativeConditionResult>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ExtractionEnrichmentError {
    #[error("knowledge content exceeds its configured byte budget")]
    ContentExceedsBudget,
    #[error("retrieval_hint is invalid")]
    InvalidRetrievalHint,
    #[error("knowledge id allocation overflowed")]
    AllocationOverflow,
    #[error("update/delete target does not reference a known knowledge item")]
    UnknownTarget,
    #[error("rumor source_role_id does not reference a known role")]
    UnknownSourceRole,
    #[error("memory owner_role_id does not reference a known role")]
    UnknownOwnerRole,
    #[error("memory_kind is invalid")]
    InvalidMemoryKind,
    #[error("knowledge change count exceeds a u32 ordinal")]
    OrdinalOverflow,
}

pub struct KnowledgeEnrichmentContext<'a> {
    pub retrieved: &'a RetrievedContext,
    pub turn_number: TurnNumber,
    pub created_at_ms: i64,
    pub max_content_bytes: usize,
}

pub fn enrich_extracted_knowledge(
    dto: &StoryStateExtractionDto,
    snapshot: &StoryReadSnapshot,
    accepted_new_roles: &[StoryRole],
    context: &KnowledgeEnrichmentContext<'_>,
) -> Result<(Vec<ValidatedKnowledgeMutation>, crate::domain::knowledge::KnowledgeIdHighWater), ExtractionEnrichmentError>
{
    let source = KnowledgeSource::CommittedTurn {
        turn_number: context.turn_number,
    };

    let add_kinds: Vec<KnowledgeKind> = std::iter::repeat_n(KnowledgeKind::Fact, dto.add_facts.len())
        .chain(std::iter::repeat_n(KnowledgeKind::Rumor, dto.add_rumors.len()))
        .chain(std::iter::repeat_n(KnowledgeKind::Memory, dto.add_memories.len()))
        .collect();
    let allocation = allocate_knowledge_ids(snapshot.knowledge_id_high_water(), &add_kinds)
        .map_err(|_| ExtractionEnrichmentError::AllocationOverflow)?;
    let new_high_water = allocation.new_high_water;
    let mut assigned = allocation.assigned.into_iter();

    let mut operations = Vec::new();

    for draft in &dto.add_facts {
        let content = bounded_content(&draft.content, context.max_content_bytes)?;
        let retrieval_hint = enriched_retrieval_hint(&draft.retrieval_hint, &content)?;
        let topics = recompute_topics(snapshot, content.as_str(), retrieval_hint.as_str());
        let id = next_fact_id(&mut assigned)?;
        operations.push(ValidatedKnowledgeOperation::Add(KnowledgeEntry::Fact(
            crate::domain::knowledge::fact::WorldFact {
                id,
                key: None,
                text: content,
                proposition: None,
                retrieval_hint,
                entities: Vec::new(),
                topics,
                salience: DEFAULT_RUNTIME_KNOWLEDGE_SALIENCE,
                source: source.clone(),
            },
        )));
    }
    for update in &dto.update_facts {
        let target = FactId::try_new(update.id.clone()).map_err(|_| ExtractionEnrichmentError::UnknownTarget)?;
        let existing_salience =
            existing_fact_salience(context.retrieved, &target).ok_or(ExtractionEnrichmentError::UnknownTarget)?;
        let content = bounded_content(&update.content, context.max_content_bytes)?;
        let retrieval_hint = enriched_retrieval_hint(&update.retrieval_hint, &content)?;
        let topics = recompute_topics(snapshot, content.as_str(), retrieval_hint.as_str());
        operations.push(ValidatedKnowledgeOperation::Update {
            target: KnowledgeSourceId::Fact(target.clone()),
            value: KnowledgeEntry::Fact(crate::domain::knowledge::fact::WorldFact {
                id: target,
                key: None,
                text: content,
                proposition: None,
                retrieval_hint,
                entities: Vec::new(),
                topics,
                salience: existing_salience,
                source: source.clone(),
            }),
        });
    }

    for draft in &dto.add_rumors {
        let content = bounded_content(&draft.content, context.max_content_bytes)?;
        let retrieval_hint = enriched_retrieval_hint(&draft.retrieval_hint, &content)?;
        let topics = recompute_topics(snapshot, content.as_str(), retrieval_hint.as_str());
        let source_role_id = resolve_source_role(&draft.source_role_id, snapshot, accepted_new_roles)?;
        let entities = source_role_id.iter().cloned().map(KnowledgeEntity::Role).collect();
        let id = next_rumor_id(&mut assigned)?;
        operations.push(ValidatedKnowledgeOperation::Add(KnowledgeEntry::Rumor(
            crate::domain::knowledge::rumor::SharedRumor {
                id,
                key: None,
                content,
                claim: None,
                retrieval_hint,
                entities,
                topics,
                salience: DEFAULT_RUNTIME_KNOWLEDGE_SALIENCE,
                source_role_id,
                truth_value: draft.truth_value.clone(),
                source: source.clone(),
            },
        )));
    }
    for update in &dto.update_rumors {
        let target = RumorId::try_new(update.id.clone()).map_err(|_| ExtractionEnrichmentError::UnknownTarget)?;
        let existing_salience =
            existing_rumor_salience(context.retrieved, &target).ok_or(ExtractionEnrichmentError::UnknownTarget)?;
        let content = bounded_content(&update.content, context.max_content_bytes)?;
        let retrieval_hint = enriched_retrieval_hint(&update.retrieval_hint, &content)?;
        let topics = recompute_topics(snapshot, content.as_str(), retrieval_hint.as_str());
        let source_role_id = resolve_source_role(&update.source_role_id, snapshot, accepted_new_roles)?;
        let entities = source_role_id.iter().cloned().map(KnowledgeEntity::Role).collect();
        operations.push(ValidatedKnowledgeOperation::Update {
            target: KnowledgeSourceId::Rumor(target.clone()),
            value: KnowledgeEntry::Rumor(crate::domain::knowledge::rumor::SharedRumor {
                id: target,
                key: None,
                content,
                claim: None,
                retrieval_hint,
                entities,
                topics,
                salience: existing_salience,
                source_role_id,
                truth_value: update.truth_value.clone(),
                source: source.clone(),
            }),
        });
    }
    for raw_id in &dto.delete_rumor_ids {
        let target = RumorId::try_new(raw_id.clone()).map_err(|_| ExtractionEnrichmentError::UnknownTarget)?;
        if existing_rumor_salience(context.retrieved, &target).is_none() {
            return Err(ExtractionEnrichmentError::UnknownTarget);
        }
        operations.push(ValidatedKnowledgeOperation::Delete {
            target: DeletableKnowledgeId::Rumor(target),
        });
    }

    for draft in &dto.add_memories {
        let owner =
            RoleId::try_new(draft.owner_role_id.clone()).map_err(|_| ExtractionEnrichmentError::UnknownOwnerRole)?;
        if !role_is_known(&owner, snapshot, accepted_new_roles) {
            return Err(ExtractionEnrichmentError::UnknownOwnerRole);
        }
        let memory_kind =
            MemoryKind::try_new(draft.memory_kind.clone()).map_err(|_| ExtractionEnrichmentError::InvalidMemoryKind)?;
        let content = bounded_content(&draft.content, context.max_content_bytes)?;
        let topics = recompute_topics(snapshot, content.as_str(), "");
        let id = next_memory_id(&mut assigned)?;
        operations.push(ValidatedKnowledgeOperation::Add(KnowledgeEntry::Memory(
            crate::domain::knowledge::memory::MemoryEntry {
                id,
                owner: owner.clone(),
                kind: memory_kind,
                content,
                entities: vec![KnowledgeEntity::Role(owner)],
                topics,
                salience: DEFAULT_RUNTIME_KNOWLEDGE_SALIENCE,
                source: source.clone(),
                created_at_ms: context.created_at_ms,
            },
        )));
    }
    for update in &dto.update_memories {
        let target = MemoryId::try_new(update.id.clone()).map_err(|_| ExtractionEnrichmentError::UnknownTarget)?;
        let (owner, existing_salience) =
            existing_memory(context.retrieved, &target).ok_or(ExtractionEnrichmentError::UnknownTarget)?;
        let memory_kind = MemoryKind::try_new(update.memory_kind.clone())
            .map_err(|_| ExtractionEnrichmentError::InvalidMemoryKind)?;
        let content = bounded_content(&update.content, context.max_content_bytes)?;
        let topics = recompute_topics(snapshot, content.as_str(), "");
        operations.push(ValidatedKnowledgeOperation::Update {
            target: KnowledgeSourceId::Memory(target.clone()),
            value: KnowledgeEntry::Memory(crate::domain::knowledge::memory::MemoryEntry {
                id: target,
                owner: owner.clone(),
                kind: memory_kind,
                content,
                entities: vec![KnowledgeEntity::Role(owner)],
                topics,
                salience: existing_salience,
                source: source.clone(),
                created_at_ms: context.created_at_ms,
            }),
        });
    }
    for raw_id in &dto.delete_memory_ids {
        let target = MemoryId::try_new(raw_id.clone()).map_err(|_| ExtractionEnrichmentError::UnknownTarget)?;
        if existing_memory(context.retrieved, &target).is_none() {
            return Err(ExtractionEnrichmentError::UnknownTarget);
        }
        operations.push(ValidatedKnowledgeOperation::Delete {
            target: DeletableKnowledgeId::Memory(target),
        });
    }

    let mutations: Vec<ValidatedKnowledgeMutation> = operations
        .into_iter()
        .enumerate()
        .map(|(index, operation)| {
            let ordinal = u32::try_from(index).map_err(|_| ExtractionEnrichmentError::OrdinalOverflow)?;
            Ok(ValidatedKnowledgeMutation { ordinal, operation })
        })
        .collect::<Result<Vec<_>, ExtractionEnrichmentError>>()?;
    Ok((mutations, new_high_water))
}

fn next_fact_id(assigned: &mut impl Iterator<Item = KnowledgeSourceId>) -> Result<FactId, ExtractionEnrichmentError> {
    match assigned.next() {
        Some(KnowledgeSourceId::Fact(id)) => Ok(id),
        _ => Err(ExtractionEnrichmentError::AllocationOverflow),
    }
}

fn next_rumor_id(assigned: &mut impl Iterator<Item = KnowledgeSourceId>) -> Result<RumorId, ExtractionEnrichmentError> {
    match assigned.next() {
        Some(KnowledgeSourceId::Rumor(id)) => Ok(id),
        _ => Err(ExtractionEnrichmentError::AllocationOverflow),
    }
}

fn next_memory_id(
    assigned: &mut impl Iterator<Item = KnowledgeSourceId>,
) -> Result<MemoryId, ExtractionEnrichmentError> {
    match assigned.next() {
        Some(KnowledgeSourceId::Memory(id)) => Ok(id),
        _ => Err(ExtractionEnrichmentError::AllocationOverflow),
    }
}

fn bounded_content(value: &str, max_bytes: usize) -> Result<BoundedText, ExtractionEnrichmentError> {
    BoundedText::try_new(value.trim().to_owned(), "knowledge_content", max_bytes)
        .map_err(|_| ExtractionEnrichmentError::ContentExceedsBudget)
}

fn enriched_retrieval_hint(raw: &str, content: &BoundedText) -> Result<RetrievalHint, ExtractionEnrichmentError> {
    let configured = if raw.trim().is_empty() {
        None
    } else {
        Some(RetrievalHint::try_new(raw.trim()).map_err(|_| ExtractionEnrichmentError::InvalidRetrievalHint)?)
    };
    normalize_static_retrieval_hint(content, configured).map_err(|_| ExtractionEnrichmentError::InvalidRetrievalHint)
}

fn recompute_topics(snapshot: &StoryReadSnapshot, content: &str, retrieval_hint: &str) -> Vec<TopicKey> {
    let matcher = TextMatcher;
    let combined = if retrieval_hint.is_empty() {
        content.to_owned()
    } else {
        format!("{content}\n{retrieval_hint}")
    };
    let mut topics = matcher.match_topics(&combined, snapshot.topic_dictionary());
    topics.sort();
    topics.dedup();
    topics
}

fn role_is_known(role_id: &RoleId, snapshot: &StoryReadSnapshot, accepted_new_roles: &[StoryRole]) -> bool {
    snapshot.role(role_id).is_some() || accepted_new_roles.iter().any(|role| &role.role_id == role_id)
}

fn resolve_source_role(
    raw: &str,
    snapshot: &StoryReadSnapshot,
    accepted_new_roles: &[StoryRole],
) -> Result<Option<RoleId>, ExtractionEnrichmentError> {
    if raw.trim().is_empty() {
        return Ok(None);
    }
    let role_id = RoleId::try_new(raw).map_err(|_| ExtractionEnrichmentError::UnknownSourceRole)?;
    if role_is_known(&role_id, snapshot, accepted_new_roles) {
        Ok(Some(role_id))
    } else {
        Err(ExtractionEnrichmentError::UnknownSourceRole)
    }
}

fn existing_fact_salience(retrieved: &RetrievedContext, id: &FactId) -> Option<u8> {
    retrieved
        .world()
        .facts
        .iter()
        .find(|item| matches!(&item.source_id, KnowledgeSourceId::Fact(existing) if existing == id))
        .map(|item| item.relevance.salience)
}

fn existing_rumor_salience(retrieved: &RetrievedContext, id: &RumorId) -> Option<u8> {
    retrieved
        .world()
        .rumors
        .iter()
        .chain(
            retrieved
                .characters()
                .values()
                .flat_map(|character| character.known_rumors.iter()),
        )
        .find(|item| matches!(&item.source_id, KnowledgeSourceId::Rumor(existing) if existing == id))
        .map(|item| item.relevance.salience)
}

fn existing_memory(retrieved: &RetrievedContext, id: &MemoryId) -> Option<(RoleId, u8)> {
    retrieved.characters().iter().find_map(|(role_id, character)| {
        character
            .memories
            .iter()
            .find(|item| matches!(&item.source_id, KnowledgeSourceId::Memory(existing) if existing == id))
            .map(|item| (role_id.clone(), item.relevance.salience))
    })
}

#[cfg(test)]
#[path = "tests/extraction_tests.rs"]
mod tests;
