use crate::core::StoryCommit;
use crate::pipeline::common::PipelineError;
use crate::trace::{
    Observation, ObservationError, ObservationKind, ObservationOutcome, ObservationSpec, ObservationStatus,
};

pub fn begin_commit(observation: &Observation, story_commit: &StoryCommit) -> Observation {
    observation.begin(ObservationSpec {
        name: "story_commit",
        kind: ObservationKind::Span,
        input: observation.capture_content(story_commit),
        metadata: Vec::new(),
    })
}

pub fn finish_commit(observation: Observation, result: &Result<StoryCommit, PipelineError>) {
    let outcome = match result {
        Ok(story_commit) => ObservationOutcome {
            status: ObservationStatus::Ok,
            output: observation.capture_content(&story_commit.turn.turn_status),
            ..ObservationOutcome::default()
        },
        Err(error) => ObservationOutcome {
            status: ObservationStatus::Error,
            error: Some(ObservationError {
                code: "commit_error".into(),
                failure_kind: "persistence".into(),
                stage: Some("commit".into()),
                message: error.to_string(),
            }),
            ..ObservationOutcome::default()
        },
    };
    observation.finish(outcome);
}
