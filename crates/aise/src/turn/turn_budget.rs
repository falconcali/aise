use crate::config::{NarrativeConfig, RetrievalConfig, StateExtractorConfig, TurnConfig, TurnContentLimitsConfig};
use crate::domain::narrative_graph::definition::NarrativeLimits;
use crate::domain::turn::StoryStateExtractionLimits;
use crate::turn::turn_contract::{LlmBudgetReservation, LlmCallId, LlmCallUsage};
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorrectionKind {
    StoryRepair,
    StateReextraction,
}

impl CorrectionKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            CorrectionKind::StoryRepair => "story_repair",
            CorrectionKind::StateReextraction => "state_reextraction",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TurnBudget {
    limits: TurnBudgetLimits,
    usage: TurnBudgetUsage,
}

#[derive(Debug, Clone)]
pub struct TurnBudgetLimits {
    pub max_repair_rounds: u32,
    pub max_llm_calls: u32,
    pub max_input_tokens: u64,
    pub max_output_tokens: u64,
    pub max_total_tokens: u64,
    pub max_candidates_per_retriever: usize,
    pub max_candidates_total: usize,
    pub max_items_per_audience: usize,
    pub max_tokens_per_audience: u64,
    pub max_total_items: usize,
    pub max_retrieved_tokens: u64,
    pub max_item_bytes: usize,
    pub max_role_bytes: usize,
    pub max_context_tokens: u64,
    pub max_character_decisions: usize,
    pub max_character_decision_bytes: usize,
    pub max_plan_bytes: usize,
    pub max_story_text_bytes: usize,
    pub max_state_extraction_bytes: usize,
    pub max_knowledge_change_bytes: usize,
    pub max_validation_issues: usize,
    pub max_validation_issue_bytes: usize,
    pub max_trace_spans: usize,
    pub state_extraction: StoryStateExtractionLimits,
    pub state_extractor_max_context_tokens: u64,
    pub state_extractor_max_output_tokens: u64,
    pub state_extractor_max_knowledge_context_items: usize,
    pub state_extractor_max_knowledge_context_tokens: u64,
    pub narrative: NarrativeLimits,
    pub max_condition_queries: usize,
    pub max_condition_evidence_bytes: usize,
    pub max_condition_reason_bytes: usize,
}

impl TurnBudgetLimits {
    pub fn from(
        turn: &TurnConfig,
        content: &TurnContentLimitsConfig,
        retrieval: &RetrievalConfig,
        state_extractor: &StateExtractorConfig,
        narrative: &NarrativeConfig,
    ) -> Self {
        Self {
            max_repair_rounds: turn.max_repair_rounds,
            max_llm_calls: turn.max_llm_calls,
            max_input_tokens: turn.max_input_tokens,
            max_output_tokens: turn.max_output_tokens,
            max_total_tokens: turn.max_total_tokens,
            max_candidates_per_retriever: retrieval.max_candidates_per_retriever,
            max_candidates_total: retrieval.max_candidates_total,
            max_items_per_audience: retrieval.max_items_per_audience,
            max_tokens_per_audience: retrieval.max_tokens_per_audience,
            max_total_items: retrieval.max_total_items,
            max_retrieved_tokens: retrieval.max_total_tokens,
            max_item_bytes: retrieval.max_item_bytes,
            max_role_bytes: content.max_role_bytes,
            max_context_tokens: turn.max_context_tokens,
            max_character_decisions: turn.max_character_decisions,
            max_character_decision_bytes: content.max_character_decision_bytes,
            max_plan_bytes: content.max_plan_bytes,
            max_story_text_bytes: content.max_story_text_bytes,
            max_state_extraction_bytes: content.max_state_extraction_bytes,
            max_knowledge_change_bytes: content.max_knowledge_change_bytes,
            max_validation_issues: turn.max_validation_issues,
            max_validation_issue_bytes: content.max_validation_issue_bytes,
            max_trace_spans: turn.max_trace_spans,
            state_extraction: StoryStateExtractionLimits {
                max_new_roles: state_extractor.max_new_roles_per_turn,
                max_role_states: state_extractor.max_role_states,
                max_relationship_states: state_extractor.max_relationship_states,
                max_knowledge_items: state_extractor.max_knowledge_items,
                max_goals_per_role: state_extractor.max_goals_per_role,
                max_attributes_per_role: state_extractor.max_attributes_per_role,
                max_item_bytes: content.max_role_bytes,
                max_role_profile_bytes: state_extractor.max_role_profile_bytes,
                max_knowledge_change_bytes: content.max_knowledge_change_bytes,
                max_cast_policy_violations: state_extractor.max_cast_policy_violations,
                max_condition_queries: narrative.max_semantic_queries_per_turn,
                max_condition_evidence_bytes: narrative.max_evidence_bytes,
                max_condition_reason_bytes: narrative.max_result_reason_bytes,
            },
            state_extractor_max_context_tokens: state_extractor.max_context_tokens,
            state_extractor_max_output_tokens: state_extractor.max_output_tokens,
            state_extractor_max_knowledge_context_items: state_extractor.max_knowledge_context_items,
            state_extractor_max_knowledge_context_tokens: state_extractor.max_knowledge_context_tokens,
            narrative: narrative.as_limits(),
            max_condition_queries: narrative.max_semantic_queries_per_turn,
            max_condition_evidence_bytes: narrative.max_evidence_bytes,
            max_condition_reason_bytes: narrative.max_result_reason_bytes,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct TurnBudgetUsage {
    llm_calls: u32,
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
    correction_rounds: u32,
    story_repair_rounds: u32,
    state_reextraction_rounds: u32,
}

impl TurnBudget {
    pub fn from_config(
        turn: &TurnConfig,
        content: &TurnContentLimitsConfig,
        retrieval: &RetrievalConfig,
        state_extractor: &StateExtractorConfig,
        narrative: &NarrativeConfig,
    ) -> Result<Self, TurnExecutionError> {
        turn.validate().map_err(|error| {
            TurnExecutionError::new(TurnFailureKind::InvalidRequest, "invalid_config", None, error.to_string())
        })?;
        content.validate().map_err(|error| {
            TurnExecutionError::new(TurnFailureKind::InvalidRequest, "invalid_config", None, error.to_string())
        })?;
        retrieval.validate().map_err(|error| {
            TurnExecutionError::new(TurnFailureKind::InvalidRequest, "invalid_config", None, error.to_string())
        })?;
        state_extractor.validate().map_err(|error| {
            TurnExecutionError::new(TurnFailureKind::InvalidRequest, "invalid_config", None, error.to_string())
        })?;
        narrative.validate().map_err(|error| {
            TurnExecutionError::new(TurnFailureKind::InvalidRequest, "invalid_config", None, error.to_string())
        })?;
        Ok(Self {
            limits: TurnBudgetLimits::from(turn, content, retrieval, state_extractor, narrative),
            usage: TurnBudgetUsage::default(),
        })
    }

    pub fn max_repair_rounds(&self) -> u32 {
        self.limits.max_repair_rounds
    }

    pub fn max_total_items(&self) -> usize {
        self.limits.max_total_items
    }

    pub fn max_items_per_audience(&self) -> usize {
        self.limits.max_items_per_audience
    }

    pub fn max_tokens_per_audience(&self) -> u64 {
        self.limits.max_tokens_per_audience
    }

    pub fn max_candidates_per_retriever(&self) -> usize {
        self.limits.max_candidates_per_retriever
    }

    pub fn max_candidates_total(&self) -> usize {
        self.limits.max_candidates_total
    }

    pub fn max_item_bytes(&self) -> usize {
        self.limits.max_item_bytes
    }

    pub fn max_role_bytes(&self) -> usize {
        self.limits.max_role_bytes
    }

    pub fn max_retrieved_tokens(&self) -> u64 {
        self.limits.max_retrieved_tokens
    }

    pub fn max_context_tokens(&self) -> u64 {
        self.limits.max_context_tokens
    }

    pub fn max_character_decisions(&self) -> usize {
        self.limits.max_character_decisions
    }

    pub fn max_character_decision_bytes(&self) -> usize {
        self.limits.max_character_decision_bytes
    }

    pub fn max_plan_bytes(&self) -> usize {
        self.limits.max_plan_bytes
    }

    pub fn max_story_text_bytes(&self) -> usize {
        self.limits.max_story_text_bytes
    }

    pub fn max_state_extraction_bytes(&self) -> usize {
        self.limits.max_state_extraction_bytes
    }

    pub fn max_knowledge_change_bytes(&self) -> usize {
        self.limits.max_knowledge_change_bytes
    }

    pub fn state_extraction_limits(&self) -> StoryStateExtractionLimits {
        self.limits.state_extraction
    }

    pub fn narrative_limits(&self) -> NarrativeLimits {
        self.limits.narrative
    }

    pub fn max_condition_queries(&self) -> usize {
        self.limits.max_condition_queries
    }

    pub fn max_condition_evidence_bytes(&self) -> usize {
        self.limits.max_condition_evidence_bytes
    }

    pub fn max_condition_reason_bytes(&self) -> usize {
        self.limits.max_condition_reason_bytes
    }

    pub fn state_extractor_max_context_tokens(&self) -> u64 {
        self.limits.state_extractor_max_context_tokens
    }

    pub fn state_extractor_max_output_tokens(&self) -> u64 {
        self.limits.state_extractor_max_output_tokens
    }

    pub fn state_extractor_max_knowledge_context_items(&self) -> usize {
        self.limits.state_extractor_max_knowledge_context_items
    }

    pub fn state_extractor_max_knowledge_context_tokens(&self) -> u64 {
        self.limits.state_extractor_max_knowledge_context_tokens
    }

    pub fn max_validation_issues(&self) -> usize {
        self.limits.max_validation_issues
    }

    pub fn max_validation_issue_bytes(&self) -> usize {
        self.limits.max_validation_issue_bytes
    }

    pub fn max_trace_spans(&self) -> usize {
        self.limits.max_trace_spans
    }

    pub fn max_llm_calls(&self) -> u32 {
        self.limits.max_llm_calls
    }

    pub fn max_input_tokens(&self) -> u64 {
        self.limits.max_input_tokens
    }

    pub fn max_output_tokens(&self) -> u64 {
        self.limits.max_output_tokens
    }

    pub fn remaining_output_tokens(&self) -> u64 {
        self.limits.max_output_tokens.saturating_sub(self.usage.output_tokens)
    }

    pub fn llm_calls(&self) -> u32 {
        self.usage.llm_calls
    }

    pub fn correction_rounds(&self) -> u32 {
        self.usage.correction_rounds
    }

    pub fn story_repair_rounds(&self) -> u32 {
        self.usage.story_repair_rounds
    }

    pub fn state_reextraction_rounds(&self) -> u32 {
        self.usage.state_reextraction_rounds
    }

    pub fn consume_correction_round(&mut self, kind: CorrectionKind) -> Result<(), TurnExecutionError> {
        if self.usage.correction_rounds >= self.limits.max_repair_rounds {
            return Err(TurnExecutionError::new(
                TurnFailureKind::ValidationBudgetExhausted,
                "validation_budget_exhausted",
                None,
                format!("validation failed after {} correction rounds", self.usage.correction_rounds),
            ));
        }
        self.usage.correction_rounds += 1;
        match kind {
            CorrectionKind::StoryRepair => self.usage.story_repair_rounds += 1,
            CorrectionKind::StateReextraction => self.usage.state_reextraction_rounds += 1,
        }
        Ok(())
    }

    pub fn input_tokens(&self) -> u64 {
        self.usage.input_tokens
    }

    pub fn output_tokens(&self) -> u64 {
        self.usage.output_tokens
    }

    pub fn total_tokens(&self) -> u64 {
        self.usage.total_tokens
    }

    pub fn reserve_llm(
        &mut self,
        input_tokens: u64,
        maximum_output_tokens: u64,
    ) -> Result<LlmBudgetReservation, TurnExecutionError> {
        if self.usage.llm_calls >= self.limits.max_llm_calls {
            return Err(self.budget_error(format!("llm call limit {} reached", self.limits.max_llm_calls)));
        }
        let projected_input = self.usage.input_tokens.saturating_add(input_tokens);
        let projected_output = self.usage.output_tokens.saturating_add(maximum_output_tokens);
        let projected_total = self
            .usage
            .total_tokens
            .saturating_add(input_tokens)
            .saturating_add(maximum_output_tokens);
        if projected_input > self.limits.max_input_tokens {
            return Err(self.budget_error("input token limit".into()));
        }
        if projected_output > self.limits.max_output_tokens {
            return Err(self.budget_error("output token limit".into()));
        }
        if projected_total > self.limits.max_total_tokens {
            return Err(self.budget_error("total token limit".into()));
        }
        self.usage.input_tokens = projected_input;
        self.usage.output_tokens = projected_output;
        self.usage.total_tokens = projected_total;
        Ok(LlmBudgetReservation::new(LlmCallId::new(), input_tokens, maximum_output_tokens))
    }

    pub fn settle_llm(
        &mut self,
        reservation: LlmBudgetReservation,
        usage: LlmCallUsage,
    ) -> Result<(), TurnExecutionError> {
        let reserved_input = reservation.reserved_input_tokens();
        let reserved_output = reservation.reserved_output_tokens();
        let actual_input = usage.input_tokens;
        let actual_output = usage.output_tokens;
        self.usage.input_tokens = self.usage.input_tokens.saturating_sub(reserved_input);
        self.usage.output_tokens = self.usage.output_tokens.saturating_sub(reserved_output);
        self.usage.total_tokens = self
            .usage
            .total_tokens
            .saturating_sub(reserved_input)
            .saturating_sub(reserved_output);
        let settled_input = self.usage.input_tokens.saturating_add(actual_input);
        let settled_output = self.usage.output_tokens.saturating_add(actual_output);
        let settled_total = self
            .usage
            .total_tokens
            .saturating_add(actual_input)
            .saturating_add(actual_output);
        if settled_input > self.limits.max_input_tokens {
            return Err(self.budget_error("input token limit exceeded".into()));
        }
        if settled_output > self.limits.max_output_tokens {
            return Err(self.budget_error("output token limit exceeded".into()));
        }
        if settled_total > self.limits.max_total_tokens {
            return Err(self.budget_error("total token limit exceeded".into()));
        }
        self.usage.llm_calls += 1;
        self.usage.input_tokens = settled_input;
        self.usage.output_tokens = settled_output;
        self.usage.total_tokens = settled_total;
        Ok(())
    }

    pub fn release_llm(&mut self, reservation: LlmBudgetReservation) {
        let reserved_input = reservation.reserved_input_tokens();
        let reserved_output = reservation.reserved_output_tokens();
        self.usage.input_tokens = self.usage.input_tokens.saturating_sub(reserved_input);
        self.usage.output_tokens = self.usage.output_tokens.saturating_sub(reserved_output);
        self.usage.total_tokens = self
            .usage
            .total_tokens
            .saturating_sub(reserved_input)
            .saturating_sub(reserved_output);
    }

    fn budget_error(&self, message: String) -> TurnExecutionError {
        TurnExecutionError::new(TurnFailureKind::TokenBudgetExceeded, "token_budget_exceeded", None, message)
    }
}
