use crate::pipeline::common::{PipelineError, PipelineStage, ProposalQualityPolicy};

pub(in crate::pipeline) struct ProposalQualityPrompt {
    pub check_criteria: String,
    pub score_criteria: String,
}

pub(in crate::pipeline) fn project_proposal_quality(
    stage: PipelineStage,
    policy: &ProposalQualityPolicy,
) -> Result<ProposalQualityPrompt, PipelineError> {
    let check_criteria = serialize_optional_slice(stage, policy.checks())?;
    let score_criteria = serialize_optional_slice(stage, policy.scores())?;
    Ok(ProposalQualityPrompt {
        check_criteria,
        score_criteria,
    })
}

fn serialize<T: serde::Serialize + ?Sized>(stage: PipelineStage, value: &T) -> Result<String, PipelineError> {
    serde_json::to_string(value)
        .map_err(|error| PipelineError::inconsistent_data(stage, "proposal_quality_serialization", error.to_string()))
}

fn serialize_optional_slice<T: serde::Serialize>(stage: PipelineStage, value: &[T]) -> Result<String, PipelineError> {
    if value.is_empty() {
        return Ok(String::new());
    }
    serialize(stage, value)
}

#[cfg(test)]
#[path = "tests/proposal_quality_prompt_tests.rs"]
mod tests;
