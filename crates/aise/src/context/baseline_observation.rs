use crate::context::error::ContextError;
use crate::domain::narrative_graph::projector::NarrativeProjection;
use crate::domain::story_instance::snapshot::StoryReadSnapshot;
use crate::domain::turn::BaselineContext;
use crate::persistence::store::StoreError;
use crate::turn::observability::{
    BoundedContent, BoundedContentEncoder, METADATA_ACTIVATED_COUNT, METADATA_CANDIDATE_COUNT,
    METADATA_CONTENT_ENCODE_FAILED, METADATA_REQUEST_COUNT, METADATA_RETURNED_ITEM_COUNT, METADATA_SNAPSHOT_REVISION,
    ObservationAttribute, ObservationError, ObservationFields, ObservationFinish, ObservationSpan, ObservationStatus,
    ObservationStep, TRACE_METADATA_STORY_ID,
};
use crate::turn::turn_context::PreparedActivation;
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_pipeline::TurnStage;
use serde::Serialize;

pub(crate) struct BaselineObservation {
    span: ObservationSpan,
    encoder: Option<BoundedContentEncoder>,
    remaining_content_bytes: usize,
}

impl BaselineObservation {
    pub(crate) fn begin(step: ObservationStep, ctx: &TurnExecutionContext) -> Self {
        let mut span = ObservationSpan::begin(step, ObservationFields::default());
        let encoder = ctx.observation_encoder().cloned();
        let max_observation_bytes = encoder.as_ref().map_or(0, BoundedContentEncoder::max_observation_bytes);
        let (input, encoding_failed) = if span.is_recording() {
            encode_input(encoder.as_ref(), step, ctx, max_observation_bytes)
        } else {
            (None, false)
        };
        let captured_bytes = input.as_ref().map_or(0, |content| content.captured_bytes);
        span.record_fields(ObservationFields {
            metadata: encoding_metadata(encoding_failed),
            input,
        });
        Self {
            span,
            encoder,
            remaining_content_bytes: max_observation_bytes.saturating_sub(captured_bytes),
        }
    }

    pub(crate) async fn in_scope<F: std::future::Future>(&self, future: F) -> F::Output {
        self.span.in_scope(future).await
    }

    pub(crate) fn finish<T: BaselineObservationOutcome>(
        self,
        ctx: &TurnExecutionContext,
        outcome: &Result<T, T::Error>,
    ) {
        let encoder = if self.span.is_recording() {
            self.encoder.as_ref()
        } else {
            None
        };
        let finish = T::finish(ctx, outcome, encoder, self.remaining_content_bytes);
        self.span.finish(finish);
    }
}

pub(crate) trait BaselineObservationOutcome {
    type Error;

    fn finish(
        ctx: &TurnExecutionContext,
        outcome: &Result<Self, Self::Error>,
        encoder: Option<&BoundedContentEncoder>,
        remaining_content_bytes: usize,
    ) -> ObservationFinish
    where
        Self: Sized;
}

impl BaselineObservationOutcome for StoryReadSnapshot {
    type Error = StoreError;

    fn finish(
        ctx: &TurnExecutionContext,
        outcome: &Result<Self, Self::Error>,
        encoder: Option<&BoundedContentEncoder>,
        remaining_content_bytes: usize,
    ) -> ObservationFinish {
        match outcome {
            Ok(snapshot) => {
                let mut metadata = vec![
                    ObservationAttribute::string(TRACE_METADATA_STORY_ID, ctx.story_id().as_str()),
                    ObservationAttribute::u64(METADATA_SNAPSHOT_REVISION, snapshot.base_revision().get()),
                    ObservationAttribute::u64(METADATA_RETURNED_ITEM_COUNT, snapshot.roles().len() as u64),
                ];
                let (output, encoding_failed) = encode(
                    encoder,
                    &LoadStorySnapshotOutput {
                        story_id: snapshot.story_id().as_str(),
                        base_revision: snapshot.base_revision().get(),
                        graph_revision: snapshot.graph_revision(),
                        story_title: snapshot.story_title().as_str(),
                        player_role_id: snapshot.player_role_id().as_str(),
                        role_count: snapshot.roles().len(),
                        relationship_count: snapshot.relationships().len(),
                        fact_value_count: snapshot.fact_values().len(),
                        active_constraint_count: snapshot.active_constraints().len(),
                        story_summary: snapshot.story_continuity().summary(),
                        recent_story: snapshot.story_continuity().recent_segments(),
                    },
                    remaining_content_bytes,
                );
                if encoding_failed {
                    metadata.push(ObservationAttribute::bool(METADATA_CONTENT_ENCODE_FAILED, true));
                }
                ObservationFinish {
                    status: ObservationStatus::Ok,
                    metadata,
                    output,
                    ..ObservationFinish::default()
                }
            }
            Err(error) => {
                let (output, encoding_failed) = encode(
                    encoder,
                    &BaselineFailureOutput {
                        status: "error",
                        error_code: "story_snapshot_load_failed",
                        failure_kind: "store",
                        stage: TurnStage::BaselineBuilder.as_str(),
                    },
                    remaining_content_bytes,
                );
                ObservationFinish {
                    status: ObservationStatus::Error,
                    metadata: encoding_metadata(encoding_failed),
                    output,
                    error: Some(ObservationError {
                        code: "story_snapshot_load_failed".into(),
                        failure_kind: "store".into(),
                        stage: Some(TurnStage::BaselineBuilder.as_str().into()),
                        message: error.to_string(),
                    }),
                    ..ObservationFinish::default()
                }
            }
        }
    }
}

impl BaselineObservationOutcome for (BaselineContext, NarrativeProjection, PreparedActivation) {
    type Error = ContextError;

    fn finish(
        ctx: &TurnExecutionContext,
        outcome: &Result<Self, Self::Error>,
        encoder: Option<&BoundedContentEncoder>,
        remaining_content_bytes: usize,
    ) -> ObservationFinish {
        match outcome {
            Ok((baseline, narrative_projection, activation)) => {
                let mut metadata = vec![
                    ObservationAttribute::u64(METADATA_REQUEST_COUNT, activation.loaded_entries.len() as u64),
                    ObservationAttribute::u64(
                        METADATA_CANDIDATE_COUNT,
                        activation.index_snapshot.metadata.len() as u64,
                    ),
                    ObservationAttribute::u64(METADATA_ACTIVATED_COUNT, activation.continuation.activated.len() as u64),
                ];
                let (output, encoding_failed) = encode(
                    encoder,
                    &ActivateWorldInfoOutput {
                        story_id: ctx.story_id().as_str(),
                        turn_number: ctx.turn_number().get(),
                        baseline,
                        narrative_projection,
                        activation: ActivationSummary {
                            loaded_entry_count: activation.loaded_entries.len(),
                            candidate_count: activation.index_snapshot.metadata.len(),
                            activated_count: activation.continuation.activated.len(),
                            activated: &activation.continuation.activated,
                        },
                    },
                    remaining_content_bytes,
                );
                if encoding_failed {
                    metadata.push(ObservationAttribute::bool(METADATA_CONTENT_ENCODE_FAILED, true));
                }
                ObservationFinish {
                    status: ObservationStatus::Ok,
                    metadata,
                    output,
                    ..ObservationFinish::default()
                }
            }
            Err(error) => {
                let (output, encoding_failed) = encode(
                    encoder,
                    &BaselineFailureOutput {
                        status: "error",
                        error_code: error.turn_code(),
                        failure_kind: "context",
                        stage: TurnStage::BaselineBuilder.as_str(),
                    },
                    remaining_content_bytes,
                );
                ObservationFinish {
                    status: ObservationStatus::Error,
                    metadata: encoding_metadata(encoding_failed),
                    output,
                    error: Some(ObservationError {
                        code: error.turn_code().into(),
                        failure_kind: "context".into(),
                        stage: Some(TurnStage::BaselineBuilder.as_str().into()),
                        message: error.to_string(),
                    }),
                    ..ObservationFinish::default()
                }
            }
        }
    }
}

#[derive(Serialize)]
struct LoadStorySnapshotInput<'a> {
    story_id: &'a str,
    turn_number: u64,
}

#[derive(Serialize)]
struct ActivateWorldInfoInput<'a> {
    story_id: &'a str,
    turn_number: u64,
    player_contribution: &'a str,
}

#[derive(Serialize)]
struct LoadStorySnapshotOutput<'a> {
    story_id: &'a str,
    base_revision: u64,
    graph_revision: u64,
    story_title: &'a str,
    player_role_id: &'a str,
    role_count: usize,
    relationship_count: usize,
    fact_value_count: usize,
    active_constraint_count: usize,
    story_summary: &'a crate::domain::narrative::StorySummary,
    recent_story: &'a [crate::domain::narrative::StorySegment],
}

#[derive(Serialize)]
struct ActivateWorldInfoOutput<'a> {
    story_id: &'a str,
    turn_number: u64,
    baseline: &'a BaselineContext,
    narrative_projection: &'a NarrativeProjection,
    activation: ActivationSummary<'a>,
}

#[derive(Serialize)]
struct ActivationSummary<'a> {
    loaded_entry_count: usize,
    candidate_count: usize,
    activated_count: usize,
    activated: &'a std::collections::BTreeMap<
        crate::domain::knowledge::KnowledgeSourceId,
        crate::domain::knowledge::activation::ActivatedKnowledgeRef,
    >,
}

#[derive(Serialize)]
struct BaselineFailureOutput<'a> {
    status: &'static str,
    error_code: &'a str,
    failure_kind: &'static str,
    stage: &'static str,
}

fn encode_input(
    encoder: Option<&BoundedContentEncoder>,
    step: ObservationStep,
    ctx: &TurnExecutionContext,
    remaining_content_bytes: usize,
) -> (Option<BoundedContent>, bool) {
    match step {
        ObservationStep::LoadStorySnapshot => encode(
            encoder,
            &LoadStorySnapshotInput {
                story_id: ctx.story_id().as_str(),
                turn_number: ctx.turn_number().get(),
            },
            remaining_content_bytes,
        ),
        ObservationStep::ActivateWorldInfo => encode(
            encoder,
            &ActivateWorldInfoInput {
                story_id: ctx.story_id().as_str(),
                turn_number: ctx.turn_number().get(),
                player_contribution: ctx.player_contribution(),
            },
            remaining_content_bytes,
        ),
        _ => (None, false),
    }
}

fn encode<T: Serialize>(
    encoder: Option<&BoundedContentEncoder>,
    value: &T,
    remaining_content_bytes: usize,
) -> (Option<BoundedContent>, bool) {
    encoder
        .map(|encoder| encoder.encode_with_status(value, remaining_content_bytes))
        .unwrap_or((None, false))
}

fn encoding_metadata(encoding_failed: bool) -> Vec<ObservationAttribute> {
    if encoding_failed {
        vec![ObservationAttribute::bool(METADATA_CONTENT_ENCODE_FAILED, true)]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
#[path = "tests/baseline_observation_tests.rs"]
mod tests;
