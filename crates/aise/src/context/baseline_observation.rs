use crate::context::error::ContextError;
use crate::domain::story_instance::snapshot::StoryReadSnapshot;
use crate::domain::turn::BaselineContext;
use crate::persistence::store::StoreError;
use crate::turn::observability::{
    METADATA_ACTIVATED_COUNT, METADATA_CANDIDATE_COUNT, METADATA_REQUEST_COUNT, METADATA_RETURNED_ITEM_COUNT,
    METADATA_SNAPSHOT_REVISION, ObservationAttribute, ObservationCaptureConfig, ObservationError, ObservationFinish,
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
    pub(crate) fn begin<T: serde::Serialize>(
        step: ObservationStep,
        story_id: Option<StoryId>,
        capture: ObservationCaptureConfig,
        input: &T,
    ) -> Self {
        Self {
            span: ObservationSpan::begin_captured(step, Vec::new(), capture, input),
            story_id,
        }
    }

    pub(crate) async fn in_scope<F: std::future::Future>(&self, future: F) -> F::Output {
        self.span.in_scope(future).await
    }

    pub(crate) fn finish<T: BaselineObservationOutcome>(self, outcome: &Result<T, T::Error>) {
        self.span
            .finish_captured(T::finish(&self.story_id, outcome), &T::output(outcome));
    }
}

pub(crate) trait BaselineObservationOutcome {
    type Error;

    fn finish(story_id: &Option<StoryId>, outcome: &Result<Self, Self::Error>) -> ObservationFinish
    where
        Self: Sized;

    fn output(outcome: &Result<Self, Self::Error>) -> serde_json::Value
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

    fn output(outcome: &Result<Self, Self::Error>) -> serde_json::Value {
        match outcome {
            Ok(snapshot) => serde_json::json!({
                "found": true,
                "base_revision": snapshot.base_revision().get(),
                "graph_revision": snapshot.graph_revision(),
                "role_count": snapshot.roles().len(),
                "relationship_count": snapshot.relationships().len(),
                "constraint_count": snapshot.active_constraints().len()
            }),
            Err(_) => serde_json::json!({"found": false}),
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

    fn output(outcome: &Result<Self, Self::Error>) -> serde_json::Value {
        match outcome {
            Ok((baseline, narrative, activation)) => serde_json::json!({
                "candidate_count": activation.index_snapshot.metadata.len(),
                "loaded_entry_count": activation.loaded_entries.len(),
                "activated_count": activation.continuation.activated.len(),
                "relevant_fact_count": baseline.relevant_world_knowledge.facts.len(),
                "relevant_rumor_count": baseline.relevant_world_knowledge.rumors.len(),
                "relevant_role_count": baseline.relevant_roles.len(),
                "role_index_count": baseline.role_index.len(),
                "knowledge_index_count": baseline.knowledge_index.len(),
                "narrative_projection": {
                    "active_node_count": narrative.plan.active_nodes.len(),
                    "world_event_intent_count": narrative.plan.world_event_intents.len(),
                    "character_impulse_count": narrative.plan.character_impulses.len(),
                    "effect_disposition_count": narrative.plan.effect_dispositions.len()
                }
            }),
            Err(_) => serde_json::json!({"completed": false}),
        }
    }
}
