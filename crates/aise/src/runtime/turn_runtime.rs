use crate::runtime::turn_pipeline_set::TurnPipelineSet;
use crate::turn::turn_budget::CorrectionKind;
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_contract::TurnPhase;
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};
use crate::turn::turn_event::{TurnEvent, TurnEventSink};
use crate::turn::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::turn::turn_trace::{PipelineData, SpanPayload};
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
    ) -> Result<(), TurnExecutionError> {
        self.execute(self.pipeline_set.initializer(), ctx, sink).await?;
        self.execute(self.pipeline_set.baseline_builder(), ctx, sink).await?;
        self.execute(self.pipeline_set.writer_planner(), ctx, sink).await?;

        if ctx.requires_retrieval()? {
            self.execute(self.pipeline_set.retrieval(), ctx, sink).await?;
        } else {
            ctx.skip_retrieval()?;
        }

        if ctx.requires_character_thinking()? {
            self.execute(self.pipeline_set.character_think(), ctx, sink).await?;
        } else {
            ctx.skip_character_thinking()?;
        }

        ctx.complete_context_preparation()?;
        self.execute(self.pipeline_set.story_generator(), ctx, sink).await?;

        loop {
            if matches!(ctx.phase(), TurnPhase::StoryReady | TurnPhase::StateReextractionRequired) {
                self.execute(self.pipeline_set.story_state_extractor(), ctx, sink).await?;
            }
            if ctx.phase() == TurnPhase::CandidateReady {
                self.execute(self.pipeline_set.validation(), ctx, sink).await?;
                let decision = ctx.validation_decision()?;
                let issue_codes = ctx
                    .validation()
                    .map(|result| result.issues().iter().map(|issue| issue.code).collect::<Vec<_>>())
                    .unwrap_or_default();
                let _ = sink.emit(TurnEvent::ValidationCompleted {
                    turn_number: Some(ctx.turn_number()),
                    attempt: ctx.budget().correction_rounds().saturating_add(1),
                    decision,
                    issue_codes,
                });
            }
            match ctx.phase() {
                TurnPhase::ReadyToCommit => break,
                TurnPhase::Failed => {
                    return Err(ctx.validation_rejected_error().unwrap_or_else(|error| error));
                }
                TurnPhase::StoryRepairRequired => {
                    ctx.consume_correction_round(CorrectionKind::StoryRepair)?;
                    self.execute(self.pipeline_set.story_repairer(), ctx, sink).await?;
                }
                TurnPhase::StateReextractionRequired => {
                    ctx.consume_correction_round(CorrectionKind::StateReextraction)?;
                }
                other => {
                    return Err(invariant(format!("unexpected turn phase in correction loop: {other:?}")));
                }
            }
        }

        self.execute(self.pipeline_set.committer(), ctx, sink).await?;
        ctx.committed_result()
            .map(|_| ())
            .ok_or_else(|| invariant("committed turn missing committed result".to_string()))
    }

    async fn execute(
        &self,
        pipeline: &dyn TurnExecutionPipeline,
        ctx: &mut TurnExecutionContext,
        sink: &dyn TurnEventSink,
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
        let pending = ctx.trace().begin_span("aise.pipeline", stage.as_str());
        let outcome = pipeline.execute(ctx).await;
        let payload = match &outcome {
            Ok(()) => SpanPayload::Pipeline(PipelineData {
                stage: stage.as_str().to_owned(),
                status: "ok".into(),
                error: None,
            }),
            Err(error) => SpanPayload::Pipeline(PipelineData {
                stage: stage.as_str().to_owned(),
                status: "error".into(),
                error: Some(error.to_string()),
            }),
        };
        ctx.trace().end_span_with(pending, &payload);
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
