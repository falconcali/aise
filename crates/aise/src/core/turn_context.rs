use crate::core::story_proposal::StoryProposal;
use crate::core::turn_budget::{LlmReservation, TurnBudget};
use crate::core::turn_contract::{CommittedTurnResult, TurnControl, TurnIdentity, TurnPhase, TurnRequest};
use crate::core::turn_data::{BaselineContext, CharacterThought, ContextItem, WriterPlan};
use crate::core::turn_pipeline::TurnStage;
use crate::core::turn_trace::{PendingSpan, TraceRecorder};
use crate::core::turn_validation::ValidationResult;
use crate::domain::ids::{StoryId, TurnId};
use crate::error::AiseError;
use serde::Serialize;
use std::time::Instant;

pub struct TurnExecutionContext {
    identity: TurnIdentity,
    phase: TurnPhase,
    request: TurnRequest,
    control: TurnControl,
    budget: TurnBudget,
    trace: TraceRecorder,
    baseline: Option<BaselineContext>,
    plan: Option<WriterPlan>,
    retrieved: Vec<ContextItem>,
    thoughts: Vec<CharacterThought>,
    proposal: Option<StoryProposal>,
    validation: Option<ValidationResult>,
    committed_result: Option<CommittedTurnResult>,
}

impl TurnExecutionContext {
    pub fn new(
        identity: TurnIdentity,
        request: TurnRequest,
        budget: TurnBudget,
        control: TurnControl,
        trace: TraceRecorder,
    ) -> Result<Self, AiseError> {
        if budget.remaining_output_tokens() == 0 {
            return Err(AiseError::InvalidRequest(
                "turn budget max_output_tokens must be positive".into(),
            ));
        }
        Ok(Self {
            identity,
            phase: TurnPhase::Created,
            request,
            control,
            budget,
            trace,
            baseline: None,
            plan: None,
            retrieved: Vec::new(),
            thoughts: Vec::new(),
            proposal: None,
            validation: None,
            committed_result: None,
        })
    }

    pub fn phase(&self) -> TurnPhase {
        self.phase
    }

    pub fn identity(&self) -> &TurnIdentity {
        &self.identity
    }

    pub fn request(&self) -> &TurnRequest {
        &self.request
    }

    pub fn control(&self) -> &TurnControl {
        &self.control
    }

    pub fn budget(&self) -> &TurnBudget {
        &self.budget
    }

    pub fn trace(&mut self) -> &mut TraceRecorder {
        &mut self.trace
    }

    pub fn story_id(&self) -> &StoryId {
        self.identity.story_id()
    }

    pub fn turn_id(&self) -> &TurnId {
        self.identity.turn_id()
    }

    pub fn player_input(&self) -> &str {
        self.request.player_input()
    }

    pub fn baseline(&self) -> Option<&BaselineContext> {
        self.baseline.as_ref()
    }

    pub fn plan(&self) -> Option<&WriterPlan> {
        self.plan.as_ref()
    }

    pub fn retrieved(&self) -> &[ContextItem] {
        &self.retrieved
    }

    pub fn thoughts(&self) -> &[CharacterThought] {
        &self.thoughts
    }

    pub fn proposal(&self) -> Option<&StoryProposal> {
        self.proposal.as_ref()
    }

    pub fn validation(&self) -> Option<&ValidationResult> {
        self.validation.as_ref()
    }

    pub fn committed_result(&self) -> Option<&CommittedTurnResult> {
        self.committed_result.as_ref()
    }

    pub fn complete_initialization(&mut self) -> Result<(), AiseError> {
        self.expect_phase(TurnPhase::Created)?;
        self.phase = TurnPhase::Initialized;
        Ok(())
    }

    pub fn set_prepared_context(&mut self, baseline: BaselineContext) -> Result<(), AiseError> {
        self.expect_phase(TurnPhase::Initialized)?;
        self.baseline = Some(baseline);
        self.phase = TurnPhase::Prepared;
        Ok(())
    }

    pub fn set_writer_plan(&mut self, plan: WriterPlan) -> Result<(), AiseError> {
        self.expect_phase(TurnPhase::Prepared)?;
        self.plan = Some(plan);
        self.phase = TurnPhase::Planned;
        Ok(())
    }

    pub fn set_retrieved_context(&mut self, items: Vec<ContextItem>) -> Result<(), AiseError> {
        self.expect_phase(TurnPhase::Planned)?;
        if items.len() > self.budget.max_retrieved_items() {
            return Err(AiseError::InvariantViolation(format!(
                "retrieved context {} exceeds budget {}",
                items.len(),
                self.budget.max_retrieved_items()
            )));
        }
        self.retrieved = items;
        Ok(())
    }

    pub fn set_character_thoughts(&mut self, thoughts: Vec<CharacterThought>) -> Result<(), AiseError> {
        self.expect_phase(TurnPhase::Planned)?;
        self.thoughts = thoughts;
        Ok(())
    }

    pub fn complete_context_preparation(&mut self) -> Result<(), AiseError> {
        self.expect_phase(TurnPhase::Planned)?;
        self.phase = TurnPhase::ContextReady;
        Ok(())
    }

    pub fn set_story_proposal(&mut self, proposal: StoryProposal) -> Result<(), AiseError> {
        self.expect_phase(TurnPhase::ContextReady)?;
        self.proposal = Some(proposal);
        self.phase = TurnPhase::ProposalReady;
        Ok(())
    }

    pub fn set_validation_result(&mut self, result: ValidationResult) -> Result<(), AiseError> {
        self.expect_phase(TurnPhase::ProposalReady)?;
        let next = if result.pass {
            TurnPhase::ReadyToCommit
        } else {
            TurnPhase::Failed
        };
        self.validation = Some(result);
        self.phase = next;
        Ok(())
    }

    pub fn set_committed_result(&mut self, result: CommittedTurnResult) -> Result<(), AiseError> {
        self.expect_phase(TurnPhase::ReadyToCommit)?;
        self.committed_result = Some(result);
        self.phase = TurnPhase::Committed;
        Ok(())
    }

    fn expect_phase(&self, expected: TurnPhase) -> Result<(), AiseError> {
        if self.phase != expected {
            return Err(AiseError::InvariantViolation(format!(
                "invalid phase transition: expected {expected:?}, current {:?}",
                self.phase
            )));
        }
        Ok(())
    }

    pub fn llm_call_scope(&mut self, stage: TurnStage) -> TurnLlmCallScope<'_> {
        TurnLlmCallScope {
            identity: &self.identity,
            control: &self.control,
            budget: &mut self.budget,
            trace: &mut self.trace,
            stage,
        }
    }
}

pub struct TurnLlmCallScope<'a> {
    identity: &'a TurnIdentity,
    control: &'a TurnControl,
    budget: &'a mut TurnBudget,
    trace: &'a mut TraceRecorder,
    stage: TurnStage,
}

impl<'a> TurnLlmCallScope<'a> {
    pub fn story_id(&self) -> &StoryId {
        self.identity.story_id()
    }

    pub fn turn_id(&self) -> &TurnId {
        self.identity.turn_id()
    }

    pub fn stage(&self) -> TurnStage {
        self.stage
    }

    pub fn deadline(&self) -> Instant {
        self.control.deadline()
    }

    pub fn cancellation(&self) -> &crate::core::turn_contract::TurnCancellation {
        self.control.cancellation()
    }

    pub fn reserve_llm(&mut self, estimated_input: u64, requested_output: u64) -> Result<LlmReservation, AiseError> {
        self.budget.reserve_llm_call(estimated_input, requested_output)
    }

    pub fn settle_llm(&mut self, actual_input: u64, actual_output: u64) -> Result<(), AiseError> {
        self.budget.settle_llm_call(actual_input, actual_output)
    }

    pub fn begin_llm_span(&mut self) -> PendingSpan {
        self.trace.begin_span("aise.llm_call", "llm.call")
    }

    pub fn end_llm_span<S: Serialize>(&mut self, span: PendingSpan, payload: &S) {
        self.trace.end_span_with(span, payload);
    }
}
