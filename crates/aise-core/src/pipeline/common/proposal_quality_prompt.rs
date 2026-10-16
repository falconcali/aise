use crate::pipeline::common::{PipelineError, PipelineStage, ProposalQualityPolicy};

pub(in crate::pipeline) struct ProposalQualityPrompt {
    pub check_criteria: String,
    pub score_criteria: String,
}

pub(in crate::pipeline) fn project_proposal_quality(
    stage: PipelineStage,
    policy: &ProposalQualityPolicy,
) -> Result<ProposalQualityPrompt, PipelineError> {
    let check_criteria = serialize(stage, policy.checks())?;
    let score_criteria = serialize(stage, policy.scores())?;
    Ok(ProposalQualityPrompt {
        check_criteria,
        score_criteria,
    })
}

fn serialize<T: serde::Serialize + ?Sized>(stage: PipelineStage, value: &T) -> Result<String, PipelineError> {
    serde_json::to_string(value)
        .map_err(|error| PipelineError::inconsistent_data(stage, "proposal_quality_serialization", error.to_string()))
}

#[cfg(test)]
#[path = "tests/proposal_quality_prompt_tests.rs"]
mod tests;
