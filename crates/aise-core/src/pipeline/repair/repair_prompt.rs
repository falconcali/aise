use crate::core::StoryContext;
use crate::persistence::Store;
use crate::pipeline::common::validation_prompt::{ValidationPromptEvidence, project_validation_evidence};
use crate::pipeline::common::{PipelineError, PipelineStage};
use crate::pipeline::repair::{RepairConfig, RepairInput};
use crate::prompt::{Prompt, PromptSpec, PromptVars, RenderedPrompt};
use crate::trace::Observation;
use serde_json::Value;

const REPAIR_STORY_PROMPT_ID: &str = "repair.repair_story";
const STORY_SUMMARY_VAR: &str = "story_summary";
const STORY_OPENING_VAR: &str = "story_opening";
const RECENT_STORY_VAR: &str = "recent_story";
const STORY_GOAL_VAR: &str = "story_goal";
const PLAYER_CONTRIBUTION_VAR: &str = "player_contribution";
const NARRATIVE_DIRECTIONS_VAR: &str = "narrative_directions";
const STORY_CONSTRAINTS_VAR: &str = "story_constraints";
const ACTIVATED_FACTS_VAR: &str = "activated_facts";
const ACTIVATED_RUMORS_VAR: &str = "activated_rumors";
const RETRIEVED_FACTS_VAR: &str = "retrieved_facts";
const RETRIEVED_RUMORS_VAR: &str = "retrieved_rumors";
const PLAYER_ROLE_VAR: &str = "player_role";
const RESOLVED_ROLES_VAR: &str = "resolved_roles";
const CHARACTER_DECISIONS_VAR: &str = "character_decisions";
const CURRENT_PROPOSAL_VAR: &str = "current_proposal";
const PROPOSAL_VERSION_VAR: &str = "proposal_version";
const REPAIR_ISSUES_VAR: &str = "repair_issues";
const MAX_PROPOSAL_BYTES_VAR: &str = "max_proposal_bytes";

pub(super) async fn repair_story(
    prompt: &Prompt,
    story_ctx: &StoryContext,
    input: &RepairInput<'_>,
    config: &RepairConfig,
    store: &Store,
    observation: &Observation,
) -> Result<RenderedPrompt, PipelineError> {
    let evidence = project_validation_evidence(PipelineStage::Repair, story_ctx, &input.context, store).await?;
    let vars = repair_vars(input, config, evidence)?;
    prompt
        .render(PromptSpec::new(REPAIR_STORY_PROMPT_ID, vars), observation)
        .map_err(PipelineError::prompt(PipelineStage::Repair))
}

fn repair_vars(
    input: &RepairInput<'_>,
    config: &RepairConfig,
    evidence: ValidationPromptEvidence,
) -> Result<PromptVars, PipelineError> {
    let repair_issues = serde_json::to_string(input.issues).map_err(|error| {
        PipelineError::inconsistent_data(PipelineStage::Repair, "repair_issues_serialization", error.to_string())
    })?;
    Ok(PromptVars::from([
        (STORY_SUMMARY_VAR.to_owned(), Value::String(evidence.story_summary)),
        (STORY_OPENING_VAR.to_owned(), Value::String(evidence.story_opening)),
        (RECENT_STORY_VAR.to_owned(), Value::String(evidence.recent_story)),
        (STORY_GOAL_VAR.to_owned(), Value::String(evidence.story_goal)),
        (PLAYER_CONTRIBUTION_VAR.to_owned(), Value::String(evidence.player_contribution)),
        (
            NARRATIVE_DIRECTIONS_VAR.to_owned(),
            Value::String(evidence.narrative_directions),
        ),
        (STORY_CONSTRAINTS_VAR.to_owned(), Value::String(evidence.story_constraints)),
        (ACTIVATED_FACTS_VAR.to_owned(), Value::String(evidence.activated_facts)),
        (ACTIVATED_RUMORS_VAR.to_owned(), Value::String(evidence.activated_rumors)),
        (RETRIEVED_FACTS_VAR.to_owned(), Value::String(evidence.retrieved_facts)),
        (RETRIEVED_RUMORS_VAR.to_owned(), Value::String(evidence.retrieved_rumors)),
        (PLAYER_ROLE_VAR.to_owned(), Value::String(evidence.player_role)),
        (RESOLVED_ROLES_VAR.to_owned(), Value::String(evidence.resolved_roles)),
        (CHARACTER_DECISIONS_VAR.to_owned(), Value::String(evidence.character_decisions)),
        (CURRENT_PROPOSAL_VAR.to_owned(), Value::String(input.proposal.to_owned())),
        (PROPOSAL_VERSION_VAR.to_owned(), Value::from(input.proposal_version)),
        (REPAIR_ISSUES_VAR.to_owned(), Value::String(repair_issues)),
        (MAX_PROPOSAL_BYTES_VAR.to_owned(), Value::from(config.max_proposal_bytes as u64)),
    ]))
}

#[cfg(test)]
#[path = "tests/repair_prompt_tests.rs"]
mod tests;
