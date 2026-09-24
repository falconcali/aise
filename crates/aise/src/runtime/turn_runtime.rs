use crate::runtime::turn_pipeline_set::TurnPipelineSet;
//use crate::turn::turn_budget::CorrectionKind;
use crate::turn::observability::{
    METADATA_CHARACTER_THINKING_SKIPPED, METADATA_RETRIEVAL_SKIPPED, METADATA_SKIP_REASON, ObservationAttribute,
    ObservationCaptureConfig, ObservationError, ObservationFinish, ObservationSpan, ObservationStatus, ObservationStep,
};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_contract::TurnPhase;
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};
use crate::turn::turn_event::{TurnEvent, TurnEventSink};
use crate::turn::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use opentelemetry::Context;
use std::time::Instant;

pub struct TurnRuntime {
    pipeline_set: TurnPipelineSet,
}

impl TurnRuntime {
    pub fn new(pipeline_set: TurnPipelineSet) -> Self {
        Self { pipeline_set }
    }

    pub async fn run(
        &self,
        ctx: &mut TurnExecutionContext,
        sink: &dyn TurnEventSink,
        parent: &Context,
        capture: &ObservationCaptureConfig,
    ) -> Result<(), TurnExecutionError> {
        let run_span = ObservationSpan::begin_with_parent_captured(
            ObservationStep::RunTurnPipelines,
            Vec::new(),
            parent,
            capture.clone(),
            &serde_json::json!({"phase": format!("{:?}", ctx.phase()).to_lowercase()}),
        );
        let run_context = run_span.context();
        let result = run_span.in_scope(self.run_inner(ctx, sink, &run_context, capture)).await;
        let mut metadata = vec![
            ObservationAttribute::bool(METADATA_RETRIEVAL_SKIPPED, ctx.retrieval_skipped()),
            ObservationAttribute::bool(METADATA_CHARACTER_THINKING_SKIPPED, ctx.character_thinking_skipped()),
        ];
        if ctx.retrieval_skipped() || ctx.character_thinking_skipped() {
            let mut reasons = Vec::new();
            if ctx.retrieval_skipped() {
                reasons.push("retrieval:not_required".to_owned());
            }
            if ctx.character_thinking_skipped() {
                reasons.push("character_thinking:not_required".to_owned());
            }
            metadata.push(ObservationAttribute::string_list(METADATA_SKIP_REASON, reasons));
        }
        let finish = match &result {
            Ok(()) => ObservationFinish {
                status: ObservationStatus::Ok,
                metadata,
                ..ObservationFinish::default()
            },
            Err(error) => ObservationFinish {
                status: runtime_status(error),
                metadata,
                error: Some(runtime_error(error)),
                ..ObservationFinish::default()
            },
        };
        run_span.finish_captured(
            finish,
            &serde_json::json!({
                "completed": result.is_ok(),
                "phase": format!("{:?}", ctx.phase()).to_lowercase(),
                "story_revision": ctx.committed_result().map(|value| value.story_revision.get())
            }),
        );
        result
    }

    async fn run_inner(
        &self,
        ctx: &mut TurnExecutionContext,
        sink: &dyn TurnEventSink,
        parent: &Context,
        capture: &ObservationCaptureConfig,
    ) -> Result<(), TurnExecutionError> {
        self.execute(self.pipeline_set.initializer(), ctx, sink, parent, capture)
            .await?;
        self.execute(self.pipeline_set.baseline_builder(), ctx, sink, parent, capture)
            .await?;
        self.execute(self.pipeline_set.writer_planner(), ctx, sink, parent, capture)
            .await?;

        if ctx.requires_retrieval()? {
            self.execute(self.pipeline_set.retrieval(), ctx, sink, parent, capture).await?;
        } else {
            ctx.skip_retrieval()?;
        }

        if ctx.requires_character_thinking()? {
            self.execute(self.pipeline_set.character_think(), ctx, sink, parent, capture)
                .await?;
        } else {
            ctx.skip_character_thinking()?;
        }

        ctx.complete_context_preparation()?;
        self.execute(self.pipeline_set.story_generator(), ctx, sink, parent, capture)
            .await?;

        // loop {
        //     if matches!(ctx.phase(), TurnPhase::StoryReady | TurnPhase::StateReextractionRequired) {
        //         self.execute(self.pipeline_set.story_state_extractor(), ctx, sink).await?;
        //     }
        //     if ctx.phase() == TurnPhase::CandidateReady {
        //         self.execute(self.pipeline_set.validation(), ctx, sink).await?;
        //         let decision = ctx.validation_decision()?;
        //         let issue_codes = ctx
        //             .validation()
        //             .map(|result| result.issues().iter().map(|issue| issue.code).collect::<Vec<_>>())
        //             .unwrap_or_default();
        //         let _ = sink.emit(TurnEvent::ValidationCompleted {
        //             turn_number: Some(ctx.turn_number()),
        //             attempt: ctx.budget().correction_rounds().saturating_add(1),
        //             decision,
        //             issue_codes,
        //         });
        //     }
        //     match ctx.phase() {
        //         TurnPhase::ReadyToCommit => break,
        //         TurnPhase::Failed => {
        //             return Err(ctx.validation_rejected_error().unwrap_or_else(|error| error));
        //         }
        //         TurnPhase::StoryRepairRequired => {
        //             ctx.consume_correction_round(CorrectionKind::StoryRepair)?;
        //             self.execute(self.pipeline_set.story_repairer(), ctx, sink).await?;
        //         }
        //         TurnPhase::StateReextractionRequired => {
        //             ctx.consume_correction_round(CorrectionKind::StateReextraction)?;
        //         }
        //         other => {
        //             return Err(invariant(format!("unexpected turn phase in correction loop: {other:?}")));
        //         }
        //     }
        // }

        ctx.set_pahse(TurnPhase::ReadyToCommit); // TODO : Debug Code
        ctx.construct_changeset()?; // TODO : Debug Code

        self.execute(self.pipeline_set.committer(), ctx, sink, parent, capture).await?;
        ctx.committed_result()
            .map(|_| ())
            .ok_or_else(|| invariant("committed turn missing committed result".to_string()))
    }

    async fn execute(
        &self,
        pipeline: &dyn TurnExecutionPipeline,
        ctx: &mut TurnExecutionContext,
        sink: &dyn TurnEventSink,
        parent: &Context,
        capture: &ObservationCaptureConfig,
    ) -> Result<(), TurnExecutionError> {
        let stage = pipeline.stage();
        if let Some(entries) = stage_entry_phases(stage) {
            if !entries.contains(&ctx.phase()) {
                return Err(invariant(format!(
                    "pipeline {} entered with unexpected phase {:?}, expected one of {entries:?}",
                    stage.as_str(),
                    ctx.phase()
                )));
            }
        }
        if ctx.control().cancellation().is_cancelled() {
            return Err(TurnExecutionError::cancelled(Some(stage)));
        }
        if Instant::now() >= ctx.control().deadline() {
            return Err(TurnExecutionError::deadline_exceeded(Some(stage)));
        }
        let _ = sink.emit(TurnEvent::StageStarted {
            turn_number: Some(ctx.turn_number()),
            stage,
        });
        let input = pipeline.observation_input(ctx);
        let observation = ObservationSpan::begin_with_parent_captured(
            observation_step(stage),
            Vec::new(),
            parent,
            capture.clone(),
            &input,
        );
        let outcome = observation.in_scope(pipeline.execute(ctx)).await;
        let finish = match &outcome {
            Ok(()) => ObservationFinish {
                status: ObservationStatus::Ok,
                ..ObservationFinish::default()
            },
            Err(error) => ObservationFinish {
                status: runtime_status(error),
                error: Some(runtime_error(error)),
                ..ObservationFinish::default()
            },
        };
        let output = pipeline.observation_output(ctx, outcome.is_ok());
        observation.finish_captured(finish, &output);
        if outcome.is_ok() {
            if let Some(exits) = stage_exit_phases(stage) {
                if !exits.contains(&ctx.phase()) {
                    return Err(invariant(format!(
                        "pipeline {} completed with unexpected phase {:?}, expected one of {exits:?}",
                        stage.as_str(),
                        ctx.phase()
                    )));
                }
            }
        }
        outcome
    }
}

pub const fn observation_step(stage: TurnStage) -> ObservationStep {
    match stage {
        TurnStage::TurnInitializer => ObservationStep::InitializeTurn,
        TurnStage::BaselineBuilder => ObservationStep::PrepareContext,
        TurnStage::WriterPlanner => ObservationStep::PlanTurn,
        TurnStage::ContextRetrieval => ObservationStep::RetrieveContext,
        TurnStage::CharacterThink => ObservationStep::ThinkCharacters,
        TurnStage::StoryGenerator => ObservationStep::GenerateStory,
        TurnStage::StoryStateExtractor => ObservationStep::ExtractStoryState,
        TurnStage::Validation => ObservationStep::ValidateStory,
        TurnStage::StoryRepairer => ObservationStep::RepairStory,
        TurnStage::TurnCommitter => ObservationStep::CommitTurn,
        TurnStage::Context => ObservationStep::PrepareContext,
    }
}

fn runtime_error(error: &TurnExecutionError) -> ObservationError {
    ObservationError {
        code: error.code().into(),
        failure_kind: format!("{:?}", error.kind()).to_lowercase(),
        stage: error.stage().map(|stage| stage.as_str().into()),
        message: error.to_string(),
    }
}

fn runtime_status(error: &TurnExecutionError) -> ObservationStatus {
    match error.kind() {
        TurnFailureKind::Cancelled => ObservationStatus::Cancelled,
        TurnFailureKind::DeadlineExceeded => ObservationStatus::DeadlineExceeded,
        TurnFailureKind::RevisionConflict | TurnFailureKind::IdempotencyConflict => ObservationStatus::Conflict,
        _ => ObservationStatus::Error,
    }
}

fn stage_entry_phases(stage: TurnStage) -> Option<&'static [TurnPhase]> {
    match stage {
        TurnStage::TurnInitializer => Some(&[TurnPhase::Created]),
        TurnStage::BaselineBuilder => Some(&[TurnPhase::Initialized]),
        TurnStage::WriterPlanner => Some(&[TurnPhase::Prepared]),
        TurnStage::ContextRetrieval => Some(&[TurnPhase::Planned]),
        TurnStage::CharacterThink => Some(&[TurnPhase::Planned]),
        TurnStage::StoryGenerator => Some(&[TurnPhase::ContextReady]),
        TurnStage::StoryStateExtractor => Some(&[TurnPhase::StoryReady, TurnPhase::StateReextractionRequired]),
        TurnStage::Validation => Some(&[TurnPhase::CandidateReady]),
        TurnStage::StoryRepairer => Some(&[TurnPhase::StoryRepairRequired]),
        TurnStage::TurnCommitter => Some(&[TurnPhase::ReadyToCommit]),
        TurnStage::Context => None,
    }
}

fn stage_exit_phases(stage: TurnStage) -> Option<&'static [TurnPhase]> {
    match stage {
        TurnStage::TurnInitializer => Some(&[TurnPhase::Initialized]),
        TurnStage::BaselineBuilder => Some(&[TurnPhase::Prepared]),
        TurnStage::WriterPlanner => Some(&[TurnPhase::Planned]),
        TurnStage::ContextRetrieval => None,
        TurnStage::CharacterThink => None,
        TurnStage::StoryGenerator => Some(&[TurnPhase::StoryReady]),
        TurnStage::StoryStateExtractor => Some(&[TurnPhase::CandidateReady, TurnPhase::StateReextractionRequired]),
        TurnStage::Validation => Some(&[
            TurnPhase::ReadyToCommit,
            TurnPhase::StoryRepairRequired,
            TurnPhase::StateReextractionRequired,
            TurnPhase::Failed,
        ]),
        TurnStage::StoryRepairer => Some(&[TurnPhase::StoryReady]),
        TurnStage::TurnCommitter => Some(&[TurnPhase::Committed]),
        TurnStage::Context => None,
    }
}

fn invariant(message: String) -> TurnExecutionError {
    TurnExecutionError::new(TurnFailureKind::InvariantViolation, "turn_runtime_invariant", None, message)
}
