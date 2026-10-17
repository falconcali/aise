use crate::core::{
    ActivationRule, BoundedText, FactBody, FactChangeOp, FactId, KnowledgeChange, MemoryBody, MemoryId, RetrievalHint,
    RoleId, RoleMemoryChangeOp, RumorBody, RumorChangeOp, RumorId, StoryContext, TurnNumber,
};
use crate::pipeline::common::{PipelineError, PipelineStage};
use crate::pipeline::extract::ExtractConfig;
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Serialize)]
pub(super) struct FactView<'a> {
    pub id: &'a FactId,
    pub content: &'a BoundedText,
    pub retrieval: &'a RetrievalHint,
    pub activation: &'a ActivationRule,
}

#[derive(Serialize)]
pub(super) struct RumorView<'a> {
    pub id: &'a RumorId,
    pub content: &'a BoundedText,
    pub retrieval: &'a RetrievalHint,
    pub activation: &'a ActivationRule,
}

#[derive(Serialize)]
pub(super) struct ExtractRoleCatalogEntry<'a> {
    pub role_id: &'a RoleId,
    pub name: &'a BoundedText,
    pub aliases: &'a [BoundedText],
    pub retrieval_hint: &'a RetrievalHint,
}

#[derive(Serialize)]
pub(super) struct RoleMemoryView<'a> {
    pub id: &'a MemoryId,
    pub content: &'a BoundedText,
    pub retrieval: &'a RetrievalHint,
    pub activation: &'a ActivationRule,
}

#[derive(Serialize)]
pub(super) struct WorldKnowledgeExtraction {
    pub facts: Vec<FactChangeOp>,
    pub rumors: Vec<RumorChangeOp>,
    pub roles_for_memory: Vec<RoleId>,
}

pub(super) enum FactProposal {
    Add { body: FactBody },
    Update { id: FactId, body: FactBody },
}

pub(super) enum RumorProposal {
    Add { body: RumorBody },
    Update { id: RumorId, body: RumorBody },
    Remove { id: RumorId },
}

pub(super) enum RoleMemoryProposal {
    Add { body: MemoryBody },
    Update { id: MemoryId, body: MemoryBody },
}

pub(super) fn project_facts(story_ctx: &StoryContext) -> Vec<FactView<'_>> {
    story_ctx
        .facts
        .values()
        .map(|entry| FactView {
            id: &entry.id,
            content: &entry.body.content,
            retrieval: &entry.body.retrieval,
            activation: &entry.body.activation,
        })
        .collect()
}

pub(super) fn project_rumors(story_ctx: &StoryContext) -> Vec<RumorView<'_>> {
    story_ctx
        .rumors
        .values()
        .map(|entry| RumorView {
            id: &entry.id,
            content: &entry.body.content,
            retrieval: &entry.body.retrieval,
            activation: &entry.body.activation,
        })
        .collect()
}

pub(super) fn project_role_catalog(story_ctx: &StoryContext) -> Vec<ExtractRoleCatalogEntry<'_>> {
    story_ctx
        .roles
        .values()
        .map(|role| ExtractRoleCatalogEntry {
            role_id: &role.id,
            name: &role.spec.name,
            aliases: &role.spec.alias,
            retrieval_hint: &role.spec.retrieval,
        })
        .collect()
}

pub(super) fn project_role_memories<'a>(
    story_ctx: &'a StoryContext,
    role_id: &RoleId,
) -> Result<Vec<RoleMemoryView<'a>>, PipelineError> {
    let role = story_ctx.roles.get(role_id).ok_or_else(|| {
        PipelineError::invalid_output(
            PipelineStage::Extract,
            "extract_role_unknown",
            format!("role {} does not exist in story context", role_id),
        )
    })?;
    Ok(role
        .memories
        .values()
        .map(|entry| RoleMemoryView {
            id: &entry.id,
            content: &entry.body.content,
            retrieval: &entry.body.retrieval,
            activation: &entry.body.activation,
        })
        .collect())
}

pub(super) fn allocate_fact_id(
    story_ctx: &StoryContext,
    next_turn: &TurnNumber,
    seq: usize,
) -> Result<FactId, PipelineError> {
    allocate_fact_id_with_reserved(story_ctx, next_turn, seq, &BTreeSet::new())
}

pub(super) fn allocate_rumor_id(
    story_ctx: &StoryContext,
    next_turn: &TurnNumber,
    seq: usize,
) -> Result<RumorId, PipelineError> {
    allocate_rumor_id_with_reserved(story_ctx, next_turn, seq, &BTreeSet::new())
}

pub(super) fn allocate_memory_id(
    story_ctx: &StoryContext,
    role_id: &RoleId,
    next_turn: &TurnNumber,
    seq: usize,
) -> Result<MemoryId, PipelineError> {
    allocate_memory_id_with_reserved(story_ctx, role_id, next_turn, seq, &BTreeSet::new())
}

pub(super) fn validate_activation_rule(rule: &ActivationRule) -> Result<(), PipelineError> {
    if rule.mode.enabled && !rule.mode.constant && rule.match_rule.keys.is_empty() {
        return Err(PipelineError::invalid_output(
            PipelineStage::Extract,
            "extract_activation_rule_invalid",
            "activation rule with mode.enabled=true and mode.constant=false must have non-empty match.keys",
        ));
    }
    Ok(())
}

pub(super) fn resolve_world_knowledge(
    story_ctx: &StoryContext,
    config: &ExtractConfig,
    fact_proposals: Vec<FactProposal>,
    rumor_proposals: Vec<RumorProposal>,
    roles_for_memory: Vec<RoleId>,
) -> Result<WorldKnowledgeExtraction, PipelineError> {
    if fact_proposals.len() > config.max_fact_changes {
        return Err(PipelineError::invalid_output(
            PipelineStage::Extract,
            "extract_fact_changes_exceeded",
            format!(
                "fact changes {} exceeded limit {}",
                fact_proposals.len(),
                config.max_fact_changes
            ),
        ));
    }
    if rumor_proposals.len() > config.max_rumor_changes {
        return Err(PipelineError::invalid_output(
            PipelineStage::Extract,
            "extract_rumor_changes_exceeded",
            format!(
                "rumor changes {} exceeded limit {}",
                rumor_proposals.len(),
                config.max_rumor_changes
            ),
        ));
    }
    if roles_for_memory.len() > config.max_roles_for_memory {
        return Err(PipelineError::invalid_output(
            PipelineStage::Extract,
            "extract_roles_for_memory_exceeded",
            format!(
                "roles_for_memory {} exceeded limit {}",
                roles_for_memory.len(),
                config.max_roles_for_memory
            ),
        ));
    }
    let next_turn = story_ctx.turn_number.increment();
    let mut updated_facts = BTreeSet::new();
    let mut reserved_fact_ids = BTreeSet::new();
    let mut fact_add_seq = 0usize;
    let mut facts = Vec::with_capacity(fact_proposals.len());
    for proposal in fact_proposals {
        match proposal {
            FactProposal::Add { body } => {
                validate_activation_rule(&body.activation)?;
                fact_add_seq += 1;
                let id = if reserved_fact_ids.is_empty() {
                    allocate_fact_id(story_ctx, &next_turn, fact_add_seq)?
                } else {
                    allocate_fact_id_with_reserved(story_ctx, &next_turn, fact_add_seq, &reserved_fact_ids)?
                };
                reserved_fact_ids.insert(id.clone());
                facts.push(FactChangeOp::Add { id, body });
            }
            FactProposal::Update { id, body } => {
                if !story_ctx.facts.contains_key(&id) {
                    return Err(PipelineError::invalid_output(
                        PipelineStage::Extract,
                        "extract_fact_unknown_id",
                        format!("fact {} does not exist in story context", id),
                    ));
                }
                if !updated_facts.insert(id.clone()) {
                    return Err(PipelineError::invalid_output(
                        PipelineStage::Extract,
                        "extract_fact_duplicate_id",
                        format!("fact {} was updated more than once", id),
                    ));
                }
                validate_activation_rule(&body.activation)?;
                facts.push(FactChangeOp::Update { id, body });
            }
        }
    }

    let mut targeted_rumors = BTreeSet::new();
    let mut reserved_rumor_ids = BTreeSet::new();
    let mut rumor_add_seq = 0usize;
    let mut rumors = Vec::with_capacity(rumor_proposals.len());
    for proposal in rumor_proposals {
        match proposal {
            RumorProposal::Add { body } => {
                validate_activation_rule(&body.activation)?;
                rumor_add_seq += 1;
                let id = if reserved_rumor_ids.is_empty() {
                    allocate_rumor_id(story_ctx, &next_turn, rumor_add_seq)?
                } else {
                    allocate_rumor_id_with_reserved(story_ctx, &next_turn, rumor_add_seq, &reserved_rumor_ids)?
                };
                reserved_rumor_ids.insert(id.clone());
                rumors.push(RumorChangeOp::Add { id, body });
            }
            RumorProposal::Update { id, body } => {
                if !story_ctx.rumors.contains_key(&id) {
                    return Err(PipelineError::invalid_output(
                        PipelineStage::Extract,
                        "extract_rumor_unknown_id",
                        format!("rumor {} does not exist in story context", id),
                    ));
                }
                if !targeted_rumors.insert(id.clone()) {
                    return Err(PipelineError::invalid_output(
                        PipelineStage::Extract,
                        "extract_rumor_duplicate_id",
                        format!("rumor {} was targeted more than once", id),
                    ));
                }
                validate_activation_rule(&body.activation)?;
                rumors.push(RumorChangeOp::Update { id, body });
            }
            RumorProposal::Remove { id } => {
                if !story_ctx.rumors.contains_key(&id) {
                    return Err(PipelineError::invalid_output(
                        PipelineStage::Extract,
                        "extract_rumor_unknown_id",
                        format!("rumor {} does not exist in story context", id),
                    ));
                }
                if !targeted_rumors.insert(id.clone()) {
                    return Err(PipelineError::invalid_output(
                        PipelineStage::Extract,
                        "extract_rumor_duplicate_id",
                        format!("rumor {} was targeted more than once", id),
                    ));
                }
                rumors.push(RumorChangeOp::Remove { id });
            }
        }
    }

    let mut seen_roles = BTreeSet::new();
    for role_id in &roles_for_memory {
        if !story_ctx.roles.contains_key(role_id) {
            return Err(PipelineError::invalid_output(
                PipelineStage::Extract,
                "extract_role_unknown",
                format!("role {} does not exist in story context", role_id),
            ));
        }
        if !seen_roles.insert(role_id.clone()) {
            return Err(PipelineError::invalid_output(
                PipelineStage::Extract,
                "extract_role_duplicate",
                format!("role {} was selected more than once in roles_for_memory", role_id),
            ));
        }
    }

    Ok(WorldKnowledgeExtraction {
        facts,
        rumors,
        roles_for_memory,
    })
}

pub(super) fn resolve_role_memories(
    story_ctx: &StoryContext,
    config: &ExtractConfig,
    role_id: &RoleId,
    proposals: Vec<RoleMemoryProposal>,
) -> Result<Vec<RoleMemoryChangeOp>, PipelineError> {
    let role = story_ctx.roles.get(role_id).ok_or_else(|| {
        PipelineError::invalid_output(
            PipelineStage::Extract,
            "extract_role_unknown",
            format!("role {} does not exist in story context", role_id),
        )
    })?;
    if proposals.len() > config.max_memory_changes_per_role {
        return Err(PipelineError::invalid_output(
            PipelineStage::Extract,
            "extract_memory_changes_exceeded",
            format!(
                "memory changes {} for role {} exceeded limit {}",
                proposals.len(),
                role_id,
                config.max_memory_changes_per_role
            ),
        ));
    }
    let next_turn = story_ctx.turn_number.increment();
    let mut updated_memories = BTreeSet::new();
    let mut reserved_memory_ids = BTreeSet::new();
    let mut memory_add_seq = 0usize;
    let mut changes = Vec::with_capacity(proposals.len());
    for proposal in proposals {
        match proposal {
            RoleMemoryProposal::Add { body } => {
                validate_activation_rule(&body.activation)?;
                memory_add_seq += 1;
                let id = if reserved_memory_ids.is_empty() {
                    allocate_memory_id(story_ctx, role_id, &next_turn, memory_add_seq)?
                } else {
                    allocate_memory_id_with_reserved(
                        story_ctx,
                        role_id,
                        &next_turn,
                        memory_add_seq,
                        &reserved_memory_ids,
                    )?
                };
                reserved_memory_ids.insert(id.clone());
                changes.push(RoleMemoryChangeOp::Add {
                    role_id: role_id.clone(),
                    id,
                    body,
                });
            }
            RoleMemoryProposal::Update { id, body } => {
                if !role.memories.contains_key(&id) {
                    return Err(PipelineError::invalid_output(
                        PipelineStage::Extract,
                        "extract_memory_unknown_id",
                        format!("memory {} does not exist for role {}", id, role_id),
                    ));
                }
                if !updated_memories.insert(id.clone()) {
                    return Err(PipelineError::invalid_output(
                        PipelineStage::Extract,
                        "extract_memory_duplicate_id",
                        format!("memory {} was updated more than once for role {}", id, role_id),
                    ));
                }
                validate_activation_rule(&body.activation)?;
                changes.push(RoleMemoryChangeOp::Update {
                    role_id: role_id.clone(),
                    id,
                    body,
                });
            }
        }
    }
    Ok(changes)
}

pub(super) fn assemble_knowledge_change(
    world_extraction: WorldKnowledgeExtraction,
    role_memories: Vec<Vec<RoleMemoryChangeOp>>,
) -> KnowledgeChange {
    KnowledgeChange {
        facts: world_extraction.facts,
        rumors: world_extraction.rumors,
        memories: role_memories.into_iter().flatten().collect(),
    }
}

fn allocate_fact_id_with_reserved(
    story_ctx: &StoryContext,
    next_turn: &TurnNumber,
    seq: usize,
    reserved: &BTreeSet<FactId>,
) -> Result<FactId, PipelineError> {
    let mut candidate_seq = seq.max(1);
    loop {
        let candidate = FactId::try_new(format!("fact_t{}_{}", next_turn.value(), candidate_seq)).map_err(|error| {
            PipelineError::inconsistent_data(PipelineStage::Extract, "extract_fact_id_allocation", error.to_string())
        })?;
        if !story_ctx.facts.contains_key(&candidate) && !reserved.contains(&candidate) {
            return Ok(candidate);
        }
        candidate_seq += 1;
    }
}

fn allocate_rumor_id_with_reserved(
    story_ctx: &StoryContext,
    next_turn: &TurnNumber,
    seq: usize,
    reserved: &BTreeSet<RumorId>,
) -> Result<RumorId, PipelineError> {
    let mut candidate_seq = seq.max(1);
    loop {
        let candidate =
            RumorId::try_new(format!("rumor_t{}_{}", next_turn.value(), candidate_seq)).map_err(|error| {
                PipelineError::inconsistent_data(
                    PipelineStage::Extract,
                    "extract_rumor_id_allocation",
                    error.to_string(),
                )
            })?;
        if !story_ctx.rumors.contains_key(&candidate) && !reserved.contains(&candidate) {
            return Ok(candidate);
        }
        candidate_seq += 1;
    }
}

fn allocate_memory_id_with_reserved(
    story_ctx: &StoryContext,
    role_id: &RoleId,
    next_turn: &TurnNumber,
    seq: usize,
    reserved: &BTreeSet<MemoryId>,
) -> Result<MemoryId, PipelineError> {
    let role = story_ctx.roles.get(role_id).ok_or_else(|| {
        PipelineError::invalid_output(
            PipelineStage::Extract,
            "extract_role_unknown",
            format!("role {} does not exist in story context", role_id),
        )
    })?;
    let mut candidate_seq = seq.max(1);
    loop {
        let candidate =
            MemoryId::try_new(format!("mem_t{}_{}", next_turn.value(), candidate_seq)).map_err(|error| {
                PipelineError::inconsistent_data(
                    PipelineStage::Extract,
                    "extract_memory_id_allocation",
                    error.to_string(),
                )
            })?;
        if !role.memories.contains_key(&candidate) && !reserved.contains(&candidate) {
            return Ok(candidate);
        }
        candidate_seq += 1;
    }
}

#[cfg(test)]
#[path = "tests/extract_knowledge_tests.rs"]
mod tests;
