use crate::runtime::turn_pipeline_set::TurnPipelineSet;
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_contract::TurnPhase;
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};
use crate::turn::turn_event::{TurnEvent, TurnEventSink};
use crate::turn::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::turn::turn_trace::{PipelineData, SpanPayload};
use crate::turn::turn_validation::ValidationDecision;
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
            self.execute(self.pipeline_set.validation(), ctx, sink).await?;
            let decision = ctx.validation_decision()?;
            let issue_codes = ctx
                .validation()
                .map(|result| result.issues().iter().map(|issue| issue.code).collect::<Vec<_>>())
                .unwrap_or_default();
            let _ = sink.emit(TurnEvent::ValidationCompleted {
                turn_id: ctx.turn_id().clone(),
                attempt: ctx.proposal_revision().saturating_add(1),
                decision,
                issue_codes,
            });
            match decision {
                ValidationDecision::Pass => break,
                ValidationDecision::Repair => {
                    ctx.consume_repair_round()?;
                    self.execute(self.pipeline_set.story_repairer(), ctx, sink).await?;
                }
                ValidationDecision::Reject => {
                    let detail = ctx
                        .validation()
                        .map(|result| {
                            result
                                .issues()
                                .iter()
                                .map(|issue| format!("{}: {}", issue.code, issue.message))
                                .collect::<Vec<_>>()
                                .join("; ")
                        })
                        .unwrap_or_else(|| "validation rejected".into());
                    return Err(TurnExecutionError::validation_rejected(detail));
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
        if let Some(entry) = stage_entry_phase(stage) {
            if ctx.phase() != entry {
                return Err(invariant(format!(
                    "pipeline {} entered with unexpected phase {:?}, expected {entry:?}",
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
            turn_id: ctx.turn_id().clone(),
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

fn stage_entry_phase(stage: TurnStage) -> Option<TurnPhase> {
    match stage {
        TurnStage::TurnInitializer => Some(TurnPhase::Created),
        TurnStage::BaselineBuilder => Some(TurnPhase::Initialized),
        TurnStage::WriterPlanner => Some(TurnPhase::Prepared),
        TurnStage::ContextRetrieval => Some(TurnPhase::Planned),
        TurnStage::CharacterThink => Some(TurnPhase::Planned),
        TurnStage::StoryGenerator => Some(TurnPhase::ContextReady),
        TurnStage::Validation => Some(TurnPhase::ProposalReady),
        TurnStage::StoryRepairer => Some(TurnPhase::RepairRequired),
        TurnStage::TurnCommitter => Some(TurnPhase::ReadyToCommit),
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
        TurnStage::StoryGenerator => Some(&[TurnPhase::ProposalReady]),
        TurnStage::Validation => Some(&[TurnPhase::RepairRequired, TurnPhase::ReadyToCommit, TurnPhase::Failed]),
        TurnStage::StoryRepairer => Some(&[TurnPhase::ProposalReady]),
        TurnStage::TurnCommitter => Some(&[TurnPhase::Committed]),
        TurnStage::Context => None,
    }
}

fn invariant(message: String) -> TurnExecutionError {
    TurnExecutionError::new(TurnFailureKind::InvariantViolation, "turn_runtime_invariant", None, message)
}
