use crate::core::story_proposal::StoryProposal;
use crate::core::turn_budget::TurnBudget;
use crate::core::turn_contract::{
    CommittedTurnResult, LlmBudgetReservation, LlmCallUsage, TurnControl, TurnIdentity, TurnPhase, TurnRequest,
};
use crate::core::turn_data::{BaselineContext, CharacterThought, ContextItem, WriterPlan};
use crate::core::turn_error::{TurnExecutionError, TurnFailureKind, TurnTerminalKind};
use crate::core::turn_pipeline::TurnStage;
use crate::core::turn_trace::{PendingSpan, TraceRecorder};
use crate::core::turn_validation::{ValidatedChangeSet, ValidationResult};
use crate::domain::ids::{StoryId, TurnId};
use crate::domain::story_state::StoryReadSnapshot;
use serde::Serialize;
use std::time::Instant;

pub struct TurnExecutionContext {
    identity: TurnIdentity,
    phase: TurnPhase,
    request: TurnRequest,
    control: TurnControl,
    budget: TurnBudget,
    trace: TraceRecorder,
    snapshot: Option<StoryReadSnapshot>,
    baseline: Option<BaselineContext>,
    plan: Option<WriterPlan>,
    retrieved: Vec<ContextItem>,
    thoughts: Vec<CharacterThought>,
    proposal: Option<StoryProposal>,
    proposal_revision: u32,
    validation: Option<ValidationResult>,
    change_set: Option<ValidatedChangeSet>,
    committed_result: Option<CommittedTurnResult>,
    terminal_kind: Option<TurnTerminalKind>,
    terminal_error: Option<TurnExecutionError>,
    llm_calls: Vec<crate::core::turn_contract::LlmCallUsage>,
}

impl TurnExecutionContext {
    pub fn new(
        identity: TurnIdentity,
        request: TurnRequest,
        budget: TurnBudget,
        control: TurnControl,
        trace: TraceRecorder,
    ) -> Result<Self, TurnExecutionError> {
        if budget.remaining_output_tokens() == 0 {
            return Err(TurnExecutionError::new(
                TurnFailureKind::InvalidRequest,
                "zero_output_budget",
                None,
                "turn budget max_output_tokens must be positive",
            ));
        }
        Ok(Self {
            identity,
            phase: TurnPhase::Created,
            request,
            control,
            budget,
            trace,
            snapshot: None,
            baseline: None,
            plan: None,
            retrieved: Vec::new(),
            thoughts: Vec::new(),
            proposal: None,
            proposal_revision: 0,
            validation: None,
            change_set: None,
            committed_result: None,
            terminal_kind: None,
            terminal_error: None,
            llm_calls: Vec::new(),
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

    pub fn snapshot(&self) -> Option<&StoryReadSnapshot> {
        self.snapshot.as_ref()
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

    pub fn llm_calls(&self) -> &[crate::core::turn_contract::LlmCallUsage] {
        &self.llm_calls
    }

    pub fn complete_initialization(&mut self) -> Result<(), TurnExecutionError> {
        self.expect_phase(TurnPhase::Created)?;
        self.phase = TurnPhase::Initialized;
        Ok(())
    }

    pub fn set_prepared_context(
        &mut self,
        snapshot: StoryReadSnapshot,
        baseline: BaselineContext,
    ) -> Result<(), TurnExecutionError> {
        self.expect_phase(TurnPhase::Initialized)?;
        let estimated = baseline.estimate_tokens();
        if estimated > self.budget.max_context_tokens() {
            return Err(TurnExecutionError::new(
                TurnFailureKind::InvariantViolation,
                "context_token_budget_exceeded",
                Some(TurnStage::Context),
                format!(
                    "prepared context {estimated} tokens exceeds budget {}",
                    self.budget.max_context_tokens()
                ),
            ));
        }
        self.snapshot = Some(snapshot);
        self.baseline = Some(baseline);
        self.phase = TurnPhase::Prepared;
        Ok(())
    }

    pub fn set_writer_plan(&mut self, plan: WriterPlan) -> Result<(), TurnExecutionError> {
        self.expect_phase(TurnPhase::Prepared)?;
        let serialized = serde_json::to_string(&plan).map_err(|error| {
            TurnExecutionError::new(
                TurnFailureKind::InvariantViolation,
                "plan_serialization_failed",
                Some(TurnStage::WriterPlanner),
                error.to_string(),
            )
        })?;
        if serialized.len() > self.budget.max_plan_bytes() {
            return Err(TurnExecutionError::new(
                TurnFailureKind::InvariantViolation,
                "plan_byte_limit",
                Some(TurnStage::WriterPlanner),
                format!(
                    "writer plan {} bytes exceeds budget {}",
                    serialized.len(),
                    self.budget.max_plan_bytes()
                ),
            ));
        }
        self.plan = Some(plan);
        self.phase = TurnPhase::Planned;
        Ok(())
    }

    pub fn set_retrieved_context(&mut self, items: Vec<ContextItem>) -> Result<(), TurnExecutionError> {
        self.expect_phase(TurnPhase::Planned)?;
        if items.len() > self.budget.max_retrieved_items() {
            return Err(TurnExecutionError::new(
                TurnFailureKind::InvariantViolation,
                "retrieved_item_limit",
                Some(TurnStage::Context),
                format!(
                    "retrieved context {} exceeds budget {}",
                    items.len(),
                    self.budget.max_retrieved_items()
                ),
            ));
        }
        let mut total_bytes = 0usize;
        let mut total_tokens = 0u64;
        for item in &items {
            total_bytes = total_bytes.saturating_add(item.content.len());
            total_tokens = total_tokens.saturating_add((item.content.chars().count() as u64).saturating_add(3) / 4);
        }
        if total_bytes > self.budget.max_retrieved_item_bytes() {
            return Err(TurnExecutionError::new(
                TurnFailureKind::InvariantViolation,
                "retrieved_item_byte_limit",
                Some(TurnStage::Context),
                format!(
                    "retrieved context {total_bytes} bytes exceeds budget {}",
                    self.budget.max_retrieved_item_bytes()
                ),
            ));
        }
        if total_tokens > self.budget.max_retrieved_tokens() {
            return Err(TurnExecutionError::new(
                TurnFailureKind::InvariantViolation,
                "retrieved_token_limit",
                Some(TurnStage::Context),
                format!(
                    "retrieved context {total_tokens} tokens exceeds budget {}",
                    self.budget.max_retrieved_tokens()
                ),
            ));
        }
        self.retrieved = items;
        Ok(())
    }

    pub fn set_character_thoughts(&mut self, thoughts: Vec<CharacterThought>) -> Result<(), TurnExecutionError> {
        self.expect_phase(TurnPhase::Planned)?;
        if thoughts.len() > self.budget.max_character_thoughts() {
            return Err(TurnExecutionError::new(
                TurnFailureKind::InvariantViolation,
                "character_thought_limit",
                Some(TurnStage::CharacterThink),
                format!(
                    "character thoughts {} exceeds budget {}",
                    thoughts.len(),
                    self.budget.max_character_thoughts()
                ),
            ));
        }
        let mut total_bytes = 0usize;
        for thought in &thoughts {
            total_bytes = total_bytes
                .saturating_add(thought.perception.len())
                .saturating_add(thought.emotion.len())
                .saturating_add(thought.goal.len())
                .saturating_add(thought.possible_action.len());
        }
        if total_bytes > self.budget.max_character_thought_bytes() {
            return Err(TurnExecutionError::new(
                TurnFailureKind::InvariantViolation,
                "character_thought_byte_limit",
                Some(TurnStage::CharacterThink),
                format!(
                    "character thoughts {total_bytes} bytes exceeds budget {}",
                    self.budget.max_character_thought_bytes()
                ),
            ));
        }
        self.thoughts = thoughts;
        Ok(())
    }

    pub fn complete_context_preparation(&mut self) -> Result<(), TurnExecutionError> {
        self.expect_phase(TurnPhase::Planned)?;
        self.phase = TurnPhase::ContextReady;
        Ok(())
    }

    pub fn requires_retrieval(&self) -> Result<bool, TurnExecutionError> {
        // TODO(temp-debug): retrieval is temporarily disabled while debugging the baseline builder.
        // Restore the original logic: require plan, then return `!plan.retrieval_requests.is_empty()`.
        Ok(false)
    }

    pub fn skip_retrieval(&mut self) -> Result<(), TurnExecutionError> {
        self.expect_phase(TurnPhase::Planned)?;
        self.retrieved = Vec::new();
        Ok(())
    }

    pub fn requires_character_thinking(&self) -> Result<bool, TurnExecutionError> {
        // TODO(temp-debug): character thinking is temporarily disabled while debugging the baseline builder.
        // Restore the original logic: require plan, then return `!plan.character_requests.is_empty()`.
        Ok(false)
    }

    pub fn skip_character_thinking(&mut self) -> Result<(), TurnExecutionError> {
        self.expect_phase(TurnPhase::Planned)?;
        self.thoughts = Vec::new();
        Ok(())
    }

    pub fn set_story_proposal(&mut self, proposal: StoryProposal) -> Result<(), TurnExecutionError> {
        self.expect_phase(TurnPhase::ContextReady)?;
        let serialized = serde_json::to_string(&proposal).map_err(|error| {
            TurnExecutionError::new(
                TurnFailureKind::InvariantViolation,
                "proposal_serialization_failed",
                Some(TurnStage::StoryGenerator),
                error.to_string(),
            )
        })?;
        if serialized.len() > self.budget.max_proposal_bytes() {
            return Err(TurnExecutionError::new(
                TurnFailureKind::InvariantViolation,
                "proposal_byte_limit",
                Some(TurnStage::StoryGenerator),
                format!(
                    "story proposal {} bytes exceeds budget {}",
                    serialized.len(),
                    self.budget.max_proposal_bytes()
                ),
            ));
        }
        self.proposal = Some(proposal);
        self.phase = TurnPhase::ProposalReady;
        Ok(())
    }

    pub fn set_validation_result(&mut self, result: ValidationResult) -> Result<(), TurnExecutionError> {
        self.expect_phase(TurnPhase::ProposalReady)?;
        for issue in result.issues() {
            if issue.message.len() > self.budget.max_validation_issue_bytes() {
                return Err(TurnExecutionError::new(
                    TurnFailureKind::InvariantViolation,
                    "validation_issue_byte_limit",
                    Some(TurnStage::Validation),
                    format!(
                        "validation issue message {} bytes exceeds budget {}",
                        issue.message.len(),
                        self.budget.max_validation_issue_bytes()
                    ),
                ));
            }
        }
        let decision = result.decision();
        self.change_set = result.clone().into_change_set();
        self.phase = decision.to_turn_phase();
        self.validation = Some(result);
        Ok(())
    }

    pub fn replace_story_proposal(&mut self, proposal: StoryProposal) -> Result<(), TurnExecutionError> {
        self.expect_phase(TurnPhase::RepairRequired)?;
        self.proposal = Some(proposal);
        self.validation = None;
        self.change_set = None;
        self.proposal_revision = self.proposal_revision.saturating_add(1);
        self.phase = TurnPhase::ProposalReady;
        Ok(())
    }

    pub fn validation_decision(&self) -> Result<crate::core::turn_validation::ValidationDecision, TurnExecutionError> {
        match &self.validation {
            Some(result) => Ok(result.decision()),
            None => Err(TurnExecutionError::new(
                TurnFailureKind::InvariantViolation,
                "no_validation_result",
                None,
                "no validation result",
            )),
        }
    }

    pub fn consume_repair_round(&mut self) -> Result<(), TurnExecutionError> {
        self.budget.consume_repair_round()
    }

    pub fn change_set(&self) -> Option<&ValidatedChangeSet> {
        self.change_set.as_ref()
    }

    pub fn proposal_revision(&self) -> u32 {
        self.proposal_revision
    }

    pub fn set_committed_result(&mut self, result: CommittedTurnResult) -> Result<(), TurnExecutionError> {
        self.expect_phase(TurnPhase::ReadyToCommit)?;
        self.committed_result = Some(result);
        self.phase = TurnPhase::Committed;
        Ok(())
    }

    pub fn mark_failed(&mut self, failure: &TurnExecutionError) -> Result<(), TurnExecutionError> {
        self.expect_terminal(TurnTerminalKind::Failed)?;
        self.terminal_kind = Some(TurnTerminalKind::Failed);
        self.phase = TurnPhase::Failed;
        self.terminal_error = Some(failure.clone());
        Ok(())
    }

    pub fn mark_cancelled(&mut self, failure: &TurnExecutionError) -> Result<(), TurnExecutionError> {
        self.expect_terminal(TurnTerminalKind::Cancelled)?;
        self.terminal_kind = Some(TurnTerminalKind::Cancelled);
        self.phase = TurnPhase::Cancelled;
        self.terminal_error = Some(failure.clone());
        Ok(())
    }

    pub fn mark_conflict(&mut self, failure: &TurnExecutionError) -> Result<(), TurnExecutionError> {
        self.expect_terminal(TurnTerminalKind::Conflict)?;
        self.terminal_kind = Some(TurnTerminalKind::Conflict);
        self.phase = TurnPhase::Conflict;
        self.terminal_error = Some(failure.clone());
        Ok(())
    }

    pub fn terminal_kind(&self) -> Option<TurnTerminalKind> {
        self.terminal_kind
    }

    pub fn terminal_phase(&self) -> Option<TurnPhase> {
        if self.is_terminal() { Some(self.phase) } else { None }
    }

    pub fn terminal_error(&self) -> Option<&TurnExecutionError> {
        self.terminal_error.as_ref()
    }

    pub fn is_terminal(&self) -> bool {
        self.phase.is_terminal()
    }

    fn expect_terminal(&self, expected: TurnTerminalKind) -> Result<(), TurnExecutionError> {
        match self.terminal_kind {
            None => Ok(()),
            Some(actual) if actual == expected => Ok(()),
            Some(actual) => Err(TurnExecutionError::new(
                TurnFailureKind::InvariantViolation,
                "terminal_kind_already_set",
                None,
                format!("terminal kind already set to {actual:?}, cannot set {expected:?}"),
            )),
        }
    }

    fn expect_phase(&self, expected: TurnPhase) -> Result<(), TurnExecutionError> {
        if self.phase != expected {
            return Err(TurnExecutionError::new(
                TurnFailureKind::InvariantViolation,
                "invalid_phase_transition",
                None,
                format!("invalid phase transition: expected {expected:?}, current {:?}", self.phase),
            ));
        }
        Ok(())
    }

    pub fn llm_call_scope(&mut self, stage: TurnStage) -> TurnLlmCallScope<'_> {
        TurnLlmCallScope {
            identity: &self.identity,
            control: &self.control,
            budget: &mut self.budget,
            trace: &mut self.trace,
            llm_calls: &mut self.llm_calls,
            stage,
        }
    }
}

pub struct TurnLlmCallScope<'a> {
    identity: &'a TurnIdentity,
    control: &'a TurnControl,
    budget: &'a mut TurnBudget,
    trace: &'a mut TraceRecorder,
    llm_calls: &'a mut Vec<crate::core::turn_contract::LlmCallUsage>,
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

    pub fn reserve_llm(
        &mut self,
        estimated_input: u64,
        requested_output: u64,
    ) -> Result<LlmBudgetReservation, TurnExecutionError> {
        self.budget.reserve_llm(estimated_input, requested_output)
    }

    pub fn release_llm(&mut self, reservation: LlmBudgetReservation) {
        self.budget.release_llm(reservation);
    }

    pub fn settle_llm(
        &mut self,
        reservation: LlmBudgetReservation,
        usage: LlmCallUsage,
    ) -> Result<(), TurnExecutionError> {
        self.budget.settle_llm(reservation, usage.clone())?;
        self.llm_calls.push(usage);
        Ok(())
    }

    pub fn begin_llm_span(&mut self) -> PendingSpan {
        self.trace.begin_span("aise.llm_call", "llm.call")
    }

    pub fn end_llm_span<S: Serialize>(&mut self, span: PendingSpan, payload: &S) {
        self.trace.end_span_with(span, payload);
    }
}
