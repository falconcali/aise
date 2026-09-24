use crate::context::error::ContextError;
use crate::domain::story_instance::snapshot::StoryReadSnapshot;
use crate::domain::turn::BaselineContext;
use crate::persistence::store::StoreError;
use crate::turn::observability::{
    METADATA_ACTIVATED_COUNT, METADATA_CANDIDATE_COUNT, METADATA_REQUEST_COUNT, METADATA_RETURNED_ITEM_COUNT,
    METADATA_SNAPSHOT_REVISION, ObservationAttribute, ObservationError, ObservationFields, ObservationFinish,
    ObservationSpan, ObservationStatus, ObservationStep, TRACE_METADATA_STORY_ID,
};
use crate::turn::turn_context::PreparedActivation;
use crate::turn::turn_pipeline::TurnStage;
use crate::{domain::ids::StoryId, domain::narrative_graph::projector::NarrativeProjection};

pub(crate) struct BaselineObservation {
    span: ObservationSpan,
    story_id: Option<StoryId>,
}

impl BaselineObservation {
    pub(crate) fn begin(step: ObservationStep, story_id: Option<StoryId>) -> Self {
        Self {
            span: ObservationSpan::begin(step, ObservationFields::default()),
            story_id,
        }
    }

    pub(crate) async fn in_scope<F: std::future::Future>(&self, future: F) -> F::Output {
        self.span.in_scope(future).await
    }

    pub(crate) fn finish<T: BaselineObservationOutcome>(self, outcome: &Result<T, T::Error>) {
        self.span.finish(T::finish(&self.story_id, outcome));
    }
}

pub(crate) trait BaselineObservationOutcome {
    type Error;

    fn finish(story_id: &Option<StoryId>, outcome: &Result<Self, Self::Error>) -> ObservationFinish
    where
        Self: Sized;
}

impl BaselineObservationOutcome for StoryReadSnapshot {
    type Error = StoreError;

    fn finish(story_id: &Option<StoryId>, outcome: &Result<Self, Self::Error>) -> ObservationFinish {
        match outcome {
            Ok(snapshot) => {
                let mut metadata = vec![
                    ObservationAttribute::u64(METADATA_SNAPSHOT_REVISION, snapshot.base_revision().get()),
                    ObservationAttribute::u64(METADATA_RETURNED_ITEM_COUNT, snapshot.roles().len() as u64),
                ];
                if let Some(story_id) = story_id {
                    metadata.insert(0, ObservationAttribute::string(TRACE_METADATA_STORY_ID, story_id.as_str()));
                }
                ObservationFinish {
                    status: ObservationStatus::Ok,
                    metadata,
                    ..ObservationFinish::default()
                }
            }
            Err(error) => ObservationFinish {
                status: ObservationStatus::Error,
                error: Some(ObservationError {
                    code: "story_snapshot_load_failed".into(),
                    failure_kind: "store".into(),
                    stage: Some(TurnStage::BaselineBuilder.as_str().into()),
                    message: error.to_string(),
                }),
                ..ObservationFinish::default()
            },
        }
    }
}

impl BaselineObservationOutcome for (BaselineContext, NarrativeProjection, PreparedActivation) {
    type Error = ContextError;

    fn finish(_: &Option<StoryId>, outcome: &Result<Self, Self::Error>) -> ObservationFinish {
        match outcome {
            Ok((_, _, activation)) => ObservationFinish {
                status: ObservationStatus::Ok,
                metadata: vec![
                    ObservationAttribute::u64(METADATA_REQUEST_COUNT, activation.loaded_entries.len() as u64),
                    ObservationAttribute::u64(
                        METADATA_CANDIDATE_COUNT,
                        activation.index_snapshot.metadata.len() as u64,
                    ),
                    ObservationAttribute::u64(METADATA_ACTIVATED_COUNT, activation.continuation.activated.len() as u64),
                ],
                ..ObservationFinish::default()
            },
            Err(error) => ObservationFinish {
                status: ObservationStatus::Error,
                error: Some(ObservationError {
                    code: error.turn_code().into(),
                    failure_kind: "context".into(),
                    stage: Some(TurnStage::BaselineBuilder.as_str().into()),
                    message: error.to_string(),
                }),
                ..ObservationFinish::default()
            },
        }
    }
}
