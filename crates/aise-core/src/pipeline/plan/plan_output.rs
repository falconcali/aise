use crate::core::{BoundedText, FactId, RumorId};
use crate::pipeline::common::{PipelineError, PipelineStage};
use crate::pipeline::plan::{PlanConfig, PlanOutput, plan_candidates::PlanCandidates};
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanLlmOutputDto {
    story_goal: String,
    facts_to_retrieve: Vec<FactId>,
    rumors_to_retrieve: Vec<RumorId>,
}

pub(super) fn parse_plan_output(
    raw: &str,
    candidates: &PlanCandidates<'_>,
    config: &PlanConfig,
) -> Result<PlanOutput, PipelineError> {
    let output = serde_json::from_str::<PlanLlmOutputDto>(raw).map_err(|error| {
        PipelineError::invalid_output(
            PipelineStage::Plan,
            "plan_output_invalid_json",
            format!("invalid plan JSON at line {} column {}", error.line(), error.column()),
        )
    })?;
    if output.story_goal.trim().is_empty() {
        return Err(PipelineError::invalid_output(
            PipelineStage::Plan,
            "plan_story_goal_empty",
            "story goal must not be empty",
        ));
    }
    if output.story_goal.len() > config.max_story_goal_bytes {
        return Err(PipelineError::invalid_output(
            PipelineStage::Plan,
            "plan_story_goal_too_long",
            "story goal exceeds the configured byte limit",
        ));
    }
    validate_fact_ids(&output.facts_to_retrieve, candidates)?;
    validate_rumor_ids(&output.rumors_to_retrieve, candidates)?;
    let story_goal =
        BoundedText::try_new(output.story_goal, "story_goal", config.max_story_goal_bytes).map_err(|error| {
            PipelineError::invalid_output(PipelineStage::Plan, "plan_story_goal_too_long", error.to_string())
        })?;

    Ok(PlanOutput {
        story_goal,
        facts_to_retrieve: output.facts_to_retrieve,
        rumors_to_retrieve: output.rumors_to_retrieve,
    })
}

fn validate_fact_ids(ids: &[FactId], candidates: &PlanCandidates<'_>) -> Result<(), PipelineError> {
    let mut seen = HashSet::with_capacity(ids.len().min(candidates.facts.len()));
    for id in ids {
        if !seen.insert(id) {
            return Err(PipelineError::invalid_output(
                PipelineStage::Plan,
                "plan_duplicate_fact_id",
                "duplicate fact ID selected",
            ));
        }
        if !candidates.contains_fact(id) {
            return Err(PipelineError::invalid_output(
                PipelineStage::Plan,
                "plan_unknown_fact_id",
                "selected fact ID was not offered",
            ));
        }
    }
    Ok(())
}

fn validate_rumor_ids(ids: &[RumorId], candidates: &PlanCandidates<'_>) -> Result<(), PipelineError> {
    let mut seen = HashSet::with_capacity(ids.len().min(candidates.rumors.len()));
    for id in ids {
        if !seen.insert(id) {
            return Err(PipelineError::invalid_output(
                PipelineStage::Plan,
                "plan_duplicate_rumor_id",
                "duplicate rumor ID selected",
            ));
        }
        if !candidates.contains_rumor(id) {
            return Err(PipelineError::invalid_output(
                PipelineStage::Plan,
                "plan_unknown_rumor_id",
                "selected rumor ID was not offered",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/plan_output_tests.rs"]
mod tests;
