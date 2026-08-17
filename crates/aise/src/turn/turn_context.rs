use crate::domain::ids::{StoryId, TurnId};
use crate::domain::narrative_graph::projector::NarrativeProjection;
use crate::domain::story_instance::snapshot::StoryReadSnapshot;
use crate::domain::turn::{BaselineContext, CharacterDecision, RetrievedContext, RetrievedContextLimits, WriterPlan};
use crate::domain::turn::{StoryGeneratorOutput, StoryStateExtractionEnvelope, StoryStateExtractorOutput};
use crate::turn::turn_budget::{CorrectionKind, TurnBudget};
use crate::turn::turn_contract::{
    CommittedTurnResult, LlmBudgetReservation, LlmCallUsage, TurnControl, TurnIdentity, TurnPhase, TurnRequest,
};
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};
use crate::turn::turn_pipeline::TurnStage;
use crate::turn::turn_trace::{PendingSpan, TraceRecorder};
use crate::turn::turn_validation::{BoundedValidationIssues, ValidatedChangeSet, ValidationDecision, ValidationResult};
use serde::Serialize;
use std::time::Instant;

struct BoundStateExtraction {
    story_version: u32,
    envelope: StoryStateExtractionEnvelope,
}

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
    narrative_projection: Option<NarrativeProjection>,
    retrieved: RetrievedContext,
    character_decisions: Vec<CharacterDecision>,
    story: Option<StoryGeneratorOutput>,
    story_version: u32,
    extraction: Option<BoundStateExtraction>,
    validation: Option<ValidationResult>,
    change_set: Option<ValidatedChangeSet>,
    committed_result: Option<CommittedTurnResult>,
    terminal_kind: Option<crate::turn::turn_error::TurnTerminalKind>,
    terminal_error: Option<TurnExecutionError>,
    llm_calls: Vec<LlmCallUsage>,
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
            narrative_projection: None,
            retrieved: RetrievedContext::default(),
            character_decisions: Vec::new(),
            story: None,
            story_version: 0,
            extraction: None,
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

    pub fn budget_mut(&mut self) -> &mut TurnBudget {
        &mut self.budget
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

    pub fn narrative_projection(&self) -> Option<&NarrativeProjection> {
        self.narrative_projection.as_ref()
    }

    pub fn set_narrative_projection(&mut self, projection: NarrativeProjection) -> Result<(), TurnExecutionError> {
        self.expect_phase(TurnPhase::Prepared)?;
        self.narrative_projection = Some(projection);
        Ok(())
    }

    pub fn retrieved(&self) -> &RetrievedContext {
        &self.retrieved
    }

    pub fn character_decisions(&self) -> &[CharacterDecision] {
        &self.character_decisions
    }

    pub fn story(&self) -> Option<&StoryGeneratorOutput> {
        self.story.as_ref()
    }

    pub fn story_version(&self) -> u32 {
        self.story_version
    }

    pub fn extraction(&self) -> Option<&StoryStateExtractorOutput> {
        self.extraction.as_ref().map(|bound| &bound.envelope.state)
    }

    pub fn extraction_envelope(&self) -> Option<&StoryStateExtractionEnvelope> {
        self.extraction.as_ref().map(|bound| &bound.envelope)
    }

    pub fn extraction_story_version(&self) -> Option<u32> {
        self.extraction.as_ref().map(|bound| bound.story_version)
    }

    pub fn validation(&self) -> Option<&ValidationResult> {
        self.validation.as_ref()
    }

    pub fn committed_result(&self) -> Option<&CommittedTurnResult> {
        self.committed_result.as_ref()
    }

    pub fn llm_calls(&self) -> &[LlmCallUsage] {
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

    pub fn set_retrieved_context(&mut self, context: RetrievedContext) -> Result<(), TurnExecutionError> {
        self.expect_phase(TurnPhase::Planned)?;
        let limits = RetrievedContextLimits {
            max_role_audiences: self.budget.max_character_decisions(),
            max_items_per_audience: self.budget.max_items_per_audience(),
            max_tokens_per_audience: self.budget.max_tokens_per_audience(),
            max_total_items: self.budget.max_total_items(),
            max_total_tokens: self.budget.max_retrieved_tokens(),
            max_item_bytes: self.budget.max_item_bytes(),
        };
        let plan = self.plan.as_ref().ok_or_else(|| {
            TurnExecutionError::new(
                TurnFailureKind::InvariantViolation,
                "missing_writer_plan",
                Some(TurnStage::Context),
                "writer plan is required before retrieval",
            )
        })?;
        for role_id in context.characters().keys() {
            if !plan.character_think_requests.iter().any(|request| &request.role_id == role_id) {
                return Err(TurnExecutionError::new(
                    TurnFailureKind::InvariantViolation,
                    "retrieved_character_audience_unauthorized",
                    Some(TurnStage::Context),
                    "retrieved character audience has no matching character think request",
                ));
            }
        }
        let revalidated = RetrievedContext::try_new(context.world().clone(), context.characters().clone(), limits)
            .map_err(|error| {
                TurnExecutionError::new(
                    TurnFailureKind::InvariantViolation,
                    "retrieved_context_limit",
                    Some(TurnStage::Context),
                    error.to_string(),
                )
            })?;
        self.retrieved = revalidated;
        Ok(())
    }

    pub fn set_character_decisions(&mut self, decisions: Vec<CharacterDecision>) -> Result<(), TurnExecutionError> {
        self.expect_phase(TurnPhase::Planned)?;
        if decisions.len() > self.budget.max_character_decisions() {
            return Err(TurnExecutionError::new(
                TurnFailureKind::InvariantViolation,
                "character_decision_limit",
                Some(TurnStage::CharacterThink),
                format!(
                    "character decisions {} exceeds budget {}",
                    decisions.len(),
                    self.budget.max_character_decisions()
                ),
            ));
        }
        let plan = self.plan.as_ref().ok_or_else(|| {
            TurnExecutionError::new(
                TurnFailureKind::InvariantViolation,
                "missing_writer_plan",
                Some(TurnStage::CharacterThink),
                "writer plan is required before character decisions",
            )
        })?;
        if decisions.len() != plan.character_think_requests.len() {
            return Err(TurnExecutionError::new(
                TurnFailureKind::InvariantViolation,
                "character_decision_count_mismatch",
                Some(TurnStage::CharacterThink),
                format!(
                    "character decisions {} does not match request count {}",
                    decisions.len(),
                    plan.character_think_requests.len()
                ),
            ));
        }
        for (decision, request) in decisions.iter().zip(plan.character_think_requests.iter()) {
            if decision.role_id != request.role_id {
                return Err(TurnExecutionError::new(
                    TurnFailureKind::InvariantViolation,
                    "character_decision_order_mismatch",
                    Some(TurnStage::CharacterThink),
                    "character decision id does not match the paired request",
                ));
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        for decision in &decisions {
            if !seen.insert(decision.role_id.clone()) {
                return Err(TurnExecutionError::new(
                    TurnFailureKind::InvariantViolation,
                    "duplicate_character_decision",
                    Some(TurnStage::CharacterThink),
                    "character decision collection contains a duplicate target",
                ));
            }
        }
        if let Some(baseline) = &self.baseline {
            if decisions
                .iter()
                .any(|decision| decision.role_id == baseline.player_role.role_id)
            {
                return Err(TurnExecutionError::new(
                    TurnFailureKind::InvariantViolation,
                    "character_think_player_target",
                    Some(TurnStage::CharacterThink),
                    "character decision targets the player character",
                ));
            }
        }
        let mut total_bytes = 0usize;
        for decision in &decisions {
            total_bytes = total_bytes.saturating_add(decision.decision.as_str().len()).saturating_add(
                decision
                    .suggested_utterance
                    .as_ref()
                    .map(|value| value.as_str().len())
                    .unwrap_or(0),
            );
        }
        if total_bytes > self.budget.max_character_decision_bytes() {
            return Err(TurnExecutionError::new(
                TurnFailureKind::InvariantViolation,
                "character_decision_byte_limit",
                Some(TurnStage::CharacterThink),
                format!(
                    "character decisions {total_bytes} bytes exceeds budget {}",
                    self.budget.max_character_decision_bytes()
                ),
            ));
        }
        self.character_decisions = decisions;
        Ok(())
    }

    pub fn complete_context_preparation(&mut self) -> Result<(), TurnExecutionError> {
        self.expect_phase(TurnPhase::Planned)?;
        self.phase = TurnPhase::ContextReady;
        Ok(())
    }

    pub fn requires_retrieval(&self) -> Result<bool, TurnExecutionError> {
        self.expect_phase(TurnPhase::Planned)?;
        let plan = self.plan.as_ref().ok_or_else(|| {
            TurnExecutionError::new(
                TurnFailureKind::InvariantViolation,
                "missing_writer_plan",
                Some(TurnStage::Context),
                "writer plan is required before retrieval",
            )
        })?;
        Ok(!plan.retrieval_plan.character_requests.is_empty() || !plan.retrieval_plan.knowledge_requests.is_empty())
    }

    pub fn skip_retrieval(&mut self) -> Result<(), TurnExecutionError> {
        self.expect_phase(TurnPhase::Planned)?;
        self.retrieved = RetrievedContext::default();
        Ok(())
    }

    pub fn requires_character_thinking(&self) -> Result<bool, TurnExecutionError> {
        self.expect_phase(TurnPhase::Planned)?;
        let plan = self.plan.as_ref().ok_or_else(|| {
            TurnExecutionError::new(
                TurnFailureKind::InvariantViolation,
                "missing_writer_plan",
                Some(TurnStage::CharacterThink),
                "writer plan is required before character thinking",
            )
        })?;
        Ok(!plan.character_think_requests.is_empty())
    }

    pub fn skip_character_thinking(&mut self) -> Result<(), TurnExecutionError> {
        self.expect_phase(TurnPhase::Planned)?;
        let plan = self.plan.as_ref().ok_or_else(|| {
            TurnExecutionError::new(
                TurnFailureKind::InvariantViolation,
                "missing_writer_plan",
                Some(TurnStage::CharacterThink),
                "writer plan is required before character thinking",
            )
        })?;
        if !plan.character_think_requests.is_empty() {
            return Err(TurnExecutionError::new(
                TurnFailureKind::InvariantViolation,
                "character_thinking_not_skippable",
                Some(TurnStage::CharacterThink),
                "writer plan requests character thinking and cannot be skipped",
            ));
        }
        self.character_decisions = Vec::new();
        Ok(())
    }

    fn ensure_story_bound(&self, story: &StoryGeneratorOutput, stage: TurnStage) -> Result<(), TurnExecutionError> {
        if story.story_text.as_str().trim().is_empty() {
            return Err(TurnExecutionError::new(
                TurnFailureKind::Llm,
                "story_text_empty",
                Some(stage),
                "story text must not be empty",
            ));
        }
        if story.story_text.as_str().len() > self.budget.max_story_text_bytes() {
            return Err(TurnExecutionError::new(
                TurnFailureKind::Llm,
                "story_text_exceeds_bounds",
                Some(stage),
                "story text exceeds its byte bound",
            ));
        }
        Ok(())
    }

    fn ensure_extraction_bound(
        &self,
        envelope: &StoryStateExtractionEnvelope,
        stage: TurnStage,
    ) -> Result<(), TurnExecutionError> {
        let serialized = serde_json::to_string(envelope).map_err(|error| {
            TurnExecutionError::new(
                TurnFailureKind::InvariantViolation,
                "extraction_serialization_failed",
                Some(stage),
                error.to_string(),
            )
        })?;
        if serialized.len() > self.budget.max_state_extraction_bytes() {
            return Err(TurnExecutionError::new(
                TurnFailureKind::Llm,
                "state_extraction_exceeds_bounds",
                Some(stage),
                "state extraction output exceeds its byte bound",
            ));
        }
        Ok(())
    }

    pub fn set_generated_story(&mut self, story: StoryGeneratorOutput) -> Result<(), TurnExecutionError> {
        self.expect_phase(TurnPhase::ContextReady)?;
        self.ensure_story_bound(&story, TurnStage::StoryGenerator)?;
        self.story = Some(story);
        self.story_version = 1;
        self.extraction = None;
        self.validation = None;
        self.change_set = None;
        self.phase = TurnPhase::StoryReady;
        Ok(())
    }

    pub fn set_state_extraction(&mut self, envelope: StoryStateExtractionEnvelope) -> Result<(), TurnExecutionError> {
        self.expect_phase(TurnPhase::StoryReady)?;
        self.ensure_extraction_bound(&envelope, TurnStage::StoryStateExtractor)?;
        self.extraction = Some(BoundStateExtraction {
            story_version: self.story_version,
            envelope,
        });
        self.validation = None;
        self.change_set = None;
        self.phase = TurnPhase::CandidateReady;
        Ok(())
    }

    pub fn record_state_extraction_failure(
        &mut self,
        issues: BoundedValidationIssues,
    ) -> Result<(), TurnExecutionError> {
        self.expect_phase_one_of(&[TurnPhase::StoryReady, TurnPhase::StateReextractionRequired])?;
        for issue in issues.issues() {
            if issue.message.len() > self.budget.max_validation_issue_bytes() {
                return Err(TurnExecutionError::new(
                    TurnFailureKind::InvariantViolation,
                    "validation_issue_byte_limit",
                    Some(TurnStage::StoryStateExtractor),
                    format!(
                        "validation issue message {} bytes exceeds budget {}",
                        issue.message.len(),
                        self.budget.max_validation_issue_bytes()
                    ),
                ));
            }
        }
        self.extraction = None;
        self.change_set = None;
        self.validation = Some(ValidationResult::ReextractState(issues));
        self.phase = TurnPhase::StateReextractionRequired;
        Ok(())
    }

    pub fn set_validation_result(&mut self, result: ValidationResult) -> Result<(), TurnExecutionError> {
        self.expect_phase(TurnPhase::CandidateReady)?;
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
        match decision {
            ValidationDecision::Pass => {
                self.change_set = result.clone().into_change_set();
            }
            ValidationDecision::RepairStory => {
                self.extraction = None;
                self.change_set = None;
            }
            ValidationDecision::ReextractState => {
                self.change_set = None;
            }
            ValidationDecision::Reject => {
                self.change_set = None;
            }
        }
        self.phase = decision.to_turn_phase();
        self.validation = Some(result);
        Ok(())
    }

    pub fn replace_story(&mut self, story: StoryGeneratorOutput) -> Result<(), TurnExecutionError> {
        self.expect_phase(TurnPhase::StoryRepairRequired)?;
        self.ensure_story_bound(&story, TurnStage::StoryRepairer)?;
        if let Some(current) = &self.story {
            if current.story_text == story.story_text {
                return Err(TurnExecutionError::story_repair_no_progress(Some(TurnStage::StoryRepairer)));
            }
        }
        self.story_version = self.story_version.checked_add(1).ok_or_else(|| {
            TurnExecutionError::new(
                TurnFailureKind::InvariantViolation,
                "story_version_overflow",
                Some(TurnStage::StoryRepairer),
                "candidate story version overflow",
            )
        })?;
        self.story = Some(story);
        self.extraction = None;
        self.validation = None;
        self.change_set = None;
        self.phase = TurnPhase::StoryReady;
        Ok(())
    }

    pub fn replace_state_extraction(
        &mut self,
        envelope: StoryStateExtractionEnvelope,
    ) -> Result<(), TurnExecutionError> {
        self.expect_phase(TurnPhase::StateReextractionRequired)?;
        self.ensure_extraction_bound(&envelope, TurnStage::StoryStateExtractor)?;
        let no_progress = match (&self.extraction, &self.validation) {
            (Some(existing), Some(ValidationResult::ReextractState(_))) => {
                serde_json::to_string(&existing.envelope).ok() == serde_json::to_string(&envelope).ok()
            }
            _ => false,
        };
        if no_progress {
            return Err(TurnExecutionError::state_reextraction_no_progress(Some(
                TurnStage::StoryStateExtractor,
            )));
        }
        self.extraction = Some(BoundStateExtraction {
            story_version: self.story_version,
            envelope,
        });
        self.validation = None;
        self.change_set = None;
        self.phase = TurnPhase::CandidateReady;
        Ok(())
    }

    pub fn validation_decision(&self) -> Result<ValidationDecision, TurnExecutionError> {
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

    pub fn validation_rejected_error(&self) -> Result<TurnExecutionError, TurnExecutionError> {
        match &self.validation {
            Some(ValidationResult::Reject(issues)) => {
                let detail = issues
                    .issues()
                    .iter()
                    .map(|issue| issue.code.as_str())
                    .collect::<Vec<_>>()
                    .join(",");
                Ok(TurnExecutionError::validation_rejected(detail))
            }
            _ => Err(TurnExecutionError::new(
                TurnFailureKind::InvariantViolation,
                "no_rejected_validation_result",
                None,
                "validation result is not a rejection",
            )),
        }
    }

    pub fn consume_correction_round(&mut self, kind: CorrectionKind) -> Result<(), TurnExecutionError> {
        self.budget.consume_correction_round(kind)
    }

    pub fn change_set(&self) -> Option<&ValidatedChangeSet> {
        self.change_set.as_ref()
    }

    pub fn set_committed_result(&mut self, result: CommittedTurnResult) -> Result<(), TurnExecutionError> {
        self.expect_phase(TurnPhase::ReadyToCommit)?;
        self.committed_result = Some(result);
        self.phase = TurnPhase::Committed;
        Ok(())
    }

    pub fn mark_failed(&mut self, failure: &TurnExecutionError) -> Result<(), TurnExecutionError> {
        self.expect_terminal(crate::turn::turn_error::TurnTerminalKind::Failed)?;
        self.terminal_kind = Some(crate::turn::turn_error::TurnTerminalKind::Failed);
        self.phase = TurnPhase::Failed;
        self.terminal_error = Some(failure.clone());
        Ok(())
    }

    pub fn mark_cancelled(&mut self, failure: &TurnExecutionError) -> Result<(), TurnExecutionError> {
        self.expect_terminal(crate::turn::turn_error::TurnTerminalKind::Cancelled)?;
        self.terminal_kind = Some(crate::turn::turn_error::TurnTerminalKind::Cancelled);
        self.phase = TurnPhase::Cancelled;
        self.terminal_error = Some(failure.clone());
        Ok(())
    }

    pub fn mark_conflict(&mut self, failure: &TurnExecutionError) -> Result<(), TurnExecutionError> {
        self.expect_terminal(crate::turn::turn_error::TurnTerminalKind::Conflict)?;
        self.terminal_kind = Some(crate::turn::turn_error::TurnTerminalKind::Conflict);
        self.phase = TurnPhase::Conflict;
        self.terminal_error = Some(failure.clone());
        Ok(())
    }

    pub fn terminal_kind(&self) -> Option<crate::turn::turn_error::TurnTerminalKind> {
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

    fn expect_terminal(&self, expected: crate::turn::turn_error::TurnTerminalKind) -> Result<(), TurnExecutionError> {
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

    fn expect_phase_one_of(&self, expected: &[TurnPhase]) -> Result<(), TurnExecutionError> {
        if !expected.contains(&self.phase) {
            return Err(TurnExecutionError::new(
                TurnFailureKind::InvariantViolation,
                "invalid_phase_transition",
                None,
                format!(
                    "invalid phase transition: expected one of {expected:?}, current {:?}",
                    self.phase
                ),
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
    llm_calls: &'a mut Vec<LlmCallUsage>,
    stage: TurnStage,
}

impl TurnLlmCallScope<'_> {
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

    pub fn cancellation(&self) -> &crate::turn::turn_contract::TurnCancellation {
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

#[cfg(test)]
#[path = "tests/turn_context_tests.rs"]
mod tests;
