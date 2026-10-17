use crate::core::StoryContext;
use crate::pipeline::common::proposal_quality_prompt::project_proposal_quality;
use crate::pipeline::common::validation_prompt::{ValidationPromptEvidence, project_validation_evidence};
use crate::pipeline::common::{PipelineError, PipelineStage, ProposalQualityPolicy};
use crate::pipeline::validate::ValidateInput;
use crate::prompt::{Prompt, PromptSpec, PromptVars, RenderedPrompt};
use crate::trace::Observation;
use serde_json::Value;

const EVALUATE_STORY_PROMPT_ID: &str = "validate.evaluate_story";
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
const CHECK_CRITERIA_VAR: &str = "check_criteria";
const SCORE_CRITERIA_VAR: &str = "score_criteria";
const MAX_ISSUE_BYTES_VAR: &str = "max_issue_bytes";

pub(super) fn evaluate_story(
    prompt: &Prompt,
    story_ctx: &StoryContext,
    input: &ValidateInput<'_>,
    policy: &ProposalQualityPolicy,
    max_issue_bytes: usize,
    observation: &Observation,
) -> Result<RenderedPrompt, PipelineError> {
    let evidence = project_validation_evidence(PipelineStage::Validate, story_ctx, &input.context)?;
    let vars = evaluate_vars(input, policy, max_issue_bytes, evidence)?;
    prompt
        .render(PromptSpec::new(EVALUATE_STORY_PROMPT_ID, vars), observation)
        .map_err(PipelineError::prompt(PipelineStage::Validate))
}

fn evaluate_vars(
    input: &ValidateInput<'_>,
    policy: &ProposalQualityPolicy,
    max_issue_bytes: usize,
    evidence: ValidationPromptEvidence,
) -> Result<PromptVars, PipelineError> {
    let quality = project_proposal_quality(PipelineStage::Validate, policy)?;
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
        (CHECK_CRITERIA_VAR.to_owned(), Value::String(quality.check_criteria)),
        (SCORE_CRITERIA_VAR.to_owned(), Value::String(quality.score_criteria)),
        (MAX_ISSUE_BYTES_VAR.to_owned(), Value::from(max_issue_bytes as u64)),
    ]))
}

#[cfg(test)]
#[path = "tests/validate_prompt_tests.rs"]
mod tests;
