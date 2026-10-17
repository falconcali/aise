use crate::core::{
    ChatMessage, FactBody, FactId, MemoryBody, MemoryId, NarrativeConditionId, RoleId, RoleMemoryChangeOp, RumorBody,
    RumorId, SemanticQuery, StoryContext, Truth, TurnControl,
};
use crate::llm::{LlmCompletionSpec, LlmError, LlmGateway};
use crate::pipeline::common::{PipelineError, PipelineStage};
use crate::pipeline::extract::{
    ExtractConfig,
    extract_knowledge::{self, FactProposal, RoleMemoryProposal, RumorProposal, WorldKnowledgeExtraction},
};
use crate::trace::Observation;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EvaluateNarrativeOutputDto {
    evaluations: Vec<SemanticEvaluationDto>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SemanticEvaluationDto {
    condition_id: NarrativeConditionId,
    truth: Truth,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorldKnowledgeLlmDto {
    facts: Vec<FactProposalDto>,
    rumors: Vec<RumorProposalDto>,
    roles_for_memory: Vec<RoleId>,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum FactProposalDto {
    Add { body: FactBody },
    Update { id: FactId, body: FactBody },
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum RumorProposalDto {
    Add { body: RumorBody },
    Update { id: RumorId, body: RumorBody },
    Remove { id: RumorId },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RoleMemoryLlmDto {
    memories: Vec<RoleMemoryProposalDto>,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum RoleMemoryProposalDto {
    Add { body: MemoryBody },
    Update { id: MemoryId, body: MemoryBody },
}

pub(super) async fn evaluate_narrative(
    gateway: &LlmGateway,
    messages: Vec<ChatMessage>,
    control: &TurnControl,
    observation: &Observation,
) -> Result<String, LlmError> {
    gateway
        .complete(LlmCompletionSpec { messages }, control, observation)
        .await
        .map(|result| result.content)
}

pub(super) async fn extract_world_knowledge(
    gateway: &LlmGateway,
    messages: Vec<ChatMessage>,
    control: &TurnControl,
    observation: &Observation,
) -> Result<String, LlmError> {
    gateway
        .complete(LlmCompletionSpec { messages }, control, observation)
        .await
        .map(|result| result.content)
}

pub(super) async fn extract_role_memory(
    gateway: &LlmGateway,
    messages: Vec<ChatMessage>,
    control: &TurnControl,
    observation: &Observation,
) -> Result<String, LlmError> {
    gateway
        .complete(LlmCompletionSpec { messages }, control, observation)
        .await
        .map(|result| result.content)
}

pub(super) fn parse_narrative_evaluations(
    raw: &str,
    queries: &[SemanticQuery],
) -> Result<BTreeMap<NarrativeConditionId, Truth>, PipelineError> {
    let output = serde_json::from_str::<EvaluateNarrativeOutputDto>(raw).map_err(|error| {
        PipelineError::invalid_output(
            PipelineStage::Extract,
            "extract_narrative_invalid_json",
            format!(
                "invalid narrative evaluation JSON at line {} column {}",
                error.line(),
                error.column()
            ),
        )
    })?;
    let expected = queries.iter().map(|query| &query.condition_id).collect::<BTreeSet<_>>();
    let mut evaluations = BTreeMap::new();
    for evaluation in output.evaluations {
        if evaluations.contains_key(&evaluation.condition_id) {
            return Err(PipelineError::invalid_output(
                PipelineStage::Extract,
                "extract_narrative_duplicate_condition",
                format!("condition {} was returned more than once", evaluation.condition_id),
            ));
        }
        if !expected.contains(&evaluation.condition_id) {
            return Err(PipelineError::invalid_output(
                PipelineStage::Extract,
                "extract_narrative_unknown_condition",
                format!("condition {} was not requested", evaluation.condition_id),
            ));
        }
        evaluations.insert(evaluation.condition_id, evaluation.truth);
    }
    let missing = queries
        .iter()
        .filter(|query| !evaluations.contains_key(&query.condition_id))
        .map(|query| query.condition_id.as_str())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(PipelineError::invalid_output(
            PipelineStage::Extract,
            "extract_narrative_missing_condition",
            format!("missing narrative conditions: {}", missing.join(", ")),
        ));
    }
    Ok(evaluations)
}

pub(super) fn parse_world_knowledge(
    raw: &str,
    story_ctx: &StoryContext,
    config: &ExtractConfig,
) -> Result<WorldKnowledgeExtraction, PipelineError> {
    let dto = serde_json::from_str::<WorldKnowledgeLlmDto>(raw).map_err(|error| {
        PipelineError::invalid_output(
            PipelineStage::Extract,
            "extract_world_invalid_json",
            format!(
                "invalid world knowledge JSON at line {} column {}: {}",
                error.line(),
                error.column(),
                error
            ),
        )
    })?;
    let fact_proposals = dto
        .facts
        .into_iter()
        .map(|proposal| match proposal {
            FactProposalDto::Add { body } => FactProposal::Add { body },
            FactProposalDto::Update { id, body } => FactProposal::Update { id, body },
        })
        .collect();
    let rumor_proposals = dto
        .rumors
        .into_iter()
        .map(|proposal| match proposal {
            RumorProposalDto::Add { body } => RumorProposal::Add { body },
            RumorProposalDto::Update { id, body } => RumorProposal::Update { id, body },
            RumorProposalDto::Remove { id } => RumorProposal::Remove { id },
        })
        .collect();
    extract_knowledge::resolve_world_knowledge(story_ctx, config, fact_proposals, rumor_proposals, dto.roles_for_memory)
}

pub(super) fn parse_role_memory(
    raw: &str,
    story_ctx: &StoryContext,
    role_id: &RoleId,
    config: &ExtractConfig,
) -> Result<Vec<RoleMemoryChangeOp>, PipelineError> {
    let dto = serde_json::from_str::<RoleMemoryLlmDto>(raw).map_err(|error| {
        PipelineError::invalid_output(
            PipelineStage::Extract,
            "extract_memory_invalid_json",
            format!(
                "invalid role memory JSON for role {} at line {} column {}: {}",
                role_id,
                error.line(),
                error.column(),
                error
            ),
        )
    })?;
    let proposals = dto
        .memories
        .into_iter()
        .map(|proposal| match proposal {
            RoleMemoryProposalDto::Add { body } => RoleMemoryProposal::Add { body },
            RoleMemoryProposalDto::Update { id, body } => RoleMemoryProposal::Update { id, body },
        })
        .collect();
    extract_knowledge::resolve_role_memories(story_ctx, config, role_id, proposals)
}

#[cfg(test)]
#[path = "tests/extract_llm_tests.rs"]
mod tests;
