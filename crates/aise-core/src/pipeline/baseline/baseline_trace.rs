use crate::core::{NarrativeError, PlayerContribution, StoryContext};
use crate::pipeline::baseline::baseline_knowledge::ActivatedKnowledge;
use crate::pipeline::baseline::baseline_narrative::NarrativeProjection;
use crate::pipeline::common::PipelineError;
use crate::trace::{
    Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};

pub fn begin_narrative_projection(observation: &Observation, story_ctx: &StoryContext) -> Observation {
    observation.begin(ObservationSpec {
        name: "narrative_projection",
        kind: ObservationKind::Span,
        input: observation.capture_content(&story_ctx.narrative),
        metadata: Vec::new(),
    })
}

pub fn finish_narrative_projection(observation: Observation, result: &Result<NarrativeProjection, NarrativeError>) {
    let outcome = match result {
        Ok(narrative) => ObservationOutcome {
            status: ObservationStatus::Ok,
            output: observation.capture_content(narrative),
            ..ObservationOutcome::default()
        },
        Err(error) => ObservationOutcome {
            status: ObservationStatus::Error,
            error: Some(ObservationError {
                code: "narrative_projection_failed".into(),
                failure_kind: "narrative".into(),
                stage: Some("baseline".into()),
                message: error.to_string(),
            }),
            ..ObservationOutcome::default()
        },
    };

    observation.finish(outcome);
}

pub fn begin_knowledge_activation(observation: &Observation, player_contribution: &PlayerContribution) -> Observation {
    observation.begin(ObservationSpec {
        name: "knowledge_activation",
        kind: ObservationKind::Span,
        input: observation.capture_content(player_contribution),
        metadata: Vec::new(),
    })
}

pub fn finish_knowledge_activation(observation: Observation, result: &Result<ActivatedKnowledge, PipelineError>) {
    let outcome = match result {
        Ok(knowledge) => ObservationOutcome {
            status: ObservationStatus::Ok,
            output: observation.capture_content(knowledge),
            ..ObservationOutcome::default()
        },
        Err(error) => ObservationOutcome {
            status: ObservationStatus::Error,
            error: Some(ObservationError {
                code: "knowledge_activation_failed".into(),
                failure_kind: "pipeline".into(),
                stage: Some("baseline".into()),
                message: error.to_string(),
            }),
            ..ObservationOutcome::default()
        },
    };

    observation.finish(outcome);
}

pub fn begin_player_contribution(observation: &Observation, player_input: &str) -> Observation {
    observation.begin(ObservationSpec {
        name: "player_contribution",
        kind: ObservationKind::Span,
        input: observation.capture_content(&player_input),
        metadata: Vec::new(),
    })
}

pub fn finish_player_contribution(observation: Observation, llm_result: &Result<PlayerContribution, PipelineError>) {
    let outcome = match llm_result {
        Ok(player_contribution) => ObservationOutcome {
            status: ObservationStatus::Ok,
            output: observation.capture_content(&player_contribution),
            ..ObservationOutcome::default()
        },
        Err(error) => ObservationOutcome {
            status: ObservationStatus::Error,
            error: Some(ObservationError {
                code: "llm_error".into(),
                failure_kind: "llm".into(),
                stage: Some("baseline".into()),
                message: error.to_string(),
            }),
            ..ObservationOutcome::default()
        },
    };

    observation.finish(outcome);
}
