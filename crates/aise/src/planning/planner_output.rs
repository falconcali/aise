use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriterPlannerOutputDto {
    pub story_goal: String,
    pub writer_context_gaps: Vec<PlannerWriterContextGapDto>,
    pub character_context_gaps: Vec<PlannerCharacterContextGapDto>,
    pub character_think_requests: Vec<CharacterThinkRequestDto>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerWriterContextGapDto {
    pub target_id: String,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerCharacterContextGapDto {
    pub role_id: String,
    pub target_id: String,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterThinkRequestDto {
    pub role_id: String,
    pub reason: String,
}
